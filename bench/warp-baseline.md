# Warp baseline

Memory and CPU of Warp hosting Claude Code, taken with `bench/sample.ps1` so the same runs can be
repeated on Cerberus.

## Setup

| | |
|---|---|
| Machine | Intel Core Ultra 9 275HX (24 logical cores), 32 GB RAM, RTX 5090 Laptop GPU, two monitors |
| OS | Windows 11, Windows PowerShell 5.1 |
| Warp | v0.2026.10.21.08.25.stable_02 |
| Claude Code | 2.1.312 |
| Date | 2026-10-28 |

## Method

- `sample.ps1 -Name warp` takes one `Win32_Process` snapshot per interval and walks every process
  descended from `warp.exe`. CPU is the change in `KernelModeTime` plus `UserModeTime` over the
  interval, as a percent of all 24 logical cores (one full core is 4.2 %). Memory is
  `WorkingSetSize` (working set) and `PrivatePageCount` (private bytes, in bytes despite the name).
- **App only** is every `warp.exe`, plus any `OpenConsole.exe` or `conhost.exe` whose parent is in
  the app group: the pseudo-console hosts Warp starts, one per tab. **Children** is everything else
  in the tree: shells, `claude`, `node` and what they start, summed per process name.
- CPU used by a process that exits between two snapshots is not counted, because `Win32_Process`
  cannot report a process after it exits. Short-lived processes are undercounted, more so at the
  workday's 10 s interval than at 1 s.
- Controlled states (a) to (d) are 60 s at a 1 s interval. The workday (e) is 4 h at 10 s.
- The sampler ran from a terminal outside Warp, so it is not in the measured tree.
- Units are MiB. Rows give the median; CPU also gives p95 and max.

### Running it

```powershell
.\bench\sample.ps1 -Name warp -Seconds 60 -Csv bench\raw\state-a.csv
.\bench\sample.ps1 -Name warp -Seconds 14400 -Interval 10 -Csv bench\raw\workday.csv -Quiet
.\bench\sample.ps1 -FromCsv bench\raw\workday.csv
Import-Module Pester -RequiredVersion 3.4.0; Invoke-Pester bench\core.tests.ps1
```

Run the sampler from a terminal outside the app under test. `-Csv` refuses an existing file, and
`-Exclude <pid>` leaves a subtree out. The tests use Pester 3.4.0, which ships with Windows
PowerShell 5.1; Pester 5 does not accept their assertion syntax.

### Check against Task Manager

State (a), same minute, both `warp.exe` processes summed:

| Source | Working set | Private bytes |
|---|---|---|
| Task Manager, Details tab: Working set (memory), Commit size | 850.7 | 821.2 |
| `sample.ps1`, 60 s median: `WorkingSetSize`, `PrivatePageCount` | 842.6 | 822.8 |
| `Get-Process` after the run: `WorkingSet64`, `PrivateMemorySize64` | 839.2 | 822.8 |

The sampler is within 1 % of Task Manager on both. `Get-Process`, read a minute later, is 1.4 %
lower on working set and within 1 % on private bytes. Task Manager showed CPU 00; the sampler median
was 0.1 %.

### Departures from the task

- The controlled states start from Warp with its restored session (26 idle PowerShell tabs, no
  `claude`), not one empty tab. Turning off session restore would have discarded the user's tabs.
  States (b) to (d) add `claude` sessions on top, so the change from (a) is the cost of a session.
  To compare, open Cerberus with the same number of idle shell tabs.
- Claude Code 2.1.312 shows a spinner while it writes and prints the whole answer when done, so
  there is no streamed text to render. (d) is split into the generating minute and the two minutes
  in which a ~4,000-word answer landed.
- Input latency was not measured (no Typometer).

## Results

### Controlled states

| State | App CPU % med / p95 / max | App WS | App private | Children WS | Children private | Total CPU % med / max | Total WS |
|---|---|---|---|---|---|---|---|
| (a) 26 idle tabs | 0.0 / 0.5 / 1.6 | 1,262 | 899 | 3,350 | 2,702 | 0.0 / 11.0 | 4,612 |
| (b) (a) + 1 idle `claude` | 0.1 / 0.3 / 1.5 | 1,265 | 901 | 3,936 | 3,523 | 0.1 / 9.0 | 5,201 |
| (c) (a) + 3 idle `claude` | 0.0 / 0.6 / 0.9 | 1,305 | 956 | 5,332 | 5,352 | 0.1 / 19.2 | 6,637 |
| (d) 1 of 3 generating | 0.2 / 1.0 / 1.9 | 1,305 | 955 | 5,346 | 5,379 | 0.4 / 15.4 | 6,650 |
| (d) answer lands | 0.1 / 0.8 / 3.7 | 1,305 | 957 | 5,352 | 5,425 | 0.1 / 17.6 | 6,657 |

- An idle `claude` session costs Warp itself about 3 MiB of working set; (c) adds 43 MiB, of which 33
  MiB are the two extra tabs' `OpenConsole.exe` hosts. One idle session's own processes (`claude`,
  three `node`, `cmd`, `conhost`) take about 590 MiB.
- Warp's busiest second in (d) was 3.7 % of the machine, about 0.9 of a core, when the answer was
  printed.

### Workday

(e) Normal work in Warp, 10:04 to 14:14, 1,440 samples: about 25 tabs, a median of 6 `claude`
sessions, builds and git. That run's sampler slept 10 s after each ~0.4 s snapshot query, so 1,440
samples spanned 4 h 10 min, not 4 h; the sampler now keeps a fixed schedule.

| Group | CPU % med / p95 / max | WS med / p95 / max | Private med / p95 / max |
|---|---|---|---|
| App only | 0.2 / 1.9 / 3.0 | 680 / 759 / 777 | 954 / 1,198 / 2,263 |
| Children | 0.3 / 4.4 / 22.7 | 4,417 / 9,451 / 13,451 | 8,472 / 16,869 / 20,470 |
| Total | 0.5 / 5.8 / 24.3 | 5,080 / 10,162 / 14,170 | 9,445 / 17,827 / 22,265 |

Windows trims working sets under memory pressure: app working set was 680 MiB over the workday and
1,262 MiB after a fresh start with the same tabs. Private bytes held at 899 to 956 MiB in both, so
compare Cerberus on private bytes first.

### Warp's git probes

About once a minute `warp.exe` starts one `powershell -NoProfile -c "git symbolic-ref ..."` per tab
that sits in a git repository (19 at once here) to refresh branch names. Each brings a `conhost.exe`
and a `git.exe`. A burst takes up to 17 % of the machine (about 4 cores) for a second or two, even
with every tab idle.

The sampler counts these under children because they are named `powershell.exe`, so Warp's
app-only CPU above leaves them out. Their average across the controlled runs was 0.2 to 0.6 % of the
machine, more than Warp's own idle median. That is a lower bound: probes that start and exit
between two snapshots are not counted.

## Observed faults

Faults seen during normal use. Cerberus gets the same checks.

- **Stale frame after `/exit`.** After `/exit` in a `claude` session, Warp left Claude Code's
  slash-command menu (`/exit`, `/context`, `/usage-credits`) on screen with a large blank gap. It
  then printed the "Resume this session with" line under the stale frame instead of redrawing.
  The user's words: "this is what i meant by laggy n unresponsive".
- **Slow start.** With 24 tabs restored, Warp creates its window in about 0.5 s (measured, two
  runs) but shows it blank. The user saw content after about 5 s on one launch and about 30 s on
  another. The second launch had a screen-capture loop running, which may have slowed it. Warp used
  10.2 s of CPU in its first 66 s. A screen-capture timer could not see Warp's window on this
  two-monitor setup, so a repeatable number needs a screen recording.
- **Slow new tab.** A new tab takes about 4 s before it accepts input (user's estimate). PowerShell
  5.1 itself starts in 0.15 s here, with or without the user profile, so most of the wait is Warp's.
