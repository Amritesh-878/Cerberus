use std::ffi::OsString;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

// Frames the value, since rc files may print their own lines around it.
const MARK: &str = "__CERBERUS_PATH__";
// An rc file that waits for input or a network call must not hold up the window for long.
const TIMEOUT: Duration = Duration::from_secs(5);

/// An app opened from Finder or the Dock gets launchd's bare PATH, where `claude` is rarely found.
pub fn adopt() {
    // Only a terminal sets TERM, and a PATH from there is already the user's.
    if std::env::var_os("TERM").is_some() {
        return;
    }
    let shell = std::env::var_os("SHELL")
        .filter(|s| Path::new(s).is_absolute())
        .unwrap_or_else(|| OsString::from("/bin/zsh"));
    let Some(login) = read_within(Path::new(&shell), TIMEOUT) else {
        eprintln!(
            "could not read the login shell's PATH; programs outside /usr/bin may not be found"
        );
        return;
    };
    let current = std::env::var("PATH").unwrap_or_default();
    // SAFETY: runs first thing in `run`, before the app has started any thread that reads the environment.
    unsafe { std::env::set_var("PATH", merge(&login, &current)) };
}

fn read_within(shell: &Path, timeout: Duration) -> Option<String> {
    let script = format!(r#"printf '%s%s%s' {MARK} "$PATH" {MARK}"#);
    let mut child = Command::new(shell)
        .args(["-ilc", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut out = String::new();
        let _ = tx.send(stdout.read_to_string(&mut out).map(|_| out));
    });
    let answer = rx.recv_timeout(timeout);
    if answer.is_err()
        && let Ok(group) = libc::pid_t::try_from(child.id())
    {
        // SAFETY: signals the group this function gave the shell; no memory is touched.
        unsafe { libc::killpg(group, libc::SIGKILL) };
    }
    let _ = child.wait();
    parse(&answer.ok()?.ok()?).map(str::to_owned)
}

fn parse(output: &str) -> Option<&str> {
    let (_, rest) = output.split_once(MARK)?;
    let (path, _) = rest.split_once(MARK)?;
    (!path.trim().is_empty()).then_some(path)
}

/// The login shell's dirs first, then any the app already had, each once.
fn merge(login: &str, current: &str) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for dir in login.split(':').chain(current.split(':')) {
        if !dir.is_empty() && !seen.contains(&dir) {
            seen.push(dir);
        }
    }
    seen.join(":")
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn the_path_is_read_between_the_marks_whatever_rc_files_print() {
        let output = format!("Welcome!\n{MARK}/opt/homebrew/bin:/usr/bin{MARK}bye\n");

        assert_eq!(parse(&output), Some("/opt/homebrew/bin:/usr/bin"));
        assert_eq!(parse(&format!("{MARK}{MARK}")), None);
        assert_eq!(parse(&format!("{MARK}/usr/bin")), None);
        assert_eq!(parse("no marks"), None);
    }

    #[test]
    fn the_login_dirs_come_first_and_none_repeat() {
        let merged = merge(
            "/Users/u/.local/bin:/opt/homebrew/bin:/usr/bin",
            "/usr/bin:/bin::/usr/sbin",
        );

        assert_eq!(
            merged,
            "/Users/u/.local/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin"
        );
    }

    #[test]
    fn a_real_login_shell_reports_its_path() {
        let path = read_within(Path::new("/bin/sh"), Duration::from_secs(20)).unwrap();

        assert!(path.split(':').any(|dir| dir == "/usr/bin"), "{path}");
    }

    #[test]
    fn a_shell_that_hangs_is_cut_off() {
        use std::os::unix::fs::PermissionsExt;
        let dir = crate::accounts::testdir::TestDir::new();
        let shell = dir.path().join("hangs");
        std::fs::write(&shell, "#!/bin/sh\nsleep 30\n").unwrap();
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
        let started = Instant::now();

        assert_eq!(read_within(&shell, Duration::from_millis(300)), None);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }
}
