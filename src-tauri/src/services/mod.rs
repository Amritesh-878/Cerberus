//! Services: named background programs Cerberus runs in a hidden terminal, restarts and lists.

pub mod app;
mod backoff;
mod output;
mod run;
pub mod store;
#[cfg(test)]
mod tests;
mod tree;
mod url;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;

use crate::programs::ProgramChoice;
use crate::pty::{PtyManager, SessionId};
use crate::services::backoff::Backoff;
use crate::services::output::Output;
use crate::services::store::{Draft, Saved, ServiceId, Store, StoreError};
use crate::services::tree::Tree;
use crate::services::url::{Finder, Rule};

/// What services need from the rest of the app.
pub trait Sink: Send + Sync + 'static {
    fn changed(&self, view: View);
    /// The shell to run in: the service's own pick if installed, else the Shell setting's.
    fn shell(&self, wanted: Option<&str>) -> Option<ProgramChoice>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Starting,
    Running,
    Restarting,
    Crashed,
    Stopped,
}

/// One row, ready to draw; the command is never in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceView {
    pub id: ServiceId,
    pub name: String,
    pub phase: Phase,
    pub url: Option<String>,
    pub folder: Option<PathBuf>,
    pub shell: Option<String>,
    pub start_with_app: bool,
    pub restart: bool,
    /// Why the last start failed, without the command.
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub folded: bool,
    pub services: Vec<ServiceView>,
}

pub type WatchId = u64;
type Watcher = Box<dyn Fn(&[u8]) + Send + Sync>;

struct Run {
    phase: Phase,
    url: Option<String>,
    problem: Option<String>,
    /// Raised by every start and stop, so a late exit or timer of an older run does nothing.
    generation: u64,
    session: Option<SessionId>,
    tree: Option<Tree>,
    output: Output,
    finder: Finder,
    backoff: Backoff,
    started: Instant,
}

impl Run {
    fn new() -> Self {
        Self {
            phase: Phase::Stopped,
            url: None,
            problem: None,
            generation: 0,
            session: None,
            tree: None,
            output: Output::default(),
            finder: Finder::new(Rule::Local),
            backoff: Backoff::default(),
            started: Instant::now(),
        }
    }
}

#[derive(Default)]
struct State {
    saved: Saved,
    runs: HashMap<ServiceId, Run>,
    watchers: HashMap<WatchId, (ServiceId, Watcher)>,
    next_watch: WatchId,
}

struct Inner {
    store: Store,
    pty: PtyManager,
    home: PathBuf,
    sink: Box<dyn Sink>,
    state: Mutex<State>,
}

#[derive(Clone)]
pub struct Services {
    inner: Arc<Inner>,
}

impl Services {
    /// Reads the saved list from `dir`; nothing starts until `start_saved`.
    pub fn new(dir: &std::path::Path, home: PathBuf, sink: impl Sink) -> Self {
        let store = Store::new(dir);
        let saved = store.load();
        let runs = saved.services.iter().map(|s| (s.id, Run::new())).collect();
        Self {
            inner: Arc::new(Inner {
                store,
                pty: PtyManager::default(),
                home,
                sink: Box::new(sink),
                state: Mutex::new(State {
                    saved,
                    runs,
                    ..State::default()
                }),
            }),
        }
    }

    /// Starts the services marked "Start with Cerberus".
    pub fn start_saved(&self) {
        let ids: Vec<ServiceId> = self
            .inner
            .state()
            .saved
            .services
            .iter()
            .filter(|s| s.start_with_app)
            .map(|s| s.id)
            .collect();
        for id in ids {
            let _ = self.start(id);
        }
    }

    pub fn view(&self) -> View {
        self.inner.view()
    }

    pub fn add(&self, draft: &Draft) -> Result<View, StoreError> {
        {
            let mut state = self.inner.state();
            let service = state.saved.checked(draft, None)?;
            let id = service.id;
            let mut saved = state.saved.clone();
            saved.services.push(service);
            saved.next_id = id + 1;
            // Saved first, so a failed write leaves nothing in memory that a restart would drop.
            self.inner.store.save(&saved)?;
            state.saved = saved;
            state.runs.insert(id, Run::new());
        }
        Ok(self.inner.changed())
    }

    /// A running service restarts with its new settings.
    pub fn update(&self, id: ServiceId, draft: &Draft) -> Result<View, StoreError> {
        let was_on = {
            let mut state = self.inner.state();
            let service = state.saved.checked(draft, Some(id))?;
            let at = state
                .saved
                .services
                .iter()
                .position(|s| s.id == id)
                .ok_or(StoreError::Gone)?;
            let mut saved = state.saved.clone();
            saved.services[at] = service;
            // A failed write keeps the old command, so no restart runs one that isn't saved.
            self.inner.store.save(&saved)?;
            state.saved = saved;
            state
                .runs
                .get(&id)
                .is_some_and(|r| r.phase != Phase::Stopped && r.phase != Phase::Crashed)
        };
        if was_on {
            self.restart(id)?;
        }
        Ok(self.inner.changed())
    }

    /// Saved first: a failed write leaves the service listed and running as it was.
    pub fn remove(&self, id: ServiceId) -> Result<View, StoreError> {
        {
            let mut state = self.inner.state();
            let mut saved = state.saved.clone();
            saved.services.retain(|s| s.id != id);
            if saved.services.len() == state.saved.services.len() {
                return Err(StoreError::Gone);
            }
            self.inner.store.save(&saved)?;
            state.saved = saved;
        }
        // Gone from the saved list, so no pending restart can start it again.
        self.inner.stop(id);
        {
            let mut state = self.inner.state();
            state.runs.remove(&id);
            state.watchers.retain(|_, (service, _)| *service != id);
        }
        Ok(self.inner.changed())
    }

    pub fn start(&self, id: ServiceId) -> Result<View, StoreError> {
        {
            let mut state = self.inner.state();
            let run = state.runs.get_mut(&id).ok_or(StoreError::Gone)?;
            if matches!(run.phase, Phase::Starting | Phase::Running) {
                return Ok(self.inner.view_of(&state));
            }
            run.backoff.reset();
        }
        Inner::launch(&self.inner, id);
        Ok(self.inner.changed())
    }

    pub fn stop(&self, id: ServiceId) -> Result<View, StoreError> {
        if !self.inner.state().runs.contains_key(&id) {
            return Err(StoreError::Gone);
        }
        self.inner.stop(id);
        Ok(self.inner.changed())
    }

    pub fn restart(&self, id: ServiceId) -> Result<View, StoreError> {
        self.stop(id)?;
        self.start(id)
    }

    /// For the edit form only.
    pub fn command(&self, id: ServiceId) -> Result<String, StoreError> {
        let state = self.inner.state();
        let service = state.saved.services.iter().find(|s| s.id == id);
        service
            .map(|s| s.command.text().to_owned())
            .ok_or(StoreError::Gone)
    }

    pub fn url(&self, id: ServiceId) -> Option<String> {
        self.inner.state().runs.get(&id)?.url.clone()
    }

    /// The output so far, then every new chunk to `send` until `unwatch`.
    pub fn watch(
        &self,
        id: ServiceId,
        send: impl Fn(&[u8]) + Send + Sync + 'static,
    ) -> Option<WatchId> {
        let mut state = self.inner.state();
        // Under the lock, so no new chunk can reach `send` ahead of the replay.
        send(&state.runs.get(&id)?.output.replay());
        let watch = state.next_watch;
        state.next_watch += 1;
        state.watchers.insert(watch, (id, Box::new(send)));
        Some(watch)
    }

    pub fn unwatch(&self, watch: WatchId) {
        self.inner.state().watchers.remove(&watch);
    }

    pub fn fold(&self, folded: bool) -> Result<View, StoreError> {
        {
            let mut state = self.inner.state();
            let mut saved = state.saved.clone();
            saved.folded = folded;
            self.inner.store.save(&saved)?;
            state.saved = saved;
        }
        Ok(self.inner.changed())
    }

    /// The app is closing: every service's tree ends now.
    pub fn stop_all(&self) {
        let ids: Vec<ServiceId> = self.inner.state().runs.keys().copied().collect();
        for id in ids {
            self.inner.stop(id);
        }
        self.inner.pty.kill_all();
    }
}
