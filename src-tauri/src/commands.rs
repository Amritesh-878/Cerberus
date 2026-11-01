use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody, IpcResponse};
use tauri::{AppHandle, Manager, State};

use crate::accounts::{
    AccountId, Accounts, AccountsError, AuthState, AuthStatus, Candidate, Profiles,
};
use crate::handover::{ContinueError, RunningTabs, continue_chat};
use crate::programs::{Finder, ProgramChoice, pick_shell};
use crate::pty::{Exit, PtyError, PtyManager, SessionId, SpawnRequest};
use crate::saved::{Loaded, Saved, SavedTree, TabId, TreeError};
use crate::sessions::{Resume, TabState, Watched, chat_ended, is_claude, plan_resume, watch};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Exited {
    exit_code: Option<u32>,
    /// A Claude tab's chat ended by itself, not by Cerberus stopping it, so the tab becomes a shell.
    ended: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spawned {
    id: SessionId,
    resume: Resume,
}

#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error(transparent)]
    Accounts(#[from] AccountsError),
    #[error(transparent)]
    Pty(#[from] PtyError),
    #[error("the chat ended, and no shell is installed to open in its place")]
    NoShell,
}

impl Serialize for SpawnError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[tauri::command]
pub fn list_programs() -> Vec<ProgramChoice> {
    Finder::from_env().choices()
}

/// Output arrives on `events` as raw bytes, a Claude tab's state as `{ state }`, and the exit as `{ exitCode, ended }`.
#[tauri::command(async)]
pub fn pty_spawn(
    app: AppHandle,
    manager: State<'_, PtyManager>,
    accounts: State<'_, Accounts>,
    mut request: SpawnRequest,
    account: Option<AccountId>,
    tab: Option<TabId>,
    events: Channel<InvokeResponseBody>,
) -> Result<Spawned, SpawnError> {
    request.claude_config_dir = accounts.claude_config_dir(account)?;
    let mut claude_home = is_claude(&request.program)
        .then(|| accounts.claude_home(account))
        .transpose()?;
    let cwd = request
        .cwd
        .clone()
        .unwrap_or_else(|| accounts.home().to_string_lossy().into_owned());
    let saved = app.state::<Saved>();
    let mut resume = Resume::New;
    if let Some(home) = &claude_home {
        let session = tab.and_then(|t| saved.session_of(t));
        let ended = tab.is_some_and(|t| saved.is_ended(t));
        (request.args, resume) = plan_resume(&request.args, home, &cwd, session.as_deref(), ended);
    }
    if resume == Resume::Ended {
        let shell = pick_shell(
            Finder::from_env().choices(),
            accounts.load()?.shell.as_deref(),
        );
        let (program, args) = shell
            .and_then(|s| Some((s.program?, s.args)))
            .ok_or(SpawnError::NoShell)?;
        (request.program, request.args) = (program, args);
        claude_home = None;
    }
    let runs_claude = claude_home.is_some();
    let ended_saved = Saved::clone(&saved);
    let started_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_millis()).ok())
        .unwrap_or(0);
    // Dropped when the program exits, which ends the session-file watcher.
    let (stop, stopped) = mpsc::channel::<()>();
    let exit_events = events.clone();
    let state_events = events.clone();
    let running = Arc::clone(&app.state::<Arc<RunningTabs>>());
    let exit_running = Arc::clone(&running);
    if let Some(tab) = tab {
        running.started(tab);
    }
    let spawned = manager.spawn(
        &request,
        move |bytes| {
            // A send fails only once the webview is gone, and then nobody is left to show it.
            let _ = events.send(InvokeResponseBody::Raw(bytes));
        },
        move |exit: Exit| {
            drop(stop);
            let ended = chat_ended(runs_claude, exit) && tab.is_some();
            if let Some(tab) = tab {
                if ended {
                    ended_saved.set_ended(tab, true);
                }
                exit_running.ended(tab);
            }
            let exited = Exited {
                exit_code: exit.code,
                ended,
            };
            if let Ok(body) = exited.body() {
                let _ = exit_events.send(body);
            }
        },
    );
    let id = match spawned {
        Ok(id) => id,
        Err(e) => {
            if let Some(tab) = tab {
                running.ended(tab);
            }
            return Err(e.into());
        }
    };
    let pid = manager.pid(id);
    if let (Some(tab), Some(pid)) = (tab, pid) {
        running.spawned(tab, pid);
    }
    if let (Some(config_dir), Some(pid)) = (claude_home, pid) {
        let watched = Watched {
            config_dir,
            pid,
            cwd,
            started_ms,
        };
        let saved = Saved::clone(&saved);
        let on_session = move |session: &str| {
            if let Some(tab) = tab {
                saved.set_session(tab, session);
            }
        };
        let on_state = move |state: TabState| {
            if let Ok(body) = state.body() {
                let _ = state_events.send(body);
            }
        };
        // Without a watcher the tab still works; it only loses its board mark and resume.
        match watch(watched, stopped, on_session, on_state) {
            Ok(watcher) => {
                if let Some(tab) = tab {
                    running.watching(tab, watcher);
                }
            }
            Err(e) => eprintln!("could not watch tab {tab:?}: {e}"),
        }
    }
    Ok(Spawned { id, resume })
}

/// Lets an ended chat's tab run `claude --resume` again the next time it starts.
#[tauri::command(async)]
pub fn resume_chat(saved: State<'_, Saved>, tab: TabId) {
    saved.set_ended(tab, false);
}

#[tauri::command(async)]
pub fn pty_write(
    manager: State<'_, PtyManager>,
    id: SessionId,
    data: String,
) -> Result<(), PtyError> {
    manager.write(id, data.as_bytes())
}

#[tauri::command(async)]
pub fn pty_resize(
    manager: State<'_, PtyManager>,
    id: SessionId,
    cols: u16,
    rows: u16,
) -> Result<(), PtyError> {
    manager.resize(id, cols, rows)
}

#[tauri::command(async)]
pub fn pty_ack(manager: State<'_, PtyManager>, id: SessionId, bytes: usize) {
    manager.ack(id, bytes);
}

#[tauri::command(async)]
pub fn pty_kill(manager: State<'_, PtyManager>, id: SessionId) {
    manager.kill(id);
}

#[tauri::command(async)]
pub fn load_profiles(accounts: State<'_, Accounts>) -> Result<Profiles, AccountsError> {
    accounts.load()
}

#[tauri::command(async)]
pub fn reset_profiles(accounts: State<'_, Accounts>) -> Result<Profiles, AccountsError> {
    accounts.reset()
}

#[tauri::command(async)]
pub fn detect_accounts(accounts: State<'_, Accounts>) -> Result<Vec<Candidate>, AccountsError> {
    accounts.candidates()
}

#[tauri::command(async)]
pub fn suggest_config_dir(accounts: State<'_, Accounts>, name: String) -> PathBuf {
    accounts.suggested_dir(&name)
}

#[tauri::command(async)]
pub fn add_account(
    accounts: State<'_, Accounts>,
    name: String,
    config_dir: PathBuf,
) -> Result<Profiles, AccountsError> {
    accounts.add(&name, config_dir)
}

#[tauri::command(async)]
pub fn rename_account(
    accounts: State<'_, Accounts>,
    id: AccountId,
    name: String,
) -> Result<Profiles, AccountsError> {
    accounts.update(|p| p.rename(id, &name))
}

#[tauri::command(async)]
pub fn remove_account(
    accounts: State<'_, Accounts>,
    id: AccountId,
) -> Result<Profiles, AccountsError> {
    accounts.update(|p| p.remove(id))
}

#[tauri::command(async)]
pub fn set_default_account(
    accounts: State<'_, Accounts>,
    id: AccountId,
) -> Result<Profiles, AccountsError> {
    accounts.update(|p| p.set_default_account(id))
}

#[tauri::command(async)]
pub fn set_start_folder(
    accounts: State<'_, Accounts>,
    folder: Option<PathBuf>,
) -> Result<Profiles, AccountsError> {
    accounts.set_start_folder(folder)
}

#[tauri::command(async)]
pub fn set_sidebar_width(
    accounts: State<'_, Accounts>,
    width: u32,
) -> Result<Profiles, AccountsError> {
    accounts.set_sidebar_width(width)
}

#[tauri::command(async)]
pub fn set_shell(accounts: State<'_, Accounts>, shell: String) -> Result<Profiles, AccountsError> {
    accounts.set_shell(shell)
}

#[tauri::command(async)]
pub fn account_status(
    accounts: State<'_, Accounts>,
    id: AccountId,
) -> Result<AuthStatus, AccountsError> {
    accounts.status(id)
}

#[tauri::command(async)]
pub fn tree_load(saved: State<'_, Saved>) -> Result<Loaded, TreeError> {
    saved.load()
}

#[tauri::command(async)]
pub fn tree_save(saved: State<'_, Saved>, tree: SavedTree) -> Result<(), TreeError> {
    saved.save(tree)
}

#[tauri::command(async)]
pub fn continue_on_account(
    accounts: State<'_, Accounts>,
    saved: State<'_, Saved>,
    running: State<'_, Arc<RunningTabs>>,
    tab: TabId,
    account: AccountId,
) -> Result<(), ContinueError> {
    let signed_in = |id| Ok(accounts.status(id)?.state == AuthState::LoggedIn);
    continue_chat(&saved, &accounts, &running, signed_in, tab, account)
}
