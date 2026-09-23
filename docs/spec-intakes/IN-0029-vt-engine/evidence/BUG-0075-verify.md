# Evidence: independent verification of BUG-0075

Subject: `894b37d2` on `fix/vt-bench-bound`, one commit on top of `main @fc1ade7c`.
Packet: `docs/spec-intakes/IN-0029-vt-engine/BUG-0075-integrity-walk-bench-measures-the-runner.md`.
Verifier host: Windows 11, 20 logical CPUs, MSVC target, debug builds throughout.
Date: 2026-09-23.

## Verdict

**FAIL — for one reason, and it is a one-line fix: `894b37d2` does not pass
`scripts/ci-local.ps1`.** The new `///` doc comment on `integrity_walk_probe` cites
`` `BUG-0075` ``, which the mandatory "published rustdoc must stand alone" grep forbids in
`crates/vt/src`. The same grep is a step in the `vt-package` job of `.github/workflows/ci.yml`,
so this commit turns the CI job red at the step after the one that was already failing. **F16
has the output and the fix.** The packet's Gates table does not list `ci-local.ps1` at all,
which is the one gate that catches it, while `AGENTS.md` § 4 says in as many words: "Do not
report a task done with only fmt/clippy/build green."

**Everything else in the packet verifies.** The diagnosis is right and I reproduced its
central claim independently: on a contended core the old estimator produced 1414.7 us and
1652.3 us on runs whose *minimum* was 138.2 us and whose ratio was 0.90x, so 2 of 10 runs
would have failed the retired 1000 us bound while the walk was bounded the whole time. The
near-empty control is the same work minus history depth and nothing else, the verdict does not
depend on which probe runs first, a whole-history walk and a 10 % partial walk both fail
loudly, a merely slower bounded walk rightly passes, `vt-paranoid` still runs the probes and
skips the assertions, the test stays under a second, the routing is correct, and with the one
offending citation removed the whole gate is green. Remove the citation and this is a PASS.

Four further findings are worth the owner's attention; none of them changes the fix. A walk
that touches about 1 % of the history passes both halves (F4) and the packet's Gaps list does
not name that blind spot; the control assertion is vacuous and would not notice a future edit
that made both probes deep (F9); the packet's explanation for the ratio sitting just below 1
is falsified by swapping the probe order (F2); and no `harness.db` row exists for `BUG-0075`
(F13).

## Findings

### F1 — Confirmed: the control is the same work minus history depth (severity: info, verified)

Instrumented `integrity_walk_probe` to report what each probe actually built, then reverted:

```
PROBE history=100000 history_len=100000 rows=45 cols=160 viewport_rows=45
PROBE history=0      history_len=20     rows=45 cols=160 viewport_rows=45
  feed(one line)  :     143.5 us  (  158.1 us near-empty, 0.91x)
  snapshot_update() :     148.8 us  (  163.8 us near-empty, 0.91x)
```

Read from the source, the two probes differ in exactly one input:

* **Geometry** — both `Size { rows: 45, cols: 160 }`, built inside the probe. Confirmed above.
* **`scrollback_limit`** — `SCROLLBACK = 100_000` is now a constant in the probe rather than
  the fill depth, so the two differ in *live* rows, not in capacity. This is the edit that
  makes the control honest and it is the right one.
* **Screen content** — the fill is `0..history + 64`, so both end with a full 45-row screen of
  `row N` lines plus the cursor row. The walk visits `row.cells()`, whose length is `cols`
  for every allocated row, so the digit count of `N` (2 vs 6) does not change the work.
* **Batch and snapshot state** — each probe constructs its own `Terminal`, `EventBatch` and
  `SnapshotState`, so nothing carries from the full probe into the control. Within a probe the
  fill's damage is drained by the `snapshot_update` + `map_colors` that precede the timing
  loop, and in any case the first sample's extra cost is discarded by the minimum.

No bias that could hide an O(history) walk: an under-filled control makes the quotient
*larger*, which is a false failure, not a false pass. The one shape that could hide a
regression — a control that is itself deep — is the subject of F9.

### F2 — Probe order does not matter, but the packet's reason for ratio < 1 is wrong (severity: low, doc accuracy)

Ran both orders. Committed order (full, then near-empty), idle, 3 runs: 0.95x / 1.04x,
0.91x / 0.91x, 0.94x / 0.93x. Order swapped (near-empty first), idle, 5 runs:

```
feed 140.3 us (157.6 near-empty, 0.89x)   snapshot_update 150.1 us (168.7, 0.89x)
feed 146.5 us (160.7, 0.91x)              snapshot_update 150.6 us (165.7, 0.91x)
feed 140.6 us (155.0, 0.91x)              snapshot_update 146.0 us (159.6, 0.91x)
feed 139.7 us (155.9, 0.90x)              snapshot_update 144.6 us (162.5, 0.89x)
feed 133.9 us (153.1, 0.87x)              snapshot_update 140.3 us (163.4, 0.86x)
```

The near-empty probe is 8-13 % more expensive **whichever probe runs first**, so the packet's
sentence — "the full-history probe costing marginally less than the near-empty one ... is the
two probes being the same measurement twice, plus the second one paying a cold cache" — is
falsified: swapping the order does not swap which probe is dearer. The effect is systematic,
not noise. The likeliest cause (read from the code, not measured): the full probe's history is
already at `scrollback_limit`, so each timed `feed` scrolls one row out and recycles it, while
the control's history is still growing from 20 rows and allocates a row per call.

Consequence for the assertion: none. A 1.13x systematic tilt sits 17x away from the 20x bound
and cannot conceal a regression that moves the ratio by three orders of magnitude. Only the
explanatory sentence in the packet is wrong, and it should say "systematic, order-independent,
about 10 %, cause unproven" instead of "cold cache".

### F3 — Confirmed: the whole-history negative control fails, both halves independently (severity: info, verified)

Reproduced here rather than taken from the packet. `Screen::integrity_lo` forced to
`return self.oldest;` with `vt-paranoid` **off**, reverted with `git checkout` afterwards:

```
  feed(one line)  :  250393.8 us  (  345.4 us near-empty, 724.94x)
  snapshot_update() :  248679.5 us  (  343.8 us near-empty, 723.33x)
panicked at crates\vt\src\snapshot\snapshot_bench.rs:122:9:
feed cost 250393.8 us over 100000 history rows against 345.4 us over almost none, 724.9x: O(history)
test result: FAILED. 0 passed; 1 failed; ... finished in 7.48s
```

724.9x against a bound of 20 (36x of margin) and 250 394 us against a ceiling of 20 000
(12.5x). The packet reports 1170x on the same mutation; the difference is the control's own
cost under the mutation (345.4 us here, 228.1 us there), which is exactly what should differ
between hosts. Both halves catch it on their own, as claimed.

### F4 — A walk touching about 1 % of the history passes both halves (severity: low-medium, unrecorded gap)

The packet proves the *whole*-history case. I ran the partial cases it does not. `integrity_lo`
pulled back a fraction of the live depth instead of to `oldest`:

| Mutation | feed | ratio | verdict |
| --- | --- | --- | --- |
| walk 10 % of history (`newest - depth/10`) | 24 494.8 us | 157.93x | **FAIL** (both halves) |
| walk 1 % of history (`newest - depth/100`) | 2 345.9 us | 15.09x | **PASS** |

The 10 % walk fails loudly (`157.9x: O(history)`, and 24 495 us over the 20 ms ceiling). The
1 % walk — a genuine O(history) regression, 2.3 ms per `feed` in debug, and 23 ms per `feed`
for a user running a 1 000 000-row scrollback — passes both the ratio and the ceiling. The
discrimination floor for this configuration is a walk over roughly 1.3 % of 100 000 rows
(~1 300 rows, ~3 ms): below it the test is blind.

Two things follow that the packet should say and does not:

1. The ratio bound is a *fraction of the configured `HISTORY`*, not an absolute: `< 20x`
   means "does not touch more than about 2 % of 100 000 rows". Raising `HISTORY` tightens it;
   lowering it loosens it.
2. The **old** bound would have caught the 1 % case (2 345.9 us against 1 000 us). The fix
   therefore trades absolute sensitivity for stability. That is the right trade — the old
   sensitivity was unusable because the same bound also fired on a descheduled thread, as F8
   shows — but it is a trade, and the Gaps section records only the constant-factor blind spot
   (F5), not this one. One line in Gaps closes it.

### F5 — A merely slower bounded walk rightly passes (severity: info, verified)

`integrity_lo` pulled back a *constant* 180 rows (four extra screens, flat in the depth):

```
  feed(one line)  :     456.8 us  (  212.3 us near-empty, 2.15x)
  snapshot_update() :     465.0 us  (  211.7 us near-empty, 2.20x)
test result: ok.
```

About 3x more expensive, still bounded, still passes — 2.15x against a bound of 20 and 457 us
against a ceiling of 20 000. This is the behaviour the packet's fourth gap predicts, and it is
the correct one: R-28 budgets the shape, not the constant. Note the ratio is 2.15x rather than
1.0x, because a constant extra depth is real history in the full probe and clamps to `oldest`
in the control — so the bound has ~9x of headroom left for this class, not ~20x.

### F6 — `CEILING_US = 20_000` is unreachable by an honest debug runner (severity: info, verified)

Instrumented the fill, then reverted:

```
FILL history=100000 took 485.3 ms
FILL history=0      took   0.4 ms
```

485.3 ms for 100 064 rows of parse-and-scroll is this host's debug speed; one timed call costs
137-178 us. Reaching 20 000 us honestly needs a runner about 130x slower per call. At that
speed the same test's own fill is ~63 s and the `--features vt-paranoid` run of this test
(7.71 s here, F7) is about 16 minutes, so the packet's own 10 s runtime acceptance criterion
and the CI job's patience both break long before the ceiling does. The ceiling is safe and,
exactly as the packet says, correspondingly loose: the ratio is the assertion carrying R-28.

### F7 — `vt-paranoid` runs both probes and skips the assertions (severity: info, verified)

```
$ cargo test -p oneterm-vt --lib integrity_walk --features vt-paranoid -- --nocapture
  feed(one line)  :  263695.8 us  (  277.3 us near-empty, 950.94x)
  snapshot_update() :  262009.0 us  (  281.7 us near-empty, 930.10x)
test result: ok. 1 passed; ... finished in 7.71s
```

The `#[cfg(not(feature = "vt-paranoid"))]` block is the only guard, so the eprintln report —
including the new near-empty columns — is produced in both configurations and nothing is
asserted under the feature. Confirmed. The added cost of the second probe under the feature is
its own 64-row fill plus twenty ~280 us calls, i.e. milliseconds, not another quarter-second
walk; the packet's claim that the feature-on suite went 40 s to 42.2 s is consistent with that.

### F8 — Independent reproduction of the root cause, and the 10 contended runs (severity: info, verified)

Contention was produced by pinning: the test binary and CPU burners all forced onto logical
CPU 0 via `Process.ProcessorAffinity = 1`, this host having 20 logical CPUs. The committed
binary, 3 burners, 10 consecutive runs, `--test-threads=1`:

| | min | max |
| --- | --- | --- |
| `feed`, full history | 137.1 us | 141.1 us |
| `feed`, near-empty | 138.4 us | 153.0 us |
| `feed` ratio (bound 20) | 0.91x | 0.99x |
| `snapshot_update`, full history | 143.3 us | 147.2 us |
| `snapshot_update`, near-empty | 150.7 us | 160.5 us |
| `snapshot_update` ratio (bound 20) | 0.91x | 0.96x |
| wall per run | 1.04 s | 2.79 s |

10 of 10 passed. Then the same thing with 7 burners and a temporary instrument reporting the
**old** estimator and the new one from the *same* samples, reverted afterwards:

| | min | max |
| --- | --- | --- |
| `feed` min (new estimator) | 137.6 us | 142.2 us |
| `feed` **mean** (old estimator, bound 1000) | 140.8 us | **1414.7 us** |
| `snapshot_update` min | 144.2 us | 147.8 us |
| `snapshot_update` **mean** (old estimator) | 145.8 us | **1652.3 us** |
| `feed` ratio | 0.87x | 0.91x |
| `snapshot_update` ratio | 0.89x | 0.92x |
| wall per run | 1.31 s | 3.52 s |

10 of 10 passed the committed assertion. Run 6 produced `feed_mean = 1414.7 us` with
`feed_min = 138.2 us` and a ratio of 0.90x; run 7 produced `render_mean = 1652.3 us` with
`render_min = 144.5 us` and a ratio of 0.91x. **The retired bound would have failed 2 of these
10 runs** on a host where the walk was bounded throughout, and 1414.7 us is within 3 % of the
1382.4 us CI reported. That is the packet's root-cause claim, reproduced from scratch: the mean
moved 10x, the minimum moved 1.03x, the ratio did not move.

Determinism and isolation: the probe owns every object it touches (`Terminal`, `EventBatch`,
`SnapshotState`, `Palette` are locals), reads `cfg!(feature = "vt-paranoid")` and mutates no
global, so there is no shared static and no ordering dependence on other tests; the whole
`oneterm-vt` suite is green in all three feature configurations (F14). Runtime is 0.78-0.86 s
idle and 1.04-3.52 s on the contended core, against the packet's 10 s budget.

### F9 — The control's fill assertion is vacuous (severity: low, test hygiene)

The old code pinned the fill exactly:

```rust
assert_eq!(term.grid().primary().history_len() as usize, HISTORY);
```

the new one relaxes it to `>= history`, which for the control (`history = 0`) is satisfied by
*any* depth, including 100 000. The measured control depth today is 20 (F1), so the test is
sound as written. But the single shape that could silently disarm this test is a control that
is itself deep — the ratio would be a constant 1.0 and the assertion would pass forever — and
that is the one shape the assertion no longer excludes. Cheapest fix, one line, no new
concept: bound the control too, e.g.

```rust
assert!(
    (history..history + 64).contains(&(term.grid().primary().history_len() as usize)),
    "the history did not fill"
);
```

Not a defect in what shipped; a guard that the rework dropped and that costs nothing to keep.

### F10 — The amended `US-0079` bullet still names a file that does not exist (severity: low, doc accuracy)

`US-0079-damage-and-render-state.md:411-412`, the bullet this commit edits, still reads
`render::bench::integrity_walk_cost_per_feed_and_render_update` in
`crates/vt/src/render/render_bench.rs`. There is no `crates/vt/src/render/` directory — the
module was renamed to `snapshot` — and the test is
`snapshot::bench::integrity_walk_cost_per_feed_and_snapshot_update`. The same stale name sits
at `US-0075-grid-and-scrollback.md:553`. Both are pre-existing, and the LLD copy of the same
paragraph gets the path right, but the implementer rewrote that exact sentence and left the
wrong file name inside it. `scripts/check-doc-paths.py` passes because it does not resolve
inline code spans of that shape, so nothing else will catch it.

### F11 — The three doc edits are accurate and the R-28 contract text is untouched (severity: info, verified)

Read against the code rather than against the packet:

* `low-level-design/testing-and-bench.md` — the diff touches only the "**The guarding test**"
  paragraph. The R-28 rule table, the "Why that bound is sound" argument, the Measured table
  (258 615 / 252 772 against 143 / 150) and the "CI runs both tiers" paragraph are unchanged.
  Every claim in the new text checks out: probe run twice in one process, same geometry, same
  scrollback limit, same screen content (F1); `DEPTH_RATIO = 20.0` and `CEILING_US = 20_000.0`
  match the source; "the *cheapest* of ten calls" matches `Duration::MAX` + `.min()`;
  "debug only" matches the unchanged `#[cfg_attr(not(debug_assertions), ignore = ...)]`.
* `US-0079` — strike-through on the 1 ms ceiling clause with the replacement text, the
  before/after table left alone. Accurate apart from F10.
* `US-0075` — strike-through on the "asserts a 1 ms ceiling" sentence only; the R-28 table, the
  1810x / 1683x ratio column and the `batch_lo` soundness argument are untouched.

The arithmetic the packet rests on is correct: 1000 us against an honest 141 us is 7.1x, not
three orders of magnitude, and the three orders of magnitude in the retired wording are the
distance between the bounded and unbounded walks, which is what the ratio now asserts. A repo
grep finds no surviving "1 ms ceiling" claim outside the packet's own quotations of it.

### F12 — Routing and packet completeness are correct (severity: info, verified)

`docs/HARNESS.md` "Route the Work" reserves acceptance rework for "work that was just built but
not yet accepted"; a new BUG is "for defects found in behavior that was already accepted or
shipped". This probe shipped with the `US-0075` / `US-0079` rework on 2026-09-13 and has gated
CI since, so a new BUG is right and reopening `US-0075` / `US-0079` would have been wrong.
`IN-0029` owns the VT engine, R-28 and this test, so the intake is right too.

Against `docs/templates/work.md`: every section is present in order, `Created: 2026-09-23`
matches the commit date, both `HARNESS:STATUS` and `HARNESS:PROOF` marked blocks are intact and
consistent with a test-only unit change (`Unit proof`, `Verify command passed`), Owning Docs
Reviewed names four documents with what each one contracts, Documentation Action commits to
"update required" in three named places before the code section, Reconciliation lists the three,
Evidence carries real command output rather than prose, and Gaps names five. The extra
"Reported by" heading is not in the template; it is useful and harmless. The one substantive
completeness gap is F4's missing gap line.

### F13 — No `harness.db` row exists for `BUG-0075`, and the packet proposes none (severity: low, records)

The packet contains no SQL — grepped for `sqlite`, `INSERT INTO`, `harness.db`, `harness story` —
so there is no proposed row to check against the schema. Read-only inspection of
`D:\TrungKFC-Research\Rust\myTerm2\harness.db` (copied, opened `mode=ro`, not written):
`story` has the expected 17 columns — `id, title, created_at, risk_lane, contract_doc,
packet_doc, status, unit_proof, integration_proof, e2e_proof, platform_proof, evidence,
verify_command, last_verified_at, last_verified_result, notes, intake_id` — and `BUG-0070`
through `BUG-0074` each have a row while `BUG-0075` has none. `HARNESS.md` makes `harness.db`
authoritative for status and proof state, so a row is owed. From the sibling rows, it would be
`intake_id = 34` (IN-0029, read off the `BUG-0074` row), `risk_lane = 'normal'`,
`status = 'implemented'`, proof flags `(1, 0, 0, 0)` to match the packet's marked block,
`packet_doc` the packet, `evidence` this file, and `verify_command`
`cargo test -p oneterm-vt --lib integrity_walk`. Not written: this task forbids touching the
database.

### F14 — `cargo test -p oneterm-vt` is green in all three configurations (severity: info, verified)

See "Gate" below. The three crate-level commands reproduce the packet's counts exactly:
837 / 837 / 806 passed, 0 failed, 4 ignored, across 20 suites each.

### F15 — The same flake was recorded once before and the packet does not cite it (severity: info)

`docs/spec-intakes/IN-0038-embeddable-vt-core/evidence/US-0098-verify.md` § 16 records this
exact test failing once "while the machine was compiling in parallel", passing in isolation,
and calls it "timing-sensitive, pre-existing, unrelated to this packet". That sighting predates
the CI failure and is direct support for the packet's "estimator, not regression" argument; the
"Reported by" section names only the CI run. Citing it would have made the diagnosis stronger,
and it shows the defect was visible and shelved once before.

### F16 — BLOCKING: the commit fails `scripts/ci-local.ps1` and would fail CI (severity: high, verified)

Found by running the gate the packet does not list. Step "rustdoc self-containment
(crates/vt/src)":

```
==> rustdoc self-containment (crates/vt/src)
...\crates\vt\src\snapshot\snapshot_bench.rs:151:/// every sample (`BUG-0075`).
ci-local: FAILED: the crate rustdoc cites a document only this repository has
exit 1
```

The rule, from `scripts/ci-local.ps1:126-139` and, identically, from
`.github/workflows/ci.yml:243-249` (job `vt-package`, step "Published rustdoc must stand
alone"): no `US-0NNN`, `BUG-0NNN`, `DEC-0NNN` or `IN-0NNN` citation and no bare `crates/` or
`docs/` path in any `///` or `//!` text under `crates/vt/src`, because `oneterm-vt` is consumed
as a git dependency and a vendored copy has none of those documents. A `https://github.com/`
link is the one allowed form.

The commit introduces exactly one violation. The old code kept its `BUG`/`US` citations in
plain `//` comments, which the rule does not touch; the rework promoted the probe's
explanation to a `///` doc comment and carried the citation into it. Note the same commit's
`//` block comment above the test also cites `BUG-0075` and is correctly ignored, so the rule
is not asking for the reference to be dropped — only for it to stay out of rustdoc.

Fix, one line, in `crates/vt/src/snapshot/snapshot_bench.rs:151`: drop the parenthetical, or
demote the whole `///` block to `//` (`integrity_walk_probe` is a private test helper, so it
owes rustdoc nothing and `#![warn(missing_docs)]` does not apply to it). I applied the first
form in my working tree and re-ran the whole of `scripts/ci-local.ps1`, which then reached its
final line clean — see "Gate". The change was reverted; this verification commit carries only
this file.

Two process points follow, and they are the reason this is severity high rather than low:

1. The packet's **Gates table lists seven commands and `ci-local.ps1` is not among them** —
   `cargo test` x3, `cargo fmt`, `cargo clippy -p oneterm-vt`, `check-doc-paths.py`,
   `check-english.py`. Each of those does pass. `AGENTS.md` § 4 requires the bundled script
   and says "Do not report a task done with only fmt/clippy/build green"; the packet's own
   Verification Plan repeats the same seven-command list, so the omission was planned, not
   forgotten in the running.
2. The `HARNESS:PROOF` block ticks **"Verify command passed"**, and `docs/HARNESS.md`'s
   completion contract says not to report a change complete while proof is failing. On the
   gate the project mandates, it is failing.

## What could not be verified

* **Non-Windows runners.** Every number here is Windows 11 / MSVC, as in the packet. The new
  verdict is a quotient of two loops in one process, so it no longer depends on a platform's
  clock resolution or scheduler, but the mutations and the contended runs are single-platform.
  CI's three-OS `cargo test -p oneterm-vt --lib` remains the platform proof.
* **A real 2-vCPU runner.** Reproduced by affinity pinning, as the packet did. Four to eight
  runnable threads on one logical CPU is harsher in scheduling terms but does not model SMT
  siblings, a hypervisor's steal time, or a cold page cache.
* **The `harness.db` row.** Read-only check only (F13); writing it is out of scope here.

## Gaps the owner should close

1. **Blocking:** remove the `` `BUG-0075` `` citation from the `///` comment at
   `crates/vt/src/snapshot/snapshot_bench.rs:151`, then re-run `pwsh scripts/ci-local.ps1`
   (F16). Add that command to the packet's Gates table and Verification Plan.
2. Add F4's blind spot to the packet's Gaps: a walk touching under ~1.3 % of the history passes
   both halves, and the retired bound would have caught that case.
3. Apply F9's one-line control assertion, or record why the vacuous form is acceptable.
4. Fix the stale `render/render_bench.rs` name in the `US-0079` bullet this commit edits, and
   at `US-0075:553` (F10).
5. Correct the "cold cache" sentence in the packet's Runs section (F2).
6. Write the `BUG-0075` row into `harness.db` (F13).

## Gate

Run in this worktree at `894b37d2` with a clean tree and `CARGO_BUILD_JOBS=6`:

| Command | Result |
| --- | --- |
| `cargo test -p oneterm-vt` | **PASS** — 837 passed, 0 failed, 4 ignored, 20 suites |
| `cargo test -p oneterm-vt --features vt-paranoid` | **PASS** — 837 passed, 0 failed, 4 ignored |
| `cargo test -p oneterm-vt --no-default-features` | **PASS** — 806 passed, 0 failed, 4 ignored |
| `pwsh scripts/ci-local.ps1` | **FAIL** — exit 1 at "rustdoc self-containment (crates/vt/src)" (F16) |
| `pwsh scripts/ci-local.ps1`, with F16's one-line fix applied | **PASS** — 4781 tests across 135 suites, 0 failed |

The three crate-level commands reproduce the packet's own counts exactly. The gate's final
line, as `894b37d2` stands:

```
ci-local: FAILED: the crate rustdoc cites a document only this repository has
```

and with the single offending citation removed, the same script run end to end:

```
ci-local: all checks passed.
```

The fix was reverted before this file was committed; `git status --porcelain` lists this file
and nothing else.

## How the mutations were applied and reverted

Every mutation in F3, F4 and F5 rewrote the tail of `Screen::integrity_lo`
(`crates/vt/src/grid/screen.rs`), every instrument in F1, F6 and F8 added an `eprintln!` or an
accumulator to `integrity_walk_probe` (`crates/vt/src/snapshot/snapshot_bench.rs`), F2 swapped
the two `integrity_walk_probe` calls in the test body, and F16's fix edited one comment line.
Each was reverted with `git checkout -- <file>` immediately after its run;
`git status --porcelain` was empty before the gate started and lists only this file now.
