//! PATH folders that answer: a folder on a dead network share or an unplugged drive stalls every file check for the network timeout.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError, mpsc};
use std::thread;
use std::time::{Duration, Instant};

/// How long all of PATH's non-local folders together may take to answer.
const WAIT: Duration = Duration::from_millis(500);
/// How long a folder that did not answer is left out before it is asked again.
const DEAD_FOR: Duration = Duration::from_secs(60);

static DEAD: Mutex<Vec<(PathBuf, Instant)>> = Mutex::new(Vec::new());

/// `dirs` without the non-local folders that do not answer within a short wait.
pub fn answering(dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut dead = DEAD.lock().unwrap_or_else(PoisonError::into_inner);
    let now = Instant::now();
    dead.retain(|(_, at)| now.duration_since(*at) < DEAD_FOR);
    // Held through the wait, so a lookup made meanwhile finds the dead folder instead of asking it again.
    keep_answering(dirs, &mut dead, now, WAIT, is_local, Path::is_dir)
}

/// Probes every folder `local` doesn't vouch for at once, in its own thread, so a hung one only costs `wait`.
fn keep_answering(
    dirs: Vec<PathBuf>,
    dead: &mut Vec<(PathBuf, Instant)>,
    now: Instant,
    wait: Duration,
    local: impl Fn(&Path) -> bool,
    probe: fn(&Path) -> bool,
) -> Vec<PathBuf> {
    let (tx, rx) = mpsc::channel();
    let mut asked = Vec::new();
    for (at, dir) in dirs.iter().enumerate() {
        if local(dir) || dead.iter().any(|(d, _)| d == dir) {
            continue;
        }
        let (tx, dir) = (tx.clone(), dir.clone());
        // A thread that cannot start leaves the folder unasked, so it is treated like one that did not answer.
        if thread::Builder::new()
            .spawn(move || {
                let _ = tx.send((at, probe(&dir)));
            })
            .is_ok()
        {
            asked.push(at);
        }
    }
    drop(tx);
    let deadline = now + wait;
    let mut answers = Vec::new();
    while answers.len() < asked.len() {
        let left = deadline.saturating_duration_since(Instant::now());
        let Ok(answer) = rx.recv_timeout(left) else {
            break;
        };
        answers.push(answer);
    }
    let mut kept = Vec::new();
    for (at, dir) in dirs.into_iter().enumerate() {
        if !asked.contains(&at) {
            if !dead.iter().any(|(d, _)| *d == dir) {
                kept.push(dir);
            }
        } else if let Some(&(_, is_dir)) = answers.iter().find(|(a, _)| *a == at) {
            if is_dir {
                kept.push(dir);
            }
        } else {
            dead.push((dir, now));
        }
    }
    kept
}

/// A folder on a fixed or RAM drive, which answers at once; a share, a removable drive or a relative entry may not.
#[cfg(windows)]
fn is_local(dir: &Path) -> bool {
    use std::path::{Component, Prefix};

    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    use windows_sys::Win32::System::WindowsProgramming::{DRIVE_FIXED, DRIVE_RAMDISK};

    let Some(Component::Prefix(prefix)) = dir.components().next() else {
        return false;
    };
    let (Prefix::Disk(letter) | Prefix::VerbatimDisk(letter)) = prefix.kind() else {
        return false;
    };
    let root: Vec<u16> = format!("{}:\\", char::from(letter))
        .encode_utf16()
        .chain([0])
        .collect();
    // SAFETY: a NUL-terminated string that outlives the call; the call only reads it.
    let kind = unsafe { GetDriveTypeW(root.as_ptr()) };
    kind == DRIVE_FIXED || kind == DRIVE_RAMDISK
}

/// Unix PATHs rarely hold a network mount, and probing each folder would cost a thread per lookup.
#[cfg(unix)]
fn is_local(_dir: &Path) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hangs_on_dead(dir: &Path) -> bool {
        if dir.ends_with("dead") {
            thread::sleep(Duration::from_secs(5));
        }
        !dir.ends_with("gone")
    }

    fn dirs(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn a_folder_that_hangs_is_left_out_after_the_wait_and_remembered() {
        let mut dead = Vec::new();
        let local = |dir: &Path| dir.starts_with("local");
        let asked = Instant::now();

        let kept = keep_answering(
            dirs(&[
                "local/a",
                "share/dead",
                "share/live",
                "share/gone",
                "local/b",
            ]),
            &mut dead,
            Instant::now(),
            Duration::from_millis(200),
            local,
            hangs_on_dead,
        );

        assert!(
            asked.elapsed() < Duration::from_secs(2),
            "{:?}",
            asked.elapsed()
        );
        assert_eq!(kept, dirs(&["local/a", "share/live", "local/b"]));
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].0, PathBuf::from("share/dead"));
    }

    #[test]
    fn a_folder_remembered_as_dead_is_not_asked_again() {
        let mut dead = vec![(PathBuf::from("share/dead"), Instant::now())];
        let asked = Instant::now();

        let kept = keep_answering(
            dirs(&["share/dead", "share/live"]),
            &mut dead,
            Instant::now(),
            Duration::from_secs(10),
            |_: &Path| false,
            hangs_on_dead,
        );

        assert!(
            asked.elapsed() < Duration::from_secs(2),
            "{:?}",
            asked.elapsed()
        );
        assert_eq!(kept, dirs(&["share/live"]));
    }

    #[cfg(windows)]
    #[test]
    fn a_fixed_drive_is_local_and_a_share_or_relative_entry_is_not() {
        let system = std::env::var_os("SystemRoot").unwrap();

        assert!(is_local(Path::new(&system)));
        assert!(!is_local(Path::new(r"\\server\share\bin")));
        assert!(!is_local(Path::new("bin")));
    }
}
