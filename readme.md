# Cerberus

A fast Windows terminal built around Claude Code. One window holds every project as a tree: a
planner tab per project with coder sub-tabs under it, each tab tied to a folder, a Claude account
and a saved session, so a crash or a closed window never loses a chat.

Each tab runs the official `claude` CLI under its account's `CLAUDE_CONFIG_DIR`, so three accounts
run side by side and a chat can continue on another account. Claude Code is the only AI in it.

Built with Claude Code.

**Status:** early. Projects and tabs run side by side; saving them across restarts comes next.

## Install

Download `Cerberus_<version>_x64-setup.exe` from
[Releases](https://github.com/Amritesh-878/Cerberus/releases) and run it. It installs for your
user only, with no admin prompt, on 64-bit Windows 10 or 11. Tabs run
[Claude Code](https://code.claude.com/docs/en/setup), which is installed separately. Each
release lists the installer's SHA-256 beside it; compare it with `Get-FileHash <file>` in
PowerShell.

The installer is not code-signed yet, so Windows SmartScreen may warn on first run: choose
**More info**, then **Run anyway**.

The installer adds its folder to your user `PATH`, so `cerberus open <project>[/<tab>]` works
from any terminal opened after the install; terminals already open keep the old `PATH`.
Uninstalling removes it again.

Uninstall from **Settings > Apps**. Your accounts and settings in
`%APPDATA%\com.cerberus.terminal` are kept unless you tick the box to delete the app data.

**winget:** coming soon.

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
window ends every program the app started.

| Keys | Does |
|---|---|
| Ctrl+Tab, Ctrl+Shift+Tab | Next or previous tab, in sidebar order |
| Ctrl+Shift+T | New tab in the current project |
| Ctrl+Shift+W | Close the current tab |

These miss Claude Code's own keys (Ctrl+T, Ctrl+C, Ctrl+V, Alt+V, Shift+Tab).

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
