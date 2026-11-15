use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

// Only this much of a task file is read, to find its first heading.
const HEAD_BYTES: u64 = 64 * 1024;
// A Markdown file this large is not a task file, and the dialog lists names only.
const MAX_BYTES: u64 = 1024 * 1024;

/// The prompt a project starts with until it is edited; the coder reads the task file itself.
pub const DEFAULT_PROMPT: &str = "Read AGENTS.md, then {task_file}, and do that task. Work in {folder} on branch {branch}. When you finish, write the handoff to {handoff_file} and tell {planner_tab}.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskFile {
    pub path: String,
    pub id: String,
    pub heading: Option<String>,
    pub modified_ms: u64,
}

/// The `*.md` files in `folder`, newest first, each with its first heading; one not UTF-8 or too large is left out.
pub fn list(folder: &Path) -> Vec<TaskFile> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<TaskFile> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let meta = entry.metadata().ok()?;
            let markdown = path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
            if !meta.is_file() || !markdown || meta.len() > MAX_BYTES {
                return None;
            }
            let heading = first_heading(&path)?;
            let modified_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .and_then(|d| u64::try_from(d.as_millis()).ok())
                .unwrap_or(0);
            let id = task_id(&path);
            Some(TaskFile {
                heading: heading.as_deref().and_then(|h| short_heading(&id, h)),
                id,
                path: path.to_string_lossy().into_owned(),
                modified_ms,
            })
        })
        .collect();
    files.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms).then(a.path.cmp(&b.path)));
    files
}

/// `None` for a file that is not UTF-8, `Some(None)` for one with no heading in its first bytes.
fn first_heading(file: &Path) -> Option<Option<String>> {
    let mut start = Vec::new();
    fs::File::open(file)
        .ok()?
        .take(HEAD_BYTES)
        .read_to_end(&mut start)
        .ok()?;
    // A cut can split a character, so only a real encoding error counts.
    let text = match std::str::from_utf8(&start) {
        Ok(text) => text,
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&start[..e.valid_up_to()]).ok()?,
        Err(_) => return None,
    };
    Some(
        text.lines()
            .find_map(|line| line.trim_start().strip_prefix('#'))
            .map(|rest| rest.trim_start_matches('#').trim().to_owned())
            .filter(|h| !h.is_empty()),
    )
}

/// `task-025.md` is `TASK-025`; any other name is its file stem as written.
pub fn task_id(file: &Path) -> String {
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let numbered = stem.split_once('-').is_some_and(|(word, rest)| {
        !word.is_empty()
            && word.chars().all(|c| c.is_ascii_alphabetic())
            && rest.starts_with(|c: char| c.is_ascii_digit())
    });
    if numbered { stem.to_uppercase() } else { stem }
}

/// Each `{name}` in `template` becomes its value; an unknown `{name}` stays as written.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    values
        .iter()
        .fold(template.to_owned(), |text, (name, value)| {
            text.replace(&format!("{{{name}}}"), value)
        })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRequest {
    pub task_file: PathBuf,
    pub project_folder: PathBuf,
    /// The worktree folder and branch, when the task gets one.
    pub worktree: Option<(PathBuf, String)>,
    /// The project's edited prompt; `None` uses the default.
    pub template: Option<String>,
    pub planner_tab: String,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub title: String,
    pub prompt: String,
    pub cwd: String,
    /// The project folder, added to a chat that runs in a worktree.
    pub dirs: Vec<String>,
}

/// What a coder tab for the task opens with; `branch` is the project folder's current one when there is no worktree.
pub fn plan(request: &PlanRequest, heading: Option<&str>, branch: Option<&str>) -> Plan {
    let id = task_id(&request.task_file);
    let shown = |p: &Path| p.to_string_lossy().into_owned();
    let (folder, branch, dirs) = match &request.worktree {
        Some((folder, branch)) => (
            shown(folder),
            branch.clone(),
            vec![shown(&request.project_folder)],
        ),
        None => (
            shown(&request.project_folder),
            branch.unwrap_or_default().to_owned(),
            Vec::new(),
        ),
    };
    let handoff = handoff_file(&request.task_file, &id);
    let task_file = shown(&request.task_file);
    let template = request.template.as_deref().unwrap_or(DEFAULT_PROMPT);
    let prompt = fill(
        template,
        &[
            ("task_file", &task_file),
            ("task_id", &id),
            ("folder", &folder),
            ("branch", &branch),
            ("handoff_file", &shown(&handoff)),
            ("planner_tab", &request.planner_tab),
        ],
    );
    let title = match heading.and_then(|h| short_heading(&id, h)) {
        Some(h) => format!("{id} · {h}"),
        None => id,
    };
    Plan {
        title,
        prompt,
        cwd: folder,
        dirs,
    }
}

/// A heading without the task id it starts with, since the id is shown beside it; `None` once nothing is left.
fn short_heading(id: &str, heading: &str) -> Option<String> {
    let rest = heading
        .get(..id.len())
        .filter(|start| start.eq_ignore_ascii_case(id))
        .map_or(heading, |_| &heading[id.len()..]);
    let rest = rest.trim_start_matches([':', ' ', '-', '·']).trim();
    (!rest.is_empty()).then(|| rest.to_owned())
}

/// `completed/<task id>.md` beside the task folder, lower case like the task file names.
fn handoff_file(task_file: &Path, id: &str) -> PathBuf {
    let folder = task_file.parent().unwrap_or(Path::new(""));
    folder
        .parent()
        .unwrap_or(folder)
        .join("completed")
        .join(format!("{}.md", id.to_lowercase()))
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeDefaults {
    pub folder: String,
    pub branch: String,
}

/// `<repo>-<task id>` beside the project's folder, on branch `feat/<task id>`; both stay editable.
pub fn worktree_defaults(project_folder: &Path, task_file: &Path) -> WorktreeDefaults {
    let id = task_id(task_file).to_lowercase();
    let name = project_folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let beside = project_folder.parent().unwrap_or(project_folder);
    WorktreeDefaults {
        folder: beside
            .join(format!("{name}-{id}"))
            .to_string_lossy()
            .into_owned(),
        branch: format!("feat/{id}"),
    }
}

/// The first of `folders` with a `tasks` folder in it.
pub fn guess_folder(folders: &[PathBuf]) -> Option<PathBuf> {
    folders.iter().map(|f| f.join("tasks")).find(|t| t.is_dir())
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Branches {
    /// The repo's default branch, which a new worktree starts from unless another is picked.
    pub default: Option<String>,
    /// Local branches, with the default first when it is only a remote one.
    pub names: Vec<String>,
}

pub fn branches(git: &crate::panel::git::Git, repo: &Path) -> Branches {
    let default = git.base(repo);
    let mut names: Vec<String> = git.branches(repo).into_iter().map(|b| b.name).collect();
    if let Some(base) = default.as_ref().filter(|b| !names.contains(b)) {
        names.insert(0, base.clone());
    }
    Branches { default, names }
}

/// `git worktree add -b <branch> -- <folder> <base>` in `repo`; a refusal comes back as git's own message.
pub fn add_worktree(
    git: &Path,
    repo: &Path,
    folder: &Path,
    branch: &str,
    base: &str,
) -> Result<(), String> {
    // Typed in the dialog, so neither may read as an option to git.
    for (what, value) in [("branch", branch), ("base branch", base)] {
        if value.trim().is_empty() || value.starts_with('-') {
            return Err(format!("the {what} name is not valid"));
        }
    }
    if !folder.is_absolute() {
        return Err("the worktree folder must be a full path".to_owned());
    }
    if folder.exists() {
        return Err(format!("{} already exists", folder.display()));
    }
    let mut command = std::process::Command::new(git);
    command
        .args(["worktree", "add", "-b", branch, "--"])
        .arg(folder)
        .arg(base)
        .current_dir(repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null());
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(
        &mut command,
        crate::run::CREATE_NO_WINDOW,
    );
    let out = command
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    Err(if said.is_empty() {
        "git worktree add failed".to_owned()
    } else {
        said
    })
}

#[tauri::command]
pub async fn task_branches(folder: PathBuf) -> Branches {
    crate::blocking::blocking(
        move || match crate::programs::Finder::from_env().find("git") {
            Some(git) => branches(&crate::panel::git::Git::new(git), &folder),
            None => Branches {
                default: None,
                names: Vec::new(),
            },
        },
    )
    .await
}

#[tauri::command]
pub async fn task_worktree(
    repo: PathBuf,
    folder: PathBuf,
    branch: String,
    base: String,
) -> Result<(), String> {
    crate::blocking::blocking(move || {
        let git = crate::programs::Finder::from_env()
            .find("git")
            .ok_or("git is not installed")?;
        add_worktree(&git, &repo, &folder, &branch, &base)
    })
    .await
}

#[tauri::command]
pub async fn task_files(folder: PathBuf) -> Vec<TaskFile> {
    crate::blocking::blocking(move || list(&folder)).await
}

#[tauri::command]
pub async fn task_folder(folders: Vec<PathBuf>) -> Option<String> {
    crate::blocking::blocking(move || {
        guess_folder(&folders).map(|f| f.to_string_lossy().into_owned())
    })
    .await
}

#[tauri::command]
pub fn task_default_prompt() -> &'static str {
    DEFAULT_PROMPT
}

#[tauri::command]
pub fn task_worktree_defaults(project_folder: PathBuf, task_file: PathBuf) -> WorktreeDefaults {
    worktree_defaults(&project_folder, &task_file)
}

#[tauri::command]
pub async fn task_plan(request: PlanRequest) -> Plan {
    crate::blocking::blocking(move || {
        let heading = first_heading(&request.task_file).flatten();
        // Only a chat in the project folder itself reports the branch that folder is on.
        let branch = request
            .worktree
            .is_none()
            .then(|| {
                let git = crate::programs::Finder::from_env().find("git")?;
                crate::panel::git::Git::new(git)
                    .head(&request.project_folder)?
                    .branch
            })
            .flatten();
        plan(&request, heading.as_deref(), branch.as_deref())
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;
    use crate::accounts::testdir::TestDir;

    fn write(dir: &Path, name: &str, body: &[u8], age_s: u64) {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        let at = SystemTime::now() - Duration::from_secs(age_s);
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(at)
            .unwrap();
    }

    #[test]
    fn task_files_are_listed_newest_first_with_their_first_heading() {
        let dir = TestDir::new();
        write(
            dir.path(),
            "task-024.md",
            b"# TASK-024: File pane\n\nbody",
            300,
        );
        write(
            dir.path(),
            "task-025.md",
            b"intro\n\n## TASK-025: Start a task\n",
            10,
        );
        write(dir.path(), "notes.md", b"no heading here\n", 100);
        write(dir.path(), "readme.txt", b"# not markdown\n", 1);
        fs::create_dir(dir.path().join("archive.md")).unwrap();

        let found = list(dir.path());

        let ids: Vec<_> = found.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["TASK-025", "notes", "TASK-024"]);
        assert_eq!(found[0].heading.as_deref(), Some("Start a task"));
        assert_eq!(found[1].heading, None);
        assert_eq!(found[2].heading.as_deref(), Some("File pane"));
        assert!(found[0].path.ends_with("task-025.md"));
    }

    #[test]
    fn a_file_that_is_not_utf8_or_is_huge_is_left_out() {
        let dir = TestDir::new();
        write(dir.path(), "task-001.md", b"# ok\n", 5);
        write(dir.path(), "task-002.md", b"# bad \xff\xfe bytes\n", 5);
        write(dir.path(), "task-003.md", &vec![b'a'; 2 * 1024 * 1024], 5);
        let mut cut = b"# cut at a char\n".to_vec();
        cut.resize(usize::try_from(HEAD_BYTES).unwrap() - 1, b'x');
        cut.extend("é rest".as_bytes());
        write(dir.path(), "task-004.md", &cut, 50);

        let ids: Vec<_> = list(dir.path()).into_iter().map(|f| f.id).collect();

        assert_eq!(ids, ["TASK-001", "TASK-004"]);
    }

    #[test]
    fn a_missing_folder_lists_nothing() {
        assert!(list(Path::new("/no/such/folder/here")).is_empty());
    }

    #[test]
    fn task_ids_come_from_numbered_file_names() {
        assert_eq!(task_id(Path::new("tasks/task-025.md")), "TASK-025");
        assert_eq!(task_id(Path::new("task-007b.md")), "TASK-007B");
        assert_eq!(task_id(Path::new("bug-12-crash.md")), "BUG-12-CRASH");
        assert_eq!(task_id(Path::new("Release notes.md")), "Release notes");
        assert_eq!(task_id(Path::new("v2-plan.md")), "v2-plan");
    }

    #[test]
    fn a_heading_drops_the_task_id_it_starts_with() {
        assert_eq!(
            short_heading("TASK-025", "TASK-025: Start a task").as_deref(),
            Some("Start a task")
        );
        assert_eq!(
            short_heading("TASK-025", "task-025 - Start").as_deref(),
            Some("Start")
        );
        assert_eq!(
            short_heading("TASK-025", "Start a task").as_deref(),
            Some("Start a task")
        );
        assert_eq!(short_heading("TASK-025", "TASK-025"), None);
        assert_eq!(
            short_heading("notes", "Notés on ünicode").as_deref(),
            Some("Notés on ünicode")
        );
    }

    #[test]
    fn placeholders_are_filled_and_unknown_ones_kept() {
        let text = fill(
            "{task_id} in {folder}, {folder} again, {unknown}",
            &[("task_id", "TASK-1"), ("folder", "/w")],
        );

        assert_eq!(text, "TASK-1 in /w, /w again, {unknown}");
    }

    fn request(worktree: Option<(PathBuf, String)>) -> PlanRequest {
        PlanRequest {
            task_file: PathBuf::from("/p/plans/tasks/task-025.md"),
            project_folder: PathBuf::from("/p/code"),
            worktree,
            template: None,
            planner_tab: "planner".to_owned(),
        }
    }

    #[test]
    fn a_worktree_plan_runs_there_adds_the_project_folder_and_names_the_branch() {
        let plan = plan(
            &request(Some((
                PathBuf::from("/p/code-task-025"),
                "feat/task-025".to_owned(),
            ))),
            Some("TASK-025: Start a task"),
            Some("main"),
        );

        assert_eq!(plan.title, "TASK-025 · Start a task");
        assert_eq!(plan.cwd, "/p/code-task-025");
        assert_eq!(plan.dirs, ["/p/code"]);
        let handoff = Path::new("/p/plans").join("completed").join("task-025.md");
        assert_eq!(
            plan.prompt,
            format!(
                "Read AGENTS.md, then /p/plans/tasks/task-025.md, and do that task. Work in /p/code-task-025 on branch feat/task-025. When you finish, write the handoff to {} and tell planner.",
                handoff.display()
            )
        );
    }

    #[test]
    fn without_a_worktree_the_plan_uses_the_project_folder_and_its_branch() {
        let mut edited = request(None);
        edited.template = Some("{task_id} on {branch} in {folder}".to_owned());

        let plan = plan(&edited, None, Some("main"));

        assert_eq!(plan.title, "TASK-025");
        assert_eq!(plan.cwd, "/p/code");
        assert!(plan.dirs.is_empty());
        assert_eq!(plan.prompt, "TASK-025 on main in /p/code");
    }

    #[test]
    fn a_worktree_goes_beside_the_project_named_for_the_task() {
        let found = worktree_defaults(
            Path::new("/p/code"),
            Path::new("/p/plans/tasks/task-025.md"),
        );

        assert_eq!(
            PathBuf::from(&found.folder),
            Path::new("/p").join("code-task-025")
        );
        assert_eq!(found.branch, "feat/task-025");
    }

    // Identity and signing come from the command line, so the machine's own git config cannot fail a commit.
    fn git(folder: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .current_dir(folder)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn repo() -> TestDir {
        let dir = TestDir::new();
        git(dir.path(), &["init", "-q"]);
        fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-qm", "first"]);
        dir
    }

    fn git_program() -> PathBuf {
        crate::programs::Finder::from_env()
            .find("git")
            .expect("git is installed")
    }

    #[test]
    fn branches_list_the_local_ones_with_the_default_named() {
        let repo = repo();
        git(repo.path(), &["branch", "dev"]);
        git(repo.path(), &["branch", "feat/x"]);

        let found = branches(&crate::panel::git::Git::new(git_program()), repo.path());

        assert_eq!(found.default.as_deref(), Some("main"));
        assert_eq!(found.names, ["dev", "feat/x", "main"]);
    }

    #[test]
    fn a_remote_only_default_comes_first() {
        let origin = repo();
        let clone = TestDir::new();
        git(
            clone.path(),
            &["clone", "-q", &origin.path().to_string_lossy(), "."],
        );
        git(clone.path(), &["checkout", "-qb", "work"]);
        git(clone.path(), &["branch", "-D", "main"]);

        let found = branches(&crate::panel::git::Git::new(git_program()), clone.path());

        assert_eq!(found.default.as_deref(), Some("origin/main"));
        assert_eq!(found.names, ["origin/main", "work"]);
    }

    #[test]
    fn a_worktree_is_added_on_a_new_branch_from_the_base() {
        let repo = repo();
        let beside = TestDir::new();
        let folder = beside.path().join("code-task-025");

        add_worktree(
            &git_program(),
            repo.path(),
            &folder,
            "feat/task-025",
            "main",
        )
        .unwrap();

        assert!(folder.join("a.txt").is_file());
        assert_eq!(git(&folder, &["branch", "--show-current"]), "feat/task-025");
        assert_eq!(
            git(repo.path(), &["rev-parse", "main"]),
            git(&folder, &["rev-parse", "HEAD"])
        );
    }

    #[test]
    fn a_refused_worktree_comes_back_with_gits_message() {
        let repo = repo();
        let beside = TestDir::new();
        git(repo.path(), &["branch", "feat/task-025"]);

        let taken = add_worktree(
            &git_program(),
            repo.path(),
            &beside.path().join("wt"),
            "feat/task-025",
            "main",
        );
        let no_base = add_worktree(
            &git_program(),
            repo.path(),
            &beside.path().join("wt2"),
            "feat/x",
            "no-such-branch",
        );

        assert!(
            taken.as_ref().is_err_and(|e| e.contains("already exists")),
            "{taken:?}"
        );
        assert!(
            no_base
                .as_ref()
                .is_err_and(|e| e.contains("no-such-branch")),
            "{no_base:?}"
        );
        assert!(!beside.path().join("wt").exists());
    }

    #[test]
    fn names_git_would_read_as_options_and_bad_folders_are_refused_before_git_runs() {
        let repo = repo();
        let beside = TestDir::new();
        let wt = beside.path().join("wt");
        let add = |folder: &Path, branch: &str, base: &str| {
            add_worktree(&git_program(), repo.path(), folder, branch, base)
        };

        assert!(add(&wt, "--force", "main").is_err());
        assert!(add(&wt, "feat/x", "-q").is_err());
        assert!(add(&wt, " ", "main").is_err());
        assert!(add(Path::new("relative/wt"), "feat/x", "main").is_err());
        assert!(add(beside.path(), "feat/x", "main").is_err());
        assert!(!wt.exists());
    }

    #[test]
    fn the_task_folder_is_the_first_folder_with_tasks_in_it() {
        let a = TestDir::new();
        let b = TestDir::new();
        fs::create_dir(b.path().join("tasks")).unwrap();

        let found = guess_folder(&[a.path().to_path_buf(), b.path().to_path_buf()]);

        assert_eq!(found, Some(b.path().join("tasks")));
        assert_eq!(guess_folder(&[a.path().to_path_buf()]), None);
    }
}
