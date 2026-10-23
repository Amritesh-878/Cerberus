---
name: Task Implementer
description: Implement a planned task following the TASK_PLAN_TEMPLATE structure. Research the task, clarify ambiguities with the user, implement with full test coverage, run the quality gate, and write handoff notes.
argument-hint: task=TASK-<number> allowBreaking?=<true|false>{default=false} info?=<additional info>
model: opus
---

# Implement a Planned Task

Arguments:

- `task=TASK-<number>` — Task to implement (e.g. TASK-005). If omitted, pick the next task from the master plan.
- `allowBreaking=<true|false>` — Whether breaking changes are approved (default false). Even when true, document them and update all call sites.
- `info=<additional info>` — Optional extra context.

You implement one task from the plans folder. Task files and the master plan live there;
each finished task gets its own handoff file in `plans/completed/<task-id>.md`. Workflow:
research → clarify → implement → verify → hand off.

## Asking the user questions

Claude Code takes conversational turns: when scope or an approach is ambiguous, ask
directly and wait. Use `AskUserQuestion` for discrete choices, chat for open-ended ones.
Do not poll files or sleep. Do not start implementing until the ambiguities are resolved.

## Read first

- `coding_standards.md` — engineering standards you must follow (MANDATORY); `AGENTS.md` for project context.
- `plans/master_plan.md` and the `task-xxx.md` you're implementing.
- Prior handoff notes from dependent tasks, for context.

## Delegating research to sub-agents

Follow the `subagent-context-management` skill before every sub-agent call. Delegate only
to map integration points or find call sites in large/unfamiliar areas, and require
targeted snippets and file paths — never full file dumps. For a localized task (1–3 files)
or a pattern you already understand, read the code yourself.

## Workflow

### 1. Research & clarify

Read the task and the code areas it touches. Resolve every ambiguity with the user before
writing code.

### 2. Implement

- Follow `coding_standards.md`, including the language notes for the stack, and any override in `AGENTS.md`.
- Keep changes scoped to the task; don't refactor unrelated code.
- After each meaningful change, run the `verify-quality` skill (lint → typecheck → tests,
  in that order). Fix the underlying cause of any failure; do not disable rules or comment
  out code or tests.

### 3. Breaking changes

Only if `allowBreaking=true`. Update every call site in the same task and document the
migration path in the handoff. The quality gate must pass before the task is complete.

### 4. Final quality gate (MANDATORY — never skip, never assume)

Run the `verify-quality` skill one final time on the finished work, even if you ran it
during step 2 and nothing has changed since. A task is not complete until this run passes:
**0 errors, 0 warnings, 100% of tests passing.**

**Capture the actual output.** You paste it into the handoff verbatim in step 5. Do not
summarize it, do not report a remembered result, and never write "tests pass" without the
run behind it — the audit gate downstream has nothing else to check you against.

If the gate fails and you cannot fix the cause within the task's scope, stop and report
that. A failing task reported honestly is worth more than a passing one reported falsely.

### 5. Hand off

- Copy `templates/handoff_template.md` to `plans/completed/<task-id>.md` and fill it in: the
  pasted gate output from step 4, what changed, decisions, breaking changes, and warnings
  for whoever picks this up next.
- **Do not write handoff prose into `master_plan.md`.** That file is a status board, and
  appending narrative to it is what turns a plan into something nobody reads. In it, only:
  flip the task to ✅ Completed, and link `plans/completed/<task-id>.md`.
- Note any work this task unblocks.
- Provide a Conventional Commits message, one line, no body and no AI-attribution trailer.

## Guidelines

- **Tests are mandatory** (see coding_standards.md). Write whatever fixtures, mocks, or helpers make
  a test mirror real usage. Do not skip a test because it's complex.
- **The quality gate is non-negotiable:** 0 errors, 0 warnings, 100% passing before a task
  is done, confirmed by a final run whose output goes in the handoff. If a rule must be
  suppressed, suppress it inline with a justification.
