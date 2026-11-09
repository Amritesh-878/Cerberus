//! Find a chat: every account's transcripts searched in memory, newest first, one search at a time.

pub mod app;
pub mod read;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::Serialize;

use crate::accounts::AccountId;
use crate::findchat::read::{Meta, Query, read};
use crate::saved::TabId;

/// Results go out in batches this big, or sooner when this long has passed.
const BATCH: usize = 25;
const BATCH_WAIT: Duration = Duration::from_millis(80);

/// An account folder to search, and the account its chats resume on.
#[derive(Debug, Clone)]
pub struct Source {
    pub account: Option<AccountId>,
    pub config_dir: PathBuf,
}

/// One chat in the list; the text is the transcript's own and goes nowhere but the dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub id: String,
    pub account: Option<AccountId>,
    pub title: String,
    pub folder: Option<String>,
    /// The recorded folder is still there, so the chat can resume in it.
    pub folder_exists: bool,
    pub last: u64,
    pub snippet: Option<String>,
    /// A tab that already has this chat.
    pub open_in: Option<TabId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Batch {
    Found { search: u64, chats: Vec<Found> },
    Done { search: u64, searched: usize },
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: u64,
    len: u64,
}

/// Lives while the dialog is open; `close` drops everything it learned.
#[derive(Clone, Default)]
pub struct ChatFinder {
    current: Arc<AtomicU64>,
    cache: Arc<Mutex<HashMap<PathBuf, (Stamp, Meta)>>>,
}

impl ChatFinder {
    fn cache(&self) -> MutexGuard<'_, HashMap<PathBuf, (Stamp, Meta)>> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts a search on its own thread and returns its number; any earlier search stops.
    pub fn search(
        &self,
        sources: Vec<Source>,
        text: &str,
        open: HashMap<String, TabId>,
        send: impl Fn(Batch) + Send + Sync + 'static,
    ) -> u64 {
        let search = self.current.fetch_add(1, Ordering::AcqRel) + 1;
        let finder = self.clone();
        let query = Query::new(text);
        let send = Arc::new(send);
        let sender = Arc::clone(&send);
        let started = thread::Builder::new().spawn(move || {
            finder.run(search, &sources, &query, &open, &*sender);
        });
        if started.is_err() {
            send(Batch::Done {
                search,
                searched: 0,
            });
        }
        search
    }

    /// The dialog closed: the running search stops and nothing read stays in memory.
    pub fn close(&self) {
        self.current.fetch_add(1, Ordering::AcqRel);
        self.cache().clear();
    }

    fn run(
        &self,
        search: u64,
        sources: &[Source],
        query: &Query,
        open: &HashMap<String, TabId>,
        send: &dyn Fn(Batch),
    ) {
        let cancelled = || self.current.load(Ordering::Acquire) != search;
        let mut files = list(sources);
        // Newest first, so the chats most likely wanted show before the rest are read.
        files.sort_by_key(|f| std::cmp::Reverse(f.2.modified));
        let mut folders: HashMap<String, bool> = HashMap::new();
        let mut batch = Vec::new();
        let mut sent = Instant::now();
        let mut searched = 0;
        for (path, account, stamp) in files {
            if cancelled() {
                return;
            }
            searched += 1;
            let Some((meta, snippet)) = self.check(&path, stamp, query, &cancelled) else {
                continue;
            };
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let folder_exists = meta.folder.as_ref().is_some_and(|f| {
                *folders
                    .entry(f.clone())
                    .or_insert_with(|| Path::new(f).is_dir())
            });
            batch.push(Found {
                id: id.to_owned(),
                account,
                title: meta.title.unwrap_or_else(|| "Untitled chat".to_owned()),
                folder: meta.folder,
                folder_exists,
                last: meta.last.unwrap_or(stamp.modified),
                snippet,
                open_in: open.get(id).copied(),
            });
            if batch.len() >= BATCH || sent.elapsed() >= BATCH_WAIT {
                send(Batch::Found {
                    search,
                    chats: std::mem::take(&mut batch),
                });
                sent = Instant::now();
            }
        }
        if cancelled() {
            return;
        }
        if !batch.is_empty() {
            send(Batch::Found {
                search,
                chats: batch,
            });
        }
        send(Batch::Done { search, searched });
    }

    /// The chat's list fields and snippet when it matches; a file that can't be read is skipped.
    fn check(
        &self,
        path: &Path,
        stamp: Stamp,
        query: &Query,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<(Meta, Option<String>)> {
        let known = self
            .cache()
            .get(path)
            .filter(|(was, _)| *was == stamp)
            .map(|(_, meta)| meta.clone());
        if query.is_empty()
            && let Some(meta) = known
        {
            return Some((meta, None));
        }
        let bytes = fs::read(path).ok()?;
        // A file without every plain word can't match, so it is not parsed; its list fields are known.
        if known.is_some() && !query.is_empty() {
            let lower = bytes.to_ascii_lowercase();
            let has = |word: &str| lower.windows(word.len()).any(|w| w == word.as_bytes());
            if !query.plain_words().all(has) {
                return None;
            }
        }
        let found = read(&bytes, query, cancelled)?;
        self.cache()
            .insert(path.to_path_buf(), (stamp, found.meta.clone()));
        found.matched.then_some((found.meta, found.snippet))
    }
}

/// Every `projects/<folder>/<id>.jsonl` of every source, with its account and stamp.
fn list(sources: &[Source]) -> Vec<(PathBuf, Option<AccountId>, Stamp)> {
    let mut files = Vec::new();
    for source in sources {
        let Ok(folders) = fs::read_dir(source.config_dir.join("projects")) else {
            continue;
        };
        for folder in folders.flatten().filter(|e| e.path().is_dir()) {
            let Ok(chats) = fs::read_dir(folder.path()) else {
                continue;
            };
            for chat in chats.flatten() {
                let path = chat.path();
                let is_chat = path.extension().is_some_and(|e| e == "jsonl")
                    && path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .is_some_and(crate::sessions::is_session_id);
                let Ok(info) = chat.metadata() else { continue };
                if !is_chat || !info.is_file() {
                    continue;
                }
                let modified = info
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .and_then(|d| u64::try_from(d.as_millis()).ok())
                    .unwrap_or(0);
                files.push((
                    path,
                    source.account,
                    Stamp {
                        modified,
                        len: info.len(),
                    },
                ));
            }
        }
    }
    files
}

#[cfg(test)]
mod tests;
