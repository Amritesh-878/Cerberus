use serde::Serialize;
use tauri::State;
use tauri::ipc::{Channel, InvokeResponseBody, IpcResponse};

use crate::programs::{Finder, ProgramChoice};
use crate::pty::{PtyError, PtyManager, SessionId, SpawnRequest};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Exited {
    exit_code: Option<u32>,
}

#[tauri::command]
pub fn list_programs() -> Vec<ProgramChoice> {
    Finder::from_env().choices()
}

/// Output arrives on `events` as raw bytes and the exit as `{ exitCode }`, in order on one channel.
#[tauri::command(async)]
pub fn pty_spawn(
    manager: State<'_, PtyManager>,
    request: SpawnRequest,
    events: Channel<InvokeResponseBody>,
) -> Result<SessionId, PtyError> {
    let exit_events = events.clone();
    manager.spawn(
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
    )
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
