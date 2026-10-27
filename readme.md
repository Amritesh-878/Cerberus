# Cerberus

A fast Windows terminal built around Claude Code. One window holds every project as a tree: a
planner tab per project with coder sub-tabs under it, each tab tied to a folder, a Claude account
and a saved session, so a crash or a closed window never loses a chat.

Each tab runs the official `claude` CLI under its account's `CLAUDE_CONFIG_DIR`, so three accounts
run side by side and a chat can continue on another account. Claude Code is the only AI in it.

Built with Claude Code.

**Status:** early. The app runs one terminal tab at a time under a chosen account; the project
tree comes next.

## Using it

### First run

The first launch opens a short walkthrough:

1. **Claude Code.** Checks that `claude` is installed. If not, it shows Anthropic's install
   command and can open it in a PowerShell tab, typed but not run; press Enter to install.
2. **Accounts.** An account is a name for a Claude Code config folder, which holds its own login,
   settings and chats. `~/.claude` (the default) and any `~/.claude-<name>` folder Claude Code
   has used are offered for import. A new account gets a folder, `~/.claude-<name>` unless you
   pick another. **Log in** opens Claude Code under that account, where `/login` signs it in.
   Each account shows its email and plan, read from `claude auth status`.
3. **Defaults.** The account, program and start folder the launcher picks first.

**Accounts…** on the launcher reopens it to add, rename or remove accounts. Removing an account
only forgets it; its folder stays on disk. Accounts and defaults live in `profiles.json` in the
app's config folder (`%APPDATA%\com.cerberus.terminal`). If that file cannot be read, the app
offers to start fresh and keeps the old file as `profiles.json.bad`.

### Tabs

A tab under an account other than the default gets `CLAUDE_CONFIG_DIR` set to its folder, so
`claude` there, or typed in a shell tab, runs on that account. The default account leaves the
variable unset, as Claude Code expects. The tab header shows the account; hover it for the email
and plan.

The launcher lists the programs it found:

| Program | What runs |
|---|---|
| Claude Code | `claude` from `PATH` |
| PowerShell | `pwsh` from `PATH`, else Windows PowerShell (`powershell.exe`) |
| Git Bash | `bin\bash.exe` under Git's install folder, found from `git` on `PATH` |

Pick one to open it in a tab under the chosen account, starting in the default folder (your
home folder unless set). A program that is not installed is
greyed out. Selecting text copies it; Ctrl+V pastes text. In Claude Code, Alt+V or Ctrl+V pastes
an image from the clipboard. When the program exits the tab stays open with its exit code; the ×
closes the tab, ends the program and anything it started, and returns to the launcher. Closing the
window ends every program the app started.

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
