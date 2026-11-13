//! The app's side of Find a chat: the accounts to search, the stream of results, and resuming one.

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::accounts::{Accounts, AccountsError};
use crate::blocking::blocking;
use crate::findchat::{Batch, ChatFinder, Source};
use crate::saved::{Saved, TabId};
use crate::sessions::transcript;

#[derive(Debug, thiserror::Error)]
pub enum FindError {
    #[error(transparent)]
    Accounts(#[from] AccountsError),
    #[error("that tab is not saved yet")]
    NoTab,
    // Names neither the chat nor its folder, so no transcript detail reaches an error.
    #[error("that chat is not in this tab's folder on its account")]
    NotThere,
}

impl serde::Serialize for FindError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// Every account's folder, or with no account set up, the default one that tabs then use.
fn sources(accounts: &Accounts) -> Result<Vec<Source>, AccountsError> {
    let profiles = accounts.load()?;
    if profiles.accounts.is_empty() {
        return Ok(vec![Source {
            account: None,
            config_dir: accounts.claude_home(None)?,
        }]);
    }
    profiles
        .accounts
        .iter()
        .map(|a| {
            Ok(Source {
                account: Some(a.id),
                config_dir: accounts.claude_home(Some(a.id))?,
            })
        })
        .collect()
}

/// Searches every account's chats for `query`. Results stream on `results`, and the number returned names this search.
#[tauri::command]
pub async fn chat_find(
    app: AppHandle,
    query: String,
    results: Channel<Batch>,
) -> Result<u64, FindError> {
    blocking(move || {
        let finder = app.state::<ChatFinder>();
        let accounts = app.state::<Accounts>();
        let saved = app.state::<Saved>();
        let sources = sources(&accounts)?;
        Ok(
            finder.search(sources, &query, saved.open_sessions(), move |batch| {
                // Fails only once the dialog is gone, and its close stops the search.
                let _ = results.send(batch);
            }),
        )
    })
    .await
}

/// The dialog closed: the search stops and what it read leaves memory.
#[tauri::command(async)]
pub fn chat_find_close(finder: State<'_, ChatFinder>) {
    finder.close();
}

/// Gives a new, not yet started tab a found chat to resume, once its transcript is where the tab will look.
#[tauri::command]
pub async fn chat_resume(app: AppHandle, tab: TabId, session: String) -> Result<(), FindError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        let saved = app.state::<Saved>();
        let saved_tab = saved.tab(tab).ok_or(FindError::NoTab)?;
        let home = accounts.chat_home(saved_tab.account)?;
        let cwd = saved_tab
            .cwd
            .unwrap_or_else(|| accounts.home().to_string_lossy().into_owned());
        transcript(&home, &cwd, &session).ok_or(FindError::NotThere)?;
        saved.set_session(tab, &session);
        Ok(())
    })
    .await
}
