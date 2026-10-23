---
name: verify-quality
description: Run the project's lint, typecheck, and test pipeline in that mandatory order after any code change. Lint with autofix first, then typecheck, then tests. 0 errors, 0 warnings, and 100% passing is the only acceptable result.
---

Run the quality pipeline in order. Never run tests before lint and typecheck pass clean.

## Step 0 — confirm this skill is configured (do this first, every time)

**This file ships as a template with placeholder commands.** Before running anything,
check that the commands in "The pipeline" below match this project: the paths exist and
the tools appear in the manifest (`package.json`, `pyproject.toml`, `Makefile`, …).

If they do not match, **stop and report this, do not guess a substitute**:

> verify-quality is not configured for this project. The commands in
> `.claude/skills/verify-quality/SKILL.md` are template defaults and do not match this
> repo. See SETUP.md step 3.

Then offer to fill it in from the real manifest. A quality gate that silently runs the
wrong commands, or nothing at all, reports a pass it never earned, which is worse than
having no gate. Do not proceed to Step 1 until the commands are real.

Delete this Step 0 section once the pipeline below is filled in for the project.

## The pipeline

<!-- SETUP AGENT: replace everything below with this project's real commands, in this
     order. Read the manifest for actual script names; do not invent them. Delete the
     language blocks that do not apply. -->

### Python

```sh
ruff check --fix .   # auto-fix lint, then confirm 0 errors remain
mypy .               # typecheck, must be 0 errors
pytest               # tests, must be 100% passing
```

Run `ruff check --fix .` first and confirm 0 errors remain before moving to `mypy`.

### JavaScript / TypeScript

```sh
npm run lint:fix     # ESLint + Prettier, not `npm run lint`
npm run typecheck
npm test
```

Use `lint:fix` rather than `lint` so autofixable problems are corrected, not just
reported.

## Acceptable results

- Lint and typecheck: 0 errors, 0 warnings.
- Tests: 100% passing.

**Name the interpreter or toolchain when reporting these.** A tool resolved from `PATH` rather than
the project's own environment reports a clean repo as broken, and the report reads identical either
way.

If a check fails, fix the underlying cause. Do not comment out code, skip or delete
tests, or disable rules project-wide to make a check pass. If a rule genuinely must be
suppressed, suppress it inline on the specific line with a justification comment.
