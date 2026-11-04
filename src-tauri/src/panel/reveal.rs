use std::ffi::OsStr;
use std::io;

/// Opens a folder in the file manager or a URL in the browser, as a double-click would.
#[cfg(windows)]
pub fn open(target: &OsStr) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide: Vec<u16> = target.encode_wide().chain(Some(0)).collect();
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    // SAFETY: both strings are NUL-terminated and outlive the call; the other pointers may be null.
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            verb.as_ptr(),
            wide.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    };
    // The API returns a value above 32 on success, anything else is an error code.
    if result as isize > 32 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
pub fn open(target: &OsStr) -> io::Result<()> {
    use std::process::Command;
    use std::time::Duration;

    let program = if cfg!(target_os = "macos") {
        "/usr/bin/open"
    } else {
        "xdg-open"
    };
    let mut command = Command::new(program);
    command.arg(target);
    let ran = crate::run::run(command, Duration::from_secs(10)).map_err(io::Error::other)?;
    if ran.success {
        Ok(())
    } else {
        Err(io::Error::other(format!("{program} failed")))
    }
}
