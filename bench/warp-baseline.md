# Warp baseline

Memory and CPU of Warp hosting Claude Code, taken with `bench/sample.ps1` so the same run can be
repeated on Cerberus.

## Setup

| | |
|---|---|
| Machine | Intel Core Ultra 9 275HX (24 logical cores), 32 GB RAM, RTX 5090 Laptop GPU |
| OS | Windows 11, Windows PowerShell 5.1 |
| Warp | v0.2026.10.21.08.25.stable_02 |
| Claude Code | 2.1.312 |
| Date | 2026-10-28 |

## Method

To be written after the controlled states.

## Results

To be written: states (a) to (d) and the workday (e).

## Observed faults

Faults seen during normal use. Cerberus gets the same check.

- **Stale frame after `/exit`.** After `/exit` in a `claude` session, Warp left Claude Code's
  slash-command menu (`/exit`, `/context`, `/usage-credits`) on screen with a large blank gap. It
  then printed the "Resume this session with" line under the stale frame instead of redrawing.
  The user's words: "this is what i meant by laggy n unresponsive".
