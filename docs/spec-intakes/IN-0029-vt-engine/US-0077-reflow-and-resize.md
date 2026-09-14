# Work: Reflow and resize policies

ID: US-0077
Intake: IN-0029
Created: 2026-09-12

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

- Change type: new capability (the column half of resize, which `US-0075` left rows-only)
- Risk lane: high_risk (the intake's lane; reflow is the component the research names most
  bug-prone, and `KeepViewportTop` is a shipped product behaviour — `DEC-0008` — that must be
  reproduced cell-exactly). Nothing in the workspace depends on `oneterm-vt` yet, so nothing
  user-reachable changes with this packet.
- Spec Intake, when required: IN-0029

## Outcome

`crates/vt` resizes both dimensions of the primary screen with reflow, against
[`low-level-design/reflow-and-resize.md`](low-level-design/reflow-and-resize.md):

1. **Column reflow** — logical lines rejoined and redistributed at the new width, trailing blanks
   trimmed per logical line, wide pairs never split, `LeadingWideSpacer` inserted and removed
   (trap 33), `WRAPPED` set on every row but the last of a line, fresh `RowId`s, history trimmed
   from the oldest end (trap 32).
2. **Both policies** — `ResizePolicy::{BottomAnchor, KeepViewportTop}` native to the engine, with
   `KeepViewportTop` implemented as the LLD's one procedure (measure, run `BottomAnchor`, shift),
   replacing `crates/terminal/src/model.rs:481-541`.
3. **The anchor remap** — every tracked anchor is carried by the character it sat on, through the
   one `Anchors::remap` mechanism; the cursor, saved cursor and viewport top are read back out of
   the list into the screen's fields **before** the next `sync_anchors`.
4. **conhost parity** — `measure_rows` reproduces `conhost_cursor_row` for the ten
   `keep_viewport_top_*` scenarios and for the geometry recorded in BUG-0051 § Measurements (b).
5. **The traps** — 27 (tab stops), 28 (region and selection), 29 (the alternate screen never
   reflows), 30 (a scrolled-back viewport), 31 (pending wrap), 32 (history truncation), 33 (wide
   spacers).
6. **Cost recorded, not gated** — resize latency at 0 / 10 000 / 100 000 rows of scrollback
   (R-29), and the R-28 debug-suite budget re-measured.

## Scope

- [ ] In scope:
  - `crates/vt/src/reflow/` — new: `mod.rs` (the policies and the resize entry point),
    `columns.rs` (the logical-line iterator), `reflow_tests.rs`, `reflow_props.rs`.
  - `crates/vt/src/grid/screen.rs` — the resize entry points the reflow module drives, listed
    under "Shape differences".
  - `crates/vt/src/grid/row.rs` — one constructor (`Row::from_cells`).
  - `crates/vt/src/grid/anchor.rs` — one helper (`Anchors::kill_selection`) for trap 28.
  - `crates/vt/src/grid/terminal_grid.rs` — `resize(size, policy) -> ResizeOutcome`.
  - `crates/vt/src/lib.rs` — the `pub mod reflow;` declaration and its re-exports.
- [ ] Out of scope:
  - `crates/vt/src/parser/` and `crates/vt/src/render/` — other packets, in flight concurrently.
  - `Terminal::resize` and `VtEvent::{RowsTrimmed, GraphicReleased}` — `US-0079` owns the event
    surface; `ResizeOutcome` carries the counts until then.
  - `RenderUpdate::Full` on resize — `US-0078` owns `RenderState`, which derives it from the size
    it last observed (`damage-and-render-state.md`), not from a per-row stamp.
  - The old engine's `keep_viewport_top_*` suite in `crates/terminal/src/model.rs`, which keeps
    running against the old engine until the adapter packet (R-44,
    [`low-level-design/migration.md`](low-level-design/migration.md)).
  - `vt-bench resize` for the new engine (`crates/tools/` is outside this packet's file scope);
    the measurement is an ignored test in `crates/vt`.
  - `NOTICE` / `THIRD-PARTY-NOTICES.md` attribution lines — outside this packet's file scope, see
    "Gaps".

## Acceptance

- [x] Ten `keep_viewport_top_*` scenarios from `crates/terminal/src/model.rs` reproduced as
  engine-level tests with the same inputs and the same expected grids, cell-exactly.
- [x] The reference's own reflow cases (`shrink_reflow`, `shrink_reflow_twice`,
  `shrink_reflow_empty_cell_inside_line`, `grow_reflow`, `grow_reflow_multiline`) reproduced.
- [x] Seven `reflow::props::` properties green: text survives a round trip, anchors stay on their
  character, no row exceeds the column count, wrap flags are consistent, wide pairs are never
  split, the alternate screen never reflows, integrity holds after any resize sequence.
- [x] Traps 28-33 each have a named test.
- [x] `measure_rows` matches the rows recorded in BUG-0051 § Measurements (b).
- [x] Resize latency measured at 0 / 10 000 / 100 000 rows of scrollback and recorded as a ratio
  against the engine being replaced (R-29 — a record, not a gate).
- [x] `pwsh scripts/ci-local.ps1` green; no `unsafe`; no new dependency.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/reflow-and-resize.md` — this packet's
  contract: the order of operations, the iterator, the two policies, the `KeepViewportTop`
  procedure, traps 28-33, the cost model (R-29), and the verification list.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md` — the storage,
  the `RowId` lanes, the canonical `AnchorKind` list, the `ScrollReport::scrolled` `Option`
  contract, trap 27, and the rows-only `Screen::resize` this packet replaces.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md` — what a
  resize must report: `RenderUpdate::Full`, derived by `RenderState` from the size it last
  observed. No per-row damage stamp is owed by this packet.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` — the R-28
  integrity budget (O(1) per method, the full walk once per `feed`/`resize`/`render_update`,
  60 s debug suite), tier 4 of the benchmark, and the trap-map rows owned by reflow (28-33).
- `docs/spec-intakes/IN-0029-vt-engine/US-0075-grid-and-scrollback.md` — readings 1, 3, 4 (the
  row-id lanes, the `scroll_up` case-2 ids, the cursor is not a content anchor), 8 (the
  `ScrollReport::scrolled` `Option`), 12 (M6, two `AnchorKind::Cursor` entries), and the recorded
  gap "`Screen::resize` is rows-only".
- `docs/spec-intakes/IN-0029-vt-engine/high-level-design.md` — P13, P14, phase row 5.
- `docs/spec-intakes/IN-0029-vt-engine/research/engine-semantics.md` § 2.16 and § 8 traps 27-33 —
  the reference's `grow_lines` / `shrink_lines` / `grow_columns` / `shrink_columns` semantics.
- `docs/spec-intakes/IN-0029-vt-engine/research/prior-art.md` § 9.3 — the avt iterator
  recommendation, the fast path, the alt-screen gate, and the ConPTY quirk set.
- `docs/decisions/DEC-0008-local-conpty-grow-resize-keeps-viewport-top.md` — the accepted
  product behaviour `KeepViewportTop` must preserve, including its two recorded tradeoffs.
- `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md` — "reflow: every row
  gets a fresh id".
- `docs/terminal-backend.md` § 5.3 — the caller resizes the PTY first and the grid second; the
  policy is chosen by the backend.
- `docs/spec-intakes/IN-0019-conpty-resize-scrollback-desync/BUG-0051-conpty-grow-resize-alignment.md`
  § Measurements — the recorded conhost rows and the re-wrap rule `measure_rows` must satisfy.
- `docs/license-analysis.md` § 3 — MIT/Apache-2.0 source may be reused with the notice retained.
- `crates/terminal/src/model.rs:440-541, 616-910` — the behaviour being reproduced.
- `vendor/alacritty_terminal/src/grid/{resize.rs,tests.rs}` — the reference algorithm and its own
  reflow tests.

### Documentation Action

**No contract change.** The reviewed LLDs already describe the behaviour that was built, including
the two revisions that landed on `feat/vt-engine` after this branch point (the screen-discriminant
paragraph in `reflow-and-resize.md` and the `ScrollReport::scrolled` / C12 / `put_tab` paragraphs
in `grid-and-scrollback.md`), which this packet was built against by reading them from the main
checkout.

Reason: this packet implements an accepted design without changing it. Where the design left a
choice or where the implementation had to read the text one way rather than another, the reading
is recorded under "Evidence and Gaps" for the design owner instead of being written into the
intake documents, which this packet may not edit. The one place the LLD explicitly offers a
choice — "`US-0077` therefore adds the discriminant (**or an equivalent lane check in
`Anchors::remap`**)" — is taken as the lane check, which needs no change to the canonical
`AnchorKind` list and therefore no edit to `grid-and-scrollback.md` or `DEC-0015`.

### Reconciliation

No owning doc changed. The no-change reason above still holds at completion: every behaviour
built is one the reviewed LLDs already state, and every divergence is recorded below rather than
back-written into the design.

## Context

- `US-0075` left `Screen::resize` rows-only, with the column half truncating and padding. That
  path survives, unchanged in meaning, as the **alternate screen's** resize (trap 29).
- The ring length is a session constant (R-30), so nothing here rehomes the ring; the reflow's
  output is capped at `scrollback_limit + rows` **before** it is written, which is both trap 32
  and what keeps the write inside the ring.
- The cursor, the saved cursor and the viewport top are screen-owned anchors (`US-0075` reading
  4): the field is the authority under a scroll, the entry is the authority under a reflow. This
  packet is the first code that runs in the second direction.

## Plan

- [x] Read the owning docs and the behaviour being reproduced; trace all thirteen
  `keep_viewport_top_*` scenarios by hand against the planned algorithm before writing it.
- [x] `crates/vt/src/reflow/columns.rs` — the logical-line iterator and the anchor remap.
- [x] `crates/vt/src/reflow/mod.rs` — `ResizePolicy`, `ResizeOutcome`, `resize`, `measure_rows`.
- [x] The `Screen` resize entry points the module drives.
- [x] Port the reference's reflow tests and the `keep_viewport_top_*` scenarios.
- [x] Seven properties; the trap tests; the timing measurement.

## Decisions

- [`DEC-0008`](../../decisions/DEC-0008-local-conpty-grow-resize-keeps-viewport-top.md) — the
  `KeepViewportTop` product behaviour this packet makes native.
- [`DEC-0015`](../../decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md) —
  reflow allocates a fresh `RowId` per new row.

No new decision: every consequential choice here is already in one of those two records or in the
LLD.

## Verification Plan

- Focused: `cargo test -p oneterm-vt reflow::` — the ported reference cases, the ten
  `keep_viewport_top_*` scenarios, traps 28-33, `measure_rows`, and the seven properties.
- Regression: `cargo test -p oneterm-vt` (the `US-0075` grid suite must stay green: the reflow
  module changes `Screen`'s resize entry points).
- Whole workspace: `pwsh scripts/ci-local.ps1` (fmt, clippy `-D warnings`, `cargo test
  --workspace`, and the five Python policy checks).
- Properties at 10 000 cases: `VT_PROPTEST_CASES=10000 cargo test -p oneterm-vt reflow::props`.
- Budget (R-28): `cargo test -p oneterm-vt` wall time against the 60 s debug-suite budget.
- Cost (R-29): `cargo test -p oneterm-vt --release resize_latency -- --ignored --nocapture`,
  80x24 to 100x40 at 0 / 10 000 / 100 000 rows of scrollback, medians, reported as a ratio
  against `evidence/US-0072-bench-baseline.md`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Commands and results

- `pwsh scripts/ci-local.ps1` — **green**, every step: `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `python scripts/verify-dependency-graph.py` (21 packages),
  `python scripts/check-doc-paths.py` (122 paths), `python -m unittest
  scripts/test_check_english.py`, `python scripts/check-english.py` (687 files),
  `python scripts/completion-catalog.py validate`, `python scripts/third-party-notices.py
  --check`.
- `cargo test --workspace` — raw totals summed over the 54 `test result:` sections:
  **1317 passed / 0 failed / 6 ignored**. Against this branch's base (`US-0075`: 1267 / 0 / 5)
  the delta is exactly this packet's **50 new tests plus the one ignored measurement**.
- `cargo test -p oneterm-vt` — **139 passed / 0 failed / 1 ignored**, `finished in 0.49 s`;
  `--list` counts 140 against 89 before. Breakdown: 41 `reflow::tests::`, 10
  `reflow::props::`, of which **14 are `keep_viewport_top_*`**.
- `VT_PROPTEST_CASES=10000 cargo test -p oneterm-vt reflow::props` — **10 passed / 0 failed**,
  `finished in 2.41 s`. The verifier additionally ran 3 x 20 000 and 1 x 40 000 fresh-seed
  cases, all green.
- **R-28 budget re-measured:** the whole `oneterm-vt` debug suite is 0.49 s at the default 256
  cases and 2.4 s at 10 000, against the stated 60 s budget. The full two-screen integrity walk
  runs once per resize (`TerminalGrid::resize`) and the O(1) tier at the end of every mutating
  method, which is the tiering R-28 mandates.
- No `unsafe` anywhere in `crates/vt`; no dependency added (`crates/vt/Cargo.toml` untouched).

### The exit criterion: the `keep_viewport_top_*` scenarios

All **thirteen** `keep_viewport_top_*` scenarios in `crates/terminal/src/model.rs` are
reproduced cell-exactly with the same inputs and the same expected grids, plus both
`default_*` companions. The intake says "ten"; the count in `model.rs` is thirteen (the three
`ls -lath` rework cases of BUG-0051 were added after the phase table was written). Four are
renamed where the old name said "alacritty", which is not what the engine is measured against
any more:

| `crates/terminal/src/model.rs` | `reflow::tests::` |
| --- | --- |
| `keep_viewport_top_grow_keeps_rows_cursor_and_history` | same |
| `keep_viewport_top_grow_larger_than_history` | same |
| `keep_viewport_top_repeated_grows_and_column_change` | same |
| `keep_viewport_top_restores_a_scrolled_back_viewport` | `keep_viewport_top_restores_the_scroll_offset` |
| `keep_viewport_top_leaves_shrink_and_history_less_grow_to_alacritty` | `..._to_bottom_anchor` |
| `keep_viewport_top_grow_during_alt_screen_corrects_the_primary_grid` | `keep_viewport_top_corrects_the_primary_screen_while_alt_is_active` |
| `keep_viewport_top_widen_joins_wrapped_rows_and_keeps_the_top_row` | same |
| `keep_viewport_top_widen_without_row_change_moves_the_cursor_up` | same |
| `keep_viewport_top_top_row_continuing_a_history_line_keeps_the_cursor_row` | same |
| `keep_viewport_top_widen_joins_the_cursor_row` | same |
| `keep_viewport_top_narrow_with_a_mid_screen_cursor_pulls_split_rows_back` | same |
| `keep_viewport_top_narrow_with_the_cursor_at_the_bottom_matches_alacritty` | `..._matches_bottom_anchor` |
| `keep_viewport_top_widen_during_alt_screen_joins_the_primary_rows` | same |
| `default_grow_pulls_history_and_moves_the_cursor_down` | `default_policy_grow_anchors_the_bottom_row` |
| `default_grow_during_alt_screen_pulls_the_primary_history` | same |

**`measure_rows` did not need the fallback.** The LLD's contingency was to port
`conhost_cursor_row` verbatim if the shared iterator could not reproduce its numbers. It can:
`measure_rows` reflows the screen rows from the top down to the cursor row through the same
`emit_line`, with no scratch grid, no padding rows and no history-size arithmetic, and every
one of the thirteen scenarios lands on the same row. The scratch-grid probe and its `pad`
trick are deleted rather than ported.

### Cost (R-29): recorded, not gated

`cargo test -p oneterm-vt --release resize_latency -- --ignored --nocapture`, 80x24 to 100x40,
median of 20 runs, the same geometry and filler shape as `vt-bench resize`:

The benchmark measures **two content shapes**, because they exercise different halves of the
algorithm: 96-character logical lines, which wrap at 80 columns so every one of them is rebuilt,
and 40-character lines, which do not — the shape a real scrollback is mostly made of.

| Scrollback rows | wrapped grow / shrink (us) | short grow / shrink (us) | old engine grow / shrink (us) |
| ---: | ---: | ---: | ---: |
| 0 | **1.8 / 2.4** | **2.5 / 3.1** | 1186 / 20 |
| 10 000 | **2 809 / 2 636** | **821 / 931** | 4704 / 3361 |
| 100 000 | **26 895 / 26 161** | **14 306 / 15 740** | 53 182 / 39 146 |

Old-engine figures from [`evidence/US-0072-bench-baseline.md`](evidence/US-0072-bench-baseline.md)
§ "Tier 4", measured on 96-character content, so the wrapped column is the comparable one. The
depth-0 figures sit close to the timer's resolution and vary by a microsecond or two between runs.

- **At 0 scrollback the research target is met**: single-digit microseconds against the old
  engine's 1186 us to grow — **two to three orders of magnitude**. The old engine's depth-0 grow
  cost was the history the grow itself creates.
- **At depth, 2.0x faster on wrapped content and 3.7x on short**, against the engine being
  replaced. The shape is still O(live rows x cols) exactly as the LLD's cost model says; what
  changed is the constant.
- **Where the time goes.** The same test times a rows-only round trip (40 rows and back at an
  unchanged width) at each depth: **0.1-0.2 us regardless of scrollback**, because that path
  moves counters and never walks a row. *All* of the cost above is therefore the column reflow.

**Three constant-factor wins the verifier measured, two applied.** The verifier's
`p14_cost_breakdown` attributed the original 33 ms at 100 000 rows to allocator traffic first
(~23 ms of allocate-and-fill plus ~18 ms of dropping the old rows) and per-cell rescanning
second (~6-12 ms in `Row::from_cells`). Applied here:

1. **Skip untouched unwrapped rows** (`columns.rs`, the fast path in `reflow_columns`, plus
   `Row::into_refitted`). This is the reference's own rule — `grid/resize.rs:103-107` makes a row
   a reflow target only when it is short **and** carries `WRAPLINE`. A row that is a whole
   logical line, has content, already fits the new width and carries no tracked position now
   changes width in place: no copy, no allocation, no hint rescan, nothing dropped. Measured by
   disabling the branch and re-running: **short content at 100 000 rows goes 29 640 / 30 665 us
   to 14 306 / 15 740 us (2.1x), and at 10 000 rows 2 562 / 2 698 to 821 / 931 (3.1x)**. Wrapped
   content is unchanged, as expected — the branch never fires there.
2. **Fold the hint computation into `emit_line`** (`columns.rs`, `Row::from_cells` no longer
   rescans). **Wrapped content at 100 000 rows: 32 947 / 33 701 us to 26 895 / 26 161 us (1.2x).**
3. **Recycling row allocations is *not* applied.** With (1) in place the rows that still allocate
   are only the ones a wrap actually splits or joins, which is where an allocation is doing real
   work; a free list would need `Row::into_cells`, a pool with a bound, and capacity handling for
   a width change that reallocates anyway. Recorded as a follow-up with its number: the remaining
   **26 ms at 100 000 rows of wrapped content** is still allocator-dominated, so a pool is worth
   roughly the ~23 ms of allocate-and-fill the verifier measured, minus whatever the width change
   forces it to realloc.

The lever that changes the *shape* rather than the constant is still the LLD's own: reflow the
viewport eagerly and the history lazily. Out of scope here; these measurements are the argument
for it, and the packet no longer claims that "no amount of care" helps.

### C13 and C14 against the parity corpus — **no recording can reach them**

Both corrections change what a **resize** does, and the parity harness never resizes:
`crates/tools/src/corpus_replay.rs:62-116` builds one `CaptureSize` from the recording's
`size.json` and replays the whole byte stream at that single geometry. The only column-changing
escape in the corpus is `DECCOLM` (`deccolm_reset`), and the reference's `deccolm`
(`vendor/alacritty_terminal/src/term/mod.rs:807-818`) clears the scrolling region and the grid
**without resizing**, so it does not reach `grid/resize.rs` either. A byte scan finds CUP
(`ESC [ … H` / `f`) in **20 of the 45** recordings, which is what C13 would need — but without a
resize it is inert.

**Therefore `US-0076` needs no `expected-diffs.json` entry for C13 or C14.** What it does need,
if either is ever declared in the corrections table, is the id itself:
`KNOWN_DEVIATIONS` in `crates/tools/src/corpus.rs:44-48` lists `C1`-`C11` and already omits
`US-0075`'s `C12`, so `vt-corpus bless --deviation C13` would be rejected today. That array is
outside this packet's file scope; `US-0076` owns it.

### Two defects the property tests found

Both are in code this packet wrote or touched, both were found by
`reflow::props::integrity_holds_after_any_resize_sequence` within its first few hundred cases,
and both now have a named unit test as well:

1. **A rows shrink left the alternate screen holding history.** `resize_rows` pushes
   `(cursor_row_index + 1) - rows` rows into history, and the alternate screen's limit is zero.
   Fixed by a `trim_history` step at the end of `resize_rows`, which the primary screen also
   needs: a shrink can take its history over the configured limit. Test:
   `reflow::tests::a_rows_shrink_never_leaves_the_alt_screen_with_history`.
2. **A `LeadingWideSpacer` survived inland.** Widening a row without reflowing (the alternate
   screen, trap 29) left the spacer that stood in the old last column stranded mid-row, where it
   means nothing. Fixed in `repair_wide_pairs`, so the rule now holds for every in-row mutation
   rather than only for the reflow. Test:
   `reflow::tests::a_leading_wide_spacer_never_survives_inland`.

### Decisions taken where the design left a choice — **for the design owner**

1. **The screen discriminant on `AnchorKind` (M6) is not added.** The revised LLD offers "the
   discriminant **or an equivalent lane check in `Anchors::remap`**", and the lane check is
   strictly smaller: the two screens draw from disjoint runs of the id space, so an entry's row
   already says which screen it is on, and `reflow_columns`'s remap closure returns positions
   outside the primary's live range unchanged. Nothing in the canonical `AnchorKind` list
   changes, so `grid-and-scrollback.md` and `DEC-0015` need no edit. If the owner wants the
   discriminant anyway it is additive and no caller here depends on its absence.
2. **A resize syncs the anchor list before reading it.** `Screen::scroll_viewport` and
   `scroll_to_bottom` move the viewport without writing the `ViewportTop` entry back — they take
   no `&mut Anchors` — so the entry can be stale by the time a resize reads it, which made the
   first visible character jump. `reflow::resize` therefore calls `sync_anchors` on both screens
   before measuring anything. That is the design's own rule stated in the other direction ("the
   field is the authority across a scroll; the entry is the authority across a reflow"), but the
   LLD does not say who performs the handover. The alternative — making the two scroll methods
   take the anchor list — is a wider `Screen` API change and belongs to the owner.
3. **`ResizeOutcome::rows_trimmed` counts both trims.** The reflow's own cap (trap 32) and the
   `trim_history` a rows shrink can trigger. The LLD names only the first; reporting both is what
   makes the number mean "rows this resize removed from history".
4. **The trailing-blank trim uses `Cell::is_blank`, not `Cell::is_erasable`.** Checked against
   `vendor/alacritty_terminal/src/term/cell.rs`'s `GridCell::is_empty`, which accepts `' '` **or
   `'\t'`** and tests `fg`, `bg`, `INVERSE`, the underlines and `STRIKEOUT`. Three differences,
   all in the "keep more" direction:
   - a **coloured** trailing blank is kept by **both** engines — no divergence;
   - a **tab** cell is trimmed there and kept here (`is_erasable` needs the interner, and R-12
     says a tab cell carries meaning);
   - a **bold, dim, italic or hidden** trailing blank is trimmed there and kept here, because
     `is_empty` ignores the attributes that paint nothing on a space (trap 37) while `is_blank`
     asks for the default style outright. **Declared as correction C14** below.
9. **A grow no longer loses the tail of a wrapped line below the cursor. Declared as correction
   C13.** Found by the verifier's differential run against the old engine (`evidence/
   US-0077-verify.md` § 3.3) and now pinned by
   `reflow::tests::a_widen_keeps_the_tail_below_the_cursor_where_the_reference_drops_it`.
   `vendor/alacritty_terminal/src/grid/resize.rs:101-242` (`grow_columns`) drives row placement
   off `cursor_line_delta` and ends with
   `reversed.truncate(reversed.len() + overflow - cursor_line_delta)` (`:216-222`), which drops
   rows off the **newest** end when the cursor did not move as far as the join did: in the
   verifier's scenario `"23456789"` vanishes and a `WRAPLINE` is left dangling on the bottom row.
   Here the layout is driven by the logical lines alone and the cursor is one more tracked point,
   so nothing below it can be truncated away. Reachable in one keystroke (any `CUP` or arrow-key
   move inside a wrapped command line before a resize), so it is a real correction, not a
   curiosity.
5. **A resize creates rows with `Cell::EMPTY`, never the erase cell.** The reference swaps its
   cursor template out for the default cell for exactly the length of a resize
   (`research/engine-semantics.md` § 2.16), so `push_rows_with(n, Cell::EMPTY)` is used on the
   resize paths and the erase-cell `push_rows` stays on the scroll paths. Observable only under
   a non-default background.
6. **A wide glyph cannot be reflowed into a one-column screen.** `cols == 1` degrades the pair to
   two blanks, the same "dropped and counted" direction `Screen::print` takes. The reference's
   shrink loop has no guard here at all, which is one of the two places Windows Terminal
   documents a hang; the degrade is what makes the loop provably terminate.
7. **The identity resize really is an identity.** It returns before the scroll-region reset, so
   a resize to the size the screen already has does **not** destroy `DECSTBM`. The LLD's step
   list puts the early return first and the region reset at step 5, so this follows from it, but
   trap 28 reads as unconditional and the two want reconciling in one sentence.
8. **`measure_rows` is `u16` and clamps at the API boundary.** `min(size.rows - 1)`, as the
   deleted `conhost_cursor_row` call site did.

### Verification round (`evidence/US-0077-verify.md`)

An independent review of `efaa186` returned **merge after fixes**: no blockers, one major, four
minors and three documentation points. It reproduced every gate and every number in this packet,
ran the properties at 3 x 20 000 and 1 x 40 000 fresh-seed cases, and wrote fifteen independent
probes of its own (wide pairs, ZWJ clusters, pending wrap, `DECSC`, marks, the alternate screen,
`cols == 1`, trim order, 1 000 random resizes) — all green.

| # | Finding | Fix |
| --- | --- | --- |
| M1 | The correction over the old engine (a grow that loses the tail below the cursor) was undeclared, so `US-0076`'s parity gate would meet an unexplained diff | Declared as **C13** with the reference lines, pinned by `reflow::tests::a_widen_keeps_the_tail_below_the_cursor_where_the_reference_drops_it`, and the corpus checked: **no recording can reach it** (above) |
| d1 | Reading 4 recorded the tab divergence but not the `BOLD`-blank one, and worried about a coloured blank that in fact both engines keep | Reading 4 rewritten with all three cases; the bold one declared as **C14** and pinned by `reflow::tests::a_styled_trailing_blank_is_content` |
| m1 | The attribution gap named no owner and no id | Assigned to `US-0087` |
| m2 | The in-place-shrink recommendation was deviated from without citing it | The peak-memory gap now cites `prior-art.md` § 9.3 and carries the 1 000 000-row figure |
| m3 | `set_cursor` / `set_saved_cursor_pos` were unguarded back doors that could recreate the staleness reading 2 exists to fix | Renamed `restate_cursor_after_reflow` / `restate_saved_cursor_after_reflow`, each with a doc line naming the only legal caller |
| m4 | The anchor remap was O(anchors squared) — ~100 ms per resize at 10 000 marks | `binary_search_by` over the already-sorted table |
| d2, d3 | The LLD's off-by-one bullet and the kind-scoped `kill_selection` | Recorded above for the design owner |
| perf 1-3 | Three constant-factor wins measured and proposed | Two applied and re-measured (above); the third recorded with its number |

### Shape differences from the design text

- **Module layout.** `crates/vt/src/reflow/` is a folder (`mod.rs`, `columns.rs`,
  `reflow_tests.rs`, `reflow_props.rs`) rather than the flat `reflow.rs` the LLD names, matching
  what `US-0075` did for the grid. Test paths are `reflow::tests::` and `reflow::props::`, so
  every filter the design names still selects the right tests.
- **The public entry point is `TerminalGrid::resize(size, policy) -> ResizeOutcome`,** not
  `Terminal::resize`: `Terminal` does not exist until `US-0079`. `ResizePolicy` and
  `ResizeOutcome` are re-exported from the crate root and are the whole public surface;
  `reflow_columns` and `measure_rows` are crate-private, as the LLD's Interfaces section says.
- **`Screen::resize` is replaced by four narrower entry points** — `resize_rows`,
  `truncate_columns`, `install_rows` and the two shift helpers `append_blank_rows` /
  `drop_trailing_rows` — because everything that touches the ring has to stay inside
  `screen.rs`. The complete list of edits to existing grid files:
  `screen.rs` (the resize section, `push_rows_with`, `take_row` made `pub(crate)`,
  `trim_history`, `restate_viewport`, `set_scroll_offset`, `set_cursor`,
  `set_saved_cursor_pos`), `row.rs` (`Row::from_cells`, the `LeadingWideSpacer` arm in
  `repair_wide_pairs`), `anchor.rs` (`Anchors::kill_selection`), `terminal_grid.rs` (the new
  `resize` signature), `grid_tests.rs` (six call sites gained the policy argument).
- **No `VtEvent::RowsTrimmed` is emitted** and no `GraphicReleased`: `US-0079` owns the event
  surface. `ResizeOutcome::rows_trimmed` and the anchor list carry the same information until
  then — a graphics placement whose anchor died is observable as `Anchors::get` returning
  `None`, which is what `graphics.md` asks the event to report.

### Gaps

- **Attribution beyond the file header.** The reflow's algorithm is credited to `avt`
  (Apache-2.0) in `crates/vt/src/reflow/columns.rs`'s header, with the statement that no `avt`
  source is copied — the iterator was written from the LLD's six-step description and from
  `research/prior-art.md` § 9.3, against OneTerm's own cell, row and anchor types. The LLD also
  asks for a line in `NOTICE` and in `THIRD-PARTY-NOTICES.md`; both files are outside this
  packet's file scope, and `THIRD-PARTY-NOTICES.md` is generated and CI-checked against
  `Cargo.lock`, so a hand-written section needs the generator's owner. **Assigned to `US-0087`**
  ("Decommission the fork … delete `vendor/`, the `[patch]` block, the `vte` dev-oracle **and the
  notices rows**; reconcile every remaining owning doc"), which is the packet that already opens
  both files — adding the `avt` line while removing the `alacritty_terminal` ones is one edit
  rather than two. The file header carries the credit until then. **Closed: `US-0087` (`c8d84ff`)
  wrote both lines** — `THIRD-PARTY-NOTICES.md` § 2.1 and the `avt` bullet in `NOTICE`.
- **The `measure_rows` fixtures are synthetic, not replayed captures (R-39).** BUG-0051's raw
  PTY and grid dumps came from instrumentation the document itself records as "both temporary
  and removed", so there is nothing to replay. `measure_rows_matches_the_recorded_conhost_rows`
  rebuilds the **geometry** § Measurements (b) records — 33x43 with eleven logical lines over the
  32 rows above the cursor, two of which still wrap at 132 columns; and 33x43 with 32 single
  rows, five long enough to split at 34 — and asserts the three recorded rows (11, 13, 37,
  0-based). That pins the rule, not the capture. **BUG-0051 does not record the `OpenConsole.exe`
  version it measured**, which R-39 requires; the test records what is knowable (Windows 11,
  2026-09-09, against the then-bundled pair) and points at
  `crates/app/assets/conpty-manifest.json`, which today carries `1.24.260710001` / file version
  `1.24.2607.10001`. Closing this needs a re-capture against a named host build, which is
  `IN-0030`'s bump checklist.
- **No lazy history reflow.** The cost table above is the argument for it and the LLD says it is
  out of scope. A drag-resize on a 100 000-row buffer still costs ~33 ms per frame.
- **The shrink is not backward and in place, and peak memory is roughly double the live cells.**
  `research/prior-art.md` § 9.3 recommends, from xterm.js's scar tissue, to "shrink **backwards,
  in place** (no temp buffer)"; the LLD's own step list does not restate it and this
  implementation does the opposite. `RingSink` builds the whole output into a `VecDeque` capped at
  `scrollback_limit + rows`, which `install_rows` then writes into the ring, so the old cells and
  the new ones are alive at once. It is not a free choice: against a ring whose slot for one row
  may be another row's destination, an in-place backward shrink has no safe order, and the cap is
  what keeps the write inside the ring at all. The reference has the same shape (`take_all` plus a
  `new_raw` vector). **The number:** at 160 columns a row's cells cost 1280 B, so a full
  1 000 000-row scrollback holds ~1.28 GB and the reflow's peak is ~2.56 GB. The fast path above
  does not help — it moves the `Row` rather than copying it, but the moved row still sits in the
  deque until `install_rows` runs. No test measures the peak.
- **No integration, E2E or platform proof.** Nothing in the workspace depends on `oneterm-vt`:
  the old `keep_viewport_top_*` suite still runs against the old engine (R-44), the parity gate
  is `US-0076` and the first time the engine is behind the application is `US-0081`. The
  cross-crate tests the LLD names (`local_session_grow_policy_matches_conpty`,
  `ssh_session_keeps_the_default_grow_policy`) keep their current meaning and are untouched.
- **`ResizeOutcome` is not yet anyone's input.** Nothing consumes `reflowed` or `rows_trimmed`
  until `US-0078` and `US-0079`; both are proven by tests only.
- **A `drop_trailing_rows` that would drop the cursor's row is clamped, not reported.** The LLD
  says a cursor with more text below it than fits "clamps to the last row"; the clamp is silent
  because there is no event surface yet to carry it.

### Two LLD corrections for the design owner

- **The `measure_rows` verification bullet is off by one.**
  [`low-level-design/reflow-and-resize.md`](low-level-design/reflow-and-resize.md), Verification,
  names "the three measured BUG-0051 cases (… giving row 12; … row 14; … row 38)". BUG-0051
  § Measurements (b) records `ESC[12;18H` / `ESC[14;18H` / `ESC[38;18H`, whose **0-based** rows
  are **11 / 13 / 37** — which is what `measure_rows_matches_the_recorded_conhost_rows` asserts
  and what the engine's row indices mean everywhere else. The bullet reads the CUP parameters
  1-based and will make the next reader think the test is wrong.
- **`Anchors::kill_selection` is kind-scoped where everything else is lane-scoped.**
  `crates/vt/src/grid/anchor.rs` — it matches on `AnchorKind::SelectionStart` / `SelectionEnd`
  and therefore kills a selection registered on the **alternate** screen's lane as well.
  Harmless today (there is one selection at a time, and a resize invalidates an alternate-screen
  selection anyway) but it is the single place where not adding the `AnchorKind` screen
  discriminant (reading 1) is observable, so it is the thing to revisit if a second selection is
  ever tracked.

## Handoff

Branch `worktree-agent-aeba210aa0ace1fb9` off `feat/vt-engine` @83933b9, **not merged and not
pushed**. `crates/vt/src/parser/` and `crates/vt/src/render/` are being implemented concurrently
on other branches and were not touched; `src/lib.rs` gained only the `pub mod reflow;` line and
its re-exports.
