# Low-Level Design: Reflow and resize

Intake: IN-0029
HLD: ../high-level-design.md
Topic: reflow-and-resize
Date: 2026-09-12

> One concern per file. Implementation-level mechanics for `crates/vt/src/reflow.rs`.

## Concern

Changing the grid's dimensions: rejoining and resplitting wrapped logical lines, moving every
anchor that pointed into the old rows, and honouring the two resize policies OneTerm needs.

This is the highest-risk component in the intake: alacritty has six open reflow bugs, xterm.js
ships a guard commented "has been known to fail for an unknown reason", and Windows Terminal's
`TextBuffer::Reflow` documents a loop that can deadlock without two defensive clamps. OneTerm
additionally has a **second** contract to satisfy — conhost's behaviour behind ConPTY
(`docs/terminal-backend.md` § 5.3, `DEC-0008`).

It replaces `vendor/alacritty_terminal/src/grid/resize.rs` **and**
`crates/terminal/src/model.rs:481-541` (`resize_keeping_viewport_top` and `conhost_cursor_row`).

## Design

### Shape of the operation

```rust
pub enum ResizePolicy { BottomAnchor, KeepViewportTop }

impl Terminal {
    pub fn resize(&mut self, size: Size, policy: ResizePolicy) -> ResizeOutcome;
}
pub struct ResizeOutcome { pub reflowed: bool, pub rows_trimmed: u32 }
```

**There is no public tracking-point slice and no public `RowRemap` (R-31).** Everything that
needs to move is already an entry in the engine's tracked-anchor list, whose canonical
`AnchorKind` list is in [`grid-and-scrollback.md`](grid-and-scrollback.md) § "Tracked anchors" and
includes `Cursor` and `ViewportTop` for exactly this reason (N-01).

**This packet owns the screen discriminant on `AnchorKind` (M6).** Each screen registers its own
`Cursor`, `SavedCursor` and `ViewportTop`, so the list holds two of each; reflow runs on the
**primary** screen only, which makes it the first code that must select one screen's entries rather
than every entry of a kind. `US-0077` therefore adds the discriminant (or an equivalent lane check
in `Anchors::remap`) and updates the canonical list in
[`grid-and-scrollback.md`](grid-and-scrollback.md) § "Tracked anchors" and `DEC-0015` with it.
Reading the remapped entries back into the fields must happen **before** the next
`Screen::sync_anchors`, which otherwise overwrites them from the fields. Reflow calls `Anchors::remap` with the same closure it uses to move the cursor, so there is
**one** anchor mechanism, shared with the scroll primitives and exercised by both test suites. A
consumer that wants its own anchor registers one; it never passes a slice into `resize` and never
receives an O(scrollback) remap table to walk.

Internally the remap is a closure over the reflow iterator's state, not a materialised table, so a
drag frame allocates nothing proportional to the scrollback.

### Order of operations

```
1. if size == current: return early (identity, no damage)     [kitty's memcpy fast path]
2. clamp rows to MAX_ROWS (1024) so the ring mask stays valid
3. if rows changed and cols unchanged: rows_only(size, policy)   [no reflow at all]
4. else: reflow_columns(new_cols) then rows_only(size, policy)
5. reset the scroll region to the full screen                    [trap 28]
6. reset the tab stops for the new width                         [trap 27 rules]
7. clear the selection iff the column count changed              [trap 28]
8. mark everything damaged -> the next render_update returns Full
```

The ring length does **not** change on resize: it is sized once from
`next_power_of_two(scrollback_limit + MAX_ROWS)`, so `mask` is a session constant and no rehome
is needed here (R-30).

### `reflow_columns` — the iterator

Ported from `avt`'s `Reflow` iterator (Apache-2.0, proptest-covered; attribution in the file
header, in `NOTICE` and in `THIRD-PARTY-NOTICES.md`).

1. Walk the old rows from `oldest` to `newest`, grouping them into **logical lines**: a run of
   rows where each but the last carries `RowFlags::WRAPPED`.
2. Trim trailing blank cells from each logical line, **except** on the cursor's logical line,
   where the line is treated as if whitespace fills up to the cursor column. This is Windows
   Terminal's `REFLOW_JANK_CURSOR_WRAP` assumption and it is what keeps the cursor's distance from
   the preceding text.
3. Redistribute each logical line into `new_cols`-wide rows. A wide character that would land in
   the new last column is replaced by a `LeadingWideSpacer` and the glyph moves to the next row
   (trap 33). A wrapped remainder ending in a `LeadingWideSpacer` drops it and moves `WRAPPED` one
   cell left.
4. Every row but the last of a logical line gets `RowFlags::WRAPPED`.
5. New rows get fresh `RowId`s. Anchors are moved during the walk: an anchor is carried by the
   character it sat on; an anchor on a cell trimmed as trailing whitespace lands at the end of its
   logical line; an anchor whose logical line is trimmed out of history dies.
6. Trim to `scrollback_limit + rows` from the **oldest** end (trap 32) and emit
   `VtEvent::RowsTrimmed`.

`pending_wrap` is converted into "the cursor column equals `cols`" before reflowing and
re-derived afterwards — re-armed only when the cursor's new row lacks `WRAPPED` and the cursor
sits past the last column (trap 31).

**The alternate screen never reflows** (trap 29): it is truncated on shrink, padded on grow, and
its cursor column is clamped.

### The viewport across a reflow (R-06)

The viewport top is a **tracked anchor** (`AnchorKind::ViewportTop`, a permanent entry, not one
registered per resize), read back after step 3 and written into `Viewport::offset`. This replaces the earlier claim
that "`viewport_top` is preserved", which was not well formed: step 5 gives every row a fresh id,
so the old top row does not exist afterwards.

The invariant that *is* well formed, and that the test asserts:

> the character that was the first visible character before the resize is the first visible
> character after it, unless it was trimmed, in which case the viewport clamps to the oldest
> surviving row.

After remapping, `offset` is recomputed as `newest - anchor.row` clamped to `history_len()`,
which reproduces the reference's "adjust `display_offset` by one per row created or destroyed
above the viewport, then `min(display_offset, history_size())`" (trap 30) without counting rows.
A viewport at `offset == 0` before the resize stays at `offset == 0`: sticky bottom survives a
resize.

### `rows_only`

| Policy | Grow | Shrink |
| --- | --- | --- |
| `BottomAnchor` (SSH, Unix PTY) | pull `min(history, added)` rows out of scrollback into the top; the cursor moves **down** by that amount | push `(cursor_row_index + 1) - new_rows` rows into history if positive; clamp the cursor and the saved cursor |
| `KeepViewportTop` (Windows local, `DEC-0008`) | — see the procedure below; this policy is **not** a variant of `rows_only`, it is a correction applied around it |

### `KeepViewportTop` — one procedure, stated once (R-07)

The earlier draft described this policy twice, as a table row ("keeps the viewport top, appends
rows at the bottom, keeps the cursor's row") and as a procedure (measure, run `BottomAnchor`,
shift). Those are different algorithms. **The table row is deleted; the procedure is the
design**, because it is the one the ten `keep_viewport_top_*` tests pin.

```
Preconditions: applies to the PRIMARY screen, always — including while the alternate screen is
active, in which case the alternate screen is resized with reflow disabled and is otherwise left
alone (docs/terminal-backend.md § 5.3). measure_rows reads the PRIMARY screen's cursor and
viewport, never the alternate's.

1. conhost_row = measure_rows(primary, new_cols)          // before touching anything
2. reflow_columns + rows_only(BottomAnchor)               // the same code path as the default
3. shift = primary.cursor_row_index - conhost_row
     shift > 0  -> scroll the whole screen up by `shift` (top rows return to history,
                   the bottom is blanked); anchors move through Anchors::shift_region
     shift < 0  -> pull `-shift` rows back from history and drop blank bottom rows
4. the saved cursor moves with the shift, clamped to the screen (it is a tracked anchor,
   so this is automatic)
5. restore the pre-resize viewport offset, clamped to history
6. clear the selection when shift != 0
```

`measure_rows` reflows **only the viewport rows from the top down to the cursor row**, through the
same iterator, and returns how many rows below the top the cursor lands on:

```rust
fn measure_rows(screen: &Screen, new_cols: u16) -> u16;
```

There is no grid clone, no placeholder grid and no double `swap_alt`: the engine owns both
screens, so the correction addresses the primary one directly. That is what deletes
`crates/terminal/src/model.rs:481-541`.

**Risk and fallback.** `measure_rows` must produce the same number the current scratch-grid probe
produces, which is what the ten `keep_viewport_top_*` tests pin. If it cannot, the fallback is to
port `conhost_cursor_row` verbatim behind the same `ResizePolicy` value — a contained loss. The
old suite keeps running against the old engine until the adapter packet
([`migration.md`](migration.md), R-44), so the comparison is always available.

**Beyond ConPTY 1.24.** Windows Terminal's own fix is for conhost to re-issue `ESC [ 6 n` after
every resize and adapt to the host's answer. The engine already answers `CSI 6 n` honestly, so
nothing is needed here; a later packet may make `KeepViewportTop` a fallback used only when no
post-resize CPR arrives. Out of scope, noted so it is not designed against.

### conhost quirks the reflow must match

| Quirk | Handling |
| --- | --- |
| Trailing whitespace is truncated | step 2 trims trailing blanks per logical line |
| The cursor row behaves as if whitespace fills up to the cursor | step 2's exception |
| Double-height rows are truncated, not reflowed | not applicable: `DECDWL`/`DECDHL` are unimplemented |
| The cursor can be lost when more text follows it than fits | the cursor anchor clamps to the last row; a debug assertion catches an unmapped cursor |

### Cost model, not a target (R-29)

The earlier phase plan carried "target: nearer Rio's 5 us than alacritty's 227 us". That number is
**deleted**: the algorithm reflows the whole live row set and allocates a fresh `RowId` per new
row, so it is inherently **O(live rows x cols)** — milliseconds for a 100 000-row scrollback, not
microseconds. The published Rio figures come from two contradictory number sets and only their
ratio is trustworthy.

What the benchmark records instead: resize latency at three scrollback depths (0, 10 000 and
100 000 rows) for both engines, so the change is visible as a ratio against the engine being
replaced. The real lever for drag-resize responsiveness is **reflowing the viewport eagerly and
the history lazily**; that is not in this intake, and the cost model above is the reason a later
packet may want it.

## Interfaces

```rust
// crates/vt/src/reflow.rs
pub(crate) fn reflow_columns(screen: &mut Screen, new_cols: u16, anchors: &mut Anchors);
pub(crate) fn measure_rows(screen: &Screen, new_cols: u16) -> u16;
```

Both are crate-private. The public surface is `Terminal::resize(size, policy) -> ResizeOutcome`.

## Edge Cases and Failure Modes

- [ ] **Trap 28 — a resize destroys the scroll region** and clears the selection when the column
  count changed.
- [ ] **Trap 29 — the alternate screen never reflows.**
- [ ] **Trap 30 — resize with the viewport scrolled back**, expressed through the
  `ViewportTop` anchor (R-06).
- [ ] **Trap 31 — reflow re-arms the pending wrap** only when the new row lacks `WRAPPED` and the
  cursor is past the last column.
- [ ] **Trap 32 — shrinking columns truncates history**, with `RowsTrimmed` emitted and anchors
  killed.
- [ ] **Trap 33 — reflow inserts and removes leading wide spacers.**
- [ ] **A logical line longer than the whole scrollback** is bounded by step 6; anchors on the
  trimmed part die.
- [ ] **Zero-size or oversized resize** — clamped to `1..=MAX_ROWS` and `1..=MAX_COLS` at the API
  boundary (`MAX_ROWS = 1024` and `MAX_COLS = 2048`, both defined in
  [`grid-and-scrollback.md`](grid-and-scrollback.md) § "Storage", N-11); a resize to the same size
  returns early.
- [ ] **An anchor on a cell reflow deleted** (trailing whitespace) lands at the end of its logical
  line, so a selection anchor never vanishes silently while its partner survives.
- [ ] **Graphics placements** move through the same anchor list; a placement whose anchor died
  emits `VtEvent::GraphicReleased` ([`graphics.md`](graphics.md)).
- [ ] **Resize during output** — the caller resizes the PTY first and the grid second
  (`docs/terminal-backend.md` § 5.3); the engine is single-threaded, so there is no race inside it.

## Verification

`cargo test -p oneterm-vt reflow::`

Ported behaviour tests (the reference's own cases, renamed):

- [ ] `reflow::tests::shrink_joins_then_splits` (`shrink_reflow`)
- [ ] `reflow::tests::shrink_twice_is_stable` (`shrink_reflow_twice`)
- [ ] `reflow::tests::shrink_with_an_empty_cell_inside_the_line`
- [ ] `reflow::tests::grow_rejoins_wrapped_rows` (`grow_reflow`)
- [ ] `reflow::tests::grow_rejoins_across_multiple_rows` (`grow_reflow_multiline`)
- [ ] `reflow::tests::alt_screen_grow_and_shrink_do_not_reflow` (trap 29)
- [ ] `reflow::tests::rows_only_grow_moves_the_cursor_down_by_the_pulled_rows`
- [ ] `reflow::tests::rows_only_shrink_pushes_rows_into_history`
- [ ] `reflow::tests::resize_resets_the_scroll_region_and_clears_the_selection` — trap 28.
- [ ] `reflow::tests::first_visible_character_survives_a_resize` — trap 30, R-06; replaces the
  ill-formed "the top row is unchanged".
- [ ] `reflow::tests::sticky_bottom_survives_a_resize`
- [ ] `reflow::tests::pending_wrap_is_rearmed_only_without_the_wrapped_flag` — trap 31.
- [ ] `reflow::tests::shrink_columns_truncates_the_oldest_history` — trap 32.
- [ ] `reflow::tests::wide_char_at_the_new_last_column_moves_to_the_next_row` — trap 33.

ConPTY policy tests — the engine-side equivalents of `crates/terminal/src/model.rs:616-910`:

- [ ] `reflow::tests::keep_viewport_top_grow_rows`
- [ ] `reflow::tests::keep_viewport_top_grow_columns_joins_wrapped_rows`
- [ ] `reflow::tests::keep_viewport_top_shrink_columns_splits_rows`
- [ ] `reflow::tests::keep_viewport_top_moves_the_saved_cursor`
- [ ] `reflow::tests::keep_viewport_top_restores_the_scroll_offset`
- [ ] `reflow::tests::keep_viewport_top_corrects_the_primary_screen_while_alt_is_active` — R-07;
  asserts the alternate screen is untouched and `measure_rows` read the primary cursor.
- [ ] `reflow::tests::keep_viewport_top_shrink_with_the_cursor_on_the_bottom_row_matches_bottom_anchor`
- [ ] `reflow::tests::default_policy_grow_anchors_the_bottom_row`
- [ ] `reflow::tests::measure_rows_matches_the_recorded_conhost_rows` — the three measured
  BUG-0051 cases (33x43 to 52x158 giving row 12; the widen to 132 columns giving row 14; the grow
  to 49x34 giving row 38). **Each fixture records the `OpenConsole.exe` version it was captured
  against** (R-39), because those numbers are a property of a specific host build; IN-0030's
  version-bump checklist gains a line to re-capture them when the bundled pair moves.

Property tests (`proptest`, 10 000 cases):

- [ ] `reflow::props::text_survives_a_resize_round_trip`
- [ ] `reflow::props::anchors_stay_on_their_character` — the tracked-anchor equivalent of the
  earlier tracking-point property; registers N anchors on known characters, resizes, asserts each
  live anchor still names the same character.
- [ ] `reflow::props::no_row_exceeds_the_column_count`
- [ ] `reflow::props::wrap_flags_are_consistent`
- [ ] `reflow::props::wide_pairs_are_never_split`
- [ ] `reflow::props::integrity_holds_after_any_resize_sequence`

Cross-crate: `crates/local-shell/src/session_tests.rs::local_session_grow_policy_matches_conpty`
and `crates/ssh/src/session.rs::ssh_session_keeps_the_default_grow_policy` keep their names and
meaning; the ten `model.rs` tests keep running against the **old** engine until the adapter packet
(R-44, [`migration.md`](migration.md)).

Benchmark: `vt-bench resize` at 0 / 10 000 / 100 000 rows of scrollback, both engines, recorded
not gated.
