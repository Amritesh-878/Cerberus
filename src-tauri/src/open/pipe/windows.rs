use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::thread::JoinHandle;

use crate::winsec::{Security, runs_as_me};

pub type Stream = File;

/// Per Windows session, so two people signed in at once each reach their own app.
pub fn name(identifier: &str) -> String {
    format!(r"\\.\pipe\{identifier}-open-{}", session())
}

fn session() -> u32 {
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    let mut session = 0;
    // On failure this stays 0, which still names one pipe for the whole machine.
    unsafe { ProcessIdToSessionId(std::process::id(), &raw mut session) };
    session
}

pub fn connect(name: &str) -> io::Result<Option<File>> {
    // ERROR_PIPE_BUSY: the app is answering someone else, and a fresh instance follows at once.
    const PIPE_BUSY: i32 = 231;
    let mut tries = 20;
    loop {
        match File::options().read(true).write(true).open(name) {
            Ok(pipe) => {
                let server = server_of(&pipe);
                // A pipe made first by another user's program would otherwise get the request and the focus.
                if !server.is_some_and(runs_as_me) {
                    return Err(io::Error::other(
                        "the program answering on Cerberus's pipe runs as another user",
                    ));
                }
                allow_focus(server);
                return Ok(Some(pipe));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) if e.raw_os_error() == Some(PIPE_BUSY) && tries > 1 => {
                tries -= 1;
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(e) => return Err(e),
        }
    }
}

fn server_of(pipe: &File) -> Option<u32> {
    use windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId;
    let mut server = 0;
    let read = unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &raw mut server) };
    (read != 0).then_some(server)
}

/// Windows lets only the foreground process raise a window, so the asker passes that right on.
fn allow_focus(server: Option<u32>) {
    use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;
    if let Some(server) = server {
        unsafe { AllowSetForegroundWindow(server) };
    }
}

/// The next pipe instance, made before an asker is handed out so the name never goes missing.
pub struct Listener {
    name: String,
    security: Security,
    waiting: Option<File>,
}

impl Listener {
    pub fn bind(name: &str) -> io::Result<Self> {
        let security = Security::current_user_only()?;
        let waiting = Some(instance(name, &security, true)?);
        Ok(Self {
            name: name.to_owned(),
            security,
            waiting,
        })
    }

    /// The outer error stops listening; the inner one is an asker that left early.
    pub fn accept(&mut self) -> io::Result<io::Result<File>> {
        let Some(pipe) = self.waiting.take() else {
            return Err(io::Error::other("the listener lost its pipe"));
        };
        let connected = wait_for_asker(pipe);
        // The next instance exists before this one can close, so an asker never finds the name missing.
        self.waiting = Some(instance(&self.name, &self.security, false)?);
        Ok(connected)
    }
}

/// A new pipe instance; the first one fails if another process already owns the name.
fn instance(name: &str, security: &Security, first: bool) -> io::Result<File> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::Pipes::{
        CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    let attributes = security.attributes();
    let open_mode = PIPE_ACCESS_DUPLEX
        | if first {
            FILE_FLAG_FIRST_PIPE_INSTANCE
        } else {
            0
        };
    let handle = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            open_mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            4096,
            4096,
            0,
            &raw const attributes,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // A valid handle that nothing else owns, so the File closes it.
    Ok(File::from(unsafe { OwnedHandle::from_raw_handle(handle) }))
}

fn wait_for_asker(pipe: File) -> io::Result<File> {
    use windows_sys::Win32::Foundation::ERROR_PIPE_CONNECTED;
    use windows_sys::Win32::System::Pipes::ConnectNamedPipe;
    let connected = unsafe { ConnectNamedPipe(pipe.as_raw_handle(), std::ptr::null_mut()) };
    if connected == 0 {
        let e = io::Error::last_os_error();
        // A client that connected before the wait began is already there.
        if e.raw_os_error() != Some(ERROR_PIPE_CONNECTED.cast_signed()) {
            return Err(e);
        }
    }
    Ok(pipe)
}

/// Ends the worker's blocked pipe call; a worker with nothing pending just fails the call.
pub fn cutter(
    _pipe: &File,
    _deadline: std::time::Duration,
) -> io::Result<impl Fn(&JoinHandle<()>) + Send + 'static> {
    Ok(|worker: &JoinHandle<()>| {
        use windows_sys::Win32::System::IO::CancelSynchronousIo;
        // The handle stays valid while the caller holds `worker`.
        unsafe { CancelSynchronousIo(worker.as_raw_handle()) };
    })
}

/// Closing without this can drop a reply the asker has not read yet.
pub fn finish(pipe: &File) -> io::Result<()> {
    pipe.sync_all()
}

#[cfg(test)]
pub fn unique_name(n: u32) -> String {
    format!(r"\\.\pipe\cerberus-test-{}-{n}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_this_user_and_the_system_may_open_the_pipe() {
        let listener = Listener::bind(&unique_name(9_000)).unwrap();
        let pipe = listener.waiting.as_ref().unwrap();

        let dacl = crate::winsec::dacl_of(pipe.as_raw_handle());

        // Protected, so nothing is inherited, and exactly two entries: the user and SYSTEM.
        assert!(dacl.starts_with("D:P"), "{dacl}");
        assert_eq!(dacl.matches("(A;").count(), 2, "{dacl}");
        assert!(dacl.ends_with(";;;SY)"), "{dacl}");
    }
}
