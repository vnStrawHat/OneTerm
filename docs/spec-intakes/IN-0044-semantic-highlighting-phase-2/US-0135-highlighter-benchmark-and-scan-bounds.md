# Work: The highlighter's cost is measured, and its two scan bounds are settled

ID: US-0135
Intake: IN-0044
Created: 2026-09-22

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

- Change type: maintenance
- Risk lane: normal
- Spec Intake, when required: `IN-0044` — `docs/spec-intakes/IN-0044-semantic-highlighting-phase-2/IN-0044.md`

Independent of `US-0133` and `US-0134`.

## Outcome

`docs/terminal-semantic-highlighting.md` §10 stops being an argument. The scanner's cost is
a committed, reproducible measurement instead of one ad-hoc probe on a debug build, and the
two bounds §10 currently qualifies — the wrap-run scope, and a run whose head is above the
viewport — are each decided with that number in hand and written down as rules.

## Scope

- [ ] In scope:
  - `crates/tools` — a benchmark binary for `oneterm-highlight`, in the style
    `crates/tools/src/bin/vt-bench.rs` established: measure, print a table, write JSON,
    compare against a committed baseline by hand, never gate CI.
  - A committed baseline file beside `crates/tools/bench-baseline.json`.
  - `crates/terminal-view/src/render/plan_cache.rs` and its tests — whatever the wrap-run
    bound decision turns out to require (possibly nothing).
  - `docs/terminal-semantic-highlighting.md` §10, and §13 Q5 if the head-above-viewport rule
    changes the cache's dependencies.
- [ ] Out of scope:
  - Benchmarking the view-side join and scatter directly. `crates/tools` may only reach down
    to leaf crates (`docs/agents/crate-dependency-rules.md`), and `oneterm-terminal-view` is
    a GPUI crate, not a leaf. That half is covered by asserting `FrameStats::class_scans`
    and `class_rows_scanned` in a view test instead.
  - Making any benchmark a CI gate. `vt-bench`'s own header states why, and the same reasons
    apply: at realistic rates there are orders of magnitude of headroom, and a threshold on
    a shared runner measures the machine.
  - Optimizing the scanner. This packet measures and decides; if the number says something
    must get faster, that is a new packet with a target.
  - Adding `criterion`. The repository has no `[[bench]]` target and no criterion anywhere;
    a new dependency for one binary is not justified when the existing style already
    produces a committed, comparable number (`docs/agents/dependencies.md`).

## Acceptance

- [x] `cargo run -p oneterm-tools --release --bin <name>` produces a table and a JSON file of
      `oneterm_highlight::scan_line_into` timings over: logical line length `N` in
      {80, 200, 2 000, 8 000}; wrap width in {80, 120, 200}; content shape in {prompt line,
      plain output, keyword-dense log, a line containing CJK}; profile in
      {`Unix`, `Cmd`, `PowerShell`}.
- [x] The run reports a spread across repetitions, as `vt-bench` does, so a number that is
      noise is visible as noise.
- [x] A baseline is committed with the machine, profile, rustc version and date recorded in
      it, matching `crates/tools/bench-baseline.json`'s own `machine` and `note` fields.
- [x] The 8 000-character worst case §10 quotes at 4.14 ms is re-measured in `release` and in
      `fast-dev`, and §10 carries the measured figures with their profile named. The existing
      debug-build figure is replaced, not left beside them.
- [x] **The wrap-run bound is decided.** Either §10 keeps the bound as it stands — the wrap
      run, which for a logical line longer than the viewport is the viewport — with the
      measurement that makes that acceptable, or a cap on the joined line length is
      implemented, and the colour error that a cap introduces at the cut is stated in §10 and
      covered by a test. Silence is not an outcome.
- [x] **The head-above-viewport rule is decided.** Either the viewport-only contract stands
      and §10 says so in a sentence beside the identical URL-pass limit `US-0092` has carried
      since it shipped, or scanning from the run's true head up to a cap is implemented. If
      the second, the cost is stated first: `SnapshotState::rows()`
      (`crates/vt/src/snapshot/state.rs:296`) is the visible rows and the engine offers no
      public way to read the row above the top, so it means a new engine read path, a cache
      dependency on rows the cache does not hold, and a second invalidation edge. The
      decision cites the measurement either way.
- [x] The view-side scope claim is asserted, not argued: a test shows `class_scans` and
      `class_rows_scanned` stay at the wrap run for an edit inside a wrapped line, and reach
      the viewport only when one logical line fills it. **Superseded in part by `US-0133`,
      which merged after this packet:** a line's OSC 133 role is read from the region its
      predecessor started in, so the *semantic* scope is now the wrap run **plus the one
      logical line after it** (`class_rows_scanned == 3`, `class_scans == 2` in the same
      12-row fixture). The URL scope is unchanged and is asserted separately
      (`url_rows_scanned == 2`), which is why the two passes were split. The second half —
      the viewport cap for a line that fills it — is untouched.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/terminal-semantic-highlighting.md` §10 — the whole performance budget: the per-line
  cost table, the rescan scope, the "the bound is the wrap run ... this document does not
  claim it is never the viewport" qualification `BUG-0071` `F5` added, and the 4.14 ms
  debug-build figure.
- `docs/terminal-semantic-highlighting.md` §13 Q5 — the cache decision and its `BUG-0071`
  amendment; it is what the head-above-viewport rule would change.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/BUG-0071-semantic-highlight-unstable-on-wrapped-lines.md`
  — its Performance section, which is the argument this packet replaces with a number, and
  its "No benchmark harness" and head-above-viewport gaps.
- `crates/tools/src/bin/vt-bench.rs` and `crates/tools/bench-baseline.json` — the
  repository's benchmark style and, in the binary's header, its rule about gating.
- `docs/agents/crate-dependency-rules.md` — why the tools crate may depend on
  `oneterm-highlight` and may not depend on `oneterm-terminal-view`.
- `docs/agents/dependencies.md` — the dependency policy that keeps criterion out.

### Documentation Action

Update required:

- `docs/terminal-semantic-highlighting.md` §10 — measured numbers with their profiles, the
  wrap-run bound as a decided rule, the head-above-viewport rule stated rather than implied,
  and a pointer to the benchmark and its baseline.
- `docs/terminal-semantic-highlighting.md` §13 Q5 — only if the head-above-viewport decision
  changes what the cache depends on.

### Reconciliation

Docs changed:

- `docs/terminal-semantic-highlighting.md` §10 — a new §10.1 "Measured": the benchmark and
  its baseline, the `release` and `fast-dev` tables for the 8 000-char worst case (the
  `opt-level = 0` 4.14 ms figure is gone, not left beside them), and the two bounds written
  down as decided rules. The `FrameStats` paragraph now names what the two counters assert.
- `docs/terminal-semantic-highlighting.md` §13 `Q5` — a `US-0135` confirmation paragraph:
  the head-above-viewport edge stated rather than implied, with the reason it stands and
  the note that the cache's dependencies are unchanged.
- `crates/terminal-view/src/render/row_plan.rs` — `class_rows_into`'s doc comment now
  states the head-of-run rule where the code implements it.

- `Cargo.toml` — `[profile.fast-dev.package]` now carries the whole regex stack, and its
  comment says where the time goes instead of quoting the retired 4.14 ms figure.
- `docs/agents/crate-dependency-rules.md` — the tools crate's reach read "the L0 leaf `vt`",
  singular. Corrected after verification `F4`: this packet added the second edge, and
  listing the file as "reviewed and unchanged" was reading the rule the file ought to state
  rather than the sentence it contained.
- `docs/agents/structure.md` — the `tools` row's dependency list was missing
  `oneterm-highlight` and its binary list was missing `highlight-bench` (`F4`).

Docs reviewed and unchanged: `crates/tools/src/bin/vt-bench.rs` (the style and the
never-gated rule this benchmark copies), `docs/agents/dependencies.md` (no new third-party
dependency: no criterion — the new profile entries name crates already in `Cargo.lock`).

## Context

- `BUG-0071`'s §10 rewrite is honest about both bounds and says so; what it lacks is a
  number. Its one figure (8 000 chars, 4.14 ms) came from a verifier's ad-hoc probe on a
  debug build, and `oneterm-highlight` was added to `[profile.fast-dev.package]` afterwards,
  so the shipped figure is pessimistic by an unknown factor.
- `FrameStats::class_scans` / `class_rows_scanned` already exist (`BUG-0071` `F10`) and
  separate the class pass from the URL pass, so the view-side scope is observable without
  new instrumentation.
- The four content shapes are not arbitrary: they exercise the prompt regex, the
  Aho-Corasick pass, the structural regexes, and the byte-to-char map `BUG-0071` `F3`
  rebuilt, which are the four per-line costs §10 tabulates.

## Plan

- [x] Write the benchmark binary and its fixtures; add `oneterm-highlight` to
      `crates/tools`.
- [x] Measure in `release` and in `fast-dev`; commit the baseline with its machine record.
- [x] Decide the wrap-run bound from the numbers; implement a cap only if they demand one.
- [x] Decide the head-above-viewport rule from the numbers and the cost stated in
      Acceptance.
- [x] Assert the view-side scope with `FrameStats`.
- [x] Reconcile §10 (and §13 Q5 if needed).

## Decisions

Both bounds are decided inside this packet and recorded in §10, because they constrain the
scan scope any future highlighting work inherits. If the head-above-viewport answer turns
out to require an engine read path, that is a decision record of its own before any code.

## Verification Plan

- Unit: the benchmark's fixtures produce the line lengths and shapes they claim; the
  baseline comparison flags a deliberate regression.
- Integration: the `FrameStats` assertions on scan scope, including the viewport-filling
  line.
- E2E: not applicable — the measurement is the evidence, and a GUI walk adds nothing to it.
- Platform: `pwsh scripts/ci-local.ps1`. The benchmark itself is run by hand, never in the
  gate.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

**The benchmark.** `crates/tools/src/bin/highlight-bench.rs`, run as
`cargo run -p oneterm-tools --release --bin highlight-bench [--runs N] [--machine TEXT]
[--json PATH]`. 144 cells: 4 content shapes x 4 lengths {80, 200, 2 000, 8 000} x 3 wrap
widths {80, 120, 200} x 3 profiles, median of `--runs` cycles with the fastest-minus-slowest
spread beside every figure. Writing JSON needs an explicit `--json PATH`, so a casual run
cannot overwrite the baseline. No criterion, no `[[bench]]`, no CI gate.

The wrap width does not change what the scanner sees — the unit is the logical line — and
the binary's header says so. It is still an axis, and it earns the place twice: the `rows`
column is how many display rows one scan covers (how many rows one frame replans), and the
three widths of one cell are three measurements of identical work, so the spread between
them is the table's own noise floor, measured rather than asserted.

**Baseline.** `crates/tools/highlight-bench-baseline.json`, `--runs 9`, machine recorded in
the file as `crates/tools/bench-baseline.json` records its own: Intel Core i7-12700, Windows
11 26200, rustc 1.96.0, release, 2026-09-22.

**The 8 000-char worst case, re-measured** (median of 9; the `opt-level = 0` 4.14 ms figure
§10 carried came from an ad-hoc probe on a build nobody runs the terminal in, and is now
replaced):

| Shape | ns/char (`release`) | one 8 000-char scan, `release` | `fast-dev` before | `fast-dev` now |
|---|---|---|---|---|
| Windows prompt line | 1.1 | 9 us | 14 us | 13 us |
| Plain output | 8.6-9.1 | 70 us | 0.43-0.45 ms | 73-79 us |
| Keyword-dense log | 10.7-11.6 | 86-93 us | 0.73-0.78 ms | 98 us |
| A line carrying CJK | 64-70 | 0.51-0.56 ms | 7.0-7.5 ms | 0.53-0.54 ms |

The two `fast-dev` columns are the same profile before and after the verification rework
recorded under Gaps.

**Decision 1 — the wrap-run bound stands, uncapped.** The pathological case costs 0.07 ms
(ASCII) to 0.56 ms (CJK) per keystroke in `release`, and a whole 40-row viewport of the
worst shape is 0.21 ms, 1.2% of a 16.7 ms frame. A cap would buy half a millisecond in the
case nobody meets and pay for it with a colour error at every cut — a string, a prompt
region or a keyword sliced at an arbitrary boundary, which is the class of defect
`BUG-0071` was. No cap implemented; §10 states the rule and the number behind it.

**Decision 2 — the scan starts at the first visible row.** The accepted limit stands.
Cost of the alternative, stated before the decision as the packet required: the scan itself
is affordable (a capped look-back of one extra viewport roughly doubles the table above,
so 0.14-1.1 ms), but the render path is handed `SnapshotState::rows()`
(`crates/vt/src/snapshot/state.rs:296`), which is the visible rows and nothing else. The
engine does expose `Terminal::screen()` and `Screen::row(RowId)` publicly, so no *engine*
API would have to change — but the view cannot reach them at plan time: the snapshot is
copied once under the terminal lock in `oneterm-terminal`, so a look-back means either a
wider snapshot or a lock taken inside render, plus a cache dependency on rows the cache
does not hold and a second invalidation edge (those rows change on every scroll). Against
that: the defect is bounded (the one partial run at the top), cosmetic, and self-correcting
after one frame of scrolling — and the `US-0092` URL pass has the identical limit, so
lifting one and not the other would make one cache contract into two. Recorded in §10 and
§13 `Q5`, and in `class_rows_into`'s doc comment. No decision record: nothing was built
that future work must inherit beyond the rule, which the contract now carries.

**Decision 3 — the view-side scope is asserted, not argued,** and it already was: the two
`plan_cache.rs` tests `class_delta_replans_the_continuation_row` and
`a_line_longer_than_the_viewport_scans_the_viewport` (`class_rows_scanned` equals
`rows_total`, `class_scans` is 1) are exactly the two halves this packet asks for — rows
scanned = the run length capped at the viewport. A third test would have restated them, so
§10 cites them instead.

> **Reconciled with `US-0133` (merged after this packet).** The first test asserted
> `class_rows_scanned == 2`, `class_scans == 1` in a 12-row viewport. `US-0133` made a
> line's role depend on the region its predecessor started in, so the semantic pass now
> covers the wrap run **plus the one logical line after each changed run** and the same
> test asserts `3` / `2`. The chain is one line long by construction. The URL pass keeps
> this packet's bound unchanged and now asserts it on its own counter
> (`url_rows_scanned == 2`, in `roles_ride_the_class_rescan`), which is why the two passes
> no longer share a loop. §10.1 and §13 `Q5` carry the same correction.

**Commands.**

- `cargo test -p oneterm-tools --bin highlight-bench` — 2 passed (the fixture tests).
- `cargo run -p oneterm-tools --release --bin highlight-bench -- --runs 9 --machine "..." --json crates/tools/highlight-bench-baseline.json` — the committed table.
- `cargo run -p oneterm-tools --profile fast-dev --bin highlight-bench -- --runs 9` — the
  `fast-dev` column.
- `python scripts/vt-public-api.py --check --no-doc` — green (nothing in `oneterm-vt`
  changed). Note for anyone copying that line out: it is not runnable on its own. It reads
  `target/doc/oneterm_vt`, so it needs the preceding
  `cargo doc -p oneterm-vt --no-deps --all-features` that `scripts/ci-local.ps1` runs before
  it.
- `pwsh scripts/ci-local.ps1` — `ci-local: all checks passed`.

**Proof.**

- Unit: `tests::every_fixture_is_exactly_the_length_it_claims` (every shape is exactly the
  char count its row claims, or every ns/char figure in that row is wrong) and
  `tests::each_shape_exercises_the_cost_it_names` (the prompt shape reaches the prompt
  branch, the plain shape classifies nothing, the log shape reaches Error/Warn/DateTime/Ip/
  Path, the CJK shape is multi-byte).
- Integration: the two `FrameStats` scope tests named above.
- E2E: not applicable, as the packet states — the measurement is the evidence.
- Platform: `pwsh scripts/ci-local.ps1`.

**Gaps and follow-ups.**

- The benchmark ran on a machine that was also compiling another worktree, which is why the
  baseline was taken at `--runs 9` rather than 5; some cells still show a 15-30% spread and
  the spread column says so. Any comparison narrower than a cell's own spread is the
  machine, not the scanner.
- **`fast-dev` did not optimize where the time goes — fixed here, not deferred.** It raised
  `oneterm-highlight` to `opt-level = 3` and left the matchers it calls at `dev`'s, so every
  shape but the prompt was 6-13x slower than `release` and the CJK worst case was 6-7 ms,
  worse than the 4.14 ms figure `fast-dev` was added to fix. The follow-up this packet first
  recorded ("one line in `[profile.fast-dev.package]`") was **wrong as written**, and
  verification measured why: `regex` is a thin layer over `regex-automata`, `regex-syntax`
  and `memchr`, so an entry for `regex` and `aho-corasick` alone leaves the hot code
  unoptimized. All five are now in `[profile.fast-dev.package]`, which brings `fast-dev` to
  1.0-1.4x of `release` (the column above) for a one-time ~15 s compile of pinned
  third-party crates that nobody steps through in a debugger — the rationale
  `[profile.dev.package]` already carries for `gpui-pre`/`smol`. This is a build-profile
  change, not the scanner optimization the packet put out of scope: no OneTerm code moved
  and no `release` figure changed. `[profile.dev.package]` was **not** touched, so
  `cargo test` still pays it; that is a wider blast radius and belongs to whoever wants it.
- The stale 4.14 ms figure survived in `Cargo.toml`'s comment for
  `oneterm-highlight = { opt-level = 3 }` (verification `F5`). That comment is rewritten in
  the same edit and now states where the time actually goes.
- CJK costs ~7x ASCII per char, in the byte-to-char map (`BUG-0071` F3), which holds one
  `usize` per *byte*. Measured, not fixed: the same scope boundary.
- No `--check` trip-wire like `vt-bench grid --check`. The baseline is compared by hand, as
  the packet's "never gated" rule intends; a trip-wire can be added if the number ever
  starts moving.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
