pub mod carry;
mod limit;
pub mod procs;
mod watch;

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use crate::sessions::watch::{TabState, Watched, watch};

// Claude Code shortens a longer slug and appends a hash of the full path.
const SLUG_MAX: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Busy,
    Idle,
    Unknown,
}

/// The fields Cerberus uses from `<config dir>/sessions/<pid>.json`; every other field is ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFile {
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub status: Status,
    /// When the process started, in milliseconds since 1970.
    pub started_at: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSessionFile {
    session_id: Option<String>,
    cwd: Option<String>,
    status: Option<String>,
    started_at: Option<f64>,
}

/// `None` for a file that is not a session object; a field that is missing or odd reads as unknown.
pub fn parse_session_file(bytes: &[u8]) -> Option<SessionFile> {
    let raw: RawSessionFile = serde_json::from_slice(bytes).ok()?;
    Some(SessionFile {
        session_id: raw.session_id.filter(|id| is_session_id(id)),
        cwd: raw.cwd,
        status: match raw.status.as_deref() {
            Some("busy") => Status::Busy,
            Some("idle") => Status::Idle,
            _ => Status::Unknown,
        },
        started_at: raw
            .started_at
            .filter(|ms| ms.is_finite() && *ms >= 0.0)
            .map(|ms| ms as u64),
    })
}

/// A session id goes into a file name and a command line, so only UUID characters pass.
pub fn is_session_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && !id.starts_with('-')
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

pub fn is_claude(program: &str) -> bool {
    Path::new(program)
        .file_stem()
        .is_some_and(|stem| stem.eq_ignore_ascii_case("claude"))
}

/// Claude Code's folder name for a cwd under `projects/`: every UTF-16 unit that is not an ASCII letter or digit becomes `-`.
pub fn slug(cwd: &str) -> String {
    cwd.chars()
        .flat_map(|c| {
            let kept = c.is_ascii_alphanumeric().then_some(c);
            let dashes = if kept.is_some() { 0 } else { c.len_utf16() };
            kept.into_iter().chain(std::iter::repeat_n('-', dashes))
        })
        .collect()
}

/// The chat's transcript, or `None` when it is gone and `--resume` would fail.
pub fn transcript(config_dir: &Path, cwd: &str, session_id: &str) -> Option<PathBuf> {
    if !is_session_id(session_id) {
        return None;
    }
    let projects = config_dir.join("projects");
    let file = format!("{session_id}.jsonl");
    let slug = slug(cwd);
    if slug.len() <= SLUG_MAX {
        return Some(projects.join(slug).join(file)).filter(|p| p.is_file());
    }
    let prefix = format!("{}-", &slug[..SLUG_MAX]);
    fs::read_dir(&projects)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .map(|entry| entry.path().join(&file))
        .find(|p| p.is_file())
}

pub fn resume_args(args: &[String], session_id: &str) -> Vec<String> {
    let mut resumed = args.to_vec();
    resumed.extend(["--resume".to_owned(), session_id.to_owned()]);
    resumed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Resume {
    New,
    Resuming,
    /// The tab had a chat whose transcript is gone, so it starts a new one.
    Missing,
}

pub fn plan_resume(
    args: &[String],
    config_dir: &Path,
    cwd: &str,
    saved: Option<&str>,
) -> (Vec<String>, Resume) {
    match saved {
        None => (args.to_vec(), Resume::New),
        Some(id) if transcript(config_dir, cwd, id).is_some() => {
            (resume_args(args, id), Resume::Resuming)
        }
        Some(_) => (args.to_vec(), Resume::Missing),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::accounts::testdir::TestDir;

    const SESSION: &str = "0f6c3a52-7d1e-4b8a-9c2f-5e4d3b2a1f00";

    #[test]
    fn a_busy_session_file_gives_its_id_cwd_and_status() {
        let file = br#"{"pid":4242,"sessionId":"0f6c3a52-7d1e-4b8a-9c2f-5e4d3b2a1f00",
            "cwd":"C:\\work\\app","status":"busy","name":"fake chat","updatedAt":1}"#;

        assert_eq!(
            parse_session_file(file),
            Some(SessionFile {
                session_id: Some(SESSION.to_owned()),
                cwd: Some(r"C:\work\app".to_owned()),
                status: Status::Busy,
                started_at: None,
            })
        );
    }

    #[test]
    fn an_idle_session_file_reads_as_idle() {
        let file = br#"{"sessionId":"abc-123","status":"idle"}"#;

        assert_eq!(parse_session_file(file).unwrap().status, Status::Idle);
    }

    #[test]
    fn missing_or_new_fields_read_as_unknown() {
        let file = br#"{"status":"thinking-hard","extra":[1,2]}"#;

        assert_eq!(
            parse_session_file(file),
            Some(SessionFile {
                session_id: None,
                cwd: None,
                status: Status::Unknown,
                started_at: None,
            })
        );
    }

    #[test]
    fn the_start_time_is_read_whole_or_fractional() {
        let whole = br#"{"status":"idle","startedAt":1793286414170}"#;
        let fractional = br#"{"status":"idle","startedAt":1793286414170.5}"#;

        assert_eq!(
            parse_session_file(whole).unwrap().started_at,
            Some(1_793_286_414_170)
        );
        assert_eq!(
            parse_session_file(fractional).unwrap().started_at,
            Some(1_793_286_414_170)
        );
    }

    #[test]
    fn garbage_is_not_a_session_file() {
        assert_eq!(parse_session_file(b""), None);
        assert_eq!(parse_session_file(b"{\"sessionId\":"), None);
        assert_eq!(parse_session_file(b"[1,2,3]"), None);
        assert_eq!(parse_session_file(br#"{"status":7}"#), None);
    }

    #[test]
    fn a_session_id_that_is_not_uuid_shaped_is_dropped() {
        let file = br#"{"sessionId":"..\\..\\secrets","status":"idle"}"#;

        assert_eq!(parse_session_file(file).unwrap().session_id, None);
        assert!(!is_session_id("--print"));
        assert!(!is_session_id(""));
        assert!(!is_session_id(&"a".repeat(65)));
        assert!(is_session_id(SESSION));
    }

    #[test]
    fn the_slug_matches_claude_codes_folder_names() {
        assert_eq!(slug(r"C:\Users\u\desktop"), "C--Users-u-desktop");
        assert_eq!(
            slug(r"C:\Users\u\My Work\app_v2.1"),
            "C--Users-u-My-Work-app-v2-1"
        );
        assert_eq!(slug("D:/é"), "D---");
        assert_eq!(slug("C:\\😀"), "C----");
    }

    #[test]
    fn a_nested_cwd_with_dots_and_spaces_slugs_every_separator() {
        assert_eq!(
            slug(r"C:\Users\u\My Work\app.v2\sub dir\deep"),
            "C--Users-u-My-Work-app-v2-sub-dir-deep"
        );
    }

    #[test]
    fn the_transcript_is_found_under_the_cwds_slug() {
        let dir = TestDir::new();
        let folder = dir.path().join("projects").join("C--work-app");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(format!("{SESSION}.jsonl")), "{}\n").unwrap();

        assert_eq!(
            transcript(dir.path(), r"C:\work\app", SESSION),
            Some(folder.join(format!("{SESSION}.jsonl")))
        );
        assert_eq!(transcript(dir.path(), r"C:\work\other", SESSION), None);
        assert_eq!(transcript(dir.path(), r"C:\work\app", "a1b2"), None);
    }

    #[test]
    fn a_long_cwd_finds_its_hashed_folder() {
        let dir = TestDir::new();
        let cwd = format!(r"C:\{}", "deep\\".repeat(50));
        let shortened = format!("{}-1x2y3z", &slug(&cwd)[..SLUG_MAX]);
        let folder = dir.path().join("projects").join(shortened);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(format!("{SESSION}.jsonl")), "{}\n").unwrap();

        assert_eq!(
            transcript(dir.path(), &cwd, SESSION),
            Some(folder.join(format!("{SESSION}.jsonl")))
        );
    }

    #[test]
    fn resuming_adds_the_session_after_the_programs_own_args() {
        assert_eq!(
            resume_args(&["--verbose".to_owned()], SESSION),
            ["--verbose", "--resume", SESSION]
        );
    }

    #[test]
    fn a_saved_chat_resumes_only_while_its_transcript_exists() {
        let dir = TestDir::new();
        let folder = dir.path().join("projects").join("C--work-app");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(format!("{SESSION}.jsonl")), "{}\n").unwrap();
        let gone = "a1b2c3d4-0000-4000-8000-000000000001";

        assert_eq!(
            plan_resume(&[], dir.path(), r"C:\work\app", None),
            (Vec::new(), Resume::New)
        );
        assert_eq!(
            plan_resume(&[], dir.path(), r"C:\work\app", Some(SESSION)),
            (resume_args(&[], SESSION), Resume::Resuming)
        );
        assert_eq!(
            plan_resume(&[], dir.path(), r"C:\work\app", Some(gone)),
            (Vec::new(), Resume::Missing)
        );
    }

    #[test]
    fn claude_is_known_by_its_file_name() {
        assert!(is_claude(r"C:\Users\u\.local\bin\claude.exe"));
        assert!(is_claude(r"C:\npm\CLAUDE.cmd"));
        assert!(!is_claude(
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"
        ));
        assert!(!is_claude(r"C:\tools\claude-helper.exe"));
    }
}
