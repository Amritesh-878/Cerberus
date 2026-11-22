use std::sync::{Mutex, MutexGuard, PoisonError};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::accounts::Accounts;
use crate::findchat;
use crate::open::shell::{arguments, checked, own_arguments, say};
use crate::open::{
    BAD_USAGE, Checked, Command, MISSED, Spec, Target, opening, parse, pipe, resolve, usage_for,
};
use crate::saved::{Saved, TabId};
use crate::wrapper;

/// Asks the page to open a chat found on another account, as Find chat would.
const CHAT_ELSEWHERE: &str = "chat-elsewhere";

/// The last tab asked for, held until the page takes it, so a request made while it loads is kept.
#[derive(Default)]
pub struct Pending(Mutex<Option<Target>>);

impl Pending {
    fn slot(&self) -> MutexGuard<'_, Option<Target>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Answers `cerberus open` before any window, and exits unless this process should become the app.
pub fn from_command_line(identifier: &str) -> Command {
    match checked(identifier, &own_arguments()) {
        Checked::Run(command, note) => {
            if let Some(note) = note {
                say(0, &note);
            }
            command
        }
        Checked::Exit(code, message) => {
            say(code, &message);
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
    if let Some(answer) = wrapper_answer(app, args) {
        return answer;
    }
    let Some(Command::Open(spec)) = parse(args) else {
        let tree = || Some(app.try_state::<Saved>()?.load().ok()?.tree);
        return (BAD_USAGE, usage_for(args, tree));
    };
    match show(app, &spec) {
        Ok(opened) => {
            focus(app);
            (0, opened)
        }
        Err(why) => (MISSED, why),
    }
}

/// A shell tab's `claude` wrapper: notes that it started, or opens a chat its account doesn't have; `None` for anything else.
fn wrapper_answer(app: &AppHandle, args: &[String]) -> Option<(i32, String)> {
    let [kind, tab, value] = args else {
        return None;
    };
    let tab: TabId = tab.parse().ok()?;
    if kind == wrapper::STARTED {
        app.state::<wrapper::Wrapped>()
            .started(tab, value.parse().ok()?);
        return Some((0, String::new()));
    }
    if kind != wrapper::RESUME {
        return None;
    }
    // Not found anywhere: the wrapper runs `claude` itself, which says so.
    let Some(found) = findchat::app::owner_of(app, value) else {
        return Some((MISSED, String::new()));
    };
    let name = found
        .account
        .and_then(|id| {
            app.state::<Accounts>()
                .load()
                .ok()?
                .accounts
                .into_iter()
                .find(|a| a.id == id)
        })
        .map(|a| a.name);
    let said = match (found.open_in, name) {
        (Some(_), _) => "It's open in another tab. Switched to it.".to_owned(),
        (None, Some(name)) => format!("It's on {name}. Opened it in a new tab."),
        (None, None) => "Opened it in a new tab.".to_owned(),
    };
    if let Err(e) = app.emit(CHAT_ELSEWHERE, found) {
        return Some((MISSED, format!("could not open the chat: {e}")));
    }
    focus(app);
    Some((0, said))
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
