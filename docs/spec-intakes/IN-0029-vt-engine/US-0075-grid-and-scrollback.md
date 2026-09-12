# Work: Grid, scrollback and tracked anchors

ID: US-0075
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

- Change type: new capability (the screen model of `oneterm-vt`, on top of `US-0074`'s storage)
- Risk lane: high_risk (the intake's lane; this packet ships no product-reachable behaviour yet —
  nothing in the workspace depends on `oneterm-vt`)
- Spec Intake, when required: IN-0029

## Outcome

The grid half of the engine exists and is proven against
[`low-level-design/grid-and-scrollback.md`](low-level-design/grid-and-scrollback.md):

1. **Storage** — a power-of-two ring of **lazily allocated** rows (`Option<Row>`, `None` until
   written), whose length is `next_power_of_two(scrollback_limit + MAX_ROWS)` and therefore a
   session constant (R-30). The ring is allocated once; `MAX_ROWS = 1024`, `MAX_COLS = 2048`,
   `SCROLLBACK_MAX = 1_000_000`.
2. **Identity** — positional `RowId`, where the ring index **is** the id (`slot = id & mask`),
   monotonic per screen, never reset by `RIS` / `ED 2` / `ED 3`, and never ambiguous between the
   two screens.
3. **Viewport** — the sticky-bottom `offset` (distance from the newest row), with every row of the
   LLD's per-operation table implemented and tested.
4. **Tracked anchors** — the canonical `AnchorKind` list, `Anchors::{register, get, set, release,
   shift_region, remap, trim}`, updated by **every** row-moving primitive, plus the `RowsScrolled`
   / `RowsTrimmed` scroll-damage reports.
5. **Scroll regions** — `DECSTBM` validation, all four `scroll_up` cases including the
   bottom-bounded region, `scroll_down` never pulling from history, and the cursor-outside-region
   rules for `IL` / `DL`.
6. **Alternate screen** — two screens over one id space, `? 1049` entry clobbering the primary
   `DECSC` slot, exit restoring the entry cursor.
7. **Print path, erase, insert, delete** — pending wrap, wide pairs (same row and across rows),
   insert mode, zero-width attachment, tab stops, `EL` / `ECH` / `DCH` / `ICH` / `ED`, with
   corrections **C1-C4** implemented spec-correct.
8. **Integrity** — `assert_integrity()` in debug builds, and the live-heap figure for a
   100 000-row empty scrollback that the intake's memory model claims.

## Scope

- [ ] In scope:
  - `crates/vt/src/grid/` — `mod.rs`, `row.rs`, `anchor.rs`, `screen.rs`, `terminal_grid.rs` and
    the three test files `grid_tests.rs`, `anchor_tests.rs`, `grid_props.rs`.
  - `crates/vt/src/lib.rs` — the `pub mod grid;` declaration and its re-exports.
- [ ] Out of scope:
  - `crates/vt/src/parser/` (`US-0073`, implemented concurrently) and `dispatch` (`US-0076`).
    This packet exposes the primitives an escape sequence calls; it decodes nothing.
  - Reflow and `Terminal::resize(size, policy)` (`US-0077`). `Screen::resize` here is the
    rows-only, non-reflowing form the two resize tests this packet owns need; the column path
    truncates and pads instead of reflowing, and `US-0077` replaces it.
  - Selection (`US-0078`), `RenderState` and `VtEvent` (`US-0079`), graphics (`US-0080`).
    The row header carries the `SeqNo` and the dirty bit those packets read; nothing else.
  - Corrections `C8` (`? 47` / `? 1047` / `? 1048`) and `C10` (`CSI ? 5 W`) — `US-0076`.
  - `? 45` reverse wrap — `US-0086`.
  - `expected-diffs.json` for `C1`-`C4`: the parity harness runs for the first time against the
    new engine in `US-0076`, so the measured cells are written there, from this packet's
    behaviour. See "Evidence and Gaps".

## Acceptance

- [x] Every test the LLD's Verification section names for `grid::` and `anchor::` exists under the
      name the LLD gives it, plus the four trap-map rows re-homed here (traps 5, 7, 8 and the
      cross-row half of 6) and `grid::props::scroll_and_erase_preserve_integrity`.
- [x] The viewport-offset table is proven row by row (R-01): sticky bottom, a scrolled-back view
      holding still, the `history_len()` cap, `ED 2` keeping the offset, `ED 3` snapping to 0.
- [x] All four `scroll_up` cases behave as tabulated (R-03), including the bottom-bounded region
      that keeps the rows below it, and the short region that rotates and then blanks (C3).
- [x] The two screens' live row ranges never overlap, and `assert_integrity` checks it (R-04).
- [x] `lines_produced` counts output lines, not wraps, and does not change across a clear, an
      alternate-screen swap or a resize (R-05).
- [x] Anchors move with their content through every primitive, die when their content is blanked
      or trimmed, and match the `RowsScrolled` report (R-02).
- [x] `C1` (`DCH` is a plain shift), `C2` (`ED 1` clears row 0), `C3` (a short region scroll
      rotates then blanks) and `C4` (insert mode repairs wide pairs) are implemented spec-correct,
      each with a test.
- [x] An unwritten row costs no cells: a 100 000-row empty scrollback allocates only its ring
      slots, measured and reported in bytes per row against the old engine's 4138 B/row.
- [x] `assert_integrity()` is invoked at the end of every mutating public method in debug builds,
      at the tier R-28 mandates, and the debug-suite runtime is measured and under the 60 s budget.
- [x] No `unsafe` and no new external dependency.
- [x] `pwsh scripts/ci-local.ps1` is green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md` — the contract
  this packet implements: storage, identity, the viewport table, anchors, scroll regions, the
  alternate screen, the print path, erase / insert / delete, the deviations and the corrections.
- `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md` — positional
  `RowId`, engine-owned anchors, the sticky-bottom offset, `lines_produced()`, and the per-row
  sequence number the render state reads.
- `docs/spec-intakes/IN-0029-vt-engine/high-level-design.md` — the memory table (row slot at
  about 48 B, the ring fixed for the session, the caps), the ownership and threading model (the
  engine holds no lock and spawns no thread), and the P3 / P5 / P15 / P16 / P23-P26 problem rows
  this packet closes.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/cell-and-style.md` — `Cell`,
  `Interner`, `repair_wide_pair_in_row`, `is_erasable` / `is_blank`, and the `'\t'` cell rule.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md` — the row
  header fields this packet owns (`seq`, `RowFlags::DIRTY`) and the `RowsScrolled` contract.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/reflow-and-resize.md` and
  `selection.md` — the anchor API they will consume (`remap`, `shift_region`, `AnchorKind`), kept
  compatible.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` — the trap-map rows
  owned here, the integrity-check budget (R-28) and the 60 s debug-suite budget.
- `docs/spec-intakes/IN-0029-vt-engine/research/engine-semantics.md` § 2, § 3, § 8 — the
  reference behaviour each primitive is stated against.
- `docs/terminal-backend.md` — the consumer-facing contract the engine must eventually satisfy
  (no lock held through paint, the ConPTY resize policy, the scrollback default).
- `docs/agents/{structure,crate-dependency-rules,dependencies,code-style,error-policy}.md` — crate
  placement, R1-R12, dependency policy, conventions, and the "never panic on untrusted input" rule.

### Documentation Action

**No contract change.** The LLDs, the HLD and `DEC-0015` already describe the behaviour this
packet implements, down to the test names. `docs/agents/structure.md` already carries `crates/vt`
from `US-0074`; this packet adds modules inside it, not a crate. `docs/terminal-backend.md` still
describes the engine being replaced and stays accurate until the shim packet (`US-0081`).

Reason: this packet is an implementation of an accepted high-risk design. Every place the
implementation had to choose a reading is recorded under "Evidence and Gaps" for the design owner
rather than silently written into the LLD, because the packet may not edit the intake documents.

### Reconciliation

**No owning doc changed, and the recorded no-change reason still holds.** Every behaviour this
packet implements was already written down in `grid-and-scrollback.md`, `DEC-0015` and the HLD,
including the test names. `docs/agents/structure.md` already carries `crates/vt` from `US-0074`;
this packet adds modules inside an existing crate, not a crate. `docs/terminal-backend.md`
describes the engine being replaced and stays accurate until `US-0081`.

Where the design text was ambiguous or self-contradictory the implementation took the reading
recorded under "Evidence and Gaps" rather than editing the intake documents, which this packet may
not do. The seven items marked **for the design owner** there are the ones worth folding back into
the LLD.

## Context

- The crate has no `Terminal` yet (`US-0079` owns it), but the shared row-id space, the shared
  anchor list, `lines_produced` and the two-screen integrity assertion all need an owner. This
  packet introduces `TerminalGrid` for exactly that role: the grid half of the future `Terminal`,
  which will hold one.
- `Screen` does not own the `Interner`; the methods whose rule reads a style or a cluster take it
  as a parameter, the same way `Cell::is_erasable` does since `US-0074`.
- The engine is fed by untrusted bytes. Nothing here returns an error and nothing here panics:
  out-of-range counts clamp, an impossible placement is dropped and counted.

## Plan

- [ ] `grid/mod.rs` — the vocabulary (`RowId`, `Pos`, `Size`, `Viewport`, `ScrollRegion`, the
      caps) and the module declarations.
- [ ] `grid/row.rs` — `Row`, `RowHeader`, `RowFlags`, `SeqNo`, `RowRef` / `RowMut`, the `occ`
      reset rule (trap 36).
- [ ] `grid/anchor.rs` — the canonical anchor list and its four mutators, plus `anchor_tests.rs`.
- [ ] `grid/screen.rs` — one screen: the ring, the viewport offset, the cursor and pending wrap,
      the print path, scroll regions, erase / insert / delete, tab stops, integrity.
- [ ] `grid/terminal_grid.rs` — the two screens, the shared id space and anchor list,
      `lines_produced`, `swap_alt`, the two-screen integrity walk.
- [ ] `grid/grid_tests.rs`, `grid/grid_props.rs` — the suite the LLD names.
- [ ] Measure the live heap of a 100 000-row empty scrollback and the debug-suite runtime.

## Decisions

- `DEC-0015` — absolute row ids and the incremental render-state contract. No new decision: every
  choice this packet made is either the LLD's or is recorded as a gap for the design owner.

## Verification Plan

- Focused: `cargo test -p oneterm-vt grid::` and `cargo test -p oneterm-vt anchor::`, with the raw
  `test result:` totals recorded.
- Property: `grid::props::scroll_and_erase_preserve_integrity` over a random sequence of scrolls,
  erases, inserts and deletes with anchors registered throughout.
- Regression: `pwsh scripts/ci-local.ps1` — the same set CI runs, including
  `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`.
- Budget: `cargo test -p oneterm-vt` wall time recorded against the 60 s debug-suite budget (R-28).
- Memory: the live-heap probe reports total bytes and bytes per row for a 100 000-row empty
  scrollback, next to the old engine's 4138 B/row baseline from `US-0072`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Commands and results

- `pwsh scripts/ci-local.ps1` — **green**, every step including
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `python scripts/verify-dependency-graph.py` (21 packages),
  `python scripts/check-english.py` (681 files) and
  `python scripts/third-party-notices.py --check`.
- `cargo test --workspace` — raw totals summed over the 54 `test result:` sections:
  **1267 passed / 0 failed / 5 ignored**. Against this branch's base the delta is exactly the
  **56 new `grid::` tests** (`cargo test -p oneterm-vt -- --list` counts 89 total, against 33
  before).
- `cargo test -p oneterm-vt` — **89 passed / 0 failed**, `finished in 0.46 s`. The R-28
  debug-suite budget is 60 s, so there is headroom for `US-0077`'s reflow property tests.
- Test breakdown: 46 `grid::tests::`, 9 `grid::anchor::tests::` (the path contains
  `anchor::tests::`, so `cargo test -p oneterm-vt anchor::` still selects them), 1
  `grid::props::scroll_and_erase_preserve_integrity` over 256 proptest cases with the full
  two-screen integrity walk after **every** operation.
- No `unsafe` anywhere in `crates/vt`; no dependency added (`Cargo.toml` untouched).

### Memory: the intake's exit measurement

`grid::tests::empty_scrollback_costs_only_its_ring_slots`, 45x160 with a 100 000-row scrollback
filled entirely by line feeds and never written:

| | This engine | Old engine (`US-0072`) |
| --- | --- | --- |
| 100 000 empty scrollback rows | **6 291 616 B total, 62.9 B/row** (131 072 ring slots x 48 B) | 4138 B/row |
| 100 000 written 160-column rows | **134 347 936 B total, 1343.5 B/row** (62.9 B of ring share + 1280 B of cells) | 4138 B/row |

**65.8x less for empty scrollback, 3.1x less for written rows.** `size_of::<Option<Row>>() == 48`
and equals `size_of::<Row>()`, so the `Option` is niche-packed into the row's `Vec` exactly as the
HLD's memory table assumes, and `Screen::allocated_rows()` is **0** after all 100 000 rows: not one
row of cells is materialised.

**The lazy-row win is conditional on the background.** A row is left unallocated only when the erase
cell is `Cell::EMPTY`; a program holding a non-default background (`CSI 4x m` then a clear, which is
what `vim_24bitcolors_bce` does all day) makes every reset materialise its row, and the empty
figure above becomes the written one. That is correct background-erase behaviour, not a defect, but
it is the boundary of the claim and belongs in the HLD's memory table.

The figure is read off the structure's own capacities (`Screen::heap_bytes()`), not off a counting
allocator: a `GlobalAlloc` implementation requires `unsafe`, which this packet forbids. The
allocator-based number for the same property belongs to `vt-bench rss` (`US-0072`, tier 5), which
will report it for the new engine once `Terminal` exists.

### Decisions taken where the design was ambiguous — **for the design owner**

1. **Row-id lanes.** "Allocated from one terminal-wide counter" and "each screen keeps its own
   contiguous run" cannot both hold: both screens allocate (the alternate scrolls while the primary
   is frozen, and `US-0077`'s `KeepViewportTop` corrects the primary *while* the alternate is
   active), so one counter necessarily leaves a gap in one run — and a gap breaks the
   `history_len = newest - oldest + 1 - rows` arithmetic. Implemented as **one `u64` id space split
   into two lanes**: `RowId::PRIMARY_ORIGIN = 0`, `RowId::ALT_ORIGIN = 1 << 63`. Every property the
   design asks for holds (one id space, ids unambiguous, each run contiguous, the runs disjoint,
   `assert_integrity` checks it).
2. **`ED 2` and the viewport offset.** The offset table says "`offset` unchanged", the prose in the
   same row says "the scrolled-back user keeps seeing the same content". Once the bottom moves down
   by `positions`, only the second is achievable — and it is what the reference does, since its
   `Grid::scroll_up` bumps `display_offset` here like any other scroll. Implemented the prose;
   `grid::tests::ed2_keeps_the_scrolled_back_viewport_position` pins the visible top row id.
3. **`scroll_up` case 2 ids.** "Rows below the region keep their ids **and** their content" cannot
   hold: the screen's bottom is `newest`, so once `newest` advances those screen positions *are*
   different ids. Implemented as the reference behaves — the rows below keep their **content and
   their place on the screen**, re-homed onto the new bottom ids, and anchors on them shift by `+n`.
4. **The cursor is not a content anchor.** The cursor, the saved cursor and the viewport top are
   registered in the list (reflow needs them), but `shift_region` **skips** them and each primitive
   writes the field back through `Anchors::set`. A content anchor follows its content; a cursor
   keeps its place on the *screen*. Making the cursor follow content would drag it up on every
   `SU`, which the reference does not do and which would break `IL`/`DL` (they use the cursor's row
   as the region origin). The field is the authority under the scroll primitives, the entry is the
   authority under reflow, and `anchor::tests::saved_cursor_survives_a_region_scroll` asserts the
   two agree.
5. **Wide-pair repair is compulsory on erase and shift.** The reference leaves orphaned spacers
   behind `EL` / `ECH` / `DCH` / `ICH`; the design's own integrity assertion ("no `Wide` without its
   `WideSpacer`") forbids that state, so `repair_wide_pairs` runs after every in-row mutation. The
   verifier measured the corpus exposure: **none of the 45 recordings contains a width-2 glyph**, so
   this produces no parity difference today, and the upper bound if the scan missed one is the 22
   recordings carrying both non-ASCII bytes and an in-row erase. Still worth a `C` row, because the
   state it forbids is reachable in principle.
6. **The wide repair is gated on the cell being overwritten.** Both the same-row and the cross-row
   repair only run when the cell under the cursor is itself half of a pair, exactly as the reference
   gates them. Without the gate, the `LeadingWideSpacer` a wrapping wide glyph has just placed is
   released by its own glyph on the next row.
7. **The cursor carries two cells, not one.** `template` is the reference's `cursor.template` (the
   SGR style, the open hyperlink or image, the OSC 133 semantic) and is what a *printed* glyph
   inherits. `erase` is the reference's `bg.into()` / `Cell::reset` — the default cell with only the
   template's background — and is what every erase and every row reset fills with. Both are derived
   once in `Screen::set_template`, which is the only writer, so the erase paths still never reach
   into the interner *and* an erase under an open underline, strikeout or `OSC 8` hyperlink leaves
   plain blanks rather than decorated ones. Consequence: `Row::reset`'s background-erase
   discriminant is the erase cell's interned **style id**, which now differs exactly when the
   background differs, because nothing else is left in that style.
8. **`RowsScrolled` reports content moving between row ids, not the viewport moving.** That is
   `damage-and-render-state.md`'s definition ("in-region motion, because that moves content without
   moving the viewport"), so `ScrollReport::scrolled` is an `Option`: a whole-screen scroll reports
   **nothing** (every row kept its id and its content; the viewport's own motion reaches the
   renderer as `RenderUpdate::Partial { scrolled }`), a bottom-bounded region reports the tail's
   `+n`, and an in-region scroll reports its `-n` / `+n`. R-02 — "anchors match the report" — now
   holds in every case, with a test for each.
9. **The LLD's `scroll_up` case-2 anchor column is wrong.** It says
   `shift_region(region, -n, kill = top n)`, which would kill anchors whose content is safely in
   scrollback and shift anchors whose content never moved. The correct bookkeeping is the opposite:
   nothing inside the region moves, and the **tail** below the region shifts by `+n`.
10. **`ED 2`: the offset table cell and research § 8's trap 9 are both wrong.** Both say the offset
    is unchanged. `Grid::clear_viewport` itself does not touch `display_offset`, but the
    `scroll_up(0..lines, positions)` it calls does (`grid/mod.rs:265-268`), so the reference bumps
    it — which is also the only way the row's own prose ("the scrolled-back user keeps seeing the
    same content") can be true. Measured here: offset 3 → 6 with the visible top row unchanged.
11. **The alternate screen shares the scroll region and the tab stops.** The reference keeps one
    `scroll_region` and one `tabs` table on `Term`, so entering the alternate screen resets neither,
    and the entry wipe is a background erase with the *entering* cursor's template, not a `RIS`.
    `swap_alt` copies both across in each direction, which is observationally the same as sharing.
    If per-screen region and tabs were the intended model, that is a deviation the LLD must declare
    along with what `? 1049 l` restores.
12. **One `AnchorKind::Cursor` per lane.** `DEC-0015` and the LLD describe `Cursor` as "the active
    screen's cursor", singular, but each screen registers its own three screen-owned entries, so the
    list holds two of each. A consumer reading the list cannot tell them apart without checking
    which lane the row id is in. Either the kind needs a screen discriminant or the pairing needs
    documenting; this packet documents it.

### Shape differences from the design text

- **Module layout.** The packet was scoped to `crates/vt/src/grid/`, so the concern lives in a
  folder (`mod.rs`, `row.rs`, `anchor.rs`, `screen.rs`, `terminal_grid.rs`) rather than the flat
  `grid.rs` + `anchor.rs` the HLD's R-49 describes. Test paths are `grid::tests::`,
  `grid::anchor::tests::` and `grid::props::`, so every `cargo test` filter the design names
  (`grid::`, `anchor::`) still selects the right tests.
- **`TerminalGrid`** is new and unnamed by any LLD: the grid half of the future `Terminal`, owning
  the two screens, the shared anchor list, the batch `SeqNo` and `lines_produced`. `US-0079`'s
  `Terminal` holds one.
- No separate internal `Viewport` struct: its three fields (`offset`, `rows`, `cols`) already exist
  on `Screen`, and `Screen::viewport()` returns the public `Viewport { top, rows, cols }`.
- `clear_viewport` / `erase_display` take `&Interner` and `row_text` takes `&GraphemeArena`, for
  the reason `Cell::is_erasable` does since `US-0074`: the rule reads a style or a cluster.
- `RowFlags::HAS_GRAPHIC` is stamped by the caller (`RowMut::mark_graphic`) and is **not** checked
  by `assert_integrity`: the grid cannot resolve an extras id without the interner. The other three
  content hints are checked for false negatives.

### Verification round (`evidence/US-0075-verify.md`)

An independent review of `864c81c` returned **merge after fixes**: two blockers, three majors, nine
minors. All five blockers and majors are fixed in the follow-up commit, each with the test the
report asked for:

| # | Defect | Fix | Test |
| --- | --- | --- | --- |
| B1 | A wide glyph wrapping over an existing pair wrote its `LeadingWideSpacer` with `RowMut::set`, orphaning the `Wide` at `cols - 2` — reachable in three operations, and exactly the state `assert_integrity` forbids | the spacer goes through `write_at_cursor`, which repairs first, as the reference routes it | `grid::tests::wide_char_wrapping_over_an_existing_pair_keeps_the_grid_intact` |
| B2 | `Anchors::trim` was lane-blind, so one line feed inside the alternate screen (scrollback 0, therefore trimming on every scroll with an `oldest` above every primary row id) destroyed **every** primary mark, selection end and graphics placement | `trim(origin, oldest)` is lane-scoped; `assert_integrity` now counts the three screen-owned anchors **per lane**, and the property test asserts that no operation on one screen kills an anchor on the other | `anchor::tests::an_alt_screen_scroll_leaves_primary_anchors_alone`, `grid::props::scroll_and_erase_preserve_integrity` |
| F3 | `RowsScrolled` was emitted for every scroll, including whole-screen ones where no row's content changed id — a `RowId`-keyed cache following it corrupted itself — and never reported the bounded region's tail shift | `ScrollReport::scrolled` is an `Option<RowsScrolled>`, `None` when no content changed id, `Some(+n)` over the tail for a bounded region | `anchor::tests::rows_scrolled_is_silent_for_a_whole_screen_scroll`, `..::rows_scrolled_reports_the_tail_shift_of_a_bounded_region`, `..::rows_scrolled_event_matches_the_anchor_shift` |
| F4 | Every erase filled with the whole SGR template, so erasing under an open underline or `OSC 8` hyperlink left decorated blanks, where the reference fills with `bg.into()` | the cursor carries a second, derived `erase` cell (reading 7 above); every erase and every row reset uses it | `grid::tests::erased_cells_keep_only_the_background` |
| F5 | `swap_alt` called `Screen::reset`, i.e. a `RIS`: it destroyed the alternate screen's scroll region and tab stops and cleared with the default background | region and tabs are copied across in both directions (the reference shares one of each), and the wipe is `clear_all_rows` with the entering cursor's erase cell | `grid::tests::entering_alt_screen_keeps_the_region_and_the_tab_stops`, `..::entering_alt_screen_clears_with_the_background_template` |

**One further defect, found by the strengthened property test itself.** Adding the verifier's
suggested `PrintAt` operation — a print at a chosen column — immediately produced
`[PrintAt('Z', col 5), Print(wide), Tab(1)]`: `put_tab` wrote its `'\t'` into any cell whose content
was a space, and **both spacer halves of a wide pair read as a space**, so the tab replaced one with
a narrow cell and orphaned its glyph. `put_tab` now changes only the content and keeps the width,
which is what the reference does (it assigns `cell.c` and nothing else).
Test: `grid::tests::tab_does_not_orphan_a_wide_pair`.

Minors fixed: **M7** (the written-row memory figure, now measured at the same ring as the empty one),
**M9** (the insert-mode shift now carries the reference's `col + width < cols` gate, so it no longer
dirties a row per glyph at the columns the reference skips), **M11** (the background caveat on the
memory claim, above), **M13** and **M14** (doc comments on `Screen::row` and `Screen::wrapline`).

Minors left, with reasons: **M6** (two `AnchorKind::Cursor` entries) is documented as design-owner
reading 12 rather than fixed, because adding a screen discriminant changes the canonical
`AnchorKind` list, which this packet may not edit. **M10** (`repair_wide_pairs` sweeps the whole row
where the reference repairs only the boundary) is correctness-neutral and O(cols) on operations that
are already O(cols); narrowing it would trade a clearly-correct invariant for a micro-optimisation
the intake forbids as an outcome. **M12** (`vt-paranoid`) still needs a `Cargo.toml` entry outside
this packet's file scope.

### Gaps

- **`assert_integrity` tiering.** `testing-and-bench.md` § 1 (R-28) and `events-and-api.md` both
  mandate the O(1) tier per mutating method with the full walk once per `feed` / `resize` /
  `render_update`, so that is what was built: `debug_assert_integrity()` (counters ordered, cursor
  inside the screen, offset inside history) runs at the end of every mutating public method in
  debug builds, and `Screen::assert_integrity()` / `TerminalGrid::assert_integrity(interner)` do
  the full walk for the tests, the property test and later the three entry points. The
  **`vt-paranoid` feature** the design names needs a `Cargo.toml` entry this packet's file scope
  forbids; the property test runs the full walk unconditionally instead, which is the same coverage.
- **`Screen::resize` is rows-only.** Columns truncate and pad instead of reflowing, and only
  `BottomAnchor` semantics exist. `US-0077` replaces the column half and adds `ResizePolicy`. What
  is already pinned here is the session-constant ring mask (R-30), the scroll-region reset (trap 28)
  and the tab-stop regrow rule (trap 27).
- **No `expected-diffs.json` for C1-C4.** The parity harness first runs against the new engine in
  `US-0076`; the exact cells cannot be measured before then. C1-C4 each have a unit test here, and
  item 5 above names a fifth candidate that the corrections table does not list — though the
  verifier measured its corpus exposure at zero.
- **No ASCII run fast path** on the print path. It is a throughput optimisation with no behavioural
  contract, and the intake forbids a throughput outcome for this packet; `print` takes one scalar
  and decides width with `scalar_width`, which is what the corpus pins.
- **Test-name divergence between the two design documents.** Where the trap map still uses the
  pre-correction name, the LLD's Verification list won, because it describes the behaviour that was
  built: `ed1_clears_row_zero` (trap map: `..._with_cursor_on_row_one_keeps_row_zero`),
  `small_region_scroll_rotates_then_blanks` (trap map: `..._blanks_without_rotating`),
  `delete_chars_shifts_left_by_n` (trap map: `..._clamps_end_to_last_column`).
- **Three tests prove less than their names promise**, because the behaviour is not observable
  through the API:
  - `decawm_off_then_tab_moves_to_the_next_stop` — the pending-wrap flag can only be armed at the
    last column, where "the next stop" *is* the last column. The test asserts what does differ: the
    cursor never leaves its row, where the reference consumes a stuck wrap and moves to the next.
  - `row_reset_respects_occ_and_the_background_template` — "cells above `occ` are not touched"
    cannot be observed, since every write bumps `occ`. The test asserts the `occ` transitions and
    the background-erase fill, which is trap 36's user-visible half.
  - `lines_produced_is_unchanged_by_reflow_and_by_clear` covers the clear, the alternate-screen
    swap, the resize and `RIS`; the reflow half lands with reflow in `US-0077`.
- **No integration, E2E or platform proof.** Nothing in the workspace depends on `oneterm-vt` yet:
  the parity gate is `US-0076`, and the first time the engine is behind the application is
  `US-0081`.

## Handoff

Branch `worktree-agent-a8c57df6ea2ef0000` off `feat/vt-engine` @3b60094, one commit, **not merged
and not pushed**. `crates/vt/src/parser/` is being implemented concurrently on another branch and
was not touched; `src/lib.rs` gained only the `pub mod grid;` line and its re-export.

Next: `US-0076` (dispatch and modes) and `US-0077` (reflow) both build directly on this. `US-0077`
should read items 1, 3 and 4 above before touching `Anchors`, and `US-0076` should read item 5
before running the parity gate.
