//! Whether an `agy` found on PATH is Antigravity CLI, since a package manager may put it in `/usr/bin` beside unrelated programs.

use std::path::Path;
#[cfg(unix)]
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, PoisonError},
    time::{Duration, SystemTime},
};

/// Its help lists these; `--version` prints a bare number that proves nothing.
#[cfg(unix)]
const OWN_FLAGS: [&str; 2] = ["--conversation", "--dangerously-skip-permissions"];
#[cfg(unix)]
const HELP_TIMEOUT: Duration = Duration::from_secs(3);

/// Answers by file and modification time, since the help check starts the program.
#[cfg(unix)]
static PROVEN: Mutex<Option<HashMap<PathBuf, (SystemTime, bool)>>> = Mutex::new(None);

/// A folder whose name says it holds Antigravity's `agy`.
pub fn names_it(dir: &Path) -> bool {
    let named = dir.to_string_lossy().to_lowercase();
    named.contains("antigravity") || named.contains("agy")
}

/// A link into a folder that names Antigravity, else a help text that lists its own flags.
#[cfg(unix)]
pub fn is_antigravity(agy: &Path) -> bool {
    let resolved = std::fs::canonicalize(agy).ok();
    if resolved
        .as_deref()
        .and_then(Path::parent)
        .is_some_and(names_it)
    {
        return true;
    }
    let Ok(modified) = agy.metadata().and_then(|m| m.modified()) else {
        return false;
    };
    let mut proven = PROVEN.lock().unwrap_or_else(PoisonError::into_inner);
    let known = proven.get_or_insert_with(HashMap::new);
    if let Some(&(at, answer)) = known.get(agy)
        && at == modified
    {
        return answer;
    }
    // A program that did not answer in time is asked again next time, not ruled out.
    let Some(answer) = lists_own_flags(agy) else {
        return false;
    };
    known.insert(agy.to_path_buf(), (modified, answer));
    answer
}

// The Windows installer's folder is known, so no other `agy` is taken there.
#[cfg(not(unix))]
pub fn is_antigravity(_agy: &Path) -> bool {
    false
}

/// `None` when it could not be asked; its help goes to stderr, so a shell joins the two.
#[cfg(unix)]
fn lists_own_flags(agy: &Path) -> Option<bool> {
    let mut command = crate::user_env::command("/bin/sh");
    command.args(["-c", r#"exec "$0" --help 2>&1"#]).arg(agy);
    let ran = crate::run::run(command, HELP_TIMEOUT).ok()?;
    let help = String::from_utf8_lossy(&ran.stdout);
    Some(OWN_FLAGS.iter().all(|flag| help.contains(flag)))
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::{PermissionsExt, symlink};

    use super::*;
    use crate::accounts::testdir::TestDir;

    fn script(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn a_link_into_an_antigravity_folder_is_antigravity() {
        let root = TestDir::new();
        let real = root.path().join("opt/antigravity/bin/agy");
        script(&real, "exit 0");
        let link = root.path().join("usr/bin/agy");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(&real, &link).unwrap();

        assert!(is_antigravity(&link));
    }

    #[test]
    fn a_program_whose_help_lists_antigravitys_flags_is_antigravity() {
        let root = TestDir::new();
        let agy = root.path().join("usr/bin/agy");
        script(
            &agy,
            "printf 'Usage of agy:\\n  --continue\\n  --conversation\\n  --dangerously-skip-permissions\\n' >&2",
        );

        assert!(is_antigravity(&agy));
    }

    #[test]
    fn an_unrelated_agy_is_not() {
        let root = TestDir::new();
        let agy = root.path().join("usr/bin/agy");
        script(&agy, "echo 'usage: agy [-v] file'");

        assert!(!is_antigravity(&agy));
    }

    #[test]
    fn a_program_that_hangs_on_help_is_not_and_is_cut_off() {
        let root = TestDir::new();
        let agy = root.path().join("usr/bin/agy");
        script(&agy, "sleep 30");
        let started = std::time::Instant::now();

        assert!(!is_antigravity(&agy));
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
