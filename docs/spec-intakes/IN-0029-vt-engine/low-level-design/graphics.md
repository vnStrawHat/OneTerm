# Low-Level Design: Graphics

Intake: IN-0029
HLD: ../high-level-design.md
Topic: graphics
Date: 2026-09-12

> One concern per file. Implementation-level mechanics for `crates/vt/src/graphics/`.

## Concern

Decoding Sixel images, anchoring them to grid cells, telling the embedder when an image is no
longer referenced, and leaving room for the Kitty graphics protocol without building it.

This moves IN-0028's `vendor/patches/alacritty_terminal/0003` (577 patch lines, the largest of
the five) into first-party code. The behaviour contract is unchanged: `DEC-0012` (graphics are
cell-anchored), the VT340 10x20 virtual cell, and the conhost cursor rule.
[`../research/api-surface.md`](../research/api-surface.md) § 3.7 and § 5.4 are the reference
specification; `crates/terminal/src/sixel_tests.rs:46-271` is the behaviour that must survive.

## Design

### Ownership

```rust
pub struct GraphicId(pub u64);       // per terminal, starts at 1, not reset by RIS
pub struct GraphicData { pub id: GraphicId, pub width: u32, pub height: u32, pub rgba: Vec<u8> }

/// One record per image. Its `anchor` is an entry in the engine's tracked-anchor list.
pub struct Placement {
    pub id: GraphicId,
    pub anchor: AnchorId,            // AnchorKind::Graphic(id) -> (RowId, col) of the top-left
    pub cols: u16, pub rows: u16,    // extent in cells
    pub pixel_size: (u32, u32),
}
```

**The cell stores only *which* image, never *where inside it* (R-21).** A cell that a Sixel
covers carries an interned `Extras { graphic: Some(GraphicId), .. }`
([`cell-and-style.md`](cell-and-style.md)), so a whole image needs **one** extras entry rather
than one per covered cell. The earlier design put `GraphicRef { id, col, row }` in the extras,
which made every covered cell a distinct interned value: a 4096x4096 Sixel at the 10x20 virtual
cell covers about 84 050 cells, more than the whole `u16` extras id space, from one image — and
the table's fallback to id 0 means "no extras", so the image would silently lose its cells.

The painter derives the offset instead: for a cell at `(row, col)` carrying `GraphicId(g)`,

```
placement = placements[g]
offset    = (row - placement.anchor.row, col - placement.anchor.col)
origin_px = cell_origin(row, col) - offset * cell_size
```

which is the same arithmetic `crates/terminal-view/src/render/frame.rs` does today from the
per-cell offset, moved one level up. `render_update` copies the placement table into the render
state next to the rows, so the painter needs no engine access
([`damage-and-render-state.md`](damage-and-render-state.md)).

The placement's anchor is an ordinary tracked anchor, so an image **moves with its content**
through `IL`, `DL`, `SU`, `SD`, a region scroll and reflow, through the one mechanism that also
moves the cursor and the selection ([`grid-and-scrollback.md`](grid-and-scrollback.md) §
"Tracked anchors"). The earlier claim that "the placement's `top` follows the row, because it is
a `RowId`" was wrong for every in-region scroll, which is the normal case inside a TUI (R-02).

Pixels queue in the engine until `Terminal::take_graphics()` drains them: RGBA8, row-major,
stride `width * 4`, **straight (non-premultiplied) alpha**, oldest first, each image handed out
exactly once. **That is the only drain (R-16)**: the adapter calls it after the batch, and
`RenderState` never touches pixels. If `render_update` drained, a second render state would never
see an image and the adapter's own call would see nothing.

The engine never learns the real font cell size. The renderer rescales by `cell_width / 10` and
`line_height / 20`. There is no `set_cell_size` and there never was — `vendor/README.md` § 2
claims one exists and is stale; the replacement docs must not repeat the claim.

### Liveness and the release signal

The capability the current engine lacks: nothing tells the embedder that an image died, so
`crates/terminal-view`'s texture store guesses with an LRU and a 64-image / 64 MB cap.

**Liveness is derived from rows, not from a counter (R-22).** An earlier design decremented a
`live_cells` counter "in exactly one place: the cell writer", which misses every way a cell actually
stops referencing an image: `Row::reset`, scroll blanking, `clear_viewport`, an alt-screen wipe, and
reflow destroying rows.

**The scan runs in `feed`, never in `render_update`.** At the end of `feed` (and after `resize`
queues releases), the engine sweeps the placements whose anchor rows were touched:

```
for placement in placements:
    if placement.anchor is dead            -> release
    else if none of the rows in
            anchor.row .. anchor.row + rows carries RowFlags::HAS_GRAPHIC  -> release
```

`render_update` cannot do it: it has no `EventBatch` to deliver a `GraphicReleased` into, and R-16
keeps `RenderState` out of graphics ownership entirely, so a release discovered there would be
unobservable. The consequence for the embedder is a contract, not an accident: **a `resize` queues
releases that reach the batch only on the next `feed` — including an empty one.** The adapter must
issue `feed(&[])` after a resize, or a reflow that killed an anchor leaves the view's texture live
until the next byte arrives.

**What releases an image, and what does not.** Liveness is the `HAS_GRAPHIC` **row** flag, which is
a deliberate false positive (set on write, cleared only on reset), so:

| Releases the placement | Does **not** release it |
| --- | --- |
| `Row::reset` — scroll blanking, `reset_rows`, an alt-screen wipe, `RIS` | Overwriting every covered cell with text: the cell references drop, the flag stays, the placement stays live |
| A history trim that drops the anchor row | `EL 2` over every covered row: same shape — the cells are cleared, the row flag is not |
| A reflow that kills the anchor (delivered on the next `feed`) | Two images sharing cells: the older placement stays live while the newer one owns the cells |
| `MAX_PLACEMENTS` eviction, oldest first | Primary-screen `CSI 2 J` — see below |

The false positives are bounded and visually correct: the painter draws from the cells, so an image
whose cells are all overwritten is invisible while its placement is retained. It costs a table slot
and, through the view, a retained texture, until the row is reset or eviction reclaims it — which is
why `US-0081`'s texture store must tolerate up to `MAX_PLACEMENTS` live textures.

**Bound: `MAX_PLACEMENTS = 256`, oldest released first.** An unbounded placement table plus a linear
sweep is a denial-of-service surface, so the table is a ring: the 257th image releases the first and
emits its `GraphicReleased`. Note what this does **not** bound: `MAX_DIMENSION` allows one image to
be 64 MiB of RGBA, so 256 placements have no total-pixel-bytes budget. That matches the engine being
replaced, so it is not a regression; a later packet may add a byte budget if a workload ever holds
many large images.

**Primary-screen `CSI 2 J` keeps the image alive, in scrollback.** `ED 2` on the primary screen
scrolls the occupied viewport into history rather than discarding it (trap 9), and a cell carrying a
`GraphicId` is not erasable (R-13), so the image travels into scrollback with its rows. The engine
being replaced destroys it outright, because its `is_empty` ignores the graphic reference. **The
user-visible behaviour that IN-0028's walk records — "`cls` removes the image" — is preserved**: the
image leaves the viewport. What changes is that scrolling back now reveals it, which is the
improvement R-13 exists for, not a regression. On the **alternate** screen `ED 2` is a row reset, so
it does release.

Two debug invariants borrowed from foot, checked in `assert_integrity()`: no two placements with
overlapping column ranges on the same bottom row, and every `GraphicId` in a cell resolves to a
live placement.

### Sixel decoder

`crates/vt/src/graphics/sixel.rs`, ported from OneTerm's own patch — it is first-party code, so
no licence question arises. The DCS sink is wired to `dcs_hook` with final byte `q`
([`parser.md`](parser.md)); any other final byte clears an in-flight parser, so a non-Sixel DCS
aborts a prior unterminated Sixel.

Grammar (DEC STD 070 subset, unchanged from IN-0028):

| Token | Meaning |
| --- | --- |
| `0x3F..=0x7E` | six vertical pixels, value = byte - 0x3F, least significant bit at the top |
| `!Pn` | repeat the next sixel byte `Pn` times |
| `#Pr` | select colour register `Pr` |
| `#Pr;Pu;Px;Py;Pz` | define register: `Pu == 2` RGB in **percent 0-100**, `Pu == 1` DEC HLS with hue 0 = blue; other modes select only |
| `"Pan;Pad;Ph;Pv` | raster attributes; the declared size wins over the measured extents; the aspect ratio is parsed and ignored |
| `$` | graphics carriage return (back to the start of the band) |
| `-` | graphics newline (next band) |

- **Colour-mode divergence (parity break).** `#Pr;Pu;Px;Py;Pz` with `Pu` outside `{1, 2}` now
  **selects** register `Pr` and ignores the definition; the engine being replaced selects nothing at
  all. Measured at 72 of 96 RGBA bytes differing on a synthetic probe. No real producer emits
  `Pu ∉ {1, 2}` and no corpus recording or fixture reaches it, so the parity gate cannot see it —
  which is exactly why it is declared here rather than left as "the LLD text was followed".
- DCS parameters `P1;P2;P3` are parsed and unused. In particular **`P2` background-select is
  ignored and untouched pixels stay fully transparent**, which differs from a strict VT340 and
  is what `sixel_tests.rs` pins. Kept (deviation none — this is parity).
- Default palette: the 16 VT340 registers; 16..255 opaque black.
- `MAX_DIMENSION = 4096` per axis, clamped.
- **New: a payload byte cap.** The parser aborts a DCS past `DCS_MAX_BYTES = 16 MiB`
  ([`parser.md`](parser.md)) and the decoder additionally refuses to grow its pixel buffer past
  `4096 * 4096 * 4` bytes. The current code has the dimension clamp but no byte cap, so a stream
  that never sends `ST` is bounded only by the clamp
  ([`../research/prior-art.md`](../research/prior-art.md) § 9.7).
- Decoding is linear in payload bytes and allocates once the raster size is known, or grows band
  by band when it is not.

### Placement

On `dcs_unhook`, in the VT340 virtual cell `VIRTUAL_CELL = (10, 20)`:

```
cols = min(ceil(width  / 10), grid.cols - cursor.col)     // clipped right, never wrapped
rows =     ceil(height / 20)
cursor_rows = bands * 6 / 20                              // the conhost rule
```

1. Register the placement anchor at the cursor position, then stamp `Extras { graphic: Some(id) }`
   into every covered cell — **one interned extras entry for the whole image** — leaving the
   cell's text and style untouched, set `RowFlags::HAS_GRAPHIC` on each row, and damage it.
2. Walk the cursor down `cursor_rows` rows through the ordinary `linefeed()`, so the scroll
   region, scrollback and damage all behave as they do for text, keeping the column.
3. Rows below the final cursor row are placed without further scrolling and are clipped at the
   screen bottom.
4. Push `Arc<GraphicData>` onto the pending queue **after** the cells are stamped.

The 10x20 virtual cell and the `bands * 6 / 20` cursor rule are the conhost agreement: they are
why the prompt lands below the image rather than inside it when conhost issues its absolute
`CUP` (IN-0028's acceptance rework). Do not "improve" either without a fresh ConPTY capture.

`RIS` drops pending images and the in-flight parser but **does not reset the id counter**;
`CSI 2 J` does not clear pending graphics. Both are parity.

### Room for Kitty graphics (not built here)

The later intake needs four things, all of which exist after this packet:

1. **APC plumbing.** The parser already streams `ESC _ … ESC \` to an APC sink
   ([`parser.md`](parser.md)); today the sink discards.
2. **A placement identity separate from the image identity.** `Placement` already has its own
   record; Kitty's `placement_id` becomes a field of `Placement`, and the cell keeps carrying
   only an id.
3. **A z-index and a "draw under text" flag.** Reserved in `Placement`, unused.
4. **A Unicode-placeholder row flag.** `RowFlags` has spare bits; kitty and Ghostty both keep
   exactly such a flag (`has_image_placeholders`, `kitty_virtual_placeholder`) so a redraw can
   skip rows without placeholders. Reserved, unused.

Nothing in this design assumes an image is Sixel: `GraphicData` is format-agnostic RGBA and the
decoder is one implementation of a sink.

Also out of scope, and re-emitting rather than vanishing: XTSMGRAPHICS (`CSI ? Pi;Pa;Pv S`)
queries, `DECSDM` (mode `? 80`, whose polarity is inverted in most terminals — implement the
hardware-correct semantics when it is built, per
[`../research/prior-art.md`](../research/prior-art.md) § 6.2), `? 8452`, and iTerm2's OSC 1337
inline images.

## Interfaces

```rust
// crates/vt/src/graphics/mod.rs
impl Terminal {
    pub fn take_graphics(&mut self) -> Vec<Arc<GraphicData>>;   // drain, oldest first, once
}
pub const VIRTUAL_CELL: (u16, u16) = (10, 20);
pub const MAX_DIMENSION: u32 = 4096;

pub(crate) trait DcsSink {
    fn start(&mut self, params: &Params, final_byte: u8) -> bool;  // false => not ours
    fn put(&mut self, byte: u8);
    fn finish(&mut self, aborted: bool) -> Option<DecodedImage>;
}
pub(crate) struct SixelSink { /* state machine, palette, band buffer */ }
```

`crates/terminal`'s `TerminalContent.graphics` and `crates/terminal-view`'s `GraphicStore` keep
their shapes; the store gains a `GraphicReleased` handler that calls `window.drop_image` at once
instead of waiting for LRU eviction.

## Edge Cases and Failure Modes

- [ ] **An image wider than the grid** is clipped on the right, never wrapped.
- [ ] **An image taller than the screen** places what fits and clips the rest at the bottom.
- [ ] **A Sixel emitted at the bottom of the scroll region** scrolls its own top bands into
  scrollback through `linefeed()`; the placement follows its **anchor**, which the scroll
  primitive moves through `Anchors::shift_region` (N-06) — not because a `RowId` keeps its
  content, which it does not for an in-region scroll.
- [ ] **Text written over an image** drops that cell's reference; the row keeps `HAS_GRAPHIC`
  (a false positive, which is allowed) until the sweep finds no referencing cell left. The
  renderer still paints the whole image from the placement — the documented v1 limitation from
  `DEC-0012`, unchanged.
- [ ] **All of an image's cells erased by any means** — `CSI 2 J`, `clear`, a TUI repaint, a
  `Row::reset`, a scroll blank, an alt-screen entry or reflow — releases the placement and emits
  `GraphicReleased` (R-22). This is the capability the current engine lacks entirely, and the
  row-derived sweep is what makes it correct for the common cases a per-cell counter missed.
- [ ] **An unterminated Sixel** is bounded by the parser's byte cap and the dimension clamp;
  `dcs_unhook(aborted: true)` discards the partial image without stamping cells.
- [ ] **A non-Sixel DCS arriving mid-Sixel** clears the in-flight parser (parity).
- [ ] **Alternate-screen swap** carries cells and their references with the grid; pending pixels
  and the in-flight parser are untouched (parity).
- [ ] **A frame skipped by mode 2026** must not lose images: decoded pixels queue in the engine
  until the adapter's next `Terminal::take_graphics()`, which is the only drain and is not tied to
  painting at all ([`damage-and-render-state.md`](damage-and-render-state.md), R-16). `RenderState`
  never holds pixels.
- [ ] **ConPTY byte loss inside a DCS** — OpenConsole 1.23 was observed dropping one byte per
  32 KiB `WriteFile` inside a DCS payload (IN-0028 risk table). The mitigation is a host bump
  through IN-0030's bundled-pair manifest, not engine code; the decoder must still degrade
  gracefully, because a corrupt band must produce wrong pixels and never a panic or an unbounded
  allocation. A fuzz seed covers it.
- [ ] **Sixel depends on the bundled console host.** The inbox `conhost.exe` on Windows 11 24H2
  swallows Sixel DCS payloads, so graphics only survive the round trip through the bundled
  `conpty.dll` + `OpenConsole.exe` pair that `oneterm-pty` loads first ([`pty.md`](pty.md)). A
  Sixel evidence walk run on a machine without those files is expected to show nothing, and that
  is a host finding, not an engine defect — report the resolved backend with it.

## Verification

`cargo test -p oneterm-vt graphics::` — the ten behaviours currently in
`crates/terminal/src/sixel_tests.rs:46-271`, reproduced against the new engine:

- [ ] `graphics::tests::decodes_a_minimal_sixel`
- [ ] `graphics::tests::raster_attributes_declare_the_size`
- [ ] `graphics::tests::repeat_and_band_control_characters`
- [ ] `graphics::tests::rgb_and_hls_colour_registers`
- [ ] `graphics::tests::untouched_pixels_stay_transparent` — parity, P2 ignored.
- [ ] `graphics::tests::placed_at_the_cursor_column_and_clipped_right`
- [ ] `graphics::tests::cursor_descends_bands_times_six_over_twenty` — the conhost rule.
- [ ] `graphics::tests::scrolls_into_history_with_its_cells`
- [ ] `graphics::tests::overwriting_or_erasing_a_cell_drops_the_reference`
- [ ] `graphics::tests::ris_drops_pending_images_but_not_the_id_counter`
- [ ] `graphics::tests::clear_screen_does_not_drop_pending_images`
- [ ] `graphics::tests::da1_advertises_sixel`
- [ ] `graphics::tests::dimensions_clamp_at_4096`

New behaviour:

- [ ] `graphics::tests::one_extras_entry_per_image` — R-21; a 400 x 200-cell image grows the
  extras table by exactly one.
- [ ] `graphics::tests::placement_moves_with_an_in_region_scroll` — R-02; an image inside a
  `DECSTBM` region followed by `SU` still paints over its content.
- [ ] `graphics::tests::release_event_fires_on_alt_screen_clear` — R-22, the case the per-cell
  counter missed. `ED 2` on the **alternate** screen is a row reset and releases.
- [ ] `graphics::tests::primary_clear_screen_keeps_the_image_in_history` — R-13; the placement
  survives, the viewport has no graphic cells, and no `GraphicReleased` fires.
- [ ] `graphics::tests::overwriting_every_covered_cell_does_not_release` and
  `graphics::tests::el2_over_the_covered_rows_does_not_release` — the declared false positives.
- [ ] `graphics::tests::placements_are_bounded_and_evict_oldest_first` — `MAX_PLACEMENTS`.
- [ ] `graphics::tests::a_resize_release_is_delivered_by_the_next_feed` — including `feed(&[])`.
- [ ] `graphics::tests::release_event_fires_on_row_reset_and_scroll_blank` — R-22.
- [ ] `graphics::tests::release_event_fires_when_the_anchor_row_is_trimmed`
- [ ] `graphics::tests::release_event_fires_when_reflow_drops_the_anchor`
- [ ] `graphics::tests::painter_offset_is_derived_from_the_placement` — the arithmetic that
  replaces the per-cell offset.
- [ ] `graphics::tests::payload_byte_cap_aborts_an_endless_sixel`
- [ ] `graphics::tests::aborted_dcs_stamps_no_cells`
- [ ] `graphics::tests::images_survive_a_frame_skipped_by_mode_2026`
- [ ] `graphics::tests::integrity_rejects_a_dangling_graphic_ref` — the debug invariant.
- [ ] `graphics::tests::corrupt_band_does_not_panic` — the ConPTY byte-loss shape.

Fuzz: `cargo fuzz run sixel -- -rss_limit_mb=512`, corpus seeded from IN-0028's evidence
generator (`docs/spec-intakes/IN-0028-sixel-graphics/evidence/make_sixel.py`) plus truncated and
bit-flipped variants.

E2E: IN-0028's evidence walk
(`docs/spec-intakes/IN-0028-sixel-graphics/evidence/gui-walk.md`) reproduced on Windows with fresh
screenshots, including the cursor-position and `cls` cases that drove its acceptance rework —
**at `US-0081`, not at `US-0080` (N-02)**, because the new engine is not behind the application
until the shim lands. `US-0080` exits on `graphics::tests::*` plus `vt-diff` over the Sixel
recordings, which are both engine-level and provable in their own packet.
