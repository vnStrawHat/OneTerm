# Independent verification: BUG-0064 (flood hand-over bound)

Verifier: adversarial review agent, own worktree, no implementer context.
Subject: `5a0106f6` "fix(ci): unbreak the Linux build and the flood hand-over gate",
parent `c8055a71` (main).
Host: Windows 11, 8 cores, toolchain 1.96.0, `test` profile (opt-level 0),
`CARGO_BUILD_JOBS=4`.

## Verdict: **PASS**

`HANDOVER_BOUND = 1 s` is a **gate, not decoration**. I proved both failing sides
experimentally on this host, not by reading. The defect the test was written to catch -
a pump that reads until the transport runs dry and never hands the engine over - still
fails, and the assertion is still live and still reports separately from the deadline.

One LOW note about a cost the packet does not mention, and one LOW note about a stale
number in the constant's own comment. Neither blocks.

---

## Attack 4a - read the test end to end: is 1 s a gate?

`crates/local-shell/src/event_loop_tests.rs:480-621`. The test has **two** independent
failure paths, and raising the constant moves both:

```rust
const HANDOVER_BOUND: Duration = Duration::from_secs(1);   // :497
const FRAMES: u32 = 5;                                     // :500
...
let deadline = HANDOVER_BOUND * FRAMES + Duration::from_secs(1);  // :605  -> 6 s
let report = report_rx.recv_timeout(deadline);                    // :606
...
let worst = report.unwrap_or_else(|_| {
    panic!("no frame reached the engine within {deadline:?} of a flooding pump")  // :616
});
assert!(worst < HANDOVER_BOUND, ...);                             // :618
```

The renderer thread (`:593-604`) takes `FRAMES` frames, each `lock_for_render()` with a
5 ms sleep between, and reports the **worst single wait** through an `mpsc` channel. The
flood thread does not stop until after `recv_timeout` returns (`:608`), so a pump that
never yields holds the engine for the whole window and the channel stays empty.

So the test passes exactly when **every one of the five waits is under 1 s** *and* their
sum (plus 25 ms of sleeps) is under 6 s. The largest single hold that still passes is just
under 1 s; **anything at or above 1 s on one frame fails at `:618`**, and a cumulative
hold of 6 s or more fails at `:616` first. The 250 ms -> 1 s change widened the first
window by 4x and the second by 2.7x; it did not remove either.

## Attack 4b - negative control: does a never-yielding pump still fail?

**Proved, not assumed.** Temporary one-line inversion in this worktree only, disabling the
hand-over at its source (`crates/local-shell/src/event_loop.rs:462`,
`if self.term.render_demand_raised()` -> `if false && self.term.render_demand_raised()`),
with the frame waits instrumented by a temporary `eprintln!` in the test. **Both edits
were reverted; `git status` is clean at `5a0106f6`.**

```text
cargo test -p oneterm-local-shell --lib -- --exact \
  event_loop::event_loop_tests::a_flooding_loop_hands_the_engine_to_a_waiting_frame --nocapture

VERIFY frame wait 6.0758477s
VERIFY frame wait 2us
VERIFY frame wait 1.2us
VERIFY frame wait 1.7us
VERIFY frame wait 1.6us
thread '...a_flooding_loop_hands_the_engine_to_a_waiting_frame' panicked at
  crates\local-shell\src\event_loop_tests.rs:616:9:
no frame reached the engine within 6s of a flooding pump
test result: FAILED. 0 passed; 1 failed; ... finished in 6.11s
```

Exactly the packet's claim, confirmed at the new bound: the defect side **does not arrive
at all**, fails on the `recv_timeout(deadline)` path at line **616**, and the first frame
only got the lock 6.08 s in - after `stop` was set and the flood stopped. The four
following frames then took microseconds, which is the signature of "the lock was never
released, then released all at once".

## Attack 4c - assertion control: is the `assert!` itself live?

Second temporary inversion, test file only: `HANDOVER_BOUND` set to `1 ms` with the real
yield restored.

```text
VERIFY frame wait 13.0519ms
VERIFY frame wait 8.2554ms
VERIFY frame wait 7.3875ms
VERIFY frame wait 7.2883ms
VERIFY frame wait 19.0143ms
thread '...' panicked at crates\local-shell\src\event_loop_tests.rs:618:5:
a frame waited 19.0529ms behind the flooding pump (bound 1ms)
test result: FAILED. 0 passed; 1 failed; ... finished in 0.19s
```

So the two paths are live **and distinguishable by line number and message**, which is
what the packet's Handoff section depends on: line 618 / "a frame waited" is acceptance
rework of this packet; line 616 / "no frame reached" points at `US-0083` gap 6, the
`TerminalHandle` lost-demand race owned by `crates/terminal`.

## Attack 4d - 10 solo runs, with frame times

Instrumented as above. `cargo test -p oneterm-local-shell --lib -- --exact ... --nocapture`,
ten consecutive runs. All five frame waits per run:

| run | frame waits | worst |
| --- | --- | --- |
| 1 | 10.52 / 6.88 / 17.25 / 18.14 / 17.72 ms | 18.14 ms |
| 2 | 0.22 / 6.44 / 6.94 / 18.56 / 17.70 ms | 18.56 ms |
| 3 | 10.97 / 6.20 / 18.71 / 6.33 / 6.29 ms | 18.71 ms |
| 4 | 11.20 / 6.35 / 17.76 / 16.87 / 17.46 ms | 17.76 ms |
| 5 | 11.65 / 6.60 / 19.29 / 6.36 / 6.39 ms | 19.29 ms |
| 6 | 11.29 / 6.44 / 17.46 / 19.19 / 18.52 ms | 19.19 ms |
| 7 | 11.68 / 7.88 / 19.12 / 19.13 / 19.02 ms | 19.13 ms |
| 8 | 0.28 / 6.47 / 6.15 / 18.02 / 18.61 ms | 18.61 ms |
| 9 | 0.53 / 6.97 / 6.73 / 18.43 / 17.96 ms | 18.43 ms |
| 10 | 11.30 / 5.91 / 17.27 / 17.01 / 20.26 ms | 20.26 ms |

**10/10 pass. Worst frame over all 50 acquisitions: 20.26 ms.** Every run finished in
0.13-0.25 s. The waits cluster near 6, 11 and 17-19 ms, which is the 16 ms watchdog
cadence at `:576` beating against the pump's chunk boundaries - the hand-over is
watchdog-paced, not parse-paced.

## Attack 4e - under `cargo test --workspace` load

`cargo test --workspace --no-fail-fast` running concurrently in the background (build
phase then the full 71-section suite) while the prebuilt `oneterm_local_shell` test
binary was invoked directly with the same `--exact` filter, twelve times across two
batches:

| run | worst frame | run | worst frame |
| --- | --- | --- | --- |
| 1 | 27.74 ms | 7 | 21.12 ms |
| 2 | 18.60 ms | 8 | 18.17 ms |
| 3 | 18.45 ms | 9 | **41.99 ms** |
| 4 | 17.95 ms | 10 | 8.54 ms |
| 5 | 21.45 ms | 11 | 33.72 ms |
| 6 | 19.58 ms | 12 | 18.38 ms |

**12/12 pass. Worst frame under load: 41.99 ms.**

Independently, the in-suite run inside my own `ci-local` gate (below) reproduced the
packet's figure to within 10 ms:

```text
Running unittests src\lib.rs (target\debug\deps\oneterm_local_shell-....exe)
running 35 tests
test event_loop::event_loop_tests::a_flooding_loop_hands_the_engine_to_a_waiting_frame ... ok
test result: ok. 33 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 10.30s
```

Packet reported 10.31 s; I measured 10.30 s. **Margin on this host: 1 s is ~24x the worst
frame I could produce under load (41.99 ms) and ~3x the 339 ms the two-vCPU runner
reported.**

## Attack 4f - does any doc still quote 250 ms?

```text
grep -rn "250" crates/local-shell/
grep -rn "250 ms|250ms|250 milli" docs/spec-intakes/IN-0029-vt-engine/
```

Nine hits, **all deliberate history, none a live claim**:

- `crates/local-shell/src/event_loop_tests.rs:494` - inside the new comment, explaining
  why 250 ms was too tight. The only `250` left in `crates/local-shell/`.
- `docs/spec-intakes/IN-0029-vt-engine/US-0083-local-shell-native.md:229` - rewritten to
  "The test's bound is **1 s** (250 ms when this packet shipped, 750 ms before the read
  cap; raised by `BUG-0064` ...)". Correct and dated.
- Seven in `BUG-0064-...md` itself - the failure text, the rejected alternative, the
  harness note.

`grep -rn "HANDOVER_BOUND" crates/` returns four lines, all in
`event_loop_tests.rs` (`:497` definition, `:605` deadline, `:618`/`:619` assertion). The
packet's claim that `deadline` follows the constant and cannot drift is **true** - it is
derived, not a second literal.

Cross-reference spot checks, all accurate: the ssh sibling's 2 s bound is at
`crates/ssh/src/task_tests.rs:171`; `US-0084`'s verifier finding F7 calling it
"decoration" is at `docs/spec-intakes/IN-0029-vt-engine/evidence/US-0084-verify.md:353-358`;
`US-0083` gap 10 ("the hand-over is asserted in wall-clock, not in batches") is at
`:394-400`.

## Attack 5 - packet honesty

**Result: honest.**

- **Owning docs reviewed**: five entries with line references. I checked each. `US-0083`
  `:229-231` is the only place the constant was quoted, and it is the one place changed.
  The `US-0084` F7 precedent, the `US-0083` gap 6 watchdog rationale and
  `low-level-design/testing-and-bench.md` (no timing bound for this test) all check out.
- **Acceptance**: every box it ticks is now independently proven above, including the two
  I expected to be assertion-only - "a pump that never yields still fails, on the
  `recv_timeout(deadline)` path" (attack 4b) and "a hold measured in seconds still fails
  on the assertion" (attack 4c, by inversion).
- **Gaps**: both are real and stated. Gap 1 in particular pre-commits to the right
  conclusion if ubuntu trips again ("a wall-clock assertion does not belong in this test
  at all"), which is the honest position given attack 4b shows the `recv_timeout` control
  carries nearly all the discriminating power.
- **Harness snippet**: parses. 17 column names, 17 `?` placeholders, 17 tuple values,
  matching the schema at
  `docs/spec-intakes/IN-0036-russh-0-63/US-0095-russh-0-63-bump.md:716-719`. Proof flags
  `1, 1, 0, 0` - unit and integration claimed, E2E and platform not - which matches the
  `HARNESS:PROOF` block and what was actually run.
- **ci-local result**: reproduced exactly. See below.

## Attack 6 - `pwsh scripts/ci-local.ps1`

Run to completion by me, in this worktree, on `5a0106f6`, no `--full`,
`CARGO_BUILD_JOBS=4`, cold target directory.

```text
ci-local: all checks passed.
[exited with code 0]
```

All **25** steps green. Test totals, aggregated from the log by summing every
`test result:` line per step:

| step | sections | passed | failed | ignored |
| --- | --- | --- | --- | --- |
| `cargo test --workspace` | 71 | 2046 | 0 | 12 |
| `cargo test -p oneterm-vt --features vt-paranoid` | 20 | 834 | 0 | 4 |
| `cargo test -p oneterm-vt --features regex` | 20 | 847 | 0 | 4 |
| `cargo test -p oneterm-vt --no-default-features` | 20 | 806 | 0 | 4 |
| **total** | **131** | **4533** | **0** | **24** |

**Identical to the packet's table**, section for section and test for test.

## Defects

| # | Severity | Where | Finding |
| --- | --- | --- | --- |
| E1 | LOW | `crates/local-shell/src/event_loop_tests.rs:605` | Raising the bound also raised `deadline` from 2.25 s to 6 s, so a genuine starvation regression now takes **6.11 s to fail** (measured, attack 4b) instead of ~2.4 s. Correct behaviour, but a 2.5x slower failure on the exact CI job the packet is trying to unblock, and the packet does not mention it. If it matters, decouple: `let deadline = HANDOVER_BOUND * FRAMES + Duration::from_secs(1)` could be `Duration::from_secs(3)` outright without weakening either gate. |
| E2 | LOW | `crates/local-shell/src/event_loop_tests.rs:484-486` | The comment calibrating the bound quotes "worst-of-five at 86 / 108 / 125 ms over three runs on the author's machine". On that same class of machine I measure **18-20 ms solo and 42 ms worst under full-workspace load** (attacks 4d, 4e) - 4-6x faster. The quoted figures are inherited from `US-0083` and predate something (the read cap, or a build-profile change); they now overstate the honoured path and make the 1 s bound look tighter than it is. Refresh or attribute them to the `US-0083` run that produced them. |
| E3 | LOW | packet `:281-302` | `intake_id` hardcoded to `34` while the packet states `harness.db` was never opened, under `INSERT OR REPLACE`. Unverified id + replace semantics can silently overwrite another intake's story. Prefer a `SELECT` lookup, or an explicit placeholder as `BUG-0061` used. Same defect in `BUG-0063`. |

No MEDIUM or HIGH findings for this packet.

## Gaps in this verification

1. **Not reproduced on a two-vCPU Linux runner** - the same gap the packet declares. My
   "under load" is an 8-core Windows host running its own workspace suite, which produced
   a worst frame of 42 ms against the runner's reported 339 ms. I cannot demonstrate the
   flake, only that the new bound has 24x headroom over the worst I can create.
2. **The seconds-long-hold case was proved by shrinking the bound, not by slowing the
   pump.** Attack 4c shows the assertion fires on `worst` and reports at line 618; it does
   not exercise a pump that genuinely holds the engine for 1-6 s. That intermediate
   regime - yields, but far too slowly - remains covered by argument rather than by a run.
3. **No instrumentation survives.** The `eprintln!` and the `if false &&` inversion were
   temporary and are reverted; the numbers above cannot be re-derived from the committed
   tree without re-applying them.
