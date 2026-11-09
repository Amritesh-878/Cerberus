//! Live cards: each Claude tab's status line reports context and limits to the app over a pipe.

pub mod event;
pub mod helper;
mod meters;
mod settings;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::accounts::AccountId;
use crate::bridge::link;
use crate::live::event::{Event, Line, MOST_LINE, PIPE_VAR, Status, TOKEN_VAR};
pub use crate::live::meters::{DEFAULT_WARN_AT, Meter, Warning};
use crate::live::meters::{Meters, Warner};
pub use crate::live::settings::Shell;
use crate::saved::TabId;

// A status line runs a few times a minute per tab; more at once than this is not Cerberus.
const MOST_CONNECTIONS: usize = 64;

/// What the live cards need from the rest of the app.
pub trait Sink: Send + Sync + 'static {
    /// Every account a chat could move to, in the app's order.
    fn accounts(&self) -> Vec<AccountId>;
    fn warn_at(&self) -> u8;
    fn changed(&self, view: View);
    /// Once per account per 5-hour window, for the desktop notification.
    fn warned(&self, warning: Warning);
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabLive {
    pub tab: TabId,
    pub account: AccountId,
    /// Share of the context window in use, 0 to 100; `None` until the first reply.
    pub context: Option<f64>,
    pub model: Option<String>,
    /// His own status line runs where Cerberus cannot wrap it, so this tab reports nothing.
    pub kept_theirs: bool,
}

/// Everything the live cards show, ready to draw.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub tabs: Vec<TabLive>,
    /// Only accounts that reported; any other is unknown.
    pub meters: Vec<Meter>,
    pub warnings: Vec<Warning>,
}

/// What a tab's `claude` is started with so its status line can report.
pub struct Ticket {
    pub token: String,
    pub env: Vec<(&'static str, OsString)>,
    /// `--settings=<file>`, or `None` when his status line has to run alone.
    pub arg: Option<String>,
}

struct Link {
    tab: TabId,
    account: AccountId,
}

#[derive(Default)]
struct State {
    links: HashMap<String, Link>,
    tabs: BTreeMap<TabId, TabLive>,
    meters: Meters,
    warner: Warner,
}

struct Core {
    name: String,
    dir: PathBuf,
    helper: PathBuf,
    shell: Shell,
    sink: Box<dyn Sink>,
    state: Mutex<State>,
}

/// Off when the helper or the pipe is missing; every tab then starts as it did before.
#[derive(Clone, Default)]
pub struct Live {
    core: Option<Arc<Core>>,
}

impl Live {
    pub fn off() -> Self {
        Self::default()
    }

    /// Listens on a new pipe; `dir` holds the per-tab settings files.
    pub fn start(dir: &Path, helper: PathBuf, shell: Shell, sink: impl Sink) -> io::Result<Self> {
        let core = Arc::new(Core {
            name: link::new_name()?,
            dir: dir.join("live"),
            helper,
            shell,
            sink: Box::new(sink),
            state: Mutex::default(),
        });
        listen(&core)?;
        Ok(Self { core: Some(core) })
    }

    /// Readies a tab's `claude`; `None` when live cards are off or its settings file can't be written.
    pub fn register(
        &self,
        tab: TabId,
        account: AccountId,
        cwd: &Path,
        config_dir: &Path,
    ) -> Option<Ticket> {
        let core = self.core.as_ref()?;
        let token = link::random_hex(16).ok()?;
        let theirs = settings::their_line(&settings::their_files(cwd, config_dir));
        let command = settings::command(
            &core.helper,
            core.shell,
            theirs.as_ref().and_then(|t| t.command.as_deref()),
        );
        let arg = match &command {
            Some(command) => {
                let file = core.dir.join(format!("tab-{tab}.json"));
                if let Err(e) =
                    settings::write(&file, &settings::settings(command, theirs.as_ref()))
                {
                    eprintln!("live: could not write tab {tab}'s settings: {e}");
                    return None;
                }
                Some(format!("--settings={}", file.display()))
            }
            None => None,
        };
        {
            let mut state = core.state();
            state.links.retain(|_, l| l.tab != tab);
            state.links.insert(token.clone(), Link { tab, account });
            state.tabs.insert(
                tab,
                TabLive {
                    tab,
                    account,
                    kept_theirs: command.is_none(),
                    ..TabLive::default()
                },
            );
        }
        core.changed();
        Some(Ticket {
            env: vec![
                (PIPE_VAR, OsString::from(&core.name)),
                (TOKEN_VAR, OsString::from(&token)),
            ],
            token,
            arg,
        })
    }

    /// The tab's `claude` ended; its card empties, while its account's meters stay.
    pub fn closed(&self, token: &str) {
        let Some(core) = &self.core else { return };
        {
            let mut state = core.state();
            let Some(link) = state.links.remove(token) else {
                return;
            };
            state.tabs.remove(&link.tab);
        }
        core.changed();
    }

    pub fn view(&self) -> View {
        self.core.as_ref().map(|c| c.view()).unwrap_or_default()
    }

    pub fn dismiss(&self, account: AccountId) {
        if let Some(core) = &self.core {
            core.state().warner.dismiss(account);
            core.changed();
        }
    }

    /// The threshold or the accounts changed, so offers and warnings are drawn again.
    pub fn refresh(&self) {
        if let Some(core) = &self.core {
            core.changed();
        }
    }
}

impl Core {
    fn state(&self) -> MutexGuard<'_, State> {
        // Every change leaves the state whole, so a panic elsewhere does not spoil it.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn view(&self) -> View {
        let now = now_s();
        let accounts = self.sink.accounts();
        let warn_at = self.sink.warn_at();
        let state = self.state();
        View {
            tabs: state.tabs.values().cloned().collect(),
            meters: state.meters.view(now),
            warnings: state.warner.view(&state.meters, &accounts, warn_at, now),
        }
    }

    fn changed(&self) {
        self.sink.changed(self.view());
    }

    fn receive(&self, line: Line) {
        let Event::Status(status) = line.event;
        let status: Status = status.checked();
        let warn_at = self.sink.warn_at();
        let warned = {
            let mut state = self.state();
            let Some(&Link { tab, account }) = state.links.get(&line.token) else {
                // A wrong token learns nothing, not even that the pipe is Cerberus.
                return;
            };
            if let Some(shown) = state.tabs.get_mut(&tab) {
                shown.context = status.context.or(shown.context);
                shown.model = status.model.clone().or(shown.model.take());
            }
            state.meters.report(account, &status);
            let crossed = status
                .five_hour
                .is_some_and(|w| state.warner.check(account, tab, w, warn_at));
            crossed.then_some(account)
        };
        let view = self.view();
        let warning = warned.and_then(|a| view.warnings.iter().find(|w| w.account == a).cloned());
        self.sink.changed(view);
        if let Some(warning) = warning {
            self.sink.warned(warning);
        }
    }
}

fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn listen(core: &Arc<Core>) -> io::Result<()> {
    let mut listener = link::Listener::bind(&core.name)?;
    let core = Arc::clone(core);
    let open = Arc::new(AtomicUsize::new(0));
    thread::Builder::new().spawn(move || {
        loop {
            let stream = match listener.accept() {
                Ok(Ok(stream)) => stream,
                Ok(Err(_)) => continue,
                Err(e) => {
                    eprintln!("live: stopped listening: {e}");
                    return;
                }
            };
            if open.fetch_add(1, Ordering::AcqRel) >= MOST_CONNECTIONS {
                open.fetch_sub(1, Ordering::AcqRel);
                continue;
            }
            let core = Arc::clone(&core);
            let done = Arc::clone(&open);
            // A helper that never finishes its line holds only its own thread.
            let started = thread::Builder::new().spawn(move || {
                if let Some(line) = read_line(stream) {
                    core.receive(line);
                }
                done.fetch_sub(1, Ordering::AcqRel);
            });
            if started.is_err() {
                // The thread never ran, so its slot is given back here.
                open.fetch_sub(1, Ordering::AcqRel);
                eprintln!("live: no thread for a report");
            }
        }
    })?;
    Ok(())
}

fn read_line(stream: link::Stream) -> Option<Line> {
    let mut text = String::new();
    BufReader::new((&stream).take(MOST_LINE))
        .read_line(&mut text)
        .ok()?;
    serde_json::from_str(&text).ok()
}

/// Claude Code runs status line commands through Git Bash when it is installed, else PowerShell.
#[cfg(windows)]
pub fn claude_shell() -> Shell {
    let named = std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH").is_some();
    if named || crate::programs::Finder::from_env().git_bash().is_some() {
        Shell::Posix
    } else {
        Shell::PowerShell
    }
}

#[cfg(not(windows))]
pub fn claude_shell() -> Shell {
    Shell::Posix
}

/// The helper that ships beside the app, if it is there.
pub fn helper_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let helper = exe.with_file_name(format!("cerberus-live{}", std::env::consts::EXE_SUFFIX));
    helper.is_file().then_some(helper)
}
