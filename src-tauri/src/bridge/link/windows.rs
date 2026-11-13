use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr;

use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

use crate::winsec::Security;

pub type Stream = File;

const BUFFER: u32 = 64 * 1024;

pub fn name_for(id: &str) -> String {
    format!(r"\\.\pipe\cerberus-bridge-{id}")
}

pub fn connect(name: &str) -> io::Result<Option<File>> {
    // ERROR_PIPE_BUSY: every instance is taken for a moment, and a fresh one follows.
    const PIPE_BUSY: i32 = 231;
    let mut tries = 40;
    loop {
        match File::options().read(true).write(true).open(name) {
            Ok(pipe) => return Ok(Some(pipe)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) if e.raw_os_error() == Some(PIPE_BUSY) && tries > 1 => {
                tries -= 1;
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Waits until the other side has read everything, so closing drops nothing.
pub fn finish(pipe: &File) -> io::Result<()> {
    pipe.sync_all()
}

/// The process at the client end of a pipe, held open so its exit can be waited on.
pub struct Peer(OwnedHandle);

impl Peer {
    /// Watches the process itself, since a read pending on a synchronous pipe would hold up every write on it.
    pub fn of(pipe: &File) -> io::Result<Self> {
        use windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId;
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};
        let mut pid = 0u32;
        if unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle(), &raw mut pid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        // A valid handle that nothing else owns, closed when the watch ends.
        Ok(Self(unsafe { OwnedHandle::from_raw_handle(process) }))
    }

    /// Blocks until the client process has exited.
    pub fn gone(self) {
        use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};
        unsafe { WaitForSingleObject(self.0.as_raw_handle(), INFINITE) };
    }
}

/// The next pipe instance, made before a client is handed out so the name never goes missing.
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

    /// The outer error stops listening; the inner one is a client that left early.
    pub fn accept(&mut self) -> io::Result<io::Result<File>> {
        let Some(pipe) = self.waiting.take() else {
            return Err(io::Error::other("the bridge lost its pipe"));
        };
        let connected = wait_for_client(pipe);
        self.waiting = Some(instance(&self.name, &self.security, false)?);
        Ok(connected)
    }
}

fn instance(name: &str, security: &Security, first: bool) -> io::Result<File> {
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
            BUFFER,
            BUFFER,
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

fn wait_for_client(pipe: File) -> io::Result<File> {
    use windows_sys::Win32::Foundation::ERROR_PIPE_CONNECTED;
    use windows_sys::Win32::System::Pipes::ConnectNamedPipe;
    let connected = unsafe { ConnectNamedPipe(pipe.as_raw_handle(), ptr::null_mut()) };
    if connected == 0 {
        let e = io::Error::last_os_error();
        // A client that connected before the wait began is already there.
        if e.raw_os_error() != Some(ERROR_PIPE_CONNECTED.cast_signed()) {
            return Err(e);
        }
    }
    Ok(pipe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_listener_cannot_take_the_name() {
        let name = super::super::new_name().unwrap();
        let _first = Listener::bind(&name).unwrap();

        assert!(Listener::bind(&name).is_err());
    }
}
