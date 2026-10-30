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
use crate::programs::{Finder, ProgramChoice};
use crate::pty::{PtyError, PtyManager, SessionId, SpawnRequest};
use crate::saved::{Loaded, Saved, SavedTree, TabId, TreeError};
use crate::sessions::{Resume, TabState, Watched, is_claude, plan_resume, watch};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Exited {
    exit_code: Option<u32>,
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

/// Output arrives on `events` as raw bytes, a Claude tab's state as `{ state }`, and the exit as `{ exitCode }`.
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
    let claude_home = is_claude(&request.program)
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
        (request.args, resume) = plan_resume(&request.args, home, &cwd, session.as_deref());
    }
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
    if let Some(tab) = tab {
        running.started(tab);
    }
    let spawned = manager.spawn(
        &request,
        move |bytes| {
            // A send fails only once the webview is gone, and then nobody is left to show it.
            let _ = events.send(InvokeResponseBody::Raw(bytes));
        },
        move |exit_code| {
            drop(stop);
            if let Some(tab) = tab {
                running.ended(tab);
            }
            if let Ok(body) = (Exited { exit_code }).body() {
                let _ = exit_events.send(body);
            }
        },
    );
    let id = match spawned {
        Ok(id) => id,
        Err(e) => {
            if let Some(tab) = tab {
                app.state::<Arc<RunningTabs>>().ended(tab);
            }
            return Err(e.into());
        }
    };
    if let (Some(config_dir), Some(pid)) = (claude_home, manager.pid(id)) {
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
        if let Err(e) = watch(watched, stopped, on_session, on_state) {
            eprintln!("could not watch tab {tab:?}: {e}");
        }
    }
    Ok(Spawned { id, resume })
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

/// Copies the tab's chat to `account`; on success the webview reopens the tab there, which resumes it.
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
