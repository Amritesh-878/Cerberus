//! The saved services (`services.json` beside the profiles), read and written whole.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type ServiceId = u32;

const MOST_NAME: usize = 80;
const MOST_COMMAND: usize = 8 * 1024;

/// A command may carry a token, so it never prints in a debug line, a panic or an error.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Command(String);

impl Command {
    pub fn new(text: &str) -> Result<Self, StoreError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(StoreError::NoCommand);
        }
        if text.len() > MOST_COMMAND || text.contains(['\0', '\n', '\r']) {
            return Err(StoreError::BadCommand);
        }
        Ok(Self(text.to_owned()))
    }

    pub fn text(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<command>")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: ServiceId,
    pub name: String,
    pub command: Command,
    /// `None` runs in the home folder.
    #[serde(default)]
    pub folder: Option<PathBuf>,
    /// A shell program id, as tabs save it; `None` follows the Shell setting.
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub start_with_app: bool,
    #[serde(default = "on")]
    pub restart: bool,
}

fn on() -> bool {
    true
}

/// What the form sends to add or change one.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub name: String,
    pub command: String,
    pub folder: Option<PathBuf>,
    pub shell: Option<String>,
    pub start_with_app: bool,
    pub restart: bool,
}

impl fmt::Debug for Draft {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Draft")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("a service needs a name")]
    NoName,
    #[error("a service's name is at most {MOST_NAME} characters")]
    LongName,
    #[error("another service is named {0}")]
    Taken(String),
    #[error("a service needs a command")]
    NoCommand,
    // Says what is wrong without echoing the command, which may hold a token.
    #[error("the command must be one line of at most {MOST_COMMAND} bytes")]
    BadCommand,
    #[error("that folder does not exist")]
    NoFolder,
    #[error("no such service")]
    Gone,
    #[error("services not saved: {0}")]
    Io(#[from] io::Error),
}

impl Serialize for StoreError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    #[serde(default)]
    pub services: Vec<Service>,
    /// The sidebar section is folded.
    #[serde(default)]
    pub folded: bool,
    #[serde(default)]
    pub next_id: ServiceId,
}

impl Saved {
    /// Checks a draft against the others; `editing` is the service being changed, if any.
    pub fn checked(
        &self,
        draft: &Draft,
        editing: Option<ServiceId>,
    ) -> Result<Service, StoreError> {
        let name = draft.name.trim();
        if name.is_empty() {
            return Err(StoreError::NoName);
        }
        if name.chars().count() > MOST_NAME {
            return Err(StoreError::LongName);
        }
        if self
            .services
            .iter()
            .any(|s| Some(s.id) != editing && s.name.eq_ignore_ascii_case(name))
        {
            return Err(StoreError::Taken(name.to_owned()));
        }
        let folder = draft.folder.clone().filter(|f| !f.as_os_str().is_empty());
        if folder.as_deref().is_some_and(|f| !f.is_dir()) {
            return Err(StoreError::NoFolder);
        }
        Ok(Service {
            id: editing.unwrap_or(self.next_id),
            name: name.to_owned(),
            command: Command::new(&draft.command)?,
            folder,
            shell: draft.shell.clone().filter(|s| !s.is_empty()),
            start_with_app: draft.start_with_app,
            restart: draft.restart,
        })
    }
}

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join("services.json"),
        }
    }

    /// A missing file is no services; an entry that doesn't read is skipped, the rest kept.
    pub fn load(&self) -> Saved {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Saved::default(),
            Err(e) => {
                eprintln!("services: could not read the saved list: {}", e.kind());
                return Saved::default();
            }
        };
        let Ok(Value::Object(mut file)) = serde_json::from_slice::<Value>(&bytes) else {
            eprintln!("services: the saved list is not readable, so it starts empty");
            return Saved::default();
        };
        let entries = match file.remove("services") {
            Some(Value::Array(entries)) => entries,
            _ => Vec::new(),
        };
        let mut saved: Saved = serde_json::from_value(Value::Object(file)).unwrap_or_default();
        let mut skipped = 0;
        for entry in entries {
            match serde_json::from_value::<Service>(entry) {
                Ok(service)
                    if !saved.services.iter().any(|s| s.id == service.id)
                        && Command::new(service.command.text()).is_ok() =>
                {
                    saved.services.push(service);
                }
                _ => skipped += 1,
            }
        }
        if skipped > 0 {
            eprintln!("services: skipped {skipped} saved entries that did not read");
        }
        let past_ids = saved.services.iter().map(|s| s.id + 1).max().unwrap_or(0);
        saved.next_id = saved.next_id.max(past_ids);
        saved
    }

    // A crash mid-write leaves only the temp file broken; the rename swaps the whole file at once.
    pub fn save(&self, saved: &Saved) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let bytes = serde_json::to_vec_pretty(saved).map_err(io::Error::other)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::testdir::TestDir;

    const SECRET: &str = "cloudflared tunnel --token FAKE-SECRET-token-0000";

    fn draft(name: &str, command: &str) -> Draft {
        Draft {
            name: name.into(),
            command: command.into(),
            folder: None,
            shell: None,
            start_with_app: true,
            restart: true,
        }
    }

    #[test]
    fn the_list_survives_a_round_trip() {
        let dir = TestDir::new();
        let store = Store::new(dir.path());
        let mut saved = Saved::default();
        let service = saved.checked(&draft(" tunnel ", SECRET), None).unwrap();
        saved.services.push(service);
        saved.next_id = 1;
        saved.folded = true;

        store.save(&saved).unwrap();

        assert_eq!(store.load(), saved);
        assert_eq!(store.load().services[0].name, "tunnel");
    }

    #[test]
    fn bad_entries_are_skipped_and_the_rest_kept() {
        let dir = TestDir::new();
        fs::write(
            dir.path().join("services.json"),
            r#"{"folded": true, "nextId": 2, "services": [
                {"id": 0, "name": "ok", "command": "npm run dev"},
                {"id": 1, "name": "no command"},
                {"id": 0, "name": "same id", "command": "x"},
                {"id": 5, "name": "two lines", "command": "a\nb"},
                "not even an object",
                {"id": 7, "name": "later", "command": "uvicorn app:app", "startWithApp": true}
            ]}"#,
        )
        .unwrap();

        let saved = Store::new(dir.path()).load();

        let names: Vec<_> = saved.services.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["ok", "later"]);
        assert!(saved.folded);
        assert_eq!(saved.next_id, 8, "never reuses a saved id");
        assert!(saved.services[0].restart, "restart is on unless saved off");
    }

    #[test]
    fn a_broken_or_missing_file_is_an_empty_list() {
        let dir = TestDir::new();
        assert_eq!(Store::new(dir.path()).load(), Saved::default());
        fs::write(dir.path().join("services.json"), "{ nope").unwrap();
        assert_eq!(Store::new(dir.path()).load(), Saved::default());
    }

    #[test]
    fn the_command_is_in_no_debug_line_or_error() {
        let saved = Saved::default();
        let service = saved.checked(&draft("t", SECRET), None).unwrap();
        let bad = saved
            .checked(&draft("t", &format!("{SECRET}\nrm")), None)
            .unwrap_err();
        let long = saved
            .checked(&draft("t", &SECRET.repeat(1000)), None)
            .unwrap_err();

        for text in [
            format!("{service:?}"),
            format!("{:?}", draft("t", SECRET)),
            bad.to_string(),
            format!("{bad:?}"),
            long.to_string(),
        ] {
            assert!(!text.contains("FAKE-SECRET"), "{text}");
        }
        assert_eq!(service.command.text(), SECRET);
    }

    #[test]
    fn a_draft_needs_a_free_name_a_command_and_a_real_folder() {
        let mut saved = Saved::default();
        saved
            .services
            .push(saved.checked(&draft("web", "npm run dev"), None).unwrap());

        assert!(matches!(
            saved.checked(&draft("  ", "x"), None),
            Err(StoreError::NoName)
        ));
        assert!(matches!(
            saved.checked(&draft("WEB", "x"), None),
            Err(StoreError::Taken(_))
        ));
        assert!(
            saved.checked(&draft("web", "npm start"), Some(0)).is_ok(),
            "its own name is fine"
        );
        assert!(matches!(
            saved.checked(&draft("api", " "), None),
            Err(StoreError::NoCommand)
        ));
        let mut elsewhere = draft("api", "x");
        elsewhere.folder = Some(PathBuf::from("/no/such/folder/here"));
        assert!(matches!(
            saved.checked(&elsewhere, None),
            Err(StoreError::NoFolder)
        ));
    }
}
