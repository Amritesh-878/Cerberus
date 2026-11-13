use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody, IpcResponse};
use tauri::{AppHandle, Manager, State};

use crate::accounts::{
    AccountId, Accounts, AccountsError, AuthState, AuthStatus, Candidate, PanelPrefs, Profiles,
    Removal, SetupStep,
};
use crate::blocking::blocking;
use crate::bridge::Bridge;
use crate::bridge::spawn::Messages;
use crate::cli_link::{self, LinkError, Linked};
use crate::handover::{ContinueError, RunningTabs, continue_chat, move_to_folder, reopen_chat};
use crate::live::Live;
use crate::programs::{Finder, ProgramChoice, pick_shell};
use crate::pty::{Exit, PtyError, PtyManager, SessionId, SpawnRequest};
use crate::saved::{EnvEdit, EnvError, Loaded, Saved, SavedTree, TabId, TreeError};
use crate::sessions::procs::Probes;
use crate::sessions::shell::{HandChat, Keeper, Report, Shell, watch_shell};
use crate::sessions::{
    Resume, TabState, Watched, add_dir_args, chat_ended, is_claude, plan_resume, watch,
};
use crate::suggestions::Suggestions;

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
pub async fn list_programs() -> Vec<ProgramChoice> {
    blocking(move || Finder::from_env().choices()).await
}

/// Output arrives on `events` as raw bytes, a Claude tab's state as `{ state }`, and the exit as `{ exitCode, ended }`.
#[tauri::command]
pub async fn pty_spawn(
    app: AppHandle,
    mut request: SpawnRequest,
    account: Option<AccountId>,
    tab: Option<TabId>,
    events: Channel<InvokeResponseBody>,
) -> Result<Spawned, SpawnError> {
    blocking(move || {
        let manager = app.state::<PtyManager>();
        let accounts = app.state::<Accounts>();
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
            (request.args, resume) =
                plan_resume(&request.args, home, &cwd, session.as_deref(), ended);
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
        // A shell is watched on the tab's own account only, for a `claude` the user starts in it.
        let shell_home = (!runs_claude)
            .then(|| accounts.claude_home(account).ok())
            .flatten();
        if runs_claude {
            request.args = add_dir_args(&request.args, &request.dirs, &cwd);
        }
        if let Some(tab) = tab {
            request.tab_env = saved.env_of(tab);
        }
        // An unreadable profiles file is reported elsewhere; the shell still opens, as it did before.
        let suggest = accounts.load().is_ok_and(|p| p.suggestions);
        if let Some(setup) = app.state::<Suggestions>().for_tab(
            suggest,
            runs_claude,
            &request.program,
            &request.args,
        ) {
            (request.args, request.env) = (setup.args, setup.env);
        }
        let bridge = Bridge::clone(&app.state::<Bridge>());
        let token = claude_home
            .as_deref()
            .zip(tab)
            .and_then(|(home, tab)| link_messages(&app, &accounts, &mut request, home, tab));
        let live = Live::clone(&app.state::<Live>());
        let live_token =
            claude_home
                .as_deref()
                .zip(tab)
                .zip(account)
                .and_then(|((home, tab), account)| {
                    link_live(&live, &accounts, &mut request, (tab, account), (home, &cwd))
                });
        let (exit_live, exit_live_token) = (live.clone(), live_token.clone());
        let (output_bridge, output_token) = (bridge.clone(), token.clone());
        let (exit_bridge, exit_token) = (bridge.clone(), token.clone());
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
                if let Some(token) = &output_token {
                    output_bridge.output(token, &bytes);
                }
                // A send fails only once the webview is gone, and then nobody is left to show it.
                let _ = events.send(InvokeResponseBody::Raw(bytes));
            },
            move |exit: Exit| {
                drop(stop);
                if let Some(token) = &exit_token {
                    exit_bridge.closed(token);
                }
                if let Some(token) = &exit_live_token {
                    exit_live.closed(token);
                }
                // Only a flag that was kept makes the tab a shell; otherwise its next start would run `claude` again.
                let ended = chat_ended(runs_claude, exit)
                    && tab.is_some_and(|tab| ended_saved.set_ended(tab, true));
                if let Some(tab) = tab {
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
                if let Some(token) = &token {
                    bridge.closed(token);
                }
                if let Some(token) = &live_token {
                    live.closed(token);
                }
                return Err(e.into());
            }
        };
        if let Some(token) = &token {
            bridge.attach(token, id);
        }
        let pid = manager.pid(id);
        if let (Some(tab), Some(pid)) = (tab, pid) {
            running.spawned(tab, pid, started_ms);
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
                if let Some(token) = &token {
                    bridge.working(token, state == TabState::Working);
                }
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
        } else if let (Some(config_dir), Some(pid), Some(tab)) = (shell_home, pid, tab) {
            let shell = Shell {
                config_dir,
                pid,
                cwd,
                started_ms,
                probes: Probes::SYSTEM,
            };
            let report = TabReport {
                keeper: Keeper {
                    saved: Saved::clone(&saved),
                    tab,
                },
                events: state_events,
                app: app.clone(),
                id,
            };
            // Without a watcher the shell still works; a `claude` started in it is just not followed.
            match watch_shell(shell, stopped, report) {
                Ok(watcher) => running.watching(tab, watcher),
                Err(e) => eprintln!("could not watch shell tab {tab}: {e}"),
            }
        }
        Ok(Spawned { id, resume })
    })
    .await
}

/// Sends a `claude` started in a shell tab to the tab as it starts, works and ends, and keeps its chat in the saved tab.
struct TabReport {
    keeper: Keeper,
    events: Channel<InvokeResponseBody>,
    app: AppHandle,
    id: SessionId,
}

impl Report for TabReport {
    fn chat(&mut self, chat: &HandChat) {
        self.keeper.chat(chat);
        if let Ok(body) = chat.clone().body() {
            let _ = self.events.send(body);
        }
    }

    fn session(&mut self, id: &str) {
        self.keeper.session(id);
    }

    fn state(&mut self, state: TabState) {
        if let Ok(body) = state.body() {
            let _ = self.events.send(body);
        }
    }

    fn open(&self) -> bool {
        self.app.state::<PtyManager>().is_open(self.id)
    }
}

/// With messages on, links the tab to the bridge and adds the `cerberus` MCP server; off leaves the spawn as it was.
fn link_messages(
    app: &AppHandle,
    accounts: &Accounts,
    request: &mut SpawnRequest,
    home: &std::path::Path,
    tab: TabId,
) -> Option<String> {
    let messages = app.state::<Messages>();
    // An unreadable profiles file is reported elsewhere; the tab starts without messages.
    if !messages.ready() || !accounts.load().is_ok_and(|p| p.messages) {
        return None;
    }
    let ticket = app.state::<Bridge>().register(tab, home)?;
    request.args = messages.args(&request.args);
    request.env.extend(ticket.env);
    Some(ticket.token)
}

/// With live cards on, gives the tab its status line settings and token; off leaves the spawn as it was.
fn link_live(
    live: &Live,
    accounts: &Accounts,
    request: &mut SpawnRequest,
    (tab, account): (TabId, AccountId),
    (home, cwd): (&std::path::Path, &str),
) -> Option<String> {
    // An unreadable profiles file is reported elsewhere; the tab starts without live cards.
    if !accounts.load().is_ok_and(|p| p.live) {
        return None;
    }
    let ticket = live.register(tab, account, std::path::Path::new(cwd), home)?;
    if let Some(arg) = ticket.arg {
        request.args.insert(0, arg);
    }
    request.env.extend(ticket.env);
    Some(ticket.token)
}

/// The tab's variable names; their values never leave the core.
#[tauri::command]
pub async fn tab_env(app: AppHandle, tab: TabId) -> Vec<String> {
    blocking(move || {
        let saved = app.state::<Saved>();
        saved.env_names(tab)
    })
    .await
}

/// Saves the dialog's variable edits for the tab's next start, all or none.
#[tauri::command]
pub async fn tab_set_env(
    app: AppHandle,
    tab: TabId,
    edits: Vec<EnvEdit>,
) -> Result<Vec<String>, EnvError> {
    blocking(move || {
        let saved = app.state::<Saved>();
        saved.set_env(tab, edits)
    })
    .await
}

/// Readies a stopped tab's chat to resume in `folder`; the webview then starts the tab there.
#[tauri::command]
pub async fn tab_move_folder(
    app: AppHandle,
    tab: TabId,
    folder: String,
) -> Result<(), ContinueError> {
    blocking(move || {
        move_to_folder(
            &app.state::<Saved>(),
            &app.state::<Accounts>(),
            &app.state::<Arc<RunningTabs>>(),
            tab,
            &folder,
        )
    })
    .await
}

/// Lets an ended chat's tab run `claude --resume` again the next time it starts.
#[tauri::command]
pub async fn resume_chat(app: AppHandle, tab: TabId) {
    blocking(move || {
        let saved = app.state::<Saved>();
        saved.set_ended(tab, false);
    })
    .await
}

/// Waits for a stopped tab's shell and the `claude` started in it to end, so the tab can resume that chat as a Claude tab.
#[tauri::command]
pub async fn reopen_as_claude(app: AppHandle, tab: TabId) -> Result<(), ContinueError> {
    blocking(move || reopen_chat(&app.state::<Saved>(), &app.state::<Arc<RunningTabs>>(), tab))
        .await
}

#[tauri::command]
pub async fn pty_write(app: AppHandle, id: SessionId, data: String) -> Result<(), PtyError> {
    blocking(move || {
        let manager = app.state::<PtyManager>();
        manager.write(id, data.as_bytes())
    })
    .await
}

#[tauri::command]
pub async fn pty_resize(
    app: AppHandle,
    id: SessionId,
    cols: u16,
    rows: u16,
) -> Result<(), PtyError> {
    blocking(move || {
        let manager = app.state::<PtyManager>();
        manager.resize(id, cols, rows)
    })
    .await
}

#[tauri::command(async)]
pub fn pty_ack(manager: State<'_, PtyManager>, id: SessionId, bytes: usize) {
    manager.ack(id, bytes);
}

#[tauri::command]
pub async fn pty_kill(app: AppHandle, id: SessionId) {
    blocking(move || {
        let manager = app.state::<PtyManager>();
        manager.kill(id);
    })
    .await
}

#[tauri::command]
pub async fn load_profiles(app: AppHandle) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.load()
    })
    .await
}

#[tauri::command]
pub async fn reset_profiles(app: AppHandle) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.reset()
    })
    .await
}

#[tauri::command]
pub async fn detect_accounts(app: AppHandle) -> Result<Vec<Candidate>, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.candidates()
    })
    .await
}

#[tauri::command]
pub async fn suggest_config_dir(app: AppHandle, name: String) -> PathBuf {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.suggested_dir(&name)
    })
    .await
}

#[tauri::command]
pub async fn add_account(
    app: AppHandle,
    name: String,
    config_dir: PathBuf,
) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.add(&name, config_dir)
    })
    .await
}

#[tauri::command]
pub async fn rename_account(
    app: AppHandle,
    id: AccountId,
    name: String,
) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.update(|p| p.rename(id, &name))
    })
    .await
}

#[tauri::command]
pub async fn remove_account(
    app: AppHandle,
    id: AccountId,
    confirmed: bool,
) -> Result<Removal, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        let saved = app.state::<Saved>();
        accounts.remove(id, confirmed, |account| saved.tabs_on(account))
    })
    .await
}

#[tauri::command]
pub async fn set_account_color(
    app: AppHandle,
    id: AccountId,
    color: String,
) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.update(|p| p.set_color(id, &color))
    })
    .await
}

#[tauri::command]
pub async fn set_default_account(app: AppHandle, id: AccountId) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.update(|p| p.set_default_account(id))
    })
    .await
}

#[tauri::command]
pub async fn set_start_folder(
    app: AppHandle,
    folder: Option<PathBuf>,
) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.set_start_folder(folder)
    })
    .await
}

#[tauri::command]
pub async fn set_sidebar_width(app: AppHandle, width: u32) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.set_sidebar_width(width)
    })
    .await
}

#[tauri::command]
pub async fn set_sidebar_collapsed(
    app: AppHandle,
    collapsed: bool,
) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.set_sidebar_collapsed(collapsed)
    })
    .await
}

#[tauri::command]
pub async fn set_split_share(app: AppHandle, share: u32) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.set_split_share(share)
    })
    .await
}

#[tauri::command]
pub async fn set_panel(app: AppHandle, panel: PanelPrefs) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.set_panel(panel)
    })
    .await
}

#[tauri::command]
pub async fn set_setup_step(
    app: AppHandle,
    step: Option<SetupStep>,
) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.set_setup(step)
    })
    .await
}

#[tauri::command]
pub async fn set_alerts(app: AppHandle, on: bool) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.set_alerts(on)
    })
    .await
}

#[tauri::command]
pub async fn set_suggestions(app: AppHandle, on: bool) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.set_suggestions(on)
    })
    .await
}

#[tauri::command]
pub async fn set_shell(app: AppHandle, shell: String) -> Result<Profiles, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.set_shell(shell)
    })
    .await
}

#[tauri::command]
pub async fn account_status(app: AppHandle, id: AccountId) -> Result<AuthStatus, AccountsError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        accounts.status(id)
    })
    .await
}

#[tauri::command]
pub async fn tree_load(app: AppHandle) -> Result<Loaded, TreeError> {
    blocking(move || {
        let saved = app.state::<Saved>();
        saved.load()
    })
    .await
}

#[tauri::command]
pub async fn tree_save(app: AppHandle, tree: SavedTree) -> Result<(), TreeError> {
    blocking(move || {
        let saved = app.state::<Saved>();
        saved.save(tree)
    })
    .await
}

#[tauri::command]
pub async fn continue_on_account(
    app: AppHandle,
    tab: TabId,
    account: AccountId,
) -> Result<(), ContinueError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        let signed_in = |id| Ok(accounts.status(id)?.state == AuthState::LoggedIn);
        continue_chat(
            &app.state::<Saved>(),
            &accounts,
            &app.state::<Arc<RunningTabs>>(),
            signed_in,
            tab,
            account,
        )
    })
    .await
}

#[tauri::command]
pub async fn install_cli(app: AppHandle) -> Result<Linked, LinkError> {
    blocking(move || cli_link::install(app.path().home_dir().ok())).await
}
