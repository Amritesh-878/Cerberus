use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

pub type Stream = File;

const BUFFER: u32 = 64 * 1024;
const SDDL_REVISION_1: u32 = 1;

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

/// A security descriptor that lets only this user, and the system, open the pipe.
struct Security(PSECURITY_DESCRIPTOR);

// The descriptor is read-only after it is made, and freed once in `Drop`.
unsafe impl Send for Security {}

impl Security {
    fn current_user_only() -> io::Result<Self> {
        let sddl = format!("D:P(A;;GA;;;{})(A;;GA;;;SY)", user_sid()?);
        let wide: Vec<u16> = sddl.encode_utf16().chain([0]).collect();
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        let made = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &raw mut descriptor,
                ptr::null_mut(),
            )
        };
        if made == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(descriptor))
    }
}

impl Drop for Security {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

/// This process's user as an SDDL SID string, such as `S-1-5-21-…`.
fn user_sid() -> io::Result<String> {
    let mut token: HANDLE = ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let result = sid_of(token);
    unsafe { CloseHandle(token) };
    result
}

fn sid_of(token: HANDLE) -> io::Result<String> {
    let mut size = 0u32;
    unsafe { GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &raw mut size) };
    // u64 words keep the buffer aligned for the TOKEN_USER read below.
    let mut buf = vec![0u64; (size as usize).div_ceil(8).max(1)];
    let filled = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buf.as_mut_ptr().cast(),
            size,
            &raw mut size,
        )
    };
    if filled == 0 {
        return Err(io::Error::last_os_error());
    }
    let user = unsafe { &*buf.as_ptr().cast::<TOKEN_USER>() };
    let mut text: *mut u16 = ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &raw mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let len = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
    let sid = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) });
    unsafe { LocalFree(text.cast()) };
    Ok(sid)
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
    let attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(0),
        lpSecurityDescriptor: security.0,
        bInheritHandle: 0,
    };
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
    fn the_user_sid_is_a_sid() {
        assert!(user_sid().unwrap().starts_with("S-1-"));
    }

    #[test]
    fn a_second_listener_cannot_take_the_name() {
        let name = super::super::new_name().unwrap();
        let _first = Listener::bind(&name).unwrap();

        assert!(Listener::bind(&name).is_err());
    }
}
