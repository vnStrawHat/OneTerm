# Work: The pump-yield test measures the contract, not the machine's load

ID: BUG-0070
Intake: IN-0032
Created: 2026-09-17

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug (test defect — the assertion, not the product)
- Risk lane: normal
- Spec Intake, when required: `IN-0032` —
  `docs/spec-intakes/IN-0032-terminal-crate-tidy/IN-0032.md`, which owns `crates/terminal`'s
  `Demand` and the handshake tests (`US-0090` moved both there from `crates/vt`).

## Outcome

`oneterm-terminal::handle::tests::a_pump_yields_to_the_demand_within_a_bounded_number_of_chunks`
passes on an idle machine run alone, and keeps failing when the pump stops yielding to a raised
render demand. A test that only passes while the machine is busy proves nothing about the
product; this one must prove the yield.

## Reported by

Two independent readings of `main`, both on `IN-0042`:

- `docs/spec-intakes/IN-0042-ux-polish-round-1/evidence/sftp-agent-wave1-verify.md` § "The flaky
  test the implementer reported" — three standalone runs after a green gate, three failures,
  `crates/terminal/src/handle.rs:364`, *"the renderer waited 65 chunks, not one"*, and the same
  test green inside the gate's `cargo test --workspace`. It records the diagnosis this packet
  starts from: *"a test that only passes under contention is not a test"*, and asks for a `BUG`.
- `docs/spec-intakes/IN-0042-ux-polish-round-1/US-0124-sftp-table-fits-and-dual-pane-has-transfer-controls.md`
  § "Rework after independent verification (2026-09-17)" — the rework that report drove. The
  diff it covers touches no file in `crates/terminal`, which is what establishes the failure as
  pre-existing rather than a regression of that packet.
- `docs/spec-intakes/IN-0042-ux-polish-round-1/evidence/before-after-report.md` § 4.3 carries it
  forward as an open item of the round with **no `BUG` opened**. This packet is that `BUG`.

The owner reproduced it again on 2026-09-17 and saw the same failure at a count of 149 chunks.

## Scope

- [ ] In scope:
  - `crates/terminal/src/handle.rs` — the test, and `Demand` / `lock_for_render` /
    `render_demand_raised` if the root cause turns out to be in them.
  - `docs/terminal-backend.md` § 5.1, if the contract text needs changing.
- [ ] Out of scope:
  - The two real pumps (`crates/local-shell/src/event_loop.rs`, `crates/ssh`'s
    `ssh_main_task`). They are covered by their own tests and no report touches them.
  - Any change to the handshake's semantics. `US-0082`'s rework and `US-0083` gap 6 settled who
    clears the demand; this packet does not reopen that.

## Acceptance

- [x] The root cause is established by reading `Demand`, the test's pump loop and the acquire
      path, and written down here, before the assertion is touched.
- [x] The test still fails if the pump stops yielding — that is, it measures the handshake and
      not a wall-clock race the scheduler decides. Proved by two mutation probes, 3/3 each; see
      "What fails when the product breaks".
- [x] 20 standalone runs on an idle machine pass, and the measured counts are recorded here.
- [x] 3 runs inside `cargo test --workspace` pass.
- [x] `docs/terminal-backend.md` is either amended or recorded as already correct.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md` § 5.1 — the demand/yield handshake, `MAX_LOCKED_READ`, and what a
  pump must do at a chunk boundary. The contract the test exists to pin.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md`
  § "Fairness and reply latency" (R-37) — where the handshake was specified.
- `docs/spec-intakes/IN-0032-terminal-crate-tidy/US-0090-migration-residue-and-visibility.md` —
  the packet that moved `Demand` and this test into `crates/terminal`.

### Documentation Action

No contract change. The reviewed docs describe the handshake correctly and say nothing about a
chunk count: § 5.1 specifies *when* a pump asks (at a chunk boundary, after the batch's replies)
and *what* it does with a `true` (drops the guard, ends the batch), not how many chunks a waiting
frame may cost. The defect is in what the test measured, so the fix is in the test and the doc
stays as it is.

Reason: the product behaviour is unchanged and the owning contract already matches it.

### Reconciliation

No owning doc changed. `docs/terminal-backend.md` § 5.1 was re-read against the reworked test and
remains accurate; the rustdoc on the test itself carries the new sentence, next to the code that
must not be undone.

## Context

### The root cause: the measurement started before the pump could know

The test builds a deliberately hostile stand-in for a pump: an **unfair** `std::sync::Mutex`, a
thread that locks, increments, unlocks and counts a chunk in a tight loop, and one `Demand`. The
renderer then raises the demand and takes the lock. The old measurement was:

```rust
let at_raise = chunks.load(Ordering::Relaxed);   // (1)
demand.raise();                                  // (2)
let waiting = Instant::now();
{ let _engine = engine.lock(); demand.release(); }
let chunks_waited = chunks.load(Ordering::Relaxed) - at_raise;   // (3)
assert!(chunks_waited <= 8);
```

Between (1) and the pump's first `demand.is_raised()` that returns `true`, the pump is
**unthrottled**: one iteration is an uncontended `SRWLOCK` acquire and release plus two atomics,
tens of nanoseconds. The renderer's own path from (1) to being queued on the mutex — the atomic
read-modify-write in `raise`, `Instant::now()` (a `QueryPerformanceCounter` call on Windows), and
the contended `lock()` — is microseconds. So the pump gets through as many chunks as fit in those
few microseconds before the raise is visible to it: 65 and 149 are 3 to 8 microseconds of a
free-running loop, which is exactly what an idle machine gives it.

`cargo test --workspace` hides it: the pump thread competes with every other test binary's
threads, it is descheduled inside that window, and the count lands under 8. **The bound passed
because the machine was busy**, which is the report's point.

Nothing in that window is a product defect. The pump *does* yield — the wall-clock half of the
same test (`waited < 2 s`) passed in every failing run, and the frame got in on the pump's first
park. What the old assertion measured is how fast the renderer thread reaches the mutex relative
to an unthrottled pump, which is a property of the scheduler.

Two facts make the reworked measurement deterministic rather than merely wider:

1. Once the pump has *seen* the demand it throttles itself — every subsequent chunk costs it a
   250 µs park. Counting from that point, a bound of 8 chunks is 2 ms of slack for the renderer
   thread, not a race with a loop that spins at nanosecond speed.
2. The pump is made to publish the chunk at which it first asked with the demand raised, and the
   renderer does not contend for the lock until that value is visible. The renderer is therefore
   provably behind the pump's first yield, which is the ordering the old test only assumed.

The count is also taken **inside** the renderer's critical section. Read after the guard is
dropped, an uncontended pump adds chunks between the acquisition and the load — the same class of
error one step later.

### The measurement that actually discriminates

Fixing the count alone would have produced a test that passes on an idle machine and on a broken
product alike. Two mutation probes proved it: with the old *shape* and the corrected mark, both
"the pump never parks" and "`Demand::is_raised` is the one-shot `swap` the `US-0082` rework
replaced" still passed, because the frame is already queued on the mutex and the pump's very
first yield lets it in. The old test had the same hole — the wall-clock half passed in every one
of the reported failures — so nothing here regresses, but nothing would have been gained either.

So the test now also watches the pump **while the demand stands and nobody is taking the lock**:
over a fixed 20 ms window a pump that keeps yielding pays a park at every chunk boundary while a
frame is outside, so its chunk count collapses by four orders of magnitude. That is the property
the handshake actually promises: the pump yields for *as long as* the frame waits, not once.

> The first bound for that window was `window / park + 1` = 81, justified as "an upper bound a
> sleep can only overshoot". **That justification was wrong and CI proved it** — see "Acceptance
> rework 2026-09-22 (portable bound)" below, which replaces the ratio with an absolute 2 000.

### What fails when the product breaks — measured

| Mutation | Result |
|---|---|
| `Demand::is_raised` back to the one-shot `swap(0, …)` (the `US-0082` regression) | **FAIL 3/3.** "the pump took 592040 chunks in 20ms with a frame waiting; a pump that keeps yielding fits 81" |
| The pump stops parking when the demand is raised | **FAIL 3/3**, 533340 / 545746 / 524253 chunks against 81 |
| The pump never asks at all | The 5 s deadline on the wait for `asked_at` fires. By construction — the probe was not run, because it is the same code path as an unset `asked_at`. |
| The pump never even reaches its steady state | The **same** 5 s deadline fires on the first wait. Added after the verification (`B70-m2`): that loop was an unbounded `while chunks < 4 { yield_now() }`, so the packet's "fails on the deadline instead of hanging" was true of the second wait only. |

The margin on the first two is about 6500x, so neither is a scheduler-dependent verdict.

## Plan

- [x] Read `Demand`, `lock_for_render`, the pump loop in the test, and the two sibling tests that
      cover the same handshake over `FairMutex`.
- [x] Decide product defect vs. test defect, and record which in Context.
- [x] Restructure the measurement; leave the handshake alone.
- [x] 20 standalone runs, 3 workspace runs, counts recorded.
- [x] Re-read `docs/terminal-backend.md` § 5.1 against the result.

## Decisions

None. `DEC-0016` and the `US-0082` rework already own the handshake's semantics; this packet
inherits them and adds nothing future work must follow.

## Verification Plan

- Focused: `cargo test -p oneterm-terminal --lib handle::` — the five handshake tests together.
- The flake itself: 20 consecutive standalone runs of the reworked test on an otherwise idle
  machine, plus 3 `cargo test --workspace` runs, with the observed chunk deltas.
- Regression: `cargo test -p oneterm-terminal`, then `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### The change

`crates/terminal/src/handle.rs`, test only, in three steps:

1. the pump publishes `asked_at` — the chunk at which it first asked with the demand raised —
   with a `compare_exchange` from a `u64::MAX` sentinel, and the renderer waits for that
   publication under a 5 s deadline before it does anything else;
2. with the demand standing and the lock untaken, the pump's chunks over a 20 ms window are
   counted and bounded by `window / park + 1`;
3. the renderer then marks the chunk count, takes the lock, reads the count again **under** the
   guard, and the old assertion runs on that delta with the bound left at 8 — now 2 ms of the
   pump's own parks rather than microseconds of an unthrottled loop.

Both waits share **one** deadline, and the measurement runs into an `Option` so that **every
assertion happens after `stop` and `pump.join()`**. Both of those came from the verification
(`evidence/acceptance-rework-2-verify.md`, `B70-m1` and `B70-m2`): the first draft left the
steady-state wait unbounded, and its deadline `assert!` panicked before the join, which detaches a
thread that spins `while !stop` at full speed until the binary exits — measured at 5.00 s of test
against 6.74 s of process. Neither can happen on a passing run; both make a failing run cheap and
honest.

No production line changed: `Demand`, `lock_for_render`, `render_demand_raised` and
`raise_render_demand` are untouched, and the other four handshake tests are untouched.

### Runs

20 consecutive standalone runs on the idle machine,
`cargo test -p oneterm-terminal --lib a_pump_yields_to_the_demand_within_a_bounded_number_of_chunks`:

```
passed=20 failed=0
chunks_waited: 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0     (bound 8)
while_standing: min 33, max 39                              (bound 81)
```

The counts were read through a temporary `eprintln!` under `--nocapture` and the line was removed
before the commit; a passing Rust test prints nothing, and keeping it would have bought an
artefact this packet does not need. The committed form was then re-run and is what the gate ran.

The same command **before** this packet failed on every standalone run the two verifiers gave it
(3/3, at 65 chunks, and again at 149).

3 runs of `cargo test --workspace`: green, green, green — no `test result: FAILED` line in any of
them.

### Gaps

- **Only the Windows host was run.** The reworked measurement rests on `std::sync::Mutex` being
  unfair and on `thread::sleep` parking the pump, both of which hold on every platform OneTerm
  builds for, but the 20 runs and the mutation probes are on Windows alone. CI runs the test on
  all three.
- ~~**The standing-window bound has a 2x margin here, not 6500x.** 33-39 chunks against 81: the
  slack comes from Windows' sleep granularity making each 250 µs park longer than asked. A host
  with a finer timer would land nearer 80, still under the bound, because the bound is
  `window / park` — an upper bound a sleep can only overshoot. It would be wrong to tighten it to
  the observed 39.~~ **This gap was the defect.** A Linux CI host landed at 100, not "nearer 80",
  and the reasoning is corrected in "Acceptance rework 2026-09-22 (portable bound)".
- **The real pumps are still covered only by their own tests.** This test is about `Demand`, not
  about `crates/local-shell`'s loop; `a_flooding_loop_hands_the_engine_to_a_waiting_frame` remains
  the one that exercises a real read loop, and it was not touched.
- **The mutation probes are not in the repository.** They were temporary edits to `Demand` and to
  the test's own pump, reverted; the numbers above are the record.

## Acceptance rework 2026-09-22 (portable bound)

The packet was accepted on the Windows host and the gap above was accepted with it. The owner's
push then failed on a non-Windows CI job:

```
handle::tests::a_pump_yields_to_the_demand_within_a_bounded_number_of_chunks panicked at crates/terminal/src/handle.rs:454:9:
the pump took 100 chunks in 20ms with a frame waiting (it asked at chunk 200352); a pump that keeps yielding fits 81
```

### Cause

Not the product, and not the primitive the review guessed at either. **The pump parks with
`std::thread::sleep`, not `std::thread::park_timeout`** (`crates/terminal/src/handle.rs`, the
test's own pump thread; the whole workspace contains one `park_timeout`, in
`crates/local-shell/src/session_tests.rs`, unrelated to this handshake). `thread::sleep` is
documented never to sleep *less* than asked, so spurious wakeups are not what happened here and
no product comment needs the caveat.

What broke is the bound's derivation. `parks_in_window = STANDING_WINDOW / PUMP_PARK + 1` is a
ratio of two **nominal** durations, and only the denominator is a guaranteed floor:

1. The window is itself a `sleep` on the *measuring* thread, and what the assertion divides by is
   the wall time between the two `chunks` loads — 20 ms **plus** the time it takes the scheduler
   to run this thread again after the timer fires. On an idle Windows box that is ~0.2 ms (the
   20 runs below measure 20.05-20.57 ms). On a loaded CI runner it is milliseconds. 100 parks of
   250 µs is 25 ms, so a 25 % stretch of the window is all the failure needed.
2. Windows was hiding it. `thread::sleep(250 µs)` there costs ≈ 500-600 µs, so the pump only ever
   reached ~36 of the 81 — the "2x margin" the packet recorded was *Windows timer granularity*,
   not slack in the bound. Linux and macOS timers reach the 250 µs floor, so the same honest pump
   lands at ~80 with **1 % margin**, and any stretch of the window puts it over.

The verification (`docs/spec-intakes/IN-0042-ux-polish-round-1/evidence/acceptance-rework-2-verify.md`
§ "Is 81 an upper bound a sleep can only overshoot?") reached "yes" by arguing that a window can
only overshoot *relatively less* than a park, because relative overshoot falls as the interval
grows. That is true of timer granularity alone; it is false once the window's measured span also
carries the measuring thread's own wake-up latency, which the park's does not — the pump's next
act after waking is an atomic increment. The claim was checked against a host whose park
granularity is 2x the request, which is exactly the host where it cannot fail.

### The new bound

```rust
const STANDING_BOUND: u64 = 2_000;   // absolute; was STANDING_WINDOW / PUMP_PARK + 1 == 81
```

Justified from the measured numbers, not from the timers:

| | chunks in the window |
|---|---|
| Yielding pump, Windows (20 runs, below) | 34-38 |
| Yielding pump, Linux CI | ~100 |
| Mutation: the pump never parks (3 runs) | 519 549 / 524 556 / 541 501 |
| Mutation: `Demand::is_raised` back to the one-shot `swap` (3 runs) | 603 022 / 604 209 / 607 271 |

The two populations are **four orders of magnitude apart**, so the bound only has to land between
them, and 2 000 does it with margin on both sides: >50x above the worst honest observation (2 000
parks is half a second of window — the test would have to be descheduled for half a second to
reach it) and >250x below the cheapest broken one. It proves *"the pump yields"*, which is the
contract in `docs/terminal-backend.md` § 5.1; it deliberately does **not** prove *"the pump sleeps
exactly `PUMP_PARK`"*, which is what the ratio pretended to and no OS owes us.

The failure message now prints the **measured** window (`stood_for`) instead of the nominal
`STANDING_WINDOW`, so a future failure says how long the window really was.

### Runs

- 20 consecutive standalone runs, instrumented (temporary `eprintln!`, reverted before the
  commit): **passed=20 failed=0**, `while_standing` sorted = 34 x7, 35 x8, 36 x4, 38 x1 (min 34,
  max 38, bound 2 000), `stood_for` 20.05-20.57 ms, `chunks_waited` 0 in 18 runs and 1 in the
  other two (bound 8).
- 20 consecutive standalone runs of the **committed** form: passed=20 failed=0.
- Both mutations re-run 3x each on the new bound: **FAIL 3/3** each, tallies in the table above.
- `cargo test -p oneterm-terminal`, `cargo clippy --workspace --all-targets -- -D warnings` and
  `pwsh scripts/ci-local.ps1` — green; the gate's last line is "ci-local: all checks passed".

### Scope of the change

`crates/terminal/src/handle.rs`, test-only again: the constant, the `stood_for` measurement it
reports, and the rustdoc that carries the reasoning. `Demand`, `lock_for_render`,
`render_demand_raised` and `raise_render_demand` are untouched, as is
`docs/terminal-backend.md` § 5.1 — re-read once more and still correct, because it says nothing
about a chunk count.

### Gap that remains

Still verified on Windows only. The new bound no longer depends on which platform's timers are
finer — that is the point of making it absolute — but the Linux and macOS evidence is CI's.

## Handoff

Complete after the 2026-09-22 rework above. Nothing is owed to a next owner: the product is
unchanged, the intake's packet list carries the row, and `docs/terminal-backend.md` needed no
edit.
