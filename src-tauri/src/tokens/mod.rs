//! Tokens and an API price estimate per account and model, from the usage fields of each account's chat files.

pub mod app;
mod days;
mod prices;
mod read;
mod store;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{self, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;
use crate::tokens::read::Parsed;
use crate::tokens::store::{Band, Batch, Store, Total};

/// Days kept in the store, past the 30 the card shows.
const KEEP_DAYS: u64 = 90;
const DAY_MS: u64 = 86_400_000;
/// Replies per commit, so a first look at many chats commits a few times rather than once per file.
const BATCH_REPLIES: usize = 20_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Tokens {
    fn add(&mut self, other: &Self) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
    }

    fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }
}

/// How far a chat file has been recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recorded {
    pub len: u64,
    pub modified: u64,
    /// Bytes up to the last complete line recorded.
    pub offset: u64,
}

/// What the chat files hold that the store doesn't yet, read a file's new lines at a time.
pub struct Ledger {
    file: PathBuf,
    store: Option<Store>,
    opened: bool,
    recorded: HashMap<PathBuf, Recorded>,
    /// The last answer, with the day, offset and accounts it was for; it stands while nothing new is recorded.
    last: Option<(i64, i32, Vec<AccountId>, Vec<AccountTokens>)>,
}

/// One model's row for a range of days, ready to draw.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRow {
    pub model: String,
    pub tokens: Tokens,
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountTokens {
    pub account: AccountId,
    pub today: Vec<ModelRow>,
    pub week: Vec<ModelRow>,
    pub month: Vec<ModelRow>,
}

impl Ledger {
    pub fn new(file: PathBuf) -> Self {
        Self {
            file,
            store: None,
            opened: false,
            recorded: HashMap::new(),
            last: None,
        }
    }

    /// Records what changed in each account's chat files and returns today, 7 and 30 days per account at the user's offset.
    pub fn view(
        &mut self,
        accounts: &[(AccountId, PathBuf)],
        offset_minutes: i32,
        now_ms: u64,
    ) -> Vec<AccountTokens> {
        if !self.opened {
            self.store = Store::open(&self.file);
            self.recorded = self.store.as_ref().map(Store::progress).unwrap_or_default();
            self.opened = true;
        }
        let oldest_ms = now_ms.saturating_sub(KEEP_DAYS * DAY_MS);
        let mut changed = self.record(accounts, oldest_ms);
        let Some(store) = &mut self.store else {
            return Vec::new();
        };
        match store.prune(oldest_ms) {
            Ok(gone) => changed |= gone > 0,
            Err(e) => eprintln!("tokens: could not drop old replies: {e}"),
        }
        let today = days::day_of(now_ms, offset_minutes);
        let ids: Vec<AccountId> = accounts.iter().map(|(id, _)| *id).collect();
        if !changed
            && let Some((day, offset, known, views)) = &self.last
            && (*day, *offset, known) == (today, offset_minutes, &ids)
        {
            return views.clone();
        }
        let at = |day: i64| days::start_ms(day, offset_minutes);
        let totals = store
            .totals(at(today - 29), at(today - 6), at(today), at(today + 1))
            .unwrap_or_else(|e| {
                eprintln!("tokens: could not add up replies: {e}");
                Vec::new()
            });
        let views: Vec<AccountTokens> = ids
            .iter()
            .map(|&account| AccountTokens {
                account,
                today: rows(&totals, account, &[Band::Today]),
                week: rows(&totals, account, &[Band::Today, Band::Week]),
                month: rows(&totals, account, &[Band::Today, Band::Week, Band::Month]),
            })
            .collect();
        self.last = Some((today, offset_minutes, ids, views.clone()));
        views
    }

    /// Reads each changed chat file from where it was last recorded; a file's place moves on only once its replies are committed. True when anything was recorded.
    fn record(&mut self, accounts: &[(AccountId, PathBuf)], oldest_ms: u64) -> bool {
        let Some(store) = &mut self.store else {
            return false;
        };
        let mut present: HashSet<PathBuf> = HashSet::new();
        let mut batches: Vec<Batch> = Vec::new();
        let mut pending = 0;
        let mut changed = false;
        for (account, config_dir) in accounts {
            for (path, len, modified) in chat_files(config_dir) {
                let known = self.recorded.get(&path).copied();
                // A file untouched for longer holds only days the store no longer keeps.
                if modified < oldest_ms && known.is_none() {
                    continue;
                }
                present.insert(path.clone());
                let from = match known {
                    Some(k) if k.len == len && k.modified == modified => continue,
                    Some(k) if len > k.len => k.offset,
                    // Shrunk or changed in place: read again whole; replies already kept are matched by id.
                    _ => 0,
                };
                let mut parsed = Parsed::default();
                let more = match read_from(&path, from, &mut parsed) {
                    Ok(more) => more,
                    Err(e) => {
                        eprintln!("tokens: could not read a chat file: {e}");
                        continue;
                    }
                };
                let replies: Vec<_> = parsed
                    .replies
                    .into_iter()
                    .filter(|r| r.at_ms >= oldest_ms)
                    .map(|r| {
                        let cost = prices::cost(&r.model, r.fast, &r.tokens, r.written_1h);
                        (r, cost)
                    })
                    .collect();
                pending += replies.len();
                batches.push(Batch {
                    path,
                    recorded: Recorded {
                        len,
                        modified,
                        offset: from + more,
                    },
                    account: *account,
                    replies,
                });
                if pending >= BATCH_REPLIES {
                    changed |= commit(store, &mut self.recorded, &mut batches);
                    pending = 0;
                }
            }
        }
        changed |= commit(store, &mut self.recorded, &mut batches);
        let gone: Vec<PathBuf> = self
            .recorded
            .keys()
            .filter(|path| !present.contains(*path))
            .cloned()
            .collect();
        if !gone.is_empty() {
            match store.forget(&gone) {
                Ok(()) => self.recorded.retain(|path, _| present.contains(path)),
                Err(e) => eprintln!("tokens: could not forget deleted chats: {e}"),
            }
        }
        changed
    }
}

/// Commits the waiting files' replies and places together; on failure their places stay, so the next look records them again.
fn commit(
    store: &mut Store,
    recorded: &mut HashMap<PathBuf, Recorded>,
    batches: &mut Vec<Batch>,
) -> bool {
    if batches.is_empty() {
        return false;
    }
    let done = std::mem::take(batches);
    match store.record(&done) {
        Ok(()) => {
            for batch in done {
                recorded.insert(batch.path, batch.recorded);
            }
            true
        }
        Err(e) => {
            eprintln!("tokens: could not record chats' replies: {e}");
            false
        }
    }
}

/// Reads a file's lines from `offset` into `parsed`; returns how far the complete lines went.
fn read_from(path: &Path, offset: u64, parsed: &mut Parsed) -> io::Result<u64> {
    let mut file = fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    parsed.read_lines(&mut BufReader::with_capacity(1 << 20, file))
}

/// Each chat file and each subagent file under an account's `projects`, with its size and change time.
fn chat_files(config_dir: &Path) -> Vec<(PathBuf, u64, u64)> {
    let mut files = Vec::new();
    let Ok(projects) = fs::read_dir(config_dir.join("projects")) else {
        return files;
    };
    let mut push = |path: PathBuf| {
        let Ok(info) = fs::metadata(&path) else {
            return;
        };
        if !info.is_file() || path.extension().is_none_or(|e| e != "jsonl") {
            return;
        }
        let modified = info
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .and_then(|d| u64::try_from(d.as_millis()).ok())
            .unwrap_or(0);
        files.push((path, info.len(), modified));
    };
    for project in projects.flatten().filter(|e| e.path().is_dir()) {
        let Ok(entries) = fs::read_dir(project.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // A chat's subagents write their own files beside it.
                if let Ok(subagents) = fs::read_dir(path.join("subagents")) {
                    for sub in subagents.flatten() {
                        push(sub.path());
                    }
                }
            } else {
                push(path);
            }
        }
    }
    files
}

/// One row per model for an account over the given bands, the costliest first; fast mode is its own row.
fn rows(totals: &[Total], account: AccountId, bands: &[Band]) -> Vec<ModelRow> {
    let mut by_name: BTreeMap<String, ModelRow> = BTreeMap::new();
    for total in totals
        .iter()
        .filter(|t| t.account == account && bands.contains(&t.band))
    {
        let name = prices::name_of(&total.model);
        let name = if total.fast {
            format!("{name} (fast)")
        } else {
            name
        };
        let row = by_name.entry(name.clone()).or_insert_with(|| ModelRow {
            model: name,
            tokens: Tokens::default(),
            cost: Some(0.0),
        });
        // Dated and undated ids of one model share a row.
        row.tokens.add(&total.tokens);
        row.cost = row.cost.zip(total.cost).map(|(a, b)| a + b);
    }
    let mut rows: Vec<ModelRow> = by_name.into_values().collect();
    rows.sort_by(|a, b| {
        b.cost
            .unwrap_or(-1.0)
            .total_cmp(&a.cost.unwrap_or(-1.0))
            .then(b.tokens.total().cmp(&a.tokens.total()))
    });
    rows
}

#[cfg(test)]
mod tests;
