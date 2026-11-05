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
    // The API returns a value above 32 on success, else an error code that matches the Win32 one it names.
    let code = result as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(
            i32::try_from(code).unwrap_or_default(),
        ))
    }
}

/// Opens a folder in the file manager or a URL in the browser, as a double-click would.
#[cfg(unix)]
pub fn open(target: &OsStr) -> io::Result<()> {
    use std::process::{Command, Stdio};

    let mut command = Command::new(if cfg!(target_os = "macos") {
        "/usr/bin/open"
    } else {
        "xdg-open"
    });
    command
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // No pipes and no timeout, so the app it starts never holds a pipe open or gets killed with it.
    if cfg!(target_os = "macos") {
        let status = command.status()?;
        return if status.success() {
            Ok(())
        } else {
            Err(io::Error::other("open could not open it"))
        };
    }
    let mut child = command.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
