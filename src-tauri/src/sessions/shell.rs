use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, UNIX_EPOCH};

use serde::Serialize;

use crate::saved::{Saved, TabId};
use crate::sessions::procs::Probes;
use crate::sessions::watch::{Follower, read_current};
use crate::sessions::{SessionFile, TabState, Watched};

// Looking costs one listing of the account's `sessions` folder; the process walk runs only for a new file in it.
const LOOK: Duration = Duration::from_secs(1);
const FOLLOW: Duration = Duration::from_millis(500);

/// A `claude` the user started in a shell tab, as the tab's channel reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "chat", rename_all = "camelCase")]
pub enum HandChat {
    /// `cwd` is the chat's folder, so the tab resumes it there; `None` when the file names no folder that exists.
    Started { cwd: Option<String> },
    /// `saved` when the chat can be resumed, so the tab keeps it as its ended chat.
    Ended { saved: bool },
}

/// A shell tab's program, watched for a `claude` started below it on the tab's own account.
pub struct Shell {
    /// The tab's account folder; a `claude` given another `CLAUDE_CONFIG_DIR` writes elsewhere and is not seen.
    pub config_dir: PathBuf,
    pub pid: u32,
    pub cwd: String,
    /// When the shell started, in milliseconds since 1970.
    pub started_ms: u64,
    pub probes: Probes,
}

/// Where the watcher's findings go.
pub trait Report: Send + 'static {
    fn chat(&mut self, chat: &HandChat);
    fn session(&mut self, id: &str);
    fn state(&mut self, state: TabState);
    /// Whether Cerberus still has the shell open; a chat that ends because Cerberus closed the shell never counts as ended.
    fn open(&self) -> bool;
}

fn pid_of(name: &str) -> Option<u32> {
    let digits = name.strip_suffix(".json")?;
    digits
        .bytes()
        .all(|b| b.is_ascii_digit())
        .then(|| digits.parse().ok())
        .flatten()
}

fn modified_ms(entry: &fs::DirEntry) -> Option<u64> {
    let modified = entry.metadata().ok()?.modified().ok()?;
    u64::try_from(modified.duration_since(UNIX_EPOCH).ok()?.as_millis()).ok()
}

/// Finds a new session file whose process runs below the shell, by process ancestry and never by folder.
struct Spotter {
    sessions: PathBuf,
    shell: u32,
    started_ms: u64,
    /// Files already judged not to be the shell's, so each costs one process walk at most.
    judged: HashSet<u32>,
}

impl Spotter {
    fn new(shell: &Shell) -> Self {
        Self {
            sessions: shell.config_dir.join("sessions"),
            shell: shell.pid,
            started_ms: shell.started_ms,
            judged: HashSet::new(),
        }
    }

    fn spot(&mut self, below: fn(u32, u64) -> Vec<u32>) -> Option<(u32, SessionFile)> {
        let Ok(entries) = fs::read_dir(&self.sessions) else {
            return None;
        };
        let mut present = HashSet::new();
        let mut fresh = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let Some(pid) = pid_of(&entry.file_name().to_string_lossy()) else {
                continue;
            };
            present.insert(pid);
            // A file untouched since before the shell started belongs to a `claude` that cannot be below it.
            let new = modified_ms(&entry).is_some_and(|at| at >= self.started_ms);
            if new && !self.judged.contains(&pid) {
                fresh.push((pid, entry.path()));
            }
        }
        // A pid whose file is gone may come back as a new `claude`, so it is judged again.
        self.judged.retain(|pid| present.contains(pid));
        let mut chats = Vec::new();
        for (pid, path) in fresh {
            match read_current(&path, self.started_ms) {
                Some(file) if file.interactive => chats.push((pid, file)),
                // A `claude -p` run is a command, not a chat for the tab to follow.
                Some(_) => {
                    self.judged.insert(pid);
                }
                None => {}
            }
        }
        let fresh = chats;
        if fresh.is_empty() {
            return None;
        }
        let tree = below(self.shell, self.started_ms);
        let mut found = None;
        for (pid, file) in fresh {
            if found.is_none() && tree.contains(&pid) {
                found = Some((pid, file));
            } else if !tree.contains(&pid) {
                self.judged.insert(pid);
            }
        }
        found
    }
}

/// The chat's folder from its session file, when it is a folder that exists.
fn folder(file: &SessionFile) -> Option<String> {
    file.cwd
        .clone()
        .filter(|cwd| Path::new(cwd).is_absolute() && Path::new(cwd).is_dir())
}

/// Watches a shell tab for a `claude` started in it and follows it like a Claude tab's own, until `stop` fires or its sender is dropped.
pub fn watch_shell(
    shell: Shell,
    stop: Receiver<()>,
    mut report: impl Report,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new().spawn(move || {
        let mut spotter = Spotter::new(&shell);
        let mut following: Option<(u32, Follower)> = None;
        loop {
            let gap = if following.is_some() { FOLLOW } else { LOOK };
            if !matches!(stop.recv_timeout(gap), Err(RecvTimeoutError::Timeout)) {
                // A first message sent just before the shell closed still makes the chat resumable.
                if let Some((_, follower)) = &mut following
                    && let (Some(id), _) = follower.finish()
                {
                    report.session(&id);
                }
                return;
            }
            let Some((pid, follower)) = &mut following else {
                if let Some((pid, file)) = spotter.spot(shell.probes.below) {
                    let cwd = folder(&file);
                    report.chat(&HandChat::Started { cwd: cwd.clone() });
                    let watched = Watched {
                        config_dir: shell.config_dir.clone(),
                        pid,
                        cwd: cwd.unwrap_or_else(|| shell.cwd.clone()),
                        started_ms: shell.started_ms,
                    };
                    following = Some((pid, Follower::at(watched)));
                }
                continue;
            };
            let file = spotter.sessions.join(format!("{pid}.json"));
            if (shell.probes.is_running)(*pid) && file.exists() {
                let (changed, unsaved) = follower.step();
                if let Some(state) = changed {
                    report.state(state);
                }
                if let Some(id) = unsaved {
                    report.session(&id);
                }
                continue;
            }
            let (unsaved, saved) = follower.finish();
            if let Some(id) = unsaved {
                report.session(&id);
            }
            if !report.open() {
                return;
            }
            report.chat(&HandChat::Ended { saved });
            // A file left by a `claude` that crashed must not be found again.
            spotter.judged.insert(*pid);
            following = None;
        }
    })
}

/// Keeps a chat started by hand in the tab's saved record: a restart resumes it, and its end leaves it as the ended chat.
pub struct Keeper {
    pub saved: Saved,
    pub tab: TabId,
}

impl Keeper {
    pub fn session(&self, id: &str) {
        self.saved.set_session(self.tab, id);
        self.saved.set_ended(self.tab, false);
    }

    pub fn chat(&self, chat: &HandChat) {
        if *chat == (HandChat::Ended { saved: true }) {
            self.saved.set_ended(self.tab, true);
        }
    }
}

#[cfg(test)]
mod tests;
