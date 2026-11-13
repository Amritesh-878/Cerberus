//! The app's side of messages between accounts: naming tabs, telling the webview, and its commands.

use std::io;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::accounts::{Accounts, AccountsError, Profiles};
use crate::blocking::blocking;
use crate::bridge::{Bridge, BridgeError, Decided, Decision, Hooks, Label, Snapshot};
use crate::pty::PtyManager;
use crate::saved::{Saved, TabId};

pub struct AppHooks(pub AppHandle);

impl Hooks for AppHooks {
    fn label(&self, tab: TabId) -> Label {
        let place = self.0.state::<Saved>().place(tab);
        let account = place
            .as_ref()
            .and_then(|(account, _)| *account)
            .and_then(|id| {
                let profiles = self.0.state::<Accounts>().load().ok()?;
                profiles.account(id).ok().map(|a| a.name.clone())
            })
            .unwrap_or_else(|| "Default account".to_owned());
        Label {
            account,
            place: place.map_or_else(|| format!("tab {tab}"), |(_, place)| place),
        }
    }

    fn changed(&self, snapshot: Snapshot) {
        // Fails only once the window is gone, and then nobody is left to show it.
        let _ = self.0.emit("bridge", snapshot);
    }
}

#[tauri::command(async)]
pub fn bridge_snapshot(bridge: State<'_, Bridge>) -> Snapshot {
    bridge.snapshot()
}

/// Approve, edit or reject a waiting message; an approval returns once it is delivered or typed in.
#[tauri::command]
pub async fn bridge_decide(
    app: AppHandle,
    id: u64,
    decision: Decision,
) -> Result<Decided, BridgeError> {
    blocking(move || {
        let bridge = app.state::<Bridge>();
        let manager = app.state::<PtyManager>();
        bridge.decide(id, decision, |pty, bytes| {
            manager.write(pty, bytes).map_err(io::Error::other)
        })
    })
    .await
}

/// Types an approved message that reached an inbox but never showed into the target's input.
#[tauri::command]
pub async fn bridge_type_in(app: AppHandle, id: u64) -> Result<(), BridgeError> {
    blocking(move || {
        let bridge = app.state::<Bridge>();
        let manager = app.state::<PtyManager>();
        bridge.type_in(id, |pty, bytes| {
            manager.write(pty, bytes).map_err(io::Error::other)
        })
    })
    .await
}

/// The user closed an unconfirmed message's notice, so its kept text is dropped.
#[tauri::command(async)]
pub fn bridge_discard(bridge: State<'_, Bridge>, id: u64) {
    bridge.discard(id);
}

#[tauri::command]
pub async fn set_messages(app: AppHandle, on: bool) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.set_messages(on)
    })
    .await
}
