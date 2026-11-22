use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use crate::sessions::watch::TabState;
use crate::sessions::{Resume, is_session_id};

const POLL: Duration = Duration::from_millis(500);
const DB_BUSY_TIMEOUT: Duration = Duration::from_millis(100);
const SUMMARY_SCAN_LIMIT: i64 = 25;

pub fn is_agy(program: &str) -> bool {
    Path::new(program)
        .file_stem()
        .is_some_and(|stem| stem.eq_ignore_ascii_case("agy"))
}

pub fn agy_home(user_home: &Path) -> PathBuf {
    user_home.join(".gemini").join("antigravity-cli")
}

/// The conversation's compact transcript, or `None` when it has no recorded steps yet.
pub fn agy_transcript(home: &Path, id: &str) -> Option<PathBuf> {
    if !is_session_id(id) {
        return None;
    }
    Some(
        home.join("brain")
            .join(id)
            .join(".system_generated")
            .join("logs")
            .join("transcript.jsonl"),
    )
    .filter(|p| p.is_file())
}

pub fn agy_resume_args(args: &[String], id: &str) -> Vec<String> {
    let mut resumed = args.to_vec();
    resumed.extend(["--conversation".to_owned(), id.to_owned()]);
    resumed
}

pub fn plan_agy_resume(
    args: &[String],
    home: &Path,
    saved: Option<&str>,
    ended: bool,
) -> (Vec<String>, Resume) {
    if ended {
        return (args.to_vec(), Resume::Ended);
    }
    match saved {
        None => (args.to_vec(), Resume::New),
        Some(id) if agy_transcript(home, id).is_some() => {
            (agy_resume_args(args, id), Resume::Resuming)
        }
        Some(_) => (args.to_vec(), Resume::Missing),
    }
}

pub struct WatchedAgy {
    pub agy_home: PathBuf,
    pub cwd: String,
    /// When the tab's process started, in milliseconds since 1970.
    pub started_ms: u64,
    pub initial_session: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SummaryRow {
    conversation_id: String,
    last_modified_ms: u64,
    workspace_uris: Vec<String>,
    running: bool,
}

#[derive(Default)]
struct AgyTracker {
    session: Option<String>,
    saved: Option<String>,
    state: Option<TabState>,
}

impl AgyTracker {
    fn new(initial_session: Option<String>) -> Self {
        Self {
            session: initial_session.clone(),
            saved: initial_session,
            state: None,
        }
    }

    fn observe(
        &mut self,
        row: Option<&SummaryRow>,
        has_transcript: impl FnOnce(&str) -> bool,
    ) -> (Option<TabState>, Option<String>) {
        let mut changed = None;
        if let Some(row) = row {
            if is_session_id(&row.conversation_id)
                && self.session.as_deref() != Some(row.conversation_id.as_str())
            {
                self.session = Some(row.conversation_id.clone());
            }
            let next = if row.running {
                TabState::Working
            } else {
                TabState::Idle
            };
            if self.state != Some(next) {
                self.state = Some(next);
                changed = Some(next);
            }
        }

        let unsaved = self.session.as_deref().and_then(|id| {
            if self.saved.as_deref() == Some(id) || !has_transcript(id) {
                return None;
            }
            self.saved = Some(id.to_owned());
            self.saved.clone()
        });

        (changed, unsaved)
    }
}

/// Follows an `agy` tab's conversation state in `conversation_summaries.db` until `stop` fires.
pub fn watch_agy(
    watched: WatchedAgy,
    stop: Receiver<()>,
    mut on_session: impl FnMut(&str) + Send + 'static,
    mut on_state: impl FnMut(TabState) + Send + 'static,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new().spawn(move || {
        let mut tracker = AgyTracker::new(watched.initial_session.clone());
        let db_path = watched.agy_home.join("conversation_summaries.db");

        while let Err(RecvTimeoutError::Timeout) = stop.recv_timeout(POLL) {
            let row = poll_summary(
                &db_path,
                &watched.cwd,
                watched.started_ms,
                tracker.session.as_deref(),
            );
            let (changed, unsaved) = tracker.observe(row.as_ref(), |id| {
                agy_transcript(&watched.agy_home, id).is_some()
            });
            if let Some(state) = changed {
                on_state(state);
            }
            if let Some(id) = unsaved {
                on_session(&id);
            }
        }

        // One final read so a fast single-turn chat is still saved on exit.
        let row = poll_summary(
            &db_path,
            &watched.cwd,
            watched.started_ms,
            tracker.session.as_deref(),
        );
        let (_, unsaved) = tracker.observe(row.as_ref(), |id| {
            agy_transcript(&watched.agy_home, id).is_some()
        });
        if let Some(id) = unsaved {
            on_session(&id);
        }
    })
}

fn poll_summary(
    db_path: &Path,
    cwd: &str,
    started_ms: u64,
    known_id: Option<&str>,
) -> Option<SummaryRow> {
    if !db_path.is_file() {
        return None;
    }
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let _ = conn.busy_timeout(DB_BUSY_TIMEOUT);
    let mut stmt = conn
        .prepare(
            "SELECT conversation_id, last_modified_time, workspace_uris, status, not_fully_idle \
             FROM conversation_summaries \
             ORDER BY last_modified_time DESC \
             LIMIT ?1",
        )
        .ok()?;
    let rows = stmt
        .query_map([SUMMARY_SCAN_LIMIT], |row| {
            let conversation_id: String = row.get(0)?;
            let modified_str: String = row.get(1)?;
            let uris_json: String = row.get(2)?;
            let status: String = row.get(3)?;
            let not_fully_idle: i64 = row.get(4)?;
            Ok((
                conversation_id,
                modified_str,
                uris_json,
                status,
                not_fully_idle,
            ))
        })
        .ok()?;

    let mut candidates = Vec::new();
    for entry in rows.flatten() {
        let (conversation_id, modified_str, uris_json, status, not_fully_idle) = entry;
        let Some(last_modified_ms) = parse_timestamp_ms(&modified_str) else {
            continue;
        };
        let workspace_uris: Vec<String> = serde_json::from_str(&uris_json).unwrap_or_default();
        if !workspace_uris.iter().any(|u| uri_matches_cwd(u, cwd)) {
            continue;
        }
        candidates.push(SummaryRow {
            conversation_id,
            last_modified_ms,
            workspace_uris,
            running: status == "CASCADE_RUN_STATUS_RUNNING" || not_fully_idle != 0,
        });
    }

    pick_summary(candidates, started_ms, known_id)
}

fn pick_summary(
    candidates: Vec<SummaryRow>,
    started_ms: u64,
    known_id: Option<&str>,
) -> Option<SummaryRow> {
    let newer_running = candidates
        .iter()
        .find(|r| r.last_modified_ms >= started_ms && r.running);
    if let Some(id) = known_id
        && let Some(known) = candidates.iter().find(|r| r.conversation_id == id)
        && (known.running || newer_running.is_none_or(|n| n.conversation_id == id))
    {
        return Some(known.clone());
    }
    if let Some(running) = newer_running {
        return Some(running.clone());
    }
    candidates
        .into_iter()
        .find(|r| r.last_modified_ms >= started_ms)
}

fn uri_matches_cwd(uri: &str, cwd: &str) -> bool {
    let Some(path) = file_uri_to_path(uri) else {
        return false;
    };
    normalize_path(&path) == normalize_path(cwd)
}

fn file_uri_to_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let raw = if rest.starts_with('/') && rest.as_bytes().get(2) == Some(&b':') {
        &rest[1..]
    } else {
        rest
    };
    percent_decode(raw)
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex_val(bytes[i + 1])?;
            let lo = hex_val(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn normalize_path(path: &str) -> String {
    let mut normalized = path.replace('\\', "/");
    while normalized.len() > 1 && normalized.ends_with('/') {
        if normalized.len() == 3 && normalized.as_bytes().get(1) == Some(&b':') {
            break;
        }
        normalized.pop();
    }
    if cfg!(windows) {
        normalized.make_ascii_lowercase();
    }
    normalized
}

/// Parses SQLite datetime strings such as `2026-11-19 15:42:25.5632788+00:00` or RFC3339 `...Z` into Unix ms.
fn parse_timestamp_ms(s: &str) -> Option<u64> {
    let bytes = s.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    let hour: i64 = s.get(11..13)?.parse().ok()?;
    let minute: i64 = s.get(14..16)?.parse().ok()?;
    let second: i64 = s.get(17..19)?.parse().ok()?;

    let mut rest = &s[19..];
    let mut millis: i64 = 0;
    if let Some(after_dot) = rest.strip_prefix('.') {
        let digits_len = after_dot.bytes().take_while(|b| b.is_ascii_digit()).count();
        let frac = &after_dot[..digits_len];
        let mut padded = [b'0'; 3];
        for (idx, b) in frac.bytes().take(3).enumerate() {
            padded[idx] = b;
        }
        millis = std::str::from_utf8(&padded).ok()?.parse().ok()?;
        rest = &after_dot[digits_len..];
    }

    let offset_secs = parse_tz_offset_secs(rest.trim())?;
    let days = days_from_civil(year, month, day)?;
    let total_secs = days * 86_400 + hour * 3_600 + minute * 60 + second - offset_secs;
    u64::try_from(total_secs * 1_000 + millis).ok()
}

fn parse_tz_offset_secs(tz: &str) -> Option<i64> {
    if tz.is_empty() || tz == "Z" || tz == "z" {
        return Some(0);
    }
    let (sign, rest) = match tz.as_bytes().first()? {
        b'+' => (1_i64, &tz[1..]),
        b'-' => (-1_i64, &tz[1..]),
        _ => return None,
    };
    let (hh, mm) = if let Some((h, m)) = rest.split_once(':') {
        (h.parse::<i64>().ok()?, m.parse::<i64>().ok()?)
    } else if rest.len() == 4 {
        (
            rest[..2].parse::<i64>().ok()?,
            rest[2..].parse::<i64>().ok()?,
        )
    } else {
        return None;
    };
    Some(sign * (hh * 3_600 + mm * 60))
}

// Howard Hinnant's civil-from-epoch inverse, keeping UTC conversion dependency-free.
fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::accounts::testdir::TestDir;

    const ID: &str = "f1971cd7-d1ec-402a-a4b4-354536bc9a52";
    const OTHER_ID: &str = "0355a127-5aa1-4e83-b172-b454b4f22da4";

    fn write_agy_transcript(home: &Path, id: &str) {
        let dir = home
            .join("brain")
            .join(id)
            .join(".system_generated")
            .join("logs");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("transcript.jsonl"), "{}\n").unwrap();
    }

    #[test]
    fn is_agy_matches_executable_stem_case_insensitively() {
        assert!(is_agy("agy"));
        assert!(is_agy(r"C:\Users\u\AppData\Local\agy\bin\AGY.exe"));
        assert!(!is_agy("claude"));
        assert!(!is_agy("agy-helper"));
    }

    #[test]
    fn plan_agy_resume_uses_conversation_flag_when_transcript_exists() {
        let dir = TestDir::new();
        let base_args = vec!["--model".to_owned(), "gemini".to_owned()];

        assert_eq!(
            plan_agy_resume(&base_args, dir.path(), None, false),
            (base_args.clone(), Resume::New)
        );
        assert_eq!(
            plan_agy_resume(&base_args, dir.path(), Some(ID), false),
            (base_args.clone(), Resume::Missing)
        );
        assert_eq!(
            plan_agy_resume(&base_args, dir.path(), Some(ID), true),
            (base_args.clone(), Resume::Ended)
        );

        write_agy_transcript(dir.path(), ID);
        assert_eq!(
            plan_agy_resume(&base_args, dir.path(), Some(ID), false),
            (
                vec![
                    "--model".to_owned(),
                    "gemini".to_owned(),
                    "--conversation".to_owned(),
                    ID.to_owned(),
                ],
                Resume::Resuming,
            )
        );
    }

    #[test]
    fn file_uri_matches_windows_and_percent_encoded_paths() {
        assert!(uri_matches_cwd(
            "file:///C:/Users/amrit/Coding%20Projects/Cerberus",
            r"C:\Users\amrit\Coding Projects\Cerberus"
        ));
        assert!(!uri_matches_cwd(
            "file:///C:/Users/amrit/Other",
            r"C:\Users\amrit\Coding Projects\Cerberus"
        ));
    }

    #[test]
    fn parses_sqlite_datetime_with_nanoseconds_and_offset() {
        assert_eq!(
            parse_timestamp_ms("1970-01-01 00:00:01.2500000+00:00"),
            Some(1250)
        );
        assert_eq!(
            parse_timestamp_ms("2026-11-19T15:42:25.563Z"),
            parse_timestamp_ms("2026-11-19 15:42:25.5632788+00:00")
        );
    }

    #[test]
    fn tracker_saves_session_once_transcript_exists_and_reports_state_changes() {
        let mut tracker = AgyTracker::new(None);
        let row = SummaryRow {
            conversation_id: ID.to_owned(),
            last_modified_ms: 2_000,
            workspace_uris: vec![],
            running: true,
        };

        // Not saved before transcript.jsonl exists on disk.
        assert_eq!(
            tracker.observe(Some(&row), |_| false),
            (Some(TabState::Working), None)
        );
        // Saved once transcript.jsonl appears; Working does not re-emit.
        assert_eq!(
            tracker.observe(Some(&row), |_| true),
            (None, Some(ID.to_owned()))
        );

        let idle = SummaryRow {
            running: false,
            ..row
        };
        assert_eq!(
            tracker.observe(Some(&idle), |_| true),
            (Some(TabState::Idle), None)
        );
    }

    #[test]
    fn pick_summary_prefers_newer_running_session_and_keeps_known_when_no_newer_running() {
        let known = SummaryRow {
            conversation_id: ID.to_owned(),
            last_modified_ms: 500,
            workspace_uris: vec![],
            running: false,
        };
        let newer = SummaryRow {
            conversation_id: OTHER_ID.to_owned(),
            last_modified_ms: 2_000,
            workspace_uris: vec![],
            running: true,
        };

        assert_eq!(
            pick_summary(vec![newer.clone(), known.clone()], 1_000, Some(ID)),
            Some(newer)
        );
        assert_eq!(
            pick_summary(vec![known.clone()], 1_000, Some(ID)),
            Some(known)
        );
    }
}
