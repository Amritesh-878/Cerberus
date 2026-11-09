//! Starting, stopping and restarting a service's runs, and passing their output on.

use std::sync::{Arc, MutexGuard, OnceLock, PoisonError, Weak};
use std::thread;
use std::time::{Duration, Instant};

use crate::pty::{SessionId, SpawnRequest};
use crate::services::backoff::Next;
use crate::services::store::{Service, ServiceId};
use crate::services::tree::{Tree, shell_args};
use crate::services::url::{Finder, Rule};
use crate::services::{Inner, Phase, ServiceView, State, View, Watcher};

const CURSOR_QUERY: &[u8] = b"\x1b[6n";
const CURSOR_ANSWER: &[u8] = b"\x1b[1;1R";
const COLS: u16 = 120;
const ROWS: u16 = 40;
// Dim, so a viewer can tell runs apart in the replayed output.
const STARTED: &[u8] = b"\x1b[2m--- started ---\x1b[0m\r\n";

impl Inner {
    pub(super) fn state(&self) -> MutexGuard<'_, State> {
        // Every change leaves the state whole, so a panic elsewhere does not spoil it.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn view_of(&self, state: &State) -> View {
        View {
            folded: state.saved.folded,
            services: state
                .saved
                .services
                .iter()
                .map(|s| {
                    let run = state.runs.get(&s.id);
                    ServiceView {
                        id: s.id,
                        name: s.name.clone(),
                        phase: run.map_or(Phase::Stopped, |r| r.phase),
                        url: run.and_then(|r| r.url.clone()),
                        folder: s.folder.clone(),
                        shell: s.shell.clone(),
                        start_with_app: s.start_with_app,
                        restart: s.restart,
                        problem: run.and_then(|r| r.problem.clone()),
                    }
                })
                .collect(),
        }
    }

    pub(super) fn view(&self) -> View {
        let state = self.state();
        self.view_of(&state)
    }

    pub(super) fn changed(&self) -> View {
        let view = self.view();
        self.sink.changed(view.clone());
        view
    }

    pub(super) fn stop(&self, id: ServiceId) {
        let (session, tree) = {
            let mut state = self.state();
            let Some(run) = state.runs.get_mut(&id) else {
                return;
            };
            run.generation += 1;
            run.phase = Phase::Stopped;
            run.url = None;
            (run.session.take(), run.tree.take())
        };
        if let Some(tree) = tree {
            tree.end();
        }
        if let Some(session) = session {
            self.pty.kill(session);
        }
    }

    pub(super) fn launch(inner: &Arc<Inner>, id: ServiceId) {
        let Some((service, generation)) = inner.begin(id) else {
            return;
        };
        let request = match inner.request(&service) {
            Ok(request) => request,
            Err(problem) => {
                inner.failed(id, generation, problem);
                return;
            }
        };
        // Output credit goes back at once: nothing waits on a webview to draw it.
        let session_of: Arc<OnceLock<SessionId>> = Arc::default();
        let (out, ended) = (Arc::downgrade(inner), Arc::downgrade(inner));
        let acked = Arc::clone(&session_of);
        let spawned = inner.pty.spawn(
            &request,
            move |bytes| {
                let Some(inner) = out.upgrade() else {
                    return;
                };
                let asks_cursor = contains(&bytes, CURSOR_QUERY);
                inner.output(id, generation, &bytes);
                let session = acked.get().copied().or_else(|| session_soon(&acked));
                if let Some(session) = session {
                    inner.pty.ack(session, bytes.len());
                    // No terminal draws a service, so this answers in its place, or ConPTY waits forever.
                    if asks_cursor {
                        let _ = inner.pty.write(session, CURSOR_ANSWER);
                    }
                }
            },
            move |_exit| {
                if let Some(inner) = ended.upgrade() {
                    Inner::exited(&inner, id, generation);
                }
            },
        );
        match spawned {
            Ok(session) => {
                let _ = session_of.set(session);
                inner.started(id, generation, session);
            }
            Err(e) => inner.failed(id, generation, e.to_string()),
        }
    }

    /// Marks a new run; `None` when the service is gone.
    pub(super) fn begin(&self, id: ServiceId) -> Option<(Service, u64)> {
        let mut state = self.state();
        let service = state.saved.services.iter().find(|s| s.id == id)?.clone();
        let run = state.runs.get_mut(&id)?;
        run.generation += 1;
        run.phase = Phase::Starting;
        run.url = None;
        run.problem = None;
        run.started = Instant::now();
        run.finder = Finder::new(Rule::for_command(service.command.text()));
        run.output.push(STARTED);
        let generation = run.generation;
        let watched: Vec<&Watcher> = state
            .watchers
            .values()
            .filter(|(service, _)| *service == id)
            .map(|(_, send)| send)
            .collect();
        for send in watched {
            send(STARTED);
        }
        Some((service, generation))
    }

    pub(super) fn request(&self, service: &Service) -> Result<SpawnRequest, String> {
        let shell = self
            .sink
            .shell(service.shell.as_deref())
            .ok_or("no shell is installed to run it in")?;
        let program = shell.program.clone().ok_or("its shell is not installed")?;
        let args = shell_args(&shell.id, &shell.args, service.command.text())
            .map_err(|e| e.to_string())?;
        let cwd = service.folder.clone().unwrap_or_else(|| self.home.clone());
        Ok(SpawnRequest {
            program,
            args,
            cwd: Some(cwd.to_string_lossy().into_owned()),
            cols: COLS,
            rows: ROWS,
            claude_config_dir: None,
            env: Vec::new(),
            dirs: Vec::new(),
            tab_env: Vec::new(),
        })
    }

    pub(super) fn started(&self, id: ServiceId, generation: u64, session: SessionId) {
        let pid = self.pty.pid(session);
        let mut state = self.state();
        let current = state.runs.get_mut(&id).filter(|r| {
            r.generation == generation && matches!(r.phase, Phase::Starting | Phase::Running)
        });
        let Some(run) = current else {
            // Stopped, or already exited, while it was starting.
            drop(state);
            self.pty.kill(session);
            return;
        };
        run.session = Some(session);
        // A tree that can't be adopted still has its shell ended by Stop; only its children could stay.
        run.tree = pid.and_then(|pid| match Tree::adopt(pid) {
            Ok(tree) => Some(tree),
            Err(e) => {
                eprintln!("services: could not hold service {id}'s processes together: {e}");
                None
            }
        });
    }

    pub(super) fn failed(&self, id: ServiceId, generation: u64, problem: String) {
        {
            let mut state = self.state();
            if let Some(run) = state
                .runs
                .get_mut(&id)
                .filter(|r| r.generation == generation)
            {
                run.phase = Phase::Stopped;
                run.problem = Some(problem);
            }
        }
        self.changed();
    }

    pub(super) fn output(&self, id: ServiceId, generation: u64, bytes: &[u8]) {
        let found = {
            let mut state = self.state();
            let State { runs, watchers, .. } = &mut *state;
            let Some(run) = runs.get_mut(&id).filter(|r| r.generation == generation) else {
                return;
            };
            run.output.push(bytes);
            for (service, send) in watchers.values() {
                if *service == id {
                    send(bytes);
                }
            }
            let url = run.finder.feed(bytes);
            let first_output = run.phase == Phase::Starting;
            if first_output {
                run.phase = Phase::Running;
            }
            if let Some(url) = &url {
                run.url = Some(url.clone());
            }
            first_output || url.is_some()
        };
        if found {
            self.changed();
        }
    }

    pub(super) fn exited(inner: &Arc<Inner>, id: ServiceId, generation: u64) {
        let next = {
            let mut state = inner.state();
            let restart = state
                .saved
                .services
                .iter()
                .find(|s| s.id == id)
                .is_some_and(|s| s.restart);
            let Some(run) = state
                .runs
                .get_mut(&id)
                .filter(|r| r.generation == generation)
            else {
                // Stop or a newer start caused this exit.
                return;
            };
            run.session = None;
            // Ends whatever the exited shell left behind.
            run.tree = None;
            run.url = None;
            let next = if restart {
                run.backoff.exited(Instant::now(), run.started.elapsed())
            } else {
                Next::Crashed
            };
            run.phase = match next {
                Next::RestartIn(_) => Phase::Restarting,
                Next::Crashed if restart => Phase::Crashed,
                Next::Crashed => Phase::Stopped,
            };
            next
        };
        inner.changed();
        if let Next::RestartIn(wait) = next {
            Self::restart_after(inner, id, generation, wait);
        }
    }

    pub(super) fn restart_after(
        inner: &Arc<Inner>,
        id: ServiceId,
        generation: u64,
        wait: Duration,
    ) {
        let weak: Weak<Inner> = Arc::downgrade(inner);
        let timer = thread::Builder::new().spawn(move || {
            thread::sleep(wait);
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let due = inner
                .state()
                .runs
                .get(&id)
                .is_some_and(|r| r.generation == generation && r.phase == Phase::Restarting);
            if due {
                Inner::launch(&inner, id);
                inner.changed();
            }
        });
        if timer.is_err() {
            inner.failed(id, generation, "could not wait to restart it".into());
        }
    }
}

fn contains(bytes: &[u8], part: &[u8]) -> bool {
    bytes.windows(part.len()).any(|w| w == part)
}

// Output can beat `spawn`'s return by a moment; the session id follows right after.
fn session_soon(session: &OnceLock<SessionId>) -> Option<SessionId> {
    for _ in 0..200 {
        if let Some(&id) = session.get() {
            return Some(id);
        }
        thread::sleep(Duration::from_millis(5));
    }
    None
}
