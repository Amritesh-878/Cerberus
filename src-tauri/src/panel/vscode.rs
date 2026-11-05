use std::io;
use std::path::{Path, PathBuf};

use crate::programs::Finder;

/// VS Code's own program, or `None` when it is not installed where it can be found.
pub fn find() -> Option<PathBuf> {
    let on_path = Finder::from_env().find("code");
    #[cfg(target_os = "macos")]
    let home = std::env::var_os("HOME").map(PathBuf::from);
    #[cfg(not(target_os = "macos"))]
    let home: Option<PathBuf> = None;
    resolve(on_path, home.as_deref(), &Path::is_file)
}

// On Windows `Code.exe` beside the `bin\code.cmd` shim, so no `cmd` runs; on macOS the bundle's `code` if none is on `PATH`.
fn resolve(
    on_path: Option<PathBuf>,
    home: Option<&Path>,
    is_file: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if cfg!(windows) {
        let found = on_path?;
        // The install folder itself may be on PATH, and then `code` finds `Code.exe`.
        if found
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        {
            return Some(found);
        }
        let exe = found.parent()?.parent()?.join("Code.exe");
        return is_file(&exe).then_some(exe);
    }
    if on_path.is_some() || !cfg!(target_os = "macos") {
        return on_path;
    }
    let bundle = Path::new("Applications")
        .join("Visual Studio Code.app")
        .join("Contents/Resources/app/bin/code");
    [
        Some(Path::new("/").join(&bundle)),
        home.map(|h| h.join(&bundle)),
    ]
    .into_iter()
    .flatten()
    .find(|code| is_file(code))
}

/// Starts VS Code on the folder, detached, with the folder passed as an argument and no shell between.
pub fn open(code: &Path, folder: &Path) -> io::Result<()> {
    use std::process::{Command, Stdio};

    let mut command = Command::new(code);
    command
        .arg(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    let mut child = {
        use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};

        // Outside the app's job, so an editor it started survives a crash of the app.
        crate::job::spawn_outside_job(&mut command, DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)?
    };
    #[cfg(not(windows))]
    let mut child = command.spawn()?;
    // Reaped on its own thread so the editor never waits on the app, nor lingers as a zombie on Unix.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(found: &[&Path]) -> impl Fn(&Path) -> bool + use<> {
        let found: Vec<PathBuf> = found.iter().map(|p| p.to_path_buf()).collect();
        move |p| found.iter().any(|f| f == p)
    }

    #[cfg(windows)]
    #[test]
    fn windows_starts_code_exe_beside_the_shim_and_hides_without_it() {
        let shim = PathBuf::from(r"C:\apps\VS Code\bin\code.cmd");
        let exe = PathBuf::from(r"C:\apps\VS Code\Code.exe");

        assert_eq!(
            resolve(Some(shim.clone()), None, &files(&[&shim, &exe])),
            Some(exe.clone())
        );
        assert_eq!(resolve(Some(shim.clone()), None, &files(&[&shim])), None);
        assert_eq!(
            resolve(Some(exe.clone()), None, &files(&[&exe])),
            Some(exe.clone())
        );
        assert_eq!(resolve(None, None, &files(&[])), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_prefers_path_then_the_app_bundle() {
        let on_path = PathBuf::from("/usr/local/bin/code");
        let system =
            PathBuf::from("/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code");
        let home = PathBuf::from("/Users/u");
        let mine = home.join("Applications/Visual Studio Code.app/Contents/Resources/app/bin/code");

        assert_eq!(
            resolve(Some(on_path.clone()), Some(&home), &files(&[&system])),
            Some(on_path)
        );
        assert_eq!(
            resolve(None, Some(&home), &files(&[&system, &mine])),
            Some(system)
        );
        assert_eq!(resolve(None, Some(&home), &files(&[&mine])), Some(mine));
        assert_eq!(resolve(None, Some(&home), &files(&[])), None);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn other_systems_use_code_on_path_only() {
        let on_path = PathBuf::from("/usr/bin/code");

        assert_eq!(
            resolve(Some(on_path.clone()), None, &files(&[])),
            Some(on_path)
        );
        assert_eq!(resolve(None, None, &files(&[])), None);
    }
}
