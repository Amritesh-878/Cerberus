//! A file the chat wrote, read for the file pane: its text and its `HEAD` version, never anything else.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::Serialize;

/// Larger files are refused; the pane offers VS Code instead.
pub const MOST_FILE: u64 = 2 * 1024 * 1024;
/// A NUL in this much of the start marks a binary file, as git decides.
const SNIFF: usize = 8000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FileView {
    #[serde(rename_all = "camelCase")]
    Text {
        text: String,
        /// The `HEAD` version; `None` outside a repo or for a file git doesn't track, so all of it is new.
        base: Option<String>,
    },
    TooBig {
        size: u64,
    },
    Binary,
    Gone,
}

/// The file and, through `head`, its committed version; `head` runs only for a text file within the limits.
pub fn view(path: &Path, head: impl FnOnce(&Path) -> Option<Vec<u8>>) -> FileView {
    let Ok(info) = fs::metadata(path) else {
        return FileView::Gone;
    };
    if !info.is_file() {
        return FileView::Gone;
    }
    if info.len() > MOST_FILE {
        return FileView::TooBig { size: info.len() };
    }
    // The file can grow after the metadata was read, so the read itself stops past the limit.
    let bytes = match read_at_most(path, MOST_FILE) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return FileView::TooBig {
                size: info.len().max(MOST_FILE + 1),
            };
        }
        Err(_) => return FileView::Gone,
    };
    let Some(text) = text_of(bytes) else {
        return FileView::Binary;
    };
    let base = head(path)
        .filter(|b| b.len() as u64 <= MOST_FILE)
        .and_then(text_of);
    FileView::Text { text, base }
}

/// The file's bytes, or `None` when it holds more than `most`; never reads past `most + 1`.
fn read_at_most(path: &Path, most: u64) -> io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(most + 1).read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= most).then_some(bytes))
}

/// When the file last changed, so the pane reloads only on a change; `None` when it is gone.
pub fn stamp(path: &Path) -> Option<String> {
    let info = fs::metadata(path).ok()?;
    // The size too, since a quick second write can keep the same modified time; text, as a u64 can pass JS's 2^53.
    Some(format!("{}-{}", modified_ms(&info), info.len()))
}

fn modified_ms(info: &fs::Metadata) -> u64 {
    info.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| u64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

fn text_of(bytes: Vec<u8>) -> Option<String> {
    if bytes.iter().take(SNIFF).any(|&b| b == 0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::testdir::TestDir;

    fn no_head(_: &Path) -> Option<Vec<u8>> {
        None
    }

    #[test]
    fn a_text_file_comes_with_its_head_version() {
        let dir = TestDir::new();
        let file = dir.path().join("app.ts");
        fs::write(&file, "export const x = 2;\n").unwrap();

        let shown = view(&file, |_| Some(b"export const x = 1;\n".to_vec()));

        let FileView::Text { text, base, .. } = shown else {
            panic!("not text: {shown:?}");
        };
        assert_eq!(text, "export const x = 2;\n");
        assert_eq!(base.as_deref(), Some("export const x = 1;\n"));
    }

    #[test]
    fn without_a_head_version_the_base_is_none() {
        let dir = TestDir::new();
        let file = dir.path().join("new.md");
        fs::write(&file, "# new\n").unwrap();

        assert!(matches!(
            view(&file, no_head),
            FileView::Text { base: None, .. }
        ));
    }

    #[test]
    fn large_binary_and_gone_files_are_refused_without_asking_git() {
        let dir = TestDir::new();
        let big = dir.path().join("big.log");
        fs::write(&big, vec![b'a'; MOST_FILE as usize + 1]).unwrap();
        let bin = dir.path().join("logo.png");
        fs::write(&bin, [0x89, b'P', b'N', b'G', 0, 1, 2]).unwrap();
        let asked = std::cell::Cell::new(0);
        let head = |_: &Path| {
            asked.set(asked.get() + 1);
            None
        };

        assert_eq!(
            view(&big, head),
            FileView::TooBig {
                size: MOST_FILE + 1
            }
        );
        assert_eq!(view(&bin, head), FileView::Binary);
        assert_eq!(view(&dir.path().join("deleted.ts"), head), FileView::Gone);
        assert_eq!(
            view(dir.path(), head),
            FileView::Gone,
            "a folder is not a file"
        );
        assert_eq!(asked.get(), 0);
    }

    #[test]
    fn a_binary_or_huge_head_version_counts_as_none() {
        let dir = TestDir::new();
        let file = dir.path().join("a.txt");
        fs::write(&file, "text\n").unwrap();

        let binary = view(&file, |_| Some(vec![0, 1, 2]));
        let huge = view(&file, |_| Some(vec![b'a'; MOST_FILE as usize + 1]));

        assert!(matches!(binary, FileView::Text { base: None, .. }));
        assert!(matches!(huge, FileView::Text { base: None, .. }));
    }

    #[test]
    fn a_read_stops_past_the_limit() {
        let dir = TestDir::new();
        let file = dir.path().join("a.txt");
        fs::write(&file, "abcd").unwrap();

        assert_eq!(
            read_at_most(&file, 4).unwrap().as_deref(),
            Some(&b"abcd"[..])
        );
        assert_eq!(read_at_most(&file, 3).unwrap(), None);
    }

    #[test]
    fn the_stamp_changes_when_the_file_does() {
        let dir = TestDir::new();
        let file = dir.path().join("a.txt");
        fs::write(&file, "one").unwrap();
        let first = stamp(&file);

        fs::write(&file, "one two").unwrap();

        assert!(first.is_some());
        assert_ne!(stamp(&file), first);
        assert_eq!(stamp(&dir.path().join("nope")), None);
    }
}
