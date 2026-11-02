use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::thread::JoinHandle;

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
                allow_focus(&pipe);
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

/// Windows lets only the foreground process raise a window, so the asker passes that right on.
fn allow_focus(pipe: &File) {
    use windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;
    let mut server = 0;
    unsafe {
        if GetNamedPipeServerProcessId(pipe.as_raw_handle(), &raw mut server) != 0 {
            AllowSetForegroundWindow(server);
        }
    }
}

/// The next pipe instance, made before an asker is handed out so the name never goes missing.
pub struct Listener {
    name: String,
    waiting: Option<File>,
}

impl Listener {
    pub fn bind(name: &str) -> io::Result<Self> {
        Ok(Self {
            name: name.to_owned(),
            waiting: Some(instance(name, true)?),
        })
    }

    /// The outer error stops listening; the inner one is an asker that left early.
    pub fn accept(&mut self) -> io::Result<io::Result<File>> {
        let Some(pipe) = self.waiting.take() else {
            return Err(io::Error::other("the listener lost its pipe"));
        };
        let connected = wait_for_asker(pipe);
        // The next instance exists before this one can close, so an asker never finds the name missing.
        self.waiting = Some(instance(&self.name, false)?);
        Ok(connected)
    }
}

/// A new pipe instance; the first one fails if another process already owns the name.
fn instance(name: &str, first: bool) -> io::Result<File> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::Pipes::{
        CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
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
            std::ptr::null(),
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
