use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::sessions::limit::{Limit, limit_in};
use crate::sessions::{SessionFile, Status, parse_session_file, transcript};

const POLL: Duration = Duration::from_millis(500);

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

#[derive(Debug, Default, PartialEq, Eq)]
struct Observed {
    session: Option<String>,
    state: Option<TabState>,
}

/// What one tab's session file last said, so only changes are reported.
#[derive(Default)]
struct Tracker {
    session: Option<String>,
    state: Option<TabState>,
}

impl Tracker {
    /// `limit` reads the transcript, so it runs only when a turn ends or a shown limit may be over.
    fn observe(
        &mut self,
        file: &SessionFile,
        now: u64,
        limit: impl FnOnce(&str) -> Option<Limit>,
    ) -> Observed {
        let mut observed = Observed::default();
        let new_chat = file.session_id.is_some() && file.session_id != self.session;
        if new_chat {
            self.session.clone_from(&file.session_id);
            observed.session.clone_from(&file.session_id);
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
        if state != self.state {
            self.state = state;
            observed.state = state;
        }
        observed
    }
}

pub struct Watched {
    /// The account's config dir, which holds `sessions/` and `projects/`.
    pub config_dir: PathBuf,
    pub pid: u32,
    /// Where the tab started, for a session file that does not say.
    pub cwd: String,
}

/// Follows a `claude` process's session file until `stop` fires or its sender is dropped.
pub fn watch(
    watched: Watched,
    stop: Receiver<()>,
    mut on_session: impl FnMut(&str) + Send + 'static,
    mut on_state: impl FnMut(TabState) + Send + 'static,
) -> io::Result<()> {
    let file = watched
        .config_dir
        .join("sessions")
        .join(format!("{}.json", watched.pid));
    thread::Builder::new().spawn(move || {
        let mut tracker = Tracker::default();
        while let Err(RecvTimeoutError::Timeout) = stop.recv_timeout(POLL) {
            // The file appears a moment after the process starts, so a missing one is normal.
            let Some(session) = fs::read(&file).ok().and_then(|b| parse_session_file(&b)) else {
                continue;
            };
            let now = unix_now();
            let cwd = session.cwd.clone().unwrap_or_else(|| watched.cwd.clone());
            let observed = tracker.observe(&session, now, |id| {
                let path = transcript(&watched.config_dir, &cwd, id)?;
                limit_in(&path, now).ok().flatten()
            });
            if let Some(id) = &observed.session {
                on_session(id);
            }
            if let Some(state) = observed.state {
                on_state(state);
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

    use super::*;

    const FIRST: &str = "0f6c3a52-7d1e-4b8a-9c2f-5e4d3b2a1f00";
    const SECOND: &str = "a1b2c3d4-0000-4000-8000-000000000001";
    const NOW: u64 = 1_800_000_000;

    fn file(session: &str, status: Status) -> SessionFile {
        SessionFile {
            session_id: Some(session.to_owned()),
            cwd: Some(r"C:\work\app".to_owned()),
            status,
        }
    }

    fn no_limit(_: &str) -> Option<Limit> {
        None
    }

    #[test]
    fn the_first_look_reports_the_chat_and_its_state() {
        let mut tracker = Tracker::default();

        let observed = tracker.observe(&file(FIRST, Status::Busy), NOW, no_limit);

        assert_eq!(
            observed,
            Observed {
                session: Some(FIRST.to_owned()),
                state: Some(TabState::Working),
            }
        );
    }

    #[test]
    fn nothing_new_reports_nothing() {
        let mut tracker = Tracker::default();
        tracker.observe(&file(FIRST, Status::Busy), NOW, no_limit);

        let observed = tracker.observe(&file(FIRST, Status::Busy), NOW, no_limit);

        assert_eq!(observed, Observed::default());
    }

    #[test]
    fn a_cleared_chat_reports_its_new_id() {
        let mut tracker = Tracker::default();
        tracker.observe(&file(FIRST, Status::Idle), NOW, no_limit);

        let observed = tracker.observe(&file(SECOND, Status::Idle), NOW, no_limit);

        assert_eq!(observed.session.as_deref(), Some(SECOND));
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

        let observed = tracker.observe(&file(FIRST, Status::Idle), NOW, |id| {
            assert_eq!(id, FIRST);
            Some(Limit {
                resets_at: Some(NOW + 60),
            })
        });

        assert_eq!(
            observed.state,
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

        assert_eq!(before.state, None);
        assert_eq!(after.state, Some(TabState::Idle));
    }

    #[test]
    fn an_unknown_status_keeps_the_last_state() {
        let mut tracker = Tracker::default();
        tracker.observe(&file(FIRST, Status::Busy), NOW, no_limit);

        let observed = tracker.observe(&file(FIRST, Status::Unknown), NOW, no_limit);

        assert_eq!(observed.state, None);
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
}
