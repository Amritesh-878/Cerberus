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

**Two tabs side by side.** Drag a tab from the sidebar onto the right half of the chat area to show
it beside the tab on screen; the right half lights up while a drop there would split. Any two tabs
can share the view. Drag the line between them to resize (the arrow keys move it when it has
focus), and double-click it for half and half; the share is saved. **Close split** in the bar
above the two ends it. On a narrow window the two stack instead.

The split stays until **Close split** or until either tab closes. Showing another tab or project
hides it, and showing either of its tabs again brings it back as it was. Messages between accounts
open a split too (below).

Claude Code's tables count an emoji such as ✅ as two columns, and so does Cerberus, so their
borders line up. When a tab's size changes (the window, the sidebar, a split), Cerberus tells the
program the new size once it settles, so Claude Code redraws once at the new width.

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

**`claude` typed in a shell tab.** Run `claude` (or `claude --resume <id>`) in a shell tab, or in
the shell of an ended chat, and within a second or two the tab becomes a Claude Code tab: its icon,
the working, waiting and limit marks, alerts, and Shift+Enter for a new line. Cerberus finds it
through the account's `sessions` file of a `claude` running below that tab's shell, so a chat in
the same folder in another tab or window is never taken for it. Once the chat has its first
message, the tab saves it, so a restart brings the tab back as a Claude Code tab resuming it, in
the folder it ran in. When it exits, the tab is a shell again with that chat as its ended chat, so
**Resume chat** and **Continue the ended chat on…** use it. A chat that exits before its first
message leaves the tab as it was. Flags you typed (such as `--model`) aren't kept: after a restart
the chat resumes with the tab's usual start. A `claude -p` run is a command, not a chat, and is not
followed. Tracking follows the tab's own account: a `claude` you start with another
`CLAUDE_CONFIG_DIR` set in the shell is not followed.

A chat started this way has no Cerberus message helper, so it can't send messages to other
accounts. The bar above the tab says so and offers **Reopen as Claude tab**, which closes the shell
and that `claude` (asking first while it is working) and starts the same chat again as a Claude
Code tab, with messages.

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

## The right panel

The panel button at the right of the top bar, or Ctrl+Shift+B (Cmd+Shift+B on macOS), shows the
tab on screen in cards, and follows you as you switch tabs. Drag its left edge to resize it
(double-click resets it); click a card's title to fold it. Whether it is open, its width, the
folded cards, starred skills and dismissed PRs are kept in `profiles.json`.

- **Environment:** the tab's account with its email and plan, the tab's folders (a click opens one
  in the file manager; an extra folder shows its branch) and the main folder's git branch. The gear
  opens **Edit environment** (below).
- **Context:** how full the shown chat's context window is, and its model. **Compact** types
  `/compact` into the chat's input; nothing is sent until you press Enter. See Live cards below.
- **Usage:** one row per account with its 5-hour and 7-day plan use and when each resets, from the
  newest reply of any tab on that account. "Unknown" until a tab on the account gets a reply; an
  account signed in with an API key never reports one.
- **Changes:** one row per repo among the tab's folders, the main folder's first: lines added and
  removed on the branch since it left the remote's default branch (`origin/HEAD`, else `main` or
  `master`), uncommitted and untracked files included. **Create PR**
  opens a shell tab in the folder with `gh pr create --fill` typed; nothing runs until you press
  Enter. The code button beside it opens the folder in VS Code, whose Source Control view shows
  the diff; it shows only when VS Code's `code` command is on `PATH` (on macOS the app in
  Applications also counts). With the [GitHub CLI](https://cli.github.com) signed in, each repo's
  PRs follow its row, newest first: up to 100 drafts and open PRs, however old, and the newest 100 of any state,
  five at a time with **Show more**. A PR's **⋯** menu has **Open on GitHub**, **Copy link**, **Dismiss** (hidden until the PR
  changes on GitHub) and, on a draft, **Ready for review**, which asks first and then runs
  `gh pr ready <number>`.
- **Skills:** the project's `.claude/skills`, the account's `skills` folder, the skills synced
  from claude.ai (named `anthropic-skills:<skill>`, marked "claude.ai") and the skills of each
  enabled plugin (named `<plugin>:<skill>`, marked "plugin"), with each skill's description. The
  ones the account uses most come first (Claude Code's own count, from the account's
  `.claude.json`, from which only skill names and counts and the signed-in organization and account
  ids are read). The star on a row pins it to the
  top for that account. **Shift-click** a skill (or Shift+Enter on it) to type `/<skill>` into the
  chat's input; nothing is sent until you press Enter, and shell tabs get nothing. Skills built
  into Claude Code are not listed. Synced skills come from the folder of the organization and
  account signed in now, as Claude Code picks it; if that is unknown, every synced folder is listed.
- **Connectors:** the MCP servers the tab's `claude` would load: the account's user servers, its
  servers for this folder, the project's `.mcp.json`, each enabled plugin's servers (named
  `<plugin>:<server>`), and Cerberus's own `cerberus` server when the tab's chat was started with
  messages between accounts on.
  Only names and kinds are read, never commands, URLs, headers or env values. "Configured" means
  set up, not checked to be running. Connectors added on claude.ai (Gmail, Drive and the like)
  live there and are not listed. The card starts folded; open it once and it stays open.

A plugin counts as enabled the way Claude Code decides it: installed for every folder or for this
one (`plugins/installed_plugins.json` in the config folder), and not switched off in
`enabledPlugins` in the user, project or local settings, the later file winning. From those files
only the plugin switches, where each plugin is installed, and the `syncClaudeAiSkills` switch are
read. Any of these files that exists but cannot be read is named at the end of Connectors.

Nothing runs while the panel is closed. While it is open, git is read when it opens, on a tab
change and every 10 seconds; GitHub is asked about a repo at most once a minute. Connectors are
read again when a chat links to Cerberus for messages or leaves it.

### Edit environment

The gear on the Environment card edits the tab on screen. Nothing changes until **Save**.

- **Name:** the tab's title.
- **Folders:** the main folder (**Change…**) and any extra folders (**Add folder…**, and the cross
  to remove one), each with its git branch. A Claude Code tab starts with each extra folder as
  `--add-dir`, every time. Adding one to a running chat also types `/add-dir <folder>` into its
  input for you to send (one folder per save is typed; the rest apply on the next start). Removing
  one applies on the next start.
- **A new main folder** restarts the tab there. A Claude Code chat is copied to the new folder's
  project on the same account and resumes with `claude --resume`, so it keeps its history; the old
  folder keeps its copy. If the chat is working, the dialog asks first. A shell tab just restarts
  in the new folder.
- **Environment variables** are set for the tab's program each time it starts, under the ones
  Cerberus manages: `CLAUDE_CONFIG_DIR`, `TERM`, `COLORTERM`, `ZDOTDIR`, the variables Claude Code
  sets for its own shells, and any name starting with `CERBERUS_` or `CLAUDE_CODE_MESSAGING_` are
  refused. The dialog shows names only. Values are stored in plain text in `tree.db` (see below)
  and never logged or shown in an error, so keep real keys in your own secret store and reference
  them from there.

### Live cards

Each Claude tab is started with `--settings` pointing at a small file of Cerberus's own (in the app
data folder, `live/tab-<id>.json`) that sets one key, `statusLine`. Claude Code runs that status
line command, the `cerberus-live` helper shipped beside the app, after each reply, and hands it the
status line's JSON. The helper sends the app only the session id, model name, context percentage,
and the 5-hour and 7-day used percentages and reset times. Prompts, file paths, costs and the
transcript path in that JSON are skipped, never kept or logged. Your settings files are never
written.

If you have a status line of your own, it still runs and shows: Cerberus's command passes the same
JSON on to it unchanged, and keeps its `padding` and `refreshInterval`. On Windows without Git Bash,
Claude Code runs status lines in PowerShell; there Cerberus leaves your status line alone, and the
Context card says the chat can't report.

**Warn when an account passes N% of its 5-hour limit** (Accounts, under Claude tabs; 85% by
default) shows a banner over the chat and, with desktop alerts on, a notification: "work is at 87%,
resets 6:40pm". It comes once per account per 5-hour window. The banner lists **Continue on
<account>** for each signed-in account below the line; a click opens the usual Continue on confirm
for the tab that crossed it, and nothing moves until you confirm. **Dismiss** hides it until the
next window.

The **Live cards** switch, in the same place, turns all of it off; Claude tabs started after that
get no `--settings` and start as before.

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
project, never chat text or an account. Turn alerts off in **Accounts > Desktop alerts**.

The state comes from Claude Code's `sessions` file. For a limit, Cerberus reads only the error
type and reset time from the chat's last record, and never shows, logs or stores anything else
from a transcript.

## Moving a chat to another account

Right-click a Claude Code tab and pick **Continue on `<account>`…** to carry its chat to another
of your accounts, for example to keep work and personal chats apart. If the chat is running,
Cerberus asks before closing it, and the row says "Moving to `<account>`…" until it lands.

On a tab whose chat ended (now a shell), the item reads **Continue the ended chat on
`<account>`…**: it moves that tab's ended chat, which is the last `claude` you ran in its shell if
that chat was saved, and asks first since the move closes the shell. While a `claude` you typed
in the shell runs, the tab offers **Continue on `<account>`…** for that chat, as any running chat.

Cerberus copies the chat's file, and its folder of subagent chats, into the other account's
folder under a temporary name, renaming it once complete, then reopens the tab there with
`claude --resume`. The original stays on the first account. When the other account already has an
older copy, it is kept as `<id>.jsonl.cerberus-old`, never deleted. It refuses when the other
account is signed out or has that chat open right now. On any failure the tab stays where it was
and says why; in the rare case that putting the older copy back fails too, the message says part of
it stays under a `.cerberus-old` name. The copy moves files only; Cerberus never reads what is in
them.

## Messages between accounts

Claude Code's own `SendMessage` reaches chats on the same account only. Cerberus lets a Claude Code
tab message a Claude Code tab on another of your accounts, and every message waits for you.

- **Sending.** Each Claude Code tab gets a `cerberus` MCP server with two tools: `list_tabs` (the
  Claude Code tabs on your other accounts, with their ids) and `send_to_tab`. Ask a chat to message
  another tab and it calls `send_to_tab`; the call waits while you decide.
- **Approving.** A popup shows the sending tab, the target tab, each with its account colour, and
  the whole message in a box you can edit. **Approve**, **Approve edited** or **Reject**. Nothing
  approves by itself, there is no "always allow", and Escape stays with the terminal. Several
  messages wait in the order they came, with a count. The sending chat is told what happened:
  delivered, delivered as you edited it (with the new text), rejected, or waiting for Enter.
- **Delivery.** An approved message goes into the target chat's own Claude Code inbox, so it
  shows there as "Message from @<account>-<tab>" and the chat reads it as a message from another
  session, not as your words. When that isn't possible, Cerberus types the message into the
  target's input box instead, never pressing Enter: you press Enter there to send it, and the popup
  says so. If the inbox took the message but the tab hasn't shown it within 5 seconds (a long turn,
  say), the popup says it may still arrive and offers **Type it in instead**. Nothing is typed
  unless you click it, but the inbox copy can still show after you do: check the target chat
  before pressing Enter, or the message arrives twice.
- **Both chats side by side.** Every delivered message opens or keeps the split of its two tabs:
  approved, approved as edited, typed into the input box, or typed in later with **Type it in
  instead**. A new split puts the sender on the left; a reply between the same two keeps their
  sides. A reject, a cancel or a failed delivery never changes the split. It stays as described in
  [Projects and tabs](#projects-and-tabs): hidden while another tab shows, back with either of its
  tabs, until **Close split** or either tab closes. The reply takes the same road and the same popup.
- **Cancelling.** Esc in the sending chat stops its call and the message leaves the popup. A closed
  target tab or a restart ends a waiting message, and the sending chat is told it wasn't sent. If
  the sending chat closes or its helper ends (killed or crashed), its message leaves the popup, which
  says the sending chat is gone.
- **Notes.** A tab whose messages can only be typed in shows "messages: prefill only" in the top
  bar: a Claude Code version Cerberus hasn't tested, or macOS, where inbox delivery isn't checked
  yet. "messages off" means the tab's helper didn't start within 10 seconds, or ended.
- **Turning it off.** **Accounts > Messages between accounts.** Off, Claude Code tabs start exactly
  as before. A change applies to Claude Code tabs started after it. Your own MCP servers load either
  way.

How it works: Cerberus starts each Claude Code tab with `--mcp-config=<app data>/mcp.json` and
`--allowedTools=mcp__cerberus__send_to_tab`, beside your own MCP servers. The server is
`cerberus-mcp`, installed next to the app. It reaches the app over a local pipe that only your user
can open, with a random token per tab handed over in the tab's environment, never written to disk.
The message text stays in memory: Cerberus never logs it or saves it. Delivery into the inbox uses
the token Claude Code gives its own child processes; Cerberus reads no credentials, key files or
transcripts for it.

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

The setting is **Accounts > Suggestions from history**, on by default, and applies to shell tabs
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
| MCP config for messages between accounts (`mcp.json`) | `%APPDATA%\com.cerberus.terminal` | `~/Library/Application Support/com.cerberus.terminal` |
| Status line settings for live cards (`live/tab-<id>.json`) | same folder | same folder |

Uninstalling on Windows keeps this folder unless you tick the box to delete the app data. An
unreadable `profiles.json` is kept as `profiles.json.bad` and the app offers to start fresh.
