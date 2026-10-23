---
name: Bug Fixer
description: Research and fix a bug — analyze the failure, confirm the root cause with the user, implement a minimal fix following coding_standards.md, add a regression test, verify, and document the resolution.
argument-hint: bug=<description-or-file> severity?=<critical|high|medium|low> info?=<additional info>
model: opus
---

# Fix a Bug

Arguments:

- `bug=<description-or-file>` — A description, error message, or path to a bug report (e.g. `plans/bugs/bug-001.md`).
- `severity?=<level>` — critical / high / medium / low (default high).
- `info?=<additional info>` — Reproduction steps, constraints.

You diagnose and fix a bug. Bug reports live in `plans/bugs/`; each finished fix gets its
own handoff file in `plans/completed/<bug-id>.md`. Workflow: analyze → confirm root cause → fix →
regression test → verify → document.

## Asking the user questions

Claude Code takes conversational turns: when you need reproduction details or to confirm
the root cause and fix approach, ask directly and wait. Use `AskUserQuestion` for discrete
choices, chat for open-ended ones. Do not poll files or sleep. Do not implement a fix
until the root cause is confirmed with the user.

## Read first

- `coding_standards.md` — engineering standards the fix must follow (MANDATORY); `AGENTS.md` for project context.
- The bug report file, if `bug=` is a path.

## Delegating research to sub-agents

Follow the `subagent-context-management` skill. Use sub-agents only when the bug spans
many files or a deep, unclear call stack; require a cleaned call chain (line numbers),
5–15-line snippets, and module names — never full file dumps. For a single-file bug or one
the error message already localizes, read the code yourself.

## Workflow

### 1. Analyze

Parse the error and stack trace to the exact origin. Form a root-cause hypothesis and note
the files/systems involved. Reproduce the bug (run the failing case) if you can.

### 2. Confirm

State the root cause, why it happens, the affected systems, the proposed fix, and its risk
(breaking? perf? test coverage). Get the user's agreement before changing code.

### 3. Fix

- Create the bug report `plans/bugs/bug-nnn-title.md` (status, severity, repro,
  expected vs actual, root cause, planned fix).
- Make the **minimal** change that fixes the root cause; don't refactor unrelated code.
  Note any architectural issues for a separate planning task instead.
- Follow `coding_standards.md`, including the stack's language notes, and any override in `AGENTS.md`.

### 4. Regression test

Add a test that fails before the fix and passes after, reproducing the exact bug
condition. Place it with the project's tests.

### 5. Final quality gate (MANDATORY — never skip, never assume)

Run the `verify-quality` skill (lint → typecheck → tests) on the finished fix, even if you
ran it earlier and nothing has changed since. Loop until 0 errors, 0 warnings, 100%
passing. Then confirm with the user that the original reproduction no longer triggers the
bug.

**Capture the actual output** — it goes into the handoff verbatim in step 6. Never report a
remembered or assumed result: the audit downstream has nothing else to check you against.

### 6. Document

Copy `templates/handoff_template.md` to `plans/completed/<bug-id>.md` and fill it in: the pasted
gate output, the root cause, the fix, the regression test added, and how the fix was
verified. Update the bug report to link it.

Provide a Conventional Commits `fix:` message, one line, no body and no AI-attribution
trailer. If the bug exposed deeper architectural problems, recommend a follow-up planning
task rather than widening this fix.

## Severity

- **Critical** — crash, data loss, security hole, or full outage.
- **High** — core functionality broken, or frequent.
- **Medium** — minor issue with a workaround, or infrequent.
- **Low** — cosmetic or edge case.

## Guidelines

- **Avoid breaking changes** unless required to fix the bug; if required, update all call
  sites in the same fix and document the migration. Get user approval first.
- **Every fix ships with a regression test** and a clean quality gate before it's done.
