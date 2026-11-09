//! Everything a service started, so Stop and app close end the whole tree, not just its shell.

use std::io;

#[cfg(windows)]
pub use windows::Tree;

#[cfg(unix)]
pub use unix::Tree;

#[cfg(windows)]
mod windows {
    use std::io;
    use std::mem::{size_of, zeroed};
    use std::ptr::null;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };

    /// A job of its own; children the shell starts join it, and closing it ends them, even on a crash.
    pub struct Tree {
        job: HANDLE,
    }

    // SAFETY: a job handle is a kernel object reference, usable from any thread.
    unsafe impl Send for Tree {}
    // SAFETY: as above; the job calls used here are thread-safe.
    unsafe impl Sync for Tree {}

    impl Tree {
        pub fn adopt(pid: u32) -> io::Result<Self> {
            // SAFETY: plain Win32 calls on handles this function owns or closes.
            unsafe {
                let job = CreateJobObjectW(null(), null());
                if job.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let tree = Self { job };
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
                    .map_err(io::Error::other)?;
                if SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    (&raw const limits).cast(),
                    size,
                ) == 0
                {
                    return Err(io::Error::last_os_error());
                }
                let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
                if process.is_null() {
                    return Err(io::Error::last_os_error());
                }
                // Nested inside the app's own job, which Windows has allowed since Windows 8.
                let joined = AssignProcessToJobObject(job, process);
                CloseHandle(process);
                if joined == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(tree)
            }
        }

        /// Ends every process in the job; dropping it after closes the handle.
        pub fn end(self) {
            // SAFETY: the job handle stays open until this value drops at the end of the call.
            unsafe {
                TerminateJobObject(self.job, 1);
            }
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            // SAFETY: owned handle, closed once; kill-on-close ends anything still in the job.
            unsafe {
                CloseHandle(self.job);
            }
        }
    }
}

#[cfg(unix)]
mod unix {
    use std::io;
    use std::thread;
    use std::time::Duration;

    /// The process group the pseudo-terminal's session leader heads, which the shell's children share.
    pub struct Tree {
        group: libc::pid_t,
    }

    const GRACE: Duration = Duration::from_secs(2);

    impl Tree {
        pub fn adopt(pid: u32) -> io::Result<Self> {
            let group = libc::pid_t::try_from(pid).map_err(io::Error::other)?;
            Ok(Self { group })
        }

        /// TERM first so a dev server can clean up, then KILL whatever is left.
        pub fn end(self) {
            let group = self.group;
            // Its drop would KILL at once and leave no grace, so only the delayed KILL below runs.
            std::mem::forget(self);
            // SAFETY: signalling a process group by id; a gone group only returns an error.
            unsafe {
                libc::kill(-group, libc::SIGTERM);
            }
            let _ = thread::Builder::new().spawn(move || {
                thread::sleep(GRACE);
                // SAFETY: as above.
                unsafe {
                    libc::kill(-group, libc::SIGKILL);
                }
            });
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            // SAFETY: as above; nothing of a dropped service may outlive it.
            unsafe {
                libc::kill(-self.group, libc::SIGKILL);
            }
        }
    }
}

/// The shell's arguments that run `command` once and exit, by the shell's id.
pub fn shell_args(shell_id: &str, base: &[String], command: &str) -> io::Result<Vec<String>> {
    match shell_id {
        // `-EncodedCommand` takes the text as is; `-Command` re-parses quotes a second time.
        "powershell" => Ok(vec![
            "-NoLogo".into(),
            "-EncodedCommand".into(),
            utf16_base64(command),
        ]),
        "git-bash" => Ok(vec!["--login".into(), "-c".into(), command.into()]),
        // Unix shells are listed with `-l`; `-c` after it runs the command in a login shell.
        _ if base == ["-l"] => Ok(vec!["-l".into(), "-c".into(), command.into()]),
        other => Err(io::Error::other(format!(
            "a service can't run in the {other} shell"
        ))),
    }
}

fn utf16_base64(text: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(TABLE[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powershell_gets_the_command_encoded_so_quotes_survive() {
        let args = shell_args("powershell", &["-NoLogo".into()], "echo \"hi\"").unwrap();

        assert_eq!(args[..2], ["-NoLogo", "-EncodedCommand"]);
        // `echo "hi"` in UTF-16LE, base64, as PowerShell's own [Convert]::ToBase64String gives it.
        assert_eq!(args[2], "ZQBjAGgAbwAgACIAaABpACIA");
    }

    #[test]
    fn bash_and_unix_shells_run_the_command_once_in_a_login_shell() {
        assert_eq!(
            shell_args("git-bash", &["--login".into(), "-i".into()], "npm run dev").unwrap(),
            ["--login", "-c", "npm run dev"]
        );
        assert_eq!(
            shell_args("zsh", &["-l".into()], "npm run dev").unwrap(),
            ["-l", "-c", "npm run dev"]
        );
        assert!(shell_args("something", &[], "x").is_err());
    }

    #[test]
    fn base64_pads_each_length() {
        assert_eq!(utf16_base64(""), "");
        assert_eq!(utf16_base64("a"), "YQA=");
        assert_eq!(utf16_base64("ab"), "YQBiAA==");
        assert_eq!(utf16_base64("abc"), "YQBiAGMA");
    }
}
