//! Find a chat: every account's transcripts searched in memory, newest first, one search at a time.

pub mod app;
mod fuzzy;
pub mod read;

use std::cmp::Ordering as Order;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::Serialize;

use crate::accounts::AccountId;
use crate::findchat::read::{Meta, Query, field, holds_all, holds_any, meta, read};
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

/// One chat in the list. Its text is the transcript's own and goes nowhere but the dialog.
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
    /// Which list it is in: its name matched, its text holds every word, or the fallback.
    pub group: Group,
    /// Higher is better within its group.
    pub score: u32,
    /// What matched: the title, the folder's name, or the snippet of message text.
    pub place: Place,
    /// Character ranges of `place` to highlight, as start and length.
    pub marks: Vec<[u32; 2]>,
    pub snippet: Option<String>,
    /// A tab that already has this chat.
    pub open_in: Option<TabId>,
}

/// The lists, best first: a fuzzy name match, then all words in its text, then "Close matches".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Group {
    Name,
    Text,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Place {
    Title,
    Folder,
    Text,
}

impl Found {
    /// The list's order, which the dialog keeps as batches merge: group, then score, then newest.
    pub fn order(&self, other: &Self) -> Order {
        (self.group, other.score, other.last).cmp(&(other.group, self.score, self.last))
    }
}

/// What a chat's check found, before it is given its id and account.
struct Matched {
    meta: Meta,
    group: Group,
    score: u32,
    place: Place,
    marks: Vec<[u32; 2]>,
    snippet: Option<String>,
}

/// Which pass reads a file: the main lists, or "Close matches" after they came up empty.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    Main,
    Close,
}

// A title or folder holding every word ranks above a chat whose words are only in its messages.
const IN_NAME: u32 = 1000;

/// The last part of a folder's path, as the list shows it.
fn folder_name(folder: &str) -> Option<&str> {
    folder.split(['/', '\\']).rfind(|part| !part.is_empty())
}

/// Each matched letter as a range, neighbours joined, for highlighting.
fn letter_ranges(marks: &[u32]) -> Vec<[u32; 2]> {
    let mut ranges: Vec<[u32; 2]> = Vec::new();
    for &at in marks {
        match ranges.last_mut() {
            Some([start, len]) if *start + *len == at => *len += 1,
            _ => ranges.push([at, 1]),
        }
    }
    ranges
}

/// The title or the folder's name with every word's letters in order, whichever scores higher.
fn by_name(meta: &Meta, query: &Query) -> Option<Matched> {
    let title = meta
        .title
        .as_deref()
        .and_then(|t| fuzzy::all_words(query.words(), t))
        .map(|f| (f, Place::Title));
    let folder = meta
        .folder
        .as_deref()
        .and_then(folder_name)
        .and_then(|f| fuzzy::all_words(query.words(), f))
        .map(|f| (f, Place::Folder));
    let (best, place) = match (title, folder) {
        (Some(t), Some(f)) => {
            if f.0.score > t.0.score {
                f
            } else {
                t
            }
        }
        (t, f) => t.or(f)?,
    };
    Some(Matched {
        meta: meta.clone(),
        group: Group::Name,
        score: best.score,
        place,
        marks: letter_ranges(&best.marks),
        snippet: None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Batch {
    Found { search: u64, chats: Vec<Found> },
    Done { search: u64, searched: usize },
}

impl Batch {
    /// A batch already in the list's order, so the dialog's merge only interleaves it.
    fn found(search: u64, mut chats: Vec<Found>) -> Self {
        chats.sort_by(Found::order);
        Self::Found { search, chats }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: u64,
    len: u64,
}

/// Lives while the dialog is open, and `close` drops everything it learned.
#[derive(Clone)]
pub struct ChatFinder {
    current: Arc<AtomicU64>,
    cache: Arc<Mutex<HashMap<PathBuf, (Stamp, Meta)>>>,
    wait: Duration,
}

impl Default for ChatFinder {
    fn default() -> Self {
        Self::with_wait(BATCH_WAIT)
    }
}

impl ChatFinder {
    fn with_wait(wait: Duration) -> Self {
        Self {
            current: Arc::default(),
            cache: Arc::default(),
            wait,
        }
    }

    fn cache(&self) -> MutexGuard<'_, HashMap<PathBuf, (Stamp, Meta)>> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts a search on its own thread and returns its number, and any earlier search stops.
    pub fn search(
        &self,
        sources: Vec<Source>,
        text: &str,
        open: HashMap<String, TabId>,
        send: impl Fn(Batch) + Send + Sync + 'static,
    ) -> u64 {
        let search = self.current.fetch_add(1, Ordering::AcqRel) + 1;
        let finder = self.clone();
        // A pasted id, alone or in `claude --resume <id>`, finds that chat on whichever account has it.
        let wanted = session_id_in(text);
        let query = Query::new(if wanted.is_some() { "" } else { text });
        let send = Arc::new(send);
        let sender = Arc::clone(&send);
        let started = thread::Builder::new().spawn(move || {
            finder.run(
                search,
                &sources,
                (&query, wanted.as_deref()),
                &open,
                &*sender,
            );
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
        (query, wanted): (&Query, Option<&str>),
        open: &HashMap<String, TabId>,
        send: &dyn Fn(Batch),
    ) {
        let cancelled = || self.current.load(Ordering::Acquire) != search;
        let mut files = list(sources);
        if let Some(wanted) = wanted {
            files.retain(|(path, _, _)| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|id| id.eq_ignore_ascii_case(wanted))
            });
        }
        // Newest first, so the chats most likely wanted show before the rest are read.
        files.sort_by_key(|f| std::cmp::Reverse(f.2.modified));
        let mut folders: HashMap<String, bool> = HashMap::new();
        let mut batch = Vec::new();
        let mut sent = Instant::now();
        let mut found_any = false;
        // "Close matches" are read only when nothing holds every word, so they never mix into the main list.
        let close_pass = query.words().len() >= 2;
        for pass in [Pass::Main, Pass::Close] {
            if pass == Pass::Close && (found_any || !close_pass) {
                break;
            }
            for (path, account, stamp) in &files {
                if cancelled() {
                    return;
                }
                // Checked on every file, so a lone early match shows while the rest are still read.
                if !batch.is_empty() && sent.elapsed() >= self.wait {
                    send(Batch::found(search, std::mem::take(&mut batch)));
                    sent = Instant::now();
                }
                let Some(hit) = self.check(search, path, *stamp, query, pass, &cancelled) else {
                    continue;
                };
                let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                found_any = true;
                let folder_exists = hit.meta.folder.as_ref().is_some_and(|f| {
                    *folders
                        .entry(f.clone())
                        .or_insert_with(|| Path::new(f).is_dir())
                });
                batch.push(Found {
                    id: id.to_owned(),
                    account: *account,
                    title: hit.meta.title.unwrap_or_else(|| "Untitled chat".to_owned()),
                    folder: hit.meta.folder,
                    folder_exists,
                    last: hit.meta.last.unwrap_or(stamp.modified),
                    group: hit.group,
                    score: hit.score,
                    place: hit.place,
                    marks: hit.marks,
                    snippet: hit.snippet,
                    open_in: open.get(id).copied(),
                });
                if batch.len() >= BATCH || sent.elapsed() >= self.wait {
                    send(Batch::found(search, std::mem::take(&mut batch)));
                    sent = Instant::now();
                }
            }
        }
        if cancelled() {
            return;
        }
        if !batch.is_empty() {
            send(Batch::found(search, batch));
        }
        send(Batch::Done {
            search,
            searched: files.len(),
        });
    }

    /// Where and how well the chat matches in this pass; a file that can't be read is skipped.
    fn check(
        &self,
        search: u64,
        path: &Path,
        stamp: Stamp,
        query: &Query,
        pass: Pass,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Matched> {
        let known = self
            .cache()
            .get(path)
            .filter(|(was, _)| *was == stamp)
            .map(|(_, meta)| meta.clone());
        if query.is_empty()
            && let Some(meta) = known
        {
            return Some(listed(meta));
        }
        // A name that matches needs no read of the chat, once an earlier search has its title.
        if pass == Pass::Main
            && let Some(hit) = known.as_ref().and_then(|meta| by_name(meta, query))
        {
            return Some(hit);
        }
        let bytes = fs::read(path).ok()?;
        let meta = known.unwrap_or_else(|| {
            let quick = meta(&bytes);
            let mut cache = self.cache();
            // Under the lock `close` clears with, so a search it stopped can't refill the cache after.
            if self.current.load(Ordering::Acquire) == search {
                cache.insert(path.to_path_buf(), (stamp, quick.clone()));
            }
            quick
        });
        if query.is_empty() {
            return Some(listed(meta));
        }
        if pass == Pass::Main
            && let Some(hit) = by_name(&meta, query)
        {
            return Some(hit);
        }
        // One letter is for names only; message text is searched from two letters on.
        if query
            .words()
            .iter()
            .map(|w| w.chars().count())
            .sum::<usize>()
            < 2
        {
            return None;
        }
        // Most files lack a word, and these checks are far cheaper than parsing their records.
        let could = match pass {
            Pass::Main => holds_all(&bytes, query),
            Pass::Close => holds_any(&bytes, query),
        };
        if !could {
            return None;
        }
        let found = read(&bytes, query, cancelled)?;
        let words = usize::try_from(found.rank.words).unwrap_or(usize::MAX);
        let all = query.words().len();
        match pass {
            Pass::Main if words == all => {
                Some(by_text(meta, found.snippet, found.rank.exact, query))
            }
            Pass::Close if words < all && words * 2 >= all => {
                let snippet_marks = found.snippet.as_deref().map(|s| field(s, query).marks);
                Some(Matched {
                    meta,
                    group: Group::Close,
                    score: found.rank.words * 100 + found.rank.exact,
                    place: Place::Text,
                    marks: snippet_marks.unwrap_or_default(),
                    snippet: found.snippet,
                })
            }
            _ => None,
        }
    }
}

/// A chat listed with nothing typed: newest first, nothing marked.
fn listed(meta: Meta) -> Matched {
    Matched {
        meta,
        group: Group::Text,
        score: 0,
        place: Place::Text,
        marks: Vec::new(),
        snippet: None,
    }
}

/// A chat holding every word: shown by its title or folder when one holds them all, else by its snippet.
fn by_text(meta: Meta, snippet: Option<String>, exact: u32, query: &Query) -> Matched {
    let title = meta.title.as_deref().map(|t| field(t, query));
    let folder = meta
        .folder
        .as_deref()
        .and_then(folder_name)
        .map(|f| field(f, query));
    let (place, marks, bonus) = match (title, folder) {
        (Some(t), _) if t.all => (Place::Title, t.marks, IN_NAME),
        (_, Some(f)) if f.all => (Place::Folder, f.marks, IN_NAME),
        _ => (
            Place::Text,
            snippet
                .as_deref()
                .map(|s| field(s, query).marks)
                .unwrap_or_default(),
            0,
        ),
    };
    Matched {
        meta,
        group: Group::Text,
        score: bonus + exact,
        place,
        marks,
        snippet,
    }
}

/// The chat with this id in any account, as Find chat lists it; a moved chat's newest copy is the one in use.
pub fn locate(sources: &[Source], id: &str, open: &HashMap<String, TabId>) -> Option<Found> {
    let (path, account, stamp) = list(sources)
        .into_iter()
        .filter(|(path, _, _)| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|stem| stem.eq_ignore_ascii_case(id))
        })
        .max_by_key(|f| f.2.modified)?;
    let meta = meta(&fs::read(&path).ok()?);
    let id = path.file_stem()?.to_str()?.to_owned();
    Some(Found {
        account,
        title: meta.title.unwrap_or_else(|| "Untitled chat".to_owned()),
        folder_exists: meta.folder.as_ref().is_some_and(|f| Path::new(f).is_dir()),
        folder: meta.folder,
        last: meta.last.unwrap_or(stamp.modified),
        group: Group::Text,
        score: 0,
        place: Place::Text,
        marks: Vec::new(),
        snippet: None,
        open_in: open.get(&id).copied(),
        id,
    })
}

/// The first chat id in `text`, shaped as Claude Code writes them (8-4-4-4-12 hex digits), lower-cased.
pub fn session_id_in(text: &str) -> Option<String> {
    let shaped = |word: &str| {
        let parts: Vec<&str> = word.split('-').collect();
        parts.len() == 5
            && parts
                .iter()
                .zip([8, 4, 4, 4, 12])
                .all(|(part, len)| part.len() == len && part.chars().all(|c| c.is_ascii_hexdigit()))
    };
    text.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '=' | ',' | ';'))
        .find(|word| shaped(word))
        .map(str::to_ascii_lowercase)
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
