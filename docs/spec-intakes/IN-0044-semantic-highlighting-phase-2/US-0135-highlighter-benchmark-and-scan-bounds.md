# Work: The highlighter's cost is measured, and its two scan bounds are settled

ID: US-0135
Intake: IN-0044
Created: 2026-09-22

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
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

- [ ] `cargo run -p oneterm-tools --release --bin <name>` produces a table and a JSON file of
      `oneterm_highlight::scan_line_into` timings over: logical line length `N` in
      {80, 200, 2 000, 8 000}; wrap width in {80, 120, 200}; content shape in {prompt line,
      plain output, keyword-dense log, a line containing CJK}; profile in
      {`Unix`, `Cmd`, `PowerShell`}.
- [ ] The run reports a spread across repetitions, as `vt-bench` does, so a number that is
      noise is visible as noise.
- [ ] A baseline is committed with the machine, profile, rustc version and date recorded in
      it, matching `crates/tools/bench-baseline.json`'s own `machine` and `note` fields.
- [ ] The 8 000-character worst case §10 quotes at 4.14 ms is re-measured in `release` and in
      `fast-dev`, and §10 carries the measured figures with their profile named. The existing
      debug-build figure is replaced, not left beside them.
- [ ] **The wrap-run bound is decided.** Either §10 keeps the bound as it stands — the wrap
      run, which for a logical line longer than the viewport is the viewport — with the
      measurement that makes that acceptable, or a cap on the joined line length is
      implemented, and the colour error that a cap introduces at the cut is stated in §10 and
      covered by a test. Silence is not an outcome.
- [ ] **The head-above-viewport rule is decided.** Either the viewport-only contract stands
      and §10 says so in a sentence beside the identical URL-pass limit `US-0092` has carried
      since it shipped, or scanning from the run's true head up to a cap is implemented. If
      the second, the cost is stated first: `SnapshotState::rows()`
      (`crates/vt/src/snapshot/state.rs:296`) is the visible rows and the engine offers no
      public way to read the row above the top, so it means a new engine read path, a cache
      dependency on rows the cache does not hold, and a second invalidation edge. The
      decision cites the measurement either way.
- [ ] The view-side scope claim is asserted, not argued: a test shows `class_scans` and
      `class_rows_scanned` stay at the wrap run for an edit inside a wrapped line, and reach
      the viewport only when one logical line fills it.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

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

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

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

- [ ] Write the benchmark binary and its fixtures; add `oneterm-highlight` to
      `crates/tools`.
- [ ] Measure in `release` and in `fast-dev`; commit the baseline with its machine record.
- [ ] Decide the wrap-run bound from the numbers; implement a cap only if they demand one.
- [ ] Decide the head-above-viewport rule from the numbers and the cost stated in
      Acceptance.
- [ ] Assert the view-side scope with `FrameStats`.
- [ ] Reconcile §10 (and §13 Q5 if needed).

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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
