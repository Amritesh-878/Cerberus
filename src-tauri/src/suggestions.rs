use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

// A pinned release; its copyright and licence travel in the file's own header.
const ZSH_AUTOSUGGESTIONS: &str =
    include_str!("../vendor/zsh-autosuggestions/zsh-autosuggestions.zsh");
const PLUGIN_FILE: &str = "zsh-autosuggestions.zsh";
const ZSH_STARTUP_FILES: [&str; 4] = [".zshenv", ".zprofile", ".zshrc", ".zlogin"];

// Runs after the profile; PSReadLine 2.0 has no predictions, and a source already chosen is kept.
const PWSH_PREDICTIONS: &str = "& { $m = Get-Module PSReadLine; if ($m -and $m.Version -ge [version]'2.1' -and (Get-PSReadLineOption).PredictionSource -eq 'None') { Set-PSReadLineOption -PredictionSource History } }";
// Only arguments that open a plain interactive session, so a tab that runs a script is left alone.
const PWSH_PLAIN_ARGS: [&str; 4] = ["-nologo", "-noprofile", "-l", "-login"];

/// What a shell tab starts with instead, so the shell suggests from history as you type.
#[derive(Debug, PartialEq, Eq)]
pub struct Setup {
    pub args: Vec<String>,
    pub env: Vec<(&'static str, OsString)>,
}

#[derive(Debug, PartialEq, Eq)]
enum Shell {
    Zsh,
    PowerShell,
}

pub struct Suggestions {
    zsh_dir: PathBuf,
    user_zdotdir: Option<OsString>,
    // MSYS2's zsh on Windows would get a Windows path in `ZDOTDIR`.
    zsh_supported: bool,
    zsh_written: Mutex<bool>,
}

impl Suggestions {
    pub fn new(app_data_dir: &Path) -> Self {
        Self {
            zsh_dir: app_data_dir.join("shell").join("zsh"),
            user_zdotdir: std::env::var_os("ZDOTDIR"),
            zsh_supported: cfg!(unix),
            zsh_written: Mutex::new(false),
        }
    }

    /// `None` starts the program exactly as asked: the setting is off, the tab runs Claude Code, or the shell needs nothing.
    pub fn for_tab(
        &self,
        on: bool,
        runs_claude: bool,
        program: &str,
        args: &[String],
    ) -> Option<Setup> {
        if !on || runs_claude {
            return None;
        }
        match shell_of(program)? {
            Shell::Zsh => self.zsh(args),
            Shell::PowerShell => Some(Setup {
                args: pwsh_args(args)?,
                env: Vec::new(),
            }),
        }
    }

    fn zsh(&self, args: &[String]) -> Option<Setup> {
        if !self.zsh_supported {
            return None;
        }
        let mut written = self
            .zsh_written
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !*written {
            // The tab still opens, only without suggestions.
            if let Err(e) = write_zsh_dir(&self.zsh_dir) {
                eprintln!("could not set up zsh suggestions: {e}");
                return None;
            }
            *written = true;
        }
        Some(Setup {
            args: args.to_vec(),
            env: zsh_env(&self.zsh_dir, self.user_zdotdir.clone()),
        })
    }
}

fn shell_of(program: &str) -> Option<Shell> {
    let name = Path::new(program)
        .file_stem()?
        .to_str()?
        .to_ascii_lowercase();
    match name.as_str() {
        "zsh" => Some(Shell::Zsh),
        "pwsh" | "powershell" => Some(Shell::PowerShell),
        _ => None,
    }
}

fn pwsh_args(args: &[String]) -> Option<Vec<String>> {
    let plain = args
        .iter()
        .all(|a| PWSH_PLAIN_ARGS.contains(&a.to_ascii_lowercase().as_str()));
    if !plain {
        return None;
    }
    let mut all = args.to_vec();
    all.extend(["-NoExit", "-Command", PWSH_PREDICTIONS].map(str::to_owned));
    Some(all)
}

fn zsh_env(wrapper: &Path, user_zdotdir: Option<OsString>) -> Vec<(&'static str, OsString)> {
    let mut env = vec![("ZDOTDIR", wrapper.as_os_str().to_owned())];
    // Without it the wrapper reads your files from `HOME`, as zsh itself would.
    env.extend(
        user_zdotdir
            .filter(|dir| !dir.is_empty())
            .map(|dir| ("CERBERUS_USER_ZDOTDIR", dir)),
    );
    env
}

fn zsh_files() -> Vec<(&'static str, String)> {
    ZSH_STARTUP_FILES
        .into_iter()
        .map(|name| (name, zsh_startup_file(name)))
        .chain(std::iter::once((
            PLUGIN_FILE,
            ZSH_AUTOSUGGESTIONS.to_owned(),
        )))
        .collect()
}

fn write_zsh_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    for (name, text) in zsh_files() {
        let path = dir.join(name);
        if fs::read_to_string(&path).is_ok_and(|old| old == text) {
            continue;
        }
        fs::write(path, text)?;
    }
    Ok(())
}

// Puts `ZDOTDIR` back to yours, or unsets it when yours is `HOME`, as it was before Cerberus pointed it here.
const RESTORE: &str = r#"if [[ $CERBERUS_USER_ZDOTDIR == "$HOME" ]]; then
  builtin unset ZDOTDIR
else
  ZDOTDIR=$CERBERUS_USER_ZDOTDIR
fi
"#;
const FORGET: &str = "builtin unset CERBERUS_USER_ZDOTDIR _cerberus_wrapper\n";

/// Each file runs yours from your `ZDOTDIR` (else `HOME`), then points zsh back here for the next one.
fn zsh_startup_file(name: &str) -> String {
    let mut text = format!(
        r#"# Written by Cerberus for its shell tabs: runs your own {name} from your ZDOTDIR or HOME.
_cerberus_wrapper=$ZDOTDIR
ZDOTDIR=${{CERBERUS_USER_ZDOTDIR:-$HOME}}
"#
    );
    if name == ".zshrc" {
        // macOS's /etc/zshrc names the history file after `ZDOTDIR`, which pointed here while it ran.
        text.push_str(
            r#"if [[ ${HISTFILE-} == "$_cerberus_wrapper/.zsh_history" ]]; then
  HISTFILE=$ZDOTDIR/.zsh_history
fi
"#,
        );
    }
    text.push_str(&format!(
        r#"if [[ $ZDOTDIR != "$_cerberus_wrapper" && -f $ZDOTDIR/{name} ]]; then
  builtin source "$ZDOTDIR/{name}"
fi
CERBERUS_USER_ZDOTDIR=${{ZDOTDIR:-$HOME}}
"#
    ));
    match name {
        // Restored before the plugin, so zsh reads your .zlogin and .zlogout from your own folder.
        ".zshrc" => {
            text.push_str(RESTORE);
            text.push_str(&format!(
                r#"if ! (( $+functions[_zsh_autosuggest_start] )); then
  builtin source "$_cerberus_wrapper/{PLUGIN_FILE}"
fi
"#
            ));
            text.push_str(FORGET);
        }
        // Reached only when no .zshrc ran, in a shell that is not interactive.
        ".zlogin" => {
            text.push_str(RESTORE);
            text.push_str(FORGET);
        }
        // With NO_RCS set zsh reads no later file, so nothing else would restore `ZDOTDIR`.
        _ => {
            text.push_str("if [[ -o rcs ]]; then\n  ZDOTDIR=$_cerberus_wrapper\nelse\n");
            for line in RESTORE.lines().chain(FORGET.lines()) {
                text.push_str(&format!("  {line}\n"));
            }
            text.push_str("fi\n");
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::testdir::TestDir;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|&a| a.to_owned()).collect()
    }

    fn suggestions(data: &Path) -> Suggestions {
        Suggestions {
            zsh_dir: data.join("shell").join("zsh"),
            user_zdotdir: None,
            zsh_supported: true,
            zsh_written: Mutex::new(false),
        }
    }

    #[test]
    fn each_startup_file_runs_the_users_own_from_their_zdotdir_or_home() {
        for name in ZSH_STARTUP_FILES {
            let text = zsh_startup_file(name);

            assert!(
                text.contains("ZDOTDIR=${CERBERUS_USER_ZDOTDIR:-$HOME}\n"),
                "{name}"
            );
            assert!(
                text.contains(&format!("-f $ZDOTDIR/{name} ]]")),
                "{name}: {text}"
            );
            assert!(
                text.contains(&format!("builtin source \"$ZDOTDIR/{name}\"\n")),
                "{name}"
            );
            assert!(
                text.contains(r#"$ZDOTDIR != "$_cerberus_wrapper""#),
                "{name} could source itself"
            );
        }
    }

    #[test]
    fn the_early_files_point_zsh_back_at_the_wrapper_unless_no_rcs_was_set() {
        for name in [".zshenv", ".zprofile"] {
            let text = zsh_startup_file(name);

            assert!(
                text.contains("if [[ -o rcs ]]; then\n  ZDOTDIR=$_cerberus_wrapper\nelse\n"),
                "{name}: {text}"
            );
            assert!(text.contains("  builtin unset ZDOTDIR\n"), "{name}");
            assert!(!text.contains(PLUGIN_FILE), "{name} loads the plugin");
        }
    }

    #[test]
    fn zshrc_restores_zdotdir_then_loads_the_plugin_once() {
        let text = zsh_startup_file(".zshrc");
        let user = text.find("builtin source \"$ZDOTDIR/.zshrc\"").unwrap();
        let restored = text.find(RESTORE).unwrap();
        let guard = text
            .find("if ! (( $+functions[_zsh_autosuggest_start] )); then")
            .unwrap();
        let plugin = text
            .find("builtin source \"$_cerberus_wrapper/zsh-autosuggestions.zsh\"")
            .unwrap();
        let forgotten = text.find(FORGET).unwrap();

        assert!(user < restored && restored < guard && guard < plugin && plugin < forgotten);
        assert!(text.contains(r#"if [[ ${HISTFILE-} == "$_cerberus_wrapper/.zsh_history" ]]"#));
    }

    #[test]
    fn zlogin_restores_zdotdir_and_loads_nothing() {
        let text = zsh_startup_file(".zlogin");

        assert!(text.ends_with(&format!("{RESTORE}{FORGET}")), "{text}");
        assert!(!text.contains(PLUGIN_FILE));
    }

    #[test]
    fn the_bundled_plugin_is_the_pinned_release_with_its_licence() {
        assert!(ZSH_AUTOSUGGESTIONS.contains("\n# v0.7.1\n"));
        assert!(ZSH_AUTOSUGGESTIONS.contains("Permission is hereby granted, free of charge"));
        assert!(ZSH_AUTOSUGGESTIONS.contains("_zsh_autosuggest_start() {"));
        assert!(!ZSH_AUTOSUGGESTIONS.contains('\r'));
    }

    #[test]
    fn zsh_gets_the_wrapper_as_zdotdir_and_an_inherited_zdotdir_is_passed_on() {
        let wrapper = Path::new("/data/shell/zsh");

        assert_eq!(
            zsh_env(wrapper, None),
            [("ZDOTDIR", OsString::from("/data/shell/zsh"))]
        );
        assert_eq!(
            zsh_env(wrapper, Some(OsString::new())),
            [("ZDOTDIR", OsString::from("/data/shell/zsh"))]
        );
        assert_eq!(
            zsh_env(wrapper, Some(OsString::from("/u/.config/zsh"))),
            [
                ("ZDOTDIR", OsString::from("/data/shell/zsh")),
                ("CERBERUS_USER_ZDOTDIR", OsString::from("/u/.config/zsh")),
            ]
        );
    }

    #[test]
    fn powershell_runs_the_prediction_line_after_its_profile() {
        let args = pwsh_args(&strings(&["-NoLogo"])).unwrap();

        assert_eq!(args, ["-NoLogo", "-NoExit", "-Command", PWSH_PREDICTIONS]);
        assert!(PWSH_PREDICTIONS.contains("$m.Version -ge [version]'2.1'"));
        assert!(PWSH_PREDICTIONS.contains("PredictionSource -eq 'None'"));
        assert!(PWSH_PREDICTIONS.contains("Set-PSReadLineOption -PredictionSource History"));
        assert!(
            !PWSH_PREDICTIONS.contains('"'),
            "a double quote would need escaping on the command line"
        );
    }

    #[test]
    fn powershell_running_a_script_or_command_is_left_alone() {
        assert_eq!(
            pwsh_args(&strings(&["-l", "-NoProfile"])).map(|a| a.len()),
            Some(5)
        );
        assert_eq!(pwsh_args(&[]).map(|a| a.len()), Some(3));
        assert_eq!(pwsh_args(&strings(&["-NoLogo", "-c", "dir"])), None);
        assert_eq!(pwsh_args(&strings(&["-File", "x.ps1"])), None);
        assert_eq!(pwsh_args(&strings(&["script.ps1"])), None);
        assert_eq!(pwsh_args(&strings(&["-NoExit"])), None);
    }

    #[test]
    fn shells_are_told_apart_by_file_name() {
        assert_eq!(shell_of("/bin/zsh"), Some(Shell::Zsh));
        assert_eq!(shell_of("pwsh"), Some(Shell::PowerShell));
        assert_eq!(shell_of("/usr/local/bin/pwsh"), Some(Shell::PowerShell));
        assert_eq!(shell_of("PowerShell.EXE"), Some(Shell::PowerShell));
        assert_eq!(shell_of("/opt/homebrew/bin/fish"), None);
        assert_eq!(shell_of("/bin/bash"), None);
        assert_eq!(shell_of("claude"), None);
        #[cfg(windows)]
        assert_eq!(
            shell_of(r"C:\Program Files\PowerShell\7\pwsh.exe"),
            Some(Shell::PowerShell)
        );
    }

    #[test]
    fn with_the_setting_off_or_in_a_claude_tab_nothing_changes_and_nothing_is_written() {
        let data = TestDir::new();
        let s = suggestions(data.path());
        let args = strings(&["-l"]);

        assert_eq!(s.for_tab(false, false, "/bin/zsh", &args), None);
        assert_eq!(
            s.for_tab(false, false, "pwsh", &strings(&["-NoLogo"])),
            None
        );
        assert_eq!(s.for_tab(true, true, "/bin/zsh", &args), None);
        assert_eq!(s.for_tab(true, true, "pwsh", &[]), None);
        assert!(!data.path().join("shell").exists());
    }

    #[test]
    fn fish_and_bash_start_as_before() {
        let data = TestDir::new();
        let s = suggestions(data.path());

        assert_eq!(s.for_tab(true, false, "/opt/homebrew/bin/fish", &[]), None);
        assert_eq!(s.for_tab(true, false, "/bin/bash", &strings(&["-l"])), None);
    }

    #[test]
    fn a_zsh_tab_gets_the_wrapper_folder_written_and_its_args_kept() {
        let data = TestDir::new();
        let s = suggestions(data.path());
        let dir = data.path().join("shell").join("zsh");

        let setup = s
            .for_tab(true, false, "/bin/zsh", &strings(&["-l"]))
            .unwrap();

        assert_eq!(setup.args, ["-l"]);
        assert_eq!(setup.env, [("ZDOTDIR", dir.clone().into_os_string())]);
        for (name, text) in zsh_files() {
            assert_eq!(fs::read_to_string(dir.join(name)).unwrap(), text, "{name}");
        }
    }

    #[test]
    fn a_stale_wrapper_file_is_rewritten() {
        let data = TestDir::new();
        let dir = data.path().join("zsh");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".zshrc"), "old").unwrap();

        write_zsh_dir(&dir).unwrap();

        assert_eq!(
            fs::read_to_string(dir.join(".zshrc")).unwrap(),
            zsh_startup_file(".zshrc")
        );
    }

    #[test]
    fn zsh_on_windows_starts_as_before() {
        let data = TestDir::new();
        let s = Suggestions {
            zsh_supported: false,
            ..suggestions(data.path())
        };

        assert_eq!(s.for_tab(true, false, "zsh", &[]), None);
    }

    /// Runs a real login, interactive zsh and prints what its startup left behind; only macOS always ships `/bin/zsh`.
    #[cfg(target_os = "macos")]
    fn zsh_state(home: &Path, data: &Path, user_zdotdir: Option<&Path>) -> String {
        let s = Suggestions {
            user_zdotdir: user_zdotdir.map(|d| d.as_os_str().to_owned()),
            ..suggestions(data)
        };
        let setup = s
            .for_tab(true, false, "/bin/zsh", &strings(&["-l"]))
            .unwrap();
        let report = concat!(
            "print -r -- \"state env=${USER_ENV-} profile=${USER_PROFILE-} rc=${USER_RC-} ",
            "login=${USER_LOGIN-} start=${+functions[_zsh_autosuggest_start]} ",
            "bind=${+functions[_zsh_autosuggest_bind_widgets]} zdotdir=${ZDOTDIR-unset} ",
            "user=${CERBERUS_USER_ZDOTDIR-unset} wrapper=${_cerberus_wrapper-unset} ",
            "histfile=${HISTFILE-}\""
        );
        let output = std::process::Command::new("/bin/zsh")
            .args(&setup.args)
            .args(["-i", "-c", report])
            .env_clear()
            .env("HOME", home)
            .env("PATH", "/usr/bin:/bin")
            .env("TERM", "xterm-256color")
            .envs(setup.env)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("zsh runs");
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        stdout
            .lines()
            .find_map(|line| line.strip_prefix("state "))
            .unwrap_or_else(|| {
                panic!(
                    "no state line; stdout {stdout:?}, stderr {:?}",
                    String::from_utf8_lossy(&output.stderr)
                )
            })
            .to_owned()
    }

    #[cfg(target_os = "macos")]
    fn user_files(dir: &Path, rc_extra: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(".zshenv"), "USER_ENV=1\n").unwrap();
        fs::write(dir.join(".zprofile"), "USER_PROFILE=1\n").unwrap();
        fs::write(dir.join(".zshrc"), format!("USER_RC=1\n{rc_extra}")).unwrap();
        fs::write(dir.join(".zlogin"), "USER_LOGIN=1\n").unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_real_zsh_runs_your_startup_files_then_has_the_plugin() {
        let root = TestDir::new();
        let home = root.path().join("home");
        // macOS keeps app data under "Application Support", so the wrapper path has a space.
        let data = root.path().join("Application Support");
        user_files(&home, "");

        let state = zsh_state(&home, &data, None);

        assert!(
            state.starts_with("env=1 profile=1 rc=1 login=1 start=1 bind=1 zdotdir=unset user=unset wrapper=unset "),
            "{state}"
        );
        // Empty where no /etc/zshrc names a history file, as on many Linux systems.
        let history = format!("histfile={}", home.join(".zsh_history").display());
        assert!(
            state.ends_with(&history) || state.ends_with("histfile="),
            "{state}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_real_zsh_with_the_plugin_already_loaded_does_not_load_it_again() {
        let root = TestDir::new();
        let home = root.path().join("home");
        user_files(&home, "_zsh_autosuggest_start() { :; }\n");

        let state = zsh_state(&home, &root.path().join("data"), None);

        assert!(state.contains(" rc=1 login=1 start=1 bind=0 "), "{state}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_real_zsh_whose_zshrc_unsets_zdotdir_still_runs_your_zlogin() {
        let root = TestDir::new();
        let home = root.path().join("home");
        user_files(&home, "unset ZDOTDIR\n");

        let state = zsh_state(&home, &root.path().join("data"), None);

        assert!(
            state.starts_with("env=1 profile=1 rc=1 login=1 start=1 bind=1 zdotdir=unset "),
            "{state}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_real_zsh_reads_your_files_from_your_own_zdotdir() {
        let root = TestDir::new();
        let home = root.path().join("home");
        let given = root.path().join("my zsh");
        user_files(&given, "");
        let state = zsh_state(&home, &root.path().join("data"), Some(&given));
        assert!(
            state.starts_with(&format!(
                "env=1 profile=1 rc=1 login=1 start=1 bind=1 zdotdir={} user=unset ",
                given.display()
            )),
            "{state}"
        );

        let moved = home.join(".config").join("zsh");
        fs::create_dir_all(&home).unwrap();
        fs::write(
            home.join(".zshenv"),
            "ZDOTDIR=$HOME/.config/zsh\nUSER_ENV=1\n",
        )
        .unwrap();
        fs::create_dir_all(&moved).unwrap();
        fs::write(moved.join(".zshrc"), "USER_RC=1\n").unwrap();
        let state = zsh_state(&home, &root.path().join("data2"), None);
        assert!(
            state.starts_with(&format!(
                "env=1 profile= rc=1 login= start=1 bind=1 zdotdir={} ",
                moved.display()
            )),
            "{state}"
        );
    }
}
