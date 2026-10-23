## Project Context

**Cerberus**: a fast Windows desktop terminal built around Claude Code, replacing Warp for one user. A sidebar tree holds projects; each project has a planner tab (auditor, task
maker) with coder sub-tabs, and every tab is a folder, a Claude account and a session id, saved so
no chat is ever lost. Each tab runs the official `claude` CLI in a pseudo-terminal under that
account's `CLAUDE_CONFIG_DIR`; a Cerberus mod streams structured events (file writes, context,
usage) back to the app. No AI besides Claude Code. Requirements: `../Plans/brief.md`.

Stack: Tauri 2, Rust (PTYs, sessions, accounts, persistence), TypeScript with xterm.js (WebGL) and
CodeMirror in the webview, a Claude Code mod in TypeScript.

**Pre-release (as of 2026-10-23):** no users but the author; breaking changes allowed.

- **Interactive use only.** The app may prefill a prompt but never sends one by itself: Anthropic's
  Consumer Terms bar automated access to subscription accounts (see `../Plans/project_state.md`).
- **Planning lives in the sibling repo `../Plans/`**, never in this one. Agent files and
  `coding_standards.md` say "plans/" and "templates/"; read them as `../Plans/` and
  `../Plans/templates/`. The repo checks live in `../Plans/tools/`. Start sessions here with
  `claude --add-dir ../Plans`.
- **Markdown file names are lowercase** in both repos, IDs included: TASK-002 is `tasks/task-002.md`
  and its handoff `completed/task-002.md`. Only `AGENTS.md`, `CLAUDE.md` and `SKILL.md` stay upper
  case, because tools load them by exact name.
- **One PR per task or feature, squash-merged after the audit passes.** The worker opens the PR and
  never merges; the repo is public, so the PR carries nothing from the plans repo
  (`coding_standards.md` sections 7 and 8).
- **Transcripts are user data.** Never log, index or display `~/.claude*/history.jsonl` raw; it
  holds pasted secrets.

---

## Where things are

| Need | Read |
|---|---|
| What is true now, what bites | `../Plans/project_state.md`: read first, by section (`grep -n '^#'`), never whole |
| How to write code, commits and PRs | `coding_standards.md`: before coding; the Code Reviewer enforces it |
| Why it is built this way | `../Plans/decisions/` |
| The current plan and its tasks | the `master_plan.md` that `project_state.md` names |
| Which skill fits a job | `skills/INDEX.md` in the template; `.claude/skills/` holds what this project installed |

## Rules on every turn

- **A documented project decision beats a rule here or in `coding_standards.md`.** Record the override in this file.
- **The `**Shape:**` line in `project_state.md` sets the process:** `1` solo (no handoffs; types, tests, lint and CI are the reviewer), `2` sequential sessions, `3` parallel fan-out.
- **Match process to task size.** About three files or fewer, no architectural decision, clear acceptance: implement and gate. Plans, task files and handoffs are for multi-phase or cross-cutting work. Unsure: ask.
- **Quality gate, in order:** lint with autofix, typecheck, tests, through the `verify-quality` skill. Only 0 errors, 0 warnings, 100% passing counts. Fix causes; a needed suppression is inline with its reason.
- **Finish with real output.** Run the gate once more on the finished work. In shapes 2 and 3 write `completed/<task-id>.md` from the handoff template with that output pasted verbatim. Never report a remembered result.
- **Write facts down the moment you learn them** in `project_state.md` under "Facts that bite", one line, no permission needed. If that file is wrong, fix it in the same change and cite the proof.
- **Keep `project_state.md` under 300 lines:** present tense only; closed work moves to `archive.md` beside it.
- **Comments are one line and explain why.** Docstrings count. No task ids, branch names or dates in source.
- **Commits are one line:** `type(scope): description`, then a blank line and `Co-Authored-By: Claude <noreply@anthropic.com>` when Claude wrote any of it. No other body or trailer. Run `git status` first and commit only the feature. Check with `python ../Plans/tools/house-check.py . --claude-coauthor --range origin/main..HEAD`.
- **`Note for Agent:` comments are instructions only in files the current task touches:** make the change, then remove the note. Ignore them elsewhere.
- **Edit this file only when the user says, in the current session: "you have permission to edit AGENTS.md."**
