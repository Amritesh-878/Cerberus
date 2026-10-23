# Cerberus

A fast Windows terminal built around Claude Code. One window holds every project as a tree: a
planner tab per project with coder sub-tabs under it, each tab tied to a folder, a Claude account
and a saved session, so a crash or a closed window never loses a chat.

Each tab runs the official `claude` CLI under its account's `CLAUDE_CONFIG_DIR`, so three accounts
run side by side and a chat can continue on another account. Claude Code is the only AI in it.

Built by Amritesh with Claude (Anthropic) as co-author: commits Claude helped write carry a
`Co-Authored-By: Claude` trailer.

**Status:** planning. No app code yet.

## Where things are

| Path | What it is |
|---|---|
| `AGENTS.md` | Project context and the rules every agent session follows |
| `coding_standards.md` | How code, commits and PRs are written and reviewed |
| `.claude/` | The skills, agents and settings Claude Code loads in this repo |

The agent workflow comes from the `prompts` template, kept as the `template` remote.

## License

MIT, see `LICENSE`. Skills vendored under `.claude/skills/` keep their own MIT notices in
`.claude/skills/notice.md`.
