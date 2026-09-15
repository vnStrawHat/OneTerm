# Evidence: the `esctest` run for US-0102

ID: US-0102
Intake: IN-0038
Recorded: 2026-09-15
Result: **not run — blocked, and not by the platform**

## What the packet asked for

> An `esctest` run on Linux is attached to Evidence as a pass/fail count per test group, with the
> count **before** this packet and after. No threshold is enforced.

No counts are attached. This document records why, so the next attempt does not re-discover it.

## What was expected to block it, and did not

`testing-and-bench.md` § 6 says `esctest` "is GPL-2.0 and cannot run on Windows", and the plan was
to write a harness in `crates/tools` that a Linux CI job could drive later: spawn `esctest.py` with
its stdin and stdout on the slave side of an `openpty`, pump the master through `Terminal::feed`,
and write every `VtEvent::Reply` back. `oneterm-vt`'s `pty` feature already has the Unix half of
that, so the harness would have been perhaps a hundred lines.

Being on Windows is a real obstacle and it is not the one that matters.

## What actually blocks it

**`esctest` reads the screen back with `DECRQCRA`, and this engine does not implement it.**

`esctest`'s assertions are `AssertScreenCharsInRectEqual(rect, strings)` and its relatives. Over a
pty there is exactly one way to satisfy them: `DECRQCRA` — `CSI Pid ; Pg ; Pt ; Pl ; Pb ; Pr * y`,
"request checksum of rectangular area" — which the terminal answers with
`DCS Pid ! ~ <hex checksum> ST`. That is the whole readback channel; `esctest` exists because
`DECRQCRA` gives a way to compare a real DEC terminal against an emulator.

`crates/vt/src/terminal/dispatch.rs` has no `* y` arm. Every rectangle assertion would therefore
wait for a reply that never comes and fail on a timeout, and the resulting matrix would measure one
absent query rather than the engine's conformance. A "before" count and an "after" count taken that
way would both be approximately zero and would say nothing about the eight gaps this packet closed.

A subset of `esctest` asserts only on replies (`DA`, `DSR`, `DECRQM`, `DECSCUSR`) and would run
without `DECRQCRA` behind `--include`. That subset is small, and the engine's unit tests already
cover those replies byte for byte, so it would buy a second opinion rather than new coverage.

## What it would take

In order, and none of it belongs in a print-path packet:

1. **`DECRQCRA` (`CSI * y`), with its `DECRQSS`-shaped `DCS` reply.** This is its own packet, and a
   careful one: it is a screen-readback primitive, xterm gates it behind `allowWindowOps` for that
   reason, and the checksum definition moved twice in xterm's own history (#334 and #336) so the
   variant has to be chosen and pinned. It also needs `DECRQPSR`-adjacent plumbing that `BUG-0058`
   deliberately left unanswered.
2. **The harness binary**, `crates/tools/src/bin/vt-esctest.rs`, Unix-only: `openpty`, spawn
   `esctest.py` on the slave, feed the master through a `Terminal`, write replies back, exit with
   `esctest`'s own status. Deliberately **not written** here: it cannot be compiled on this host
   (it would be `#[cfg(unix)]` and invisible to every check that runs on Windows) and it cannot be
   exercised anywhere until step 1 lands, so it would be unverified code shipped on a promise.
3. **The Linux CI job**, report-only, publishing the matrix as an artifact and never failing the
   build — which is what `testing-and-bench.md` § 6 already specifies.

## What was verified instead

The eight items are covered by byte-feed tests that drive real bytes through a real `Terminal` and
assert on the observable result; they are listed in `US-0102-conformance-gaps.md` § Evidence with
the sequence each one feeds. The 46-recording parity corpus replays byte-identically, which is the
packet's own statement that none of the eight changed behaviour it should not have.

## References

- `esctest2` — <https://github.com/ThomasDickey/esctest2>
- `DECRQCRA` — <https://vt100.net/docs/vt510-rm/DECRQCRA.html>
- `testing-and-bench.md` § 6, "Conformance as a report, not a gate"
