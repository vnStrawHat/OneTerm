# Work: the history row dropped at the limit is reused for the next new row

ID: US-0143
Intake: IN-0046
Created: 2026-09-25

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: maintenance (performance; no behaviour or public item changes)
- Risk lane: normal (`oneterm-vt` is an external contract, but no public item changes and
  the CHANGELOG promise lists allocation behaviour and performance as not promised)
- Spec Intake, when required: [`IN-0046`](IN-0046.md), candidate § 5 of
  [`research/hotpath-evaluation.md`](research/hotpath-evaluation.md)

## Outcome

When the scrollback is at its limit, a scrolled line costs zero heap allocations in steady
state: the row a trim drops is kept as one spare per screen, and the next row that has to be
materialised takes its cell vector instead of allocating a new one. Today the profiler sees
one 715 B allocation (and one free) per scrolled line inside `Parser::advance`.

## Scope

- [x] In scope: `Screen` keeps the row `push_rows_with` trims in a `spare` field;
  `row_mut` and `blank_row` build a new row in the spare's allocation when there is one
  (`Row::new_in`, crate-private). `Screen::heap_bytes` counts the spare. Two tests.
- [x] Out of scope: the other trim paths (`trim_history`, a scrollback-limit change,
  `clear_history`), which are not per-line; a free list of more than one row; US-0144 and
  later candidates.

## Acceptance

- [x] With the history full, N scrolled lines of text through `Terminal::feed` make 0
  allocations on the feeding thread (counting allocator in an integration test).
- [x] A recycled row is indistinguishable from a fresh one: no stale cells, styles, wrap
  flag, hyperlink ids, grapheme ids or content hints.
- [x] `RowRef`/`Row` semantics, `history_len`, `is_allocated` of an unwritten bottom row,
  styled blank rows and the integrity walk are unchanged (existing suites, including
  `--features vt-paranoid`).
- [x] Measured before and after with the hotpath release build (flood 300k): the
  `Parser::advance` average with the nested `Screen::scroll_up` site disabled (US-0142
  finding F5), its allocation count and bytes; the BUG-0075 bench ratio;
  `frame_time_under_output`.
- [x] Full `pwsh scripts/ci-local.ps1` passes.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md` § Storage —
  "writing one allocates the row's `Vec<Cell>`", "no free list". Needs one sentence.
- `docs/terminal-backend.md` — the output pipeline; allocation per line is not described.
  No change.
- `crates/vt/README.md`, `crates/vt/CHANGELOG.md` — the promise excludes allocation
  behaviour and performance, and an entry belongs there only when the compiled API or the
  reply bytes change. No change.
- [`high-level-design.md`](high-level-design.md) — the profiling wiring used to measure.
  No change.

### Documentation Action

- Update required: the IN-0029 grid LLD § Storage names the one-row spare.

Reason: the LLD states that a new row allocates and that there is no free list.

### Reconciliation

- Changed: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md`
  § Storage (the one-row spare; "no free list beyond that one spare").
- No-change reasons above still hold: no public item changed (`Row::new_in` and the field are
  crate-private), so the CHANGELOG, README and `vt-public-api.py` surface are untouched.

## Context

- The ring is `next_power_of_two(limit + MAX_ROWS)` slots, so the trimmed slot and the new
  bottom slot are different slots: reuse needs the row carried across, not left in place.
- A new bottom row with an empty erase template stays an unwritten `None` slot
  (`blank_slot`); materialising it at scroll time would change `is_allocated` and the
  memory claim. So the spare is consumed where a row is materialised (`row_mut`, the first
  print), not where it enters the screen.
- The alternate screen has limit 0 and trims on every scroll, so it benefits too.

## Plan

- [x] Packet; code; unit test (fully reset); integration test (0 allocations).
- [x] Measure before (main) and after; table below.
- [x] LLD sentence; gates; commit on `perf/vt-row-reuse`.

## Decisions

None.

## Verification Plan

- `cargo test -p oneterm-vt` (new tests), `--features vt-paranoid`, `--no-default-features`.
- Release hotpath builds of main and the branch (timing with the `Screen::scroll_up` site
  removed, then allocation count and bytes), `research/hotpath-measure.ps1 -Mode Flood`.
- `snapshot_bench` ratio and `frame_time_under_output`, before and after.
- Full `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Windows 11, toolchain 1.96.0, worktree's own `target/`, `CARGO_BUILD_JOBS=3`, two other agents
building on the same machine throughout.

### Tests

- `crates/vt/tests/scroll_allocations.rs` (per-thread counting `#[global_allocator]`, not
  ignored): 1,000 one-line feeds over a full 100-row history. **Before: 1,000 allocations**
  (the test fails on main with exactly one per line). **After: 0.** It passes under
  `--features vt-paranoid` too.
- `grid::tests::a_row_recycled_from_a_trim_carries_nothing_of_the_row_it_was`: a row full of
  styled, hyperlinked, graphic-covered `z` cells with `WRAPPED` and `HAS_GRAPHIC` set is
  trimmed; the new bottom row stays unwritten (`is_allocated() == false`) until a print, then
  holds the **same cell buffer** (pointer-equal) with `x` in column 0, `Cell::EMPTY` in every
  other column, flags exactly `DIRTY`, and the integrity walk passes.
- `cargo test -p oneterm-vt`, `--features vt-paranoid`, `--no-default-features`: all green
  (564 / 564 / 537 lib tests).

### Measurements

Release builds of `oneterm-app` from main `7d8de84b` and from this change, both with the
`Screen::scroll_up` site removed for the measurement only (US-0142 finding F5: that nested
per-line site inflates `Parser::advance`). Flood 300k lines, 1280x800, one cmd tab,
`research/hotpath-measure.ps1 -Mode Flood`, before and after runs interleaved. Reports:
`research/raw/us0143-*.json`.

| Metric | Before | After | Change |
| --- | ---: | ---: | ---: |
| `Parser::advance` avg (timing build, run 1 / run 2) | 865 / 872 ns | 660 / 630 ns | **-26 %** |
| `Parser::advance` p50 / p95 (run 1) | 700 ns / 1.70 µs | 500 ns / 1.40 µs | |
| `Terminal::feed` avg (run 1 / run 2) | 1.00 / 1.01 µs | 803 / 773 ns | -22 % |
| `Pump::advance` avg (run 1 / run 2) | 1.20 / 1.20 µs | 1.01 µs / 973 ns | -17 % |
| `Parser::advance` allocations (count build) | 300,079 (1 per call) | 10,115 | -97 % |
| `Parser::advance` bytes (bytes build) | 203.8 MB (712 B p50) | 6.9 MB (0 B p50) | -97 % |
| BUG-0075 bench, feed / snapshot ratio (debug) | 0.91x / 0.91x (152.9 / 160.6 µs) | 0.86x / 0.87x (138.0 / 144.9 µs) | unchanged shape |
| `frame_time_under_output` flood avg, fast-dev, 3 runs | 1616 / 1595 / 1613 µs | 1697 / 1657 / 1639 µs | noise, see below |
| same, interleaved A/B, flood p50 (spare off / on) | 1589 / 1600 / 1600 µs | 1591 / 1579 / 1664 µs | none |

The remaining 10,115 allocations are the scrollback filling up: the first 10,000 lines (the
default limit) plus the screen still allocate a row each, because nothing has been trimmed
yet. The frame-time test times `prepaint` + `paint` only, not the feed, so it is not expected
to move; the first three-run pair differed by 3 %, and an interleaved A/B (the spare
switched off and on in the same source, three pairs) shows p50 within 1 % except one noisy
run. Treat it as no change.

### Gate

`pwsh scripts/ci-local.ps1` after deleting the worktree's `target/release`: exit 0, final line
`ci-local: all checks passed.`

### Gaps

- Timing figures come from a machine shared with two other building agents; the two timing
  pairs agree to within 5 %.
- Only the flood load was measured; the TUI load (181k scrolled lines per 3 min in US-0142)
  was not re-run.
- `trim_history`, a scrollback-limit change and `clear_history` still drop rows; they are not
  per-line.

## Handoff

Done. Next candidate: US-0144 (the highlight scanner's per-line buffers, now the largest
per-line allocation in the flood: 4 per scanned line).
