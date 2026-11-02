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
    /// What a paste of only an image sends, so the program reads the clipboard itself (`None` pastes nothing).
    pub image_paste_key: Option<&'static str>,
    /// Whether the Shell button can open it.
    pub is_shell: bool,
    /// What a shell runs to install the program, shown while it is missing.
    pub install_command: Option<&'static str>,
    /// The shell `install_command` is written for; `None` means the user's own.
    pub install_shell: Option<&'static str>,
}

// Claude Code's image-paste key: Alt+V on Windows, Ctrl+V elsewhere.
#[cfg(windows)]
const CLAUDE_IMAGE_PASTE: &str = "\x1bv";
#[cfg(not(windows))]
const CLAUDE_IMAGE_PASTE: &str = "\x16";

#[cfg(windows)]
const CLAUDE_INSTALL: (&str, Option<&str>) = (
    "irm https://claude.ai/install.ps1 | iex",
    Some("powershell"),
);
#[cfg(not(windows))]
const CLAUDE_INSTALL: (&str, Option<&str>) =
    ("curl -fsSL https://claude.ai/install.sh | bash", None);

/// The shell the Shell button opens: the saved choice when installed, else the first shell installed.
pub fn pick_shell(choices: Vec<ProgramChoice>, saved: Option<&str>) -> Option<ProgramChoice> {
    let mut shells: Vec<_> = choices
        .into_iter()
        .filter(|c| c.is_shell && c.program.is_some())
        .collect();
    let at = shells.iter().position(|c| Some(c.id) == saved).unwrap_or(0);
    (at < shells.len()).then(|| shells.swap_remove(at))
}

pub struct Finder {
    dirs: Vec<PathBuf>,
    exts: Vec<String>,
    #[cfg(windows)]
    program_files: Option<PathBuf>,
    #[cfg(unix)]
    login_shell: Option<PathBuf>,
    is_file: Box<dyn Fn(&Path) -> bool + Send + Sync>,
}

impl Finder {
    pub fn from_env() -> Self {
        let dirs = env::var_os("PATH")
            .map(|path| env::split_paths(&path).collect())
            .unwrap_or_default();
        #[cfg(windows)]
        let exts = env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned())
            .split(';')
            .filter(|ext| !ext.is_empty())
            .map(str::to_lowercase)
            .collect();
        // Unix programs carry no extension, so the bare name is the one candidate.
        #[cfg(unix)]
        let exts = vec![String::new()];
        Self {
            dirs,
            exts,
            #[cfg(windows)]
            program_files: env::var_os("ProgramFiles").map(PathBuf::from),
            #[cfg(unix)]
            login_shell: env::var_os("SHELL").map(PathBuf::from),
            #[cfg(windows)]
            is_file: Box::new(Path::is_file),
            #[cfg(unix)]
            is_file: Box::new(is_executable),
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
    #[cfg(windows)]
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
        let claude = ProgramChoice {
            id: "claude",
            label: "Claude Code",
            program: shown(self.find("claude")),
            args: Vec::new(),
            image_paste_key: Some(CLAUDE_IMAGE_PASTE),
            is_shell: false,
            install_command: Some(CLAUDE_INSTALL.0),
            install_shell: CLAUDE_INSTALL.1,
        };
        std::iter::once(claude).chain(self.shells()).collect()
    }

    #[cfg(windows)]
    fn shells(&self) -> Vec<ProgramChoice> {
        vec![
            shell(
                "powershell",
                "PowerShell",
                self.find("pwsh").or_else(|| self.find("powershell")),
                &["-NoLogo"],
            ),
            shell("git-bash", "Git Bash", self.git_bash(), &["--login", "-i"]),
        ]
    }

    /// The login shell first, so a new install opens what Terminal would.
    #[cfg(unix)]
    fn shells(&self) -> Vec<ProgramChoice> {
        let installed = |path: &str| Some(PathBuf::from(path)).filter(|p| (self.is_file)(p));
        let login = self
            .login_shell
            .as_ref()
            .filter(|p| p.is_absolute() && (self.is_file)(p));
        let mut shells = vec![
            shell("zsh", "zsh", installed("/bin/zsh"), &["-l"]),
            shell("bash", "bash", installed("/bin/bash"), &["-l"]),
        ];
        let is_login =
            |c: &ProgramChoice| login.is_some_and(|l| c.program.as_deref() == l.to_str());
        match shells.iter().position(is_login) {
            Some(at) => shells[..=at].rotate_right(1),
            None => {
                if let Some(other) = login {
                    shells.insert(
                        0,
                        shell("login-shell", "Login shell", Some(other.clone()), &["-l"]),
                    );
                }
            }
        }
        shells
    }
}

fn shown(path: Option<PathBuf>) -> Option<String> {
    path.map(|p| p.to_string_lossy().into_owned())
}

fn shell(
    id: &'static str,
    label: &'static str,
    path: Option<PathBuf>,
    args: &[&str],
) -> ProgramChoice {
    ProgramChoice {
        id,
        label,
        program: shown(path),
        args: args.iter().map(|&a| a.to_owned()).collect(),
        image_paste_key: None,
        is_shell: true,
        install_command: None,
        install_shell: None,
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[cfg(windows)]
    fn finder(path: &[&str], files: &[&str]) -> Finder {
        let files: HashSet<PathBuf> = files.iter().map(PathBuf::from).collect();
        Finder {
            dirs: path.iter().map(PathBuf::from).collect(),
            exts: vec![".exe".to_owned(), ".cmd".to_owned()],
            program_files: Some(PathBuf::from(r"C:\Program Files")),
            is_file: Box::new(move |p| files.contains(p)),
        }
    }

    #[cfg(windows)]
    #[test]
    fn find_tries_each_path_dir_with_each_extension() {
        let f = finder(&[r"C:\a", r"C:\b"], &[r"C:\b\claude.cmd", r"C:\b\claude"]);

        assert_eq!(f.find("claude"), Some(PathBuf::from(r"C:\b\claude.cmd")));
    }

    #[cfg(windows)]
    #[test]
    fn find_prefers_the_earlier_path_dir() {
        let f = finder(&[r"C:\a", r"C:\b"], &[r"C:\a\pwsh.exe", r"C:\b\pwsh.exe"]);

        assert_eq!(f.find("pwsh"), Some(PathBuf::from(r"C:\a\pwsh.exe")));
    }

    #[cfg(windows)]
    #[test]
    fn find_accepts_a_name_with_its_extension() {
        let f = finder(&[r"C:\w"], &[r"C:\w\powershell.exe"]);

        assert_eq!(
            f.find("powershell.exe"),
            Some(PathBuf::from(r"C:\w\powershell.exe"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn find_checks_a_full_path_as_given() {
        let f = finder(&[], &[r"C:\tools\x.exe"]);

        assert_eq!(
            f.find(r"C:\tools\x.exe"),
            Some(PathBuf::from(r"C:\tools\x.exe"))
        );
        assert_eq!(f.find(r"C:\tools\y.exe"), None);
    }

    #[cfg(windows)]
    #[test]
    fn find_misses_a_program_that_is_not_installed() {
        assert_eq!(finder(&[r"C:\a"], &[]).find("claude"), None);
    }

    #[cfg(windows)]
    #[test]
    fn git_bash_is_found_from_git_on_path() {
        let f = finder(
            &[r"D:\Git\cmd"],
            &[r"D:\Git\cmd\git.exe", r"D:\Git\bin\bash.exe"],
        );

        assert_eq!(f.git_bash(), Some(PathBuf::from(r"D:\Git\bin\bash.exe")));
    }

    #[cfg(windows)]
    #[test]
    fn git_bash_is_found_from_mingw_git() {
        let f = finder(
            &[r"D:\Git\mingw64\bin"],
            &[r"D:\Git\mingw64\bin\git.exe", r"D:\Git\bin\bash.exe"],
        );

        assert_eq!(f.git_bash(), Some(PathBuf::from(r"D:\Git\bin\bash.exe")));
    }

    #[cfg(windows)]
    #[test]
    fn git_bash_falls_back_to_program_files() {
        let f = finder(&[], &[r"C:\Program Files\Git\bin\bash.exe"]);

        assert_eq!(
            f.git_bash(),
            Some(PathBuf::from(r"C:\Program Files\Git\bin\bash.exe"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn the_saved_shell_is_picked_when_installed_else_the_first_one() {
        let f = finder(
            &[r"C:\w", r"D:\Git\cmd"],
            &[
                r"C:\w\powershell.exe",
                r"D:\Git\cmd\git.exe",
                r"D:\Git\bin\bash.exe",
            ],
        );
        let id = |pick: Option<ProgramChoice>| pick.map(|c| c.id);

        assert_eq!(
            id(pick_shell(f.choices(), Some("git-bash"))),
            Some("git-bash")
        );
        assert_eq!(
            id(pick_shell(f.choices(), Some("gone"))),
            Some("powershell")
        );
        assert_eq!(id(pick_shell(f.choices(), None)), Some("powershell"));
        assert_eq!(id(pick_shell(finder(&[], &[]).choices(), None)), None);
    }

    #[cfg(windows)]
    #[test]
    fn choices_mark_missing_programs() {
        let f = finder(&[r"C:\w"], &[r"C:\w\powershell.exe"]);
        let choices = f.choices();

        let ids: Vec<_> = choices.iter().map(|c| c.id).collect();
        assert_eq!(ids, ["claude", "powershell", "git-bash"]);
        assert_eq!(choices[0].program, None);
        assert_eq!(choices[1].program.as_deref(), Some(r"C:\w\powershell.exe"));
        assert_eq!(choices[2].program, None);
        let paste_keys: Vec<_> = choices.iter().map(|c| c.image_paste_key).collect();
        assert_eq!(paste_keys, [Some("\x1bv"), None, None]);
        assert_eq!(
            choices[0].install_command,
            Some("irm https://claude.ai/install.ps1 | iex")
        );
        assert_eq!(choices[0].install_shell, Some("powershell"));
        let shells: Vec<_> = choices.iter().map(|c| c.is_shell).collect();
        assert_eq!(shells, [false, true, true]);
    }

    #[cfg(unix)]
    fn unix_finder(path: &[&str], files: &[&str], login: Option<&str>) -> Finder {
        let files: HashSet<PathBuf> = files.iter().map(PathBuf::from).collect();
        Finder {
            dirs: path.iter().map(PathBuf::from).collect(),
            exts: vec![String::new()],
            login_shell: login.map(PathBuf::from),
            is_file: Box::new(move |p| files.contains(p)),
        }
    }

    #[cfg(unix)]
    #[test]
    fn find_takes_the_bare_name_from_the_earliest_path_dir() {
        let f = unix_finder(
            &["/usr/bin", "/opt/homebrew/bin", "/Users/u/.local/bin"],
            &["/opt/homebrew/bin/claude", "/Users/u/.local/bin/claude"],
            None,
        );

        assert_eq!(
            f.find("claude"),
            Some(PathBuf::from("/opt/homebrew/bin/claude"))
        );
        assert_eq!(
            f.find("/Users/u/.local/bin/claude"),
            Some(PathBuf::from("/Users/u/.local/bin/claude"))
        );
        assert_eq!(f.find("pwsh"), None);
    }

    #[cfg(unix)]
    #[test]
    fn the_login_shell_comes_first_and_is_not_listed_twice() {
        let both = ["/bin/zsh", "/bin/bash"];
        let ids = |f: &Finder| f.choices().into_iter().map(|c| c.id).collect::<Vec<_>>();

        assert_eq!(
            ids(&unix_finder(&[], &both, Some("/bin/zsh"))),
            ["claude", "zsh", "bash"]
        );
        assert_eq!(
            ids(&unix_finder(&[], &both, Some("/bin/bash"))),
            ["claude", "bash", "zsh"]
        );
        assert_eq!(
            ids(&unix_finder(&[], &both, None)),
            ["claude", "zsh", "bash"]
        );
        let fish = unix_finder(
            &[],
            &["/bin/zsh", "/bin/bash", "/opt/homebrew/bin/fish"],
            Some("/opt/homebrew/bin/fish"),
        );
        let choices = fish.choices();
        assert_eq!(choices[1].id, "login-shell");
        assert_eq!(
            choices[1].program.as_deref(),
            Some("/opt/homebrew/bin/fish")
        );
        assert_eq!(choices[1].args, ["-l"]);
        assert_eq!(choices[0].image_paste_key, Some("\x16"));
        assert_eq!(choices[1].image_paste_key, None);
        assert!(
            choices[0]
                .install_command
                .is_some_and(|c| c.contains("install.sh"))
        );
        assert_eq!(choices[0].install_shell, None);
    }

    #[cfg(unix)]
    #[test]
    fn a_missing_or_relative_login_shell_is_left_out() {
        let ids = |login| {
            unix_finder(&[], &["/bin/zsh"], Some(login))
                .choices()
                .into_iter()
                .map(|c| c.id)
                .collect::<Vec<_>>()
        };

        assert_eq!(ids("/usr/local/bin/fish"), ["claude", "zsh", "bash"]);
        assert_eq!(ids("fish"), ["claude", "zsh", "bash"]);
    }

    #[cfg(unix)]
    #[test]
    fn the_saved_shell_is_picked_when_installed_else_the_login_shell() {
        let f = || unix_finder(&[], &["/bin/zsh", "/bin/bash"], Some("/bin/bash"));
        let id = |pick: Option<ProgramChoice>| pick.map(|c| c.id);

        assert_eq!(id(pick_shell(f().choices(), Some("zsh"))), Some("zsh"));
        assert_eq!(
            id(pick_shell(f().choices(), Some("powershell"))),
            Some("bash")
        );
        assert_eq!(
            id(pick_shell(unix_finder(&[], &[], None).choices(), None)),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn executables_are_files_with_an_exec_bit() {
        use std::os::unix::fs::PermissionsExt;
        let dir = crate::accounts::testdir::TestDir::new();
        let plain = dir.path().join("plain");
        let tool = dir.path().join("tool");
        std::fs::write(&plain, "").unwrap();
        std::fs::write(&tool, "").unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(is_executable(&tool));
        assert!(!is_executable(&plain));
        assert!(!is_executable(dir.path()));
    }
}
