//! Live cards: each Claude tab's status line reports context and limits to the app over a pipe.

pub mod event;
pub mod files;
pub mod helper;
mod hooks;
mod meters;
mod settings;
#[cfg(test)]
mod tests;
mod work;

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
use crate::live::event::{Event, Line, MOST_LINE, PIPE_VAR, TOKEN_VAR};
pub use crate::live::meters::{DEFAULT_WARN_AT, Meter, Warning};
use crate::live::meters::{Meters, Warner};
pub use crate::live::settings::Shell;
use crate::live::work::{Output, Running, Work};
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
    /// His own status line runs where Cerberus cannot wrap it, so this tab reports no context.
    pub kept_theirs: bool,
    /// Files the chat wrote, newest first.
    pub outputs: Vec<Output>,
    /// Background shells and subagents still running, oldest first.
    pub tasks: Vec<Running>,
    pub finished: u32,
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
    /// Kept across the tab's restarts, since a resumed chat is the same session.
    work: HashMap<TabId, (PathBuf, Work)>,
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
        let hook = settings::hook_command(&core.helper, core.shell);
        let file = core.dir.join(format!("tab-{tab}.json"));
        let written = settings::settings(command.as_deref(), &hook, theirs.as_ref());
        if let Err(e) = settings::write(&file, &written) {
            eprintln!("live: could not write tab {tab}'s settings: {e}");
            return None;
        }
        let arg = Some(format!("--settings={}", file.display()));
        {
            let mut state = core.state();
            state.links.retain(|_, l| l.tab != tab);
            state.links.insert(token.clone(), Link { tab, account });
            state
                .work
                .entry(tab)
                .or_insert_with(|| (cwd.to_path_buf(), Work::default()))
                .0 = cwd.to_path_buf();
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
            // Its tasks ended with its `claude`.
            if let Some((_, work)) = state.work.get_mut(&link.tab) {
                work.ended();
            }
        }
        core.changed();
    }

    /// Whether `path` is a file this tab's chat reported writing; nothing else is ever read for the webview.
    fn wrote(&self, tab: TabId, path: &str) -> bool {
        self.core.as_ref().is_some_and(|core| {
            core.state()
                .work
                .get(&tab)
                .is_some_and(|(_, work)| work.has_output(path))
        })
    }

    /// A written file for the file pane, with its `HEAD` version from `head`.
    pub fn read_output(
        &self,
        tab: TabId,
        path: &str,
        head: impl FnOnce(&Path) -> Option<Vec<u8>>,
    ) -> io::Result<files::FileView> {
        if !self.wrote(tab, path) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "not a file this chat wrote",
            ));
        }
        Ok(files::view(Path::new(path), head))
    }

    /// Changes whenever the written file does, so the pane reloads only then.
    pub fn output_stamp(&self, tab: TabId, path: &str) -> Option<String> {
        self.wrote(tab, path)
            .then(|| files::stamp(Path::new(path)))
            .flatten()
    }

    /// Opens a file the tab's chat wrote in VS Code; without VS Code, shows its folder instead.
    pub fn open_output(&self, tab: TabId, path: &str) -> io::Result<()> {
        // Only a path a hook reported for this tab, never one the webview names.
        if !self.wrote(tab, path) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "not a file this chat wrote",
            ));
        }
        match crate::panel::vscode::find() {
            Some(code) => crate::panel::vscode::open(&code, Path::new(path)),
            None => crate::panel::reveal::open(shown_instead(Path::new(path))?.as_os_str()),
        }
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
            tabs: state
                .tabs
                .values()
                .map(|shown| {
                    let mut shown = shown.clone();
                    if let Some((folder, work)) = state.work.get(&shown.tab) {
                        shown.outputs = work.outputs(folder);
                        shown.tasks = work.running();
                        shown.finished = work.finished();
                    }
                    shown
                })
                .collect(),
            meters: state.meters.view(now),
            warnings: state.warner.view(&state.meters, &accounts, warn_at, now),
        }
    }

    fn changed(&self) {
        self.sink.changed(self.view());
    }

    fn receive(&self, line: Line) {
        let Some(event) = hooks::checked(line.event) else {
            return;
        };
        let status = match event {
            Event::Status(status) => status.checked(),
            hooked => {
                self.hooked(&line.token, &hooked);
                return;
            }
        };
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

/// The folder to show for a file when no editor is there: a double-click on the file could run it.
fn shown_instead(path: &Path) -> io::Result<&Path> {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the file has no folder"))
}

impl Core {
    fn hooked(&self, token: &str, event: &Event) {
        {
            let mut state = self.state();
            let Some(&Link { tab, .. }) = state.links.get(token) else {
                return;
            };
            let Some((_, work)) = state.work.get_mut(&tab) else {
                return;
            };
            match event {
                Event::Wrote(wrote) => work.wrote(wrote),
                Event::Tasks(tasks) => work.listed(tasks),
                Event::Subagent(subagent) => work.subagent(subagent),
                Event::Status(_) => return,
            }
        }
        self.changed();
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
