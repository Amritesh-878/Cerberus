//! Tokens and an API price estimate per account and model, from the usage fields of each account's chat files.

pub mod app;
mod days;
mod prices;
mod read;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;
use crate::tokens::read::Reply;

/// Days kept in Cerberus's own store, past the 30 the card shows.
const KEEP_DAYS: i64 = 90;
const STORE_VERSION: u32 = 1;
// A chat file is a few MiB; one far larger is not read whole into memory.
const MOST_FILE: u64 = 512 * 1024 * 1024;

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

    fn max(&mut self, other: &Self) {
        self.input = self.input.max(other.input);
        self.output = self.output.max(other.output);
        self.cache_read = self.cache_read.max(other.cache_read);
        self.cache_write = self.cache_write.max(other.cache_write);
    }

    fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }
}

/// One model's tokens on one day, and their cost at API prices; `None` when the model has no price.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub tokens: Tokens,
    pub cost: Option<f64>,
}

/// Account, then `YYYY-MM-DD`, then model key.
type Days = BTreeMap<AccountId, BTreeMap<String, BTreeMap<String, Bucket>>>;

/// What the store file holds: daily totals only, never a chat's words or ids.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Stored {
    version: u32,
    days: Days,
}

/// A chat file's replies, kept until the file changes.
struct Read {
    len: u64,
    modified: u64,
    replies: Arc<Vec<Reply>>,
}

/// The chat files read so far and the store of daily totals.
pub struct Ledger {
    file: PathBuf,
    read: HashMap<PathBuf, Read>,
    stored: Option<Days>,
    /// False when the file is from a newer build, which this one must not overwrite.
    writable: bool,
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
            read: HashMap::new(),
            stored: None,
            writable: true,
        }
    }

    /// Reads what changed in each account's chat files, keeps the daily totals, and returns today, 7 and 30 days per account.
    pub fn view(
        &mut self,
        accounts: &[(AccountId, PathBuf)],
        offset_minutes: i32,
        now_ms: u64,
    ) -> Vec<AccountTokens> {
        let fresh = self.scan(accounts, offset_minutes);
        let today = days::day_of(now_ms, offset_minutes);
        let (mut stored, writable) = self
            .stored
            .take()
            .map_or_else(|| load(&self.file), |kept| (kept, self.writable));
        self.writable = writable;
        let changed =
            merge(&mut stored, &fresh) | prune(&mut stored, &days::date_of(today - KEEP_DAYS));
        if changed
            && writable
            && let Err(e) = save(&self.file, &stored)
        {
            eprintln!("tokens: could not keep the daily totals: {e}");
        }
        let views = accounts
            .iter()
            .map(|(account, _)| {
                let range = |from: i64| {
                    rows(
                        stored.get(account),
                        &days::date_of(today - from),
                        &days::date_of(today),
                    )
                };
                AccountTokens {
                    account: *account,
                    today: range(0),
                    week: range(6),
                    month: range(29),
                }
            })
            .collect();
        self.stored = Some(stored);
        views
    }

    /// Every reply in every account's chat files, each message once, the first account in order keeping a copied one.
    fn scan(&mut self, accounts: &[(AccountId, PathBuf)], offset_minutes: i32) -> Days {
        let mut seen: HashSet<String> = HashSet::new();
        let mut days = Days::new();
        let mut present: HashSet<PathBuf> = HashSet::new();
        for (account, config_dir) in accounts {
            for (path, len, modified) in chat_files(config_dir) {
                present.insert(path.clone());
                let Some(replies) = self.replies_of(&path, len, modified) else {
                    continue;
                };
                for reply in replies.iter() {
                    if !seen.insert(reply.id.clone()) {
                        continue;
                    }
                    let day = days::date_of(days::day_of(reply.at_ms, offset_minutes));
                    let key = if reply.fast {
                        format!("{} fast", reply.model)
                    } else {
                        reply.model.clone()
                    };
                    let bucket = days
                        .entry(*account)
                        .or_default()
                        .entry(day)
                        .or_default()
                        .entry(key)
                        .or_insert_with(|| Bucket {
                            tokens: Tokens::default(),
                            cost: Some(0.0),
                        });
                    bucket.tokens.add(&reply.tokens);
                    let cost =
                        prices::cost(&reply.model, reply.fast, &reply.tokens, reply.written_1h);
                    bucket.cost = bucket.cost.zip(cost).map(|(a, b)| a + b);
                }
            }
        }
        self.read.retain(|path, _| present.contains(path));
        days
    }

    fn replies_of(&mut self, path: &Path, len: u64, modified: u64) -> Option<Arc<Vec<Reply>>> {
        if let Some(read) = self.read.get(path)
            && read.len == len
            && read.modified == modified
        {
            return Some(Arc::clone(&read.replies));
        }
        if len > MOST_FILE {
            return None;
        }
        let bytes = fs::read(path).ok()?;
        let replies = Arc::new(read::replies(&bytes));
        self.read.insert(
            path.to_path_buf(),
            Read {
                len,
                modified,
                replies: Arc::clone(&replies),
            },
        );
        Some(replies)
    }
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

/// Keeps the larger of each stored and fresh count, so a total survives the chat file it came from being deleted.
fn merge(stored: &mut Days, fresh: &Days) -> bool {
    let mut changed = false;
    for (account, by_day) in fresh {
        for (day, by_model) in by_day {
            for (model, bucket) in by_model {
                let kept = stored
                    .entry(*account)
                    .or_default()
                    .entry(day.clone())
                    .or_default()
                    .entry(model.clone())
                    .or_insert_with(|| {
                        changed = true;
                        Bucket::default()
                    });
                let before = kept.clone();
                kept.tokens.max(&bucket.tokens);
                kept.cost = match (kept.cost, bucket.cost) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                };
                changed |= *kept != before;
            }
        }
    }
    changed
}

/// Drops days before `oldest`.
fn prune(stored: &mut Days, oldest: &str) -> bool {
    let mut changed = false;
    for by_day in stored.values_mut() {
        let before = by_day.len();
        by_day.retain(|day, _| day.as_str() >= oldest);
        changed |= by_day.len() != before;
    }
    changed
}

/// One row per model over the days from `from` to `to`, the costliest first.
fn rows(
    by_day: Option<&BTreeMap<String, BTreeMap<String, Bucket>>>,
    from: &str,
    to: &str,
) -> Vec<ModelRow> {
    let mut by_model: BTreeMap<String, Bucket> = BTreeMap::new();
    for (_, models) in by_day
        .into_iter()
        .flatten()
        .filter(|(day, _)| day.as_str() >= from && day.as_str() <= to)
    {
        for (model, bucket) in models {
            let sum = by_model.entry(model.clone()).or_insert_with(|| Bucket {
                tokens: Tokens::default(),
                cost: Some(0.0),
            });
            sum.tokens.add(&bucket.tokens);
            sum.cost = sum.cost.zip(bucket.cost).map(|(a, b)| a + b);
        }
    }
    let mut rows: Vec<ModelRow> = by_model
        .into_iter()
        .map(|(key, bucket)| {
            let (id, fast) = key
                .strip_suffix(" fast")
                .map_or((key.as_str(), false), |id| (id, true));
            let name = prices::name_of(id);
            ModelRow {
                model: if fast { format!("{name} (fast)") } else { name },
                tokens: bucket.tokens,
                cost: bucket.cost,
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.cost
            .unwrap_or(-1.0)
            .total_cmp(&a.cost.unwrap_or(-1.0))
            .then(b.tokens.total().cmp(&a.tokens.total()))
    });
    rows
}

/// The stored totals, and whether this build may write them: a newer build's file is shown from fresh counts and left alone.
fn load(file: &Path) -> (Days, bool) {
    let Ok(text) = fs::read_to_string(file) else {
        return (Days::new(), true);
    };
    match serde_json::from_str::<Stored>(&text) {
        Ok(stored) if stored.version <= STORE_VERSION => (stored.days, true),
        Ok(_) => (Days::new(), false),
        Err(_) => {
            // Set aside rather than overwritten, so nothing kept is lost to a bad write.
            let _ = fs::rename(file, file.with_extension("json.unreadable"));
            eprintln!("tokens: the daily totals file can't be read; it was set aside");
            (Days::new(), true)
        }
    }
}

fn save(file: &Path, days: &Days) -> io::Result<()> {
    let stored = Stored {
        version: STORE_VERSION,
        days: days.clone(),
    };
    let text = serde_json::to_string(&stored)?;
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let temp = file.with_extension("json.saving");
    fs::write(&temp, text)?;
    fs::rename(&temp, file)
}

#[cfg(test)]
mod tests;
