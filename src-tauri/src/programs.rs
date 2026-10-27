use std::env;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramChoice {
    pub id: &'static str,
    pub label: &'static str,
    /// Absolute path to run, or `None` when the program is not installed.
    pub program: Option<String>,
    pub args: Vec<String>,
    /// Whether Ctrl+V with only an image on the clipboard should send the program's image-paste key.
    pub pastes_images: bool,
}

pub struct Finder {
    dirs: Vec<PathBuf>,
    exts: Vec<String>,
    program_files: Option<PathBuf>,
    is_file: Box<dyn Fn(&Path) -> bool + Send + Sync>,
}

impl Finder {
    pub fn from_env() -> Self {
        let dirs = env::var_os("PATH")
            .map(|path| env::split_paths(&path).collect())
            .unwrap_or_default();
        let exts = env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned())
            .split(';')
            .filter(|ext| !ext.is_empty())
            .map(str::to_lowercase)
            .collect();
        Self {
            dirs,
            exts,
            program_files: env::var_os("ProgramFiles").map(PathBuf::from),
            is_file: Box::new(Path::is_file),
        }
    }

    /// Resolves a name the way `cmd` does: a path as given, else each PATH dir with each PATHEXT.
    pub fn find(&self, name: &str) -> Option<PathBuf> {
        let given = Path::new(name);
        if given.components().count() > 1 || given.is_absolute() {
            return (self.is_file)(given).then(|| given.to_path_buf());
        }
        // A bare name without an extension would match npm's extensionless sh shims.
        let has_ext = given.extension().is_some();
        self.dirs.iter().find_map(|dir| {
            let exact = has_ext.then(|| dir.join(name));
            exact
                .into_iter()
                .chain(self.exts.iter().map(|ext| dir.join(format!("{name}{ext}"))))
                .find(|candidate| (self.is_file)(candidate))
        })
    }

    /// Git Bash is `bin\bash.exe` under Git's install root, which is not itself on PATH.
    pub fn git_bash(&self) -> Option<PathBuf> {
        let from_git = self.find("git").into_iter().flat_map(|git| {
            git.ancestors()
                .skip(1)
                .take(3)
                .map(Path::to_path_buf)
                .collect::<Vec<_>>()
        });
        let from_program_files = self.program_files.iter().map(|dir| dir.join("Git"));
        from_git
            .chain(from_program_files)
            .map(|root| root.join("bin").join("bash.exe"))
            .find(|bash| (self.is_file)(bash))
    }

    pub fn choices(&self) -> Vec<ProgramChoice> {
        let shown = |path: Option<PathBuf>| path.map(|p| p.to_string_lossy().into_owned());
        vec![
            ProgramChoice {
                id: "claude",
                label: "Claude Code",
                program: shown(self.find("claude")),
                args: Vec::new(),
                pastes_images: true,
            },
            ProgramChoice {
                id: "powershell",
                label: "PowerShell",
                program: shown(self.find("pwsh").or_else(|| self.find("powershell"))),
                args: vec!["-NoLogo".to_owned()],
                pastes_images: false,
            },
            ProgramChoice {
                id: "git-bash",
                label: "Git Bash",
                program: shown(self.git_bash()),
                args: vec!["--login".to_owned(), "-i".to_owned()],
                pastes_images: false,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn finder(path: &[&str], files: &[&str]) -> Finder {
        let files: HashSet<PathBuf> = files.iter().map(PathBuf::from).collect();
        Finder {
            dirs: path.iter().map(PathBuf::from).collect(),
            exts: vec![".exe".to_owned(), ".cmd".to_owned()],
            program_files: Some(PathBuf::from(r"C:\Program Files")),
            is_file: Box::new(move |p| files.contains(p)),
        }
    }

    #[test]
    fn find_tries_each_path_dir_with_each_extension() {
        let f = finder(&[r"C:\a", r"C:\b"], &[r"C:\b\claude.cmd", r"C:\b\claude"]);

        assert_eq!(f.find("claude"), Some(PathBuf::from(r"C:\b\claude.cmd")));
    }

    #[test]
    fn find_prefers_the_earlier_path_dir() {
        let f = finder(&[r"C:\a", r"C:\b"], &[r"C:\a\pwsh.exe", r"C:\b\pwsh.exe"]);

        assert_eq!(f.find("pwsh"), Some(PathBuf::from(r"C:\a\pwsh.exe")));
    }

    #[test]
    fn find_accepts_a_name_with_its_extension() {
        let f = finder(&[r"C:\w"], &[r"C:\w\powershell.exe"]);

        assert_eq!(
            f.find("powershell.exe"),
            Some(PathBuf::from(r"C:\w\powershell.exe"))
        );
    }

    #[test]
    fn find_checks_a_full_path_as_given() {
        let f = finder(&[], &[r"C:\tools\x.exe"]);

        assert_eq!(
            f.find(r"C:\tools\x.exe"),
            Some(PathBuf::from(r"C:\tools\x.exe"))
        );
        assert_eq!(f.find(r"C:\tools\y.exe"), None);
    }

    #[test]
    fn find_misses_a_program_that_is_not_installed() {
        assert_eq!(finder(&[r"C:\a"], &[]).find("claude"), None);
    }

    #[test]
    fn git_bash_is_found_from_git_on_path() {
        let f = finder(
            &[r"D:\Git\cmd"],
            &[r"D:\Git\cmd\git.exe", r"D:\Git\bin\bash.exe"],
        );

        assert_eq!(f.git_bash(), Some(PathBuf::from(r"D:\Git\bin\bash.exe")));
    }

    #[test]
    fn git_bash_is_found_from_mingw_git() {
        let f = finder(
            &[r"D:\Git\mingw64\bin"],
            &[r"D:\Git\mingw64\bin\git.exe", r"D:\Git\bin\bash.exe"],
        );

        assert_eq!(f.git_bash(), Some(PathBuf::from(r"D:\Git\bin\bash.exe")));
    }

    #[test]
    fn git_bash_falls_back_to_program_files() {
        let f = finder(&[], &[r"C:\Program Files\Git\bin\bash.exe"]);

        assert_eq!(
            f.git_bash(),
            Some(PathBuf::from(r"C:\Program Files\Git\bin\bash.exe"))
        );
    }

    #[test]
    fn choices_mark_missing_programs() {
        let f = finder(&[r"C:\w"], &[r"C:\w\powershell.exe"]);
        let choices = f.choices();

        let ids: Vec<_> = choices.iter().map(|c| c.id).collect();
        assert_eq!(ids, ["claude", "powershell", "git-bash"]);
        assert_eq!(choices[0].program, None);
        assert_eq!(choices[1].program.as_deref(), Some(r"C:\w\powershell.exe"));
        assert_eq!(choices[2].program, None);
        let pastes_images: Vec<_> = choices.iter().map(|c| c.pastes_images).collect();
        assert_eq!(pastes_images, [true, false, false]);
    }
}
