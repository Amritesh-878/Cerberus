use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::sessions::{is_session_id, parse_session_file, transcript};

// A copy lands under this suffix first, so a half copy is never mistaken for a chat.
const IN_PROGRESS: &str = "cerberus-copy";
const SET_ASIDE: &str = "cerberus-old";

#[derive(Debug, thiserror::Error)]
pub enum CarryError {
    #[error("the chat's file is gone from the account it started on")]
    Missing,
    #[error("the other account has this chat open right now, so nothing was copied")]
    OpenOnTarget,
    #[error("could not copy the chat: {0}")]
    Io(#[from] io::Error),
}

/// Copies a chat's transcript and its folder from one config dir to the same project in another.
pub fn copy_chat(
    from: &Path,
    to: &Path,
    cwd: &str,
    id: &str,
    is_running: &dyn Fn(u32) -> bool,
) -> Result<(), CarryError> {
    copy_chat_with(from, to, cwd, id, is_running, &mut |source, target| {
        fs::copy(source, target).map(|_| ())
    })
}

fn copy_chat_with(
    from: &Path,
    to: &Path,
    cwd: &str,
    id: &str,
    is_running: &dyn Fn(u32) -> bool,
    copy: &mut dyn FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<(), CarryError> {
    if !is_session_id(id) {
        return Err(CarryError::Missing);
    }
    let source = transcript(from, cwd, id).ok_or(CarryError::Missing)?;
    // The same name as the source's, so a long cwd's hashed folder matches without recomputing the hash.
    let project = source
        .parent()
        .and_then(Path::file_name)
        .ok_or(CarryError::Missing)?;
    if open_in(to, id, is_running) {
        return Err(CarryError::OpenOnTarget);
    }
    let target_dir = to.join("projects").join(project);
    let target = target_dir.join(format!("{id}.jsonl"));
    let target_folder = target_dir.join(id);
    fs::create_dir_all(&target_dir)?;
    let temp = target_dir.join(format!("{id}.jsonl.{IN_PROGRESS}"));
    let temp_folder = target_dir.join(format!("{id}.{IN_PROGRESS}"));
    remove(&temp, &temp_folder)?;
    if target.exists() {
        set_aside(&target_dir, id)?;
    } else if target_folder.exists() {
        // A folder with no transcript is what a copy cut short between its two renames leaves.
        fs::remove_dir_all(&target_folder)?;
    }
    let source_folder = source.with_file_name(id);
    let mut folder_moved = false;
    let copied = (|| {
        if source_folder.is_dir() {
            copy_tree(&source_folder, &temp_folder, copy)?;
        }
        copy(&source, &temp)?;
        if temp_folder.exists() {
            fs::rename(&temp_folder, &target_folder)?;
            folder_moved = true;
        }
        // Last, so the transcript, which is what `--resume` looks for, appears only once all is in place.
        fs::rename(&temp, &target)
    })();
    if let Err(e) = copied {
        let _ = remove(&temp, &temp_folder);
        if folder_moved {
            let _ = fs::remove_dir_all(&target_folder);
        }
        return Err(e.into());
    }
    Ok(())
}

/// Whether a running `claude` on that config dir has this chat open.
fn open_in(config_dir: &Path, id: &str, is_running: &dyn Fn(u32) -> bool) -> bool {
    let Ok(entries) = fs::read_dir(config_dir.join("sessions")) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        let path = entry.path();
        let pid = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.parse::<u32>().ok());
        let file = fs::read(&path)
            .ok()
            .and_then(|bytes| parse_session_file(&bytes));
        match (pid, file) {
            (Some(pid), Some(file)) => file.session_id.as_deref() == Some(id) && is_running(pid),
            _ => false,
        }
    })
}

/// Keeps the target's older copy of the chat, and its folder, under the first free `.cerberus-old` name.
fn set_aside(dir: &Path, id: &str) -> io::Result<()> {
    let names = |n: u32| -> (PathBuf, PathBuf) {
        let suffix = if n == 1 {
            SET_ASIDE.to_owned()
        } else {
            format!("{SET_ASIDE}{n}")
        };
        (
            dir.join(format!("{id}.jsonl.{suffix}")),
            dir.join(format!("{id}.{suffix}")),
        )
    };
    let (file, folder) = (1..1000)
        .map(names)
        .find(|(file, folder)| !file.exists() && !folder.exists())
        .ok_or_else(|| io::Error::other("no free name to keep the older copy under"))?;
    let current_folder = dir.join(id);
    if current_folder.exists() {
        fs::rename(&current_folder, folder)?;
    }
    fs::rename(dir.join(format!("{id}.jsonl")), file)
}

fn copy_tree(
    from: &Path,
    to: &Path,
    copy: &mut dyn FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target, copy)?;
        } else {
            copy(&entry.path(), &target)?;
        }
    }
    Ok(())
}

// Leftovers from a copy that a crash cut short.
fn remove(file: &Path, folder: &Path) -> io::Result<()> {
    for result in [fs::remove_file(file), fs::remove_dir_all(folder)] {
        match result {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::accounts::testdir::TestDir;
    use crate::sessions::slug;

    const ID: &str = "0f6c3a52-7d1e-4b8a-9c2f-5e4d3b2a1f00";
    const CWD: &str = r"C:\work\app";

    /// A fake chat in `dir/<config>`: a transcript and a subagent file, with fake text only.
    fn chat_in(dir: &TestDir, config: &str, cwd: &str) -> PathBuf {
        let project = dir.path().join(config).join("projects").join(slug(cwd));
        fs::create_dir_all(project.join(ID).join("subagents")).unwrap();
        fs::write(project.join(format!("{ID}.jsonl")), "{\"fake\":\"chat\"}\n").unwrap();
        fs::write(
            project.join(ID).join("subagents").join("agent-1.jsonl"),
            "{\"fake\":\"agent\"}\n",
        )
        .unwrap();
        project
    }

    fn carry(dir: &TestDir, cwd: &str) -> Result<(), CarryError> {
        copy_chat(
            &dir.path().join("personal"),
            &dir.path().join("work"),
            cwd,
            ID,
            &|_| false,
        )
    }

    fn listing(dir: &Path) -> Vec<String> {
        let mut names = Vec::new();
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().unwrap().is_dir() {
                names.extend(
                    listing(&entry.path())
                        .into_iter()
                        .map(|n| format!("{name}/{n}")),
                );
            } else {
                names.push(name);
            }
        }
        names.sort();
        names
    }

    #[test]
    fn the_chat_and_its_folder_land_in_the_same_project_and_the_source_stays() {
        let dir = TestDir::new();
        let source = chat_in(&dir, "personal", CWD);

        carry(&dir, CWD).unwrap();

        let target = dir.path().join("work").join("projects").join(slug(CWD));
        assert_eq!(listing(&target), listing(&source));
        assert_eq!(
            fs::read(target.join(format!("{ID}.jsonl"))).unwrap(),
            fs::read(source.join(format!("{ID}.jsonl"))).unwrap()
        );
        assert!(source.join(format!("{ID}.jsonl")).is_file());
    }

    #[test]
    fn an_older_copy_in_the_target_is_kept_aside_and_the_chat_copied_over_it() {
        let dir = TestDir::new();
        let source = chat_in(&dir, "personal", CWD);
        let target = dir.path().join("work").join("projects").join(slug(CWD));
        fs::create_dir_all(target.join(ID)).unwrap();
        fs::write(target.join(format!("{ID}.jsonl")), "older copy").unwrap();
        fs::write(target.join(ID).join("old.txt"), "older folder").unwrap();

        carry(&dir, CWD).unwrap();

        assert_eq!(
            fs::read(target.join(format!("{ID}.jsonl"))).unwrap(),
            fs::read(source.join(format!("{ID}.jsonl"))).unwrap()
        );
        assert_eq!(
            fs::read_to_string(target.join(format!("{ID}.jsonl.{SET_ASIDE}"))).unwrap(),
            "older copy"
        );
        assert_eq!(
            fs::read_to_string(target.join(format!("{ID}.{SET_ASIDE}")).join("old.txt")).unwrap(),
            "older folder"
        );
    }

    #[test]
    fn a_chat_bounced_back_twice_keeps_both_older_copies() {
        let dir = TestDir::new();
        chat_in(&dir, "personal", CWD);
        let target = dir.path().join("work").join("projects").join(slug(CWD));
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join(format!("{ID}.jsonl")), "first").unwrap();

        carry(&dir, CWD).unwrap();
        carry(&dir, CWD).unwrap();

        assert_eq!(
            fs::read_to_string(target.join(format!("{ID}.jsonl.{SET_ASIDE}"))).unwrap(),
            "first"
        );
        assert!(target.join(format!("{ID}.jsonl.{SET_ASIDE}2")).is_file());
    }

    #[test]
    fn a_folder_left_between_the_two_renames_does_not_block_a_retry() {
        let dir = TestDir::new();
        let source = chat_in(&dir, "personal", CWD);
        let target = dir.path().join("work").join("projects").join(slug(CWD));
        fs::create_dir_all(target.join(ID)).unwrap();
        fs::write(target.join(ID).join("stray.jsonl"), "half").unwrap();

        carry(&dir, CWD).unwrap();

        assert_eq!(listing(&target), listing(&source));
    }

    fn open_on_work(dir: &TestDir, pid: u32) {
        let sessions = dir.path().join("work").join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        fs::write(
            sessions.join(format!("{pid}.json")),
            format!(r#"{{"pid":{pid},"sessionId":"{ID}","status":"idle"}}"#),
        )
        .unwrap();
    }

    #[test]
    fn a_chat_open_on_the_target_right_now_is_refused() {
        let dir = TestDir::new();
        chat_in(&dir, "personal", CWD);
        open_on_work(&dir, 4242);

        let result = copy_chat(
            &dir.path().join("personal"),
            &dir.path().join("work"),
            CWD,
            ID,
            &|pid| pid == 4242,
        );

        assert!(matches!(result, Err(CarryError::OpenOnTarget)));
        assert!(!dir.path().join("work").join("projects").exists());
    }

    #[test]
    fn a_session_file_left_by_a_closed_chat_does_not_block_the_copy() {
        let dir = TestDir::new();
        chat_in(&dir, "personal", CWD);
        open_on_work(&dir, 4242);

        assert!(carry(&dir, CWD).is_ok());
    }

    #[test]
    fn a_missing_source_copies_nothing() {
        let dir = TestDir::new();

        let result = carry(&dir, CWD);

        assert!(matches!(result, Err(CarryError::Missing)));
        assert!(!dir.path().join("work").exists());
    }

    #[test]
    fn a_copy_cut_short_leaves_no_chat_and_no_scraps() {
        let dir = TestDir::new();
        chat_in(&dir, "personal", CWD);
        let mut copies = 0;
        let mut failing_second = |source: &Path, target: &Path| {
            copies += 1;
            if copies == 2 {
                return Err(io::Error::other("disk full (fake)"));
            }
            fs::copy(source, target).map(|_| ())
        };

        let result = copy_chat_with(
            &dir.path().join("personal"),
            &dir.path().join("work"),
            CWD,
            ID,
            &|_| false,
            &mut failing_second,
        );

        assert!(matches!(result, Err(CarryError::Io(_))));
        let target = dir.path().join("work").join("projects").join(slug(CWD));
        assert_eq!(listing(&target), Vec::<String>::new());
    }

    #[test]
    fn scraps_from_an_earlier_crash_are_replaced() {
        let dir = TestDir::new();
        let source = chat_in(&dir, "personal", CWD);
        let target = dir.path().join("work").join("projects").join(slug(CWD));
        fs::create_dir_all(target.join(format!("{ID}.{IN_PROGRESS}"))).unwrap();
        fs::write(target.join(format!("{ID}.jsonl.{IN_PROGRESS}")), "half").unwrap();

        carry(&dir, CWD).unwrap();

        assert_eq!(listing(&target), listing(&source));
    }

    #[test]
    fn a_long_cwd_keeps_the_source_projects_hashed_folder_name() {
        let dir = TestDir::new();
        let cwd = format!(r"C:\{}", r"nested\deeper.dir\with space\".repeat(10));
        let hashed = format!("{}-k3x9", &slug(&cwd)[..200]);
        let source = dir.path().join("personal").join("projects").join(&hashed);
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join(format!("{ID}.jsonl")), "{}\n").unwrap();

        carry(&dir, &cwd).unwrap();

        assert!(
            dir.path()
                .join("work")
                .join("projects")
                .join(hashed)
                .join(format!("{ID}.jsonl"))
                .is_file()
        );
    }
}
