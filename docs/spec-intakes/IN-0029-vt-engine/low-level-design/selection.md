# Low-Level Design: Selection

Intake: IN-0029
HLD: ../high-level-design.md
Topic: selection
Date: 2026-09-12

> One concern per file. Implementation-level mechanics for `crates/vt/src/selection.rs`.

## Concern

The four selection kinds, how a drag turns into a range, how the range survives scrolling,
trimming, erasing and reflow, and how the selected text is materialised. Added because the
public API in the High-Level Design exposes
`selection_start` / `selection_update` / `selection_range` / `selection_text` / `selection_clear`
/ `select_all` and nothing designed them; the adapter swap cannot compile without this file.

Reference behaviour: [`../research/engine-semantics.md`](../research/engine-semantics.md) § 2.18
(`vendor/alacritty_terminal/src/selection.rs`) and the OneTerm call sites
`crates/terminal/src/model.rs:248-300`, `crates/terminal-view/src/input/mouse.rs:300-346`.

## Design

### Anchors are tracked anchors

```rust
pub struct Selection {
    kind: SelectionKind,
    start: AnchorId,          // AnchorKind::SelectionStart
    end: AnchorId,            // AnchorKind::SelectionEnd
    start_side: Side,
    end_side: Side,
}
pub enum SelectionKind { Simple, Block, Semantic, Lines }
pub enum Side { Left, Right }
```

Both ends are registered in the engine's tracked-anchor list, whose canonical `AnchorKind` list
lives in [`grid-and-scrollback.md`](grid-and-scrollback.md) § "Tracked anchors"; it is the single
mechanism every row-moving primitive and reflow already update. That replaces the reference's
`Selection::rotate(dimensions, range, delta)` called from four separate places, and it means a
selection made inside a `tmux` pane follows its content through an `IL`/`DL` repaint, which the
reference's viewport-relative rotation does not do.

Consequences, each with a test:

- A scroll that pushes rows into history moves both anchors with their content until the content
  is trimmed; when an anchor's row is trimmed, that anchor dies and the **selection is cleared**
  (the reference returns `None` from `to_range` once the end is above the topmost line; clearing
  is the same outcome, stated once).
- A region scroll that blanks rows without rotating kills any anchor inside it, so the selection
  is cleared rather than pointing at unrelated text.
- Reflow remaps both anchors through `Anchors::remap`; a selection therefore survives a resize
  **when the column count is unchanged**, and is cleared when it changes — matching trap 28 and
  what `crates/terminal/src/model.rs:508` does today.

### Fractional hit testing

The view passes fractional row and column (`mouse.rs:346`), and the engine converts:

```rust
pub fn hit_test(&self, viewport_row: f32, col: f32) -> (Pos, Side);
// row -> the RowId at that viewport index, clamped to the live range
// col -> floor(col), clamped to cols - 1
// side -> Left when col.fract() < 0.5 else Right
```

This is `crates/terminal/src/model.rs:425-437` moved into the engine, so the view stops doing
`line + display_offset` arithmetic.

### `to_range` semantics

`selection_range()` is O(1) and must not materialise text (the property `has_selection()` relies
on today, PERF-14). It orders the two anchors, returns `None` when either is dead, and dispatches
by kind:

| Kind | Rule |
| --- | --- |
| `Simple` | Drop the last cell when the end side is `Left` (wrapping to the previous row's last column when the end column is 0), and drop the first cell when the start side is `Right` (wrapping forward). Empty when the anchors coincide, or when they are adjacent with `Right` then `Left` |
| `Block` | Normalise to top-left / bottom-right by swapping **columns and sides only**, never rows; `is_block = true`. Empty by the same column-only rule. **Correction:** the range is never inverted or out of bounds. An exhaustive oracle over all 576 (position, side) pairs on a 4-column grid found the reference emitting an inverted rectangle in **24 of 576** cases — every one a same-column, opposite-side pair on different rows, two of them also past the last column (`start.col 4` on a four-column grid). Those drags cover zero columns, so OneTerm returns `None`. The same guard on `range_simple` is provably unreachable (0/576 divergences) and is kept so the property holds by construction |
| `Semantic` | If start and end coincide, try `bracket_search` first (matching `()[]{}<>` across rows); otherwise expand left and right with `semantic_escape_chars`. **Never empty** |
| `Lines` | Expand to whole logical lines: walk left and right across `RowFlags::WRAPPED` continuations, then snap to column 0 and the last column. **Never empty** |

`SelectionRange { start: Pos, end: Pos, is_block: bool }`, both ends **inclusive**, which is what
`crates/terminal-view/src/render/frame.rs:549-559` expects.

`contains_cell` keeps the two reference quirks: a `Block`-shaped cursor cell sitting exactly on a
selection corner is not inverted, and a `Wide` cell's membership extends to its `WideSpacer`.

### Semantic expansion

`Config::semantic_escape_chars`, default `",│`|:\"' ()[]{}<>\t"` — the reference's default,
preserved because the word-selection behaviour is user-visible and OneTerm never overrode it.
Expansion walks cells, skipping `WideSpacer` and `LeadingWideSpacer`, and **stops at a row
boundary whose row lacks `RowFlags::WRAPPED`**, so a word never runs across two unrelated lines.

### `selection_text`

One function, shared by copy, the clipboard policy path and `selection_text()`:

- Wide spacers (`WideSpacer`, `LeadingWideSpacer`) are skipped.
- A grapheme cell emits its whole cluster.
- A tab cell emits `\t`; runs of trailing blanks before a line break are trimmed the way the
  reference trims them (a tab run collapses to the next tab stop).
- A `\n` is appended when the selection reaches the last column of a row that is **not**
  `WRAPPED`; a wrapped continuation joins without a newline. This is what makes copying a wrapped
  command line paste as one line.
- `Block` extracts a rectangle: for each row in range, the columns between the normalised left
  and right, each row terminated by `\n`.

### Invalidation matrix

Stated once, so `grid-and-scrollback.md` can refer to it instead of restating rules:

Two rules that look like gaps and are not:

- **A partial region scroll can split a selection**, leaving the two endpoints describing text that
  was never contiguous. That is **parity**: the reference's `Selection::rotate` moves both anchors
  independently by the same delta and has the same effect. Accepted behaviour, not a gap.
- **An endpoint scrolled out of a region top kills the selection (correction C15).** The reference
  clamps that endpoint to `(range_top, column 0, Left)` for non-`Block` kinds
  (`vendor/alacritty_terminal/src/selection.rs:160-166`, pinned by its own `rotate_in_region_up`
  test), keeping the selection alive over content the user never selected. **Kill is the confirmed
  rule here**: the anchored content was genuinely discarded by the scroll, and clamping is exactly
  what this file's own principle forbids — "cleared rather than pointing at unrelated text". It is
  a user-visible difference from the reference, so it is declared, not silent.

| Operation | Effect on the selection |
| --- | --- |
| `EL 0` / `EL 1` / `EL 2` | cleared when the range intersects the cursor row |
| `ED 0` | cleared when the range intersects the cleared rows |
| `ED 1` | cleared when the range intersects `screen_top..=cursor`. **Bounded at the screen top**: `ED 1` never touches scrollback, so a selection lying entirely in history survives it |
| `ED 2` | cleared |
| `ED 3` | cleared when the range intersects history |
| `SU` / `SD` / `IL` / `DL` / `RI` | anchors move with their content; cleared if either anchor lands in a blanked range, **including an endpoint scrolled out of a region top** (C15) |
| Scroll into history | anchors move; cleared when an anchor is trimmed |
| `swap_alt` | cleared |
| `RIS` | cleared |
| Resize, columns unchanged | anchors remapped; selection kept |
| Resize, columns changed | cleared (trap 28) |
| `scroll_viewport` | unaffected — the selection is in row space, not viewport space |

That last row is a behaviour improvement the reference cannot express: today a selection is
viewport-relative and must be rotated on every scroll.

### `select_all`

Registers the two anchors at `(oldest, 0)` and `(newest, cols - 1)` with kind `Simple` and sides
forced outward, covering history and viewport.

## Interfaces

**Shape, as shipped.** The module is written against **`TerminalGrid`**, not `Terminal`, because
`Terminal` does not exist until `US-0079`. The functions below are free functions or methods on the
selection value taking `&TerminalGrid`; the seven `impl Terminal` wrappers are **one line each and
owned by `US-0076`**, which is also where the escape set moves from a `&str` parameter (default
published as `SEMANTIC_ESCAPE_CHARS`) onto `Config::semantic_escape_chars`.

`US-0076` inherits one obligation beyond the seven wrappers: the invalidation matrix is a
**predicate**, `Selection::invalidated_by(grid, Invalidation::…)`, not a set of call sites inside
the grid. Its dispatch of `erase_line`, `erase_display`, `reset` and `swap_alt` must evaluate the
predicate **before** performing the operation, because the matrix is stated against the pre-operation
grid. A dispatch that forgets it leaves a selection pointing at erased cells.

Two further shape notes: `WRAPPED` is read as the **row** flag (deviation G1), not as a flag on the
last cell — equivalent, including the reference's `line_length` short-circuit; and `contains_cell`
takes the block-cursor position as `Option<Pos>` rather than a `CursorShape`, because `CursorShape`
is `US-0076`'s type. The quirk it guards is unchanged.

```rust
// crates/vt/src/selection/ — against &TerminalGrid; the wrappers below land in US-0076
impl Terminal {
    pub fn selection_start(&mut self, pos: Pos, side: Side, kind: SelectionKind);
    pub fn selection_update(&mut self, pos: Pos, side: Side);
    pub fn selection_range(&self) -> Option<SelectionRange>;    // O(1)
    pub fn selection_text(&self) -> Option<String>;
    pub fn selection_clear(&mut self);
    pub fn select_all(&mut self);
    pub fn hit_test(&self, viewport_row: f32, col: f32) -> (Pos, Side);
}
pub struct SelectionRange { pub start: Pos, pub end: Pos, pub is_block: bool }
```

`SelectionRange` is also what `render_update` copies into the render state each frame, so the
painter never asks the engine a second question
([`damage-and-render-state.md`](damage-and-render-state.md)).

## Edge Cases and Failure Modes

- [ ] **An anchor is trimmed out of history** — the selection clears; `selection_range()` returns
  `None` rather than a range with one valid end.
- [ ] **A `Semantic` selection whose word is cut by a trim** — expansion runs against the live
  rows, so it simply produces a shorter word.
- [ ] **Zero-width selection** — `Simple` and `Block` report empty; `Semantic` and `Lines` never
  do, matching the reference.
- [ ] **A selection over an image** — cells carrying only a `GraphicId` are spaces, so the copied
  text has spaces; this matches today's behaviour.
- [ ] **A selection during a reflow that changes the column count** — cleared, and the anchors are
  released so the tracked-anchor list does not leak entries.
- [ ] **Anchor leak** — `selection_clear`, `select_all` and every clearing rule above release the
  previous anchors; a debug assertion checks the anchor list holds at most two selection entries.
- [ ] **`hit_test` beyond the viewport** clamps rather than returning an id outside the live range.

## Verification

`cargo test -p oneterm-vt selection::`

- [ ] `selection::tests::simple_side_rules_drop_the_right_cells`
- [ ] `selection::tests::simple_is_empty_when_anchors_coincide_or_are_adjacent`
- [ ] `selection::tests::block_normalises_columns_not_rows`
- [ ] `selection::tests::block_range_is_never_inverted_or_out_of_bounds` — the 24-of-576 correction,
  driven by the exhaustive oracle.
- [ ] `selection::tests::block_text_extracts_a_rectangle`
- [ ] `selection::tests::semantic_expansion_uses_the_escape_chars`
- [ ] `selection::tests::semantic_stops_at_an_unwrapped_row_boundary`
- [ ] `selection::tests::semantic_bracket_match_when_the_anchors_coincide`
- [ ] `selection::tests::lines_expands_across_wrapped_continuations`
- [ ] `selection::tests::text_joins_wrapped_rows_without_a_newline`
- [ ] `selection::tests::text_skips_wide_spacers_and_emits_whole_graphemes`
- [ ] `selection::tests::text_emits_tab_cells_as_tabs`
- [ ] `selection::tests::contains_cell_extends_a_wide_char_to_its_spacer`
- [ ] `selection::tests::range_is_o1_and_does_not_allocate` — a counting allocator, the PERF-14
  property `has_selection()` depends on.
- [ ] `selection::tests::anchors_follow_a_region_scroll` — the improvement over the reference.
- [ ] `selection::tests::selection_clears_when_an_anchor_is_trimmed`
- [ ] `selection::tests::endpoint_scrolled_out_of_a_region_top_kills_the_selection` — C15.
- [ ] `selection::tests::ed1_does_not_clear_a_selection_wholly_in_history` — the screen-top bound.
- [ ] `selection::tests::a_partial_region_scroll_splits_the_selection` — parity, pinned so it is not
  later "fixed".
- [ ] `selection::tests::selection_survives_a_row_only_resize_and_clears_on_a_column_resize` — trap 28.
- [ ] `selection::tests::viewport_scrolling_does_not_change_the_range`
- [ ] `selection::tests::invalidation_matrix` — one table-driven test per row of the matrix above.
- [ ] `selection::tests::hit_test_side_and_clamping`
- [ ] `selection::tests::anchors_are_released_on_clear`

Cross-crate, at the adapter packet: the eleven search tests
(`crates/terminal/src/search.rs:226-344`) and the mouse tests
(`crates/terminal-view/src/input/mouse_tests.rs`) keep their coverage; the click-count to
`SelectionKind` mapping stays in the view.
