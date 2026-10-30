use std::fs;
use std::io;
use std::path::Path;

use crate::sessions::{is_session_id, transcript};

// A copy lands under this suffix first, so a half copy is never mistaken for a chat.
const IN_PROGRESS: &str = "cerberus-copy";

#[derive(Debug, thiserror::Error)]
pub enum CarryError {
    #[error("the chat's file is gone from the account it started on")]
    Missing,
    #[error("the other account already has this chat, so nothing was copied")]
    Exists,
    #[error("could not copy the chat: {0}")]
    Io(#[from] io::Error),
}

/// Copies a chat's transcript and its folder from one config dir to the same project in another.
pub fn copy_chat(from: &Path, to: &Path, cwd: &str, id: &str) -> Result<(), CarryError> {
    copy_chat_with(from, to, cwd, id, &mut |source, target| {
        fs::copy(source, target).map(|_| ())
    })
}

fn copy_chat_with(
    from: &Path,
    to: &Path,
    cwd: &str,
    id: &str,
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
    let target_dir = to.join("projects").join(project);
    let target = target_dir.join(format!("{id}.jsonl"));
    let target_folder = target_dir.join(id);
    if target.exists() || target_folder.exists() {
        return Err(CarryError::Exists);
    }
    fs::create_dir_all(&target_dir)?;
    let temp = target_dir.join(format!("{id}.jsonl.{IN_PROGRESS}"));
    let temp_folder = target_dir.join(format!("{id}.{IN_PROGRESS}"));
    remove(&temp, &temp_folder)?;
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

        copy_chat(
            &dir.path().join("personal"),
            &dir.path().join("work"),
            CWD,
            ID,
        )
        .unwrap();

        let target = dir.path().join("work").join("projects").join(slug(CWD));
        assert_eq!(listing(&target), listing(&source));
        assert_eq!(
            fs::read(target.join(format!("{ID}.jsonl"))).unwrap(),
            fs::read(source.join(format!("{ID}.jsonl"))).unwrap()
        );
        assert!(source.join(format!("{ID}.jsonl")).is_file());
    }

    #[test]
    fn a_chat_the_target_already_has_is_refused_and_left_alone() {
        let dir = TestDir::new();
        chat_in(&dir, "personal", CWD);
        let target = dir.path().join("work").join("projects").join(slug(CWD));
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join(format!("{ID}.jsonl")), "theirs").unwrap();

        let result = copy_chat(
            &dir.path().join("personal"),
            &dir.path().join("work"),
            CWD,
            ID,
        );

        assert!(matches!(result, Err(CarryError::Exists)));
        assert_eq!(
            fs::read_to_string(target.join(format!("{ID}.jsonl"))).unwrap(),
            "theirs"
        );
    }

    #[test]
    fn a_leftover_chat_folder_in_the_target_also_counts_as_taken() {
        let dir = TestDir::new();
        chat_in(&dir, "personal", CWD);
        let target = dir.path().join("work").join("projects").join(slug(CWD));
        fs::create_dir_all(target.join(ID)).unwrap();

        let result = copy_chat(
            &dir.path().join("personal"),
            &dir.path().join("work"),
            CWD,
            ID,
        );

        assert!(matches!(result, Err(CarryError::Exists)));
    }

    #[test]
    fn a_missing_source_copies_nothing() {
        let dir = TestDir::new();

        let result = copy_chat(
            &dir.path().join("personal"),
            &dir.path().join("work"),
            CWD,
            ID,
        );

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

        copy_chat(
            &dir.path().join("personal"),
            &dir.path().join("work"),
            CWD,
            ID,
        )
        .unwrap();

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

        copy_chat(
            &dir.path().join("personal"),
            &dir.path().join("work"),
            &cwd,
            ID,
        )
        .unwrap();

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
