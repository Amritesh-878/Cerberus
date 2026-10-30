use std::sync::{Mutex, MutexGuard, PoisonError};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::open::{Checked, Command, Spec, Target, check, parse, resolve};
use crate::saved::{self, Saved, TreeError};

/// The last tab asked for, held until the page takes it, so a request made while it loads is kept.
#[derive(Default)]
pub struct Pending(Mutex<Option<Target>>);

impl Pending {
    fn slot(&self) -> MutexGuard<'_, Option<Target>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn arguments(all: impl Iterator<Item = String>) -> Vec<String> {
    all.skip(1).collect()
}

/// Answers `cerberus open` before any window, and exits on a miss, so a shell sees the exit code.
pub fn from_command_line(identifier: &str) -> Command {
    let args = arguments(std::env::args_os().map(|a| a.to_string_lossy().into_owned()));
    let read = || {
        // Where Tauri's `app_config_dir` points, found before there is an app to ask.
        let dir = dirs::config_dir()
            .ok_or_else(|| TreeError::Unavailable("no app data folder".to_owned()))?;
        saved::read_unlocked(&dir.join(identifier).join(saved::FILE_NAME))
    };
    match check(&args, read) {
        Checked::Run(command, note) => {
            if let Some(note) = note {
                println!("{note}");
            }
            command
        }
        Checked::Exit(code, message) => {
            eprintln!("{message}");
            std::process::exit(code)
        }
    }
}

/// Points the page at the tab `spec` names in the live tree; a name that no longer matches is ignored.
pub fn request(app: &AppHandle, spec: &Spec) {
    let Some(Ok(loaded)) = app.try_state::<Saved>().map(|saved| saved.load()) else {
        return;
    };
    let Ok(found) = resolve(&loaded.tree, spec) else {
        return;
    };
    let Some(pending) = app.try_state::<Pending>() else {
        return;
    };
    *pending.slot() = Some(found.target());
    // Fails only once the window is gone, and then nobody is left to show the tab.
    let _ = app.emit("open-request", ());
}

/// Runs in the first app for each later launch, which exits once this returns.
pub fn on_second_launch(app: &AppHandle, args: Vec<String>, _cwd: String) {
    if let Some(Command::Open(spec)) = parse(&arguments(args.into_iter())) {
        request(app, &spec);
    }
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    // Each fails only while the window is closing, and then there is nothing to bring forward.
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

#[tauri::command]
pub fn open_take(pending: State<'_, Pending>) -> Option<Target> {
    pending.slot().take()
}
