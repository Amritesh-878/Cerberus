---
name: verify-quality
description: Run the project's lint, typecheck, and test pipeline in that mandatory order after any code change. Lint with autofix first, then typecheck, then tests. 0 errors, 0 warnings, and 100% passing is the only acceptable result.
---

Run the quality pipeline in order. Never run tests before lint and typecheck pass clean.

## The pipeline

Run both halves; a change to either side can break the other (Tauri commands, shared config).

### TypeScript (repo root)

Install from the lockfile first if `node_modules/` is missing or stale: `npm ci`, never
`npm install`, which on Windows rewrites `package-lock.json`.

```sh
npm run lint:fix     # ESLint --fix, then Prettier --write
npm run typecheck    # tsc --noEmit for src/ and for vite.config.ts
npm test             # vitest run
```

### Rust (`src-tauri/`)

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

Run each half in its order: lint, then typecheck (`clippy` is both for Rust), then tests. Confirm
0 errors and 0 warnings before the next step.

## Acceptable results

- Lint and typecheck: 0 errors, 0 warnings.
- Tests: 100% passing.

**Name the toolchain when reporting these:** Node and npm versions, and `cargo -V`. A tool
resolved from `PATH` rather than the project's own `node_modules/.bin` or the pinned toolchain
reports a clean repo as broken, and the report reads identical either way.

If a check fails, fix the underlying cause. Do not comment out code, skip or delete
tests, or disable rules project-wide to make a check pass. If a rule genuinely must be
suppressed, suppress it inline on the specific line with a justification comment.
