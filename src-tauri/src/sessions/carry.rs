use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::sessions::{SLUG_MAX, is_session_id, parse_session_file, slug, transcript};

// A copy lands under this suffix first, so a half copy is never mistaken for a chat.
const IN_PROGRESS: &str = "cerberus-copy";
const SET_ASIDE: &str = "cerberus-old";

#[derive(Debug, thiserror::Error)]
pub enum CarryError {
    #[error("the chat's file is gone from the account it started on")]
    Missing,
    #[error("the other account has this chat open right now, so nothing was copied")]
    OpenOnTarget,
    #[error("another tab or window has this chat open, so it stays where it is")]
    OpenElsewhere,
    #[error("the new folder's path is too long to carry the chat to")]
    LongFolder,
    #[error("could not copy the chat: {0}")]
    Io(#[from] io::Error),
    #[error(
        "could not copy the chat, and part of its older copy stays under a .{SET_ASIDE} name: {0}"
    )]
    NotRestored(io::Error),
}

type FileOp<'a> = &'a mut dyn FnMut(&Path, &Path) -> io::Result<()>;

/// The two file operations a copy makes, so tests can make either fail.
struct Ops<'a> {
    copy: FileOp<'a>,
    rename: FileOp<'a>,
}

/// Copies a chat's transcript and its folder from one config dir to the same project in another.
pub fn copy_chat(
    from: &Path,
    to: &Path,
    cwd: &str,
    id: &str,
    is_running: &dyn Fn(u32) -> bool,
) -> Result<(), CarryError> {
    let ops = Ops {
        copy: &mut |source, target| fs::copy(source, target).map(|_| ()),
        rename: &mut |source, target| fs::rename(source, target),
    };
    copy_chat_with(from, to, cwd, None, id, is_running, ops)
}

/// Copies a chat to another folder's project on the same account, so `--resume` finds it there.
pub fn copy_to_folder(
    config: &Path,
    from_cwd: &str,
    to_cwd: &str,
    id: &str,
    is_running: &dyn Fn(u32) -> bool,
) -> Result<(), CarryError> {
    // Paths that differ only where the slug puts `-` share one project, so the chat is there already.
    let to = slug(to_cwd);
    if to == slug(from_cwd) && to.len() <= SLUG_MAX {
        return Ok(());
    }
    let ops = Ops {
        copy: &mut |source, target| fs::copy(source, target).map(|_| ()),
        rename: &mut |source, target| fs::rename(source, target),
    };
    copy_chat_with(config, config, from_cwd, Some(to_cwd), id, is_running, ops)
}

/// `to_cwd` names another folder's project; `None` keeps the chat's own project.
fn copy_chat_with(
    from: &Path,
    to: &Path,
    cwd: &str,
    to_cwd: Option<&str>,
    id: &str,
    is_running: &dyn Fn(u32) -> bool,
    ops: Ops<'_>,
) -> Result<(), CarryError> {
    if !is_session_id(id) {
        return Err(CarryError::Missing);
    }
    let source = transcript(from, cwd, id).ok_or(CarryError::Missing)?;
    // The same name as the source's, so a long cwd's hashed folder matches without recomputing the hash.
    let project = match to_cwd {
        None => source
            .parent()
            .and_then(Path::file_name)
            .ok_or(CarryError::Missing)?
            .to_owned(),
        // Claude Code hashes a longer path into its folder name, which Cerberus cannot recompute.
        Some(to_cwd) if slug(to_cwd).len() > SLUG_MAX => return Err(CarryError::LongFolder),
        Some(to_cwd) => slug(to_cwd).into(),
    };
    if open_in(to, id, is_running)? {
        return Err(if to_cwd.is_some() {
            CarryError::OpenElsewhere
        } else {
            CarryError::OpenOnTarget
        });
    }
    let target_dir = to.join("projects").join(project);
    fs::create_dir_all(&target_dir)?;
    let temp = target_dir.join(format!("{id}.jsonl.{IN_PROGRESS}"));
    let temp_folder = target_dir.join(format!("{id}.{IN_PROGRESS}"));
    remove(&temp, &temp_folder)?;
    let Ops { copy, rename } = ops;
    let source_folder = source.with_file_name(id);
    let copied = (|| {
        if source_folder.is_dir() {
            copy_tree(&source_folder, &temp_folder, copy)?;
        }
        copy(&source, &temp)
    })();
    if let Err(e) = copied {
        let _ = remove(&temp, &temp_folder);
        return Err(e.into());
    }
    // Every rename is logged, so a failure part way can be undone and the older copy keeps its name.
    let mut done = Vec::new();
    let Err(failed) = replace(&target_dir, id, &temp, &temp_folder, rename, &mut done) else {
        return Ok(());
    };
    let undone = undo(&done, rename);
    let _ = remove(&temp, &temp_folder);
    match undone {
        Ok(()) => Err(failed.into()),
        Err(_) => Err(CarryError::NotRestored(failed)),
    }
}

/// Moves any older copy aside, then the new chat in: its folder first, its transcript last.
fn replace(
    dir: &Path,
    id: &str,
    temp: &Path,
    temp_folder: &Path,
    rename: FileOp<'_>,
    done: &mut Vec<(PathBuf, PathBuf)>,
) -> io::Result<()> {
    let target = dir.join(format!("{id}.jsonl"));
    let target_folder = dir.join(id);
    if target.exists() {
        set_aside(dir, id, rename, done)?;
    } else if target_folder.exists() {
        // A folder with no transcript is what a copy cut short between its two renames leaves.
        fs::remove_dir_all(&target_folder)?;
    }
    if temp_folder.exists() {
        logged(rename, temp_folder, &target_folder, done)?;
    }
    // Last, so the transcript, which is what `--resume` looks for, appears only once all is in place.
    logged(rename, temp, &target, done)
}

fn logged(
    rename: FileOp<'_>,
    from: &Path,
    to: &Path,
    done: &mut Vec<(PathBuf, PathBuf)>,
) -> io::Result<()> {
    rename(from, to)?;
    done.push((from.to_path_buf(), to.to_path_buf()));
    Ok(())
}

/// Reverses logged renames, newest first; it tries them all and reports the first that failed.
fn undo(done: &[(PathBuf, PathBuf)], rename: FileOp<'_>) -> io::Result<()> {
    let mut first_error = None;
    for (from, to) in done.iter().rev() {
        if let Err(e) = rename(to, from) {
            first_error.get_or_insert(e);
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// Whether a running `claude` on that config dir has this chat open; only a missing file means "no".
fn open_in(config_dir: &Path, id: &str, is_running: &dyn Fn(u32) -> bool) -> io::Result<bool> {
    let entries = match fs::read_dir(config_dir.join("sessions")) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let path = entry?.path();
        let pid = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.parse::<u32>().ok());
        let Some(pid) = pid else { continue };
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            // The chat ended between the listing and the read.
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let has_chat =
            parse_session_file(&bytes).is_some_and(|file| file.session_id.as_deref() == Some(id));
        if has_chat && is_running(pid) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Keeps the target's older copy of the chat, and its folder, under the first free `.cerberus-old` name.
fn set_aside(
    dir: &Path,
    id: &str,
    rename: FileOp<'_>,
    done: &mut Vec<(PathBuf, PathBuf)>,
) -> io::Result<()> {
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
        logged(rename, &current_folder, &folder, done)?;
    }
    logged(rename, &dir.join(format!("{id}.jsonl")), &file, done)
}

fn copy_tree(from: &Path, to: &Path, copy: FileOp<'_>) -> io::Result<()> {
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
            None,
            ID,
            &|_| false,
            Ops {
                copy: &mut failing_second,
                rename: &mut |a, b| fs::rename(a, b),
            },
        );

        assert!(matches!(result, Err(CarryError::Io(_))));
        let target = dir.path().join("work").join("projects").join(slug(CWD));
        assert_eq!(listing(&target), Vec::<String>::new());
    }

    /// The target holds an older copy of the chat and its folder, as after a move there and back.
    fn older_copy_on_work(dir: &TestDir) -> PathBuf {
        let target = dir.path().join("work").join("projects").join(slug(CWD));
        fs::create_dir_all(target.join(ID)).unwrap();
        fs::write(target.join(format!("{ID}.jsonl")), "older copy").unwrap();
        fs::write(target.join(ID).join("old.txt"), "older folder").unwrap();
        target
    }

    /// Copies with renames that fail on the calls numbered in `failing`.
    fn carry_failing_renames(dir: &TestDir, failing: &[usize]) -> Result<(), CarryError> {
        let mut calls = 0;
        let mut rename = |from: &Path, to: &Path| {
            calls += 1;
            if failing.contains(&calls) {
                return Err(io::Error::other("rename refused (fake)"));
            }
            fs::rename(from, to)
        };
        copy_chat_with(
            &dir.path().join("personal"),
            &dir.path().join("work"),
            CWD,
            None,
            ID,
            &|_| false,
            Ops {
                copy: &mut |a, b| fs::copy(a, b).map(|_| ()),
                rename: &mut rename,
            },
        )
    }

    #[test]
    fn a_failed_rename_at_any_step_puts_the_older_copy_back() {
        // Four renames: the older folder aside, the older file aside, the new folder in, the new file in.
        for step in 1..=4 {
            let dir = TestDir::new();
            chat_in(&dir, "personal", CWD);
            let target = older_copy_on_work(&dir);
            let before = listing(&target);

            let result = carry_failing_renames(&dir, &[step]);

            assert!(matches!(result, Err(CarryError::Io(_))), "step {step}");
            assert_eq!(listing(&target), before, "step {step}");
            assert_eq!(
                fs::read_to_string(target.join(format!("{ID}.jsonl"))).unwrap(),
                "older copy",
                "step {step}"
            );
        }
    }

    #[test]
    fn a_put_back_that_fails_part_way_says_so_and_keeps_what_it_could() {
        let dir = TestDir::new();
        chat_in(&dir, "personal", CWD);
        let target = older_copy_on_work(&dir);

        // The new file fails to land, then moving the new folder back out fails too.
        let result = carry_failing_renames(&dir, &[4, 5]);

        assert!(matches!(result, Err(CarryError::NotRestored(_))));
        assert_eq!(
            fs::read_to_string(target.join(format!("{ID}.jsonl"))).unwrap(),
            "older copy"
        );
        assert!(
            target
                .join(format!("{ID}.{SET_ASIDE}"))
                .join("old.txt")
                .is_file()
        );
    }

    #[test]
    fn an_unreadable_sessions_folder_stops_the_copy() {
        let dir = TestDir::new();
        chat_in(&dir, "personal", CWD);
        fs::create_dir_all(dir.path().join("work")).unwrap();
        fs::write(dir.path().join("work").join("sessions"), "not a folder").unwrap();

        let result = carry(&dir, CWD);

        assert!(matches!(result, Err(CarryError::Io(_))));
        assert!(!dir.path().join("work").join("projects").exists());
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

    #[test]
    fn a_folder_that_shares_the_chats_project_copies_nothing() {
        let dir = TestDir::new();
        let project = chat_in(&dir, "personal", CWD);
        let config = dir.path().join("personal");
        let before = fs::read_dir(&project).unwrap().count();

        copy_to_folder(&config, CWD, r"C:\work-app", ID, &|_| false).unwrap();

        assert_eq!(fs::read_dir(&project).unwrap().count(), before);
    }

    #[test]
    fn a_chat_is_copied_to_another_folders_project_on_the_same_account() {
        let dir = TestDir::new();
        chat_in(&dir, "personal", CWD);
        let config = dir.path().join("personal");
        let new_cwd = r"C:\work\plans";

        copy_to_folder(&config, CWD, new_cwd, ID, &|_| false).unwrap();

        assert!(transcript(&config, new_cwd, ID).is_some());
        assert!(
            transcript(&config, CWD, ID).is_some(),
            "the old folder lost its chat"
        );
        let moved = config.join("projects").join(slug(new_cwd)).join(ID);
        assert!(moved.join("subagents").join("agent-1.jsonl").is_file());
    }

    #[test]
    fn a_chat_open_elsewhere_or_a_too_long_folder_is_not_copied() {
        let dir = TestDir::new();
        chat_in(&dir, "personal", CWD);
        let config = dir.path().join("personal");
        let long = format!(r"C:\{}", "a".repeat(SLUG_MAX));

        assert!(matches!(
            copy_to_folder(&config, CWD, &long, ID, &|_| false),
            Err(CarryError::LongFolder)
        ));
        fs::create_dir_all(config.join("sessions")).unwrap();
        fs::write(
            config.join("sessions").join("4242.json"),
            format!(r#"{{"pid":4242,"sessionId":"{ID}"}}"#),
        )
        .unwrap();
        assert!(matches!(
            copy_to_folder(&config, CWD, r"C:\work\plans", ID, &|pid| pid == 4242),
            Err(CarryError::OpenElsewhere)
        ));
    }
}
