use std::env;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramChoice {
    pub id: String,
    pub label: String,
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
    /// Where Cerberus looked, shown while the program is missing.
    pub searched: Option<String>,
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

/// Under the home folder, where installers put `claude` and `agy` though an app started from a launcher may lack them on PATH.
#[cfg(unix)]
const INSTALL_DIRS: [&str; 4] = [".local/bin", ".claude/local", ".npm-global/bin", ".bun/bin"];

#[cfg(unix)]
fn searched() -> String {
    let dirs: Vec<String> = INSTALL_DIRS.iter().map(|d| format!("~/{d}")).collect();
    match dirs.split_last() {
        Some((last, rest)) => format!("Looked on PATH and in {} and {last}.", rest.join(", ")),
        None => "Looked on PATH.".to_owned(),
    }
}

#[cfg(windows)]
fn searched() -> String {
    "Looked on PATH.".to_owned()
}

/// Other AI CLIs a tab can run as a plain program, by id and label.
const OTHER_CLIS: [(&str, &str); 2] = [("agy", "Antigravity CLI"), ("codex", "Codex CLI")];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Claude,
    /// Antigravity CLI or Codex CLI: no account, bridge, live cards or chat tracking.
    OtherCli,
    Shell,
}

/// What a tab's program is, read from its file name the way `is_claude` reads it.
pub fn kind_of(program: &str) -> Kind {
    if crate::sessions::is_claude(program) {
        return Kind::Claude;
    }
    let stem = Path::new(program).file_stem().and_then(|s| s.to_str());
    if stem.is_some_and(|stem| {
        OTHER_CLIS
            .iter()
            .any(|(id, _)| stem.eq_ignore_ascii_case(id))
    }) {
        Kind::OtherCli
    } else {
        Kind::Shell
    }
}

/// The shell the Shell button opens: the saved choice when installed, else the first shell installed.
pub fn pick_shell(choices: Vec<ProgramChoice>, saved: Option<&str>) -> Option<ProgramChoice> {
    let mut shells: Vec<_> = choices
        .into_iter()
        .filter(|c| c.is_shell && c.program.is_some())
        .collect();
    let at = shells
        .iter()
        .position(|c| saved == Some(c.id.as_str()))
        .unwrap_or(0);
    (at < shells.len()).then(|| shells.swap_remove(at))
}

pub struct Finder {
    dirs: Vec<PathBuf>,
    /// Searched after PATH, so a program the login shell's PATH misses is still found.
    fallback: Vec<PathBuf>,
    exts: Vec<String>,
    #[cfg(windows)]
    program_files: Option<PathBuf>,
    #[cfg(unix)]
    login_shell: Option<PathBuf>,
    #[cfg(unix)]
    etc_shells: String,
    /// Where Antigravity's installer puts `agy`, which an app started before the install has no PATH entry for.
    agy_dirs: Vec<PathBuf>,
    is_file: Box<dyn Fn(&Path) -> bool + Send + Sync>,
    is_antigravity: Box<dyn Fn(&Path) -> bool + Send + Sync>,
}

impl Finder {
    pub fn from_env() -> Self {
        let dirs = crate::reach::answering(
            env::var_os("PATH")
                .map(|path| env::split_paths(&path).collect())
                .unwrap_or_default(),
        );
        #[cfg(windows)]
        let exts = launchable_exts(
            &env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned()),
        );
        // Unix programs carry no extension, so the bare name is the one candidate.
        #[cfg(unix)]
        let exts = vec![String::new()];
        #[cfg(unix)]
        let fallback = env::var_os("HOME")
            .map(|home| {
                INSTALL_DIRS
                    .iter()
                    .map(|d| Path::new(&home).join(d))
                    .collect()
            })
            .unwrap_or_default();
        #[cfg(windows)]
        let fallback = Vec::new();
        Self {
            dirs,
            fallback,
            exts,
            #[cfg(windows)]
            program_files: env::var_os("ProgramFiles").map(PathBuf::from),
            #[cfg(unix)]
            login_shell: env::var_os("SHELL").map(PathBuf::from),
            // A system without the file simply offers fewer shells.
            #[cfg(unix)]
            etc_shells: std::fs::read_to_string("/etc/shells").unwrap_or_default(),
            #[cfg(windows)]
            agy_dirs: env::var_os("LOCALAPPDATA")
                .map(|dir| PathBuf::from(dir).join("agy").join("bin"))
                .into_iter()
                .collect(),
            #[cfg(unix)]
            agy_dirs: env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(".local").join("bin"))
                .into_iter()
                .collect(),
            #[cfg(windows)]
            is_file: Box::new(Path::is_file),
            #[cfg(unix)]
            is_file: Box::new(is_executable),
            is_antigravity: Box::new(crate::agy::is_antigravity),
        }
    }

    /// The same search without one folder, so the `claude` wrapper never finds itself.
    pub fn without(mut self, skip: &Path) -> Self {
        let key = |dir: &Path| {
            let text = dir.to_string_lossy();
            let text = text.trim_end_matches(['/', '\\']);
            // Windows folder names ignore case; elsewhere they don't.
            if cfg!(windows) {
                text.to_lowercase()
            } else {
                text.to_owned()
            }
        };
        let skip = key(skip);
        self.dirs.retain(|dir| key(dir) != skip);
        self.fallback.retain(|dir| key(dir) != skip);
        self
    }

    /// Resolves a name the way `cmd` does: a path as given, else each PATH dir with each PATHEXT, then the fallback dirs.
    pub fn find(&self, name: &str) -> Option<PathBuf> {
        let given = Path::new(name);
        if given.components().count() > 1 || given.is_absolute() {
            return (self.is_file)(given).then(|| given.to_path_buf());
        }
        // A bare name without an extension would match npm's extensionless sh shims.
        let has_ext = given.extension().is_some();
        self.dirs.iter().chain(&self.fallback).find_map(|dir| {
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

    /// Antigravity's `agy`: the first on PATH, then in the installer's folder, that is Antigravity's.
    fn agy(&self) -> Option<PathBuf> {
        // Another program may be called `agy`, so one outside the installer's folder or a folder naming it must prove it.
        let ours = |candidate: &PathBuf| {
            let dir = candidate.parent().unwrap_or(candidate);
            self.agy_dirs.iter().any(|d| d == dir)
                || crate::agy::names_it(dir)
                || (self.is_antigravity)(candidate)
        };
        self.dirs
            .iter()
            .chain(&self.agy_dirs)
            .flat_map(|dir| {
                self.exts
                    .iter()
                    .map(move |ext| dir.join(format!("agy{ext}")))
            })
            .filter(|candidate| (self.is_file)(candidate))
            .find(ours)
    }

    pub fn choices(&self) -> Vec<ProgramChoice> {
        let program = shown(self.find("claude"));
        let claude = ProgramChoice {
            id: "claude".to_owned(),
            label: "Claude Code".to_owned(),
            searched: program.is_none().then(searched),
            program,
            args: Vec::new(),
            image_paste_key: Some(CLAUDE_IMAGE_PASTE),
            is_shell: false,
            install_command: Some(CLAUDE_INSTALL.0),
            install_shell: CLAUDE_INSTALL.1,
        };
        let others = OTHER_CLIS.iter().map(|&(id, label)| ProgramChoice {
            id: id.to_owned(),
            label: label.to_owned(),
            program: shown(if id == "agy" {
                self.agy()
            } else {
                self.find(id)
            }),
            args: Vec::new(),
            image_paste_key: None,
            is_shell: false,
            install_command: None,
            install_shell: None,
            searched: None,
        });
        std::iter::once(claude)
            .chain(self.shells())
            .chain(others)
            .collect()
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

    #[cfg(unix)]
    fn shells(&self) -> Vec<ProgramChoice> {
        let login = self.login_shell.as_deref().and_then(Path::to_str);
        unix_shells(&self.etc_shells, login, |path| {
            (self.is_file)(Path::new(path)).then(|| {
                std::fs::canonicalize(path)
                    .map_or_else(|_| path.to_owned(), |p| p.to_string_lossy().into_owned())
            })
        })
    }
}

// Listed ahead of `/etc/shells`, whose macOS order puts csh, dash, ksh and sh before zsh; Homebrew's fish is often missing from it.
#[cfg(any(unix, test))]
const KNOWN_SHELLS: [&str; 5] = [
    "/bin/zsh",
    "/bin/bash",
    "/opt/homebrew/bin/fish",
    "/usr/local/bin/fish",
    "/usr/bin/fish",
];

// Linux lists these in `/etc/shells`, but none is a shell to open a tab in.
#[cfg(any(unix, test))]
const NOT_SHELLS: [&str; 7] = [
    "nologin",
    "false",
    "true",
    "rbash",
    "tmux",
    "screen",
    "git-shell",
];

/// Every installed shell in `/etc/shells` and Homebrew, login shell first, each real file once per name (bash run as `sh` acts differently).
#[cfg(any(unix, test))]
fn unix_shells(
    etc_shells: &str,
    login: Option<&str>,
    real: impl Fn(&str) -> Option<String>,
) -> Vec<ProgramChoice> {
    let base = |path: &str| path.rsplit('/').next().unwrap_or_default().to_owned();
    let login = login
        .filter(|l| l.starts_with('/'))
        .and_then(|l| Some((l, (real(l)?, base(l)))));
    let listed = etc_shells
        .lines()
        .map(|line| line.split('#').next().unwrap_or_default().trim())
        .filter(|line| line.starts_with('/'));
    let mut paths: Vec<&str> = Vec::new();
    let mut seen: Vec<(String, String)> = Vec::new();
    for path in KNOWN_SHELLS
        .into_iter()
        .chain(listed)
        .chain(login.as_ref().map(|(l, _)| *l))
    {
        let name = base(path);
        let Some(target) = real(path).filter(|_| !NOT_SHELLS.contains(&name.as_str())) else {
            continue;
        };
        let key = (target, name);
        if !paths.contains(&path) && !seen.contains(&key) {
            paths.push(path);
            seen.push(key);
        }
    }
    let name_of = base;
    // Ids are saved with each tab, so the bare name goes to the system copy wherever others are listed.
    let owner = |name: &str| {
        let named = || paths.iter().copied().filter(|p| name_of(p) == name);
        named()
            .find(|p| p.starts_with("/bin/") || p.starts_with("/usr/bin/"))
            .or_else(|| named().next())
    };
    let mut shells: Vec<ProgramChoice> = Vec::new();
    for &path in &paths {
        let name = name_of(path);
        let id = if !name.is_empty() && owner(&name) == Some(path) {
            name.as_str()
        } else {
            path
        };
        shells.push(shell(id, id, Some(PathBuf::from(path)), &["-l"]));
    }
    if let Some(at) = login
        .as_ref()
        .and_then(|(_, key)| seen.iter().position(|s| s == key))
    {
        shells[..=at].rotate_right(1);
    }
    shells
}

/// PATHEXT's entries a tab can start: npm puts a `.ps1` beside each `.cmd`, and a PTY cannot run a script file.
#[cfg(any(windows, test))]
fn launchable_exts(pathext: &str) -> Vec<String> {
    pathext
        .split(';')
        .map(str::to_lowercase)
        .filter(|ext| [".com", ".exe", ".bat", ".cmd"].contains(&ext.as_str()))
        .collect()
}

fn shown(path: Option<PathBuf>) -> Option<String> {
    path.map(|p| p.to_string_lossy().into_owned())
}

fn shell(id: &str, label: &str, path: Option<PathBuf>, args: &[&str]) -> ProgramChoice {
    ProgramChoice {
        id: id.to_owned(),
        label: label.to_owned(),
        program: shown(path),
        args: args.iter().map(|&a| a.to_owned()).collect(),
        image_paste_key: None,
        is_shell: true,
        install_command: None,
        install_shell: None,
        searched: None,
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
            fallback: Vec::new(),
            exts: vec![".exe".to_owned(), ".cmd".to_owned()],
            program_files: Some(PathBuf::from(r"C:\Program Files")),
            agy_dirs: vec![PathBuf::from(r"C:\Users\u\AppData\Local\agy\bin")],
            is_file: Box::new(move |p| files.contains(p)),
            is_antigravity: Box::new(|_| false),
        }
    }

    #[cfg(windows)]
    #[test]
    fn without_skips_the_wrapper_folder_in_any_case_or_trailing_slash() {
        let f = finder(
            &[r"C:\App\Bin\", r"C:\real"],
            &[r"C:\App\Bin\claude.exe", r"C:\real\claude.exe"],
        );

        let found = f.without(Path::new(r"c:\app\bin")).find("claude");

        assert_eq!(found, Some(PathBuf::from(r"C:\real\claude.exe")));
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

    fn id(pick: Option<ProgramChoice>) -> Option<String> {
        pick.map(|c| c.id)
    }

    fn ids(choices: &[ProgramChoice]) -> Vec<&str> {
        choices.iter().map(|c| c.id.as_str()).collect()
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

        assert_eq!(
            id(pick_shell(f.choices(), Some("git-bash"))).as_deref(),
            Some("git-bash")
        );
        assert_eq!(
            id(pick_shell(f.choices(), Some("gone"))).as_deref(),
            Some("powershell")
        );
        assert_eq!(
            id(pick_shell(f.choices(), None)).as_deref(),
            Some("powershell")
        );
        assert_eq!(id(pick_shell(finder(&[], &[]).choices(), None)), None);
    }

    #[cfg(windows)]
    #[test]
    fn choices_mark_missing_programs() {
        let f = finder(&[r"C:\w"], &[r"C:\w\powershell.exe"]);
        let choices = f.choices();

        assert_eq!(
            ids(&choices),
            ["claude", "powershell", "git-bash", "agy", "codex"]
        );
        assert_eq!(choices[0].program, None);
        assert_eq!(choices[1].program.as_deref(), Some(r"C:\w\powershell.exe"));
        assert_eq!(choices[2].program, None);
        assert_eq!(choices[3].program, None);
        assert_eq!(choices[4].program, None);
        let paste_keys: Vec<_> = choices.iter().map(|c| c.image_paste_key).collect();
        assert_eq!(paste_keys, [Some("\x1bv"), None, None, None, None]);
        assert_eq!(
            choices[0].install_command,
            Some("irm https://claude.ai/install.ps1 | iex")
        );
        assert_eq!(choices[0].install_shell, Some("powershell"));
        assert_eq!(choices[0].searched.as_deref(), Some("Looked on PATH."));
        assert_eq!(choices[1].searched, None);
        let shells: Vec<_> = choices.iter().map(|c| c.is_shell).collect();
        assert_eq!(shells, [false, true, true, false, false]);
    }

    #[cfg(windows)]
    fn program_of(choices: &[ProgramChoice], id: &str) -> Option<String> {
        choices.iter().find(|c| c.id == id)?.program.clone()
    }

    #[cfg(windows)]
    #[test]
    fn antigravity_and_codex_are_offered_from_path_with_their_labels() {
        let f = finder(
            &[r"C:\Users\u\AppData\Local\agy\bin", r"C:\tools"],
            &[
                r"C:\Users\u\AppData\Local\agy\bin\agy.exe",
                r"C:\tools\codex.exe",
            ],
        );
        let choices = f.choices();
        let other = |id: &str| choices.iter().find(|c| c.id == id).unwrap();

        assert_eq!(other("agy").label, "Antigravity CLI");
        assert_eq!(
            program_of(&choices, "agy").as_deref(),
            Some(r"C:\Users\u\AppData\Local\agy\bin\agy.exe")
        );
        assert_eq!(other("codex").label, "Codex CLI");
        assert_eq!(
            program_of(&choices, "codex").as_deref(),
            Some(r"C:\tools\codex.exe")
        );
        assert!(!other("codex").is_shell);
        assert_eq!(id(pick_shell(choices, Some("agy"))), None);
    }

    #[cfg(windows)]
    #[test]
    fn agy_is_found_in_its_install_folder_when_path_lacks_it() {
        let f = finder(&[r"C:\w"], &[r"C:\Users\u\AppData\Local\agy\bin\agy.exe"]);

        assert_eq!(
            program_of(&f.choices(), "agy").as_deref(),
            Some(r"C:\Users\u\AppData\Local\agy\bin\agy.exe")
        );
    }

    #[cfg(windows)]
    #[test]
    fn an_agy_on_path_counts_only_from_a_folder_that_names_antigravity_or_agy() {
        let named = finder(
            &[r"C:\Program Files\Google\Antigravity\bin"],
            &[r"C:\Program Files\Google\Antigravity\bin\agy.exe"],
        );
        let unrelated = finder(&[r"C:\tools"], &[r"C:\tools\agy.exe"]);
        let both = finder(
            &[r"C:\tools"],
            &[
                r"C:\tools\agy.exe",
                r"C:\Users\u\AppData\Local\agy\bin\agy.cmd",
            ],
        );

        assert_eq!(
            program_of(&named.choices(), "agy").as_deref(),
            Some(r"C:\Program Files\Google\Antigravity\bin\agy.exe")
        );
        assert_eq!(program_of(&unrelated.choices(), "agy"), None);
        assert_eq!(
            program_of(&both.choices(), "agy").as_deref(),
            Some(r"C:\Users\u\AppData\Local\agy\bin\agy.cmd")
        );
    }

    #[cfg(windows)]
    #[test]
    fn an_unrelated_agy_earlier_on_path_does_not_hide_antigravitys_later_one() {
        let f = finder(
            &[r"C:\tools", r"D:\Antigravity\bin"],
            &[r"C:\tools\agy.exe", r"D:\Antigravity\bin\agy.exe"],
        );

        assert_eq!(
            program_of(&f.choices(), "agy").as_deref(),
            Some(r"D:\Antigravity\bin\agy.exe")
        );
    }

    #[test]
    fn pathext_keeps_only_what_a_tab_can_start_in_its_order() {
        assert_eq!(
            launchable_exts(".COM;.PS1;.EXE;.BAT;.CMD;.VBS;.JS;;.Py"),
            [".com", ".exe", ".bat", ".cmd"]
        );
        assert_eq!(launchable_exts(".CMD;.EXE"), [".cmd", ".exe"]);
    }

    #[test]
    fn kind_of_tells_claude_the_other_clis_and_shells_apart() {
        assert_eq!(kind_of("/Users/u/.local/bin/claude"), Kind::Claude);
        assert_eq!(kind_of("/home/u/.local/bin/agy"), Kind::OtherCli);
        assert_eq!(kind_of("/usr/local/bin/codex"), Kind::OtherCli);
        assert_eq!(kind_of("/usr/local/bin/codex-helper"), Kind::Shell);
        assert_eq!(kind_of("/opt/homebrew/bin/gemini"), Kind::Shell);
        assert_eq!(kind_of("/bin/zsh"), Kind::Shell);
    }

    // Backslashes separate folders only on Windows.
    #[cfg(windows)]
    #[test]
    fn kind_of_reads_windows_paths() {
        assert_eq!(kind_of(r"C:\Users\u\.local\bin\claude.exe"), Kind::Claude);
        assert_eq!(
            kind_of(r"C:\Users\u\AppData\Local\agy\bin\agy.exe"),
            Kind::OtherCli
        );
        assert_eq!(kind_of(r"C:\npm\Codex.CMD"), Kind::OtherCli);
        assert_eq!(kind_of(r"C:\tools\codex-helper.exe"), Kind::Shell);
        assert_eq!(kind_of(r"C:\w\powershell.exe"), Kind::Shell);
    }

    const MAC_ETC_SHELLS: &str = "# List of acceptable shells for chpass(1).
# Ftpd will not allow users to connect who are not using
# one of these shells.

/bin/bash
/bin/csh
/bin/dash
/bin/ksh
/bin/sh
/bin/tcsh
/bin/zsh
";

    fn shells_with(etc_shells: &str, files: &[&str], login: Option<&str>) -> Vec<ProgramChoice> {
        unix_shells(etc_shells, login, |path| {
            files.contains(&path).then(|| path.to_owned())
        })
    }

    const UBUNTU_ETC_SHELLS: &str = "# /etc/shells: valid login shells
/bin/sh
/usr/bin/sh
/bin/bash
/usr/bin/bash
/bin/rbash
/usr/bin/rbash
/usr/bin/dash
/usr/bin/tmux
/usr/bin/screen
/bin/zsh
/usr/bin/zsh
/usr/bin/fish
";

    // Ubuntu's merged /usr: each /bin path is a link into /usr/bin, and sh is dash.
    fn ubuntu_real(path: &str) -> Option<String> {
        let real = path.replace("/bin/sh", "/bin/dash");
        let real = real
            .strip_prefix("/usr")
            .map_or(real.clone(), str::to_owned);
        UBUNTU_ETC_SHELLS
            .lines()
            .any(|line| line == path)
            .then(|| format!("/usr{real}"))
    }

    #[test]
    fn ubuntu_lists_each_real_shell_once_and_no_multiplexers() {
        let ids_for = |login| ids(&unix_shells(UBUNTU_ETC_SHELLS, login, ubuntu_real)).join(" ");

        assert_eq!(ids_for(Some("/bin/bash")), "bash zsh fish sh dash");
        assert_eq!(ids_for(Some("/usr/bin/bash")), "bash zsh fish sh dash");
        assert_eq!(ids_for(Some("/usr/bin/fish")), "fish zsh bash sh dash");
        assert_eq!(ids_for(None), "zsh bash fish sh dash");
        let shells = unix_shells(UBUNTU_ETC_SHELLS, Some("/usr/bin/bash"), ubuntu_real);
        assert_eq!(shells[0].program.as_deref(), Some("/bin/bash"));
    }

    #[test]
    fn sh_linked_to_bash_stays_its_own_choice_and_can_be_the_login_shell() {
        // Fedora and Arch link sh to bash, which then runs in POSIX mode with other startup files.
        let etc = "/bin/sh
/bin/bash
/usr/bin/sh
/usr/bin/bash
";
        let real = |path: &str| {
            etc.lines()
                .any(|line| line == path)
                .then(|| "/usr/bin/bash".to_owned())
        };

        let shells = unix_shells(etc, Some("/bin/sh"), real);

        assert_eq!(ids(&shells), ["sh", "bash"]);
        assert_eq!(shells[0].program.as_deref(), Some("/bin/sh"));
        assert_eq!(
            ids(&unix_shells(etc, Some("/usr/bin/bash"), real)),
            ["bash", "sh"]
        );
    }

    #[test]
    fn every_installed_shell_in_etc_shells_is_offered_with_l() {
        let files = ["/bin/bash", "/bin/sh", "/bin/tcsh", "/bin/zsh"];

        let shells = shells_with(MAC_ETC_SHELLS, &files, None);

        assert_eq!(ids(&shells), ["zsh", "bash", "sh", "tcsh"]);
        assert!(shells.iter().all(|s| s.is_shell && s.args == ["-l"]));
        assert_eq!(shells[3].program.as_deref(), Some("/bin/tcsh"));
        assert_eq!(shells[3].label, "tcsh");
    }

    #[test]
    fn a_stock_mac_lists_the_login_shell_then_zsh_bash_and_fish_then_the_rest() {
        let files = [
            "/bin/bash",
            "/bin/csh",
            "/bin/dash",
            "/bin/ksh",
            "/bin/sh",
            "/bin/tcsh",
            "/bin/zsh",
            "/opt/homebrew/bin/fish",
        ];
        let order = |login| ids(&shells_with(MAC_ETC_SHELLS, &files, Some(login))).join(" ");

        assert_eq!(order("/bin/zsh"), "zsh bash fish csh dash ksh sh tcsh");
        assert_eq!(
            order("/opt/homebrew/bin/fish"),
            "fish zsh bash csh dash ksh sh tcsh"
        );
        assert_eq!(order("/bin/ksh"), "ksh zsh bash fish csh dash sh tcsh");
    }

    #[test]
    fn the_login_shell_comes_first_and_is_not_listed_twice() {
        let files = ["/bin/bash", "/bin/zsh"];

        assert_eq!(
            ids(&shells_with(MAC_ETC_SHELLS, &files, Some("/bin/zsh"))),
            ["zsh", "bash"]
        );
        assert_eq!(
            ids(&shells_with(MAC_ETC_SHELLS, &files, Some("/bin/bash"))),
            ["bash", "zsh"]
        );
    }

    #[test]
    fn homebrew_fish_is_offered_when_present_even_outside_etc_shells() {
        let arm = ["/bin/zsh", "/opt/homebrew/bin/fish"];
        let intel = ["/bin/zsh", "/usr/local/bin/fish"];

        let shells = shells_with(MAC_ETC_SHELLS, &arm, Some("/opt/homebrew/bin/fish"));
        assert_eq!(ids(&shells), ["fish", "zsh"]);
        assert_eq!(shells[0].program.as_deref(), Some("/opt/homebrew/bin/fish"));
        assert_eq!(shells[0].args, ["-l"]);
        assert_eq!(
            ids(&shells_with(MAC_ETC_SHELLS, &intel, None)),
            ["zsh", "fish"]
        );
    }

    #[test]
    fn a_shell_listed_in_etc_shells_and_known_to_cerberus_appears_once() {
        let etc = "/bin/zsh\n/opt/homebrew/bin/fish\n/bin/zsh\n";
        let files = ["/bin/zsh", "/opt/homebrew/bin/fish"];

        let shells = shells_with(etc, &files, Some("/opt/homebrew/bin/fish"));

        assert_eq!(ids(&shells), ["fish", "zsh"]);
    }

    #[test]
    fn a_second_shell_of_the_same_name_is_told_apart_by_its_path() {
        let files = ["/bin/bash", "/opt/homebrew/bin/bash"];

        for etc in [
            "/bin/bash\n/opt/homebrew/bin/bash\n",
            "/opt/homebrew/bin/bash\n/bin/bash\n",
        ] {
            let shells = shells_with(etc, &files, Some("/opt/homebrew/bin/bash"));
            let bash = shells.iter().find(|s| s.id == "bash").unwrap();

            assert_eq!(shells[0].id, "/opt/homebrew/bin/bash", "{etc}");
            assert_eq!(shells[0].label, "/opt/homebrew/bin/bash");
            assert_eq!(bash.program.as_deref(), Some("/bin/bash"), "{etc}");
        }
    }

    #[test]
    fn a_moved_homebrew_shell_keeps_its_id() {
        let arm = shells_with("", &["/opt/homebrew/bin/fish"], None);
        let intel = shells_with("", &["/usr/local/bin/fish"], None);

        assert_eq!(ids(&arm), ["fish"]);
        assert_eq!(ids(&intel), ["fish"]);
    }

    #[test]
    fn comments_blank_lines_and_relative_names_in_etc_shells_are_skipped() {
        let etc = "  # a comment\n\nfish\n /bin/ksh # trailing note\n";
        let files = ["fish", "/bin/ksh"];

        assert_eq!(ids(&shells_with(etc, &files, None)), ["ksh"]);
    }

    #[test]
    fn a_missing_or_relative_login_shell_is_left_out() {
        let files = ["/bin/zsh"];

        assert_eq!(
            ids(&shells_with("", &files, Some("/usr/local/bin/elvish"))),
            ["zsh"]
        );
        assert_eq!(ids(&shells_with("", &files, Some("zsh"))), ["zsh"]);
        assert!(shells_with("", &[], None).is_empty());
    }

    #[test]
    fn a_login_shell_found_nowhere_else_is_still_offered_first() {
        let files = ["/bin/zsh", "/opt/local/bin/nu"];

        let shells = shells_with("", &files, Some("/opt/local/bin/nu"));

        assert_eq!(ids(&shells), ["nu", "zsh"]);
    }

    #[test]
    fn the_saved_shell_is_picked_when_installed_else_the_login_shell() {
        let shells = || {
            shells_with(
                MAC_ETC_SHELLS,
                &["/bin/zsh", "/bin/bash"],
                Some("/bin/bash"),
            )
        };

        assert_eq!(
            id(pick_shell(shells(), Some("zsh"))).as_deref(),
            Some("zsh")
        );
        assert_eq!(
            id(pick_shell(shells(), Some("powershell"))).as_deref(),
            Some("bash")
        );
    }

    #[cfg(unix)]
    fn unix_finder(path: &[&str], files: &[&str], login: Option<&str>) -> Finder {
        let files: HashSet<PathBuf> = files.iter().map(PathBuf::from).collect();
        Finder {
            dirs: path.iter().map(PathBuf::from).collect(),
            fallback: INSTALL_DIRS
                .iter()
                .map(|d| Path::new("/Users/u").join(d))
                .collect(),
            exts: vec![String::new()],
            login_shell: login.map(PathBuf::from),
            etc_shells: MAC_ETC_SHELLS.to_owned(),
            agy_dirs: vec![PathBuf::from("/Users/u/.local/bin")],
            is_file: Box::new(move |p| files.contains(p)),
            // Stands in for the link and help checks, which `agy`'s own tests cover on real files.
            is_antigravity: Box::new(|p| p.starts_with("/usr/local/bin")),
        }
    }

    #[cfg(unix)]
    #[test]
    fn an_agy_on_path_that_proves_to_be_antigravitys_is_found_and_another_is_not() {
        let program = |f: Finder| f.choices().into_iter().find(|c| c.id == "agy")?.program;

        assert_eq!(
            program(unix_finder(
                &["/usr/bin", "/usr/local/bin"],
                &["/usr/bin/agy", "/usr/local/bin/agy"],
                None
            ))
            .as_deref(),
            Some("/usr/local/bin/agy")
        );
        assert_eq!(
            program(unix_finder(&["/usr/bin"], &["/usr/bin/agy"], None)),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn claude_is_found_in_its_install_folders_when_path_lacks_them() {
        for dir in [
            "/Users/u/.local/bin",
            "/Users/u/.claude/local",
            "/Users/u/.npm-global/bin",
            "/Users/u/.bun/bin",
        ] {
            let claude = format!("{dir}/claude");
            let f = unix_finder(&["/usr/bin"], &[&claude], None);

            assert_eq!(f.find("claude"), Some(PathBuf::from(&claude)), "{dir}");
            let choice = f.choices().into_iter().next().unwrap();
            assert_eq!(choice.program.as_deref(), Some(claude.as_str()));
            assert_eq!(choice.searched, None);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_path_claude_comes_before_the_install_folders_and_a_missing_one_says_where_it_looked() {
        let f = unix_finder(
            &["/opt/homebrew/bin"],
            &["/opt/homebrew/bin/claude", "/Users/u/.local/bin/claude"],
            None,
        );
        assert_eq!(
            f.find("claude"),
            Some(PathBuf::from("/opt/homebrew/bin/claude"))
        );

        let missing = unix_finder(&["/usr/bin"], &[], None).choices();
        assert_eq!(
            missing[0].searched.as_deref(),
            Some(
                "Looked on PATH and in ~/.local/bin, ~/.claude/local, ~/.npm-global/bin and ~/.bun/bin."
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn agy_comes_from_local_bin_or_a_folder_that_names_it() {
        let program = |f: Finder| f.choices().into_iter().find(|c| c.id == "agy")?.program;

        assert_eq!(
            program(unix_finder(&[], &["/Users/u/.local/bin/agy"], None)).as_deref(),
            Some("/Users/u/.local/bin/agy")
        );
        assert_eq!(
            program(unix_finder(
                &["/opt/antigravity/bin"],
                &["/opt/antigravity/bin/agy"],
                None
            ))
            .as_deref(),
            Some("/opt/antigravity/bin/agy")
        );
        assert_eq!(
            program(unix_finder(&["/usr/bin"], &["/usr/bin/agy"], None)),
            None
        );
        assert_eq!(
            program(unix_finder(
                &["/usr/bin", "/opt/antigravity/bin"],
                &["/usr/bin/agy", "/opt/antigravity/bin/agy"],
                None
            ))
            .as_deref(),
            Some("/opt/antigravity/bin/agy")
        );
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
    fn choices_list_claude_then_the_shells_from_etc_shells() {
        let f = unix_finder(
            &[],
            &["/bin/zsh", "/bin/bash", "/opt/homebrew/bin/fish"],
            Some("/opt/homebrew/bin/fish"),
        );
        let choices = f.choices();

        assert_eq!(
            ids(&choices),
            ["claude", "fish", "zsh", "bash", "agy", "codex"]
        );
        assert_eq!(choices[0].image_paste_key, Some("\x16"));
        assert_eq!(choices[1].image_paste_key, None);
        assert!(
            choices[0]
                .install_command
                .is_some_and(|c| c.contains("install.sh"))
        );
        assert_eq!(choices[0].install_shell, None);
        assert_eq!(id(pick_shell(choices, None)).as_deref(), Some("fish"));
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
