use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::sessions::limit::{Limit, limit_in};
use crate::sessions::procs::descendants;
use crate::sessions::{SessionFile, Status, parse_session_file, transcript};

const POLL: Duration = Duration::from_millis(500);
const QUICK_SEARCH: Duration = Duration::from_secs(10);
const SLOW_SEARCH: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum TabState {
    Working,
    Idle,
    #[serde(rename_all = "camelCase")]
    Limited {
        resets_at: Option<u64>,
    },
}

/// What one tab's session file last said, so only changes are reported.
#[derive(Default)]
struct Tracker {
    session: Option<String>,
    saved: Option<String>,
    state: Option<TabState>,
}

impl Tracker {
    /// `limit` reads the transcript, so it runs only when a turn ends or a shown limit may be over.
    fn observe(
        &mut self,
        file: &SessionFile,
        now: u64,
        limit: impl FnOnce(&str) -> Option<Limit>,
    ) -> Option<TabState> {
        let new_chat = file.session_id.is_some() && file.session_id != self.session;
        if new_chat {
            self.session.clone_from(&file.session_id);
        }
        let state = match file.status {
            Status::Busy => Some(TabState::Working),
            Status::Idle => {
                let limit_over = matches!(
                    self.state,
                    Some(TabState::Limited { resets_at: Some(at) }) if at <= now
                );
                let turn_ended =
                    !matches!(self.state, Some(TabState::Idle | TabState::Limited { .. }));
                if new_chat || turn_ended || limit_over {
                    let found = self.session.as_deref().and_then(limit);
                    Some(found.map_or(TabState::Idle, |l| TabState::Limited {
                        resets_at: l.resets_at,
                    }))
                } else {
                    self.state
                }
            }
            Status::Unknown => self.state,
        };
        if state == self.state {
            return None;
        }
        self.state = state;
        state
    }

    /// The chat to save, once: Claude Code writes its transcript on the first message, and an empty chat cannot be resumed.
    fn unsaved(&mut self, has_transcript: impl FnOnce(&str) -> bool) -> Option<String> {
        let current = self.session.as_deref()?;
        if self.saved.as_deref() == Some(current) || !has_transcript(current) {
            return None;
        }
        self.saved = Some(current.to_owned());
        self.saved.clone()
    }
}

pub struct Watched {
    /// The account's config dir, which holds `sessions/` and `projects/`.
    pub config_dir: PathBuf,
    /// The tab's process; a shim such as `claude.cmd` runs the real `claude` below it.
    pub pid: u32,
    /// Where the tab started, for a session file that does not say.
    pub cwd: String,
    /// When the tab's process started, in milliseconds since 1970.
    pub started_ms: u64,
}

/// A file from before the tab started belongs to a dead process that had the same pid.
fn is_current(file: &SessionFile, modified_ms: Option<u64>, started_ms: u64) -> bool {
    file.started_at
        .or(modified_ms)
        .is_some_and(|at| at >= started_ms)
}

/// How long to wait before another search for a file that was not there: quick at first, then slower.
fn search_gap(age: Duration) -> Duration {
    if age < QUICK_SEARCH {
        POLL
    } else {
        SLOW_SEARCH
    }
}

/// Finds the `claude` process's session file and the chat's transcript, each searched for only as often as `search_gap` allows.
struct Finder {
    watched: Watched,
    born: Instant,
    pid: Option<u32>,
    next_pid_search: Instant,
    transcript: Option<(String, PathBuf)>,
    next_transcript_search: Instant,
}

impl Finder {
    fn new(watched: Watched) -> Self {
        let now = Instant::now();
        Self {
            watched,
            born: now,
            pid: None,
            next_pid_search: now,
            transcript: None,
            next_transcript_search: now,
        }
    }

    fn file_of(&self, pid: u32) -> PathBuf {
        self.watched
            .config_dir
            .join("sessions")
            .join(format!("{pid}.json"))
    }

    fn read(&self, pid: u32) -> Option<SessionFile> {
        let path = self.file_of(pid);
        let file = parse_session_file(&fs::read(&path).ok()?)?;
        let modified_ms = fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .and_then(|d| u64::try_from(d.as_millis()).ok());
        is_current(&file, modified_ms, self.watched.started_ms).then_some(file)
    }

    fn session(&mut self) -> Option<SessionFile> {
        if let Some(pid) = self.pid {
            return self.read(pid);
        }
        let now = Instant::now();
        if now < self.next_pid_search {
            return None;
        }
        self.next_pid_search = now + search_gap(now - self.born);
        let root = self.watched.pid;
        let (pid, file) = std::iter::once(root)
            .chain(descendants(root))
            .find_map(|pid| Some((pid, self.read(pid)?)))?;
        self.pid = Some(pid);
        Some(file)
    }

    fn transcript(&mut self, cwd: &str, id: &str) -> Option<PathBuf> {
        if let Some((known, path)) = &self.transcript
            && known == id
        {
            return Some(path.clone());
        }
        let now = Instant::now();
        if now < self.next_transcript_search {
            return None;
        }
        self.next_transcript_search = now + search_gap(now - self.born);
        let path = transcript(&self.watched.config_dir, cwd, id)?;
        self.transcript = Some((id.to_owned(), path.clone()));
        Some(path)
    }
}

/// Follows a `claude` process's session file until `stop` fires or its sender is dropped.
pub fn watch(
    watched: Watched,
    stop: Receiver<()>,
    mut on_session: impl FnMut(&str) + Send + 'static,
    mut on_state: impl FnMut(TabState) + Send + 'static,
) -> io::Result<()> {
    thread::Builder::new().spawn(move || {
        let fallback_cwd = watched.cwd.clone();
        let mut finder = Finder::new(watched);
        let mut tracker = Tracker::default();
        while let Err(RecvTimeoutError::Timeout) = stop.recv_timeout(POLL) {
            // The file appears a moment after the process starts, so a missing one is normal.
            let Some(session) = finder.session() else {
                continue;
            };
            let now = unix_now();
            let cwd = session.cwd.clone().unwrap_or_else(|| fallback_cwd.clone());
            let changed = tracker.observe(&session, now, |id| {
                limit_in(&finder.transcript(&cwd, id)?, now).ok().flatten()
            });
            if let Some(state) = changed {
                on_state(state);
            }
            if let Some(id) = tracker.unsaved(|id| finder.transcript(&cwd, id).is_some()) {
                on_session(&id);
            }
        }
    })?;
    Ok(())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::accounts::testdir::TestDir;

    const FIRST: &str = "0f6c3a52-7d1e-4b8a-9c2f-5e4d3b2a1f00";
    const SECOND: &str = "a1b2c3d4-0000-4000-8000-000000000001";
    const NOW: u64 = 1_800_000_000;

    fn file(session: &str, status: Status) -> SessionFile {
        SessionFile {
            session_id: Some(session.to_owned()),
            cwd: Some(r"C:\work\app".to_owned()),
            status,
            started_at: None,
        }
    }

    fn no_limit(_: &str) -> Option<Limit> {
        None
    }

    #[test]
    fn the_first_look_reports_the_state() {
        let mut tracker = Tracker::default();

        let changed = tracker.observe(&file(FIRST, Status::Busy), NOW, no_limit);

        assert_eq!(changed, Some(TabState::Working));
    }

    #[test]
    fn nothing_new_reports_nothing() {
        let mut tracker = Tracker::default();
        tracker.observe(&file(FIRST, Status::Busy), NOW, no_limit);

        assert_eq!(
            tracker.observe(&file(FIRST, Status::Busy), NOW, no_limit),
            None
        );
        assert_eq!(tracker.unsaved(|_| true).as_deref(), Some(FIRST));
        assert_eq!(tracker.unsaved(|_| true), None);
    }

    #[test]
    fn a_chat_is_saved_only_once_it_has_a_transcript() {
        let mut tracker = Tracker::default();
        tracker.observe(&file(FIRST, Status::Idle), NOW, no_limit);

        assert_eq!(tracker.unsaved(|_| false), None);
        assert_eq!(tracker.unsaved(|_| true).as_deref(), Some(FIRST));
    }

    #[test]
    fn a_cleared_chat_is_saved_under_its_new_id() {
        let mut tracker = Tracker::default();
        tracker.observe(&file(FIRST, Status::Idle), NOW, no_limit);
        tracker.unsaved(|_| true);

        tracker.observe(&file(SECOND, Status::Idle), NOW, no_limit);

        assert_eq!(tracker.unsaved(|id| id == SECOND).as_deref(), Some(SECOND));
    }

    #[test]
    fn the_transcript_is_read_only_when_a_turn_ends() {
        let mut tracker = Tracker::default();
        let reads = Cell::new(0);
        let counting = |_: &str| {
            reads.set(reads.get() + 1);
            None
        };

        tracker.observe(&file(FIRST, Status::Busy), NOW, counting);
        tracker.observe(&file(FIRST, Status::Idle), NOW, counting);
        tracker.observe(&file(FIRST, Status::Idle), NOW, counting);
        tracker.observe(&file(FIRST, Status::Idle), NOW, counting);

        assert_eq!(reads.get(), 1);
    }

    #[test]
    fn a_turn_that_hit_the_limit_shows_limited_with_its_reset() {
        let mut tracker = Tracker::default();
        tracker.observe(&file(FIRST, Status::Busy), NOW, no_limit);

        let changed = tracker.observe(&file(FIRST, Status::Idle), NOW, |id| {
            assert_eq!(id, FIRST);
            Some(Limit {
                resets_at: Some(NOW + 60),
            })
        });

        assert_eq!(
            changed,
            Some(TabState::Limited {
                resets_at: Some(NOW + 60)
            })
        );
    }

    #[test]
    fn a_limit_ends_once_its_reset_time_passes() {
        let mut tracker = Tracker::default();
        let limited = |_: &str| {
            Some(Limit {
                resets_at: Some(NOW + 60),
            })
        };
        tracker.observe(&file(FIRST, Status::Idle), NOW, limited);

        let before = tracker.observe(&file(FIRST, Status::Idle), NOW + 59, limited);
        let after = tracker.observe(&file(FIRST, Status::Idle), NOW + 60, no_limit);

        assert_eq!(before, None);
        assert_eq!(after, Some(TabState::Idle));
    }

    #[test]
    fn an_unknown_status_keeps_the_last_state() {
        let mut tracker = Tracker::default();
        tracker.observe(&file(FIRST, Status::Busy), NOW, no_limit);

        let changed = tracker.observe(&file(FIRST, Status::Unknown), NOW, no_limit);

        assert_eq!(changed, None);
    }

    #[test]
    fn states_reach_the_webview_as_tagged_objects() {
        let limited = TabState::Limited {
            resets_at: Some(NOW),
        };

        assert_eq!(
            serde_json::to_string(&limited).unwrap(),
            r#"{"state":"limited","resetsAt":1800000000}"#
        );
        assert_eq!(
            serde_json::to_string(&TabState::Working).unwrap(),
            r#"{"state":"working"}"#
        );
    }

    #[test]
    fn a_session_file_from_before_the_tab_started_is_ignored() {
        let mut stale = file(FIRST, Status::Busy);
        stale.started_at = Some(1_000);
        let mut fresh = file(FIRST, Status::Busy);
        fresh.started_at = Some(2_000);
        let unknown = file(FIRST, Status::Busy);

        assert!(!is_current(&stale, Some(9_000), 2_000));
        assert!(is_current(&fresh, None, 2_000));
        assert!(is_current(&unknown, Some(2_500), 2_000));
        assert!(!is_current(&unknown, Some(1_500), 2_000));
        assert!(!is_current(&unknown, None, 2_000));
    }

    #[test]
    fn searches_slow_down_after_the_first_seconds() {
        assert_eq!(search_gap(Duration::from_secs(1)), POLL);
        assert_eq!(
            search_gap(QUICK_SEARCH + Duration::from_secs(1)),
            SLOW_SEARCH
        );
    }

    fn watched_in(dir: &TestDir, pid: u32, started_ms: u64) -> Watched {
        Watched {
            config_dir: dir.path().to_path_buf(),
            pid,
            cwd: r"C:\work\app".to_owned(),
            started_ms,
        }
    }

    fn write_transcript(dir: &TestDir, records: &str) -> PathBuf {
        let folder = dir.path().join("projects").join("C--work-app");
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join(format!("{FIRST}.jsonl"));
        fs::write(&path, records).unwrap();
        path
    }

    #[test]
    fn a_found_transcript_is_kept_and_a_missing_one_waits_for_the_gap() {
        let dir = TestDir::new();
        let mut finder = Finder::new(watched_in(&dir, std::process::id(), 0));

        assert_eq!(finder.transcript(r"C:\work\app", FIRST), None);
        let path = write_transcript(&dir, "{}\n");
        assert_eq!(
            finder.transcript(r"C:\work\app", FIRST),
            None,
            "searched before the gap"
        );
        finder.next_transcript_search = Instant::now();
        assert_eq!(finder.transcript(r"C:\work\app", FIRST), Some(path.clone()));
        fs::remove_dir_all(dir.path().join("projects")).unwrap();

        assert_eq!(
            finder.transcript(r"C:\work\app", FIRST),
            Some(path),
            "searched again"
        );
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0))
    }

    fn write_session(dir: &TestDir, pid: u32, status: &str) {
        let sessions = dir.path().join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        let file = format!(
            r#"{{"pid":{pid},"sessionId":"{FIRST}","cwd":"C:\\work\\app","status":"{status}","name":"fake chat name","startedAt":{}}}"#,
            now_ms()
        );
        fs::write(sessions.join(format!("{pid}.json")), file).unwrap();
    }

    /// Everything the watcher hands out, as text: saved chat ids and the state messages sent to the page.
    fn run(watched: Watched, done: impl Fn(&[String]) -> bool) -> Vec<String> {
        let events = Arc::new(Mutex::new(Vec::new()));
        let (sessions, states) = (Arc::clone(&events), Arc::clone(&events));
        let (stop, stopped) = mpsc::channel();
        watch(
            watched,
            stopped,
            move |id| sessions.lock().unwrap().push(format!("session {id}")),
            move |state| {
                let json = serde_json::to_string(&state).unwrap();
                states.lock().unwrap().push(format!("state {json}"));
            },
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(&events.lock().unwrap()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        drop(stop);
        events.lock().unwrap().clone()
    }

    #[test]
    fn nothing_but_the_chat_id_and_reset_time_leaves_the_watcher() {
        let dir = TestDir::new();
        let pid = std::process::id();
        let resets = unix_now() + 3_600;
        let secret = "sk-fake-0000-not-a-real-key";
        write_transcript(
            &dir,
            &format!(
                "{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"my key is {secret}\"}}}}\n\
                 {{\"type\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"text\",\"text\":\"You've hit your session limit {secret}\"}}]}},\
                 \"quotaLimits\":{{\"status\":\"rejected\",\"resetsAt\":{resets},\"rateLimitType\":\"five_hour\"}},\
                 \"error\":\"rate_limit\",\"isApiErrorMessage\":true}}\n"
            ),
        );
        write_session(&dir, pid, "idle");

        let events = run(watched_in(&dir, pid, now_ms() - 1_000), |e| e.len() >= 2);

        assert_eq!(
            events,
            [
                format!(r#"state {{"state":"limited","resetsAt":{resets}}}"#),
                format!("session {FIRST}"),
            ]
        );
        for text in [
            secret,
            "hit your",
            "fake chat name",
            "five_hour",
            "rejected",
        ] {
            assert!(
                !events.iter().any(|e| e.contains(text)),
                "{text} left the watcher"
            );
        }
    }

    #[test]
    fn a_stale_session_file_from_a_reused_pid_is_not_followed() {
        let dir = TestDir::new();
        let pid = std::process::id();
        write_session(&dir, pid, "busy");

        let until = Instant::now() + Duration::from_secs(2);
        let events = run(watched_in(&dir, pid, now_ms() + 60_000), |_| {
            Instant::now() > until
        });

        assert_eq!(events, Vec::<String>::new());
    }

    #[cfg(windows)]
    #[test]
    fn a_session_below_a_cmd_shim_is_found() {
        let dir = TestDir::new();
        let mut shim = std::process::Command::new("cmd")
            .args(["/c", "ping -n 30 127.0.0.1 >nul"])
            .spawn()
            .unwrap();
        let started_ms = now_ms() - 1_000;
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut below = Vec::new();
        while below.is_empty() && Instant::now() < deadline {
            below = descendants(shim.id());
        }
        write_session(&dir, below[0], "busy");

        let events = run(watched_in(&dir, shim.id(), started_ms), |e| !e.is_empty());
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &shim.id().to_string()])
            .output();
        let _ = shim.wait();

        assert_eq!(events, [r#"state {"state":"working"}"#]);
    }
}
