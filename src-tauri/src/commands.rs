use std::path::PathBuf;

use serde::Serialize;
use tauri::State;
use tauri::ipc::{Channel, InvokeResponseBody, IpcResponse};

use crate::accounts::{
    AccountId, Accounts, AccountsError, AuthStatus, Candidate, Defaults, Profiles,
};
use crate::programs::{Finder, ProgramChoice};
use crate::pty::{PtyError, PtyManager, SessionId, SpawnRequest};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Exited {
    exit_code: Option<u32>,
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

/// Output arrives on `events` as raw bytes and the exit as `{ exitCode }`, in order on one channel.
#[tauri::command(async)]
pub fn pty_spawn(
    manager: State<'_, PtyManager>,
    accounts: State<'_, Accounts>,
    mut request: SpawnRequest,
    account: Option<AccountId>,
    events: Channel<InvokeResponseBody>,
) -> Result<SessionId, SpawnError> {
    request.claude_config_dir = accounts.claude_config_dir(account)?;
    let exit_events = events.clone();
    let id = manager.spawn(
        &request,
        move |bytes| {
            // A send fails only once the webview is gone, and then nobody is left to show it.
            let _ = events.send(InvokeResponseBody::Raw(bytes));
        },
        move |exit_code| {
            if let Ok(body) = (Exited { exit_code }).body() {
                let _ = exit_events.send(body);
            }
        },
    )?;
    Ok(id)
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
pub fn set_defaults(
    accounts: State<'_, Accounts>,
    defaults: Defaults,
) -> Result<Profiles, AccountsError> {
    accounts.set_defaults(defaults)
}

#[tauri::command(async)]
pub fn account_status(
    accounts: State<'_, Accounts>,
    id: AccountId,
) -> Result<AuthStatus, AccountsError> {
    accounts.status(id)
}
