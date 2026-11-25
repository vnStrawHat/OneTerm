# Work: The integrity-walk bench measures the runner, not the R-28 bound

ID: BUG-0075
Intake: IN-0029
Created: 2026-09-23

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

- Change type: bug (test defect — the assertion, not the product)
- Risk lane: normal
- Spec Intake, when required: `IN-0029` —
  `docs/spec-intakes/IN-0029-vt-engine/IN-0029.md`, which owns the VT engine, the R-28
  integrity-check budget and the probe that guards it.

## Outcome

`snapshot::bench::integrity_walk_cost_per_feed_and_snapshot_update` asserts the R-28
property — *the integrity walk's cost is bounded to what the operation touched, so the history
depth must not appear in it* — and nothing else. It passes on a loaded 2-vCPU CI runner in a
debug build, and it still fails when the walk goes back to O(history).

## Reported by

A CI run of `cargo test -p oneterm-vt --lib`, `vt-paranoid = false`, debug, on a loaded runner:

```
integrity walk over 100000 history rows, 160x45, vt-paranoid = false:
  feed(one line)  :    1382.4 us
  snapshot_update() :     764.3 us
panicked at crates/vt/src/snapshot/snapshot_bench.rs:143:9: feed cost 1382.4 us is O(history)
```

The walk was not O(history). 1382.4 us is the mean of ten calls whose honest cost is ~141 us
each: one descheduling of a few milliseconds inside the ten-call loop is the whole difference.

**And it had been seen before.**
[`IN-0038`'s `US-0098` verification](../IN-0038-embeddable-vt-core/evidence/US-0098-verify.md)
§ 16 records this same test failing once *"while the machine was compiling in parallel"*, passing
in isolation and on re-run, and files it as *"timing-sensitive, pre-existing, unrelated to this
packet"*. That sighting predates the CI failure and no `BUG` was opened for it. It is direct
support for the diagnosis below — the failure correlates with the machine's load and not with any
change to the walk — and it is also the reason this packet exists: the defect was visible once,
correctly judged out of scope, and then shelved.

This is the same defect class as
[`BUG-0070`](../IN-0032-terminal-crate-tidy/BUG-0070-pump-yield-test-only-passes-under-load.md)
in `crates/terminal` — a test that measured the scheduler instead of the contract — with the
sign flipped: `BUG-0070` only passed while the machine was busy, this one only passes while the
machine is idle.

## Scope

- [ ] In scope:
  - `crates/vt/src/snapshot/snapshot_bench.rs` — the probe and its assertions.
  - `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` § 1 and
    `US-0075` / `US-0079`, if the "asserts a 1 ms ceiling" wording has to change.
- [ ] Out of scope:
  - `Screen::integrity_lo`, `Screen::assert_integrity`, `TerminalGrid::assert_integrity` and the
    `vt-paranoid` feature. The bound itself is correct and is not reopened; only what measures it
    changes.
  - The sibling bench `render_state_build_cost_per_frame` in the same file. It is `#[ignore]`d in
    debug builds and its assertions are ordering comparisons between three numbers taken in the
    same process, which is already the shape this packet moves the other test to.

## Acceptance

- [x] The assertion's verdict does not depend on the machine's load: it compares two measurements
      taken in the same process on the same machine, not a measurement against a wall clock.
- [x] The absolute ceiling that remains is loose enough for a debug build on a loaded 2-vCPU
      runner and tight enough that a whole-history walk still trips it.
- [x] The `eprintln!` report is kept, and gains the near-empty control numbers.
- [x] 20 consecutive runs pass on a loaded machine, with the observed minimum and maximum recorded
      here, and the load is heavy enough to have failed the old assertion.
- [x] A deliberate O(history) regression makes the test fail, with the output recorded here.
- [x] The test stays under 10 s in a debug build.
- [x] `cargo test -p oneterm-vt`, `--features vt-paranoid` and `--no-default-features` pass, as do
      `cargo fmt --all -- --check`, `cargo clippy -p oneterm-vt --all-targets -- -D warnings`,
      `python scripts/check-doc-paths.py` and `python scripts/check-english.py`.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed." (added after the
      verification; the first submission never ran it and failed it — see the rework note).
- [x] The owning docs are amended or recorded as already correct.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` § 1 — R-28, the
  integrity-check budget. It states the contract in the form this packet needs: *"The cost becomes
  O(touched rows) and is flat in the scrollback depth."* Its closing paragraph names this test and
  describes it as asserting **a 1 ms ceiling** with *"three orders of magnitude of margin ... so it
  never fails on a slow machine"*. That last clause is the claim CI falsified, and the sentence
  names a mechanism that is changing, so it must be updated.
- `docs/spec-intakes/IN-0029-vt-engine/US-0079-damage-and-render-state.md`
  § "Rework (2026-09-13) — bounded integrity" — the **contract doc for this probe**: *"The probe
  lives here, next to the tier-3 bench it belongs with"*, and it repeats the 1 ms ceiling. Its
  before/after table (258 615.5 / 252 771.9 us against 142.9 / 150.2 us) is where the 1700x figure
  the new ratio bound is justified from comes from.
- `docs/spec-intakes/IN-0029-vt-engine/US-0075-grid-and-scrollback.md`
  § "Rework (2026-09-13) — bounded integrity" — the grid half: `batch_lo`, `integrity_lo`, why the
  bound is sound, the same measured table with the ratio column (1810x / 1683x), and the same 1 ms
  ceiling sentence.
- `docs/spec-intakes/IN-0032-terminal-crate-tidy/BUG-0070-pump-yield-test-only-passes-under-load.md`
  — the precedent, read for method rather than for contract: establish the root cause before
  touching the assertion, make the measurement discriminate by construction rather than by a
  wall-clock margin, and prove the new bound with a mutation probe.

### Documentation Action

Update required, in three places, all of them the same sentence:

- `low-level-design/testing-and-bench.md` § 1, the "**The guarding test**" paragraph.
- `US-0079-damage-and-render-state.md`, the "**The probe lives here**" bullet.
- `US-0075-grid-and-scrollback.md`, the "The probe asserts a 1 ms ceiling ..." sentence.

Each says the probe "asserts a 1 ms ceiling ... with three orders of magnitude of margin ... so it
never fails on a slow machine". The margin claim was arithmetic on the honest cost (141 us against
1000 us is 7x, not 1000x — the three orders of magnitude are between the *bounded* and the
*unbounded* walk, not between the bounded walk and the ceiling), and the "never fails on a slow
machine" claim is what CI falsified. The mechanism is also changing, from one wall-clock ceiling to
a same-process ratio plus a loose ceiling, so the description would be stale even if the numbers
held.

Reason: the R-28 **contract** is unchanged and correct — the walk is O(touched rows) and flat in
the scrollback depth, which is exactly what the new assertion checks. What changes is the
description of how the guarding test checks it, and that description lives in these three
documents.

### Reconciliation

Three docs changed, all of them the same paragraph and none of them the contract:

- `low-level-design/testing-and-bench.md` § 1 — the "**The guarding test**" paragraph now describes
  the two-probe ratio, the ceiling, and `BUG-0075`.
- `US-0079-damage-and-render-state.md` — the "**The probe lives here**" bullet, same substitution,
  with a pointer to this packet.
- `US-0075-grid-and-scrollback.md` — the "The probe asserts ..." sentence, same substitution.

The R-28 table, the soundness argument for `batch_lo`, the measured before/after tables and the
`vt-paranoid` wiring are untouched: none of them was wrong.

## Context

### The root cause: a wall-clock bound cannot see the property it was asked about

R-28's claim is a *shape* — "flat in the scrollback depth" — and the old test tried to prove it
with a single number:

```rust
let per_feed = fed.as_secs_f64() * 1e6 / f64::from(CALLS);   // the MEAN of 10 calls
assert!(per_feed < 1000.0, "feed cost {per_feed:.1} us is O(history)");
```

Two independent defects, either of which is enough on its own:

1. **The mean of ten samples is not the cost of the operation.** Every sample is `walk +
   whatever the OS did to this thread during it`. The second term is unbounded and is added, never
   subtracted, so the mean drifts up with the machine's load and the *estimator itself* is what CI
   measured. The reported 1382.4 us is 13.8 ms of wall time over ten calls against an honest 1.4 ms
   — one preemption. The same run's `snapshot_update` number, 764.3 us, was under the identical
   bound in the identical process, which is the signature of noise rather than of a regression:
   a walk that had become O(history) would have moved both by three orders of magnitude, not one
   by 10x and the other by 5x.

2. **The bound was wrong about its own margin.** The comment reads *"a couple of hundred
   microseconds when the bound holds ... and hundreds of milliseconds when it does not"*, and then
   bounds at 1000 us — 7x above the honest cost, not the "three orders of magnitude" the LLD
   claims. A debug build on a shared 2-vCPU runner does not owe anyone 7x.

Nothing in that window is a product defect, and the fix is not to raise 1000 to 20 000: that keeps
the same estimator and buys time until a slower runner. The measurement has to discriminate by
construction.

### The measurement that discriminates

Two changes, and no third:

1. **Take the cheapest call, not the mean.** The walk is what *every* call pays; a descheduling is
   what *one* call pays. Over ten samples the minimum is the only estimator a loaded runner cannot
   inflate, and it is the one the regression cannot deflate either — an O(history) walk costs a
   quarter of a second in *every* sample, so its minimum is a quarter of a second.

2. **Run the same probe loop twice in the same process: once over a full 100 000-row history, once
   over a near-empty one, and compare.** This is the property R-28 actually states. Both probes
   build the same 160x45 terminal with the same `scrollback_limit`, fill the screen with the same
   content, and differ in exactly one variable — the number of rows of scrollback behind that
   screen. If the walk is bounded to the rows the operation touched, the two cost the same; if it
   is O(history), they differ by the history depth. The verdict is a ratio between two numbers
   taken on the same machine in the same second, so the runner's speed, the build profile and the
   machine's load cancel out of it.

The absolute ceiling stays, but only as a backstop for a regression that slows *both* probes and
so cannot show up in their ratio, and it is set where a debug build on a loaded runner will never
reach it (tens of milliseconds against an honest ~150 us) while the 250 ms whole-history walk
still does.

### The two populations the bound has to separate

From the measured tables in `US-0075` and `US-0079`, and confirmed on this host:

| | `feed` (one line) | `snapshot_update` | ratio to near-empty |
| --- | --- | --- | --- |
| bounded walk (this host, idle, min of 10) | 144.8 us | 150.3 us | 0.96x / 0.95x |
| bounded walk (this host, contended core, 20 runs) | 138.0-171.7 us | 145.0-177.9 us | 0.64x-1.00x |
| whole-history walk, measured here (see the negative control) | 266 905.7 us | 269 902.1 us | 1170x / 1144x |
| whole-history walk (`vt-paranoid`, `US-0075`'s table) | 258 615.5 us | 252 771.9 us | ~1700x |

The honest ratio is about 1, the broken one is over 1000, and `DEPTH_RATIO = 20.0` sits between
them with 20x of margin below and 58x above. It is not a timing threshold: it does not move when
the machine does.

## Plan

- [x] Read `Screen::integrity_lo`, the probe, and the R-28 text before touching the assertion.
- [x] Decide product defect vs. test defect, and record which in Context.
- [x] Restructure the measurement into a two-probe comparison; leave the bound itself alone.
- [x] 20 runs under a parallel `cargo build`, numbers recorded.
- [x] Negative control: a deliberate O(history) walk must still fail.
- [x] Reconcile the three owning docs.

## Decisions

None. R-28 and the `batch_lo` bound are owned by `US-0075` / `US-0079` and are inherited
unchanged; this packet adds nothing future work must follow beyond the precedent `BUG-0070`
already set.

## Verification Plan

- Focused: `cargo test -p oneterm-vt --lib integrity_walk -- --nocapture`, 20 consecutive runs on a
  loaded machine — a parallel `cargo build --workspace` first, and, since this host has 20 logical
  CPUs and that is not contention, the test and three CPU burners pinned to one logical CPU — with
  the observed per-call minima and ratios, and the old estimator measured alongside the new one on
  the same samples so the two verdicts are comparable.
- Negative control: make `Screen::integrity_lo` return `oldest` again — the O(history) walk,
  without the `vt-paranoid` feature that would switch the assertions off — and confirm the test
  fails.
- Regression: `cargo test -p oneterm-vt`, `cargo test -p oneterm-vt --features vt-paranoid`,
  `cargo test -p oneterm-vt --no-default-features`, `cargo fmt --all -- --check`,
  `cargo clippy -p oneterm-vt --all-targets -- -D warnings`, `python scripts/check-doc-paths.py`,
  `python scripts/check-english.py`.
- **The gate: `pwsh scripts/ci-local.ps1`, end to end, ending in "ci-local: all checks passed."**
  `AGENTS.md` § 4 requires the bundled script and says in as many words: *"Do not report a task
  done with only fmt/clippy/build green."* The first submission of this packet listed the seven
  commands above and not the script, and the script is what catches the rustdoc
  self-containment rule that the first submission broke — see the rework note at the end.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### The change

`crates/vt/src/snapshot/snapshot_bench.rs`, test only. The body of the old test became
`integrity_walk_probe(history: usize) -> (f64, f64)` with two edits — `scrollback_limit` is a
constant rather than the fill depth, so the two probes differ in live history and not in capacity,
and the accumulators are `Duration::MAX` with `.min()` instead of `Duration::ZERO` with `+=`. The
test itself now calls it twice and asserts on the ratio:

```rust
const DEPTH_RATIO: f64 = 20.0;
const CEILING_US: f64 = 20_000.0;

assert!(
    feed_ratio < DEPTH_RATIO,
    "feed cost {full_feed:.1} us over {HISTORY} history rows \
     against {empty_feed:.1} us over almost none, {feed_ratio:.1}x: O(history)"
);
assert!(
    full_feed < CEILING_US,
    "feed cost {full_feed:.1} us, over the {CEILING_US:.0} us ceiling"
);
```

and the same pair for `snapshot_update`. The fill assertion is pinned at **both** ends,
`(history..history + 64).contains(&filled)`, because the one shape that could silently disarm the
ratio is a control whose own history is deep — the quotient would sit at 1.0 for ever, whatever the
walk did. The probe's comment block is plain `//` rather than `///`: it cites `BUG-0075`, and the
crate's published rustdoc must stand alone. Both of those came from the verification (F9, F16).

No production line changed: `Screen::integrity_lo`, `Screen::assert_integrity`,
`TerminalGrid::assert_integrity` and the `vt-paranoid` feature are untouched, and so is the sibling
bench in the same file.

### How the CI runner was reproduced

The host is a 20-logical-CPU Windows box, so a parallel `cargo build --workspace` (on a separate
`CARGO_TARGET_DIR`, to avoid the target-dir lock serialising the two) is not contention: 20 runs
under it never pushed the reported mean past 204 us. **The reported runner has 2 vCPUs**, so the
load was reproduced by *pinning* instead: the test process and three CPU burners all forced onto
logical CPU 0 (`Process.ProcessorAffinity = 1`). That is a harsher environment than the CI runner —
four runnable threads on one core — and it reproduces the reported failure exactly.

### Runs

**The failure, reproduced.** 20 runs on the contended single core, with a temporary `eprintln!`
reporting **both** estimators per probe — the old mean of ten calls and the new minimum — so the
two verdicts are read off the *same* measurements. The instrumentation was reverted before the
commit (`BUG-0070` did the same; a passing Rust test should print nothing it does not need to).

```
passed=20 failed=0            (the new assertion)

                    min       max
feed   mean       141.1    1899.1     <- the OLD estimator and its 1000 us bound
feed   min        138.0     171.7     <- the NEW estimator
render mean       149.2    1873.0
render min        145.0     177.9
near-empty min    154.6     247.3 (feed) / 160.8 - 255.0 (snapshot_update)
feed   ratio        0.64      0.99     (bound 20)
render ratio        0.67      1.00     (bound 20)
wall per run       1.34 s    2.77 s
```

The old bound would have failed **2 of these 20 runs** — run 3 at `feed_mean = 1899.1 us` and run
15 at `render_mean = 1873.0 us` — against the CI report's 1382.4 us, on a machine where the walk
was bounded the whole time. The same two runs' minima are 152.8 us and 151.6 us, and their ratios
0.79x and 0.94x. That is the defect and the fix in one table: the mean moved 13x, the minimum
moved 1.1x, and the ratio did not move at all.

**The committed form**, same contended single core, 20 consecutive runs:

```
passed=20 failed=0
feed    full: min 139.4 us  max 156.2 us
feed   empty: min 141.0 us  max 178.9 us
feed   ratio: min 0.82x     max 1.02x     (bound 20)
render  full: min 147.0 us  max 164.5 us
render empty: min 160.0 us  max 184.8 us
render ratio: min 0.85x     max 1.00x     (bound 20)
wall per run: 1.26 s - 2.97 s
```

The ratio never left 0.82x-1.02x, a factor of 20 inside its bound.

**On the ratio sitting just below 1.** The near-empty probe is consistently the *dearer* of the
two, by about 8-13 %. The first draft of this packet called that "the second probe paying a cold
cache"; the verification falsified it by swapping the two calls — the control stays ~1.13x dearer
whichever one runs first, so the effect is systematic and order-independent, not a warm-up
artefact (`evidence/BUG-0075-verify.md` F2). The likeliest cause, read from the code and **not**
measured, is allocation: the full probe's history is already at `scrollback_limit`, so each timed
`feed` scrolls a row out and recycles it, while the control's history is still growing from ~20
rows and allocates a row per call. That remains a hypothesis. What matters for the assertion is
settled either way: a systematic 1.13x tilt sits 17x from the 20x bound and cannot conceal a
regression that moves the ratio by three orders of magnitude.

Idle, for the record, `cargo test -p oneterm-vt --lib integrity_walk -- --nocapture`:

```
integrity walk over 100000 history rows, 160x45, vt-paranoid = false:
  feed(one line)  :     144.8 us  (  150.7 us near-empty, 0.96x)
  snapshot_update() :     150.3 us  (  157.7 us near-empty, 0.95x)
test result: ok. 1 passed; 0 failed; ... finished in 0.79s
```

### Negative control

`Screen::integrity_lo` temporarily reduced to an unconditional `return self.oldest;` — the
O(history) walk, with the `vt-paranoid` feature **off** so the assertions still run — reverted
immediately afterwards:

```
integrity walk over 100000 history rows, 160x45, vt-paranoid = false:
  feed(one line)  :  266905.7 us  (  228.1 us near-empty, 1170.13x)
  snapshot_update() :  269902.1 us  (  235.9 us near-empty, 1144.14x)

thread 'snapshot::bench::integrity_walk_cost_per_feed_and_snapshot_update' (9668) panicked at
crates\vt\src\snapshot\snapshot_bench.rs:122:9:
feed cost 266905.7 us over 100000 history rows against 228.1 us over almost none, 1170.1x: O(history)
test snapshot::bench::integrity_walk_cost_per_feed_and_snapshot_update ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 564 filtered out; finished in 6.58s
```

1170x against a bound of 20, and 266 906 us against a ceiling of 20 000: both halves catch it
independently, the ratio with 58x of margin. The near-empty probe stayed at 228.1 us, which is the
control doing its job — the regression is in the history, and only the history-bearing probe sees
it. Note that this is a *minimum* of ten calls, so the regression is not a sampling artefact: the
O(history) walk costs a quarter of a second in every one of them.

`git checkout -- crates/vt/src/grid/screen.rs` afterwards; `git diff --stat` lists
`crates/vt/src/snapshot/snapshot_bench.rs` only.

### Gates

| Command | Result |
| --- | --- |
| `cargo test -p oneterm-vt` | **837 passed / 0 failed / 4 ignored** across 20 suites, 32.3 s |
| `cargo test -p oneterm-vt --features vt-paranoid` | **837 passed / 0 failed / 4 ignored**, 42.2 s |
| `cargo test -p oneterm-vt --no-default-features` | **806 passed / 0 failed / 4 ignored** |
| `cargo fmt --all -- --check` | clean, exit 0 |
| `cargo clippy -p oneterm-vt --all-targets -- -D warnings` | clean |
| `python scripts/check-doc-paths.py` | "Doc path check passed for 207 current paths in 11 documents." |
| `python scripts/check-english.py` | "English contributor-text check passed for 1002 files." |
| `pwsh scripts/ci-local.ps1` (`CARGO_BUILD_JOBS=6`) | **"ci-local: all checks passed."** — 4781 passed / 0 failed / 29 ignored across 135 suites, 8 min |

The `ci-local.ps1` row is the one that matters and the one the first submission of this packet
did not have; it failed then and passes now. See the rework note at the end.

Runtime, against the 10 s budget this packet was given: **0.79 s** idle, 1.26-2.97 s on the
contended single core. Under `--features vt-paranoid` the whole crate suite is 42.2 s against 40 s
before — the near-empty probe's ten extra whole-history walks are over ~19 rows, so they cost
microseconds; what the second probe adds there is its own 64-line fill, not a second 100 000-row
one.

### Gaps

- **Only the Windows host was run.** The new verdict is a ratio between two loops in one process,
  so it no longer depends on any platform's clock resolution or scheduler — that is the point of
  the change — but the 20 runs, the mean-vs-min instrumentation and the negative control are on
  Windows alone. CI runs `cargo test -p oneterm-vt --lib` on all three platforms.
- **The 2-vCPU runner was simulated by processor affinity, not rented.** Four runnable threads on
  one logical CPU is harsher than 2 vCPUs and it reproduced the reported failure mode (a mean of
  1899.1 us against the reported 1382.4 us), but it is a simulation: SMT siblings, cache pressure
  and a hypervisor's steal time are not modelled.
- **`CEILING_US = 20_000.0` is a judgement, not a measurement.** It sits 116x above the worst
  honest observation here (171.7 us), 10x above the worst *mean* the contended core produced
  (1899.1 us), and 13x below the cheapest broken one (266 906 us). If a future runner ever trips
  it while the ratio stays near 1, the ceiling is the number to raise — the ratio is the assertion
  that carries the contract.
- **The minimum hides a genuine constant-factor slowdown in the walk.** A change that made the
  bounded walk 5x more expensive without making it O(history) would pass both halves, because both
  probes would pay it. R-28 budgets the *shape*, not a constant, and the numbers stay in the log
  for anyone reading them; gating a constant factor would put a wall-clock threshold back in,
  which is this packet's defect. Measured by the verification: a walk pulled back a constant 180
  rows costs 456.8 us and passes at 2.15x — correctly.
- **There is a sensitivity floor, and the retired bound was below it.** The verification ran the
  *partial* walks this packet did not (`evidence/BUG-0075-verify.md` F4). A walk over 10 % of the
  history fails loudly (24 494.8 us, 157.93x — both halves). A walk over **1 %** — a real
  O(history) regression, 2 345.9 us per `feed` in debug, and ten times that for a user running a
  1 000 000-row scrollback — **passes** at 15.09x. The discrimination floor for this configuration
  is a walk over roughly **1.3 %** of 100 000 rows, about 1 300 rows or ~3 ms; below it the test is
  blind. Two consequences worth stating plainly:
  - The retired 1 ms bound **would** have caught the 1 % case (2 345.9 us against 1 000 us). So the
    fix trades absolute sensitivity for stability. That is the right trade — the old sensitivity
    was unusable, because the same bound also fired on a descheduled thread with the walk bounded
    the whole time, which is the defect this packet exists for — but it is a trade, not a free
    win, and the ceiling is where to recover some of it if a partial-walk regression ever ships.
  - `DEPTH_RATIO` is a *fraction of the configured `HISTORY`*, not an absolute: `< 20x` means "does
    not touch more than about 2 % of 100 000 rows". Raising `HISTORY` tightens the test; lowering
    it loosens it.
- **Ten calls is unchanged.** A minimum over ten samples was enough on every run here (the spread
  between the ten was 1.1x even on the contended core), but it was not tuned; if a future host
  ever shows all ten samples descheduled, raising `CALLS` costs `vt-paranoid` a quarter of a
  second per extra call and nothing elsewhere.

## Rework after independent verification (2026-09-23)

`docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0075-verify.md` verified `894b37d2` and returned
**FAIL on the gate, PASS on the engineering**: it reproduced the root cause from scratch (1414.7 us
and 1652.3 us means against 138.2 us and 144.5 us minima, 2 of 10 contended runs that the retired
bound would have failed), confirmed the control is the same work minus history depth, confirmed the
verdict does not depend on probe order, and re-ran the whole-history mutation itself. One blocker
and five findings; all are closed here.

### F16, the blocker — the commit failed `scripts/ci-local.ps1`

```
==> rustdoc self-containment (crates/vt/src)
...\crates\vt\src\snapshot\snapshot_bench.rs:151:/// every sample (`BUG-0075`).
ci-local: FAILED: the crate rustdoc cites a document only this repository has
exit 1
```

`oneterm-vt` ships as a git dependency, so `scripts/ci-local.ps1` and the `vt-package` job in
`.github/workflows/ci.yml` forbid a `US-`/`BUG-`/`DEC-`/`IN-` citation or a bare `crates/` or
`docs/` path in any `///` or `//!` text under `crates/vt/src`: a vendored copy has none of those
documents. The rework promoted the probe's explanation from a `//` block — where the old code kept
its citations, and where the rule does not reach — to a `///` doc comment, and carried the citation
in with it.

Fixed by demoting the whole block back to `//` rather than by deleting the citation. That is the
root-cause form: `integrity_walk_probe` is a private helper in a `#[cfg(test)]` module, it owes no
rustdoc, and with the block at `//` no future edit to it can break the rule either. A comment on
the block says so, so the next person does not promote it again.

**The process failure behind it is the real finding.** The packet's Gates table listed seven
commands and not `ci-local.ps1`, the Verification Plan listed the same seven, and the
`HARNESS:PROOF` block ticked "Verify command passed" on that basis. `AGENTS.md` § 4 requires the
bundled script and says exactly this: *"Do not report a task done with only fmt/clippy/build
green."* The omission was in the plan, not in the running — which is why it survived to a commit.
`pwsh scripts/ci-local.ps1` is now in both the Verification Plan and the Gates table, with its
output.

### The other five

| Finding | What changed |
| --- | --- |
| **F2** — "cold cache" is falsified; the control is ~1.13x dearer whichever probe runs first | The Runs section now records the tilt as systematic and order-independent, with row recycling at the scrollback limit versus fresh allocation in the control as an **unproven** hypothesis read from the code |
| **F4** — a walk over ~1 % of history passes (15.09x, 2 345.9 us); the floor is ~1.3 % | Added to Gaps, with the 10 % case that does fail (157.93x), the fact that the **retired bound would have caught the 1 % case**, and that `DEPTH_RATIO` is a fraction of `HISTORY` and not an absolute |
| **F9** — the control's `>= history` assertion is vacuous | Pinned at both ends; see "The change" |
| **F10** — the amended `US-0079` bullet and `US-0075:553` name `crates/vt/src/render/render_bench.rs`, which does not exist | Both now name `crates/vt/src/snapshot/snapshot_bench.rs`, with the rename noted so the historical record still reads correctly |
| **F15** — the flake was recorded once before and not cited | `IN-0038`'s `US-0098` verification § 16 is now cited in "Reported by" |

Not closed here: **F13**, the missing `harness.db` row. This task was told not to touch the
database. The verification read the schema and drafted the row — `intake_id = 34`, `risk_lane =
'normal'`, `status = 'implemented'`, proof flags `(1, 0, 0, 0)`, `packet_doc` this file, `evidence`
the verification file, `verify_command` `cargo test -p oneterm-vt --lib integrity_walk` — and it is
owed by whoever owns the database.

### Verification of the rework

`cargo test -p oneterm-vt --lib integrity_walk` passes (0.77 s; feed 133.4 us against 151.2 us
near-empty, 0.88x; `snapshot_update` 142.9 us against 154.7 us, 0.92x). The rustdoc grep from
`scripts/ci-local.ps1:133-135`, run on its own against `crates/vt/src`, returns nothing.
`cargo fmt --all -- --check`, `python scripts/check-doc-paths.py` and
`python scripts/check-english.py` are clean. And the gate, end to end with `CARGO_BUILD_JOBS=6`:
**4781 passed / 0 failed / 29 ignored across 135 suites, 8 minutes, final line "ci-local: all
checks passed."**

## Handoff

Complete after the rework above. The product is unchanged, the three owning docs are reconciled,
and the only thing owed to a next owner is the `harness.db` row (F13), which this task was not
permitted to write.
