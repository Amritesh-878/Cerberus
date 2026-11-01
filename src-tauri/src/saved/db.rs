use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension, params};

use crate::saved::{SavedProject, SavedTab, SavedTree, Snapshot, TabId, TreeError};

/// SQLite reads `%`, `?` and `#` in a URI path as syntax, so they are escaped.
fn unlocked_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut uri = String::from(if text.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for c in text.chars() {
        match c {
            '%' => uri.push_str("%25"),
            '?' => uri.push_str("%3f"),
            '#' => uri.push_str("%23"),
            c => uri.push(c),
        }
    }
    uri.push_str("?immutable=1");
    uri
}

// Forward only: a merged step is never edited, and each one runs once, in order.
const MIGRATIONS: [&str; 1] = ["
    CREATE TABLE projects (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        folder TEXT NOT NULL,
        sort INTEGER NOT NULL
    );
    CREATE TABLE tabs (
        id INTEGER PRIMARY KEY,
        project_id INTEGER NOT NULL,
        parent_id INTEGER,
        title TEXT NOT NULL,
        program TEXT NOT NULL,
        cwd TEXT,
        account_id INTEGER,
        session_id TEXT,
        sort INTEGER NOT NULL
    );
    CREATE TABLE meta (
        key TEXT PRIMARY KEY,
        value INTEGER NOT NULL
    );
"];

pub struct Db(Connection);

impl Db {
    pub fn open(path: &Path) -> Result<Self, TreeError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let connection = Connection::open(path)?;
        // A second window learns at once that the file is taken, instead of after rusqlite's 5 s default wait.
        connection.busy_timeout(Duration::ZERO)?;
        // Held until the app exits, so a second Cerberus cannot replace this one's tabs with its own.
        connection.pragma_update(None, "locking_mode", "EXCLUSIVE")?;
        connection
            .execute_batch("BEGIN EXCLUSIVE; COMMIT;")
            .map_err(|e| match e.sqlite_error_code() {
                Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => TreeError::InUse,
                _ => TreeError::Db(e),
            })?;
        let mut db = Self(connection);
        db.migrate()?;
        Ok(db)
    }

    /// Reads past the running app's exclusive lock: `immutable` takes no lock and ignores the journal.
    pub fn read_unlocked(path: &Path) -> Result<SavedTree, TreeError> {
        if !path.exists() {
            return Ok(SavedTree::default());
        }
        let connection = Connection::open_with_flags(
            unlocked_uri(path),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        match usize::try_from(version) {
            Ok(0) => Ok(SavedTree::default()),
            Ok(done) if done == MIGRATIONS.len() => Ok(Self(connection).load()?.tree),
            _ => Err(TreeError::UnknownVersion(version)),
        }
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self, TreeError> {
        let mut db = Self(Connection::open_in_memory()?);
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&mut self) -> Result<(), TreeError> {
        let version: i64 = self
            .0
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let done = usize::try_from(version).unwrap_or(usize::MAX);
        if done > MIGRATIONS.len() {
            return Err(TreeError::UnknownVersion(version));
        }
        for (step_number, step) in (1_i64..).zip(MIGRATIONS).skip(done) {
            let tx = self.0.transaction()?;
            tx.execute_batch(step)?;
            tx.pragma_update(None, "user_version", step_number)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn load(&self) -> Result<Snapshot, TreeError> {
        let projects = self
            .0
            .prepare("SELECT id, name, folder FROM projects ORDER BY sort")?
            .query_map([], |row| {
                Ok(SavedProject {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    folder: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut sessions = HashMap::new();
        let tabs = self
            .0
            .prepare(
                "SELECT id, project_id, parent_id, title, program, cwd, account_id, session_id
                 FROM tabs ORDER BY sort",
            )?
            .query_map([], |row| {
                let tab = SavedTab {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    parent_id: row.get(2)?,
                    title: row.get(3)?,
                    program: row.get(4)?,
                    cwd: row.get(5)?,
                    account: row.get(6)?,
                };
                Ok((tab, row.get::<_, Option<String>>(7)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(tab, session)| {
                if let Some(session) = session {
                    sessions.insert(tab.id, session);
                }
                tab
            })
            .collect();
        let next_id: Option<TabId> = self
            .0
            .query_row("SELECT value FROM meta WHERE key = 'next_id'", [], |row| {
                row.get(0)
            })
            .optional()?;
        let tree = SavedTree {
            projects,
            tabs,
            next_id: next_id.unwrap_or(1),
        };
        tree.validate()?;
        Ok(Snapshot { tree, sessions })
    }

    /// Replaces everything saved with `snapshot` in one transaction, so a crash keeps the old tree whole.
    pub fn write(&mut self, snapshot: &Snapshot) -> Result<(), TreeError> {
        let tx = self.0.transaction()?;
        tx.execute("DELETE FROM tabs", [])?;
        tx.execute("DELETE FROM projects", [])?;
        {
            let mut insert = tx
                .prepare("INSERT INTO projects (id, name, folder, sort) VALUES (?1, ?2, ?3, ?4)")?;
            for (sort, p) in (0_i64..).zip(&snapshot.tree.projects) {
                insert.execute(params![p.id, p.name, p.folder, sort])?;
            }
            let mut insert = tx.prepare(
                "INSERT INTO tabs (id, project_id, parent_id, title, program, cwd, account_id, session_id, sort)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for (sort, t) in (0_i64..).zip(&snapshot.tree.tabs) {
                insert.execute(params![
                    t.id,
                    t.project_id,
                    t.parent_id,
                    t.title,
                    t.program,
                    t.cwd,
                    t.account,
                    snapshot.sessions.get(&t.id),
                    sort
                ])?;
            }
        }
        tx.execute(
            "INSERT INTO meta (key, value) VALUES ('next_id', ?1)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [snapshot.tree.next_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    #[cfg(test)]
    pub fn connection(&self) -> &Connection {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::testdir::TestDir;
    use crate::saved::Saved;
    use crate::saved::tests::{SESSION, two_projects};

    #[test]
    fn a_tree_survives_a_round_trip_in_order() {
        let mut db = Db::in_memory().unwrap();
        let mut snapshot = Snapshot {
            tree: two_projects(),
            sessions: HashMap::from([(3, SESSION.to_owned())]),
        };
        snapshot.tree.tabs.swap(0, 2);

        db.write(&snapshot).unwrap();

        assert_eq!(db.load().unwrap(), snapshot);
    }

    #[test]
    fn a_new_file_is_migrated_to_the_latest_version() {
        let db = Db::in_memory().unwrap();

        let version: i64 = db
            .connection()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();

        assert_eq!(version, i64::try_from(MIGRATIONS.len()).unwrap());
        assert_eq!(
            db.load().unwrap(),
            Snapshot {
                tree: SavedTree {
                    next_id: 1,
                    ..SavedTree::default()
                },
                sessions: HashMap::new(),
            }
        );
    }

    #[test]
    fn reopening_a_file_keeps_its_tabs_and_migrates_nothing_twice() {
        let dir = TestDir::new();
        let path = dir.path().join("tree.db");
        let snapshot = Snapshot {
            tree: two_projects(),
            sessions: HashMap::new(),
        };
        Db::open(&path).unwrap().write(&snapshot).unwrap();

        let reopened = Db::open(&path).unwrap();

        assert_eq!(reopened.load().unwrap(), snapshot);
    }

    #[test]
    fn the_tree_reads_while_the_app_holds_the_file() {
        let dir = TestDir::new();
        let folder = dir.path().join("odd 100% #1");
        let path = folder.join("tree.db");
        let mut held = Db::open(&path).unwrap();
        held.write(&Snapshot {
            tree: two_projects(),
            sessions: HashMap::new(),
        })
        .unwrap();

        assert!(matches!(Db::open(&path), Err(TreeError::InUse)));
        assert_eq!(Db::read_unlocked(&path).unwrap(), two_projects());
    }

    #[test]
    fn a_missing_or_unmigrated_file_reads_as_empty() {
        let dir = TestDir::new();
        let path = dir.path().join("tree.db");

        assert_eq!(Db::read_unlocked(&path).unwrap(), SavedTree::default());
        let other = Connection::open(&path).unwrap();
        other.execute_batch("CREATE TABLE unrelated (x);").unwrap();
        assert_eq!(Db::read_unlocked(&path).unwrap(), SavedTree::default());
        other.pragma_update(None, "user_version", 99).unwrap();
        assert!(matches!(
            Db::read_unlocked(&path),
            Err(TreeError::UnknownVersion(99))
        ));
    }

    #[test]
    fn a_file_from_a_newer_app_is_an_error() {
        let dir = TestDir::new();
        let path = dir.path().join("tree.db");
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();

        let saved = Saved::open(&path, |_| {});

        assert!(matches!(saved.load(), Err(TreeError::Unavailable(_))));
        assert!(
            !dir.path().join("tree.db.bad").exists(),
            "a newer file was set aside"
        );
        assert!(saved.save(two_projects()).is_err());
    }
}
