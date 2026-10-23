---
name: subagent-context-management
description: Use this skill when delegating work to sub-agents, and when deciding whether to delegate at all. Defines what sub-agents should and should not return to avoid context overflow, when they are worth the overhead, and how to manage context working solo with none. Apply this before every sub-agent call and when working alone in one long session.
---

# Sub-Agent Context Management

## Language Protocol

**All communication between orchestrator and sub-agents must be terse, instruction-dense English.**

### Rules

- No preamble, no pleasantries, no restating the task
- Omit filler: articles, conjunctions, and transitional phrases where meaning is unambiguous
- Lead with the verb
- Specify exactly what to return — format, fields, nothing extra

## ⚠️ The Core Rule

**Sub-agents must return summaries, not dumps.**

Context overflow silently degrades output quality. The model starts forgetting earlier instructions. Requesting full file contents is the most common cause.

## What sub-agents MUST return

✅ Specific line ranges with direct relevance to your task (not entire files)
✅ API signatures and interface declarations (not full implementations)
✅ File paths + class names for dependency lists (not the classes themselves)
✅ Violation summaries with `file:line` references (not surrounding code)
✅ Targeted snippets: 5–15 lines max per finding
✅ Flow descriptions in plain text (not code reconstructions)

## What sub-agents must NEVER return

❌ Full file contents
❌ Entire class implementations
❌ Large code blocks copy-pasted for "context"
❌ "All code related to X"
❌ Anything that could be replaced with a file path + one-sentence description

## Example prompts

**❌ BAD — will overflow context:**

```
"Find all code related to order processing and return it"
"Scan src/ and return their contents for audit"
```

**✅ GOOD — targeted research:**

```
"Research order processing for TASK-005. Return:
1. Classes involved in processing (names + file paths only)
2. Public API of OrderService (signatures only, no implementations)
3. Where checkout timing is controlled (file:line + 3 lines of context)
4. Integration points with the payment system (brief description)
5. Existing test file paths + test names only

DO NOT return full implementations."
```

```
"Search src/ for AGENTS.md error-handling violations. Return:
1. Files with catch-all/silent failures (paths only)
2. Each violation: file:line + 3 lines of context
3. Violation count by severity (HIGH/MEDIUM/LOW)
4. Most common violation types (summary only)

DO NOT return full file contents."
```

## When to use sub-agents

✅ Auditing or exploring > 20 files
✅ Tracing a pattern or dependency across a large codebase
✅ Task touches > 10 files and you need integration point mapping
✅ Architecture is unfamiliar and requires exploration before implementation
✅ Need to find all call sites of a method across the full project

## When NOT to use sub-agents

❌ < 10 files (read them yourself — the overhead isn't worth it)
❌ You already know which files are affected
❌ Task plan includes specific file references
❌ Simple implementation following an existing, understood pattern
❌ You're doing detailed analysis of violations you've already found

## Working solo, with no sub-agents

Most of this skill assumes you are delegating. Often you are not — one session, one repo,
nobody to hand anything to. **The rules invert, and the inversion matters more than the
delegation rules do, because this is the common case.**

With a sub-agent, reading is cheap to you: it absorbs the tokens and hands back a summary.
Solo, you pay for every line you read, in the same window you need for the actual work, and
what you spend on reading you do not get back.

**So prefer executing a check over reading code and reasoning about it.**

Reading a function to work out what it returns costs the whole function and gives you a
belief. Running it costs one line and gives you the answer. The same trade shows up
constantly:

| Question | Don't | Do |
| -------- | ----- | -- |
| What does this return for that input? | Read the implementation and trace it | Call it and print the result |
| Is this still referenced? | Read the modules that might import it | `rg` for the symbol and count |
| Does this break anything? | Read the dependents | Run their tests |
| What shape is this data? | Read the parser and the schema | Print one record |
| Is the file the size I think? | Read it | `wc -l`, then read the part you need |

The habit generalizes: **when a question has a command that answers it, run the command.**
An executed check is also better evidence than a derived belief — it cannot be confidently
wrong the way a trace through unfamiliar code can, and it does not quietly depend on an
assumption you did not notice making.

What survives from the delegation rules: **still spawn a sub-agent for a genuinely wide
sweep** — auditing 20+ files, tracing a pattern across an unfamiliar codebase, finding every
call site. That is the case where the reading really is large and a summary really is
enough, and it is the one place solo work should still delegate.

And when you do read something expensive and learn something durable from it, write the
conclusion to `plans/project_state.md` immediately. Solo, the context you spent to learn it
is gone at the next compaction; the file is what makes the spend survive.

## ⚠️ Parallel Agents and File Conflicts

**Parallel agents that write to overlapping files WILL clobber each other.**

When two agents run simultaneously without isolation:

- Agent B reads a file, Agent A writes it, Agent B writes its (stale) version → A's changes are lost
- No error is thrown — the last writer wins silently

### Rule: Use worktrees for parallel write agents — and only for those

A worktree is a cost, not a default. It buys isolation between concurrent writers and
nothing else. With one writer there is nothing to isolate, and you still pay: a second
checkout on disk, a second path a command can be run in by mistake, and a `git status` that
reads clean in the tree you are not standing in. Sequential work — one agent after another,
or one session after another — is not concurrency; branches already handle it.

So the trigger is **two or more agents writing at the same time**, not "this task is
important" or "this change is large." Before spawning them:

1. **Check for overlap**: If their file sets share even one file (including test helpers, shared services, package.json), use isolated worktrees.
2. **Use `isolation: "worktree"` on the Agent tool call**: This gives each agent its own branch/directory; the orchestrator merges results after both finish.
3. **Read-only agents** (Explore subagents, research-only) are safe to parallelize without worktrees.

### Rule: If worktrees weren't used, verify before committing

After parallel write agents complete without worktrees:

- Read every file both agents claimed to touch
- Check for missing changes — the last writer may have silently dropped the other's edits
- Run full quality gates (`lint:fix`, `tc`, `test`) before committing
- Fix gaps manually before making any commit

### Tell agents about concurrent peers

If an agent will run alongside another that touches shared infrastructure (test helpers, config, package.json), include in the prompt:

> "Another agent is concurrently editing [file list]. Do NOT touch those files. Or if they are working on the same files, make sure they are aware of it."

## Subagent Types

- **Research Sub-agents**: For codebase exploration, pattern tracing, and dependency mapping. They return summaries of findings. The Explore subagent is an example.
- **Bug-Fixing Sub-agents**: For targeted fixes across multiple files. They return specific line edits or file paths. The Bug Fixer subagent is an example.
    - For complex bugs, it is recommended to use a Bug Fixer Democracy, or BFD for short, where 3-5 Bug-Fixing Sub-agents should be invoke simultaneously with the same prompt, and then once all are done, another Bug-Fixing Sub-agent should be invoked to aggregate the results and determine the best fix based on the returned summaries. This approach can help mitigate individual sub-agent errors and provide a more robust solution.
- **Core Review Sub-agents**: For reviewing code changes before commit. They return feedback on potential issues, style violations, or logic errors. The Code Reviewer subagent is an example.
    - This one can also be used to help with refactoring tasks.
- **Planning Sub-agents**: For generating implementation plans or task breakdowns. They return structured plans or checklists. The Task Planner subagent is an example.
- **Execution Sub-agents**: For performing specific actions in the codebase, such as refactoring or applying fixes. They return success/failure status and any relevant output. The Task Implementer subagent is an example.
    - This one can also be used to help with refactoring tasks.
