# Cerberus guide

How each part of Cerberus works, in more detail than the [README](../readme.md).

## First run and accounts

The first launch opens a short walkthrough:

1. **Install.** Checks that `claude` is installed. If not, it shows Anthropic's install command and
   can open it in a shell tab (PowerShell on Windows, your login shell on macOS), typed but not
   run: press Enter to install.
2. **Accounts.** Each account keeps its own sign-in, settings and chats, in its own folder.
   `~/.claude` (the default) and any `~/.claude-<name>` folder Claude Code has used are offered
   for import. A new account needs only a name; its folder is `~/.claude-<name>` unless you pick
   another with **Change…**. **Add** opens Claude Code under the new account, and its own first run
   signs it in. Each account shows its email and plan, read from `claude auth status`.
3. **New tabs.** Which account new tabs use.

Setup ends with **Finish**, once there is an account, or with **Skip setup** at its top right.
Nothing else leaves it: the sidebar shows only setup's own tabs (such as the installer),
the app's shortcuts do nothing, and a `cerberus open` request waits until setup ends. Closing the
app part way through reopens setup at the same step. After a skip, the start page says what is
missing (Claude Code, or an account) and **Finish setup** reopens it there.

**Accounts** (the person button at the bottom of the sidebar) adds, renames or removes accounts,
and sets the account new tabs use, the shell the **Shell** button opens, where **New project**
opens its folder picker, and whether alerts and shell suggestions are on.

- **Colours.** Click an account's dot to pick its colour from eight. Every dot for that account
  follows: the sidebar, the account pill, the new tab box and the menus. A new account gets the
  first colour no other account shows; an account from before colours keeps the one it had.
- **Removing an account** only forgets it; its folder stays on disk. When saved tabs use it,
  Cerberus names them and asks first. On yes, their programs close and the tabs stay in the
  sidebar, dimmed, with **Continue on `<account>`…** to carry each chat to another account.
  The last account can't be removed.

## Projects and tabs

**New project** opens a folder picker, and the folder's name becomes the project's name. The
**+** beside a project opens the new tab box beside it: pick the account, whether the tab goes on
its own or under a planner, then **Claude Code** (Enter) or **Shell**. The tab starts in the
project's folder. Ctrl+Shift+T (Cmd+T on macOS) opens the box beside the current project's **+**.

The account pill at the top right shows the shown tab's account and its sign-in. Clicking it
opens the same menu as right-clicking the tab: **Continue on `<account>`…** for the other
accounts, **Resume chat** for an ended chat, and **Accounts**, always last.

- A **planner** is a Claude Code tab at the top of a project.
- A **sub-tab** sits under a planner: a Claude Code coder or a shell. Closing a planner moves its
  sub-tabs up to the top of the project.
- The **×** beside a project removes it from the sidebar, asking first when it has tabs open. The
  folder is never touched.

Every tab keeps running while another shows. Closing a tab ends its program and anything it
started, and no other tab's. Closing the window ends every tab's program, but a window started
from a tab (an editor opened with `code .`) stays open. On Windows a crash or forced kill also
ends what the app started; on macOS a tab's programs get a hangup when its terminal closes.

Drag the sidebar's right edge to resize it, or double-click the edge to reset it.

## Restarts and resuming

Projects and tabs are saved as you change them, in `tree.db` in the app's data folder. After a
restart or a crash the sidebar comes back as it was, and nothing starts until you open a tab.

Opening a saved Claude Code tab runs `claude --resume` with that tab's chat, in its folder and
under its account. Cerberus learns the chat from Claude Code's `sessions` file and follows it to
the new chat after `/clear`. A tab whose chat file has since been deleted starts a new chat and
says so. Shell tabs start fresh in their folder.

**Ended chats.** When a chat ends by itself, for example with `/exit`, its tab becomes a shell in
the same folder and account, with Claude Code's "Resume this session with" line still above the
prompt. It stays a shell after a restart. **Resume chat** in its right-click menu brings the chat
back. Closing Cerberus, a crash, or moving the chat never counts as ended.

A reload of the window would end every tab's program, so an installed Cerberus blocks it: the
window's own right-click menu (text boxes and the app's menus keep theirs), F5, Ctrl+R and
Ctrl+Shift+R (Cmd+R and Cmd+Shift+R on macOS). Ctrl+R in a terminal still reaches the shell.

If a save fails, the bar shows "Tabs not saved" and Cerberus retries. Starting Cerberus again
brings the open window forward instead of opening a second one. An unreadable `tree.db` is kept
as `tree.db.bad` and Cerberus starts with no saved tabs.

## Keyboard shortcuts

Ctrl+Shift+/ (Cmd+/ or **Help > Keyboard Shortcuts** on macOS) shows every shortcut Cerberus
answers to, from anywhere, setup included. Escape, a click outside or the same keys close it.
Ctrl+/ alone stays with the terminal. The start page lists the main ones.

## The session board and alerts

Each Claude Code tab's row shows what its chat is doing:

| Mark | Means |
|---|---|
| A small spinner | Working on a reply |
| A white dot and a bold title | Stopped while you were on another tab, so it is waiting on you |
| A clock and a time | The last turn hit a usage limit; the time is when it resets |
| A dimmed title | The program exited |

Ctrl+Shift+A (Cmd+Shift+A on macOS) jumps to the next tab waiting on you.

When a chat stops, hits a limit or ends while you aren't looking at it (its tab is hidden, or the
window isn't focused), you get a desktop notification such as "planner · alpha: Waiting on you".
Clicking it, while it is on screen, opens that tab. A notification names only the tab and its
project, never chat text or an account. Turn alerts off in **Accounts**.

The state comes from Claude Code's `sessions` file. For a limit, Cerberus reads only the error
type and reset time from the chat's last record, and never shows, logs or stores anything else
from a transcript.

## Moving a chat to another account

Right-click a Claude Code tab and pick **Continue on `<account>`…** to carry its chat to another
of your accounts, for example to keep work and personal chats apart. If the chat is running,
Cerberus asks before closing it, and the row says "Moving to `<account>`…" until it lands.

Cerberus copies the chat's file, and its folder of subagent chats, into the other account's
folder under a temporary name, renaming it once complete, then reopens the tab there with
`claude --resume`. The original stays on the first account. When the other account already has an
older copy, it is kept as `<id>.jsonl.cerberus-old`, never deleted. It refuses when the other
account is signed out or has that chat open right now. On any failure the tab stays where it was
and says why; in the rare case that putting the older copy back fails too, the message says part of
it stays under a `.cerberus-old` name. The copy moves files only; Cerberus never reads what is in
them.

## `cerberus open`

The project matches by name, then the same name in any case, then its folder's name; the tab by
title the same way. Words after `open` are joined, so `cerberus open my app/planner` needs no
quotes. A running Cerberus answers from its open window; otherwise the saved file is read.

Exit codes: 0 opened, 1 no match or more than one (it lists them), 2 bad usage. Only project
names, folders and tab titles are printed, never chat content. With no arguments it starts the
app or brings it forward.

`cerberus` is a small console command beside the app, so a shell waits for it and sees what it
prints. The Windows installer adds it as `cerberus.com` in `%LOCALAPPDATA%\Cerberus`, on your
`PATH`. On macOS it is `Cerberus.app/Contents/MacOS/cerberus-cli`: **Install `cerberus` command**
in **Accounts** links it as `~/.local/bin/cerberus`, replacing an older link but never a file of
your own. If `~/.local/bin` isn't on your login shell's `PATH`, it shows the line to add to your
shell's startup file; Cerberus never edits that file itself. It refuses while Cerberus runs from
the disk image. In a dev build the command is `src-tauri/target/debug/cerberus-cli`.

## Programs and environment

| Program | What runs |
|---|---|
| Claude Code | `claude` from `PATH` |
| PowerShell (Windows) | `pwsh` from `PATH`, else Windows PowerShell |
| Git Bash (Windows) | `bin\bash.exe` under Git's install folder |
| Shells (macOS) | Your login shell first, then `/bin/zsh`, `/bin/bash` and Homebrew's `fish`, then each other installed shell in `/etc/shells`, each with `-l` |

A tab under an account other than the default gets `CLAUDE_CONFIG_DIR` set to its folder, so
`claude` there, or typed in a shell tab, runs on that account. The variables a Claude Code session
sets for its own shells (`CLAUDECODE`, `CLAUDE_CODE_SESSION_ID` and the like, with their
`NO_COLOR`) are removed, so a tab runs the same however Cerberus was started. Settings such as
`CLAUDE_CODE_USE_BEDROCK`, and a `NO_COLOR` of your own, stay.

Selecting text copies it. Ctrl+V pastes text (Cmd+V on macOS). In Claude Code, Alt+V or Ctrl+V
pastes an image (Cmd+V or Ctrl+V on macOS).

## Suggestions in shell tabs

As you type in a shell tab, a grey suggestion from your history appears after the cursor, and the
Right arrow accepts it. Cerberus doesn't draw these itself: it turns on what each shell already
has, through environment variables and startup files only, and your own config still loads first.
Cerberus never types into the shell.

| Shell | What it gets |
|---|---|
| zsh (macOS) | `ZDOTDIR` points at a folder Cerberus writes. Its `.zshenv`, `.zprofile`, `.zshrc` and `.zlogin` each run yours, from your own `ZDOTDIR` or else your home folder, and then the `.zshrc` puts `ZDOTDIR` back and loads a bundled [zsh-autosuggestions](https://github.com/zsh-users/zsh-autosuggestions) v0.7.1. If your config already loads zsh-autosuggestions, it isn't loaded twice. Your history file stays where it was |
| PowerShell 7 (`pwsh`) | After your profile, `Set-PSReadLineOption -PredictionSource History`, but only when PSReadLine is 2.1 or newer and predictions are off. A prediction source you already chose is kept |
| Windows PowerShell 5.1 | Nothing changes: it ships PSReadLine 2.0, which has no predictions. Install PowerShell 7, or PSReadLine 2.1 or newer, to get them |
| fish | Nothing: fish suggests by itself |
| bash and Git Bash | Nothing yet |

The setting is **Accounts > Suggestions in shell tabs**, on by default, and applies to shell tabs
started after you change it. Off, shells start exactly as before. Claude Code tabs never get it;
a tab whose chat ended and became a shell does. A shell tab started with its own command or
script (`pwsh -c ...`) is left alone, and so is the login shell Cerberus reads `PATH` from at
start on macOS.

## macOS notes

- Opened from Finder or the Dock, an app gets a short `PATH`. Cerberus reads `PATH` from your
  login shell once at start, so `claude` is found wherever your shell finds it. Tabs get a UTF-8
  locale when macOS gives none.
- Cerberus's keys use Cmd, so Ctrl keys reach the terminal, except Ctrl+Tab and Ctrl+Shift+Tab,
  which switch tabs as in Terminal.app. Option works as Meta, so
  Option+B, Option+F and Option+arrows move by word in a shell.
- In a terminal, Cmd+Left and Cmd+Right go to the start and end of the line, Cmd+Backspace
  deletes to the start, Option+Backspace deletes a word, Cmd+K clears the screen and scrollback,
  and Cmd+A selects all, with the bytes Ghostty sends. They are sent only when you press them.
- The terminal font is SF Mono, falling back to Menlo, in regular and bold alike.
- If `xattr` isn't an option: open Cerberus once, close the warning, then choose **Open Anyway**
  in **System Settings > Privacy & Security**. (Right-click > Open stopped working in macOS 15.)
- Windows only: the installer, its `PATH` entry and the uninstaller.
- The macOS build is built and tested in CI, and is in its first hands-on testing.

## Where data lives

| | Windows | macOS |
|---|---|---|
| Accounts and settings (`profiles.json`) | `%APPDATA%\com.cerberus.terminal` | `~/Library/Application Support/com.cerberus.terminal` |
| Saved tabs (`tree.db`) | same folder | same folder |
| zsh suggestion files (`shell/zsh`) | same folder | same folder |

Uninstalling on Windows keeps this folder unless you tick the box to delete the app data. An
unreadable `profiles.json` is kept as `profiles.json.bad` and the app offers to start fresh.
