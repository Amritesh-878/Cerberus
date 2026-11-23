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
3. **New tabs use.** The one account new tabs and Start a task start on.

Setup ends with **Finish**, once there is an account, or with **Skip setup** at its top right.
Nothing else leaves it: the sidebar shows only setup's own tabs (such as the installer),
the app's shortcuts do nothing, and a `cerberus open` request waits until setup ends. Closing the
app part way through reopens setup at the same step. After a skip, the start page says what is
missing (Claude Code, or an account) and **Finish setup** reopens it there.

### Accounts and chats

An account is one Claude sign-in, kept in its own folder (`~/.claude`, `~/.claude-work`). Claude Code
saves each chat in the folder of the account it ran on, so a chat id resumes only there. Cerberus
remembers which account has each chat: Find chat, `claude --resume <id>` in a shell tab, and a
restart all open a chat on its own account.

**Move to `<account>`** copies a chat into another account's folder and resumes it there, for
example when one account hits its limit. The copy left behind is never deleted. Between accounts of
two organizations, Claude can't reuse the chat's earlier thinking, so it rereads the chat and the
first replies are slower.

**Accounts** (the person button at the bottom of the sidebar) adds, renames or removes accounts,
and sets **New tabs use** (the account New tab and Start a task pick unless you choose another
there), the shell the **Shell** button opens, where **New project** opens its folder picker, and
whether alerts and shell suggestions are on.

- **Colours.** Click an account's dot to pick its colour from eight. Every dot for that account
  follows: the sidebar, the account pill, the new tab box and the menus. A new account gets the
  first colour no other account shows; an account from before colours keeps the one it had.
- **One account per sign-in.** A folder already added is refused with "Already added as Work.",
  and a folder signed in to the same login as an account you have with "Same login as Work. Use
  that account." An account added before this check shows "Same login as `<name>`" on its row;
  remove it when you're ready. An older accounts file with two accounts on one folder keeps the
  first, and their tabs go to it.
- **Removing an account** only forgets it; its folder stays on disk. When tabs use it, Cerberus
  names them and asks where they go (**Move them to**, the **New tabs use** account first). **Move
  and remove** moves each chat there, starts each shell there, then removes the account. The last
  account can't be removed.
- **A tab whose account is gone** says "Work was removed. This chat is saved." with **Move to
  `<account>`** (the New tabs use account) and **Other account…**. If an account you have uses the
  same folder, the tab goes to it by itself.

## Projects and tabs

**New project** opens a folder picker, and the folder's name becomes the project's name. The
**+** beside a project opens the new tab box beside it: pick the account, whether the tab goes on
its own or under a planner, then **Claude Code** (Enter) or **Shell** (or **Antigravity CLI** or
**Codex CLI**, when installed: see below). The tab starts in the
project's folder. Ctrl+Shift+T (Cmd+T on macOS) opens the box beside the current project's **+**.

The account pill at the top right shows the shown tab's account and its sign-in. Clicking it
opens the same menu as right-clicking the tab: **Move to `<account>`…** for the other
accounts, **Resume chat** for an ended chat, and **Accounts**, always last.

- A **planner** is a Claude Code tab at the top of a project.
- A **sub-tab** sits under a planner: a Claude Code coder or a shell. Closing a planner moves its
  sub-tabs up to the top of the project.
- The **×** beside a project removes it from the sidebar, asking first when it has tabs open. The
  folder is never touched.

Every tab keeps running while another shows. Closing a tab ends its program and anything it
started, and no other tab's. Closing the window ends every tab's program, but a window started
from a tab (an editor opened with `code .`) stays open. On Windows a crash or forced kill also
ends what the app started; on macOS and Linux a tab's programs get a hangup when its terminal closes.

Drag the sidebar's right edge to resize it, or double-click the edge to reset it.

**Two tabs side by side.** Drag a tab from the sidebar onto either half of the chat area to show
it on that side of the tab on screen; the half under the pointer lights up. Any two tabs can share
the view. Drag the line between them to resize (the arrow keys move it when it has focus), and
double-click it for half and half; the share is saved. **⇄** in the bar above the two swaps their
sides and keeps the line where it is; **Close split** ends it. On a narrow window the two stack
instead. Switching windows or pressing a key mid-drag cancels the drag.

The split stays until **Close split** or until either tab closes. Showing another tab or project
hides it, and showing either of its tabs again brings it back as it was. Messages between accounts
open a split too (below).

Claude Code's tables count an emoji such as ✅ as two columns, and so does Cerberus, so their
borders line up. When a tab's size changes (the window, the sidebar, a split), Cerberus tells the
program the new size once it settles, so Claude Code redraws once at the new width.

### Collapsing the sidebar

The button beside the search box at the top, or Ctrl+Shift+E (Cmd+Shift+E on macOS), folds the sidebar to a
thin rail: one icon per project (its initials), a dot on a project where a chat is waiting on you,
and an expand button at the top. Clicking a project's icon opens the sidebar with that project
unfolded. Collapsed or open is saved in `profiles.json`, and the width it opens to stays as you
dragged it. The rail starts no drags; drag a tab to split from the open sidebar.

Below 960 pixels wide the sidebar folds to the rail by itself, and opening it there lasts until the
window grows again; your own choice is kept for wider windows. Below 720 it becomes a drawer behind
the menu button, as before.

When the right panel would leave the chat less than about 60 columns, it opens over the chat
instead of beside it, with a close button in its own bar.

### Antigravity CLI and Codex CLI tabs

The new tab box lists **Antigravity CLI** (`agy`) and **Codex CLI** (`codex`) under **Shell** when
they are installed, and neither when it isn't. `codex` is found on your `PATH`. `agy` is found in its
installer's folder (`%LOCALAPPDATA%\agy\bin` on Windows, `~/.local/bin` on macOS and Linux), even
when Cerberus started before the install, or on your `PATH` from a folder whose name has
`antigravity` or `agy` in it; an unrelated program named `agy` elsewhere is not offered. To install:

```sh
irm https://antigravity.google/cli/install.ps1 | iex           # Antigravity CLI, Windows PowerShell
curl -fsSL https://antigravity.google/cli/install.sh | bash    # Antigravity CLI, macOS and Linux
npm install -g @openai/codex                                   # Codex CLI
```

Google replaced Gemini CLI with Antigravity CLI for unpaid and Google One users on 2026-06-18. If
your Gemini Code Assist plan still includes Gemini CLI, run `gemini` in a shell tab.

These tabs run the program as it is, in the tab's folder with the tab's environment variables.
Cerberus never signs in to them or reads their config, sign-in or history files, and none of the
Claude Code features apply: no account (the tab's account choice is ignored), no
`CLAUDE_CONFIG_DIR`, no messages between accounts, live cards, limit marks or chat tracking. The
right panel shows only **Environment** (folder and branch) and **Changes**.

- **Shift+Enter** sends Ctrl+J, which both CLIs document as a new line in the prompt.
- **A restart** starts the program again, fresh, in the same folder. Neither CLI can resume one
  tab's own chat without its history, so pick it inside the CLI with `/resume`, or reopen the
  folder's newest chat by hand with `agy --continue` or `codex resume --last`.

Claude, Antigravity and Codex names and marks belong to Anthropic, Google and OpenAI; Cerberus uses
them only to show which CLI a tab runs, and is not affiliated with or endorsed by them.

### Start a task

The planner writes a task file, a coder chat does the task: **Start a task…** opens that coder for
you. It is in a Claude tab's menu (right-click the tab, or click its account pill) and in the
planner's right panel, under **Environment**, for a planner tab or a coder under one.

- **Task file**: the `*.md` files in the project's task folder, newest first, each with its first
  heading. The folder is `tasks/` in the tab's folder or one of its added folders, until you pick
  another with **Change folder…**, which the project then keeps. **Choose file…** takes any one
  file. Cerberus reads only each file's name and first heading; the coder reads the task itself.
- **Account**: the one the project's last task used, else the tab's own.
- **Folder**: the project folder, or **New worktree**: `git worktree add` from a base branch (the
  repo's default branch unless you pick another) into `../<repo>-<task id>` on a new branch
  `feat/<task id>`, both editable. If git refuses, its message shows and no tab opens.

**Open** adds a Claude tab under the planner, named `TASK-025 · <first heading>`, in that folder on
that account. In a worktree the project folder is added to the chat (`--add-dir`). Once the chat is
ready, after any trust prompt for a new folder, the project's task prompt is typed into its input
with the placeholders filled. Cerberus never presses Enter: read it, then send it yourself.

The task prompt is under **Task prompt** in the same dialog, kept per project in the saved tabs.
**Reset to default** goes back to:

```text
Read AGENTS.md, then {task_file}, and do that task. Work in {folder} on branch {branch}. When you finish, write the handoff to {handoff_file} and tell {planner_tab}.
```

| Placeholder | Becomes |
|---|---|
| `{task_file}` | The task file's full path |
| `{task_id}` | `TASK-025` for `task-025.md`, else the file name without `.md` |
| `{folder}` | The coder's folder: the worktree, or the project folder |
| `{branch}` | The worktree's new branch, or the project folder's current branch |
| `{handoff_file}` | `completed/<task id>.md` beside the task folder (`plans/completed/task-025.md`) |
| `{planner_tab}` | The planner tab's name |

### Sending a handoff back

A coder tab opened by Start a task knows its task and handoff file, and its sidebar row shows
`TASK-025 · working`. When its chat reports writing that file, or the file was written since the
task opened (checked when the chat goes idle and when Cerberus starts), the row and the tab's bar
show **Handoff ready**. A desktop notification says so when the tab isn't on screen.

**Send to planner** (the bar, the row, or the tab's menu) puts the planner tab beside the coder and
tells it where the handoff is:

```text
TASK-025 handoff is ready: <handoff file>. PR: <link>
```

The PR is the one made from the coder's branch, found the same way as the Changes card's list; with
none, the line ends after the file. Cerberus never reads the handoff into the message: the planner
reads the file itself.

- **Planner on the same account**: the line is typed into the planner's input, or once its chat is
  ready if it wasn't running. Edit it there and send it yourself.
- **Planner on another account**: the line waits in the approval popup of
  [Messages between accounts](#messages-between-accounts), where you can edit it, approve it or
  reject it. Those messages need to be on in Settings.

The row then shows `TASK-025 · handoff sent`, and the menu offers **Send to planner again**.

A coder tab you opened by hand, under a planner, has **Send handoff…** in its menu instead. It asks
for the file, starting at the newest Markdown file the chat wrote, and takes the task id from the
file's name (`task-026.md` is `TASK-026`).

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

Shell tabs put Cerberus's own `claude` first on their PATH. It runs the real `claude` with
your arguments and the tab's Cerberus settings, so the chat has messages and live cards like any
Claude Code tab, and it types nothing. PowerShell and zsh put it first again after your profile.
`claude --resume <id>` for a chat on another account opens that chat in a new tab on its account;
the shell prints "It's on Work. Opened it in a new tab."

A `claude` started another way (by full path, or from a shell that puts its own folder first)
has no message helper. The bar above the tab says "Started by hand: messages need a Claude tab."
with **Reopen as Claude tab**, which closes the shell and that `claude` (asking first while it is
working) and starts the same chat again as a Claude Code tab.

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
(double-click resets it); click a card's title to fold it, and drag a card by its title row to move
it. **Reset card order**, at the foot of the panel once you have moved one, puts them back. Whether
it is open, its width, the folded cards, their order, starred skills and dismissed PRs are kept in
`profiles.json`.

- **Environment:** the tab's account with its email and plan, the tab's folders (a click opens one
  in the file manager; an extra folder shows its branch) and the main folder's git branch. The gear
  opens **Edit environment** (below).
- **Context:** how full the shown chat's context window is, and its model. **Compact** types
  `/compact` into the chat's input; nothing is sent until you press Enter. See Live cards below.
- **Outputs:** the files the shown chat wrote or edited, newest first, each once, with its folder
  relative to the tab's. A click shows the file in the file pane (below). The row's **<>** button is
  **Open in VS Code**; without VS Code it opens the file's folder instead, since opening a file the
  chat wrote, such as a script, could run it.
- **Background tasks:** the chat's background shells and subagents, running ones first, then the last
  20 finished, each by Claude Code's short description (kept in memory only, never the command), its
  kind, and how long it ran (counted from when Cerberus first saw it, since Claude Code gives no start
  time). A click types `/tasks` into the chat for you to send, where Claude Code shows the output.
  There is no stop button: Claude Code offers no way to stop one from outside the chat.
- **Usage:** one row per account with its 5-hour and 7-day plan use and how long until each resets
  ("in 2h 10m"; hover for the clock time), from the newest reply of any tab on that account. Bars
  take the account's colour, turn amber at the warning percentage and red at 100%. The last numbers are kept in the profiles file (numbers
  and times only), so after a restart they show dimmed with "as of" and the time they came, until a
  new reply. A window past its reset reads "reset". "Unknown" until a tab on the account has ever
  had a reply; an account signed in with an API key never reports one.
- **Changes:** one row per repo among the tab's folders, the main folder's first: lines added and
  removed on the branch since it left the remote's default branch (`origin/HEAD`, else `main` or
  `master`), uncommitted and untracked files included. **Create PR**
  opens a shell tab in the folder with `gh pr create --fill` typed; nothing runs until you press
  Enter. The code button beside it opens the folder in VS Code, whose Source Control view shows
  the diff; it shows only when VS Code's `code` command is on `PATH` (on macOS the app in
  Applications also counts). With the [GitHub CLI](https://cli.github.com) signed in, each repo's
  PRs follow its row, newest first: up to 100 drafts and open PRs, however old, and the newest 100 of any state,
  five at a time with **Show more**. A PR row's arrow button opens it on GitHub; its **⋯** menu has **Copy link**, **Dismiss** (hidden until the PR
  changes on GitHub) and, on a draft, **Ready for review**, which asks first and then runs
  `gh pr ready <number>`. A repo the chat wrote into (from Outputs, a worktree beside the
  tab's folder for instance) gets a row of its own for this session, marked "written by this
  chat", with its branch, changes, PRs and Open in VS Code. Its **⋯** menu has **Add to the tab**,
  which saves it as one of the tab's folders, as Edit environment's Add folder does. Every repo
  row's **⋯** menu also has **Worktrees** (below).
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

### Worktrees

**Worktrees** in a repo row's **⋯** menu lists every worktree of the repo (`git worktree list`),
the main folder first, each with its branch, its PR's number and state (from the PR list above),
"uncommitted changes" when it has any, and the tabs whose folder or added folder is inside it.

- **Stale** marks a worktree whose work is over (its branch's PR merged or closed, the remote
  branch it tracked is gone, or its folder is already gone) with no uncommitted or untracked
  files and no tab in it.
- **Remove stale (N)**, or **Remove…** in one worktree's **⋯** menu, lists what it will remove
  and asks first. Each is then checked again and removed with `git worktree remove`, never with
  `--force`. Its local branch is deleted only if it is merged: already in the default branch's
  history, or its PR merged at exactly the commit the branch is on (a squash merge). Nothing on
  GitHub changes: no push, and the remote branch stays.
- **Refused, with the reason:** the main folder, a worktree with uncommitted or untracked files,
  one a tab runs in, and one git has locked. "In use" means a tab's folder or added folder; a
  program you `cd` into the worktree from a shell is not seen, and Windows may then stop git
  from deleting the folder itself (its files go, and nothing uncommitted is lost, since a dirty
  worktree is never removed).
- **Open** from a worktree's **⋯** menu: in VS Code, in the file manager, or as a new shell or
  Claude tab there, under the tab on screen and on its account.

The list is read when you open it and again after a removal, and closes when you switch tabs.

### Edit environment

The gear on the Environment card edits the tab on screen. Nothing changes until **Save**.

- **Name:** the tab's title.
- **Folders:** the main folder (**Change…**) and any extra folders (**Add folder…**, and the cross
  to remove one), each with its git branch. A Claude Code tab starts with each extra folder as
  `--add-dir`, every time. Adding one to a running chat also sends `/add-dir <folder>` to it once
  the chat is idle with nothing typed in its input, and the tab says "Added <folder> to the chat"
  (one folder per save; the rest apply on the next start). `/add-dir` is handled by Claude Code
  itself and never reaches the model; it is the only command Cerberus sends with Enter. Removing
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
data folder, `live/tab-<id>.json`) that sets `statusLine` and `hooks`. Claude Code runs the status
line command, the `cerberus-live` helper shipped beside the app, after each reply, and hands it the
status line's JSON. The helper sends the app only the session id, model name, context percentage,
and the 5-hour and 7-day used percentages and reset times. Prompts, file paths, costs and the
transcript path in that JSON are skipped, never kept or logged. Your settings files are never
written.

The same helper runs as a hook, in the background so the chat never waits on it, after each Write,
Edit, MultiEdit or NotebookEdit, when a turn ends, and when a subagent starts or stops. From those it
keeps only the written file's path, and each background task's id, kind (shell or subagent), state and
subagent type. Commands, file contents, task descriptions, prompts and replies are skipped. Hooks
add to any you have; they never replace yours.

If you have a status line of your own, it still runs and shows: Cerberus's command passes the same
JSON on to it unchanged, and keeps its `padding` and `refreshInterval`. On Windows without Git Bash,
Claude Code runs status lines in PowerShell; there Cerberus leaves your status line alone, and the
Context card says the chat can't report.

**Warn when an account passes N% of its 5-hour limit** (Accounts, under Claude tabs; 90% by
default) shows a popup over the tab that crossed it: "work is at 87%, resets 6:40 pm". It comes
once per account per 5-hour window. When that tab isn't on screen, the popup waits for it and,
with desktop alerts on, a notification says so. Like the limit popup below, it has one **Move to
<account>** button for each signed-in account below the line, the first as the default, and
**Not now**. An account's button moves the chat in one click; nothing moves without it. **Not now**
leaves a small bar over the chat: **Show** opens the popup again, and **Dismiss** hides it until
the next window.

The **Live cards** switch, in the same place, turns all of it off; Claude tabs started after that
get no `--settings` and start as before.

### The file pane

A click on a file in Outputs shows it beside the chat, in the right half where a split's second tab
or a service's output goes; the line between them drags to resize. The file is read only, with
colours for TypeScript, JavaScript, Rust, Python, JSON, Markdown, CSS and HTML, and plain text
otherwise.

- **Diff** shows the change against the file's last commit (git `HEAD`) inline: removed lines in
  red above the added ones in green. A file outside a repo, or one git doesn't track, shows as all
  added.
- The pane reloads when the file changes on disk, about a second after the writes stop, and keeps
  your place in it.
- **Follow** moves the pane to each new file the chat writes.
- **Close**, or Escape while the pane has focus, closes it. Switching tabs, a split of two tabs or
  a service's output closes it too.

A file over 2 MB or a binary file isn't shown; the pane says so and offers **Open in VS Code**.
Claude Code's login token (`.credentials.json`) and prompt history (`history.jsonl`) are never
read, even through a link. The
pane reads only files that chat's own hooks reported writing, and the text stays in the window:
it is never logged or saved.

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

## Finding a tab or a chat

`claude --resume` lists only the chats of the folder you are in, on the account you are on. The
search box at the top of the sidebar (or Ctrl+Shift+F, Cmd+Shift+F on macOS) opens one list for
your open tabs and every account's chats:

- **Open tabs** come first, best match first: each word you type must appear, letters in order
  (`cerpl` finds Cerberus Planner), in the tab's title, project, folder name or account. The matched
  letters are highlighted. With nothing typed, all tabs show. Enter or a click shows that tab.
- **Chats** follow. A chat already open in a tab is listed once, as that tab. Ctrl+K is left to the
  shell, where it deletes to the end of the line.
- Before you type, it lists every chat, newest first. Each row has the chat's title (its summary,
  else its first message), its account, its folder, and when its last message was.
- A chat's title and folder name match the same way as tabs, letters in order. These chats come
  first, best match first.
- Then chats whose text holds **every** word you typed, in any order and case: in the title, the
  folder or what you and Claude wrote. Tool calls, their output and Claude's thinking are not
  searched. A chat whose title or folder holds every word comes before one where they are only in
  its messages, an exact match before a typo, then the newest.
- Only when no chat holds every word, **Close matches** lists the chats holding most of them,
  under their own heading.
- One letter filters the open tabs only; chats are searched from two letters on.
- A word of five or more letters and digits also finds a word one typo away: one letter added,
  missing, changed, or two neighbours swapped, so `authentcation` finds `authentication`. Shorter
  words, and words with other characters such as a path, must appear as typed. An exact match ranks
  above a typo.
- A row says where it matched: "title" or "folder" beside the title, or a line around the match
  in its messages, with your words highlighted, typos included.
- Paste a chat's id, alone or as `claude --resume <id>`, and the list shows just that chat on
  whichever account has it, so a resume that failed in a shell tab is one paste away.
- Arrow keys move, Enter (or a click) opens, Escape closes.
- **Opening** a chat that is already in a tab switches to that tab. Otherwise a new Claude tab starts
  on the chat's own account, in its recorded folder, inside the project that holds that folder (or a
  new one), and resumes the chat. If the folder is gone, it says so and offers **Choose a folder**,
  which copies the chat to the folder you pick (as Edit environment's move does) and resumes it there.

The search reads each account's `projects` folder on a background thread, and results come in as
they are found; each new keystroke stops the last search. Nothing it reads is written anywhere,
logged or kept: it lives in memory while the dialog is open and is dropped when it closes.
`history.jsonl` is never read.

## Moving a chat to another account

Right-click a Claude Code tab and pick **Move to `<account>`…** to move its chat to another
of your accounts, for example to keep work and personal chats apart. If the chat is running,
Cerberus asks before closing it, and the row says "Moving to `<account>`…" until it lands. Closing
it also ends what the chat left running, such as its background shells, as Claude Code does on a
normal exit; only processes started by that chat are ended. The limit and low-limit popups'
**Move to** run the same steps. The chat resumes in the same tab, in its place and split, and the
terminal keeps what it showed, scrolled where it was, until the resumed chat draws.

On a tab whose chat ended (now a shell), the item reads **Move the ended chat to
`<account>`…**: it moves that tab's ended chat, which is the last `claude` you ran in its shell if
that chat was saved, and asks first since the move closes the shell. While a `claude` you typed
in the shell runs, the tab offers **Move to `<account>`…** for that chat, as any running chat.

Cerberus copies the chat's file, and its folder of subagent chats, into the other account's
folder under a temporary name, renaming it once complete, then reopens the tab there with
`claude --resume`. The original stays on the first account. When the other account already has an
older copy, it is kept as `<id>.jsonl.cerberus-old`, never deleted. It refuses when the other
account is signed out or has that chat open right now. On any failure the tab stays where it was
and a popup says why; in the rare case that putting the older copy back fails too, the message says part of
it stays under a `.cerberus-old` name. The copy moves files only; Cerberus never reads what is in
them.

### When a chat hits its limit

When a chat hits its usage limit, from Claude Code's limit message or its 5-hour meter at 100%, a
popup says so ("Work is out until 7:18 am"), once per chat per limit window, with a desktop
notification when the tab isn't shown. Under "Move planner to:" it has one button per signed-in
account below the warning line, the first one as the default, and **Not now**, which leaves the tab
as it is. An account's button runs the same move as above, without a second confirm (the popup's
click is it). Once the moved chat is
ready, `continue` is typed into it for you to send, so Claude carries on with the request that hit
the limit. A move from the low-limit popup types it too; a move from the tab's menu doesn't.

A move between accounts of two organizations adds "Other organization: Claude rereads the chat,
so it starts slower." to its confirm and to the limit popup: Claude can't reuse the chat's earlier
thinking there. Moving back restores it.

Cerberus never switches accounts by itself: using several accounts to get around usage limits
automatically is against Anthropic's Usage Policy, so every move waits for your click.

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
  bar, and its tooltip says why: Claude Code older than 2.1.317, a Claude Code that reports no inbox,
  or macOS, where inbox delivery isn't checked yet. A newer Claude Code is tried directly; a message
  it doesn't show in time falls back to **Type it in instead**. "messages off" means the tab's helper
  didn't start within 10 seconds, or ended.
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

## Services

A service is a background program Cerberus keeps running for you, such as a cloudflared tunnel or
a dev server, without a tab of its own. They are listed under **Services** at the foot of the
sidebar; click the title to fold the list (kept across restarts).

- **+** adds one: a name, the command, a folder (your home folder if none), a shell (the Shell
  setting's unless you pick one), **Start with Cerberus** and **Restart if it stops** (on by
  default). **Start from** fills the form: **cloudflared tunnel** asks for a local port and fills
  `cloudflared tunnel --url http://localhost:<port>`; **Dev server** fills `npm run dev`.
- Each row has a dot for its state: green running, amber starting or restarting, red crashed,
  hollow stopped. Its **⋯** menu has Start or Stop, Restart, Show output, Edit and Remove (which
  asks first, and stops it).
- **The address**: for a tunnel, the first `https://….trycloudflare.com` it prints; for anything
  else, the first `http://localhost:<port>` or `http://127.0.0.1:<port>`. Click it to open it in
  the browser, or **Copy** it. A restart finds it again.
- **Show output** (or a click on the name) shows what it printed beside the tab on screen, read
  only. The last ~5,000 lines are kept in memory only: never written to disk or logs, and gone when
  Cerberus closes.
- **Restart if it stops**: after an exit you didn't ask for, it starts again in 1 s, then 2, 4, up
  to 30 s. Five exits within two minutes mark it **crashed**, with a **Restart** button, and it
  isn't tried again until you click.

A service runs in a hidden terminal, in the chosen shell and folder, with the same `PATH` a shell
tab gets. Stop ends everything it started, not just its shell (its own job object on Windows, its
process group on macOS). **Closing Cerberus stops every service**, as it ends tabs; the ones marked
**Start with Cerberus** start again the next time it opens. Services are kept in `services.json`
in the app's data folder, the commands with them, so a token in a command is stored there as a
tab's variables are in `tree.db`; it never appears in the list, logs or error messages.

## `cerberus open`

The project matches by name, then the same name in any case, then its folder's name; the tab by
title the same way. Words after `open` are joined, so `cerberus open my app/planner` needs no
quotes. A running Cerberus answers from its open window; otherwise the saved file is read. A
match by folder name says so: `Opening alpha (its folder is cerberus)`. The command talks only to
the installed Cerberus, never to a build with another identifier. `cerberus open` with no name
prints the usage and each project with its tab count, and exits with 2.

Exit codes: 0 opened, 1 no match or more than one (it lists them), 2 bad usage. Only project
names, folders and tab titles are printed, never chat content. With no arguments it starts the
app or brings it forward.

`cerberus` is a small console command beside the app, so a shell waits for it and sees what it
prints. The Windows installer adds it as `cerberus.com` in `%LOCALAPPDATA%\Cerberus`, on your
`PATH`. On macOS it is `Cerberus.app/Contents/MacOS/cerberus-cli`: **Install `cerberus` command**
in **Accounts** links it as `~/.local/bin/cerberus`, replacing an older link but never a file of
your own. If `~/.local/bin` isn't on your login shell's `PATH`, it shows the line to add to your
shell's startup file; Cerberus never edits that file itself. It refuses while Cerberus runs from
the disk image. On Linux the .deb puts it in `/usr/bin/cerberus-cli`, and the same button links it
as `~/.local/bin/cerberus`; it refuses from an AppImage. In a dev build the command is
`src-tauri/target/debug/cerberus-cli`.

## Programs and environment

| Program | What runs |
|---|---|
| Claude Code | `claude` from `PATH` |
| Antigravity CLI, Codex CLI | `agy` from its install folder or `PATH`, `codex` from `PATH`, listed only when found |
| PowerShell (Windows) | `pwsh` from `PATH`, else Windows PowerShell |
| Git Bash (Windows) | `bin\bash.exe` under Git's install folder |
| Shells (macOS) | Your login shell first, then `/bin/zsh`, `/bin/bash` and Homebrew's `fish`, then each other installed shell in `/etc/shells`, each with `-l` |
| Shells (Linux) | The same list. A shell reached by two paths (`/bin/bash` and `/usr/bin/bash`) is listed once, and `tmux`, `screen`, `rbash` and `nologin` are left out |

A tab under an account other than the default gets `CLAUDE_CONFIG_DIR` set to its folder, so
`claude` there, or typed in a shell tab, runs on that account. The variables a Claude Code session
sets for its own shells (`CLAUDECODE`, `CLAUDE_CODE_SESSION_ID` and the like, with their
`NO_COLOR`) are removed, so a tab runs the same however Cerberus was started. Settings such as
`CLAUDE_CODE_USE_BEDROCK`, and a `NO_COLOR` of your own, stay.

Selecting text copies it. Ctrl+V pastes text (Cmd+V on macOS). In Claude Code, Alt+V or Ctrl+V
pastes an image (Cmd+V or Ctrl+V on macOS, Ctrl+V on Linux).

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

## Linux notes

The Linux build is a preview: built and tested in CI on Ubuntu, and run on Ubuntu 26.04 under
WSLg, but not yet on a full desktop.

- Keys are the Windows ones (Ctrl+Shift+T, Ctrl+Shift+W and so on); there is no Mac remapping.
- Opened from a desktop launcher, an app may get a short `PATH`. As on macOS, Cerberus reads `PATH`
  from your login shell once at start (bash when `SHELL` is unset).
- The terminal font is DejaVu Sans Mono, falling back to Ubuntu Mono and Liberation Mono.
- **Install `cerberus` command** in **Accounts** links `~/.local/bin/cerberus` to the .deb's
  `/usr/bin/cerberus-cli`. An AppImage can't link it, since its files are gone once it closes.
- A tab's processes are read from `/proc`. Messages between accounts use a socket in your
  session's runtime folder (`$XDG_RUNTIME_DIR`), never the shared `/tmp`, and every socket is
  yours alone (0600).
- As on macOS, a tab's programs get a hangup when its terminal closes, a crash included.

## Where data lives

| | Windows | macOS | Linux |
|---|---|---|---|
| Accounts and settings (`profiles.json`) | `%APPDATA%\com.cerberus.terminal` | `~/Library/Application Support/com.cerberus.terminal` | `~/.config/com.cerberus.terminal` |
| Saved tabs (`tree.db`) | same folder | same folder | same folder |
| Services (`services.json`) | same folder | same folder | same folder |
| zsh suggestion files (`shell/zsh`) | same folder | same folder | `~/.local/share/com.cerberus.terminal` |
| MCP config for messages between accounts (`mcp.json`) | same folder | same folder | `~/.local/share/com.cerberus.terminal` |
| Status line settings for live cards (`live/tab-<id>.json`) | same folder | same folder | `~/.local/share/com.cerberus.terminal` |

Uninstalling on Windows keeps this folder unless you tick the box to delete the app data. An
unreadable `profiles.json` is kept as `profiles.json.bad` and the app offers to start fresh.

## Building from source

The README's "Build from source" has the commands. The app installs its JS packages with pnpm
(`pnpm install --frozen-lockfile`), which runs no package's install script. CI runs `cargo audit`,
where any vulnerability fails the build, and `pnpm audit --prod --audit-level high`, which checks the
shipped JS packages only and fails at high severity or above. Accepted upstream warnings are in
`src-tauri/.cargo/audit.toml`.
