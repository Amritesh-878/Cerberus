use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

const COMMAND: &str = "cerberus-cli";
const LINK_DIR: &str = ".local/bin";
const LINK_NAME: &str = "cerberus";

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("the `cerberus` command is installed this way only on macOS")]
    Unsupported,
    #[error(
        "Cerberus is running from a disk image or a temporary copy; move it to Applications and open it from there first, so the link keeps working"
    )]
    NotInstalled,
    #[error("`{COMMAND}` is missing beside the app; reinstall Cerberus")]
    NoCommand,
    #[error(
        "~/{LINK_DIR}/{LINK_NAME} already exists and is not a link; move it away, then try again"
    )]
    Taken,
    #[error("could not find your home folder")]
    NoHome,
    #[error("could not link ~/{LINK_DIR}/{LINK_NAME}: {0}")]
    Io(#[from] io::Error),
}

impl Serialize for LinkError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Linked {
    pub message: String,
    /// The line to add to a shell startup file; Cerberus never edits one itself.
    pub path_line: Option<String>,
}

/// Links `~/.local/bin/cerberus` to the console command inside the running app.
pub fn install(home: Option<PathBuf>) -> Result<Linked, LinkError> {
    if !cfg!(target_os = "macos") {
        return Err(LinkError::Unsupported);
    }
    let exe = std::env::current_exe()?;
    let home = home.ok_or(LinkError::NoHome)?;
    let shell = std::env::var("SHELL").ok();
    link(&exe, &home)?;
    Ok(linked(&home, login_path(), shell.as_deref()))
}

#[cfg(unix)]
fn login_path() -> Option<&'static str> {
    crate::login_path::login_dirs()
}

#[cfg(not(unix))]
fn login_path() -> Option<&'static str> {
    None
}

// A link into a mounted dmg or a translocated copy breaks once that copy goes away.
fn is_temporary(exe: &Path) -> bool {
    let shown = exe.to_string_lossy();
    shown.starts_with("/Volumes/") || shown.contains("/AppTranslocation/")
}

/// Replaces an older link, as `ln -sf` would, but never a file that isn't a link.
fn link(exe: &Path, home: &Path) -> Result<(), LinkError> {
    if is_temporary(exe) {
        return Err(LinkError::NotInstalled);
    }
    let target = exe
        .parent()
        .map(|dir| dir.join(COMMAND))
        .filter(|t| t.is_file())
        .ok_or(LinkError::NoCommand)?;
    let dir = home.join(LINK_DIR);
    let link = dir.join(LINK_NAME);
    match fs::symlink_metadata(&link) {
        Ok(meta) if !meta.file_type().is_symlink() => return Err(LinkError::Taken),
        Ok(_) => fs::remove_file(&link)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    fs::create_dir_all(&dir)?;
    symlink(&target, &link)?;
    Ok(())
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn symlink(_target: &Path, _link: &Path) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}

/// What to tell the user, from the login shell's `PATH` read at start (`None` when it could not be read).
fn linked(home: &Path, login_path: Option<&str>, shell: Option<&str>) -> Linked {
    let done = format!("Linked ~/{LINK_DIR}/{LINK_NAME} to this app.");
    let dir = home.join(LINK_DIR);
    let on_path = login_path.map(|path| path.split(':').any(|d| Path::new(d) == dir));
    let (file, line) = startup_line(shell);
    match on_path {
        Some(true) => Linked {
            message: format!("{done} Open a new terminal and run `cerberus`."),
            path_line: None,
        },
        Some(false) => Linked {
            message: format!(
                "{done} ~/{LINK_DIR} isn't on your login shell's PATH, so add this line to {file}, then open a new terminal:"
            ),
            path_line: Some(line),
        },
        None => Linked {
            message: format!(
                "{done} Cerberus couldn't read your login shell's PATH. If a new terminal says `cerberus` is not found, add this line to {file}:"
            ),
            path_line: Some(line),
        },
    }
}

fn startup_line(shell: Option<&str>) -> (&'static str, String) {
    let name = shell.and_then(|s| s.rsplit('/').next()).unwrap_or("zsh");
    match name {
        "fish" => (
            "~/.config/fish/config.fish",
            format!("fish_add_path ~/{LINK_DIR}"),
        ),
        "bash" => ("~/.bash_profile", export_line()),
        "zsh" => ("~/.zshrc", export_line()),
        _ => ("your shell's startup file", export_line()),
    }
}

fn export_line() -> String {
    format!(r#"export PATH="$HOME/{LINK_DIR}:$PATH""#)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::testdir::TestDir;

    const HOME: &str = "/Users/u";

    #[test]
    fn a_link_dir_on_the_login_path_needs_no_line() {
        let home = Path::new(HOME);
        let found = linked(
            home,
            Some("/opt/homebrew/bin:/Users/u/.local/bin:/usr/bin"),
            Some("/bin/zsh"),
        );
        let slashed = linked(home, Some("/usr/bin:/Users/u/.local/bin/"), None);

        assert_eq!(found.path_line, None);
        assert!(
            found.message.contains("run `cerberus`"),
            "{}",
            found.message
        );
        assert_eq!(slashed.path_line, None);
    }

    #[test]
    fn another_users_or_an_unexpanded_local_bin_does_not_count() {
        let home = Path::new(HOME);

        for path in [
            "/Users/other/.local/bin",
            "~/.local/bin",
            "/Users/u/.local/bin2",
        ] {
            assert!(linked(home, Some(path), None).path_line.is_some(), "{path}");
        }
    }

    #[test]
    fn a_missing_link_dir_gets_the_line_for_the_login_shell() {
        let path = Some("/opt/homebrew/bin:/usr/bin:/bin");
        let home = Path::new(HOME);
        let line = |shell| linked(home, path, Some(shell)).path_line;

        assert_eq!(
            line("/bin/zsh").as_deref(),
            Some(r#"export PATH="$HOME/.local/bin:$PATH""#)
        );
        assert_eq!(
            line("/opt/homebrew/bin/fish").as_deref(),
            Some("fish_add_path ~/.local/bin")
        );
        let bash = linked(home, path, Some("/bin/bash"));
        assert!(bash.message.contains("~/.bash_profile"), "{}", bash.message);
        assert!(bash.message.contains("isn't on your login shell's PATH"));
        let tcsh = linked(home, path, Some("/bin/tcsh"));
        assert!(tcsh.message.contains("your shell's startup file"));
    }

    #[test]
    fn an_unread_login_path_still_gives_the_line() {
        let unknown = linked(Path::new(HOME), None, None);

        assert!(
            unknown.message.contains("couldn't read"),
            "{}",
            unknown.message
        );
        assert!(unknown.message.contains("~/.zshrc"));
        assert!(unknown.path_line.is_some());
    }

    #[test]
    fn an_app_run_from_a_disk_image_or_a_temporary_copy_is_refused() {
        let home = TestDir::new();
        let dmg = Path::new("/Volumes/Cerberus/Cerberus.app/Contents/MacOS/cerberus");
        let moved = Path::new(
            "/private/var/folders/x/AppTranslocation/1234/d/Cerberus.app/Contents/MacOS/cerberus",
        );

        assert!(matches!(
            link(dmg, home.path()),
            Err(LinkError::NotInstalled)
        ));
        assert!(matches!(
            link(moved, home.path()),
            Err(LinkError::NotInstalled)
        ));
        assert!(!home.path().join(LINK_DIR).exists());
    }

    fn app(root: &Path) -> PathBuf {
        let bundle = root.join("Cerberus.app/Contents/MacOS");
        fs::create_dir_all(&bundle).unwrap();
        fs::write(bundle.join("cerberus"), "").unwrap();
        fs::write(bundle.join(COMMAND), "").unwrap();
        bundle.join("cerberus")
    }

    #[test]
    fn a_missing_command_is_reported_before_anything_is_made() {
        let root = TestDir::new();
        let exe = app(root.path());
        fs::remove_file(exe.with_file_name(COMMAND)).unwrap();

        assert!(matches!(link(&exe, root.path()), Err(LinkError::NoCommand)));
        assert!(!root.path().join(LINK_DIR).exists());
    }

    #[test]
    fn a_file_that_is_not_a_link_is_never_replaced() {
        let root = TestDir::new();
        let exe = app(root.path());
        let dir = root.path().join(LINK_DIR);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(LINK_NAME), "my own script").unwrap();

        assert!(matches!(link(&exe, root.path()), Err(LinkError::Taken)));
        assert_eq!(
            fs::read_to_string(dir.join(LINK_NAME)).unwrap(),
            "my own script"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_link_is_made_and_an_old_one_replaced() {
        let root = TestDir::new();
        let exe = app(root.path());
        let link_path = root.path().join(LINK_DIR).join(LINK_NAME);

        link(&exe, root.path()).unwrap();
        assert_eq!(
            fs::read_link(&link_path).unwrap(),
            exe.with_file_name(COMMAND)
        );

        let newer = app(&root.path().join("Applications"));
        link(&newer, root.path()).unwrap();
        assert_eq!(
            fs::read_link(&link_path).unwrap(),
            newer.with_file_name(COMMAND)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_old_link_is_replaced() {
        let root = TestDir::new();
        let exe = app(root.path());
        let dir = root.path().join(LINK_DIR);
        fs::create_dir_all(&dir).unwrap();
        std::os::unix::fs::symlink(root.path().join("gone"), dir.join(LINK_NAME)).unwrap();

        link(&exe, root.path()).unwrap();

        assert_eq!(
            fs::read_link(dir.join(LINK_NAME)).unwrap(),
            exe.with_file_name(COMMAND)
        );
    }
}
