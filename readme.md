# Cerberus

A fast Windows terminal built around Claude Code. One window holds every project as a tree: a
planner tab per project with coder sub-tabs under it, each tab tied to a folder, a Claude account
and a saved session, so a crash or a closed window never loses a chat.

Each tab runs the official `claude` CLI under its account's `CLAUDE_CONFIG_DIR`, so three accounts
run side by side and a chat can continue on another account. Claude Code is the only AI in it.

Built with Claude Code.

**Status:** early. Projects and tabs run side by side and are saved across restarts.

## Using it

### First run

The first launch opens a short walkthrough:

1. **Install.** Checks that `claude` is installed. If not, it shows Anthropic's install
   command and can open it in a PowerShell tab, typed but not run; press Enter to install.
2. **Accounts.** Each account keeps its own sign-in, settings and chats, in its own folder.
   `~/.claude` (the default) and any `~/.claude-<name>` folder Claude Code has used are offered
   for import. A new account needs only a name; its folder is `~/.claude-<name>` unless you
   pick another with **Change…**. **Add** opens Claude Code under the new account, and its own
   first run signs it in; close the tab (or press **Done**) once you're signed in. An account
   that is signed out shows **Sign in**. Each account shows its email and plan, read from
   `claude auth status`.
3. **New tabs.** Which account new tabs use.

**Accounts** (the person button at the bottom of the sidebar) adds, renames or removes accounts,
and picks the account new tabs use, the shell the **Shell** button opens, and the folder where
**New project** opens its folder picker. Removing an account only forgets it; its folder stays
on disk.
Accounts and settings live in `profiles.json` in the app's config folder
(`%APPDATA%\com.cerberus.terminal`). If that file cannot be read, the app offers to start fresh
and keeps the old file as `profiles.json.bad` (`.bad2`, `.bad3` and so on when that name is
taken).

### Projects and tabs

The sidebar lists projects, each a folder. **New project** opens the Windows folder picker, and
the folder's name becomes the project's name. Under each project sit its tabs:

- A **planner** is a Claude Code tab at the top of a project.
- A **sub-tab** sits under a planner, one level down: a Claude Code coder or a shell.
- A shell tab opened on its own sits at the top of the project, beside the planners.

The **+** beside a project opens the new tab box: pick the account, pick whether the tab goes on
its own or under a planner, then **Claude Code** or **Shell**. The tab starts in the project's
folder. Its title is the program and the account until you rename it (double-click it or press
F2). Closing a planner leaves its sub-tabs open, moved up to the top of the project. Search
filters tabs by title or account.

A project is renamed the same way, from its row. The **×** beside it removes it from the
sidebar: when it has tabs open, Cerberus asks first, then closes them. The folder is never
touched.

Every tab keeps running while another shows, with its scrollback. Closing a tab (its ×, or
Ctrl+Shift+W) ends that tab's program and anything it started, and no other tab's. Closing the
window ends every tab's program. A window started from a tab, such as an editor opened with
`code .` in a shell tab, stays open, as in Windows Terminal. A crash or a forced kill ends
everything the app started, those windows included: Cerberus runs in a Windows job that ends with
it, and lets go of that job only on a normal close.

| Keys | Does |
|---|---|
| Ctrl+Tab, Ctrl+Shift+Tab | Next or previous tab, in sidebar order |
| Ctrl+Shift+T | New tab in the current project |
| Ctrl+Shift+W | Close the current tab |
| Ctrl+Shift+A | Next tab whose chat is waiting on you |

These miss Claude Code's own keys (Ctrl+T, Ctrl+C, Ctrl+V, Alt+V, Shift+Tab, Ctrl+Shift+C) and
WebView2's Ctrl+Shift+J.

### Restarts and resuming

Projects and tabs are saved as you change them, in `tree.db` in the app's folder
(`%APPDATA%\com.cerberus.terminal`). After a restart, or a crash, the sidebar comes back as it
was: names, accounts, folders and order. Nothing starts until you open a tab, so twenty saved
tabs cost nothing until you use them.

Opening a saved Claude Code tab runs `claude --resume` with that tab's chat, in its folder and
under its account, and the tab says "Resuming chat…" while it loads. Cerberus learns the chat
from Claude Code's `sessions` file for the tab's process, and follows it to the new chat after
`/clear`. A chat is saved once it has its first message; a tab whose chat file has since been
deleted starts a new chat and says so. Shell tabs start fresh in their folder. Setup tabs, such
as signing an account in, are not saved.

If a save fails, the bar shows "Tabs not saved" and Cerberus retries every two seconds. Starting
Cerberus again brings the open window forward instead of opening a second one. Should a second
copy start anyway, only the first saves tabs: the second runs with saving off and says so.
A `tree.db` that cannot be read is renamed to `tree.db.bad` (or `.bad2` and so on), and Cerberus
starts with no saved tabs and says where the old file went.

### Opening a tab from the command line

```powershell
cerberus open alpha/planner   # project alpha, tab planner
cerberus open alpha           # project alpha: the tab last shown there since Cerberus started, else its first
```

This starts Cerberus if it is not running, or brings the open window forward, and shows that
tab; a saved Claude Code tab resumes its chat as usual. The project matches by name, then the
same name in any case, then its folder's name; the tab matches by title the same way. Words
after `open` are joined, so `cerberus open my app/planner` needs no quotes. A running Cerberus
answers from its open window, so a tab made a moment ago is found; otherwise the saved file is read.

When nothing matches, Cerberus prints the close names and exits with 1. A name that matches
two projects or tabs lists them and exits with 1 too; rename one to tell them apart. Bad usage
exits with 2. Only project names, folders and tab titles are printed, never chat content.

The installer does not put `cerberus` on `PATH` yet; call it by its full path
(`%LOCALAPPDATA%\Cerberus\cerberus.exe`). A release build is a windowed program, so a shell does
not wait for it and its messages and exit code are lost; the debug build that `npm run tauri dev`
makes (`src-tauri\target\debug\cerberus.exe`) is a console program and shows both.

### The session board

Each Claude Code tab's row shows what its chat is doing:

| Mark | Means |
|---|---|
| A small spinner | Working on a reply |
| A white dot, and the title in bold | Finished while you were on another tab, so it is waiting on you. Opening the tab clears it. |
| A clock and a time | The last turn hit a usage limit; the time is when it resets |
| A dimmed title | The program exited |
| Nothing | Idle |

The state comes from Claude Code's `sessions` file. The limit comes from the last record of the
chat's transcript, of which Cerberus reads only the error type and the reset time; it never
shows, logs or stores anything else from a transcript.

A tab under an account other than the default gets `CLAUDE_CONFIG_DIR` set to its folder, so
`claude` there, or typed in a shell tab, runs on that account. The default account leaves the
variable unset, as Claude Code expects. The variables a Claude Code session sets for its own
shells (`CLAUDECODE`, `CLAUDE_CODE_SESSION_ID` and the like, and the `NO_COLOR` that comes with
them) are removed, so a tab runs the same whether Cerberus started from the Start menu or from
a Claude Code chat. Settings such as `CLAUDE_CODE_USE_BEDROCK`, and a `NO_COLOR` of your own,
stay. The bar above the
terminal shows the account; hover it for the email and plan.

The programs a tab can run:

| Program | What runs |
|---|---|
| Claude Code | `claude` from `PATH` |
| PowerShell | `pwsh` from `PATH`, else Windows PowerShell (`powershell.exe`) |
| Git Bash | `bin\bash.exe` under Git's install folder, found from `git` on `PATH` |

Selecting text copies it; Ctrl+V pastes text. In Claude Code, Alt+V or Ctrl+V pastes an image
from the clipboard. When the program exits the tab stays open with its exit code.

The panel button at the top right opens an empty side panel, kept for later.

## Stack

Tauri 2 with a Rust core (PTYs, sessions, accounts, persistence) and a Solid + TypeScript webview
built with Vite. Terminals are xterm.js (WebGL renderer) over `portable-pty`; the file pane will use
CodeMirror.

## Run it

Needs Node 24 and a Rust toolchain with the MSVC target, plus the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) for Windows (WebView2, C++ build
tools).

```sh
npm ci              # install from the lockfile; `npm install` rewrites it on Windows
npm run tauri dev   # start Vite and open the app window
npm run tauri build # release build and installer
```

## Quality gate

Run in this order; 0 errors, 0 warnings and 100% passing is the only pass.

```sh
npm run lint:fix && npm run typecheck && npm test
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
```

## Where things are

| Path | What it is |
|---|---|
| `src/` | Solid webview: components, styles and their Vitest tests |
| `src-tauri/` | Rust core, Tauri config and window permissions (`src-tauri/capabilities/`) |
| `AGENTS.md` | Project context and the rules every agent session follows |
| `coding_standards.md` | How code, commits and PRs are written and reviewed |
| `.claude/` | The skills, agents and settings Claude Code loads in this repo |

The agent workflow comes from the `prompts` template, kept as the `template` remote. Planning docs
(task files, decisions such as `DEC-###`, project state) are kept privately and are not part of
this repo.

## License

MIT, see `LICENSE`. Skills vendored under `.claude/skills/` keep their own MIT notices in
`.claude/skills/notice.md`.
