use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

const COMMAND: &str = "cerberus-cli";
const LINK_DIR: &str = ".local/bin";
const LINK_NAME: &str = "cerberus";
/// Set by the AppImage's script, so the app's own binary answers as the console command does.
pub const CLI_VAR: &str = "CERBERUS_CLI";
/// The script's second line, so a reinstall replaces it and a user's own file is never taken for it.
const SCRIPT_MARK: &str = "# Written by Cerberus to run its AppImage as the `cerberus` command.";

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("the `cerberus` command is installed this way only on macOS and Linux")]
    Unsupported,
    #[error(
        "Cerberus is running from a disk image. Move it to Applications, open it from there, and try again"
    )]
    NotInstalled,
    #[error(
        "the AppImage didn't say where its file is (APPIMAGE is unset). Start Cerberus from the AppImage file"
    )]
    AppImage,
    #[error("`{COMMAND}` is missing beside the app; reinstall Cerberus")]
    NoCommand,
    #[error("~/{LINK_DIR}/{LINK_NAME} is a file Cerberus didn't make. Move it away and try again")]
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

/// Links `~/.local/bin/cerberus` to the console command inside the running app, or writes a script that runs its AppImage.
pub fn install(home: Option<PathBuf>) -> Result<Linked, LinkError> {
    if !cfg!(any(target_os = "macos", target_os = "linux")) {
        return Err(LinkError::Unsupported);
    }
    let exe = std::env::current_exe()?;
    let home = home.ok_or(LinkError::NoHome)?;
    let shell = std::env::var("SHELL").ok();
    let done = if is_mounted_image(&exe) {
        let image = appimage().ok_or(LinkError::AppImage)?;
        write_script(&image, &home)?;
        format!("Wrote ~/{LINK_DIR}/{LINK_NAME}, which runs this AppImage.")
    } else {
        link(&exe, &home)?;
        format!("Linked ~/{LINK_DIR}/{LINK_NAME} to this app.")
    };
    // The .deb's app is `/usr/bin/cerberus`, which a PATH with `/usr/bin` first would run instead.
    if let Some(first) = shadowing(&home, login_path(), Path::is_file) {
        return Ok(linked_behind(&done, &first, &home, shell.as_deref()));
    }
    Ok(linked(&done, &home, login_path(), shell.as_deref()))
}

/// What `~/.local/bin/cerberus` runs now, for Accounts to show before and after an install.
pub fn state(home: Option<PathBuf>) -> Result<String, LinkError> {
    if !cfg!(any(target_os = "macos", target_os = "linux")) {
        return Err(LinkError::Unsupported);
    }
    let exe = std::env::current_exe()?;
    let home = home.ok_or(LinkError::NoHome)?;
    Ok(describe(&exe, appimage().as_deref(), &home))
}

// The runtime sets it to the image file, which outlives the mount the app runs from.
fn appimage() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

// An AppImage runs from a mount under the temp folder that goes away when the app closes.
fn is_mounted_image(exe: &Path) -> bool {
    exe.to_string_lossy().contains("/.mount_")
}

/// The state line, from what `~/.local/bin/cerberus` is and what this app runs as.
fn describe(exe: &Path, image: Option<&Path>, home: &Path) -> String {
    let path = home.join(LINK_DIR).join(LINK_NAME);
    let packaged = exe.starts_with("/usr/bin");
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(_) if packaged => {
            return "The package's /usr/bin/cerberus is the app itself, so `cerberus open` keeps the terminal until the app closes. Install the command for one that returns at once.".to_owned();
        }
        Err(_) => return "The `cerberus` command isn't installed.".to_owned(),
    };
    if meta.file_type().is_symlink() {
        let Ok(target) = fs::read_link(&path) else {
            return format!("~/{LINK_DIR}/{LINK_NAME} can't be read.");
        };
        return if target == exe.with_file_name(COMMAND) {
            "`cerberus` runs this app.".to_owned()
        } else if target.exists() {
            format!("`cerberus` runs {}.", target.display())
        } else {
            format!(
                "`cerberus` points at {}, which is gone. Install it again.",
                target.display()
            )
        };
    }
    let Some(runs) = fs::read_to_string(&path)
        .ok()
        .as_deref()
        .and_then(script_image)
    else {
        return format!(
            "~/{LINK_DIR}/{LINK_NAME} is a file Cerberus didn't make, so it is left as it is."
        );
    };
    if Some(runs.as_path()) == image {
        "`cerberus` runs this AppImage.".to_owned()
    } else if runs.exists() {
        format!("`cerberus` runs the AppImage at {}.", runs.display())
    } else {
        format!(
            "`cerberus` runs the AppImage at {}, which has moved. Install it again.",
            runs.display()
        )
    }
}

/// The shell script for `image`; it says where the image was when it has moved.
fn script_for(image: &Path) -> String {
    let quoted = image.to_string_lossy().replace('\'', r"'\''");
    format!(
        "#!/bin/sh
{SCRIPT_MARK}
appimage='{quoted}'
if [ ! -x \"$appimage\" ]; then
  echo \"cerberus: the Cerberus AppImage is no longer at $appimage.\" >&2
  echo \"Open Cerberus from where it is now and choose Install \\`cerberus\\` command in Accounts.\" >&2
  exit 1
fi
{CLI_VAR}=1 exec \"$appimage\" \"$@\"
"
    )
}

/// The image a script of ours runs, or `None` for any other file.
fn script_image(text: &str) -> Option<PathBuf> {
    let mut lines = text.lines();
    if lines.next() != Some("#!/bin/sh") || lines.next() != Some(SCRIPT_MARK) {
        return None;
    }
    let quoted = lines
        .next()?
        .strip_prefix("appimage='")?
        .strip_suffix('\'')?;
    Some(PathBuf::from(quoted.replace(r"'\''", "'")))
}

/// Writes the script beside any older one of ours or a link, but never over a user's own file.
fn write_script(image: &Path, home: &Path) -> Result<(), LinkError> {
    let dir = home.join(LINK_DIR);
    let path = dir.join(LINK_NAME);
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() => fs::remove_file(&path)?,
        Ok(_) => {
            let ours = fs::read_to_string(&path)
                .ok()
                .as_deref()
                .and_then(script_image)
                .is_some();
            if !ours {
                return Err(LinkError::Taken);
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    fs::create_dir_all(&dir)?;
    // Written beside and renamed over, so a shell never runs half a script.
    let partial = dir.join(format!(".{LINK_NAME}.partial"));
    fs::write(&partial, script_for(image))?;
    make_executable(&partial)?;
    fs::rename(&partial, &path)?;
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}

/// A `cerberus` in a login `PATH` folder ahead of the link's, which a shell would run instead.
fn shadowing(
    home: &Path,
    login_path: Option<&str>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let dir = home.join(LINK_DIR);
    login_path?
        .split(':')
        .map(Path::new)
        .take_while(|d| *d != dir)
        .map(|d| d.join(LINK_NAME))
        .find(|candidate| exists(candidate))
}

fn linked_behind(done: &str, first: &Path, home: &Path, shell: Option<&str>) -> Linked {
    let behind = format!(
        "{} But {} comes first on your login shell's PATH, so `cerberus` runs that.",
        done,
        first.display()
    );
    match startup_line(home, shell) {
        Some((file, line)) => Linked {
            message: format!(
                "{behind} Add this line to the end of {file}, then open a new terminal:"
            ),
            path_line: Some(line),
        },
        None => Linked {
            message: format!(
                "{behind} Put ~/{LINK_DIR} ahead of it on your shell's PATH, or run `cerberus-cli`."
            ),
            path_line: None,
        },
    }
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
    if is_mounted_image(exe) {
        return Err(LinkError::AppImage);
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
fn linked(done: &str, home: &Path, login_path: Option<&str>, shell: Option<&str>) -> Linked {
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
    const DONE: &str = "Linked ~/.local/bin/cerberus to this app.";

    #[test]
    fn a_cerberus_ahead_of_the_link_on_path_is_reported_with_the_line_that_puts_the_link_first() {
        let home = Path::new("/home/u");
        let deb = Path::new("/usr/bin/cerberus");
        let exists = |p: &Path| p == deb;

        let behind = "/usr/local/bin:/usr/bin:/home/u/.local/bin";
        let ahead = "/home/u/.local/bin:/usr/bin";
        assert_eq!(shadowing(home, Some(behind), exists).as_deref(), Some(deb));
        assert_eq!(shadowing(home, Some(ahead), exists), None);
        assert_eq!(shadowing(home, Some("/usr/local/bin"), |_| false), None);
        assert_eq!(shadowing(home, None, exists), None);

        let told = linked_behind(DONE, deb, home, Some("/bin/zsh"));
        assert!(
            told.message.contains("/usr/bin/cerberus comes first"),
            "{}",
            told.message
        );
        assert_eq!(
            told.path_line.as_deref(),
            Some(r#"export PATH="$HOME/.local/bin:$PATH""#)
        );
        let unknown = linked_behind(DONE, deb, home, Some("/opt/bin/nu"));
        assert!(
            unknown.message.contains("cerberus-cli"),
            "{}",
            unknown.message
        );
        assert_eq!(unknown.path_line, None);
    }

    #[test]
    fn a_link_dir_on_the_login_path_needs_no_line() {
        let home = Path::new(HOME);
        let found = linked(
            DONE,
            home,
            Some("/opt/homebrew/bin:/Users/u/.local/bin:/usr/bin"),
            Some("/bin/zsh"),
        );
        let slashed = linked(DONE, home, Some("/usr/bin:/Users/u/.local/bin/"), None);

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
            assert!(
                linked(DONE, home, Some(path), None).path_line.is_some(),
                "{path}"
            );
        }
    }

    #[test]
    fn a_missing_link_dir_gets_the_line_for_the_login_shell() {
        let path = Some("/opt/homebrew/bin:/usr/bin:/bin");
        let home = Path::new(HOME);
        let line = |shell| linked(DONE, home, path, Some(shell)).path_line;

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
        let bash = linked(DONE, home, path, Some("/bin/bash"));
        assert!(bash.message.contains("~/.bash_profile"), "{}", bash.message);
        assert!(bash.message.contains("isn't on your login shell's PATH"));
        let csh = linked(DONE, home, path, Some("/bin/csh"));
        assert!(csh.message.contains("~/.cshrc"), "{}", csh.message);
        assert!(
            linked(DONE, home, path, Some("/bin/ksh"))
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

            let advice = linked(DONE, home.path(), Some("/usr/bin"), Some(shell));

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
        let nu = linked(
            DONE,
            Path::new(HOME),
            Some("/usr/bin"),
            Some("/opt/local/bin/nu"),
        );

        assert_eq!(nu.path_line, None);
        assert!(
            nu.message.contains("add ~/.local/bin to your shell's PATH"),
            "{}",
            nu.message
        );
    }

    #[test]
    fn an_unread_login_path_still_gives_the_line() {
        let unknown = linked(DONE, Path::new(HOME), None, None);

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

    #[test]
    fn an_app_run_from_an_appimage_mount_is_refused_and_a_deb_install_is_linked() {
        let home = TestDir::new();
        let mounted = Path::new("/tmp/.mount_CerberXyZ12/usr/bin/cerberus");

        assert!(matches!(
            link(mounted, home.path()),
            Err(LinkError::AppImage)
        ));
        assert!(!home.path().join(LINK_DIR).exists());

        let root = TestDir::new();
        let bin = root.path().join("usr/bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("cerberus"), "").unwrap();
        fs::write(bin.join(COMMAND), "").unwrap();
        let linked = link(&bin.join("cerberus"), home.path());

        if cfg!(unix) {
            assert!(linked.is_ok(), "{linked:?}");
            assert_eq!(
                fs::read_link(home.path().join(LINK_DIR).join(LINK_NAME)).unwrap(),
                bin.join(COMMAND)
            );
        }
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
    #[cfg(unix)]
    const MOUNTED: &str = "/tmp/.mount_CerberAb12/usr/bin/cerberus";

    #[test]
    fn the_script_names_its_appimage_and_reads_back_even_with_a_quote_in_the_path() {
        for image in [
            "/home/u/Apps/Cerberus_1.1.0_amd64.AppImage",
            "/home/u/it's here/Cerberus.AppImage",
        ] {
            let script = script_for(Path::new(image));

            assert!(script.starts_with("#!/bin/sh\n"));
            assert!(script.contains(&format!("{CLI_VAR}=1 exec \"$appimage\" \"$@\"")));
            assert_eq!(script_image(&script), Some(PathBuf::from(image)), "{image}");
        }
        assert_eq!(script_image("#!/bin/sh\necho mine\n"), None);
    }

    #[cfg(unix)]
    #[test]
    fn the_script_runs_the_appimage_with_its_arguments_and_says_when_it_moved() {
        let root = TestDir::new();
        let image = root.path().join("Apps/Cerberus.AppImage");
        fs::create_dir_all(image.parent().unwrap()).unwrap();
        fs::write(&image, "#!/bin/sh\nprintf '%s|' \"$CERBERUS_CLI\" \"$@\"\n").unwrap();
        make_executable(&image).unwrap();
        write_script(&image, root.path()).unwrap();
        let command = root.path().join(LINK_DIR).join(LINK_NAME);
        let run = || {
            std::process::Command::new(&command)
                .args(["open", "my app/ui"])
                .output()
                .unwrap()
        };

        let ran = run();
        assert_eq!(String::from_utf8_lossy(&ran.stdout), "1|open|my app/ui|");

        fs::remove_file(&image).unwrap();
        let moved = run();
        assert!(!moved.status.success());
        let said = String::from_utf8_lossy(&moved.stderr);
        assert!(said.contains(&image.display().to_string()), "{said}");
        assert!(said.contains("Install `cerberus` command"), "{said}");
    }

    #[cfg(unix)]
    #[test]
    fn a_reinstall_replaces_our_script_or_a_link_but_never_a_users_file() {
        let root = TestDir::new();
        let path = root.path().join(LINK_DIR).join(LINK_NAME);
        let old = Path::new("/home/u/old/Cerberus.AppImage");
        let new = Path::new("/home/u/new/Cerberus.AppImage");

        write_script(old, root.path()).unwrap();
        write_script(new, root.path()).unwrap();
        assert_eq!(
            script_image(&fs::read_to_string(&path).unwrap()).as_deref(),
            Some(new)
        );

        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink("/usr/bin/cerberus-cli", &path).unwrap();
        write_script(new, root.path()).unwrap();
        assert!(
            !fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );

        fs::write(&path, "#!/bin/sh\necho mine\n").unwrap();
        assert!(matches!(
            write_script(new, root.path()),
            Err(LinkError::Taken)
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), "#!/bin/sh\necho mine\n");
    }

    #[cfg(unix)]
    #[test]
    fn accounts_says_what_the_command_runs_for_an_appimage() {
        let root = TestDir::new();
        let image = root.path().join("Cerberus.AppImage");
        fs::write(&image, "").unwrap();
        let elsewhere = root.path().join("older.AppImage");
        fs::write(&elsewhere, "").unwrap();
        let describe_now = || describe(Path::new(MOUNTED), Some(&image), root.path());

        assert_eq!(describe_now(), "The `cerberus` command isn't installed.");
        write_script(&image, root.path()).unwrap();
        assert_eq!(describe_now(), "`cerberus` runs this AppImage.");
        write_script(&elsewhere, root.path()).unwrap();
        assert!(
            describe_now().contains("runs the AppImage at"),
            "{}",
            describe_now()
        );
        fs::remove_file(&elsewhere).unwrap();
        assert!(
            describe_now().ends_with("which has moved. Install it again."),
            "{}",
            describe_now()
        );
        fs::write(root.path().join(LINK_DIR).join(LINK_NAME), "mine").unwrap();
        assert!(describe_now().contains("didn't make"), "{}", describe_now());
    }

    #[cfg(unix)]
    #[test]
    fn accounts_says_what_the_command_runs_for_a_package_or_a_mac_app() {
        let root = TestDir::new();
        let packaged = Path::new("/usr/bin/cerberus");

        assert!(
            describe(packaged, None, root.path()).contains("/usr/bin/cerberus is the app itself"),
            "{}",
            describe(packaged, None, root.path())
        );
        let dir = root.path().join(LINK_DIR);
        fs::create_dir_all(&dir).unwrap();
        std::os::unix::fs::symlink("/usr/bin/cerberus-cli", dir.join(LINK_NAME)).unwrap();
        assert_eq!(
            describe(packaged, None, root.path()),
            "`cerberus` runs this app."
        );

        let exe = app(root.path());
        assert_eq!(
            describe(&exe, None, root.path()),
            "`cerberus` points at /usr/bin/cerberus-cli, which is gone. Install it again."
        );
        fs::remove_file(dir.join(LINK_NAME)).unwrap();
        link(&exe, root.path()).unwrap();
        assert_eq!(
            describe(&exe, None, root.path()),
            "`cerberus` runs this app."
        );
        assert_eq!(
            describe(&exe, None, &root.path().join("nobody")),
            "The `cerberus` command isn't installed."
        );
    }
}
