//! `claude` typed in a shell tab: the real `claude` with the tab's Cerberus flags, or another account's chat in its own tab.

use std::collections::HashMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::{Mutex, PoisonError};

use crate::open::pipe;
use crate::programs::Finder;
use crate::saved::TabId;
use crate::sessions::is_session_id;

/// The wrapper's folder, for PowerShell and zsh to put first again after the user's own startup files.
pub const BIN_VAR: &str = "CERBERUS_BIN";

/// The flags the tab's own `claude` gets, as a JSON list, set on the shell by the app.
pub const ARGS_VAR: &str = "CERBERUS_CLAUDE_ARGS";
/// The shell tab's id, so the app knows which tab a wrapped `claude` runs in.
pub const TAB_VAR: &str = "CERBERUS_TAB";
/// The app's request pipe; a test build's differs from the installed app's.
pub const PIPE_VAR: &str = "CERBERUS_PIPE";
/// Tells the app the tab's `claude` came through the wrapper: `[STARTED, tab, pid]`.
pub const STARTED: &str = "wrapper-started";
/// Asks the app to open a chat this account doesn't have: `[RESUME, tab, session id]`.
pub const RESUME: &str = "wrapper-resume";

const NAME: &str = "claude";
const BIN: &str = "bin";

/// True when this program runs under the wrapper's name.
pub fn called_as_wrapper() -> bool {
    env::args_os()
        .next()
        .is_some_and(|first| is_wrapper_name(&first))
}

fn is_wrapper_name(program: &OsStr) -> bool {
    Path::new(program)
        .file_stem()
        .is_some_and(|stem| stem.eq_ignore_ascii_case(NAME))
}

/// What the app set on the shell; absent outside a Cerberus shell tab, where the wrapper is plain `claude`.
struct Link {
    tab: String,
    pipe: String,
    flags: Vec<String>,
}

impl Link {
    fn from_env() -> Option<Self> {
        let flags = serde_json::from_str(&env::var(ARGS_VAR).ok()?).ok()?;
        Some(Self {
            tab: env::var(TAB_VAR).ok()?,
            pipe: env::var(PIPE_VAR).ok()?,
            flags,
        })
    }

    fn ask(&self, request: &str, value: &str) -> Option<(i32, String)> {
        let args = [request.to_owned(), self.tab.clone(), value.to_owned()];
        pipe::ask(&self.pipe, &args).ok().flatten()
    }
}

/// The chat id a `--resume <id>`, `-r <id>` or `--resume=<id>` asks for; the bare flag opens Claude Code's picker.
fn resume_id(args: &[OsString]) -> Option<String> {
    let args: Vec<&str> = args.iter().map(|a| a.to_str()).collect::<Option<_>>()?;
    let id = args.iter().enumerate().find_map(|(at, arg)| {
        if let Some(id) = arg.strip_prefix("--resume=") {
            return Some(id);
        }
        (*arg == "--resume" || *arg == "-r")
            .then(|| args.get(at + 1).copied())
            .flatten()
    })?;
    is_session_id(id).then(|| id.to_owned())
}

/// Whether the account folder holds the chat, in any project folder.
fn holds(config_dir: &Path, id: &str) -> bool {
    let file = format!("{id}.jsonl");
    fs::read_dir(config_dir.join("projects")).is_ok_and(|projects| {
        projects
            .filter_map(Result::ok)
            .any(|project| project.path().join(&file).is_file())
    })
}

/// The account folder this shell's `claude` uses: Claude Code's own default when the variable is unset.
fn config_dir() -> Option<PathBuf> {
    env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".claude")))
}

/// Runs as `claude`: never sends input, only adds the tab's flags or opens a chat in another account's tab.
pub fn run() -> ExitCode {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let own = env::current_exe().ok();
    // The app's folder, and this program's, which a link resolves to `cerberus-cli`'s on Linux.
    let skip = env::var_os(BIN_VAR)
        .map(PathBuf::from)
        .into_iter()
        .chain(
            own.as_ref()
                .and_then(|exe| exe.parent().map(Path::to_path_buf)),
        )
        .collect();
    let find = |skip: &[PathBuf]| {
        skip.iter()
            .fold(Finder::from_env(), |finder, dir| finder.without(dir))
            .find(NAME)
    };
    let is_self = |found: &Path| own.as_deref().is_some_and(|own| same_file(found, own));
    let Some(real) = real_claude(find, is_self, skip) else {
        eprintln!("claude: Claude Code is not installed, or not on PATH");
        return ExitCode::from(127);
    };
    let link = Link::from_env();
    if let Some(link) = &link
        && let Some(id) = resume_id(&args)
        && !config_dir().is_some_and(|dir| holds(&dir, &id))
        && let Some((0, opened)) = link.ask(RESUME, &id)
    {
        println!("{opened}");
        return ExitCode::SUCCESS;
    }
    let mut command = Command::new(&real);
    if let Some(link) = &link {
        command.args(&link.flags);
        // A `claude` that this one starts is not the tab's chat, so it runs plain.
        command.env_remove(ARGS_VAR).env_remove(TAB_VAR);
        let _ = link.ask(STARTED, &std::process::id().to_string());
    }
    command.args(&args);
    start(command)
}

/// The first `claude` on PATH that is not this program, under whatever name or link it was found.
fn real_claude(
    find: impl Fn(&[PathBuf]) -> Option<PathBuf>,
    is_self: impl Fn(&Path) -> bool,
    mut skip: Vec<PathBuf>,
) -> Option<PathBuf> {
    // A few rounds cover every copy or link of the wrapper a PATH could hold.
    for _ in 0..8 {
        let found = find(&skip)?;
        if !is_self(&found) {
            return Some(found);
        }
        skip.push(found.parent()?.to_path_buf());
    }
    None
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(unix)]
fn start(mut command: Command) -> ExitCode {
    use std::os::unix::process::CommandExt;
    // `exec` keeps this pid, so the app's note about it names the chat itself.
    let error = command.exec();
    eprintln!("claude: could not start Claude Code: {error}");
    ExitCode::from(126)
}

#[cfg(windows)]
fn start(mut command: Command) -> ExitCode {
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
    // SAFETY: a null handler with TRUE only makes this process ignore Ctrl+C, which belongs to Claude Code on the shared console.
    unsafe { SetConsoleCtrlHandler(None, 1) };
    match command.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!("claude: could not start Claude Code: {error}");
            ExitCode::from(126)
        }
    }
}

/// The wrapper's folder, or why it could not be put in place, in which case shell tabs start as before.
pub struct Bin(pub Result<PathBuf, String>);

impl Bin {
    pub fn dir(&self) -> Option<&Path> {
        self.0.as_deref().ok()
    }
}

/// Why `claude` typed in a shell tab runs without the tab's links, for Accounts to say; `None` when the wrapper is in place.
#[tauri::command]
pub fn wrapper_failure(bin: tauri::State<'_, Bin>) -> Option<String> {
    bin.0.as_ref().err().cloned()
}

/// Shell tabs whose `claude` came through the wrapper, by the wrapper's pid while it runs.
#[derive(Default)]
pub struct Wrapped(Mutex<HashMap<TabId, u32>>);

impl Wrapped {
    pub fn started(&self, tab: TabId, pid: u32) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(tab, pid);
    }

    /// Whether the tab's `claude` came through the wrapper, which still runs while its `claude` does.
    pub fn runs(&self, tab: TabId, is_running: fn(u32) -> bool) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&tab)
            .is_some_and(|&pid| is_running(pid))
    }
}

/// What a shell tab gets so a `claude` typed in it is the tab's own chat: the wrapper first on PATH, and the tab's flags.
pub fn shell_env(
    bin: &Path,
    tab: TabId,
    pipe: String,
    flags: &[String],
    path: Option<OsString>,
) -> Vec<(&'static str, OsString)> {
    let mut dirs = vec![bin.to_path_buf()];
    dirs.extend(path.iter().flat_map(env::split_paths));
    // A PATH entry holding the separator can't be joined; the wrapper then simply isn't first.
    let path = env::join_paths(dirs).unwrap_or_else(|_| path.unwrap_or_default());
    vec![
        ("PATH", path),
        (BIN_VAR, bin.into()),
        (TAB_VAR, tab.to_string().into()),
        (PIPE_VAR, pipe.into()),
        (
            ARGS_VAR,
            serde_json::to_string(flags).unwrap_or_default().into(),
        ),
    ]
}

/// Program names of the shells whose own profile can reorder PATH after the app sets it.
fn is_powershell(program: &str) -> bool {
    Path::new(program)
        .file_stem()
        .and_then(OsStr::to_str)
        .is_some_and(|stem| {
            stem.eq_ignore_ascii_case("powershell") || stem.eq_ignore_ascii_case("pwsh")
        })
}

// Run after the profile, which may put another `claude` first; it reads the folder from the variable, so no path is quoted.
const PWSH_WRAPPER_FIRST: &str = "if ($env:CERBERUS_BIN) { $env:PATH = $env:CERBERUS_BIN + [IO.Path]::PathSeparator + $env:PATH }";

/// PowerShell's arguments with the wrapper put first again after its profile; a tab that runs a script is left alone.
pub fn powershell_args(program: &str, args: Vec<String>) -> Vec<String> {
    if !is_powershell(program) {
        return args;
    }
    let lower: Vec<String> = args.iter().map(|a| a.to_ascii_lowercase()).collect();
    if let Some(at) = lower.iter().position(|a| a == "-command") {
        // Only the suggestions setup's own `-NoExit -Command`, which this joins.
        if !lower.iter().any(|a| a == "-noexit") || at + 2 != args.len() {
            return args;
        }
        let mut args = args;
        let last = format!("{PWSH_WRAPPER_FIRST}; {}", args[at + 1]);
        args[at + 1] = last;
        return args;
    }
    if lower
        .iter()
        .any(|a| a == "-file" || a == "-encodedcommand" || !a.starts_with('-'))
    {
        return args;
    }
    let mut args = args;
    args.extend(["-NoExit", "-Command", PWSH_WRAPPER_FIRST].map(str::to_owned));
    args
}

/// The console command's names beside the app, first found wins: the Windows installer renames it to `cerberus.com`.
#[cfg(windows)]
const CLI_NAMES: [&str; 2] = ["cerberus-cli.exe", "cerberus.com"];
#[cfg(unix)]
const CLI_NAMES: [&str; 1] = ["cerberus-cli"];

/// Puts the wrapper in `<data dir>/bin` beside nothing else, and returns that folder for shell tabs' PATH.
pub fn install(data_dir: &Path) -> io::Result<PathBuf> {
    let exe = env::current_exe()?;
    let app_dir = exe
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "the app has no folder"))?;
    install_beside(app_dir, data_dir)
}

fn install_beside(app_dir: &Path, data_dir: &Path) -> io::Result<PathBuf> {
    let cli = CLI_NAMES
        .iter()
        .map(|name| app_dir.join(name))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} is not in {}", CLI_NAMES.join(" or "), app_dir.display()),
            )
        })?;
    let bin = data_dir.join(BIN);
    fs::create_dir_all(&bin)?;
    place(
        &cli,
        &bin.join(format!("{NAME}{}", env::consts::EXE_SUFFIX)),
    )?;
    Ok(bin)
}

// A copy, since Windows links need rights a user may not have; skipped when it already matches.
#[cfg(windows)]
fn place(cli: &Path, wrapper: &Path) -> io::Result<()> {
    let same = |a: &Path, b: &Path| -> Option<bool> {
        let (a, b) = (a.metadata().ok()?, b.metadata().ok()?);
        Some(a.len() == b.len() && a.modified().ok()? <= b.modified().ok()?)
    };
    if same(cli, wrapper) == Some(true) {
        return Ok(());
    }
    fs::copy(cli, wrapper).map(|_| ())
}

#[cfg(unix)]
fn place(cli: &Path, wrapper: &Path) -> io::Result<()> {
    if fs::read_link(wrapper).is_ok_and(|to| to == cli) {
        return Ok(());
    }
    match fs::remove_file(wrapper) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    std::os::unix::fs::symlink(cli, wrapper)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0b5f4a2e-8c1d-4e7a-9f3b-2a6c8d0e1f42";

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_wrapper_answers_to_claude_with_or_without_an_extension_or_folder() {
        assert!(is_wrapper_name(OsStr::new("claude")));
        #[cfg(windows)]
        assert!(is_wrapper_name(OsStr::new(r"C:\data\bin\Claude.exe")));
        #[cfg(unix)]
        assert!(is_wrapper_name(OsStr::new("/data/bin/claude")));
        assert!(!is_wrapper_name(OsStr::new("cerberus-cli")));
    }

    #[test]
    fn the_real_claude_is_the_first_that_is_not_this_program() {
        let path = ["/app/bin", "/cli", "/real"].map(PathBuf::from);
        let find = |skip: &[PathBuf]| {
            path.iter()
                .find(|dir| !skip.contains(dir))
                .map(|dir| dir.join("claude"))
        };
        // A link in a folder not skipped up front still resolves to this program, so it is skipped then.
        let is_self = |found: &Path| !found.starts_with("/real");

        let real = real_claude(find, is_self, Vec::new());
        let only_self = real_claude(
            |_: &[PathBuf]| Some(PathBuf::from("/app/bin/claude")),
            |_: &Path| true,
            Vec::new(),
        );

        assert_eq!(real, Some(PathBuf::from("/real/claude")));
        assert_eq!(
            only_self, None,
            "a PATH holding only the wrapper never loops"
        );
    }

    #[test]
    fn resume_reads_the_id_in_each_form_and_leaves_the_picker_alone() {
        assert_eq!(resume_id(&args(&["--resume", ID])).as_deref(), Some(ID));
        assert_eq!(
            resume_id(&args(&["-r", ID, "--model", "x"])).as_deref(),
            Some(ID)
        );
        assert_eq!(
            resume_id(&args(&[&format!("--resume={ID}")])).as_deref(),
            Some(ID)
        );
        assert_eq!(resume_id(&args(&["--resume"])), None);
        assert_eq!(resume_id(&args(&["--resume", "--model"])), None);
        assert_eq!(resume_id(&args(&["hello", ID])), None);
    }

    #[test]
    fn an_account_holds_a_chat_only_when_its_file_is_in_a_project_folder() {
        let dir = std::env::temp_dir().join(format!("cerberus-wrapper-{}", std::process::id()));
        let project = dir.join("projects").join("C--code-alpha");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join(format!("{ID}.jsonl")), "{}\n").unwrap();

        let held = holds(&dir, ID);
        let other = holds(&dir, "1b5f4a2e-8c1d-4e7a-9f3b-2a6c8d0e1f42");
        let _ = fs::remove_dir_all(&dir);

        assert!(held);
        assert!(!other);
    }

    /// An install folder holding the app and the console command under `cli`, and an empty app data folder.
    fn installed(cli: Option<&str>) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "cerberus-install-{}-{}",
            std::process::id(),
            cli.unwrap_or("none")
        ));
        let _ = fs::remove_dir_all(&root);
        let app = root.join("Cerberus");
        let data = root.join("data");
        fs::create_dir_all(&app).unwrap();
        fs::write(
            app.join(format!("cerberus{}", env::consts::EXE_SUFFIX)),
            "app",
        )
        .unwrap();
        if let Some(cli) = cli {
            fs::write(app.join(cli), "the console command").unwrap();
        }
        (root, app, data)
    }

    #[cfg(windows)]
    #[test]
    fn the_installer_s_cerberus_com_becomes_the_wrapper() {
        let (root, app, data) = installed(Some("cerberus.com"));

        let bin = install_beside(&app, &data).unwrap();
        let copied = fs::read_to_string(bin.join("claude.exe"));
        let _ = fs::remove_dir_all(&root);

        assert_eq!(copied.unwrap(), "the console command");
    }

    #[test]
    fn a_build_s_cerberus_cli_becomes_the_wrapper() {
        let cli = format!("cerberus-cli{}", env::consts::EXE_SUFFIX);
        let (root, app, data) = installed(Some(&cli));

        let bin = install_beside(&app, &data).unwrap();
        let wrapper = bin.join(format!("claude{}", env::consts::EXE_SUFFIX));
        let placed = wrapper.exists();
        let _ = fs::remove_dir_all(&root);

        assert!(placed);
    }

    #[test]
    fn no_console_command_says_which_names_it_looked_for() {
        let (root, app, data) = installed(None);

        let error = install_beside(&app, &data).unwrap_err().to_string();
        let _ = fs::remove_dir_all(&root);

        assert!(error.contains("cerberus-cli"), "{error}");
        #[cfg(windows)]
        assert!(error.contains("cerberus.com"), "{error}");
    }

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_shell_gets_the_wrapper_first_on_path_and_the_tabs_flags() {
        let bin = std::env::temp_dir().join("cerberus-bin");
        let rest = std::env::temp_dir().join("elsewhere");
        let path = env::join_paths([&rest]).unwrap();
        let flags = strings(&["--mcp-config=/d/mcp.json", "--settings=/d/tab-3.json"]);

        let env = shell_env(&bin, 3, "pipe".to_owned(), &flags, Some(path));

        let get = |name: &str| env.iter().find(|(n, _)| *n == name).map(|(_, v)| v.clone());
        let dirs: Vec<PathBuf> = env::split_paths(&get("PATH").unwrap()).collect();
        assert_eq!(dirs, [bin.clone(), rest]);
        assert_eq!(get(TAB_VAR), Some("3".into()));
        assert_eq!(get(PIPE_VAR), Some("pipe".into()));
        let read: Vec<String> =
            serde_json::from_str(get(ARGS_VAR).unwrap().to_str().unwrap()).unwrap();
        assert_eq!(read, flags);
    }

    #[test]
    fn powershell_puts_the_wrapper_first_after_its_profile_and_leaves_scripts_alone() {
        let plain = powershell_args("powershell.exe", strings(&["-NoLogo"]));
        assert_eq!(
            plain,
            strings(&["-NoLogo", "-NoExit", "-Command", PWSH_WRAPPER_FIRST])
        );

        let suggested = powershell_args("pwsh", strings(&["-NoExit", "-Command", "Set-X"]));
        assert_eq!(suggested[2], format!("{PWSH_WRAPPER_FIRST}; Set-X"));

        let script = strings(&["-File", "build.ps1"]);
        assert_eq!(powershell_args("powershell", script.clone()), script);
        assert_eq!(powershell_args("bash", strings(&["-l"])), strings(&["-l"]));
    }
}
