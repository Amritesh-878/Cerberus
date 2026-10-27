use std::fs::{self, File};
use std::io::{self, ErrorKind, Write};
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use crate::accounts::{AccountId, AccountsError, Profiles};

const VERSION: u32 = 1;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Saving<'a> {
    version: u32,
    next_id: AccountId,
    #[serde(flatten)]
    profiles: &'a Profiles,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    next_id: AccountId,
    #[serde(flatten)]
    profiles: Profiles,
}

#[derive(Deserialize)]
struct Versioned {
    version: u32,
}

pub struct Store {
    path: PathBuf,
    writing: Mutex<()>,
}

impl Store {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            writing: Mutex::new(()),
        }
    }

    pub fn load(&self) -> Result<Profiles, AccountsError> {
        match fs::read(&self.path) {
            Ok(bytes) => parse(&bytes),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(Profiles::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn update(
        &self,
        change: impl FnOnce(&mut Profiles) -> Result<(), AccountsError>,
    ) -> Result<Profiles, AccountsError> {
        let _writing = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        let mut profiles = self.load()?;
        change(&mut profiles)?;
        self.save(&profiles)?;
        Ok(profiles)
    }

    /// Keeps the old file beside the new start, in case its accounts are worth recovering by hand.
    pub fn reset(&self) -> Result<Profiles, AccountsError> {
        let _writing = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        match fs::rename(&self.path, self.path.with_extension("json.bad")) {
            Ok(()) => Ok(Profiles::default()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(Profiles::default()),
            Err(e) => Err(e.into()),
        }
    }

    // A crash mid-write leaves only the temp file broken; the rename swaps the whole file at once.
    fn save(&self, profiles: &Profiles) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let saving = Saving {
            version: VERSION,
            next_id: profiles.next_id,
            profiles,
        };
        let bytes = serde_json::to_vec_pretty(&saving).map_err(io::Error::other)?;
        let temp = self.path.with_extension("json.tmp");
        let mut file = File::create(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &self.path)
    }
}

fn parse(bytes: &[u8]) -> Result<Profiles, AccountsError> {
    let corrupt = |e: serde_json::Error| AccountsError::Corrupt(e.to_string());
    let Versioned { version } = serde_json::from_slice(bytes).map_err(corrupt)?;
    if version != VERSION {
        return Err(AccountsError::UnknownVersion(version));
    }
    let Saved {
        next_id,
        mut profiles,
    } = serde_json::from_slice(bytes).map_err(corrupt)?;
    profiles.next_id = next_id;
    profiles.validate().map_err(|e| match e {
        AccountsError::Corrupt(_) => e,
        other => AccountsError::Corrupt(other.to_string()),
    })?;
    Ok(profiles)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::accounts::Defaults;
    use crate::accounts::testdir::TestDir;

    fn store_in(dir: &TestDir) -> Store {
        Store::new(dir.path().join("app").join("profiles.json"))
    }

    fn add(store: &Store, name: &str, dir: &str) -> Profiles {
        store
            .update(|p| p.add(name, PathBuf::from(dir)).map(drop))
            .unwrap()
    }

    #[test]
    fn a_missing_file_is_a_first_run() {
        let dir = TestDir::new();

        assert_eq!(store_in(&dir).load().unwrap(), Profiles::default());
    }

    #[test]
    fn profiles_survive_a_round_trip() {
        let dir = TestDir::new();
        let store = store_in(&dir);
        add(&store, "personal", r"C:\h\.claude");
        add(&store, "work", r"C:\h\.claude-work");
        let saved = store
            .update(|p| {
                p.set_defaults(Defaults {
                    account: Some(1),
                    program: Some("claude".to_owned()),
                    cwd: Some(PathBuf::from(r"C:\code")),
                })
            })
            .unwrap();

        let loaded = store_in(&dir).load().unwrap();

        assert_eq!(loaded, saved);
        assert_eq!(loaded.next_id, 2);
        assert_eq!(loaded.default_program.as_deref(), Some("claude"));
    }

    #[test]
    fn a_write_replaces_the_file_whole_and_leaves_no_temp_file() {
        let dir = TestDir::new();
        let store = store_in(&dir);
        let app = dir.path().join("app");
        add(&store, "one", r"C:\h\.claude-one");
        fs::write(app.join("profiles.json.tmp"), b"half a write").unwrap();

        add(&store, "two", r"C:\h\.claude-two");

        let files: Vec<_> = fs::read_dir(&app)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(files, ["profiles.json"]);
        assert_eq!(store.load().unwrap().accounts.len(), 2);
    }

    #[test]
    fn a_failed_change_writes_nothing() {
        let dir = TestDir::new();
        let store = store_in(&dir);
        add(&store, "one", r"C:\h\.claude-one");

        let result = store.update(|p| p.add("ONE", PathBuf::from(r"C:\h\x")).map(drop));

        assert!(matches!(result, Err(AccountsError::DuplicateName(_))));
        assert_eq!(store.load().unwrap().accounts.len(), 1);
    }

    fn write_raw(dir: &TestDir, text: &str) -> Store {
        let path = dir.path().join("profiles.json");
        fs::write(&path, text).unwrap();
        Store::new(path)
    }

    #[test]
    fn a_corrupt_file_is_an_error_not_a_crash() {
        let dir = TestDir::new();

        let result = write_raw(&dir, "{ not json").load();

        assert!(matches!(result, Err(AccountsError::Corrupt(_))));
    }

    #[test]
    fn a_file_that_breaks_the_rules_is_corrupt() {
        let dir = TestDir::new();
        let text = r#"{"version":1,"nextId":2,"accounts":[
            {"id":0,"name":"a","configDir":"C:\\h\\.claude"},
            {"id":1,"name":"A","configDir":"C:\\h\\.claude-2"}
        ],"defaultAccount":0,"defaultProgram":null,"defaultCwd":null}"#;

        let result = write_raw(&dir, text).load();

        assert!(matches!(result, Err(AccountsError::Corrupt(_))));
    }

    #[test]
    fn an_unknown_version_is_an_error() {
        let dir = TestDir::new();

        let result = write_raw(&dir, r#"{"version":9,"accounts":"anything"}"#).load();

        assert!(matches!(result, Err(AccountsError::UnknownVersion(9))));
    }

    #[test]
    fn reset_sets_the_bad_file_aside() {
        let dir = TestDir::new();
        let store = write_raw(&dir, "{ not json");

        assert_eq!(store.reset().unwrap(), Profiles::default());

        assert_eq!(store.load().unwrap(), Profiles::default());
        assert!(Path::new(&dir.path().join("profiles.json.bad")).is_file());
    }
}
