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
    let missing = match on_path {
        Some(true) => {
            return Linked {
                message: format!("{done} Open a new terminal and run `cerberus`."),
                path_line: None,
            };
        }
        Some(false) => format!("{done} ~/{LINK_DIR} isn't on your login shell's PATH, so"),
        None => format!(
            "{done} Cerberus couldn't read your login shell's PATH. If a new terminal says `cerberus` is not found,"
        ),
    };
    match startup_line(home, shell) {
        Some((file, line)) => Linked {
            message: format!("{missing} add this line to {file}, then open a new terminal:"),
            path_line: Some(line),
        },
        None => Linked {
            message: format!("{missing} add ~/{LINK_DIR} to your shell's PATH."),
            path_line: None,
        },
    }
}

/// The file and line for shells whose syntax is known; another shell gets no line it might not parse.
fn startup_line(home: &Path, shell: Option<&str>) -> Option<(&'static str, String)> {
    let name = shell.and_then(|s| s.rsplit('/').next()).unwrap_or("zsh");
    let export = || format!(r#"export PATH="$HOME/{LINK_DIR}:$PATH""#);
    let setenv = || format!(r#"setenv PATH "$HOME/{LINK_DIR}:$PATH""#);
    match name {
        "zsh" => Some(("~/.zshrc", export())),
        "bash" => Some((
            read_first(home, &["~/.bash_profile", "~/.bash_login", "~/.profile"]),
            export(),
        )),
        "sh" | "dash" | "ksh" => Some(("~/.profile", export())),
        "fish" => Some((
            "~/.config/fish/config.fish",
            format!("fish_add_path ~/{LINK_DIR}"),
        )),
        "tcsh" => Some((read_first(home, &["~/.tcshrc", "~/.cshrc"]), setenv())),
        "csh" => Some(("~/.cshrc", setenv())),
        _ => None,
    }
}

// A login bash or tcsh reads only the first of its files that exists, so a new earlier one would hide the user's own.
fn read_first(home: &Path, files: &[&'static str]) -> &'static str {
    files
        .iter()
        .copied()
        .find(|file| home.join(file.trim_start_matches("~/")).exists())
        .unwrap_or(files[0])
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
        assert_eq!(
            line("/bin/tcsh").as_deref(),
            Some(r#"setenv PATH "$HOME/.local/bin:$PATH""#)
        );
        let bash = linked(home, path, Some("/bin/bash"));
        assert!(bash.message.contains("~/.bash_profile"), "{}", bash.message);
        assert!(bash.message.contains("isn't on your login shell's PATH"));
        let csh = linked(home, path, Some("/bin/csh"));
        assert!(csh.message.contains("~/.cshrc"), "{}", csh.message);
        assert!(
            linked(home, path, Some("/bin/ksh"))
                .message
                .contains("~/.profile")
        );
    }

    #[test]
    fn bash_and_tcsh_name_the_first_startup_file_the_home_already_has() {
        let cases: [(&str, &[&str], &str); 7] = [
            ("/bin/bash", &[], "~/.bash_profile"),
            ("/bin/bash", &[".profile"], "~/.profile"),
            ("/bin/bash", &[".bash_login", ".profile"], "~/.bash_login"),
            (
                "/bin/bash",
                &[".bash_profile", ".profile"],
                "~/.bash_profile",
            ),
            ("/bin/tcsh", &[], "~/.tcshrc"),
            ("/bin/tcsh", &[".cshrc"], "~/.cshrc"),
            ("/bin/tcsh", &[".tcshrc", ".cshrc"], "~/.tcshrc"),
        ];
        for (shell, files, expected) in cases {
            let home = TestDir::new();
            for file in files {
                fs::write(home.path().join(file), "").unwrap();
            }

            let advice = linked(home.path(), Some("/usr/bin"), Some(shell));

            assert!(
                advice
                    .message
                    .contains(&format!("add this line to {expected},")),
                "{shell} with {files:?}: {}",
                advice.message
            );
        }
    }

    #[test]
    fn zsh_fish_and_sh_keep_their_one_file_whatever_the_home_has() {
        let home = TestDir::new();
        fs::write(home.path().join(".profile"), "").unwrap();
        fs::write(home.path().join(".bash_profile"), "").unwrap();
        let file = |shell| {
            startup_line(home.path(), Some(shell))
                .map(|(file, _)| file)
                .unwrap()
        };

        assert_eq!(file("/bin/zsh"), "~/.zshrc");
        assert_eq!(file("/opt/homebrew/bin/fish"), "~/.config/fish/config.fish");
        assert_eq!(file("/bin/sh"), "~/.profile");
    }

    #[test]
    fn a_shell_of_unknown_syntax_gets_no_line_to_paste() {
        let nu = linked(Path::new(HOME), Some("/usr/bin"), Some("/opt/local/bin/nu"));

        assert_eq!(nu.path_line, None);
        assert!(
            nu.message.contains("add ~/.local/bin to your shell's PATH"),
            "{}",
            nu.message
        );
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
