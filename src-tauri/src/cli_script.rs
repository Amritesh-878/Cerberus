//! The script that runs an AppImage as the `cerberus` command, since the image's own files go away when it closes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cli_link::{CLI_VAR, LINK_DIR, LINK_NAME, LinkError};

/// The script's second line, so a reinstall replaces it and a user's own file is never taken for it.
const SCRIPT_MARK: &str = "# Written by Cerberus to run its AppImage as the `cerberus` command.";

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
pub fn image_of(text: &str) -> Option<PathBuf> {
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
pub fn write(image: &Path, home: &Path) -> Result<(), LinkError> {
    let dir = home.join(LINK_DIR);
    let path = dir.join(LINK_NAME);
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() => fs::remove_file(&path)?,
        Ok(_) => {
            let ours = fs::read_to_string(&path)
                .ok()
                .as_deref()
                .and_then(image_of)
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
pub fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
pub fn make_executable(_path: &Path) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::accounts::testdir::TestDir;

    #[test]
    fn the_script_names_its_appimage_and_reads_back_even_with_a_quote_in_the_path() {
        for image in [
            "/home/u/Apps/Cerberus_1.1.0_amd64.AppImage",
            "/home/u/it's here/Cerberus.AppImage",
        ] {
            let script = script_for(Path::new(image));

            assert!(script.starts_with("#!/bin/sh\n"));
            assert!(script.contains(&format!("{CLI_VAR}=1 exec \"$appimage\" \"$@\"")));
            assert_eq!(image_of(&script), Some(PathBuf::from(image)), "{image}");
        }
        assert_eq!(image_of("#!/bin/sh\necho mine\n"), None);
    }

    #[cfg(unix)]
    #[test]
    fn the_script_runs_the_appimage_with_its_arguments_and_says_when_it_moved() {
        let root = TestDir::new();
        let image = root.path().join("Apps/Cerberus.AppImage");
        fs::create_dir_all(image.parent().unwrap()).unwrap();
        fs::write(&image, "#!/bin/sh\nprintf '%s|' \"$CERBERUS_CLI\" \"$@\"\n").unwrap();
        make_executable(&image).unwrap();
        write(&image, root.path()).unwrap();
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

        write(old, root.path()).unwrap();
        write(new, root.path()).unwrap();
        assert_eq!(
            image_of(&fs::read_to_string(&path).unwrap()).as_deref(),
            Some(new)
        );

        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink("/usr/bin/cerberus-cli", &path).unwrap();
        write(new, root.path()).unwrap();
        assert!(
            !fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );

        fs::write(&path, "#!/bin/sh\necho mine\n").unwrap();
        assert!(matches!(write(new, root.path()), Err(LinkError::Taken)));
        assert_eq!(fs::read_to_string(&path).unwrap(), "#!/bin/sh\necho mine\n");
    }
}
