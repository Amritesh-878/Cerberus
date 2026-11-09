//! The `--settings` file a tab's `claude` gets: one status line that feeds the app and runs his.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::live::helper::PASS;

// Claude Code refuses a `--settings` file over 2 MiB, so a larger file is not one it reads either.
const MOST_SETTINGS: u64 = 2 * 1024 * 1024;

/// The shell Claude Code runs status line commands in: Git Bash or `sh`, else PowerShell on Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Posix,
    PowerShell,
}

/// The user's own status line, as Claude Code would pick it without Cerberus.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TheirLine {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub padding: Option<Value>,
    #[serde(default)]
    pub refresh_interval: Option<Value>,
    #[serde(default)]
    pub hide_vim_mode_indicator: Option<Value>,
}

// Only `statusLine` is read; every other key, `env` and hooks included, is skipped unkept.
#[derive(Deserialize)]
struct SettingsIn {
    #[serde(default, rename = "statusLine")]
    status_line: Option<Value>,
}

/// The files that could set his status line, highest precedence first.
pub fn their_files(cwd: &Path, config_dir: &Path) -> [PathBuf; 3] {
    let project = cwd.join(".claude");
    [
        project.join("settings.local.json"),
        project.join("settings.json"),
        config_dir.join("settings.json"),
    ]
}

/// The first of `files` that sets a status line; an unreadable file is skipped, as Claude Code would.
pub fn their_line(files: &[PathBuf]) -> Option<TheirLine> {
    files.iter().find_map(|file| {
        let size = fs::metadata(file).ok()?.len();
        if size > MOST_SETTINGS {
            return None;
        }
        let parsed: SettingsIn = serde_json::from_slice(&fs::read(file).ok()?).ok()?;
        // A `statusLine` that is not an object sets nothing, so a lower file may still.
        serde_json::from_value(parsed.status_line?).ok()
    })
}

/// Cerberus's status line command, or `None` when adding one would break his.
pub fn command(helper: &Path, shell: Shell, theirs: Option<&str>) -> Option<String> {
    let theirs = theirs.map(str::trim).filter(|c| !c.is_empty());
    match (shell, theirs) {
        (Shell::Posix, None) => Some(format!("{} status", posix_quote(helper))),
        // The group keeps `&&`, `;` and a trailing comment in his command; the newline ends a comment.
        (Shell::Posix, Some(theirs)) => Some(format!(
            "{} status {PASS} | {{ {theirs}\n}}",
            posix_quote(helper)
        )),
        (Shell::PowerShell, None) => Some(format!("& {} status", powershell_quote(helper))),
        // A PowerShell pipe re-encodes text between programs, so his command would not get the same bytes.
        (Shell::PowerShell, Some(_)) => None,
    }
}

fn posix_quote(path: &Path) -> String {
    // Git Bash takes `C:/…`; backslashes would need escaping it does not do inside single quotes.
    let text = path.to_string_lossy().replace('\\', "/");
    format!("'{}'", text.replace('\'', r"'\''"))
}

fn powershell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
}

/// The settings object: Cerberus's command with his display keys, so his line looks as it did.
pub fn settings(command: &str, theirs: Option<&TheirLine>) -> Value {
    let mut line = Map::new();
    line.insert("type".into(), json!("command"));
    line.insert("command".into(), json!(command));
    if let Some(theirs) = theirs {
        for (key, value) in [
            ("padding", &theirs.padding),
            ("refreshInterval", &theirs.refresh_interval),
            ("hideVimModeIndicator", &theirs.hide_vim_mode_indicator),
        ] {
            if let Some(value) = value {
                line.insert(key.into(), value.clone());
            }
        }
    }
    json!({ "statusLine": line })
}

pub fn write(path: &Path, settings: &Value) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, serde_json::to_vec_pretty(settings)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::testdir::TestDir;

    #[test]
    fn the_highest_file_with_a_status_line_wins_and_nothing_else_is_read() {
        let dir = TestDir::new();
        let cwd = dir.path().join("work");
        let home = dir.path().join("home");
        fs::create_dir_all(cwd.join(".claude")).unwrap();
        fs::create_dir_all(&home).unwrap();
        fs::write(
            home.join("settings.json"),
            r#"{"statusLine":{"type":"command","command":"user-line"},
                "env":{"API_KEY":"FAKE-SECRET-sk-000"}}"#,
        )
        .unwrap();
        fs::write(
            cwd.join(".claude/settings.json"),
            r#"{"statusLine":{"type":"command","command":"project-line","padding":2,"refreshInterval":5}}"#,
        )
        .unwrap();
        // Broken, so Claude Code would skip it and so does this.
        fs::write(cwd.join(".claude/settings.local.json"), "{ nope").unwrap();

        let line = their_line(&their_files(&cwd, &home)).unwrap();

        assert_eq!(line.command.as_deref(), Some("project-line"));
        assert_eq!(line.padding, Some(json!(2)));
        assert_eq!(line.refresh_interval, Some(json!(5)));
        assert!(!format!("{line:?}").contains("FAKE-SECRET"));
    }

    #[test]
    fn no_status_line_anywhere_is_none() {
        let dir = TestDir::new();
        fs::write(dir.path().join("settings.json"), r#"{"statusLine":"odd"}"#).unwrap();

        assert_eq!(their_line(&their_files(dir.path(), dir.path())), None);
    }

    #[test]
    fn with_no_line_of_his_the_helper_runs_alone() {
        let helper = Path::new(r"C:\Users\Jo O'Neil\AppData\Local\Cerberus\cerberus-live.exe");

        assert_eq!(
            command(helper, Shell::Posix, None).unwrap(),
            r"'C:/Users/Jo O'\''Neil/AppData/Local/Cerberus/cerberus-live.exe' status"
        );
        assert_eq!(
            command(helper, Shell::PowerShell, Some("  ")).unwrap(),
            r"& 'C:\Users\Jo O''Neil\AppData\Local\Cerberus\cerberus-live.exe' status"
        );
    }

    #[test]
    fn his_line_reads_what_the_helper_passes_on() {
        let helper = Path::new("/Applications/Cerberus.app/Contents/MacOS/cerberus-live");

        assert_eq!(
            command(
                helper,
                Shell::Posix,
                Some("jq -r .model.id && echo hi # mine")
            )
            .unwrap(),
            "'/Applications/Cerberus.app/Contents/MacOS/cerberus-live' status --pass \
             | { jq -r .model.id && echo hi # mine\n}"
        );
        assert_eq!(command(helper, Shell::PowerShell, Some("mine")), None);
    }

    #[test]
    fn his_display_keys_are_kept_and_the_file_holds_only_the_status_line() {
        let theirs = TheirLine {
            command: Some("mine".into()),
            padding: Some(json!(1)),
            refresh_interval: None,
            hide_vim_mode_indicator: Some(json!(true)),
        };

        let written = settings("cmd", Some(&theirs));

        assert_eq!(
            written,
            json!({"statusLine": {"type": "command", "command": "cmd", "padding": 1,
                "hideVimModeIndicator": true}})
        );
        assert_eq!(
            settings("cmd", None),
            json!({"statusLine": {"type": "command", "command": "cmd"}})
        );
    }
}
