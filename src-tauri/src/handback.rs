//! A coder tab's finished handoff: noticing the file and the message that tells its planner tab.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::blocking::blocking;
use crate::bridge::{Bridge, BridgeError};
use crate::live::Live;
use crate::panel::{PrCache, branch_pr};
use crate::saved::TabId;

/// A planner chat that is still starting gets this long to link before the offer gives up.
const LINK_WAIT: Duration = Duration::from_secs(15);

/// The path with its folder resolved where it exists, so a symlinked or junctioned folder names the same file.
fn resolved(path: &Path) -> PathBuf {
    if let Ok(full) = fs::canonicalize(path) {
        return full;
    }
    match (path.parent().map(fs::canonicalize), path.file_name()) {
        (Some(Ok(folder)), Some(name)) => folder.join(name),
        _ => path.to_path_buf(),
    }
}

/// Compared as text: either separator, no `\\?\` prefix, and without case where the file system ignores it.
fn key(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = text.strip_prefix("//?/").unwrap_or(&text).to_owned();
    if cfg!(any(windows, target_os = "macos")) {
        text.to_lowercase()
    } else {
        text
    }
}

pub fn same_file(a: &Path, b: &Path) -> bool {
    key(&resolved(a)) == key(&resolved(b))
}

fn modified_since(file: &Path, since_ms: u64) -> bool {
    // A folder at the path is not a handoff, whenever it changed.
    fs::metadata(file)
        .ok()
        .filter(fs::Metadata::is_file)
        .and_then(|m| m.modified().ok())
        .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
        .is_some_and(|at| at.as_millis() >= u128::from(since_ms))
}

/// The chat reported writing the file, or it was written since the task opened; an older file is an earlier run's.
pub fn is_written(outputs: &[String], file: &Path, since_ms: u64) -> bool {
    outputs.iter().any(|o| same_file(Path::new(o), file)) || modified_since(file, since_ms)
}

/// Names the file and the PR only; the planner reads the handoff itself.
pub fn message(task_id: &str, file: &str, pr: Option<&str>) -> String {
    match pr {
        Some(pr) => format!("{task_id} handoff is ready: {file}. PR: {pr}"),
        None => format!("{task_id} handoff is ready: {file}."),
    }
}

#[tauri::command]
pub async fn handoff_written(app: AppHandle, tab: TabId, file: PathBuf, since: u64) -> bool {
    blocking(move || is_written(&app.state::<Live>().written(tab), &file, since)).await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// The given id, or one read from the file's name for a tab started by hand.
    pub task_id: String,
    pub text: String,
}

/// The message for the planner, with the PR of the coder's branch when `gh` knows one.
#[tauri::command]
pub async fn handoff_message(
    app: AppHandle,
    folder: PathBuf,
    file: PathBuf,
    task_id: Option<String>,
) -> Message {
    blocking(move || {
        let task_id = task_id.unwrap_or_else(|| crate::tasks::task_id(&file));
        let pr = branch_pr(&app.state::<PrCache>(), &folder);
        let text = message(&task_id, &file.to_string_lossy(), pr.as_deref());
        Message { task_id, text }
    })
    .await
}

/// Puts the message in the approval popup for a planner on another account.
#[tauri::command]
pub async fn handoff_offer(
    app: AppHandle,
    from: TabId,
    to: TabId,
    text: String,
) -> Result<u64, BridgeError> {
    blocking(move || app.state::<Bridge>().offer(from, to, text, LINK_WAIT)).await
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::accounts::testdir::TestDir;

    fn now_ms() -> u64 {
        let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        u64::try_from(since.as_millis()).unwrap()
    }

    #[test]
    fn a_write_reported_with_other_separators_matches_the_handoff() {
        let dir = TestDir::new();
        let file = dir.path().join("completed").join("task-026.md");
        let reported = file.to_string_lossy().replace('\\', "/");
        let other = dir.path().join("completed").join("task-027.md");

        assert!(is_written(std::slice::from_ref(&reported), &file, u64::MAX));
        assert!(!is_written(&[reported], &other, u64::MAX));
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn on_windows_and_macos_case_does_not_matter() {
        let dir = TestDir::new();
        let file = dir.path().join("completed").join("task-026.md");
        let shouted = dir.path().join("COMPLETED").join("TASK-026.md");

        assert!(is_written(
            &[shouted.to_string_lossy().into_owned()],
            &file,
            u64::MAX
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn on_linux_case_names_another_file() {
        let file = Path::new("/p/completed/task-026.md");

        assert!(!is_written(
            &["/p/completed/TASK-026.md".to_owned()],
            file,
            u64::MAX
        ));
    }

    fn linked_folder(real: &Path, link: &Path) {
        #[cfg(windows)]
        {
            let made = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(link)
                .arg(real)
                .output()
                .unwrap();
            assert!(made.status.success(), "no junction: {made:?}");
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(real, link).unwrap();
    }

    #[test]
    fn a_write_through_a_linked_folder_matches_the_handoff() {
        let dir = TestDir::new();
        let real = dir.path().join("plans");
        let link = dir.path().join("plans-link");
        fs::create_dir_all(real.join("completed")).unwrap();
        linked_folder(&real, &link);
        let through_link = link.join("completed").join("task-026.md");
        fs::write(&through_link, "done").unwrap();

        let reported = [through_link.to_string_lossy().into_owned()];

        assert!(is_written(
            &reported,
            &real.join("completed").join("task-026.md"),
            u64::MAX
        ));
    }

    #[test]
    fn a_file_written_since_the_task_opened_counts_and_an_older_one_does_not() {
        let dir = TestDir::new();
        let file = dir.path().join("task-026.md");
        fs::write(&file, "done").unwrap();
        let written_at = now_ms();

        assert!(is_written(&[], &file, written_at - 60_000));
        assert!(!is_written(&[], &file, written_at + 60_000));
        assert!(!is_written(&[], &dir.path().join("missing.md"), 0));
    }

    #[test]
    fn a_folder_at_the_handoff_path_is_not_a_handoff() {
        let dir = TestDir::new();
        let folder = dir.path().join("task-026.md");
        fs::create_dir_all(&folder).unwrap();

        assert!(!is_written(&[], &folder, 0));
    }

    #[test]
    fn the_message_names_the_task_file_and_pr_and_nothing_from_the_file() {
        let dir = TestDir::new();
        let file = dir.path().join("task-026.md");
        fs::write(&file, "SECRET handoff body").unwrap();
        let path = file.to_string_lossy();

        let with_pr = message("TASK-026", &path, Some("https://github.com/o/r/pull/64"));
        let without = message("TASK-026", &path, None);

        assert_eq!(
            with_pr,
            format!("TASK-026 handoff is ready: {path}. PR: https://github.com/o/r/pull/64")
        );
        assert_eq!(without, format!("TASK-026 handoff is ready: {path}."));
        assert!(!with_pr.contains("SECRET") && !without.contains("SECRET"));
    }
}
