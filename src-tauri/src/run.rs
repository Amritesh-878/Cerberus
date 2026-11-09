use std::io::{self, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("could not start it: {0}")]
    Start(io::Error),
    #[error("could not read its answer: {0}")]
    Read(io::Error),
    #[error("it did not answer in {} s", .0.as_secs())]
    TimedOut(Duration),
}

/// What a finished program printed, and whether it exited with success.
#[derive(Debug)]
pub struct Ran {
    pub success: bool,
    pub stdout: Vec<u8>,
}

#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const EXIT_POLL: Duration = Duration::from_millis(10);

/// Runs a program with no window, no input and stderr dropped, ending its whole tree at the deadline.
pub fn run(mut command: Command, timeout: Duration) -> Result<Ran, RunError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut command, CREATE_NO_WINDOW);
    // Its own group, so a timeout can end whatever a wrapper script started.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn().map_err(RunError::Start)?;
    let Some(mut stdout) = child.stdout.take() else {
        return Err(RunError::Read(io::Error::other("no output pipe")));
    };
    let deadline = Instant::now() + timeout;
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .spawn(move || {
            let mut out = Vec::new();
            let _ = tx.send(stdout.read_to_end(&mut out).map(|_| out));
        })
        .map_err(RunError::Read)?;
    let result = match rx.recv_timeout(timeout) {
        Ok(read) => read.map_err(RunError::Read),
        Err(_) => Err(RunError::TimedOut(timeout)),
    };
    let mut success = false;
    let result = finished(
        result,
        || match exits_by(&mut child, deadline) {
            Some(ok) => {
                success = ok;
                true
            }
            None => false,
        },
        timeout,
    );
    if result.is_err() {
        kill_tree(&mut child);
    }
    let _ = child.wait();
    result.map(|stdout| Ran { success, stdout })
}

// Output from a program that then misses the deadline is not trusted, however complete it looks.
fn finished(
    read: Result<Vec<u8>, RunError>,
    exits: impl FnOnce() -> bool,
    timeout: Duration,
) -> Result<Vec<u8>, RunError> {
    let stdout = read?;
    if exits() {
        Ok(stdout)
    } else {
        Err(RunError::TimedOut(timeout))
    }
}

// A program can close its output and still not exit, so the wait shares the read's deadline.
fn exits_by(child: &mut Child, deadline: Instant) -> Option<bool> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status.success()),
            Ok(None) if Instant::now() < deadline => thread::sleep(EXIT_POLL),
            _ => return None,
        }
    }
}

// A `.cmd` shim's child is cmd.exe, so killing only that would leave the real program running.
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let mut taskkill = Command::new("taskkill");
        taskkill
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        std::os::windows::process::CommandExt::creation_flags(&mut taskkill, CREATE_NO_WINDOW);
        if taskkill.status().is_ok_and(|status| status.success()) {
            return;
        }
    }
    #[cfg(unix)]
    if let Ok(group) = libc::pid_t::try_from(child.id()) {
        // SAFETY: signals the group `run` gave the child; no memory is touched.
        if unsafe { libc::killpg(group, libc::SIGKILL) } == 0 {
            return;
        }
    }
    let _ = child.kill();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn output_is_read_whatever_the_exit_code() {
        let mut command = Command::new("cmd");
        // Raw, because Rust's quoting for an argument with quotes is not what cmd's echo expects.
        std::os::windows::process::CommandExt::raw_arg(&mut command, r"/c echo hello& exit 1");

        let ran = run(command, Duration::from_secs(20)).unwrap();

        assert!(!ran.success);
        assert!(String::from_utf8_lossy(&ran.stdout).starts_with("hello"));
    }

    #[cfg(unix)]
    #[test]
    fn output_is_read_whatever_the_exit_code() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "echo hello; exit 1"]);

        let ran = run(command, Duration::from_secs(20)).unwrap();

        assert!(!ran.success);
        assert!(String::from_utf8_lossy(&ran.stdout).starts_with("hello"));
    }

    #[test]
    fn output_from_a_program_that_never_exits_is_a_timeout() {
        let second = Duration::from_secs(1);
        let status = b"done".to_vec();

        let hung = finished(Ok(status.clone()), || false, second);
        let done = finished(Ok(status.clone()), || true, second);
        let unread = finished(Err(RunError::TimedOut(second)), || true, second);

        assert!(matches!(hung, Err(RunError::TimedOut(_))));
        assert_eq!(done.unwrap(), status);
        assert!(matches!(unread, Err(RunError::TimedOut(_))));
    }

    #[cfg(windows)]
    #[test]
    fn the_exit_wait_gives_up_at_the_deadline() {
        let mut hung = Command::new("powershell")
            .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"])
            .spawn()
            .unwrap();
        let mut quick = Command::new("cmd").args(["/c", "exit 0"]).spawn().unwrap();
        let started = Instant::now();

        let hung_exited = exits_by(&mut hung, started + Duration::from_millis(300));
        kill_tree(&mut hung);
        let _ = hung.wait();

        assert_eq!(hung_exited, None);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(
            exits_by(&mut quick, Instant::now() + Duration::from_secs(10)),
            Some(true)
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_hung_program_times_out() {
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"]);

        let result = run(command, Duration::from_millis(300));

        assert!(matches!(result, Err(RunError::TimedOut(_))));
    }

    #[cfg(windows)]
    const PID_FILE: &str = "CERBERUS_RUN_TEST_PID_FILE";

    /// Runs only as the wrapped program a test below starts: it writes its pid first thing, then hangs.
    #[cfg(windows)]
    #[test]
    fn wrapper_test_child() {
        let Some(pid_file) = std::env::var_os(PID_FILE) else {
            return;
        };
        std::fs::write(pid_file, std::process::id().to_string()).unwrap();
        thread::sleep(Duration::from_secs(30));
    }

    #[cfg(windows)]
    #[test]
    fn a_timeout_ends_what_a_wrapper_started() {
        let dir = crate::accounts::testdir::TestDir::new();
        let pid_file = dir.path().join("pid.txt");
        let exe = std::env::current_exe().unwrap();
        let mut command = Command::new("cmd");
        // This test binary as the wrapped program, since PowerShell can take longer to start than any timeout here.
        // `/s` strips only the outer quotes, so a path with spaces, brackets or a share name survives.
        std::os::windows::process::CommandExt::raw_arg(
            &mut command,
            format!(
                r#"/s /c ""{}" --exact run::tests::wrapper_test_child --nocapture""#,
                exe.display()
            ),
        );
        command.env(PID_FILE, &pid_file);

        let result = run(command, Duration::from_secs(10));

        assert!(matches!(result, Err(RunError::TimedOut(_))));
        let pid = std::fs::read_to_string(&pid_file).unwrap();
        let listed = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid.trim()), "/NH"])
            .output()
            .unwrap();
        let listed = String::from_utf8_lossy(&listed.stdout);
        assert!(!listed.contains(pid.trim()), "still running: {listed}");
    }

    #[cfg(unix)]
    #[test]
    fn the_exit_wait_gives_up_at_the_deadline() {
        use std::os::unix::process::CommandExt;
        let mut hung = Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .unwrap();
        let mut quick = Command::new("/usr/bin/true").spawn().unwrap();
        let started = Instant::now();

        let hung_exited = exits_by(&mut hung, started + Duration::from_millis(300));
        kill_tree(&mut hung);
        let _ = hung.wait();

        assert_eq!(hung_exited, None);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(
            exits_by(&mut quick, Instant::now() + Duration::from_secs(10)),
            Some(true)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_program_times_out() {
        let mut command = Command::new("/bin/sleep");
        command.arg("30");

        let result = run(command, Duration::from_millis(300));

        assert!(matches!(result, Err(RunError::TimedOut(_))));
    }

    #[cfg(unix)]
    #[test]
    fn a_timeout_ends_what_a_wrapper_started() {
        let dir = crate::accounts::testdir::TestDir::new();
        let pid_file = dir.path().join("pid.txt");
        let mut command = Command::new("/bin/sh");
        // Backgrounded and waited on, so sleep is the wrapper's child rather than its replacement.
        command.args([
            "-c",
            &format!("sleep 30 & echo $! > '{}'; wait", pid_file.display()),
        ]);

        let result = run(command, Duration::from_secs(5));

        assert!(matches!(result, Err(RunError::TimedOut(_))));
        let pid: u32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let gone = (0..100).any(|_| {
            thread::sleep(Duration::from_millis(20));
            !crate::sessions::procs::is_running(pid)
        });
        assert!(gone, "still running: {pid}");
    }
}
