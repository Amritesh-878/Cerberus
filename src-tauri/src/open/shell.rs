use std::path::Path;
use std::process::{Command as Process, ExitCode, Stdio};

use crate::open::{Checked, Command, check, pipe};
use crate::saved::{self, TreeError};

/// The bundle identifier, which names the app's data folder and its pipe; a test ties it to `tauri.conf.json`.
pub const IDENTIFIER: &str = "com.cerberus.terminal";

/// The app beside the console command, as the installer lays them out.
const APP: &str = "cerberus.exe";

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
            let dir = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(Path::to_path_buf));
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
    }
    process.spawn().map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identifier_matches_the_tauri_config() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();

        assert_eq!(config["identifier"], IDENTIFIER);
    }

    #[test]
    fn exit_codes_pass_through_and_odd_ones_fail() {
        assert_eq!(exit_code(0), ExitCode::SUCCESS);
        assert_eq!(exit_code(2), ExitCode::from(2));
        assert_eq!(exit_code(-1), ExitCode::FAILURE);
    }
}
