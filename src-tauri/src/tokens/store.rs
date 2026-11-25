//! Cerberus's own record of replies' token counts: one row per reply, kept 90 days, so totals outlive deleted chats.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::accounts::AccountId;
use crate::tokens::read::Reply;
use crate::tokens::{Recorded, Tokens};

const VERSION: i64 = 1;
const REPLIES: &str = "
    CREATE TABLE replies (
        id INTEGER PRIMARY KEY,
        account INTEGER NOT NULL,
        at_ms INTEGER NOT NULL,
        model TEXT NOT NULL,
        fast INTEGER NOT NULL,
        input INTEGER NOT NULL,
        output INTEGER NOT NULL,
        cache_read INTEGER NOT NULL,
        cache_write INTEGER NOT NULL,
        cost REAL
    );
    CREATE INDEX replies_at ON replies (at_ms);
";
// How far each chat file has been recorded, so a restart reads only what changed.
const FILES: &str = "
    CREATE TABLE files (
        path TEXT PRIMARY KEY,
        len INTEGER NOT NULL,
        modified INTEGER NOT NULL,
        offset INTEGER NOT NULL
    );
";

/// Which ranges a total covers: today is in all three, the last week in two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    Today,
    Week,
    Month,
}

/// One account's tokens for one model in one band; `cost` is `None` when any reply in it had no price.
#[derive(Debug, Clone, PartialEq)]
pub struct Total {
    pub account: AccountId,
    pub model: String,
    pub fast: bool,
    pub band: Band,
    pub tokens: Tokens,
    pub cost: Option<f64>,
}

/// One chat file's new replies and how far that leaves it recorded.
pub struct Batch {
    pub path: PathBuf,
    pub recorded: Recorded,
    pub account: AccountId,
    pub replies: Vec<(Reply, Option<f64>)>,
}

pub struct Store {
    connection: Connection,
}

/// A stable 64-bit FNV-1a hash of a message id, so the file holds no ids and a reply is still recognised.
fn key_of(id: &str) -> i64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in id.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    i64::from_ne_bytes(hash.to_ne_bytes())
}

fn signed(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn unsigned(n: i64) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

impl Store {
    /// Opens or makes the store. A newer build's file is left alone and this run keeps its counts in memory; an unreadable one is set aside.
    pub fn open(file: &Path) -> Option<Self> {
        match Self::open_file(file) {
            Ok(Some(store)) => Some(store),
            Ok(None) => Self::in_memory(),
            Err(e) => {
                eprintln!("tokens: the reply store can't be read ({e}); it was set aside");
                let _ = fs::rename(file, file.with_extension("db.unreadable"));
                Self::open_file(file)
                    .ok()
                    .flatten()
                    .or_else(Self::in_memory)
            }
        }
    }

    fn open_file(file: &Path) -> rusqlite::Result<Option<Self>> {
        if let Some(dir) = file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let connection = Connection::open(file)?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > VERSION {
            return Ok(None);
        }
        // Counts can be read again from the chats, so a cheap commit matters more than surviving a power cut.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        if version == 0 {
            connection.execute_batch(REPLIES)?;
            connection.execute_batch(FILES)?;
            connection.pragma_update(None, "user_version", VERSION)?;
        }
        Ok(Some(Self { connection }))
    }

    fn in_memory() -> Option<Self> {
        let connection = Connection::open_in_memory().ok()?;
        connection.execute_batch(REPLIES).ok()?;
        connection.execute_batch(FILES).ok()?;
        Some(Self { connection })
    }

    /// How far each chat file was recorded by earlier runs.
    pub fn progress(&self) -> HashMap<PathBuf, Recorded> {
        let read = || -> rusqlite::Result<HashMap<PathBuf, Recorded>> {
            let mut query = self
                .connection
                .prepare("SELECT path, len, modified, offset FROM files")?;
            let rows = query.query_map([], |row| {
                Ok((
                    PathBuf::from(row.get::<_, String>(0)?),
                    Recorded {
                        len: unsigned(row.get(1)?),
                        modified: unsigned(row.get(2)?),
                        offset: unsigned(row.get(3)?),
                    },
                ))
            })?;
            rows.collect()
        };
        read().unwrap_or_else(|e| {
            eprintln!("tokens: could not read how far chats were recorded: {e}");
            HashMap::new()
        })
    }

    /// Records new replies and the files' new places in one commit; a kept reply stays with its first account and only grows.
    pub fn record(&mut self, batches: &[Batch]) -> rusqlite::Result<()> {
        let tx = self.connection.transaction()?;
        {
            let mut upsert = tx.prepare_cached(
                "INSERT INTO replies (id, account, at_ms, model, fast, input, output, cache_read, cache_write, cost)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT (id) DO UPDATE SET
                    input = max(input, excluded.input),
                    output = max(output, excluded.output),
                    cache_read = max(cache_read, excluded.cache_read),
                    cache_write = max(cache_write, excluded.cache_write),
                    cost = CASE WHEN cost IS NULL OR excluded.cost IS NULL THEN NULL ELSE max(cost, excluded.cost) END
                 WHERE account = excluded.account",
            )?;
            let mut place = tx.prepare_cached(
                "INSERT INTO files (path, len, modified, offset) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (path) DO UPDATE SET len = excluded.len, modified = excluded.modified, offset = excluded.offset",
            )?;
            for batch in batches {
                for (reply, cost) in &batch.replies {
                    upsert.execute(params![
                        key_of(&reply.id),
                        batch.account,
                        signed(reply.at_ms),
                        reply.model,
                        reply.fast,
                        signed(reply.tokens.input),
                        signed(reply.tokens.output),
                        signed(reply.tokens.cache_read),
                        signed(reply.tokens.cache_write),
                        cost,
                    ])?;
                }
                place.execute(params![
                    batch.path.to_string_lossy(),
                    signed(batch.recorded.len),
                    signed(batch.recorded.modified),
                    signed(batch.recorded.offset),
                ])?;
            }
        }
        tx.commit()
    }

    /// Forgets the places of chat files that are gone; their replies stay.
    pub fn forget(&mut self, paths: &[PathBuf]) -> rusqlite::Result<()> {
        let tx = self.connection.transaction()?;
        {
            let mut delete = tx.prepare_cached("DELETE FROM files WHERE path = ?1")?;
            for path in paths {
                delete.execute([path.to_string_lossy()])?;
            }
        }
        tx.commit()
    }

    /// Each account's totals per model in one pass: from `month_ms`, split where the week and today start, up to `end_ms`.
    pub fn totals(
        &self,
        month_ms: u64,
        week_ms: u64,
        today_ms: u64,
        end_ms: u64,
    ) -> rusqlite::Result<Vec<Total>> {
        let mut query = self.connection.prepare_cached(
            "SELECT account, model, fast,
                    CASE WHEN at_ms >= ?3 THEN 0 WHEN at_ms >= ?2 THEN 1 ELSE 2 END AS band,
                    sum(input), sum(output), sum(cache_read), sum(cache_write),
                    CASE WHEN count(cost) = count(*) THEN sum(cost) END
             FROM replies WHERE at_ms >= ?1 AND at_ms < ?4
             GROUP BY account, model, fast, band",
        )?;
        let rows = query.query_map(
            params![
                signed(month_ms),
                signed(week_ms),
                signed(today_ms),
                signed(end_ms)
            ],
            |row| {
                let band = match row.get::<_, i64>(3)? {
                    0 => Band::Today,
                    1 => Band::Week,
                    _ => Band::Month,
                };
                Ok(Total {
                    account: row.get(0)?,
                    model: row.get(1)?,
                    fast: row.get(2)?,
                    band,
                    tokens: Tokens {
                        input: unsigned(row.get(4)?),
                        output: unsigned(row.get(5)?),
                        cache_read: unsigned(row.get(6)?),
                        cache_write: unsigned(row.get(7)?),
                    },
                    cost: row.get(8)?,
                })
            },
        )?;
        rows.collect()
    }

    /// Drops replies from before `before_ms`; how many went.
    pub fn prune(&mut self, before_ms: u64) -> rusqlite::Result<usize> {
        self.connection
            .execute("DELETE FROM replies WHERE at_ms < ?1", [signed(before_ms)])
    }

    #[cfg(test)]
    pub fn break_writes(&self) {
        self.connection.execute_batch("DROP TABLE replies").unwrap();
    }

    #[cfg(test)]
    pub fn mend_writes(&self) {
        self.connection.execute_batch(REPLIES).unwrap();
    }

    #[cfg(test)]
    pub fn owner_of(&self, id: &str) -> Option<AccountId> {
        use rusqlite::OptionalExtension;
        self.connection
            .query_row(
                "SELECT account FROM replies WHERE id = ?1",
                [key_of(id)],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten()
    }
}
