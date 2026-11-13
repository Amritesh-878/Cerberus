//! Who may open Cerberus's local pipes on Windows: this user and the system, nobody else.

use std::io;
use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

const SDDL_REVISION_1: u32 = 1;

/// A security descriptor that lets only this user, and the system, open the object.
pub struct Security(PSECURITY_DESCRIPTOR);

// The descriptor is read-only after it is made, and freed once in `Drop`.
unsafe impl Send for Security {}

impl Security {
    pub fn current_user_only() -> io::Result<Self> {
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

    /// Attributes pointing at this descriptor, so `self` must outlive the call they are passed to.
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(0),
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}

impl Drop for Security {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

/// Whether process `pid` runs as this user; one that can't be asked does not.
pub fn runs_as_me(pid: u32) -> bool {
    let theirs = unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return false;
        }
        let sid = token_sid(process);
        CloseHandle(process);
        sid
    };
    matches!((theirs, user_sid()), (Ok(theirs), Ok(mine)) if theirs == mine)
}

/// This process's user as an SDDL SID string, such as `S-1-5-21-…`.
fn user_sid() -> io::Result<String> {
    token_sid(unsafe { GetCurrentProcess() })
}

fn token_sid(process: HANDLE) -> io::Result<String> {
    let mut token: HANDLE = ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) } == 0 {
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

/// The DACL of a kernel object as SDDL, for tests that check who may open a pipe.
#[cfg(test)]
pub fn dacl_of(handle: HANDLE) -> String {
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SE_KERNEL_OBJECT,
    };
    use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;

    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    let read = unsafe {
        GetSecurityInfo(
            handle,
            SE_KERNEL_OBJECT,
            DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &raw mut descriptor,
        )
    };
    assert_eq!(read, 0, "GetSecurityInfo failed");
    let mut text: *mut u16 = ptr::null_mut();
    let made = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &raw mut text,
            ptr::null_mut(),
        )
    };
    assert_ne!(made, 0, "could not write the descriptor as SDDL");
    let len = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
    let sddl = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) });
    unsafe {
        LocalFree(text.cast());
        LocalFree(descriptor);
    }
    sddl
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_user_sid_is_a_sid() {
        assert!(user_sid().unwrap().starts_with("S-1-"));
    }

    #[test]
    fn this_process_runs_as_me_and_the_system_process_does_not() {
        assert!(runs_as_me(std::process::id()));
        // Pid 4 is always the kernel's System process.
        assert!(!runs_as_me(4));
    }
}
