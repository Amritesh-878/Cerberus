//! The app's side of the live cards: accounts and threshold in, views and warnings out.

use tauri::{AppHandle, Emitter, Manager, State};

use crate::accounts::{AccountId, Accounts, AccountsError, KeptUsage, Profiles};
use crate::blocking::blocking;
use crate::live::files::FileView;
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

    fn restored(&self) -> Vec<KeptUsage> {
        self.profiles().map(|p| p.usage).unwrap_or_default()
    }

    fn keep(&self, usage: Vec<KeptUsage>) {
        // Losing a save only costs the numbers shown after the next restart.
        if let Err(e) = self.0.state::<Accounts>().keep_usage(usage) {
            eprintln!("could not keep the usage numbers: {e}");
        }
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
#[tauri::command]
pub async fn live_open_output(app: AppHandle, tab: TabId, path: String) -> Result<(), String> {
    blocking(move || {
        let live = app.state::<Live>();
        live.open_output(tab, &path).map_err(|e| e.to_string())
    })
    .await
}

/// A written file and its `HEAD` version for the file pane; any path the chat didn't write is refused.
#[tauri::command]
pub async fn live_read_output(
    app: AppHandle,
    tab: TabId,
    path: String,
) -> Result<FileView, String> {
    blocking(move || {
        let live = app.state::<Live>();
        let git = crate::programs::Finder::from_env()
            .find("git")
            .map(crate::panel::git::Git::new);
        let head = |file: &std::path::Path| {
            git.as_ref()
                .and_then(|g| g.show_head(file, crate::live::files::MOST_FILE))
        };
        live.read_output(tab, &path, head)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn live_output_stamp(app: AppHandle, tab: TabId, path: String) -> Option<String> {
    blocking(move || {
        let live = app.state::<Live>();
        live.output_stamp(tab, &path)
    })
    .await
}

/// Applies to Claude tabs started after the change.
#[tauri::command]
pub async fn set_live(app: AppHandle, on: bool) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.set_live(on)
    })
    .await
}

#[tauri::command]
pub async fn set_warn_at(app: AppHandle, percent: u8) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        let live = app.state::<Live>();
        let profiles = accounts.set_warn_at(percent)?;
        live.refresh();
        Ok(profiles)
    })
    .await
}
