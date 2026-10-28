use std::fs::{self, File};
use std::io::{self, ErrorKind, Write};
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::accounts::{AccountId, AccountsError, Profiles};

const VERSION: u32 = 2;

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
        profiles.revision += 1;
        self.save(&profiles)?;
        Ok(profiles)
    }

    /// Keeps the old file beside the new start, in case its accounts are worth recovering by hand.
    pub fn reset(&self) -> Result<Profiles, AccountsError> {
        let _writing = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        let unused = (1..)
            .map(|n| match n {
                1 => self.path.with_extension("json.bad"),
                n => self.path.with_extension(format!("json.bad{n}")),
            })
            .find(|backup| !backup.exists());
        let Some(backup) = unused else {
            return Err(io::Error::other("no free backup name").into());
        };
        match fs::rename(&self.path, backup) {
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
        // Named per process, so two app instances never write into one temp file.
        let temp = self
            .path
            .with_extension(format!("json.{}.tmp", std::process::id()));
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
    let mut saved: Value = serde_json::from_slice(bytes).map_err(corrupt)?;
    match version {
        1 => upgrade_from_v1(&mut saved),
        VERSION => {}
        other => return Err(AccountsError::UnknownVersion(other)),
    }
    let Saved {
        next_id,
        mut profiles,
    } = serde_json::from_value(saved).map_err(corrupt)?;
    profiles.next_id = next_id;
    profiles.validate().map_err(|e| match e {
        AccountsError::Corrupt(_) => e,
        other => AccountsError::Corrupt(other.to_string()),
    })?;
    Ok(profiles)
}

// Version 1 had a default program and folder picked in the walkthrough; they keep their values.
fn upgrade_from_v1(saved: &mut Value) {
    let Some(fields) = saved.as_object_mut() else {
        return;
    };
    for (old, new) in [
        ("defaultProgram", "lastProgram"),
        ("defaultCwd", "startFolder"),
    ] {
        if let Some(value) = fields.remove(old) {
            fields.insert(new.to_owned(), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
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
                p.set_default_account(1)?;
                p.last_program = Some("claude".to_owned());
                p.set_start_folder(Some(PathBuf::from(r"C:\code")))
            })
            .unwrap();

        let loaded = store_in(&dir).load().unwrap();

        assert_eq!(loaded, saved);
        assert_eq!(loaded.next_id, 2);
        assert_eq!(loaded.last_program.as_deref(), Some("claude"));
    }

    #[test]
    fn a_write_replaces_the_file_whole_and_leaves_no_temp_file() {
        let dir = TestDir::new();
        let store = store_in(&dir);
        let app = dir.path().join("app");
        add(&store, "one", r"C:\h\.claude-one");
        fs::write(app.join("profiles.json.1.tmp"), b"a crashed write").unwrap();

        add(&store, "two", r"C:\h\.claude-two");

        let files: Vec<_> = fs::read_dir(&app)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(files, ["profiles.json", "profiles.json.1.tmp"]);
        assert_eq!(store.load().unwrap().accounts.len(), 2);
    }

    #[test]
    fn every_save_raises_the_revision() {
        let dir = TestDir::new();
        let store = store_in(&dir);

        let first = add(&store, "one", r"C:\h\.claude-one");
        let second = add(&store, "two", r"C:\h\.claude-two");

        assert_eq!((first.revision, second.revision), (1, 2));
        assert_eq!(store_in(&dir).load().unwrap().revision, 2);
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
        let text = r#"{"version":2,"nextId":2,"accounts":[
            {"id":0,"name":"a","configDir":"C:\\h\\.claude"},
            {"id":1,"name":"A","configDir":"C:\\h\\.claude-2"}
        ],"defaultAccount":0,"lastProgram":null,"startFolder":null}"#;

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
    fn a_version_1_file_keeps_its_program_and_folder() {
        let dir = TestDir::new();
        let text = r#"{"version":1,"nextId":1,"accounts":[
            {"id":0,"name":"a","configDir":"C:\\h\\.claude"}
        ],"defaultAccount":0,"defaultProgram":"powershell","defaultCwd":"C:\\code"}"#;
        let store = write_raw(&dir, text);

        let loaded = store.load().unwrap();
        let saved = store.update(|_| Ok(())).unwrap();

        assert_eq!(loaded.last_program.as_deref(), Some("powershell"));
        assert_eq!(loaded.start_folder, Some(PathBuf::from(r"C:\code")));
        assert_eq!(saved.revision, loaded.revision + 1);
        assert_eq!(
            Profiles {
                revision: 0,
                ..saved
            },
            loaded
        );
        let raw = fs::read_to_string(dir.path().join("profiles.json")).unwrap();
        assert!(raw.contains(r#""version": 2"#) && !raw.contains("defaultCwd"));
    }

    #[test]
    fn reset_sets_the_bad_file_aside() {
        let dir = TestDir::new();
        let store = write_raw(&dir, "{ not json");

        assert_eq!(store.reset().unwrap(), Profiles::default());

        assert_eq!(store.load().unwrap(), Profiles::default());
        assert!(Path::new(&dir.path().join("profiles.json.bad")).is_file());
    }

    #[test]
    fn a_second_reset_keeps_the_first_backup() {
        let dir = TestDir::new();
        let store = write_raw(&dir, "first");
        store.reset().unwrap();
        fs::write(dir.path().join("profiles.json"), "second").unwrap();

        store.reset().unwrap();

        let read = |name: &str| fs::read_to_string(dir.path().join(name)).unwrap();
        assert_eq!(read("profiles.json.bad"), "first");
        assert_eq!(read("profiles.json.bad2"), "second");
    }
}
