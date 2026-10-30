use std::sync::{Mutex, MutexGuard, PoisonError};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::open::{
    BAD_USAGE, Checked, Command, MISSED, Spec, Target, USAGE, check, opening, parse, pipe, resolve,
};
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

/// Answers `cerberus open` before any window, and exits unless this process should become the app.
pub fn from_command_line(identifier: &str) -> Command {
    let args = arguments(std::env::args_os().map(|a| a.to_string_lossy().into_owned()));
    let ask = |args: &[String]| pipe::ask(&pipe::name(identifier), args);
    let read = || {
        // Where Tauri's `app_config_dir` points, found before there is an app to ask.
        let dir = dirs::config_dir()
            .ok_or_else(|| TreeError::Unavailable("no app data folder".to_owned()))?;
        saved::read_unlocked(&dir.join(identifier).join(saved::FILE_NAME))
    };
    match check(&args, ask, read) {
        Checked::Run(command, note) => {
            if let Some(note) = note {
                println!("{note}");
            }
            command
        }
        Checked::Exit(0, message) => {
            println!("{message}");
            std::process::exit(0)
        }
        Checked::Exit(code, message) => {
            eprintln!("{message}");
            std::process::exit(code)
        }
    }
}

/// Points the page at the tab `spec` names in the live tree, and says what it opened or why nothing matched.
fn show(app: &AppHandle, spec: &Spec) -> Result<String, String> {
    let saved = app
        .try_state::<Saved>()
        .ok_or("the saved tabs are not loaded yet")?;
    let loaded = saved.load().map_err(|e| e.to_string())?;
    let found = resolve(&loaded.tree, spec).map_err(|miss| miss.to_string())?;
    if let Some(pending) = app.try_state::<Pending>() {
        *pending.slot() = Some(found.target());
    }
    // Fails only once the window is gone, and then nobody is left to show the tab.
    let _ = app.emit("open-request", ());
    Ok(opening(&found))
}

fn focus(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    // Each fails only while the window is closing, and then there is nothing to bring forward.
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

/// Shows a request this process was started with, once the saved tree is loaded.
pub fn request(app: &AppHandle, spec: &Spec) {
    // The command line already checked the name, and a miss now has nobody to tell.
    let _ = show(app, spec);
}

/// Runs in the first app for each later launch, which exits once this returns.
pub fn on_second_launch(app: &AppHandle, args: Vec<String>, _cwd: String) {
    if let Some(Command::Open(spec)) = parse(&arguments(args.into_iter())) {
        let _ = show(app, &spec);
    }
    focus(app);
}

fn answer(app: &AppHandle, args: &[String]) -> (i32, String) {
    let Some(Command::Open(spec)) = parse(args) else {
        return (BAD_USAGE, USAGE.to_owned());
    };
    match show(app, &spec) {
        Ok(opened) => {
            focus(app);
            (0, opened)
        }
        Err(why) => (MISSED, why),
    }
}

/// Lets `cerberus open` resolve against the live tree, which can be up to a save delay ahead of the file.
pub fn listen(app: &AppHandle) {
    let handle = app.clone();
    let name = pipe::name(&app.config().identifier);
    if let Err(e) = pipe::serve(&name, move |args| answer(&handle, args)) {
        eprintln!("cerberus open will read the saved file instead of this window: {e}");
    }
}

#[tauri::command]
pub fn open_take(pending: State<'_, Pending>) -> Option<Target> {
    pending.slot().take()
}
