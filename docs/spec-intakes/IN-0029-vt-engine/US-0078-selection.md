# Work: Selection

ID: US-0078
Intake: IN-0029
Created: 2026-09-13

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

- Change type: new capability (the engine's selection concern; nothing in `crates/vt` selected
  anything before this packet, and the public API the High-Level Design exposes —
  `selection_start` / `selection_update` / `selection_range` / `selection_text` /
  `selection_clear` / `select_all` — had no design until
  [`low-level-design/selection.md`](low-level-design/selection.md)).
- Risk lane: high_risk (the intake's lane). Selection is a user-visible, clipboard-facing
  behaviour: a wrong range copies the wrong text. Nothing in the workspace depends on
  `oneterm-vt` yet, so nothing user-reachable changes with this packet.
- Spec Intake, when required: IN-0029

## Outcome

`crates/vt` owns selection, against
[`low-level-design/selection.md`](low-level-design/selection.md):

1. **Four kinds** — `Simple`, `Block`, `Semantic`, `Lines`, each with the LLD's `to_range` rule,
   including reversed anchors, the side rules, and the "never empty" rule for `Semantic` and
   `Lines`.
2. **Anchors are tracked anchors** — both endpoints are `AnchorKind::SelectionStart` /
   `SelectionEnd` entries in the engine's one anchor list, so a selection rotates through scroll,
   `IL`/`DL`, a trim and a reflow by the mechanism that already exists, and the reference's
   `Selection::rotate` has no counterpart here.
3. **The invalidation matrix**, stated once in code, kind-scoped where the LLD says so.
4. **Text extraction** — wide chars once, spacers never, graphemes whole, tab runs collapsed to
   the next tab stop, trailing blanks trimmed by the reference's `line_length` policy, wrapped
   rows joined without a newline, hard newlines between rows, `Block` as a rectangle.
5. **The render hook** — `RenderState::selection()`, refreshed every update from
   `EngineView::selection`, which is what [`damage-and-render-state.md`](low-level-design/damage-and-render-state.md)
   names and what the migration gap table assigns to this packet.

## Scope

- [ ] In scope:
  - `crates/vt/src/selection/` — new: `mod.rs` (the types, the anchor plumbing, `to_range`,
    `hit_test`, the invalidation matrix), `expand.rs` (the semantic, bracket and logical-line
    walks), `text.rs` (extraction), `selection_tests.rs`, `selection_props.rs`.
  - `crates/vt/src/lib.rs` — the `pub mod selection;` declaration and its re-exports.
  - `crates/vt/src/render/state.rs` — additive: one `EngineView` field, one `RenderState` field,
    one accessor, and the `selection_changed` term in the `Unchanged` test (a drag over static
    content must still produce a frame).
- [ ] Out of scope:
  - `crates/vt/src/grid/screen.rs` and `crates/vt/src/grid/terminal_grid.rs` — **not touched.**
    The invalidation matrix is a predicate here rather than a `kill_selection` call sprinkled
    through the erase paths; see reading 4. A concurrent packet is reworking the blanking paths.
  - `crates/vt/src/terminal/` — a concurrent packet. `Terminal::selection_*` are thin wrappers
    over this module; see reading 1.
  - `crates/vt/src/grid/anchor.rs` — nothing needed; `register` / `get` / `set` / `release` /
    `shift_region` / `remap` / `trim` / `kill_selection` are all already there from `US-0075`
    and `US-0077`.
  - The click-count to `SelectionKind` mapping, which stays in the view
    ([`low-level-design/migration.md`](low-level-design/migration.md), `US-0085`).
  - `crates/terminal` and `crates/terminal-view` — the adapter packets (`US-0081`, `US-0085`).

## Acceptance

- [x] Every test the LLD's Verification section names exists and passes, under
  `cargo test -p oneterm-vt selection::`.
- [x] Each kind's `to_range` on a fixed grid, including reversed anchors.
- [x] Semantic selection over escape chars, wide chars and wrapped rows; bracket matching when
  the anchors coincide.
- [x] `Lines` across a wrapped logical line; `Block` with wide chars at both edges.
- [x] Rotation: a selection survives a scroll into history, `IL`/`DL` inside a region moves it,
  a trim kills it, the invalidation matrix cases kill it, and a column reflow round trip
  (narrow then widen) is compared by `selection_text` before and after.
- [x] Text extraction: trailing blanks trimmed per policy, a wide char once, a spacer never,
  wrapped rows joined without a newline, hard newlines between rows, an empty selection yields
  empty text.
- [x] A property test: for random grids and random anchors, `to_range` is ordered and within
  bounds, and `selection_text` length equals the covered visible cells' text.
- [x] `pwsh scripts/ci-local.ps1` green; no `unsafe`; no new dependency.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/selection.md` — this packet's contract:
  the four kinds, tracked anchors, fractional hit testing, the `to_range` table, semantic
  expansion and its escape chars, `selection_text`, the invalidation matrix, `select_all`, the
  edge cases and the verification list.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md` — the canonical
  `AnchorKind` list, the lane rule that makes `Anchors::trim` screen-scoped, `RowFlags::WRAPPED`
  as a **row** flag (deviation G1, which replaces the reference's per-cell `WRAPLINE` in every
  wrapped-line rule here), and the statement that selection invalidation is specified in
  `selection.md` rather than restated there.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/reflow-and-resize.md` — the anchor API,
  `Anchors::remap`, the `sync_anchors` ordering, trap 28, and the recorded exception that
  `Anchors::kill_selection` matches on **kind**, not lane.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md` — the
  `selection: Option<SelectionRange>` field and the `selection()` accessor on `RenderState`, and
  the rule that selection and the cursor are refreshed every update rather than carried by row
  damage.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` — the R-28
  integrity budget and the trap map; trap 28 is reflow's row and stays there.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/migration.md` — the view-needs gap table
  row "Selection range — `start`, `end`, `is_block`", which assigns this packet the render-state
  hook, and the `US-0085` row that keeps the click-count mapping in the view.
- `docs/spec-intakes/IN-0029-vt-engine/research/engine-semantics.md` § 2.18 and § 8 — the
  reference behaviour and the traps.
- `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md` — row ids name
  positions, not content, which is the whole reason both endpoints are anchors.
- `docs/terminal-backend.md` — the backend's selection surface today
  (`TerminalModel::{start_selection, update_selection, selection_text, has_selection,
  clear_selection, select_all}`), which this module must be able to reproduce.
- `vendor/alacritty_terminal/src/selection.rs`, `src/term/mod.rs:544-645`,
  `src/term/search.rs:465-620` — the reference implementation being replaced.

### Documentation Action

No contract change. `selection.md` describes the behaviour implemented here, and the shape
differences below are all consequences of types the intake's earlier packets already shipped
(`Terminal` does not exist yet; `Config` does not exist yet; `WRAPPED` is a row flag; `Pos`
carries a `RowId` rather than a viewport `Line`). They are recorded as readings for the design
owner rather than as edits to the LLD, which this packet must not modify.

Reason: the LLD is written against a `Terminal` that `US-0076` will introduce. Rewriting it now
would pre-empt that packet's own shape. Everything observable — the four kinds, the side rules,
the escape chars, the matrix, the extraction policy — is implemented exactly as written.

### Reconciliation

Docs changed: none. The no-change reason above still holds after the verification round. The
readings and the corrections are recorded here, in this packet, which is where the intake's
other packets record theirs.

Two sentences in `selection.md` are **wrong** and are the design owner's to correct; neither is
edited here, because this packet must not modify the LLD:

- the `to_range` table's `contains_cell` line, and the verification list's
  `contains_cell_extends_a_wide_char_to_its_spacer`, state the wide-cell membership rule
  backwards (reading 6);
- the invalidation matrix's `ED 0` / `ED 1` rows say "cleared when the range intersects the
  cleared rows", which is correct but reads as a one-sided bound — and a one-sided bound is
  exactly what this packet shipped until the verification round caught it (F1). The matrix is
  worth a sentence saying each erase is bounded at the screen top and bottom.

Correction **C15** (kill-over-clamp on a region scroll) needs the design owner's confirmation,
and the region-scroll split is a new behaviour nobody has specified. Both are under "Gaps".

## Context

### Shape differences from the LLD

1. **No `Terminal`.** The LLD's interfaces are `impl Terminal`. `crates/vt/src/terminal/` is
   being built by a concurrent packet, so this module is written against the types that do
   exist and the `Terminal` methods become one-line wrappers:

   | LLD | This packet |
   | --- | --- |
   | `Terminal::selection_start(pos, side, kind)` | `Selection::new(&mut TerminalGrid, kind, pos, side)` |
   | `Terminal::selection_update(pos, side)` | `Selection::update(&mut self, &mut TerminalGrid, pos, side)` |
   | `Terminal::selection_range()` | `Selection::to_range(&self, &TerminalGrid, escape_chars) -> Option<SelectionRange>` |
   | `Terminal::selection_text()` | `Selection::text(&self, &TerminalGrid, &Interner, escape_chars) -> Option<String>` |
   | `Terminal::selection_clear()` | `Selection::release(self, &mut TerminalGrid)` |
   | `Terminal::select_all()` | `Selection::all(&mut TerminalGrid) -> Selection` |
   | `Terminal::hit_test(row, col)` | `selection::hit_test(&TerminalGrid, row, col) -> (Pos, Side)` |
   | *(no LLD method — deviation 4 below)* | `Selection::invalidated_by(&TerminalGrid, Invalidation)`, which `Terminal`'s `EL` / `ED 0` / `ED 1` / `ED 2` / `ED 3` / `RIS` / `swap_alt` dispatch must evaluate **before** performing the operation, clearing the selection when it returns `true`. Nothing clears a selection today because nothing asks |

   `Terminal` will hold `selection: Option<Selection>` and route each method through it. That is
   also why `release` consumes `self`: the anchors must be released exactly once, and the
   type system says so.

2. **No `Config`.** `Config::semantic_escape_chars` does not exist (`US-0076` owns the mode and
   config table), so the escape set is a `&str` parameter on the two entry points that need it,
   with the LLD's default published as `selection::SEMANTIC_ESCAPE_CHARS`.

3. **`WRAPPED` is a row flag** (deviation G1, `grid-and-scrollback.md`). Every reference rule
   phrased as "the cell at the last column carries `WRAPLINE`" is implemented as
   `screen.row(id).wrapped()`. This is strictly better defined: the reference reads a flag off
   a cell that an `EL` can erase.

4. **The invalidation matrix is a predicate, not a call site.** `Selection::invalidated_by(grid,
   Invalidation)` answers the matrix for the seven operations that blank or replace content
   without moving a row. The row-**moving** half of the matrix (`SU` / `SD` / `IL` / `DL` / `RI`,
   a scroll into history, a resize) needs no code at all: those primitives already call
   `Anchors::shift_region`, `Anchors::trim` and `Anchors::remap`, and a dead anchor makes
   `to_range` return `None`, which is the LLD's "the selection is cleared". Putting the erase
   half in the same place keeps one mechanism and keeps this packet out of `screen.rs`, whose
   blanking paths a concurrent packet is reworking.

5. **`contains_cell` takes `Option<Pos>` for the block cursor.** The reference takes a
   `CursorShape`, which is `US-0076`'s type. `Some(pos)` means "a block-shaped cursor is at
   `pos`"; `None` means every other shape. The quirk itself — a block cursor sitting exactly on
   a selection corner is not inverted — is reproduced unchanged.

### Readings for the design owner

1. The `Terminal` wrappers above are the only thing `US-0076` owes this module.
2. `SEMANTIC_ESCAPE_CHARS` is byte-for-byte the reference's `,│`|:"' ()[]{}<>\t`, as the LLD
   requires. It is a `&str` and matching is by `char`, so a multi-byte escape char such as `│`
   works.
3. `Selection::all` registers `(oldest, 0)` and `(newest, cols - 1)` on the **active** screen's
   run, not on the primary's. On the alternate screen that is the alternate lane, which is the
   only sensible reading of "covering history and viewport" when the alternate screen has no
   history.
4. The matrix's `EL`/`ED 0`/`ED 1` rows are evaluated **before** the operation runs, because
   each is about the rows the operation is going to touch and the cursor row is the input. The
   enum's doc comment says so.
5. The reference's `line_to_string` tail rule — "if a wide char is not part of the selection but
   its leading spacer is, include it" — reads
   `self.grid[line - 1i32][Column(0)].c`, which is the row **above**; a `LeadingWideSpacer` at
   the end of a row belongs to the glyph on the row **below**. This packet implements the rule
   the way it is described rather than the way it is coded, and pins it with
   `text_includes_a_wide_glyph_whose_leading_spacer_ends_the_selection`.
6. **The LLD's `to_range` table and its test name state the wide-cell membership rule
   backwards.** They say "a `Wide` cell's membership extends to its `WideSpacer`"; the reference
   asks the inverse — `vendor/alacritty_terminal/src/selection.rs:82-84`, comment "Check if a
   wide char's trailing spacer is selected" — so a selected **spacer** pulls in its `Wide`
   partner, and selecting only the glyph leaves the spacer unpainted. The code implements the
   reference; the doc comment and the test name were corrected here, and the LLD sentence is the
   design owner's to fix.
7. `semantic_search_right` steps back one cell without skipping spacers, where
   `semantic_search_left` steps forward one cell **and** skips them. The asymmetry is the
   reference's; it is not observable in the extracted text, because a range whose end lands on
   a `WideSpacer` still covers its `Wide` partner at `end - 1`. Reproduced rather than
   "fixed", and named here so the choice is visible.

## Plan

- [x] Write this packet and mirror the story row into `harness.db`.
- [x] `selection/mod.rs` — types, anchor plumbing, `to_range` dispatch, `hit_test`, the matrix.
- [x] `selection/expand.rs` — the cell walker, `semantic_search_*`, `line_search_*`,
  `bracket_search`.
- [x] `selection/text.rs` — `line_text`, `bounds_text`, the block rectangle.
- [x] `selection/selection_tests.rs`, `selection/selection_props.rs`.
- [x] `render/state.rs` — the selection field, the accessor and the `Unchanged` term.
- [x] `pwsh scripts/ci-local.ps1`.

## Decisions

- `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md` — clause 1 (row ids
  name positions) is why both endpoints are anchors; clause 2 (the incremental render state) is
  why the selection range is a field on `RenderState` rather than a question the painter asks
  the engine a second time.

No new decision. Nothing here is a choice future work must inherit that the LLD does not
already state.

## Verification Plan

- `cargo test -p oneterm-vt selection::` — the LLD's named tests plus the intake's outcome list.
- `cargo test -p oneterm-vt render::` — the render-state hook, including the new
  `a_selection_change_alone_reports_a_frame`.
- `VT_PROPTEST_CASES=10000 cargo test -p oneterm-vt selection::props` — the ordering, bounds and
  text-length property at depth.
- `pwsh scripts/ci-local.ps1` — the full gate, raw `test result:` totals recorded below.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Base correction

The worktree was created on `main` (`c936ac0`), which has no `crates/vt` at all. It was
`git reset --hard 4b833a0` — the tip of `feat/vt-engine`, `US-0077`'s last commit — before any
file was read or written. Same correction `US-0079` recorded.

### Verification round

Independent verification at `9ef7d9c`:
[`evidence/US-0078-verify.md`](evidence/US-0078-verify.md). **Verdict: merge after fixes** — one
correctness defect, two documentation-accuracy findings, four minors. The verifier reproduced
every gate and number, ran the properties at 20 000 cases, and wrote fourteen probes, two of
them **exhaustive oracles**: the reference's `is_empty` + `range_simple` / `range_block`
transcribed literally and compared against `to_range` over all 576 anchor combinations on a
3×4 grid per kind. `Simple` diverged **0/576**. `Block` diverged 24/576, every one of them
declared correction 1, and two of those were also out of bounds in the reference.

All findings are addressed here:

| # | Finding | Fix |
| --- | --- | --- |
| **F1** | `ED 1` cleared a selection lying wholly in history: `EraseAbove` had no lower bound, where the reference bounds the erase at the screen top (`term/mod.rs:1791`) | `bottom >= screen.screen_top()` added, and the mirror bound on `EraseBelow` for symmetry. `invalidation_matrix` now asserts a history-only selection survives `EL`, `ED 0` and `ED 1` |
| **F2** | The region-scroll gap claimed the reference deletes the selection. Measured, it does not | Gap rewritten: both engines split it, this is parity, and the coherent-split rule is a **new** behaviour for the design owner |
| **F3** | The real divergence — an endpoint scrolled out of a region top is killed here, clamped to `(range_top, 0, Left)` there — was undeclared | Declared as **correction C15** with `an_endpoint_scrolled_out_of_a_region_top_is_killed_not_clamped`; kill-over-clamp routed to the design owner |
| **F4** | `contains_cell`'s wide-cell prose and test name were inverted (the code was right) | Doc comment reworded, test renamed, and a second assertion added proving the rule does **not** run the other way. Reading 6 hands the LLD sentence to the design owner |
| **F5** | `Selection::kind()` was dead public surface | Removed |
| **F6** | The `US-0076` table omitted the `invalidated_by` obligation | Row added |
| **F7** | `bracket_search` is unbounded on an unmatched bracket (parity with the reference) | Recorded as a gap, owner `US-0076` |
| **F8** | The adopted tail glyph must be a scalar, so a clustered one is dropped | Recorded as a gap, owner this module |

Also noted by the verifier, no action: `clamp` clamps row and column independently where the
reference's `Point::grid_clamp` snaps to a corner. Unreachable — anchors are only set from live
positions and `hit_test` clamps first — but a later `Terminal::selection_update` from a stale
viewport could reach it.

Verifier's measurement of PERF-14, on a 100 000-line scrollback with a 99 000-row selection:
`to_range` **1 µs**, `text` **97 ms** for 2 564 975 bytes. Three orders of magnitude apart, which
is the `O(1)` / `O(n)` split the design asks for.

### Upstream merge

`feat/vt-engine` moved to `02962f4` mid-packet (`US-0075` rework: `Screen::blank_row` as the
single blanking path, `RenderRow::allocated` removed). Merged cleanly — the rework touches
`grid/screen.rs`, `grid/grid_tests.rs`, `grid/grid_props.rs` and `render/row.rs`, none of which
this packet edits, and `render/state.rs` / `render/render_tests.rs` auto-merged. The whole suite
is green on the merge.

### Commands

All numbers below are **after** the verification round's fixes.

| Command | Result |
| --- | --- |
| `pwsh scripts/ci-local.ps1` | **green** — fmt, clippy `-D warnings`, `cargo test --workspace`, and all five Python policy checks |
| `cargo test --workspace` | raw totals over **56** `test result:` sections: **1456 passed / 0 failed / 8 ignored** (the `US-0077` baseline on this branch was 54 sections / 1317 / 6; this packet's 47 new tests and the merged `US-0075` rework account for the delta. Before the fixes: 1455, the delta being correction C15's test) |
| `cargo test -p oneterm-vt` | 268 passed / 0 failed / 2 ignored |
| `cargo test -p oneterm-vt selection::` | **46 passed / 0 failed** in 0.05 s — 42 `selection::tests`, 4 `selection::props` |
| `VT_PROPTEST_CASES=10000 cargo test -p oneterm-vt selection::props` | 4 passed in 0.77 s; the verifier repeated it at 20 000 in 3.24 s |

### What the tests pin

Every test the LLD's Verification section names exists, under its own name where the name still
described the behaviour and under a corrected one where it did not:

| LLD name | Here |
| --- | --- |
| `simple_side_rules_drop_the_right_cells` | same |
| `simple_is_empty_when_anchors_coincide_or_are_adjacent` | same |
| `block_normalises_columns_not_rows` | same |
| `block_text_extracts_a_rectangle` | same |
| `semantic_expansion_uses_the_escape_chars` | same |
| `semantic_stops_at_an_unwrapped_row_boundary` | same |
| `semantic_bracket_match_when_the_anchors_coincide` | same |
| `lines_expands_across_wrapped_continuations` | same |
| `text_joins_wrapped_rows_without_a_newline` | same |
| `text_skips_wide_spacers_and_emits_whole_graphemes` | same |
| `text_emits_tab_cells_as_tabs` | same |
| `contains_cell_extends_a_wide_char_to_its_spacer` | **renamed** `contains_cell_pulls_in_the_wide_partner_of_a_selected_spacer` — the LLD's name states the rule backwards; see reading 6 |
| `range_is_o1_and_does_not_allocate` | same |
| `anchors_follow_a_region_scroll` | same |
| `selection_clears_when_an_anchor_is_trimmed` | same |
| `selection_survives_a_row_only_resize_and_clears_on_a_column_resize` | same |
| `viewport_scrolling_does_not_change_the_range` | same |
| `invalidation_matrix` | same, table-driven over the seven operations plus the history case |
| `hit_test_side_and_clamping` | same |
| `anchors_are_released_on_clear` | same |

Beyond the list: `reversed_anchors_produce_the_same_range_for_every_kind`,
`the_last_half_cell_of_a_row_through_the_first_of_the_next_keeps_that_cell`,
`block_is_empty_by_the_column_rule_alone`, `block_that_would_invert_its_columns_is_empty`,
`block_text_trims_each_row_on_its_own`, `block_with_wide_chars_at_the_edges`,
`semantic_runs_across_a_wrapped_row`,
`semantic_expands_over_wide_chars_without_splitting_a_pair`, `semantic_is_never_empty`,
`lines_is_never_empty`, `text_puts_a_newline_between_unwrapped_rows_and_trims_trailing_blanks`,
`text_includes_a_wide_glyph_whose_leading_spacer_ends_the_selection`,
`text_of_an_empty_selection_is_empty`, `select_all_covers_history_and_the_viewport`,
`a_block_cursor_on_a_corner_is_not_inverted`,
`insert_lines_inside_a_region_moves_the_selection`,
`delete_lines_inside_a_region_moves_the_selection_and_kills_a_deleted_one`,
`a_selection_survives_a_scroll_into_history`,
`a_column_reflow_round_trip_carries_the_anchors_and_the_text`,
`an_endpoint_scrolled_out_of_a_region_top_is_killed_not_clamped` (correction C15),
`a_selection_reads_the_screen_its_anchors_are_on`,
`a_cell_carrying_only_a_grapheme_id_still_reads_as_text`, and in
`crates/vt/src/render/render_tests.rs`,
`selection_change_only_returns_partial_and_refreshes_the_range`.

Properties (`selection::props`): `to_range_is_ordered_and_within_bounds` and
`text_covers_exactly_the_cells_in_the_range` over random grids, random wrap flags, all four
kinds and both sides on both anchors; `a_region_scroll_leaves_the_range_well_formed`; and
`the_property_grid_is_dense`, which stops the first two being vacuously true.

### Rotation and reflow, concretely

- **Scroll into history** — `a_selection_survives_a_scroll_into_history`: five line feeds later
  the range starts five rows above the screen top and `selection_text` is still `"aaa"`.
- **`IL` / `DL`** — the selection moves from screen row 1 to row 2 (`IL`) and from row 2 to row 1
  (`DL`), with the same text; a selection on the row `DL` discards resolves to `None`.
- **Region scroll** — `anchors_follow_a_region_scroll`: `SU 1` over rows 1..4 moves the selection
  from row 2 to row 1 with the text unchanged. The reference needs `Selection::rotate` here.
- **Trim** — with a two-row scrollback the selection resolves to `None` once its row is dropped.
- **Reflow** — `a_column_reflow_round_trip_carries_the_anchors_and_the_text`: 10 → 6 → 10
  columns; `"cdefghijklmno"` before, `None` for the selection afterwards (trap 28), and
  `"cdefghijklmno"` again from the two `AnchorKind::Mark` anchors that rode the same
  `Anchors::remap`. That single test carries both halves of the intake's outcome line and pins
  `kill_selection` as **kind**-scoped.

### Declared corrections over the reference

1. **`range_block` never returns an inverted rectangle.** A one-column block whose end cannot
   step left (`end.col == 0`) leaves `start.col > end.col` in the reference, which its own
   `SelectionRange::new` would assert on. Here it is empty.
   Test: `block_that_would_invert_its_columns_is_empty`.
2. **`range_simple` has the same guard**, though it is unreachable: the end is adjusted before
   the start, so the two coincide before the start could overshoot. Kept so the property holds
   by construction rather than by argument, and the reference's outcome is pinned by
   `the_last_half_cell_of_a_row_through_the_first_of_the_next_keeps_that_cell`.
3. **The `LeadingWideSpacer` tail rule reads the row below**, not the row above. See reading 5.
4. **Block text passes `include_wrapped_wide` only on the last row**, where the reference passes
   `start.column != 0` on every other row. The reference's condition has no stated meaning and
   would emit the same glyph on several rows of one rectangle.
5. **C15 — an endpoint scrolled out of a region top is killed, where the reference clamps it.**
   A region scroll discards the rows at the region top. The reference keeps the selection alive
   and rewrites the discarded endpoint to `(range_top, column 0, Side::Left)` for every
   non-`Block` kind (`vendor/alacritty_terminal/src/selection.rs:160-166`, pinned by its own
   `rotate_in_region_up`). Here the anchor dies with its content and the selection resolves to
   `None`.

   Deliberate, and the reason is `selection.md`'s own: the clamp leaves the selection covering
   text the user never selected, which is what the design calls "pointing at unrelated text". It
   also needs no selection-specific code — `Anchors::shift_region` already kills an anchor in a
   blanked range, which is the same mechanism `IL`/`DL`/`SU` use.

   **The design owner confirms kill-over-clamp.** Test:
   `an_endpoint_scrolled_out_of_a_region_top_is_killed_not_clamped`. Numbered C15 as the next
   free correction id after `US-0077`'s C14.

### Gaps

- **A region scroll can split a selection, in both engines.** When a scroll region contains one
  endpoint and not the other, the anchor list moves that endpoint alone, so the selection grows
  or shrinks and can end up with an unrelated row interposed.

  **This is parity, not a regression.** `Selection::rotate`
  (`vendor/alacritty_terminal/src/selection.rs:137-189`) moves an endpoint only when it lies in
  `range_top..range_bottom`, so an endpoint outside the region stays exactly where
  `Anchors::shift_region` leaves it here; hand-running `rotate` on the same selection produces
  the same split and the same interposed blank row. The reference returns `None` in only two
  sub-cases — a scroll **down** that rotates the start past `range_bottom` while the end is
  still inside, and the end overtaking the start — neither of which is the split case. An
  earlier version of this bullet claimed the reference deletes the selection; it does not, and
  the verifier measured it.

  So this is a **new behaviour for the design owner**, not a defect against the reference:
  neither engine keeps a partially contained selection coherent. The property
  `a_region_scroll_leaves_the_range_well_formed` asserts what does hold — the result is ordered,
  in bounds and materialisable. Closing it needs a rule nobody has written: either a
  `SelectionKind`-aware rotation hook on `Anchors` or a post-scroll consistency check on
  `Terminal`. Owner: the `IN-0029` design owner.
- **F7 — `bracket_search` is unbounded.** An unmatched bracket walks to the oldest or newest
  live row: eight million cells on a 100 000 × 80 scrollback. The reference's `iter_from` does
  the same, so this is parity, and a double-click on a lone `(` is rare. A row or cell budget
  belongs with the packet that gives `Terminal` a cost model. Owner: `US-0076`.
- **F8 — the adopted tail glyph must be a scalar.** `text.rs` matches only
  `CellContent::Scalar` when adopting the glyph a `LeadingWideSpacer` pushed onto the next row,
  so a clustered glyph there (an emoji with a variation selector) is silently omitted; the
  reference pushes `cell.c` unconditionally. One `match` arm, and it needs the interner lookup
  that arm currently avoids. Owner: this module, at the first packet that touches it.
- **The invalidation matrix is not wired to anything**, because nothing calls it yet: `Terminal`
  and the dispatch table are `US-0076`'s. The matrix is a predicate with its own test; the
  packet that adds `ED`/`EL` dispatch must call it. `grid/screen.rs` and
  `grid/terminal_grid.rs` are deliberately untouched, so `RIS` and `swap_alt` do **not**
  currently kill a selection by themselves — the caller must ask.
- **A grapheme cell is never an escape character or a bracket.** `to_range` does not take the
  interner (PERF-14), so a cluster reads as `NUL`. A bracket or a space carrying a combining
  mark therefore behaves as word content, where the reference matches on the base scalar. Fixing
  it costs an `&Interner` parameter on `to_range`; recorded rather than taken.
- **`range_is_o1_and_does_not_allocate` proves the weaker half.** A counting allocator needs
  `GlobalAlloc`, an `unsafe` trait this crate does not have, so the test watches
  `TerminalGrid::heap_bytes()` across a 1 500-row selection and checks the returned type is a
  small `Copy` value. `US-0075` and `US-0079` set the same precedent.
- **No integration, E2E or platform proof.** Nothing in the workspace depends on `oneterm-vt`
  yet; the eleven search tests and the mouse tests keep running against the old engine until
  `US-0082` / `US-0085` (R-44). The `SelectionKind` the view maps click counts to is still the
  fork's.
- **`hit_test` takes the *active* screen's viewport** while `to_range` resolves against the
  screen the anchors are on. A selection made on the primary screen and dragged while the
  alternate screen is active would mix the two — except that the matrix clears the selection on
  `swap_alt`, so the situation cannot arise once the caller asks. Named because it is an
  invariant the caller upholds rather than one the types enforce.
- **`Selection` is `Copy`.** Copying it duplicates two anchor handles, and releasing twice would
  free slots another selection may already have taken. It is `Copy` because the tests hold it by
  value across `&mut TerminalGrid` calls; `Terminal` will own exactly one in an `Option`, and
  `release` consuming `self` is what keeps that honest. Dropping `Copy` when `Terminal` lands
  would make it stricter still.

## Handoff

Branch `worktree-agent-a3a509ddbf185b930`, off `feat/vt-engine` @ `4b833a0` and merged up to
`02962f4`. Not merged into `feat/vt-engine`, not pushed.

Next owner: the integrator for `IN-0029`. Then:

- **`US-0076`** — the seven `Terminal::selection_*` wrappers in the table above, and the
  `Invalidation` call in the `EL` / `ED` / `RIS` / `swap_alt` dispatch paths. Nothing clears a
  selection today because nothing asks.
- **`US-0079`'s owner** — the `EngineView::selection` field this packet added, and the
  `selection_changed` term that stops a drag over static content returning `Unchanged`.
- **`US-0085`** — the view's click-count to `SelectionKind` mapping and `render/frame.rs`'s
  `SelectionRange`.
- **The design owner** — the region-scroll split under "Gaps" is the one behaviour the anchor
  list cannot express, and the choice of mechanism is theirs.
