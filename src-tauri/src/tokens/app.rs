//! The app's side of token counts: which accounts to read, and the command the Usage card calls.

use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};

use crate::accounts::{Accounts, AccountsError};
use crate::blocking::blocking;
use crate::tokens::{AccountTokens, Ledger};

/// The ledger behind one lock, so two cards asking at once read the chat files once.
pub struct TokenLedger(pub Mutex<Ledger>);

/// Today, 7 and 30 days of tokens and an API price estimate per account and model; `offset_minutes` is the user's offset east of UTC.
#[tauri::command]
pub async fn usage_tokens(
    app: AppHandle,
    offset_minutes: i32,
) -> Result<Vec<AccountTokens>, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        let profiles = accounts.load()?;
        let homes = profiles
            .accounts
            .iter()
            .map(|a| Ok((a.id, accounts.claude_home(Some(a.id))?)))
            .collect::<Result<Vec<_>, AccountsError>>()?;
        // No real offset is past 14 hours; anything else is a bad call, read as UTC.
        let offset = if (-14 * 60..=14 * 60).contains(&offset_minutes) {
            offset_minutes
        } else {
            0
        };
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|d| u64::try_from(d.as_millis()).ok())
            .unwrap_or(0);
        let ledger = app.state::<TokenLedger>();
        let mut ledger = ledger.0.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(ledger.view(&homes, offset, now_ms))
    })
    .await
}
