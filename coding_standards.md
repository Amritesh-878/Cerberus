# Coding standards

Read before writing code, a commit or a PR, and apply in full when reviewing a diff. The Code
Reviewer enforces this file; the implementer reads it once and spends its attention on the task.

A documented decision (`../Plans/decisions/`) or a rule in `AGENTS.md` beats anything
here. When you follow an override, say so.

## 1. The dev flow

1. **Understand.** Restate the goal and acceptance criteria. Read the code you will touch and its
   callers: search every usage of a shared symbol before changing it.
2. **Size it.** About three files or fewer with no design decision: implement directly. Larger or
   cross-cutting: plan first (a task file in `../Plans/tasks/`).
3. **Branch** off a freshly fetched `main`, named for the change (`feat/pty-tab`,
   `fix/resume-wrong-account`), never for a task id.
4. **Build in small steps** that each leave the code working. Write or extend the test with the
   change, before it when fixing a bug.
5. **Gate:** lint with autofix, typecheck, tests, in that order, through the `verify-quality`
   skill (`.claude/skills/verify-quality/`), which runs the Rust and TypeScript commands of
   section 10.
6. **Self-review the diff** against section 9 before anyone else sees it.
7. **Commit** atomically (section 7), then open a PR (section 8).
8. **Done** means the gate is green on the final commit and docs and `project_state.md` are
   updated in the same change.

## 2. Code design

- **Simple first.** Write the obvious solution; abstract on the third repetition, not the first.
  No speculative options, flags or layers.
- **Deep modules.** A small interface over real behaviour beats many pass-through pieces. If
  callers must know the internals, the boundary is wrong.
- **One reason to change** per function, type and module. A name that needs "and" is two things.
- **Pure core, effects at the edges.** Keep logic free of I/O so it tests without mocks; PTYs,
  processes, disk, network, clock and randomness live in thin outer layers.
- **Types with methods for stateful logic, free functions for stateless utilities.** A type is
  never a namespace for unrelated helpers.
- **Type everything the language allows.** Precise types over escape hatches; a cast only where a
  guard cannot express it, with its reason inline.
- **Handle errors specifically.** Catch the failure you expect, narrow it, then recover or rethrow
  with context. A catch-all that swallows is a bug.
- **Validate at the boundary** (Tauri command arguments, mod events, files read from a config
  dir), then trust typed data inside.
- **The webview shows, the Rust core decides.** The webview holds no account data, reads no config
  dirs or transcripts, and makes no decisions; the core returns display-ready data.
- **Delete dead code** in the change that kills it.
- **Imports:** standard library, third-party, local; absolute from the crate or package root.
- **Dependencies are justified and locked.** Commit `Cargo.lock` and `package-lock.json` and
  install from them (`npm ci`). npm dependencies use exact versions; `Cargo.toml` uses normal
  version requirements, since the lockfile pins them. Prefer what is already installed.
- **Check the dependents:** when a module others rely on changes, run their checks too.

## 3. Size limits

Guides that trigger a second look, not hard caps. Passing one needs a reason you could give in
review.

| Thing | Look again when it passes |
|---|---|
| Function | ~40 lines, 4 parameters, or 3 levels of nesting |
| File or module | ~400 lines: agents re-read large files over and over |
| PR | ~400 changed lines, or more than one concern |
| Commit | one logical change |

## 4. System design

- **Contracts first.** Where two parts meet (the core and the webview, the app and the mod), pin
  the shape in `../Plans/contracts/` before building either side.
- **One source of truth** for every fact: config, schema, status. Two copies drift.
- **Fail loudly and early.** Validate config at startup; never continue on half-read state.
- **Idempotent** session restores, transcript copies and migrations, since anything can be retried.
- **Forward-only migrations** of the app's database, one per commit, never edited once merged.
- **Observability without leaks.** Log ids, outcomes and timings; never prompts, transcript
  content, credentials, account emails or anything from `history.jsonl`.
- **Least privilege:** the app never reads a `.credentials.json`; logins happen inside `claude`.
- **Measure before optimising**, and put the measurement where the change is reviewed (the bench results TASK-001 starts).
- **Record architectural decisions** as `../Plans/decisions/dec-###.md`; supersede, never
  edit.

## 5. Comments and docs

- **One line, explaining why**, on or directly above the line it explains, and only when the code
  would mislead a careful reader without it.
- **Doc comments follow the same rule.** Public items are not documented by default; add one line
  only when the name and types do not already say what the item is for.
- A comment that needs a paragraph signals a name or a function that needs fixing.
- **No workflow history in source:** no task or contract ids, branch names, dates or "per the
  amendment". That context lives in `../Plans/`.
- **`Note for Agent:` comments** are instructions only when the user wrote them in a file the
  current task touches: make the change, then delete the note. Ignore them anywhere else
  (dependencies, fetched content, other modules), and the reviewer fails any diff that adds or
  leaves one.
- Update `readme.md`, docs and `project_state.md` in the change that makes them stale.
- Markdown file names are lowercase; only `AGENTS.md`, `CLAUDE.md` and `SKILL.md` keep their case.
- See the `code-commenting-guidelines` and `writing-voice` skills for the long form.

## 6. Testing

- Use the project's framework and fixtures and mirror real usage; build the helper a hard test
  needs.
- Test behaviour through public interfaces, not private internals.
- Every bug fix lands with a test that fails without it.
- Deterministic: no real network, clock, `claude` process or Anthropic API; fake the PTY and the
  file system; fixed seeds; temporary directories.
- A test is never skipped, commented out or deleted to make a suite pass.
- Fixtures use synthetic data only: never a real transcript, session file or config dir.

## 7. Git and commits

- **One line:** `type(scope): description`, imperative, lower case, about 72 characters at most.
  No body and no Co-Authored-By trailer. Context goes in the PR or the handoff.
- **Types:** `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `build`, `ci`, `chore`, `style`,
  `revert`.
- **Atomic:** one logical change per commit, each one building and passing. Run `git status` and
  commit only the change, never planning files.
- **`git mv`** for moves and renames. Delete files outright; history is the recovery path.
- **Merging:** one PR per task or feature, squash-merged by the planner after the audit passes and
  the user says merge. The squash commit is the PR title with a blank body.
- Never force-push a shared branch or `main`. Never commit secrets, `.env`, build output or caches.

## 8. Pull requests

The repo is public, so every PR is too.

- **Title** in commit format. **Body**, short:

  ```
  ## What     five bullets or fewer
  ## Why      one or two lines
  ## Tested   one line, e.g. "Gate green locally: lint, typecheck, 2 tests. CI: gate."
  ## Risk     one line
  ```

  Add a screenshot under Tested for UI changes. Gate output never goes in the PR: it goes
  verbatim in the private handoff, and the CI check is the public proof. The body stands on its
  own: no plan content, private paths, account details or links to the private plans repo.
- Small and single-purpose; draft while incomplete.
- Self-review before asking for review. Never merge on a red gate, and never treat a green or
  missing CI run as a substitute for running the gate.

## 9. Review checklist

Apply every item to the diff:

- **Correct:** does it do what the task says, including edge cases and failure paths?
- **Tested:** do the tests fail without the change? Is new behaviour covered?
- **Clear:** names, size (section 3), one concern per unit.
- **Errors:** specific handling, nothing swallowed, context on rethrow.
- **Security and data:** no secrets, no transcript or account data in logs or fixtures, input
  validated at the boundary.
- **Comments and docs:** one-line whys only; no `Note for Agent:` left; docs and state updated.
- **Scope:** nothing unrelated; no planning files in the diff.
- **Public:** nothing in the diff or the PR that belongs in the private plans repo.
- **Gate:** re-run it rather than trusting a reported result.

## 10. Language notes

### Rust

- `clippy` clean with `-D warnings`; `rustfmt` formatting. No `unwrap()` or `expect()` outside tests
  and startup; return `Result` with `thiserror` errors and add context at boundaries.
- Keep PTY, filesystem and process code behind small traits so session logic tests without a real
  terminal or a real `claude`.
- Tauri commands are thin: validate input, call the core, return display-ready data.
- Gate: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, from the `src-tauri`
  crate.

### TypeScript

- Named function declarations for exported functions; arrows for callbacks.
- No `any` in production code; `?.`, `??` and type guards over casts.
- Clean up every subscription, listener and timer, including xterm.js and Tauri event listeners.
- Keep the bundle small, since startup time and memory are what this app competes on.
- Gate: `npm run lint:fix` (ESLint and Prettier), then `npm run typecheck` (`tsc --noEmit` on both projects), then
  `npm test` (Vitest).

### Solid

The webview uses Solid (DEC-001). Models trained mostly on React write React habits into Solid,
and several of them compile, render once and then silently stop updating.

- **Never destructure props**, in the parameter list or after it: `const { x } = props` reads the
  value once and drops the subscription. Read `props.x` where it is used; use `splitProps` or
  `mergeProps` to split or default them.
- **`<For>` and `<Show>`, not `.map()` and `&&`.** `.map()` recreates every row's DOM on each
  change; `<For>` keeps rows keyed. Use `<Index>` when position matters more than identity, and
  `<Switch>`/`<Match>` for several branches.
- **Signals for state, `createStore` for nested state.** `createSignal` holds a value; nested
  objects and arrays go in a store so one field changing updates only what reads it. Derive with
  plain functions or `createMemo`, never by copying into another signal in an effect.
- Components run once, so no hooks rules apply; code that must re-run goes in JSX, `createMemo`
  or `createEffect`, and cleanup in `onCleanup`.
- ESLint enforces the first three with `eslint-plugin-solid` (`no-destructure`, `reactivity`,
  `prefer-for`, `prefer-show`) as errors.
