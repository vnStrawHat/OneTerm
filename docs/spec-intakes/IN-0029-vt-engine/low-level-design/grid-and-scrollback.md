# Low-Level Design: Grid and scrollback

Intake: IN-0029
HLD: ../high-level-design.md
Topic: grid-and-scrollback
Date: 2026-09-12

> One concern per file. Implementation-level mechanics for `crates/vt/src/grid/`
> (`mod.rs`, `row.rs`, `anchor.rs`, `screen.rs`, `terminal_grid.rs` — a folder, because the concern
> genuinely splits). `TerminalGrid` is the grid half of the future `Terminal`: it owns the two
> screens, the shared anchor list, the batch `SeqNo` and `lines_produced`, and `US-0079`'s
> `Terminal` holds one.

## Concern

Row storage and identity, the viewport anchoring model, the tracked-anchor list, scroll regions,
the alternate screen, the print path's cursor and wrap bookkeeping, and the erase / insert /
delete operations. Replaces `vendor/alacritty_terminal/src/grid/{mod,storage,row}.rs` and the
screen half of `term/mod.rs`.

Reflow and the column half of resize are a separate concern
([`reflow-and-resize.md`](reflow-and-resize.md), `US-0077`); this file owns only the rows-only
`Screen::resize_rows`, which truncates and pads; selection is
[`selection.md`](selection.md); damage stamping is
[`damage-and-render-state.md`](damage-and-render-state.md); which escape sequence calls which
operation is [`dispatch-and-modes.md`](dispatch-and-modes.md).

## Design

### Row identity: what `RowId` names

```rust
pub struct RowId(pub u64);   // a POSITION in the output stream, not a piece of content
```

**Two lanes, one id space.** Each screen allocates from its own origin —
`PRIMARY_ORIGIN = 0` and `ALT_ORIGIN = 1 << 63` — and every id is unambiguous because the lanes
cannot meet: `TerminalGrid::screen_of(id)` routes on the high bit, and `assert_integrity` checks
each screen's run stays inside its lane. A single shared counter cannot work: both screens allocate
independently, so a shared counter leaves gaps in each screen's run and
`history_len = newest - oldest + 1 - rows` stops holding. `slot(id) = id & mask` is unaffected,
because `ALT_ORIGIN & mask == 0` and the two screens own separate rings.

Everything that compares a `RowId` against a bound must therefore be **lane-scoped**. In
particular `Anchors::trim(origin, oldest)` kills only anchors in `origin..oldest`: the alternate
screen has `scrollback_limit = 0`, so it trims on every scroll, and a lane-blind trim would kill
every primary-screen mark, selection anchor and graphics placement on the first line feed inside
vim or tmux.

`slot(id) = (id.0 as usize) & mask` — the ring index **is** the id. That has a direct
consequence, and `DEC-0015` now states it:

| Operation | Ids | Content |
| --- | --- | --- |
| Output pushing rows into scrollback | new ids allocated at the bottom | each id keeps its content |
| Viewport scrolling | unchanged | unchanged |
| `RIS`, `ED 2`, `ED 3` | unchanged (ids are never reset) | cleared or moved into history |
| `SU` / `SD` / `IL` / `DL` / `RI` inside a region, any region that is not the whole viewport | **unchanged** | **content is copied between fixed ids** |
| Reflow | every row gets a fresh id | content redistributed |

So a consumer must never store a `RowId` and assume the content is still there after a TUI
repaints. Anchoring is an engine service instead.

### Tracked anchors — one mechanism, shared with reflow

**This is the canonical definition of `AnchorKind` and `Anchors`.** `DEC-0015`,
[`reflow-and-resize.md`](reflow-and-resize.md), [`selection.md`](selection.md) and
[`graphics.md`](graphics.md) reference it and never restate the variant list.

```rust
// crates/vt/src/anchor.rs
pub struct AnchorId(u32);

pub enum AnchorKind {
    Cursor,                  // the active screen's cursor
    SavedCursor,             // DECSC
    ViewportTop,             // the first visible row, so reflow can restate the viewport
    SelectionStart,
    SelectionEnd,
    Graphic(GraphicId),      // one per placement
    Mark(u32),               // OSC 133 prompt marks
}

pub struct Anchor { pub kind: AnchorKind, pub pos: Pos, pub alive: bool }

pub struct Anchors { entries: Vec<Anchor>, free: Vec<AnchorId> }
impl Anchors {
    pub fn register(&mut self, kind: AnchorKind, pos: Pos) -> AnchorId;
    pub fn get(&self, id: AnchorId) -> Option<Pos>;
    pub fn set(&mut self, id: AnchorId, pos: Pos);
    pub fn release(&mut self, id: AnchorId);
    // called by every row-moving primitive; `rows` and `kill` are RowId ranges, and the
    // caller converts its viewport-coordinate ScrollRegion with `Screen::rows_of(region)`
    // (N-13: one coordinate space inside the anchor list, converted at the boundary):
    pub(crate) fn shift_region(&mut self, rows: Range<RowId>, delta: i32, kill: Range<RowId>);
    pub(crate) fn remap(&mut self, f: impl Fn(Pos) -> Option<Pos>);   // reflow
    pub(crate) fn trim(&mut self, origin: RowId, oldest: RowId);   // lane-scoped: origin..oldest
}
```

**`Cursor`, `SavedCursor` and `ViewportTop` are registered per screen**, so the list holds one of
each per lane rather than one in total. That is not a duplicate: it is what gives the primary and
the alternate screen independent `DECSC` slots and independent scroll offsets, which is the
behaviour `? 1049` requires. A consumer reads them through the screen, never by scanning the list
for a kind — because today `AnchorKind::Cursor` alone cannot say *which* screen's cursor it is.
**Adding a screen discriminant to `AnchorKind` is owned by `US-0077`**, which is the first packet
that walks the list generically: reflow runs on the primary screen only, so `Anchors::remap` must
select that screen's entries rather than every `Cursor` in the list. Until then the pairing is the
documented contract, and `assert_integrity` counts the three screen-owned entries **per lane**.

**The field is the authority across a scroll; the anchor entry is the authority across a reflow.**
`Screen::cursor.pos`, `Screen::saved_cursor.pos` and `Viewport::offset` stay ordinary fields,
because the print path writes the cursor on every glyph and must not pay a lookup, and because the
region primitives deliberately do **not** move them: `IL` and `DL` read the cursor's screen row as
their origin, which a content shift would destroy, and the reference's `saved_cursor` is
screen-relative so no scroll touches it either. `Anchors::shift_region` therefore **skips** those
three kinds and moves only `SelectionStart`, `SelectionEnd`, `Graphic` and `Mark`;
`Anchors::remap` (reflow) moves all of them, which is what
[`reflow-and-resize.md`](reflow-and-resize.md) needs. After a reflow the moved entries are read
back into the fields **before** the next `sync_anchors`, which otherwise overwrites them from the
fields. A debug assertion checks field and entry agree at the end of `feed`, `resize` and
`render_update`.

- The list is small and bounded: one saved cursor, two selection anchors, one per live graphics
  placement, one per visible OSC 133 mark. A linear scan per scroll is cheaper than any index.
- **Every** primitive below that moves content calls `shift_region`; an anchor whose row lands in
  `kill` is marked dead. Reflow calls `remap` with the same list
  ([`reflow-and-resize.md`](reflow-and-resize.md)), so there is one mechanism, exercised by the
  reflow property tests as well as by the scroll tests.
- The engine additionally emits `VtEvent::RowsScrolled { top, bottom, delta }` so a consumer with
  its own cache (the renderer's row plan, a search index) can shift it instead of rebuilding.
  The event is a **notification**; it is not how anchors move. Its contract, which
  [`damage-and-render-state.md`](damage-and-render-state.md) § "Scroll damage" states in the same
  words:

  `ScrollReport::scrolled` is an `Option<RowsScrolled>` for exactly this reason: `None` means no
  row's content changed id.

  | Case | Reported |
  | --- | --- |
  | Whole-viewport scroll | **`None`**. Every surviving id keeps its content and the new rows are new ids, so a `RowId`-keyed cache is already correct; a `delta` here would make it corrupt itself. The viewport's own motion reaches the renderer as `RenderUpdate::Partial { scrolled }` instead |
  | Region anchored at row 0 with a bounded bottom | one event over the **region's** id range with `delta = -n`, and a **second** event over the tail below the region with `delta = +n`, because those rows kept their content but changed id |
  | Region not anchored at row 0 | one event over the region's id range, `delta = -n` for `SU` / `IL`, `+n` for `SD` / `DL` |
  | Invalid or empty region | **nothing**, and never a malformed range with `bottom < top` |
- Debug assertion: every live anchor's row is inside `oldest..=newest`.

### Viewport anchoring

The one invariant everything else is stated against:

```rust
struct Viewport { offset: u32, rows: u16, cols: u16 }   // offset = distance from the bottom
```

- `offset == 0` is **sticky bottom**: the view follows output. This is the reference's
  `display_offset == 0`.
- `offset > 0` holds the viewed content still. When rows are pushed into scrollback the offset
  **grows by the same amount**, capped at `history_len()`, so the user keeps looking at the same
  content. This is exactly the reference's `Grid::scroll_up`: "if `display_offset != 0` then
  `display_offset = min(offset + n, max_scroll_limit)`".
- The public API exposes `Viewport { top: RowId, rows, cols }` with
  `top = newest - offset - (rows - 1)`; the offset is the internal representation, because it is
  the one that makes "sticky" expressible.
- `offset` is clamped to `history_len()` after any operation that shortens history.

Effect of every operation that moves rows, stated once:

| Operation | `offset` |
| --- | --- |
| Output / `LF` / wrap pushing `n` rows into scrollback | `0` stays `0` (sticky); otherwise `min(offset + n, history_len())` |
| `SU` / `SD` / `IL` / `DL` / `RI` inside a region that is **not** anchored at row 0 | unchanged (nothing enters history) |
| `scroll_viewport(delta)` | `clamp(offset - delta, 0, history_len())`; negative delta scrolls towards history |
| `scroll_to_bottom()` | `0` |
| `ED 2` on the primary screen (`clear_viewport`) | it scrolls the occupied rows into history, so the sticky rule applies like any other push: `0` stays `0`, otherwise `min(offset + positions, history_len())`. The scrolled-back user keeps seeing the same content because the offset grows with the bottom — trap 9 |
| `ED 3` (`clear_history`) | `0` — trap 10 |
| Trim (history full) | `min(offset, history_len())` |
| Resize, rows only | preserved, clamped |
| Resize with reflow | the viewport top is a tracked anchor and is remapped; see [`reflow-and-resize.md`](reflow-and-resize.md) — trap 30 |
| `RIS` | `0` |
| Alternate-screen swap | each screen keeps its own offset; the alternate's is always `0` (no history) |

Trap 35 dissolves under this model: a change to a row that is currently off-screen stamps that
row's sequence number, and `render_update` simply does not copy it because it is not in the
viewport. There are no shifted damage indices to get wrong.

### Storage: a power-of-two ring of lazily allocated rows

```rust
pub struct Screen {
    slots: Box<[Option<Row>]>,   // len = ring_len(), fixed for the session
    mask: usize,
    newest: RowId,
    oldest: RowId,
    rows: u16, cols: u16,
    scrollback_limit: u32,
    viewport: Viewport,
    region: ScrollRegion,        // { top: u16, bottom: u16 } in viewport coordinates
    cursor: Cursor,
    saved_cursor: Cursor,
    tabs: TabStops,
}
```

**Ring sizing (R-30).** The ring length is fixed for the session:
`ring_len() = next_power_of_two(scrollback_limit + MAX_ROWS)` with `MAX_ROWS = 1024` and
`MAX_COLS = 2048` (hard caps on the viewport, enforced at `resize`; N-11). It therefore does **not** change when the viewport
is resized, and `mask` is a session constant. Changing `scrollback_limit` at run time (the user
editing `terminal.json`) is the only rehome: it reallocates the ring and copies live rows to
their new slots, is O(live rows), and is explicitly not on the resize path. `Terminal::new`
computes it once.

- **`Option<Row>`, `None` until first written.** Reading a `None` slot yields blanks; writing one
  allocates the row's `Vec<Cell>`. **The slot itself is not a pointer (N-10):** `Row` is
  `RowHeader` plus an inline `Vec<Cell>`, so a slot is about 48 B, and the whole `Box<[Option<Row>]>`
  is allocated in `Terminal::new`. At the default 10 000-row scrollback that is 16 384 slots, under
  1 MB; at `SCROLLBACK_MAX = 1_000_000` it is about 50 MB of empty slots, which is the price of a
  session-constant mask and is stated in the HLD memory table. The win over the reference is that
  the *cells* are not materialised: 100 000 unwritten rows cost 4.8 MB of slots instead of
  100 000 x cols x 8 B of cells.
- Trimming emits `VtEvent::RowsTrimmed { oldest }` and calls `Anchors::trim`.
- There is no `zero` rotation, no free list and no storage-layout equality, so the reference's
  "comparing two grids without `rezero()` panics" hazard (trap 45) cannot exist here.

### Row

```rust
pub struct Row { header: RowHeader, cells: Vec<Cell> }      // one representation, always

pub struct RowHeader { seq: SeqNo, id: RowId, flags: RowFlags, occ: u16 }
bitflags! { pub struct RowFlags: u8 {
    const WRAPPED, DIRTY, STYLED, HAS_GRAPHEME, HAS_EXTRAS, HAS_GRAPHIC;
} }
```

**One row representation (R-51).** The dual-form (uniform runs plus general vector) design is
**deferred**. Every operation that matters — `ICH`, `DCH`, `ECH`, a `CUP` into the middle, a
wide-pair repair, insert mode, graphic stamping — coerces to the general form anyway, so the
uniform form survives only for append-only ASCII rows, where the win is memory and the memory
win the HLD actually claims comes from `Option<Row>` lazy allocation, which is independent. The
intake forbids a throughput outcome, so there is no measurement asking for it. A later packet may
add it, gated on the tier-5 RSS numbers `US-0072` produces; the `RowRef` / `RowMut` API below is
written so that change is internal.

- `RowFlags` follows Ghostty's "false positives allowed, never false negatives" rule: `STYLED`,
  `HAS_GRAPHEME`, `HAS_EXTRAS` and `HAS_GRAPHIC` are set on write and cleared only on reset, so
  the grapheme sweep, the graphics release scan and the render pass can skip a row with one load.
- `occ` is the reference's **over-approximating hint**: "no column above `occ` has been touched
  since the last reset". It is not exact, it is not part of any equality, and it is **not**
  checked by `assert_integrity` (R-10). `Row::reset(template)` clears `0..occ` when the last
  cell's background matches the template's, and the whole row otherwise (trap 36); when the
  template is the default style that path is a `memset` of `Cell::EMPTY`, and when it is not it
  is a fill with the template cell.

### Cursor and the pending-wrap flag

```rust
pub struct Cursor {
    pub pos: Pos,                 // { row: RowId, col: u16 }
    pub pending_wrap: bool,
    pub style: Style,             // the SGR template
    pub charsets: [Charset; 4],   // G0..G3 designations (saved and restored by DECSC/DECRC)
}
```

`active_charset` (which of G0..G3 `SI`/`SO` selects) lives on the **terminal**, not the cursor,
so `DECSC`/`DECRC` do not save it and the alternate screen does not swap it — reference
behaviour.

The saved cursor keeps a screen-relative row, like the reference's, so a region scroll does not
move it and `DECRC` after `SU` lands on the same screen row the reference lands on. Its anchor
entry exists for reflow, which does move it.

`pending_wrap` is set when a glyph lands on the last column and cleared by every explicit
positioning operation: `goto`, `move_forward`, `move_backward`, `carriage_return`, `backspace`,
`wrapline`. It is **not** cleared by `linefeed` or `reverse_index`.

**Deviation G3, with its real scope (R-09).** With `DECAWM` off the reference never clears the
flag, so it stays set forever. Here, with `DECAWM` off the flag is simply not set and the column
clamps at the last column. The printing result is identical, but the flag is read by two other
paths, so this is **observable**: after `DECAWM` off plus a glyph in the last column, `EL 0`
erases (the reference erases nothing, trap 2) and `HT` moves to the next tab stop (the reference
consumes the wrap, trap 3). Both combinations have a test. The deviation is kept because "the
flag is stuck forever" is not a state worth reproducing, and it lands in `US-0086`, after the
parity gate (R-53).

### Print path

`dispatch::print_str(s)` per run:

1. **Fast path** when nothing unusual is enabled — no insert mode, no charset translation, no
   open hyperlink, no `HAS_GRAPHIC` on the row, and the whole run is ASCII `0x20..=0x7E`: write
   cells with the current `style_id` until the last column, advance, stamp the row once. This is
   foot's `ascii_printer_fast`, selected by one `u8` of "is anything unusual enabled" recomputed
   only when a mode or the template changes.
2. **General path** per character (or per grapheme cluster): honour `pending_wrap`, insert mode,
   width, wide-pair repair, charset mapping, then `write_at_cursor`.
3. Either path ends with `col + 1 < cols ? col += 1 : pending_wrap = true`.

`wrapline()`: set `RowFlags::WRAPPED` on the current row; if the cursor is on the last row of the
scroll region, `scroll_up(1)`, otherwise move down one row; column 0; clear `pending_wrap`.

### Line counting

Two counters, because they answer different questions (R-05):

| Counter | Meaning | Used by |
| --- | --- | --- |
| `newest` (`RowId`) | rows created, including every wrapped continuation and every row reflow creates | the ring; never exposed as a line number |
| `lines_produced: u64` | **output lines**: incremented by `linefeed`, `NEL`, `IND` and a region scroll that pushes a row into history; **not** by an implicit wrap and **not** by reflow | `Terminal::lines_produced()`, the gutter |

`lines_produced` reproduces what `crates/terminal/src/backend/line_accounting.rs:16-48` computes
today, without the newline rescan, and keeps the gutter's numbers unchanged across a `clear`, an
alternate-screen swap and a resize. Using `newest + 1` instead would have changed the gutter
visibly on any wrapped output — a user-visible change that no deviation table listed.

### Scroll region (DECSTBM)

`ScrollRegion { top, bottom }` in viewport coordinates, default `0..rows`.

- `set_region(top, bottom)`: `bottom` defaults to `rows`; `top >= bottom` is a **no-op that
  leaves the previous region intact** (trap 15); both clamp to `rows`; every valid call ends with
  `goto(0, 0)`, which is origin-relative, so `DECSTBM` always homes the cursor.
- A resize unconditionally resets the region to the full screen (trap 28).

`scroll_up(region, n)` — all four cases (R-03):

| Case | Behaviour | Ids | `offset` | Anchors |
| --- | --- | --- | --- | --- |
| `region == 0..rows` (whole viewport) | `newest += n`; `n` fresh rows at the bottom; rows leave the top **into scrollback**; trim if over the limit. The only path that fills history (trap 17) | new ids at the bottom, existing ids keep their content | sticky rule above | `trim` only |
| `region.top == 0`, `region.bottom < rows` | the region's rows move up into scrollback as above, then the rows **below** `region.bottom` are lifted out and put back at the same screen positions — the reference's lift, rotate, put-back (`grid/mod.rs:285-292`) | rows below the region keep their **content and their screen position**, but not their ids: `newest` advanced, so every id below the region shifts by `n`. The LLD previously said "keep their ids and their content", which cannot both hold | as above | `shift_region` over the **region's** id range only, `kill` = the `n` rows that left the top. Anchors below the region are **not** killed (their content is safe) and are **not** shifted by the region call; the id shift is the ordinary push, handled by the same `+n` that moves the rest of the screen |
| `region.top != 0`, `region.height() > n` | content moves up **inside** the region only; the bottom `n` rows of the region are reset. Nothing enters history | ids fixed, content copied | unchanged | `shift_region(region, -n, kill = top n)` |
| `region.top != 0`, `region.height() <= n` | **Spec-correct (C3)**: the region rotates by its own height and is then blank, reached by the same path as every other count. The reference short-circuits and blanks with no rotation at all | ids fixed | unchanged | every anchor in the region dies |

`scroll_down(region, n)` (`SD`, `CSI T`, and `RI` at the region top) never pulls rows back out of
scrollback; it always blanks the top `n` rows of the region (trap 18), and calls
`shift_region(region, +n, kill = bottom n)`.

`insert_lines` (`IL`) and `delete_lines` (`DL`) are **complete no-ops when the cursor is outside
the region** (trap 16), use `cursor.row` as the origin rather than `region.top`, and go through
the same `shift_region` call.

Every one of these emits `VtEvent::RowsScrolled { top, bottom, delta }`.

### Alternate screen

Two `Screen`s drawing ids from the one counter. The alternate has `scrollback_limit = 0` and
never reflows. `swap_alt()`:

```
if entering:
    alt.cursor        = primary.cursor with pos.row replaced by alt's row at the same INDEX
    primary.saved_cursor = primary.cursor      // clobbers the primary DECSC slot (trap 14)
    alt.reset_rows_with(primary.cursor.erase_cell)   // BCE clear, NOT a reset
swap(keyboard_mode_stack, inactive_keyboard_mode_stack)
swap(primary, alt); mode ^= ALT_SCREEN
selection = None
full damage
```

**Entering the alternate screen is not `RIS` on that screen.** Three things must not happen, and
each was observed going wrong when the entry path called a screen-level reset:

| Shared or preserved | Why |
| --- | --- |
| The **scroll region** | the reference keeps one `scroll_region` on `Term`, shared by both screens, so a `DECSTBM` set before `? 1049 h` still applies inside the alternate screen and survives the return |
| The **tab stops** | one `tabs` table on `Term` for the same reason; resetting them on entry resurrects stops a `TBC 3` had cleared |
| The **clear is BCE** | the alternate screen's rows are reset with the *entering cursor's* erase cell, not with the default background, because the reference clones the primary cursor into the inactive grid before resetting its region |

Getting any of the three wrong moves `alt_reset`, `wrapline_alt_toggle`, `saved_cursor_alt`,
`tmux_htop`, `tmux_git_log` and the `vim_*` recordings, with no declared difference.

The cursor copy carries the **row index within the viewport** and the column, never a `RowId`
from the other screen (R-04). Leaving takes none of the `if` branch, so the primary screen
returns with exactly the cursor and saved cursor it had on entry.

**Correction C8:** `CSI ? 47 h/l`, `? 1047` and `? 1048` are implemented, where the reference
recognises only `1049` and silently drops the others (trap 13). `US-0072` greps the recordings for
them and records any affected cells as declared expected differences, so the behaviour is correct
from the start and the gate stays meaningful.

### Erase, insert, delete

**Correctness first (owner ruling, 2026-09-12).** The engine is a new build, not a copy: where the
reference's behaviour is a defect, this engine does the spec-correct thing from the start, and the
difference is declared as a `C`-row in the deviation table with the recordings it affects. The
parity harness carries per-recording, cell-level expected differences keyed by deviation id
([`testing-and-bench.md`](testing-and-bench.md) § 2), so a corrected quirk never hides a
regression: the gate still fails on any difference that is not declared.

| Op | Behaviour |
| --- | --- |
| `EL 0` (right) | `col..cols`. **Returns immediately, erasing nothing, if `pending_wrap` is set** (trap 2; see G3 for when the flag can be set) |
| `EL 1` (left) | `0..=col` |
| `EL 2` | `0..cols` |
| `ECH` (`CSI X`) | `col..min(col + n, cols)`, filled with the **erase cell** (see below) |
| `DCH` (`CSI P`) | **Spec-correct (C1)**: shift `row[col + n..]` left to `col` and fill the last `min(n, cols - col)` cells with the template background. The reference clamps `end` to `cols - 1`, which for a large `n` is not a plain shift (trap 19) |
| `ICH` (`CSI @`) | `n = min(count, cols - col)`; swap from the end; fill `col..col+n` with the **erase cell** |
| `ED 0` (below) | clear `col..` on the cursor row, then reset every row below |
| `ED 1` (above) | **Spec-correct (C2)**: reset every row above the cursor, then clear `0..=col` on the cursor row. The reference guards with `cursor_row_index > 1`, so row 0 survives when the cursor is on row 1 (trap 11) |
| `ED 2` (all) | alternate screen: reset every viewport row. Primary: scroll the occupied part of the viewport into scrollback (`clear_viewport`), keeping the content, moving the bottom down by `positions` and leaving the scroll offset unchanged so a scrolled-back user keeps seeing the same content (trap 9). The cursor does not move |
| `ED 3` (saved) | when history is non-empty, drop it and set `offset = 0` (trap 10). `VtEvent::ScreenCleared` is emitted **before** the "is there history" check (trap 12) |

**Blanking a row is a mutation, and must be stamped — `US-0075` rework.** `place_row` and
`reset_row` (`crates/vt/src/grid/screen.rs:445-469`) currently store `None` when the source slot
was unallocated or the erase template is empty, which drops the row header: the row keeps its
`RowId` but its `seq` reads back as the default and its `DIRTY` bit is gone. The rule is the
opposite, and it is the documented damage contract:

> Every operation that de-allocates or re-places a row stamps that row with the **current batch
> sequence number** and sets `RowFlags::DIRTY`, exactly as a write does. A blanked row is a changed
> row.

Without it `DEC-0015`'s "a second consumer becomes possible without an engine change" is false —
any consumer reading `row.seq() > watermark`, which is the documented mechanism, misses the
blanking — and `DIRTY` produces a **false negative**, which
[`damage-and-render-state.md`](damage-and-render-state.md) forbids: a de-allocated row that held a
`GraphemeId` leaks an arena entry, and one that held a `GraphicId` never fires
`VtEvent::GraphicReleased`, so the view's texture never evicts. Both are latent until `US-0074`'s
sweep and `US-0080`'s release scan read `DIRTY`. `US-0079` works around it today with a private
`RenderRow::allocated` flag, which is correct for that one consumer and only that one.

**Accepted simplification (M10):** `repair_wide_pairs` sweeps the **whole** row after every
in-row mutation (`EL`, `ECH`, `DCH`, `ICH`, the insert shift), where the reference repairs only the
boundary cells. It is correctness-neutral and O(cols) on operations that are already O(cols), and
narrowing it would trade a plainly-correct invariant for a micro-optimisation the intake forbids as
an outcome. Revisit only if a measurement asks.

**The erase cell, and why it is not the SGR template.** `Cursor::template` is a `Cell`, not a
`Style`: it carries the interned `StyleId`, the extras id and the OSC 133 semantic, so
`Row::reset`'s background-change discriminant can be read from it without touching the interner.
But **every erase fills with the template's background only** — never with its foreground,
attributes, hyperlink or semantic — because the reference fills with `bg.into()`
(`term/mod.rs:1669-1671`, `:1548-1553`, `:1583-1588`, `:1216-1219`, `:1789-1800`). Erasing while
an underline, a strikeout, an inverse or an open `OSC 8` hyperlink is active must not leave
underlined or clickable blanks.

**The cursor therefore carries two cells, not one.** `Cursor::template` is the reference's
`cursor.template` — the SGR style, the open hyperlink or image, the OSC 133 semantic — and is what
a *printed* glyph inherits. `Cursor::erase` is the reference's `bg.into()`: the default cell with
only the template's background. Both are derived once in `Screen::set_template`, the **only**
writer, so the erase paths never reach into the interner and an erase under an open underline,
strikeout or `OSC 8` hyperlink still leaves plain blanks. Consequence to keep in mind:
`Row::reset`'s background-erase discriminant is the **erase** cell's interned style id, which now
differs exactly when the background differs, because nothing else is left in that style.

`ED 2`'s occupancy scan walks backwards from the bottom-right for the last non-blank cell using
`Cell::is_erasable()`, which decides how many rows enter scrollback. **A cell carrying a graphic
reference is not erasable** (R-13): otherwise `CSI 2 J` over an image would decide the image rows
are unoccupied and discard them instead of scrolling them into history. See
[`cell-and-style.md`](cell-and-style.md) for both predicates.

Selection invalidation follows the reference and is specified in
[`selection.md`](selection.md).

### Tab stops

`TabStops` is a bitmap, initialised to every eighth column.

- `HTS` sets a stop at the cursor column; `TBC 0` clears the stop under the cursor; `TBC 3`
  clears all.
- `put_tab(n)`: if `pending_wrap` is set, `wrapline()` and **return** (trap 3). Otherwise, per
  count: write a `\t` into the cell **only if it currently holds a space**, then walk right to
  the next stop, stopping at `cols - 1`. What a `\t` cell means downstream is specified in
  [`cell-and-style.md`](cell-and-style.md) (R-12).
- On resize, stops in the retained prefix keep their cleared state and the regrown region gets
  default stops at absolute multiples of eight (trap 27). Reproduced.
- **Correction C10:** `CSI ? 5 W` (reset every stop to the default eight-column grid) is
  implemented; the reference parses it and does nothing.

### Deliberate deviations

| # | Deviation | Packet | Recording risk |
| --- | --- | --- | --- |
| G1 | `WRAPPED` is a row flag, not a flag on the last cell | `US-0075` | none — a representation change |
| G2 | `RowId` replaces signed `Line`; no negative indices | `US-0075` | none |
| G3 | With `DECAWM` off the pending-wrap flag is not set, so `EL 0` erases and `HT` moves (R-09) | `US-0075` | **measured**: 4 recordings reset `? 7` — `vttest_origin_mode_1`, `vttest_origin_mode_2`, `vttest_scroll`, `vttest_tab_clear_set`. Observable only through `EL 0` or `HT` while the flag would have been armed, so one or more may need a declared diff |
| G6 | The ring index is the row id; no `zero` rotation and no free list | `US-0075` | none — removes trap 45 |
| G7 | One row representation; dual-form rows deferred (R-51) | `US-0075` | none |

**Corrections — spec-correct from the start, with declared expected differences.** Each row below
is a defect in the engine being replaced, fixed rather than reproduced (owner ruling, 2026-09-12).
`US-0072`'s scripted grep has measured which recordings each correction can touch (below, from
`evidence/US-0072-recording-risk.md`); the packet that implements a correction writes the exact
cells into that recording's `expected-diffs.json`, so a corrected quirk can never hide a
regression: the gate still fails on any difference that is not declared.

| C | Correction | Trap | Packet | Affected recordings (**measured in `US-0072`**, `evidence/US-0072-recording-risk.md`) |
| --- | --- | --- | --- | --- |
| C1 | `DCH` is a plain shift left by `n` | 19 | `US-0075` | **8 recordings send `DCH`**: `decaln_reset`, `deccolm_reset`, `delete_chars_reset`, `erase_chars_reset`, `insert_blank_reset`, `region_scroll_down`, `scroll_up_reset`, `underline`. The quirk needs `count >= cols - col`, so the large counts are the likely ones — `erase_chars_reset` (max 21), `deccolm_reset` (max 15), `scroll_up_reset` (max 11). The packet writes the exact cells into each affected `expected-diffs.json` |
| C2 | `ED 1` clears row 0 | 11 | `US-0075` | **2 recordings**: `vttest_cursor_movement_1`, `vttest_origin_mode_1` |
| C3 | A short region scroll rotates, then blanks | 17 | `US-0075` | **2 recordings** pair `DECSTBM` with `IL`/`DL`: `vim_24bitcolors_bce` and `vttest_insert`. `vttest_insert`'s 22-row region with counts up to 24 is exactly the "count at or above the region height" case. (The earlier guesses `region_scroll_down` and `scroll_in_region_up_preserves_history` pair no region with a region-scroll primitive) |
| C4 | Insert mode repairs wide pairs instead of leaving orphaned spacers | 7 | `US-0075` | **1 recording**: `vttest_insert` (IRM set once, one non-ASCII character printed) — possible, not certain |
| C8 | `? 47` / `? 1047` / `? 1048` implemented | 13 | `US-0076` | **none of the 45** — `wrapline_alt_toggle`, `alt_reset` and `saved_cursor_alt` all use `? 1049`. Free |
| C10 | `CSI ? 5 W` restores the default tab stops | 27 | `US-0076` | **none of the 45**. Free |
| C12 | Wide pairs are repaired after every in-row mutation, so `EL` / `ECH` / `DCH` / `ICH` and the insert shift never leave an orphaned spacer | 6 | `US-0075` | **none measured**: the US-0075 verification UTF-8-decoded all 45 recordings and found **no width-2 glyph**. Upper bound if that scan missed one — recordings carrying both non-ASCII bytes and an in-row erase — is 22 of 45, the material ones being `vim_large_window_scroll`, `vim_24bitcolors_bce`, `tmux_git_log`, `tmux_htop`, `region_scroll_down`, `zerowidth`, `wrapline_alt_toggle`, `issue_855`, `fish_cc`, `colored_underline`. The reference leaves the orphans; the design's own integrity assertion forbids that state, so the repair is compulsory |

Kept deliberately, because they are correct behaviour or OneTerm product behaviour rather than
defects: pending wrap and its interaction with `BS`, `EL 0` and `HT` (traps 1, 2, 3); `ED 2`
scrolling the viewport into scrollback instead of discarding it (trap 9, which users rely on);
`ED 3` snapping to the bottom (trap 10); `DECSTBM` validation (trap 15); the cursor-outside-region
rules (trap 16); `SD` never pulling from history (trap 18); scrollback filling only from a region
anchored at row 0 (the other half of trap 17, which is what xterm does); `occ`-bounded row resets
(trap 36); and entering the alternate screen taking the primary `DECSC` slot (trap 14, which is
what `? 1049` means).

## Interfaces

```rust
// crates/vt/src/grid.rs
impl Screen {
    pub fn row(&self, id: RowId) -> RowRef<'_>;              // a None slot reads as blanks
    pub fn row_mut(&mut self, id: RowId) -> RowMut<'_>;      // allocates the slot, stamps seq
    pub fn row_range(&self) -> Range<RowId>;                 // oldest..newest + 1
    pub fn viewport(&self) -> Viewport;                      // { top: RowId, rows, cols }
    pub fn scroll_viewport(&mut self, delta: i32);
    pub fn scroll_to_bottom(&mut self);
    pub fn history_len(&self) -> u32;

    /// Rows only: truncate or pad. Columns and reflow are `Terminal::resize` in `US-0077`
    /// ([`reflow-and-resize.md`](reflow-and-resize.md)); this entry point never reflows.
    pub fn resize_rows(&mut self, rows: u16, anchors: &mut Anchors);

    pub fn scroll_up(&mut self, region: ScrollRegion, n: u16, anchors: &mut Anchors);
    pub fn scroll_down(&mut self, region: ScrollRegion, n: u16, anchors: &mut Anchors);
    pub fn clear_viewport(&mut self, anchors: &mut Anchors);
    pub fn clear_history(&mut self, anchors: &mut Anchors);
    pub fn reset_rows(&mut self, rows: Range<u16>);          // viewport-relative, BCE template

    pub fn erase_line(&mut self, mode: LineClear);
    pub fn erase_chars(&mut self, n: u16);
    pub fn delete_chars(&mut self, n: u16);
    pub fn insert_blanks(&mut self, n: u16);

    pub fn row_text(&self, id: RowId, out: &mut String);     // spacer- and grapheme-aware
}
```

`assert_integrity` (debug only, budget in [`testing-and-bench.md`](testing-and-bench.md)) checks:
`oldest <= newest`; the two screens' live ranges do not overlap; the viewport offset is within
`history_len()`; no `Wide` cell without its `WideSpacer` and no `WideSpacer` without its `Wide`;
every row's `id` matches its slot; `RowFlags` has no false negative; every live anchor is inside
the live row range; every interned id resolves. It does **not** check `occ`, which is an
over-approximation by definition.

## Edge Cases and Failure Modes

- [ ] **Trap 1 — pending wrap then `BS`.** `BS` decrements the column and clears the pending
  wrap. `BS` at column 0 is a no-op **while `Mode::ReverseWrap` (`? 45`) is reset, which is the
  default** (R-08); with it set, `BS` at column 0 moves to the previous row's last column when
  that row is `WRAPPED`. `? 45` is an additive feature and lands in `US-0086`.
- [ ] **Trap 2 / 3 — pending wrap then `EL 0` / `HT`**, including the G3 combinations.
- [ ] **Trap 9 / 10 / 11 — `ED 2` / `ED 3` / `ED 1`**, each stated against the offset table.
- [ ] **Trap 14 — entering the alternate screen clobbers the primary DECSC slot.**
- [ ] **Trap 16 / 17 / 18 / 19 / 27 / 36 / 46** as tabulated above.
- [ ] **An anchor inside a region that is blanked without rotating** dies; the consumer sees its
  mark disappear rather than move to unrelated content.
- [ ] **Scrollback limit of 0** (the alternate screen) — `scroll_up` trims immediately.
- [ ] **`cols == 1`** — wide characters can never be placed; they are dropped with a counter.
- [ ] **Row slot reuse across a trim** — the slot is cleared before reuse; `assert_integrity`
  checks `row.header.id == id`.
- [ ] **`scrollback_limit` changed at run time** — one rehome, O(live rows), off the resize path.
- [ ] **A resize beyond `MAX_ROWS = 1024`** is clamped, so the ring mask stays valid.

## Verification

`cargo test -p oneterm-vt grid::` and `anchor::`

Viewport and identity:

- [ ] `grid::tests::offset_zero_follows_output`
- [ ] `grid::tests::scrolled_back_view_holds_still_while_rows_are_pushed`
- [ ] `grid::tests::offset_is_capped_by_history`
- [ ] `grid::tests::row_ids_are_monotonic_across_scroll_clear_and_ris`
- [ ] `grid::tests::the_two_screens_never_share_a_row_id`
- [ ] `grid::tests::lines_produced_counts_output_lines_not_wraps` — the gutter contract (R-05).
- [ ] `grid::tests::lines_produced_is_unchanged_by_reflow_and_by_clear`
- [ ] `grid::tests::unwritten_slots_read_as_blanks`
- [ ] `grid::tests::ring_mask_is_constant_across_resizes` — R-30.
- [ ] `grid::tests::changing_the_scrollback_limit_rehomes_live_rows`

Anchors:

- [ ] `anchor::tests::region_scroll_moves_anchors_with_their_content`
- [ ] `anchor::tests::anchor_in_a_blanked_region_dies`
- [ ] `anchor::tests::trim_kills_anchors_below_oldest_within_the_lane`
- [ ] `anchor::tests::an_alt_screen_scroll_leaves_primary_anchors_alone` — the lane rule; one alt
  line feed must not kill a primary mark, selection anchor or graphics placement.
- [ ] `anchor::tests::saved_cursor_survives_a_region_scroll` — `DECSC`, `SU`, `DECRC`.
- [ ] `anchor::tests::rows_scrolled_event_matches_the_anchor_shift` — per the contract table:
  a whole-viewport scroll reports nothing, a bottom-bounded region reports the region **and** the
  tail, an invalid region reports nothing.
- [ ] `grid::tests::erased_cells_keep_only_the_background` — an `ECH` under an active underline and
  hyperlink leaves plain blanks (reading 7).
- [ ] `grid::tests::tab_does_not_orphan_a_wide_pair` — the `put_tab` rule above.
- [ ] `grid::tests::wide_char_wrapping_over_an_existing_pair_keeps_the_grid_intact` — the wrapping
  `LeadingWideSpacer` goes through `write_at_cursor`, which repairs first.
- [ ] `anchor::tests::rows_scrolled_is_silent_for_a_whole_screen_scroll`,
  `anchor::tests::rows_scrolled_reports_the_tail_shift_of_a_bounded_region` — the `Option` contract.
- [ ] `grid::tests::entering_alt_screen_keeps_the_region_and_the_tab_stops`,
  `grid::tests::entering_alt_screen_clears_with_the_background_template`
- [ ] `grid::tests::resize_rows_truncates_and_pads_without_reflowing`

Scrolling, erase and tabs (trap-mapped):

- [ ] `grid::tests::pending_wrap_then_backspace`, `..::backspace_at_column_zero_is_a_noop` — trap 1.
- [ ] `grid::tests::pending_wrap_then_el0_erases_nothing` — trap 2.
- [ ] `grid::tests::decawm_off_then_el0_erases` — deviation G3's observable half (R-09).
- [ ] `grid::tests::pending_wrap_then_tab_wraps_and_returns` — trap 3.
- [ ] `grid::tests::decawm_off_then_tab_moves_to_the_next_stop` — G3, second half.
- [ ] `grid::tests::ed2_scrolls_the_viewport_into_history` — trap 9.
- [ ] `grid::tests::ed2_keeps_the_scrolled_back_viewport_position` — trap 9, written against the
  offset table.
- [ ] `grid::tests::ed2_over_an_image_keeps_the_image_rows` — R-13.
- [ ] `grid::tests::ed3_resets_the_viewport_to_the_bottom` — trap 10.
- [ ] `grid::tests::ed1_clears_row_zero` — correction C2, trap 11.
- [ ] `grid::tests::entering_alt_screen_overwrites_the_saved_cursor` — trap 14.
- [ ] `grid::tests::leaving_alt_screen_restores_the_entry_cursor` — trap 14.
- [ ] `grid::tests::linefeed_below_the_region_does_not_scroll` — trap 16.
- [ ] `grid::tests::insert_and_delete_lines_outside_the_region_are_noops` — trap 16.
- [ ] `grid::tests::scroll_up_fills_history_only_from_row_zero` — trap 17.
- [ ] `grid::tests::scroll_up_with_a_bottom_bounded_region_keeps_the_rows_below` — R-03.
- [ ] `grid::tests::small_region_scroll_rotates_then_blanks` — correction C3, trap 17.
- [ ] `grid::tests::scroll_down_never_pulls_from_history` — trap 18.
- [ ] `grid::tests::delete_chars_shifts_left_by_n` — correction C1, trap 19.
- [ ] `grid::tests::decstbm_invalid_range_is_a_noop_and_valid_homes_the_cursor` — trap 15.
- [ ] `grid::tests::tab_stops_survive_and_regrow_across_a_resize` — trap 27.
- [ ] `grid::tests::row_reset_respects_occ_and_the_background_template` — trap 36.
- [ ] `grid::tests::first_visible_cell_is_viewport_top_column_zero` — trap 46.
- [ ] `grid::tests::wide_char_dropped_when_cols_is_one`
- [ ] `grid::tests::trimmed_slot_is_cleared_before_reuse`
- [ ] `grid::tests::blanking_a_row_stamps_it_dirty_with_the_batch_seq` — the `US-0075` rework
  above, over both `place_row` and the `reset_row` empty-template path.

In `US-0076` with the other corrections: `grid::tests::alt_screen_47_and_1047_and_1048` (C8) and
`grid::tests::csi_5_w_restores_default_tab_stops` (C10). Deferred to `US-0086` as an additive
feature rather than a correction: `grid::tests::reverse_wrap_crosses_a_wrapped_row` (`? 45`).

Property test: `grid::props::scroll_and_erase_preserve_integrity` — a random sequence of scrolls,
erases, inserts and deletes with anchors registered throughout, then `assert_integrity()`.
