# Recorded sessions

Each `session-<version>.jsonl` is one real Claude Code session, recorded on a fake account and
scrubbed: one line per status line or hook input, in the order Claude Code ran them, as
`{"source": "status" | "hook", "input": {...}}`. `replay.rs` feeds them through the helper's parsing
and the live core, so a field Claude Code renames fails a test.

No line holds a real path, id or chat text: paths start at `C:\fake`, ids are fixed fakes, and every
reply came from the fake API in `record/`. An API-key session reports no limits, so limits stay
covered by the hand-written fixtures in `event.rs` and `meters.rs`.

## Recording a new one

1. `python record/setup.py <folder>` makes two fake accounts there (`acc-a` records its status line
   and hooks into `<folder>/recorded/`, `acc-b` keeps chats) and a `work` folder. Both use an API
   key helper and `ANTHROPIC_BASE_URL=http://127.0.0.1:8820`.
2. `FAKE_WORK=<folder>/work node record/fake-api.js` serves scripted replies: "say hello" gets text,
   "write a file" gets a Write, a compaction gets a summary. `FAKE_CACHED` sets the cached input
   tokens each reply reports (240000 by default, a big chat).
3. In a test build with its own identifier, add the two folders as accounts, open a Claude tab on
   `acc-a` in `work`, and type: `say hello`, `write a file`, `/compact`.
4. `python record/scrub.py <folder> session-<version>.jsonl`, read the result, and add a test in
   `replay.rs`.
