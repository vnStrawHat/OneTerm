# Work: the flood hand-over bound is a stopwatch and a loaded CI runner trips it

ID: BUG-0064
Intake: IN-0029
Created: 2026-09-16

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug (a flaky gate, in test code only)
- Risk lane: standard
- Spec Intake, when required: IN-0029 (`US-0083`, the local-shell pump's render hand-over)

## Outcome

`a_flooding_loop_hands_the_engine_to_a_waiting_frame` passes on a loaded two-vCPU CI
runner while still failing for the defect it was written to catch.

The reported failure, "Full workspace quality gate" on ubuntu-latest (2 vCPU),
`cargo test --workspace`:

```text
thread 'event_loop::event_loop_tests::a_flooding_loop_hands_the_engine_to_a_waiting_frame'
panicked at crates/local-shell/src/event_loop_tests.rs:608:
a frame waited 339.690609ms behind the flooding pump (bound 250ms)
27 passed; 1 failed; 1 ignored
```

339 ms is not the defect. The defect is a pump that reads until the transport runs dry
and so never hands the engine over at all: with the flood still running, such a pump
holds the lock for the whole test and no frame ever arrives. The 250 ms number was
measured on the owner's machine (worst-of-five 86 / 108 / 125 ms over three runs) and
carries a margin that a shared runner running the rest of the workspace suite alongside
it does not have.

## Scope

- [x] In scope: `HANDOVER_BOUND` in `crates/local-shell/src/event_loop_tests.rs`, the
      comment that justifies it, and the sentence in `US-0083` that quotes the number.
- [x] Out of scope: the pump, the yield, `MAX_LOCKED_READ`, `TerminalHandle`, and the
      `US-0083` gap 6 race the watchdog thread works around. No production code changes.

## Acceptance

- [x] The bound is no longer a stopwatch reading from one machine: its comment says what
      property it gates and why the number is what it is.
- [x] A pump that never yields still fails, on the `recv_timeout(deadline)` path with
      "no frame reached the engine within ...".
- [x] A hold measured in seconds still fails on the assertion.
- [x] `deadline` stays derived from the bound, so the two cannot drift apart.
- [x] The test passes 10 consecutive solo runs and passes under `cargo test --workspace`
      load.
- [x] `docs/spec-intakes/IN-0029-vt-engine/US-0083-local-shell-native.md` no longer states
      a bound the code does not have.
- [x] `pwsh scripts/ci-local.ps1` passes.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/US-0083-local-shell-native.md` -- the owning
  packet. `:229-231` states "The test's bound is **250 ms** (was 750 ms before the read
  cap)" and, in the same paragraph, why the number is not the property: "the property the
  test pins is the design's: **one batch, not the whole flood**". `:397-400` (gap 10)
  says the same from the other side -- the hand-over is asserted in wall-clock only
  because the pump's batch count is not observable from outside the loop, "with the
  negative control proving the other side does not arrive". **Changed:** the bound
  sentence and its measurement table row.
- The measurement tables at `:210-217` and `:229-231` -- the loopback fixture numbers
  (6.0-17.0 ms after the read cap) and the negative control ("never arrives", failing in
  4.99 s). These are a record of runs that happened; they are not rewritten, and the new
  bound is stated against them.
- `docs/spec-intakes/IN-0029-vt-engine/US-0083-local-shell-native.md` gap 6
  (`:345-372`) -- the `TerminalHandle` demand race the watchdog thread in this test works
  around, and the reason the test already failed once in the workspace gate. Still open,
  still owned by `crates/terminal`. **No change needed:** this packet does not touch it.
- `docs/spec-intakes/IN-0029-vt-engine/evidence/US-0084-verify.md` `:353-358` (F7) -- the
  verifier's finding on the *ssh* sibling of this test, whose bound is 2 s and which the
  verifier recorded as "decoration": "only `assert!(taken)` proves anything". The same
  reading applies here and is the precedent for widening rather than tightening. **No
  change needed**, and evidence is not rewritten.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` -- the
  intake's testing contract. It sets no timing bound for this test. **No change needed.**

### Documentation Action

Update required: `docs/spec-intakes/IN-0029-vt-engine/US-0083-local-shell-native.md`, one
sentence, because it quotes the constant by value.

Reason: the packet's own framing of the property ("one batch, not the whole flood", proven
by the negative control) is unchanged and remains correct. Only the number moves.

### Reconciliation

Docs changed: `US-0083-local-shell-native.md` `:229-231`. No other document quotes the
constant -- `grep -rn "250 ms\|HANDOVER" docs/` returns only that line plus unrelated
hits (an OSC debounce, a placement bound, a DEC-0016 grace period).

## Context

What the test actually discriminates, and therefore what the bound is allowed to be:

| Case | How it fails today | Sensitive to the bound? |
| --- | --- | --- |
| Pump never yields (the `US-0083` defect, negative control measured at "never arrives", failing in 4.99 s) | `report_rx.recv_timeout(deadline)` expires -- **no frame at all**, because the flood does not stop while the frames are taken and a fair mutex cannot help a waiter that never sees an unlock | No. Any bound fails it. |
| Pump yields but holds for seconds | assertion on `worst` | Yes, and it still fails at 1 s. |
| Pump yields per chunk, runner is loaded | passes on the owner's machine (86-125 ms), **failed CI at 339 ms** | Yes. This is the flake. |

So the discriminating power sits almost entirely in "does a frame arrive at all", which is
the `recv_timeout` path, not the assertion. Widening the assertion costs the gate a
failure mode that is not real -- a hand-over that works but takes 300 ms on a shared
runner is not the regression `US-0083` guards against -- while a hold of a second or more,
let alone the whole flood, still fails.

**The change, and why this one.** `HANDOVER_BOUND` goes from 250 ms to **1 s**, a single
constant, with the comment rewritten to say that it is a ceiling on a hold, not a
measurement. `deadline` is already `HANDOVER_BOUND * FRAMES + 1 s`, so it follows to 6 s
and cannot drift from the bound.

The alternative considered was judging the **median** of the five frames rather than the
worst, keeping the 250 ms number. Rejected: it is more code (collect, sort, index) for a
weaker property -- "at least three of five frames were fast" -- and it does not actually
remove the flake, because a two-vCPU runner descheduling the renderer thread affects
consecutive frames, not one unlucky one. The `FRAMES = 5` / worst-of-five shape is worth
keeping exactly as it is: five acquisitions, and one slow one fails. Only the ceiling
moves.

1 s is also still tighter than the ssh sibling of this test, whose bound is 2 s and which
`US-0084`'s verifier recorded as decoration.

## Plan

- [x] Confirm the constant is used nowhere else (`grep -rn "HANDOVER_BOUND" crates/`).
- [x] Raise the bound; rewrite its comment to state the property and the two failing sides.
- [x] Update the `US-0083` sentence.
- [x] 10 solo runs, plus a run under `cargo test --workspace` load.
- [x] `pwsh scripts/ci-local.ps1`.

## Decisions

None. The choice is recorded in Context and binds no future work: it is a test ceiling,
not a contract.

## Verification Plan

- Unit proof: `cargo test -p oneterm-local-shell a_flooding_loop_hands_the_engine_to_a_waiting_frame`,
  10 consecutive runs.
- Integration proof: the same test under whole-workspace load, which is the condition that
  produced the failure -- `cargo test --workspace` inside the gate run.
- No E2E or platform proof: the test is a unit test of the local-shell pump against an
  in-process peer fixture, and the changed line is platform-independent.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Ten solo runs

`cargo test -p oneterm-local-shell --lib -- --exact event_loop::event_loop_tests::a_flooding_loop_hands_the_engine_to_a_waiting_frame`,
ten times in a row on the Windows host:

```text
run  1: test result: ok. 1 passed; 0 failed; 0 ignored; 34 filtered out; finished in 0.13s
run  2: test result: ok. 1 passed; 0 failed; 0 ignored; 34 filtered out; finished in 0.17s
run  3: test result: ok. 1 passed; 0 failed; 0 ignored; 34 filtered out; finished in 0.13s
run  4: test result: ok. 1 passed; 0 failed; 0 ignored; 34 filtered out; finished in 0.15s
run  5: test result: ok. 1 passed; 0 failed; 0 ignored; 34 filtered out; finished in 0.17s
run  6: test result: ok. 1 passed; 0 failed; 0 ignored; 34 filtered out; finished in 0.13s
run  7: test result: ok. 1 passed; 0 failed; 0 ignored; 34 filtered out; finished in 0.15s
run  8: test result: ok. 1 passed; 0 failed; 0 ignored; 34 filtered out; finished in 0.15s
run  9: test result: ok. 1 passed; 0 failed; 0 ignored; 34 filtered out; finished in 0.15s
run 10: test result: ok. 1 passed; 0 failed; 0 ignored; 34 filtered out; finished in 0.15s
```

10/10. Solo, the whole test finishes in about 150 ms, so every one of the five frames is
served well inside the old bound too: this run proves the change did not break the test,
not that it fixed the flake. The load run below is the one that matters.

### Under workspace load

The `oneterm-local-shell` library section of `cargo test --workspace` inside the gate run
below, which is the condition that produced the CI failure -- 20 other crates' test
binaries running on the same machine:

```text
running 35 tests
test event_loop::event_loop_tests::a_flooding_loop_hands_the_engine_to_a_waiting_frame ... ok
test result: ok. 33 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 10.31s
```

The section takes 10.31 s under load against 0.15 s solo, roughly seventyfold, which is
the scheduling pressure that turned 125 ms into 339 ms on the CI runner.

### `pwsh scripts/ci-local.ps1`

Run in the worktree, Windows host, no `--full`. All 25 steps passed, exit 0:

```text
==> cargo fmt --all -- --check
==> cargo clippy --workspace --all-targets -- -D warnings
==> cargo clippy --workspace --all-targets --features oneterm-app/terminal-diagnostics -- -D warnings
==> cargo test --workspace
==> cargo test -p oneterm-vt --features vt-paranoid
==> cargo test -p oneterm-vt --features regex
==> cargo build -p oneterm-vt --no-default-features --examples
==> cargo test -p oneterm-vt --no-default-features
==> cargo build -p oneterm-vt --all-features --examples
==> cargo tree -p oneterm-vt -e normal --no-default-features
==> cargo run -p oneterm-vt --example headless
==> cargo doc -p oneterm-vt --no-deps
==> cargo doc -p oneterm-vt --no-deps --all-features
==> python scripts/vt-public-api.py --check --no-doc
==> python scripts/vt-public-api.py --check-nameable --no-doc
==> python scripts/vt-public-api.py --diff-platforms
==> cargo package -p oneterm-vt --list | verify-dependency-graph.py --package-list -
==> rustdoc self-containment (crates/vt/src)
==> rustdoc self-containment (crates/vt/docs/guide)
==> python scripts/verify-dependency-graph.py
==> python scripts/check-doc-paths.py
==> python -m unittest scripts/test_check_english.py
==> python scripts/check-english.py
==> python scripts/completion-catalog.py validate
==> python scripts/third-party-notices.py --check
ci-local: all checks passed.
```

Totals over the four test steps, **131 sections, 4533 passed, 0 failed, 24 ignored**:

| step | sections | passed | failed | ignored |
| --- | --- | --- | --- | --- |
| `cargo test --workspace` | 71 | 2046 | 0 | 12 |
| `cargo test -p oneterm-vt --features vt-paranoid` | 20 | 834 | 0 | 4 |
| `cargo test -p oneterm-vt --features regex` | 20 | 847 | 0 | 4 |
| `cargo test -p oneterm-vt --no-default-features` | 20 | 806 | 0 | 4 |


### Gaps

1. **Not reproduced on a two-vCPU Linux runner.** The failure is a CI observation; the fix
   is verified on the owner's eight-core Windows host, where the original 250 ms bound also
   passed. The proof that the widened bound is enough for ubuntu-latest is the owner's next
   push. The bound is 3x the observed CI worst case (339 ms) and 8x the owner's worst
   (125 ms); if the job trips it again, the honest conclusion is that a wall-clock
   assertion does not belong in this test at all and only the `recv_timeout` negative
   control should remain, which is `US-0084`'s verifier finding F7 applied here.
2. **`US-0083` gap 6 is still open.** The watchdog thread in this test exists to work
   around a lost-demand race in `TerminalHandle::lock_for_render`, owned by
   `crates/terminal`. Untouched here. A CI failure of the "no frame reached the engine
   within ..." kind, rather than the assertion kind, points at that race and not at this
   bound.

## Harness Record

This packet was written in an isolated worktree, so `harness.db` in the main checkout was
not touched. Run this from the repository root to insert the row (Python 3, standard
library only):

```python
import sqlite3

with sqlite3.connect("harness.db") as db:
    db.execute(
        """INSERT OR REPLACE INTO story (
            id, title, created_at, risk_lane, contract_doc, packet_doc, status,
            unit_proof, integration_proof, e2e_proof, platform_proof, evidence,
            verify_command, last_verified_at, last_verified_result, notes, intake_id
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
        (
            "BUG-0064",
            "The flood hand-over bound is a stopwatch and a loaded CI runner trips it",
            "2026-09-16",
            "standard",
            "docs/spec-intakes/IN-0029-vt-engine/US-0083-local-shell-native.md",
            "docs/spec-intakes/IN-0029-vt-engine/BUG-0064-flood-handover-bound-fails-on-ci.md",
            "implemented",
            1, 1, 0, 0,
            "HANDOVER_BOUND raised from 250 ms to 1 s. 10/10 solo runs pass; the test passes inside cargo test --workspace (oneterm-local-shell section 33 passed, 0 failed, 10.31s under load). pwsh scripts/ci-local.ps1 passed, 25 steps, 131 sections, 4533 passed, 0 failed.",
            "pwsh scripts/ci-local.ps1",
            "2026-09-16",
            "pass",
            "Test-only change. Not reproduced on a two-vCPU Linux runner; the owner push is the proof. US-0083 gap 6 (the TerminalHandle lost-demand race) is still open and owned by crates/terminal.",
            34,
        ),
    )
```

## Handoff

Next action is the owner's: push, and read the ubuntu-latest job. A repeat failure on the
assertion line belongs back on this packet as acceptance rework; a failure on the
`recv_timeout` line is `US-0083` gap 6 and belongs to `crates/terminal`.
