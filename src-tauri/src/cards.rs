//! The app's side of the live cards: accounts and threshold in, views and warnings out.

use tauri::{AppHandle, Emitter, Manager, State};

use crate::accounts::{AccountId, Accounts, AccountsError, Profiles};
use crate::live::{DEFAULT_WARN_AT, Live, Sink, View, Warning};
use crate::saved::TabId;

pub struct AppSink(pub AppHandle);

impl AppSink {
    fn profiles(&self) -> Option<Profiles> {
        self.0.state::<Accounts>().load().ok()
    }
}

impl Sink for AppSink {
    fn accounts(&self) -> Vec<AccountId> {
        self.profiles()
            .map(|p| p.accounts.iter().map(|a| a.id).collect())
            .unwrap_or_default()
    }

    fn warn_at(&self) -> u8 {
        self.profiles().map_or(DEFAULT_WARN_AT, |p| p.warn_at)
    }

    fn changed(&self, view: View) {
        // Fails only once the window is gone, and then nobody is left to show it.
        let _ = self.0.emit("live", view);
    }

    fn warned(&self, warning: Warning) {
        let _ = self.0.emit("live-warning", warning);
    }
}

#[tauri::command(async)]
pub fn live_view(live: State<'_, Live>) -> View {
    live.view()
}

/// Hides an account's warning until its next 5-hour window.
#[tauri::command(async)]
pub fn live_dismiss(live: State<'_, Live>, account: AccountId) {
    live.dismiss(account);
}

/// Opens a file the tab's chat wrote; any other path is refused.
#[tauri::command(async)]
pub fn live_open_output(live: State<'_, Live>, tab: TabId, path: String) -> Result<(), String> {
    live.open_output(tab, &path).map_err(|e| e.to_string())
}

/// Applies to Claude tabs started after the change.
#[tauri::command(async)]
pub fn set_live(accounts: State<'_, Accounts>, on: bool) -> Result<Profiles, AccountsError> {
    accounts.set_live(on)
}

#[tauri::command(async)]
pub fn set_warn_at(
    accounts: State<'_, Accounts>,
    live: State<'_, Live>,
    percent: u8,
) -> Result<Profiles, AccountsError> {
    let profiles = accounts.set_warn_at(percent)?;
    live.refresh();
    Ok(profiles)
}
