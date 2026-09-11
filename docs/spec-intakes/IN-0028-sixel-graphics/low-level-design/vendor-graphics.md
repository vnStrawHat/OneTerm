# Low-Level Design: Vendored graphics patch (Sixel decoder, placement, DA1)

Intake: IN-0028
HLD: ../high-level-design.md
Topic: vendor-graphics
Date: 2026-09-11

> One concern per file. Keep this focused on implementation-level mechanics for a single area of the HLD so it stays reviewable. Do not restate the whole intake here.

## Concern

The two vendor patches (`vendor/patches/vte/0002-*.patch`,
`vendor/patches/alacritty_terminal/0003-*.patch`): the `Handler` DCS hooks, the Sixel decoder,
how an image becomes grid cells, and the DA1 answer. Everything here runs on the pump
thread under the `Term` lock.

## Design

### vte: DCS hooks (`src/ansi.rs`)

```rust
pub trait Handler {
    // OneTerm fork additions, default no-op.
    fn dcs_hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn dcs_put(&mut self, _byte: u8) {}
    fn dcs_unhook(&mut self) {}
}
```

`Performer::hook/put/unhook` call them instead of the `debug!` stubs. `Params` is
`crate::Params` (already imported for `csi_dispatch`).

### alacritty_terminal: `src/term/graphics.rs` (new file, `pub mod graphics;` in `term/mod.rs`)

```rust
pub struct GraphicId(pub u64);                       // Copy, Eq, Hash, Debug (+ serde)
pub struct GraphicCell { pub id: GraphicId, pub col: u16, pub row: u16 }   // in CellExtra
pub struct GraphicData { pub id: GraphicId, pub width: u32, pub height: u32, pub rgba: Vec<u8> }
pub(crate) struct Graphics { next_id: u64, pending: Vec<Arc<GraphicData>>, cell_size: (u16, u16) }
pub(crate) struct SixelParser { ... }
pub const MAX_DIMENSION: u32 = 4096;
```

`CellExtra` gains `graphic: Option<GraphicCell>`; `Cell::graphic()`, `Cell::set_graphic()`.
`set_underline_color`/`set_hyperlink` drop-`extra` conditions also test `graphic.is_none()`
so a graphic is never lost by a colour reset. `GridCell::is_empty` and `LineLength` are
unchanged: a graphic cell with a space still counts as empty text for reflow trimming, which
matches xterm (an image does not extend the line's text).

### Sixel grammar (`SixelParser`)

State: `palette: [u32; 256]` (RGBA, initialised to the VT340 16-colour default, rest black),
`color: usize`, `x`, `band` (row of the current 6-pixel band, `y = band * 6`), `width`,
`height` (current extents, grow as data arrives), `raster: Option<(w, h)>` from `"`, `repeat:
Option<u32>`, `arg: Vec<u32>` (numeric parameters being read), `state: Ground | Repeat |
Color | Raster`, `pixels: Vec<u32>` (RGBA per pixel, row-major, capacity grows in whole bands),
`transparent: bool` (P2 == 1).

Byte handling in `put`:

| Byte | Action |
| --- | --- |
| `0x3F..=0x7E` (`?`..`~`) | six pixels: bits `b0..b5` of `byte - 0x3F` set rows `y..y+6` at `x` (repeat `n` times, then `x += n`); a set bit writes `palette[color]`, a clear bit writes nothing (background stays `0` alpha when transparent, or the palette's colour 0 when opaque: opaque fill is applied in `finish` to every untouched pixel) |
| `!` | start `Repeat`; digits accumulate `n`; next data char uses it |
| `#` | start `Color`; `#n` selects register `n`; `#n;2;r;g;b` sets RGB in percent (0..100 scaled to 0..255); `#n;1;h;l;s` sets HLS (h 0..360, l/s 0..100, converted to RGB); `;` separates args, the next non-digit non-`;` byte ends the command and is reprocessed |
| `"` | start `Raster`: `"Pan;Pad;Ph;Pv`; `Ph`/`Pv` set `raster` (clamped); `Pan/Pad` ignored (aspect 1:1) |
| `$` | carriage return: `x = 0` |
| `-` | next band: `x = 0`, `band += 1` |
| anything else (space, CR, LF) | ignored |

Extents: `width = max(width, x)` after each data char, `height = max(height, y + 6)`; both
clamped to `MAX_DIMENSION`; a pixel outside the clamp is dropped. `finish()` returns
`Option<(width, height, rgba)>`: `None` when `width == 0 || height == 0`. Final size is
`raster` when given (data beyond it is clipped; missing area is background), else the
extents. Rows are padded/cropped to the final size when the buffer is materialised.

Storage: pixels are kept in a `Vec<u32>` sized to the *current* extents, reallocated on
growth (widths grow rarely, bands append). This keeps a well-formed image at one allocation
per band and bounds a hostile one by the clamp (64 MB RGBA max).

### Placement (`Term::dcs_unhook`)

```text
let (cw, ch) = self.graphics.cell_size;            // default (8, 16)
let cols = min(ceil(width / cw), columns - cursor.col)
let rows = ceil(height / ch)
for r in 0..rows:
    line = cursor.line
    for c in 0..cols: grid[line][cursor.col + c].set_graphic(GraphicCell { id, col: c, row: r })
    damage line (left = cursor.col, right = cursor.col + cols - 1)
    if r + 1 < rows: self.linefeed()               // scroll region + scrollback rules
self.linefeed(); self.carriage_return();           // cursor below the image, column 0
self.graphics.pending.push(Arc::new(GraphicData { .. }))
```

`linefeed` scrolls the grid when the cursor is at the bottom of the scroll region, so an
image taller than the screen leaves its top rows in scrollback, as xterm does with sixel
scrolling enabled (DECSDM off, the default). Placement never touches `cell.c`, `fg`, `bg` or
flags. A `GraphicCell` is overwritten together with `extra` when text is written into the
cell (`write_at_cursor` copies the template `extra`), erased by `Cell::reset`, and moves
with the row on scroll and resize.

`Term::take_graphics(&mut self) -> Vec<Arc<GraphicData>>` uses `mem::take`.
`Term::set_cell_size(w, h)` ignores zero. `reset_state` clears `pending`.

### DA1

`identify_terminal(None)` writes `"\x1b[?62;4c"`. Secondary DA unchanged.

## Tests (`crates/terminal/src/sixel_tests.rs`, through `Processor::advance` on `Term::new`)

- decoder: 2x2 image with two colours, repeat `!3`, HLS colour, raster attributes crop,
  clamp at `MAX_DIMENSION`, empty image places nothing.
- placement: cursor at (2, 3) with cell 8x16 and a 20x20 image -> cells (2,3..5) and
  (3,3..5) hold `(id, c, r)`, cursor ends at (4, 0); image at the last screen line scrolls
  one line into history; `CSI 2J` clears the refs; typing over a cell drops its ref;
  `take_graphics` returns the image once.
- DA1: `CSI c` yields a `PtyWrite("\x1b[?62;4c")` event.

## Patch generation

Per `vendor/README.md` § 4: temporary git repo from the pristine source, apply the series,
commit the change, `git format-patch --zero-commit --no-signature`, then
`bash vendor/refresh.sh --check`. The new files are `src/term/graphics.rs` and the edits in
`src/term/cell.rs`, `src/term/mod.rs` (alacritty) and `src/ansi.rs` (vte).
