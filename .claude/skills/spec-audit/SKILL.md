---
name: spec-audit
description: Audit written artifacts - plans, contracts, specs, READMEs, and any figure quoted in prose - for claims that were true when written and never checked since. Use any time, and always before a spec freezes, ships to another team, or is amended. Not for code, which verify-quality covers.
---

# Spec audit

`verify-quality` gates the code. Nothing gates the writing.

Lint, typecheck, and tests fail loudly on every change. Plans, contracts, specs, and the numbers
quoted in prose have no equivalent, so a defect there survives until someone acts on it. That is
the document half of a rule this template already states: **you cannot audit your own work**, and
re-reading your own spec checks it against the understanding that produced it.

## When to run it

Any time. It costs a few greps and a careful read, and nothing about it needs a milestone.

It is **mandatory** before a spec freezes, before anything is handed to another team or client,
before an interface or baseline is frozen, and after any governing document is amended, because
those are the points where a wrong document stops being cheap to fix.

The one thing to watch is auditing instead of building. If nothing froze, nothing shipped, and the
last pass found nothing, the next pass is probably avoidance.

## Read this before running anything

**The checks in this file are a floor, not the job.** A checklist finds what is on the checklist,
and everything on a checklist is something already thought of. The defects that cost the most are
the ones nobody had a name for yet, so a run that only executes section 2 has audited the things
already known to go wrong and nothing else.

Two consequences, and they are the whole method:

1. **Do the open pass in section 1 first.** Running the mechanical checks first anchors you: the
   open pass then degenerates into confirming what the greps already said. Open first, mechanical
   second, in that order every time.
2. **Every check here detects the same underlying thing.** A claim that was true when written, has
   no mechanism that re-verifies it, and someone will act on. That sentence is the generator. To
   find a check this file does not have, look for another place the project makes a claim of that
   shape. Section 3 is where you write it down.

**Greps find wrong text. They never find missing text.** No command in this file can detect a
constraint nobody wrote, an assumption never stated, or a decision made in chat and never recorded.
That gap is permanent and is most of what section 1 exists to cover.

## 1. The open pass, which comes first

Do not turn this into a list to tick. Each prompt is a way of getting outside what is already
known to be wrong, and the answers are supposed to be different every time.

- **Read one document straight through as the person who will act on it**, not as its author. Where
  would they do the wrong thing and be able to defend it from the text?
- **Name the single sentence that would be most expensive if wrong**, then go verify that one
  sentence. Expense is not the same as prominence, and this usually lands somewhere unremarkable.
- **Argue the other side.** What would a reviewer who wants to reject this open with? Write their
  strongest objection in full, not a strawman, then answer it or concede it.
- **Sample, do not sweep.** Pick three claims at random, including boring ones, and trace each to
  its source. Random sampling finds classes that no check can name, which is exactly the point.
- **Ask what is not written anywhere.** Which decision does the project depend on that lives only
  in someone's memory or in a chat log? Absence is invisible to every command in section 2.
- **Look for the thing that is true but no longer relevant**, and for the thing that changed
  without anyone editing the document that describes it.
- **Check the document against reality, not against another document.** Run the command it
  documents. Open the file it references. Compare the interface it specifies to the one that
  exists.

If a finding here has no matching check in section 2, that is the normal case, and section 3 says
what to do with it.

## 2. Standing checks

These are the classes already known to recur. Run them after section 1. Each is one command
because a check that depends on noticing something will not fire, and not noticing is the failure
mode.

**Each one ends with the defect it came from.** That is required, not decoration: a check whose
origin is recorded gets trusted, and one that reads as generic advice gets skipped. All five below
came out of a single research project, described here without its identifiers. Do not ship a sixth
without an origin line.

### A. Stale authority after an amendment

Amending a governing document silently invalidates every other document that restates it.

```sh
grep -rn "DEC-[0-9]\{3\}\|FEAT-[0-9]\{3\}" --include=*.md plans
```

Open each hit and compare it against the amended section. Look for a document calling a spec
"draft" after it locked; a restated procedure the amendment replaced, carrying no supersession
note; a version or count restated in a second place and left behind; and a supersession note
placed **after** the text it supersedes, so a reader hits the stale statement first. A supersession
note goes before the text it invalidates, every time.

*Origin: amending one contract turned up three stale restatements in one pass. A master plan
called that contract "draft" long after it locked, a second contract still specified schema 2
after schema 3 had shipped to an outside team, and an FAQ presented a statistical procedure the
amendment had replaced.*

### B. A figure in prose with no source behind it

Any number in a document traces to a named source. A figure appearing in two or more documents is
an **anchor**, and anchors get recomputed from the source rather than copied between documents.

```sh
grep -rh --include=*.md "" plans \
  | sed -E 's/[0-9]{4}-[0-9]{2}-[0-9]{2}//g' \
  | grep -oE "(^|[^A-Za-z0-9])([0-9]+\.[0-9]{2,4}|[0-9]{3,})" | sed -E 's/^[^0-9]+//' \
  | sort -u | while read -r n; do
  e=$(printf '%s' "$n" | sed 's/\./\\./g')
  c=$(grep -rlE "(^|[^0-9.-])$e([^0-9]|$)" --include=*.md plans | wc -l)
  [ "$c" -ge 1 ] && echo "$c $n"
done | sort -rn
```

**The boundaries here are deliberately asymmetric, and three earlier forms got it wrong.**

- **Strip dates, do not filter years.** An early form ended with `grep -vE "^(19|20)[0-9]{2}$"`
  to drop dates, and it silently ate `2050` - the most-cited token count in the project it was run
  against. Any measurement that happens to look like a year disappears. Removing ISO dates from the
  text *first* drops dates without judging four-digit numbers. The cost is that a bare year in
  prose is now a figure; that noise is worth one real measurement.
- **No boundary *after* the number.** `[0-9]{3,}\b` never matches `350ms`, `463ms` or `17GB`,
  because a letter is a word character, so the trailing boundary cannot fire. A unit suffix is
  exactly what a measurement looks like.
- **But a boundary *before* it.** Dropping both lets identifier digits in: a Windows codepage
  written `cp1252`, or a Dijkstra manuscript number written `EWD854`, each yield their digits as
  though they were measurements. Requiring no letter or digit immediately before the number keeps
  the unit suffixes and drops the identifiers.

*Note the shape of those two examples: they are written here in identifier form only, never as
bare numbers. A doc that quotes its own false positives as standalone figures reintroduces them
on the next run.*

*Measured on a real repo: the year-filter form found 40 distinct figures and missed six, of which
three were the `NNNms` durations this exists to catch. Dropping both boundaries found all six but
added two identifier artifacts. The asymmetric form above keeps the three real ones and drops both
artifacts. A check that cannot see a project's most-cited number reports clean, which is worse than
no check - and one that reports codepage numbers as measurements gets ignored, which is the same
outcome by a slower route.*

**Read the whole list, not just the top.** A count of 2 or more is an anchor: recompute it from the
source rather than copying it onward. A count of 1 is not exonerated, it is unexamined, and that is
where the expensive ones sit, because nothing contradicts a figure quoted once. This template shipped
two of them - the resident-token estimates in `SETUP.md` step 0 - and an anchors-only version of this
check could not have seen either, since each appeared in exactly one file.

**An anchor is a quantity, not a numeral.** Before recomputing anything, check what each hit
actually refers to. The same numeral in five files can be five different measurements, and the same
measurement can be split across two rows by precision alone - `0.875` and `0.8750` count separately,
so neither reaches its real total. Small denominators make this worse rather than rarer: 7 of 8 and
17 of 20 keep landing on the same exact fractions, so collisions cluster in exactly the
small-sample figures that most need re-deriving.

Three things in the command are load-bearing:

- **Integers, not just decimals - but only from three digits up.** The decimals-only form matched
  nothing at all in `SETUP.md`, the file that was carrying both stale figures. Most numbers in prose
  are counts, and counts are integers. Two-digit integers are a different story: on a real planning
  folder they return section numbers and date fragments and bury the signal, while the three-digit
  floor leaves the denominators standing.
- **The boundaries.** A plain substring count finds `1.50` inside `1.500` and `11.50`, so it invents
  anchors that do not exist, and a check that cries wolf is one nobody reruns. Not hypothetical: run
  against the originating project's documents, the substring form reported one figure in 6 files when
  5 contain it.
- **Dropping a count of 0.** Excluding a leading `-` from the count is what makes `TASK-014` and
  `2026-08-03` fall out on their own, so identifiers and dates never reach the list. Years are
  filtered outright.

For each, ask whether it is one realization of a quantity that varies. A single measurement quoted
as a settled value is the most expensive defect in this class, because it propagates: once four
documents cite it, the number reads as established and nobody re-derives it.

*Origin: one measured value quoted in four documents as the settled size of an effect. It was one
of two runs. The other run put the same effect at roughly a third of it, and run-to-run noise alone
exceeded the smaller figure. Four citations were what made it read as established.*

### C. Aggregates whose denominator can move

A rate or a mean is comparable across two measurements only if both cover the same population.
Denominators a run computes, rather than the design fixing, are the trap: filtered subsets,
"applicable cases", tiers, anything with a count beside it.

```sh
grep -rn "_count\|_total\|n=\|out of" --include=*.json --include=*.md bench plans
```

If two runs differ, differencing the two published values mixes real movement with a change in what
is being averaged. Restrict to the cases present on both sides and report the matched count next to
the difference.

*Origin: a metric averaged over a filtered subset covered 103 of 120 cases in one run and 102 in
the next, because one case changed category. Differencing the two published aggregates gave 0.0289
when the matched-item movement was 0.0245; the remaining 0.0044 was the denominator moving.*

### D. Present tense on work that never ran

Keep the list of procedures that have never executed under **Known-broken / accepted** in
`plans/project_state.md`, and grep for each by name. A list with no home is not kept, and this one
has to survive the session that wrote it. Validation steps, baselines, dry runs, and migrations are
the usual entries. "The suite runs nightly" claims an
operating procedure; "will run nightly, and has not yet" states a plan. Also flag comparative
phrasing that presumes a measurement exists: "the same as the baseline" asserts a baseline was
taken.

*Origin: a reader-facing explainer described a validation step as operating when it had never run,
in the one document the governing contract named by name as forbidden from doing that. A status
file compared a result to "the same as the baseline" when no baseline run existed.*

### E. Two documents by the same author that disagree

Documents written days apart contradict each other, and re-reading catches neither, because each
was correct when written.

```sh
grep -rn "<keyword>" --include=*.md plans | sed 's/:.*//' | sort -u
```

Read the hits side by side rather than in place. The pairs that matter state an **obligation**:
whether a change forces a rebuild, whether a result counts, whether something requires a version
bump.

*Origin: two documents by the same author, written hours apart, disagreed on whether re-pinning a
dependency obliged re-running the measurements taken against the old pin. Both were correct when
written, which is why re-reading either one alone caught nothing.*

## 3. Growing this file

**A defect class found once and not written down will be found again by someone else, later, at
more cost.** When the open pass turns up something section 2 could not have caught:

1. Add a check for it, with a command if one exists and a sharp question if it does not.
2. Say which real defect it came from in one line. A check whose origin is recorded gets trusted;
   a check that reads as generic advice gets skipped.
3. If the class cannot be reduced to a command, put it in section 1 as another prompt rather than
   forcing a grep that half-detects it.

Editing this file is part of running it, not a separate chore. A spec-audit that has not changed
across several projects is not stable, it is unused.

**Close the class, do not stop at the instance.** Having found one bad figure, enumerate every
figure. Having found one moved denominator, enumerate every aggregate. Recording "checked all of
them, this is the only one" is worth more than the fix, because it converts a lucky catch into a
bounded claim.

## 4. Model budget, if you fan this out to subagents

Fanning out is optional. The standing checks are cheaper to run yourself than to delegate, and
delegating them costs a full context spin-up per agent to re-derive what you already know.

When you do fan out:

- **Sonnet by default.** Section 2 is mechanical: run a command, read the hits, report `file:line`.
  Model capability does not change that output, and a fan-out of four sonnet agents costs a
  fraction of one opus agent.
- **Opus only for section 1**, where capability genuinely changes what gets found. An open
  adversarial pass is the one part worth the price.
- **Keep the opus fan-out small, and know your own limit before you widen it.** An agent that hits
  a spend limit **fails mid-run and returns nothing at all** — the work is lost rather than
  degraded, and the tokens it burned before dying are still spent. That failure mode is the durable
  part. The number of opus agents it takes to trigger depends on the plan and the month, so it is a
  per-project fact: record yours in `plans/project_state.md` under "Facts that bite" the first time
  you find it, rather than inheriting someone else's.
- **Verify what comes back before acting on it.** A reported finding is a claim, not a fact, whether
  it came from a subagent, an external reviewer, or a handoff — the rule the Task Auditor already
  follows when it re-runs the quality gate instead of trusting the handoff's pasted output. A
  subagent shares your model and your blind spots. Reproduce the finding before writing the fix; if
  it does not reproduce, the finding is the thing that is wrong.

## 5. What this skill does not cover

Code correctness. `verify-quality` owns that.
