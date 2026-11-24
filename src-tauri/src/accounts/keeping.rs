//! How long Claude Code keeps an account's chats: one key of the folder's `settings.json`, read and written alone.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use serde::de::{Deserializer, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Serialize};

pub const KEY: &str = "cleanupPeriodDays";
/// Claude Code's own default when the key is unset.
pub const DEFAULT_DAYS: u32 = 30;
/// Below this the account row says when chats go.
pub const SHOWN_UNDER: u32 = 365;
/// What Keep them writes: a hundred years.
pub const KEEP_DAYS: u32 = 36_500;
// A settings file is a few KiB; a far larger one is not one to edit by hand.
const MOST_FILE: u64 = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum KeepError {
    #[error("could not read settings.json: {0}")]
    Read(io::Error),
    #[error("settings.json is not plain JSON, so Cerberus left it alone")]
    NotJson,
    #[error("could not save settings.json: {0}")]
    Write(io::Error),
}

/// An account whose chats Claude Code deletes, and after how many days since their last use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Deletes {
    pub id: crate::accounts::AccountId,
    pub days: u32,
}

/// The key's value, read with every other value skipped unkept; the last of repeated keys wins, as in Claude Code.
struct Only(Option<u32>);

#[derive(Deserialize)]
#[serde(untagged)]
enum Lenient {
    Number(u64),
    Other(IgnoredAny),
}

impl<'de> Deserialize<'de> for Only {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_map(OnlyVisitor)
    }
}

struct OnlyVisitor;

impl<'de> Visitor<'de> for OnlyVisitor {
    type Value = Only;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a settings object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Only, A::Error> {
        let mut days = None;
        while let Some(key) = map.next_key::<String>()? {
            if key == KEY {
                // A value Claude Code could not read counts as unset, so its default applies.
                days = match map.next_value::<Lenient>()? {
                    Lenient::Number(n) => Some(u32::try_from(n).unwrap_or(u32::MAX)),
                    Lenient::Other(_) => None,
                };
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(Only(days))
    }
}

/// The days Claude Code keeps chats for, or `None` when the file can't be read as JSON.
pub fn kept_days(file: &Path) -> Option<u32> {
    match read(file) {
        Ok(None) => Some(DEFAULT_DAYS),
        Ok(Some(text)) => serde_json::from_str::<Only>(strip_bom(&text))
            .ok()
            .map(|only| only.0.unwrap_or(DEFAULT_DAYS)),
        Err(_) => None,
    }
}

/// Sets the key to keep chats, leaving every other key, the order and the spacing as they were.
pub fn keep(file: &Path) -> Result<(), KeepError> {
    let edited = match read(file).map_err(KeepError::Read)? {
        None => format!("{{\n  \"{KEY}\": {KEEP_DAYS}\n}}\n"),
        Some(text) => with_kept(&text).ok_or(KeepError::NotJson)?,
    };
    let temp = fresh_temp(file).map_err(KeepError::Write)?;
    let written = fs::OpenOptions::new()
        .write(true)
        .open(&temp)
        .and_then(|mut out| out.write_all(edited.as_bytes()))
        .and_then(|()| fs::rename(&temp, file));
    written.map_err(|e| {
        let _ = fs::remove_file(&temp);
        KeepError::Write(e)
    })
}

/// A new, empty file beside `file` that no other save shares, so two saves at once can't write into each other's.
fn fresh_temp(file: &Path) -> io::Result<PathBuf> {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    loop {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let temp = file.with_extension(format!("json.cerberus-{}-{n}", std::process::id()));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(_) => return Ok(temp),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
}

fn read(file: &Path) -> io::Result<Option<String>> {
    match fs::metadata(file) {
        Ok(meta) if meta.len() > MOST_FILE => Err(io::Error::other("the file is too large")),
        Ok(_) => fs::read_to_string(file).map(Some),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// The file's text with the key set, or `None` when it is not one JSON object.
fn with_kept(text: &str) -> Option<String> {
    let body = strip_bom(text);
    serde_json::from_str::<IgnoredAny>(body).ok()?;
    let offset = text.len() - body.len();
    let object = members(body)?;
    let value = KEEP_DAYS.to_string();
    let mut out = text.to_owned();
    let ours: Vec<_> = object.members.iter().filter(|m| m.key == KEY).collect();
    if !ours.is_empty() {
        // Claude Code reads the last of repeated keys, so every one is set alike.
        for member in ours.iter().rev() {
            out.replace_range(offset + member.value.0..offset + member.value.1, &value);
        }
        return Some(out);
    }
    let added = format!("\"{KEY}\": {value}");
    match object.members.last() {
        Some(last) => {
            let gap = &body[object.open + 1..object.members[0].key_at];
            out.insert_str(offset + last.value.1, &format!(",{gap}{added}"));
        }
        None => {
            let inner = &body[object.open + 1..object.close];
            let newline = if inner.contains("\r\n") { "\r\n" } else { "\n" };
            out.replace_range(
                offset + object.open + 1..offset + object.close,
                &format!("{newline}  {added}{newline}"),
            );
        }
    }
    Some(out)
}

struct Member {
    key: String,
    key_at: usize,
    /// Byte range of the value.
    value: (usize, usize),
}

struct Object {
    open: usize,
    close: usize,
    members: Vec<Member>,
}

/// Where each top-level key and value sits in text that serde already read as valid JSON.
fn members(text: &str) -> Option<Object> {
    let bytes = text.as_bytes();
    let mut at = skip_space(bytes, 0);
    if bytes.get(at) != Some(&b'{') {
        return None;
    }
    let open = at;
    at = skip_space(bytes, at + 1);
    let mut members = Vec::new();
    if bytes.get(at) == Some(&b'}') {
        return Some(Object {
            open,
            close: at,
            members,
        });
    }
    loop {
        let key_at = at;
        let key_end = string_end(bytes, at)?;
        let key: String = serde_json::from_str(&text[key_at..key_end]).ok()?;
        at = skip_space(bytes, key_end);
        if bytes.get(at) != Some(&b':') {
            return None;
        }
        let start = skip_space(bytes, at + 1);
        let end = value_end(bytes, start)?;
        members.push(Member {
            key,
            key_at,
            value: (start, end),
        });
        at = skip_space(bytes, end);
        match bytes.get(at) {
            Some(b',') => at = skip_space(bytes, at + 1),
            Some(b'}') => {
                return Some(Object {
                    open,
                    close: at,
                    members,
                });
            }
            _ => return None,
        }
    }
}

fn skip_space(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

/// The index just past the string that opens at `at`.
fn string_end(bytes: &[u8], at: usize) -> Option<usize> {
    if bytes.get(at) != Some(&b'"') {
        return None;
    }
    let mut i = at + 1;
    while let Some(&b) = bytes.get(i) {
        match b {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

/// The index just past the value that starts at `at`.
fn value_end(bytes: &[u8], at: usize) -> Option<usize> {
    match bytes.get(at)? {
        b'"' => string_end(bytes, at),
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut i = at;
            while let Some(&b) = bytes.get(i) {
                match b {
                    b'"' => {
                        i = string_end(bytes, i)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i + 1);
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            None
        }
        _ => {
            let mut i = at;
            while bytes
                .get(i)
                .is_some_and(|b| !b.is_ascii_whitespace() && !b",}]".contains(b))
            {
                i += 1;
            }
            (i > at).then_some(i)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::testdir::TestDir;

    fn kept(text: &str) -> String {
        with_kept(text).unwrap()
    }

    #[test]
    fn an_unset_key_reads_as_claude_codes_default() {
        let dir = TestDir::new();
        let file = dir.path().join("settings.json");

        assert_eq!(kept_days(&file), Some(DEFAULT_DAYS));
        fs::write(&file, r#"{"model": "opus", "permissions": {"allow": []}}"#).unwrap();
        assert_eq!(kept_days(&file), Some(DEFAULT_DAYS));
        fs::write(&file, r#"{"cleanupPeriodDays": 36500}"#).unwrap();
        assert_eq!(kept_days(&file), Some(36_500));
        fs::write(&file, r#"{"cleanupPeriodDays": "lots"}"#).unwrap();
        assert_eq!(kept_days(&file), Some(DEFAULT_DAYS));
        fs::write(&file, "{ not json").unwrap();
        assert_eq!(kept_days(&file), None);
        fs::write(
            &file,
            r#"{"cleanupPeriodDays": 7, "env": {}, "cleanupPeriodDays": 9}"#,
        )
        .unwrap();
        assert_eq!(kept_days(&file), Some(9), "the last of repeated keys wins");
        fs::write(&file, "[30]").unwrap();
        assert_eq!(kept_days(&file), None);
    }

    #[test]
    fn a_set_key_changes_in_place_and_nothing_else_moves() {
        let before = "{\n    \"model\": \"opus\",\n    \"cleanupPeriodDays\": 30,\n    \"hooks\": {\"Stop\": [\"x,}\"]}\n}\n";

        assert_eq!(
            kept(before),
            "{\n    \"model\": \"opus\",\n    \"cleanupPeriodDays\": 36500,\n    \"hooks\": {\"Stop\": [\"x,}\"]}\n}\n"
        );
    }

    #[test]
    fn a_missing_key_is_added_last_with_the_files_own_spacing() {
        let before = "{\r\n\t\"model\": \"opus\",\r\n\t\"env\": {\"A\": \"}\"}\r\n}";

        assert_eq!(
            kept(before),
            "{\r\n\t\"model\": \"opus\",\r\n\t\"env\": {\"A\": \"}\"},\r\n\t\"cleanupPeriodDays\": 36500\r\n}"
        );
        assert_eq!(kept(r#"{"a":1}"#), r#"{"a":1,"cleanupPeriodDays": 36500}"#);
        assert_eq!(kept("{}"), "{\n  \"cleanupPeriodDays\": 36500\n}");
    }

    #[test]
    fn a_byte_order_mark_and_repeated_keys_are_kept_alike() {
        let before = "\u{feff}{\"cleanupPeriodDays\": 7, \"cleanupPeriodDays\": 9}";

        assert_eq!(
            kept(before),
            "\u{feff}{\"cleanupPeriodDays\": 36500, \"cleanupPeriodDays\": 36500}"
        );
    }

    #[test]
    fn a_key_inside_another_object_is_not_the_setting() {
        let before = r#"{"env": {"cleanupPeriodDays": 1}}"#;

        assert_eq!(
            kept(before),
            r#"{"env": {"cleanupPeriodDays": 1},"cleanupPeriodDays": 36500}"#
        );
    }

    #[test]
    fn what_is_not_one_object_is_left_alone() {
        for text in ["[1]", "{\"a\": 1", "// note\n{}", ""] {
            assert_eq!(with_kept(text), None, "{text}");
        }
    }

    #[test]
    fn keep_writes_the_file_or_makes_one() {
        let dir = TestDir::new();
        let file = dir.path().join("settings.json");

        keep(&file).unwrap();
        assert_eq!(kept_days(&file), Some(KEEP_DAYS));

        fs::write(&file, "{\"model\": \"opus\"}\n").unwrap();
        keep(&file).unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "{\"model\": \"opus\",\"cleanupPeriodDays\": 36500}\n"
        );

        fs::write(&file, "{ broken").unwrap();
        assert!(matches!(keep(&file), Err(KeepError::NotJson)));
        assert_eq!(fs::read_to_string(&file).unwrap(), "{ broken");
        let left: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(left.len(), 1, "no temp file is left behind");
    }

    #[test]
    fn saves_at_once_each_land_whole() {
        let dir = TestDir::new();
        let file = dir.path().join("settings.json");
        fs::write(&file, "{\"model\": \"opus\"}").unwrap();

        let saves: Vec<_> = (0..8)
            .map(|_| {
                let file = file.clone();
                std::thread::spawn(move || keep(&file))
            })
            .collect();

        for save in saves {
            save.join().unwrap().unwrap();
        }
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "{\"model\": \"opus\",\"cleanupPeriodDays\": 36500}"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
