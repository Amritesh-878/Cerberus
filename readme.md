# Cerberus

A fast Windows terminal built around Claude Code. One window holds every project as a tree: a
planner tab per project with coder sub-tabs under it, each tab tied to a folder, a Claude account
and a saved session, so a crash or a closed window never loses a chat.

Each tab runs the official `claude` CLI under its account's `CLAUDE_CONFIG_DIR`, so three accounts
run side by side and a chat can continue on another account. Claude Code is the only AI in it.

Built by Amritesh with Claude (Anthropic) as co-author: commits Claude helped write carry a
`Co-Authored-By: Claude` trailer.

**Status:** skeleton. The app opens one empty window; terminal tabs come next.

## Stack

Tauri 2 with a Rust core (PTYs, sessions, accounts, persistence) and a Solid + TypeScript webview
built with Vite. Terminals will use xterm.js and the file pane CodeMirror.

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

The agent workflow comes from the `prompts` template, kept as the `template` remote.

## License

MIT, see `LICENSE`. Skills vendored under `.claude/skills/` keep their own MIT notices in
`.claude/skills/notice.md`.
