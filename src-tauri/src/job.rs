use std::io;
use std::sync::OnceLock;

// A raw handle is not `Sync`, so the job is kept as its integer value.
static JOB: OnceLock<usize> = OnceLock::new();

/// Joins a job that Windows ends with this process on a crash or forced kill, so no tab program outlives it.
#[cfg(windows)]
pub fn end_children_with_app() -> io::Result<()> {
    use std::ptr::null;

    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    // SAFETY: plain Win32 calls on a handle this function owns.
    unsafe {
        let job = CreateJobObjectW(null(), null());
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let joined = set_limits(job, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE).and_then(|()| {
            if AssignProcessToJobObject(job, GetCurrentProcess()) == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
        if let Err(error) = joined {
            CloseHandle(job);
            return Err(error);
        }
        // Never closed: Windows closes it as the process ends, which is what ends the job on a crash.
        let _ = JOB.set(job as usize);
    }
    Ok(())
}

/// Drops kill-on-close before a normal exit, so a window started from a tab stays open, as in Windows Terminal.
#[cfg(windows)]
pub fn spare_children_on_exit() {
    if let Some(&job) = JOB.get()
        && let Err(e) = set_limits(job as windows_sys::Win32::Foundation::HANDLE, 0)
    {
        eprintln!("programs started from tabs may close with the app: {e}");
    }
}

#[cfg(windows)]
fn set_limits(job: windows_sys::Win32::Foundation::HANDLE, flags: u32) -> io::Result<()> {
    use std::mem::{size_of, zeroed};

    use windows_sys::Win32::System::JobObjects::{
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    let size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
        .map_err(io::Error::other)?;
    // SAFETY: a zeroed struct of the documented size, on a job handle that stays open for the process's life.
    let set = unsafe {
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
        limits.BasicLimitInformation.LimitFlags = flags;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size,
        )
    };
    if set == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn end_children_with_app() -> io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn spare_children_on_exit() {}

#[cfg(all(test, windows))]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    use super::*;

    const AS_PARENT: &str = "CERBERUS_JOB_TEST_PARENT";

    /// Runs only inside the parent process a test below starts: it joins the job, starts a child, then waits or exits.
    #[test]
    fn job_test_parent() {
        let Some(mode) = std::env::var_os(AS_PARENT) else {
            return;
        };
        end_children_with_app().unwrap();
        let mut child = Command::new("cmd")
            .args(["/c", "ping -n 120 127.0.0.1 >nul"])
            .spawn()
            .unwrap();
        println!("child-pid={}", child.id());
        if mode == "exit" {
            spare_children_on_exit();
            std::process::exit(0);
        }
        let _ = child.wait();
    }

    fn ended_within(pid: u32, wait: Duration) -> bool {
        // SAFETY: the handle is only waited on and closed here.
        unsafe {
            let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if process.is_null() {
                return true;
            }
            let millis = u32::try_from(wait.as_millis()).unwrap_or(u32::MAX);
            let ended = WaitForSingleObject(process, millis) == WAIT_OBJECT_0;
            CloseHandle(process);
            ended
        }
    }

    fn start_parent(mode: &str) -> (Child, u32) {
        let mut parent = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "job::tests::job_test_parent", "--nocapture"])
            .env(AS_PARENT, mode)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = parent.stdout.take().unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let child_pid: u32 = BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
            .take_while(|_| Instant::now() < deadline)
            .find_map(|line| line.strip_prefix("child-pid=").map(str::to_owned))
            .and_then(|pid| pid.trim().parse().ok())
            .expect("the parent never reported its child");
        (parent, child_pid)
    }

    #[test]
    fn a_forced_kill_of_the_app_ends_its_children() {
        let (mut parent, child_pid) = start_parent("kill");
        assert!(
            !ended_within(child_pid, Duration::ZERO),
            "the child ended early"
        );

        // `kill` is TerminateProcess, the same forced end as `taskkill /F`.
        parent.kill().unwrap();
        parent.wait().unwrap();

        assert!(
            ended_within(child_pid, Duration::from_secs(10)),
            "the child outlived its killed parent"
        );
    }

    #[test]
    fn a_normal_exit_leaves_a_detached_child_running() {
        let (mut parent, child_pid) = start_parent("exit");

        parent.wait().unwrap();
        let survived = !ended_within(child_pid, Duration::from_secs(2));
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &child_pid.to_string()])
            .output();

        assert!(survived, "the child ended with its parent's normal exit");
    }
}
