use std::io;

/// Joins a job that Windows ends with this process, even on a forced kill, so no tab program outlives it.
#[cfg(windows)]
pub fn end_children_with_app() -> io::Result<()> {
    use std::mem::{size_of, zeroed};
    use std::ptr::null;

    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    // SAFETY: plain Win32 calls on a handle this function owns, with a zeroed struct of the documented size.
    unsafe {
        let job = CreateJobObjectW(null(), null());
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let set = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
                .map_err(io::Error::other)?,
        );
        if set == 0 || AssignProcessToJobObject(job, GetCurrentProcess()) == 0 {
            let error = io::Error::last_os_error();
            CloseHandle(job);
            return Err(error);
        }
        // The handle is left open on purpose: Windows closes it as the process ends, which ends the job.
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn end_children_with_app() -> io::Result<()> {
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    use super::*;

    const AS_PARENT: &str = "CERBERUS_JOB_TEST_PARENT";

    /// Runs only inside the parent process the next test starts, where it joins the job and starts a child.
    #[test]
    fn job_test_parent() {
        if std::env::var_os(AS_PARENT).is_none() {
            return;
        }
        end_children_with_app().unwrap();
        let mut child = Command::new("cmd")
            .args(["/c", "ping -n 120 127.0.0.1 >nul"])
            .spawn()
            .unwrap();
        println!("child-pid={}", child.id());
        // Blocks until the outer test kills this process.
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

    #[test]
    fn a_forced_kill_of_the_app_ends_its_children() {
        let mut parent = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "job::tests::job_test_parent", "--nocapture"])
            .env(AS_PARENT, "1")
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
}
