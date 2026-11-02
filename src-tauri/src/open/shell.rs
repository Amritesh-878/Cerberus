use std::path::Path;
use std::process::{Command as Process, ExitCode, Stdio};

use crate::open::{Checked, Command, check, pipe};
use crate::saved::{self, TreeError};

/// The bundle identifier, which names the app's data folder and its pipe; a test ties it to `tauri.conf.json`.
pub const IDENTIFIER: &str = "com.cerberus.terminal";

/// The app beside the console command, as the installer or the `.app` bundle lays them out.
#[cfg(windows)]
const APP: &str = "cerberus.exe";
#[cfg(not(windows))]
const APP: &str = "cerberus";

pub fn arguments(all: impl Iterator<Item = String>) -> Vec<String> {
    all.skip(1).collect()
}

pub fn own_arguments() -> Vec<String> {
    arguments(std::env::args_os().map(|a| a.to_string_lossy().into_owned()))
}

/// Asks a running app, else reads the saved file, before anything starts.
pub fn checked(identifier: &str, args: &[String]) -> Checked {
    let ask = |args: &[String]| pipe::ask(&pipe::name(identifier), args);
    let read = || {
        // Where Tauri's `app_config_dir` points, found before there is an app to ask.
        let dir = dirs::config_dir()
            .ok_or_else(|| TreeError::Unavailable("no app data folder".to_owned()))?;
        saved::read_unlocked(&dir.join(identifier).join(saved::FILE_NAME))
    };
    check(args, ask, read)
}

pub fn say(code: i32, message: &str) {
    if code == 0 {
        println!("{message}");
    } else {
        eprintln!("{message}");
    }
}

fn exit_code(code: i32) -> ExitCode {
    u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from)
}

/// The console command a shell runs: it waits for the answer, unlike the windowed app.
pub fn console() -> ExitCode {
    let args = own_arguments();
    match checked(IDENTIFIER, &args) {
        Checked::Exit(code, message) => {
            say(code, &message);
            exit_code(code)
        }
        Checked::Run(command, note) => {
            let dir = std::env::current_exe().ok().and_then(|exe| app_dir(&exe));
            match dir.map(|dir| start_app(&dir.join(APP), &command)) {
                Some(Ok(())) => {
                    if let Some(note) = note {
                        say(0, &note);
                    }
                    ExitCode::SUCCESS
                }
                Some(Err(e)) => {
                    say(1, &format!("could not start Cerberus: {e}"));
                    ExitCode::FAILURE
                }
                None => {
                    say(1, "could not find the folder this command runs from");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

/// The folder the command really runs from: on macOS it is usually reached through a symlink on PATH.
fn app_dir(exe: &Path) -> Option<std::path::PathBuf> {
    #[cfg(unix)]
    let exe = &std::fs::canonicalize(exe).ok()?;
    exe.parent().map(Path::to_path_buf)
}

/// Starts the app on its own, so closing the shell or pressing Ctrl+C leaves it running.
fn start_app(app: &Path, command: &Command) -> std::io::Result<()> {
    let mut process = Process::new(app);
    if let Command::Open(spec) = command {
        process.arg("open").arg(spec.to_string());
    }
    process
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        process.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        if let Err(e) = keep_own_handles() {
            // Starting the app matters more; at worst a capturing caller waits until it closes.
            eprintln!("cerberus: could not keep this shell's handles from the app: {e}");
        }
    }
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut process, 0);
    process.spawn().map(drop)
}

/// Inherited output pipes would keep a capturing caller (`$x = cerberus open a`) waiting until the app closes.
#[cfg(windows)]
fn keep_own_handles() -> std::io::Result<()> {
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    // SAFETY: GetStdHandle takes no pointers.
    let handles = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE]
        .map(|id| unsafe { GetStdHandle(id) });
    not_inherited(&handles)
}

/// Clears the inherit flag on every real handle, reporting the first that would not change.
#[cfg(windows)]
fn not_inherited(handles: &[windows_sys::Win32::Foundation::HANDLE]) -> std::io::Result<()> {
    use windows_sys::Win32::Foundation::{
        HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation,
    };
    let mut first_error = None;
    for &handle in handles {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            continue;
        }
        // SAFETY: SetHandleInformation only reads the handle; a bad one fails rather than faults.
        if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
            first_error.get_or_insert_with(std::io::Error::last_os_error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn a_handle_the_app_would_inherit_no_longer_is() {
        use windows_sys::Win32::Foundation::{
            CloseHandle, GetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT,
        };
        use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
        use windows_sys::Win32::System::Pipes::CreatePipe;

        let inheritable = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).unwrap(),
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 1,
        };
        let (mut read, mut write): (HANDLE, HANDLE) = (std::ptr::null_mut(), std::ptr::null_mut());
        // SAFETY: out-pointers to locals; both handles are closed at the end.
        assert_ne!(
            unsafe { CreatePipe(&mut read, &mut write, &inheritable, 0) },
            0
        );
        let inherited = |handle: HANDLE| {
            let mut flags = 0;
            // SAFETY: an out-pointer to a local, on a handle this test owns.
            assert_ne!(unsafe { GetHandleInformation(handle, &mut flags) }, 0);
            flags & HANDLE_FLAG_INHERIT != 0
        };
        assert!(inherited(write));

        not_inherited(&[write, std::ptr::null_mut()]).unwrap();

        assert!(!inherited(write));
        // SAFETY: closing the two handles CreatePipe gave this test.
        unsafe {
            CloseHandle(read);
            CloseHandle(write);
        }
    }

    #[test]
    fn the_identifier_matches_the_tauri_config() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();

        assert_eq!(config["identifier"], IDENTIFIER);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_command_finds_the_app_beside_its_target() {
        let dir = crate::accounts::testdir::TestDir::new();
        let bundle = dir.path().join("Cerberus.app/Contents/MacOS");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("cerberus-cli"), "").unwrap();
        let link = dir.path().join("cerberus");
        std::os::unix::fs::symlink(bundle.join("cerberus-cli"), &link).unwrap();

        let found = app_dir(&link).unwrap();

        assert_eq!(found, std::fs::canonicalize(&bundle).unwrap());
    }

    #[test]
    fn exit_codes_pass_through_and_odd_ones_fail() {
        assert_eq!(exit_code(0), ExitCode::SUCCESS);
        assert_eq!(exit_code(2), ExitCode::from(2));
        assert_eq!(exit_code(-1), ExitCode::FAILURE);
    }
}
