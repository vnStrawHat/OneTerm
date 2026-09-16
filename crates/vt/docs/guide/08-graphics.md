# 8. Images

A program can print a picture with a Sixel DCS sequence. The engine decodes it
to RGBA, anchors it to the cells it covers, moves it with those cells, and tells
you when it dies. It does not draw it -- you upload the pixels to whatever your
renderer calls a texture and blit them.

Nothing in the module assumes Sixel: `GraphicData` is format-agnostic RGBA and
the Sixel decoder is one producer of it.

## The lifecycle

```text
  DCS ... q <sixel> ST
        |
        v  decoded during feed
  GraphicData { id, width, height, rgba }   <- Terminal::take_graphics()
        |
        v  placed at the cursor, covering cols x rows cells
  every covered cell stores the id, and only the id
        |
        v  the anchor moves with the content through IL, DL, SU, SD, reflow
  SnapshotPlacement { id, row, col, cols, rows, pixel_size }
        |
        v  the last referencing cell is gone
  VtEvent::GraphicReleased(id)              <- drop your texture
```

Three rules carry the whole design, and each of them exists because the obvious
alternative breaks at scale.

**A cell stores which image, never where inside it.** All the covered cells share
one interned entry. A 4096x4096 image covers about 84 000 cells, which is more
than the whole id space a per-cell offset would need, so the painter derives its
offset from the placement instead. `SnapshotState::graphic_offset(id, row, col)`
does that arithmetic for you and returns `None` when the cell is outside the
placement -- which an `IL` or an `SD` that splits an image can produce.

**A placement is a tracked anchor.** It moves with its content through line
insertion, line deletion, region scrolls and reflow, by the same mechanism that
moves the cursor and the selection. There is no separate image-fixup pass to get
wrong.

**Liveness is derived from rows, not from a counter.** A counter decremented by
the cell writer would miss `Row::reset`, scroll blanking, the alternate-screen
wipe and reflow -- that is, `CSI 2 J`, `clear` and every full-screen TUI
repaint, which is the common case. Instead a sweep at the end of each `feed`
asks which placements still have a live cell, and the ones that do not are
released.

## What the embedder does

```rust
use std::time::Instant;
use oneterm_vt::{Config, EventBatch, Size, SnapshotState, Terminal, VtEvent};

let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
let mut batch = EventBatch::new();
let mut state = SnapshotState::new();

// A one-pixel Sixel: DCS q, one band, ST.
term.feed(b"\x1bPq#0;2;100;0;0#0~\x1b\\", &mut batch, Instant::now());

// The one drain. Upload these to your renderer, keyed by `id`.
for image in term.take_graphics() {
    assert_eq!(image.rgba.len(), (image.width * image.height * 4) as usize);
}

// Where to draw them: refreshed on every update, including a suppressed one.
term.snapshot_update(&mut state, Instant::now());
for placement in state.placements() {
    let _ = (placement.id, placement.row, placement.col, placement.rows, placement.cols);
    let _ = placement.pixel_size;
}

// When to drop them.
for event in batch.iter() {
    if let VtEvent::GraphicReleased(id) = event {
        let _ = id;
    }
}
```

`take_graphics` is the **only** drain, and it is on `Terminal` rather than on a
snapshot on purpose: more than one consumer may hold its own `SnapshotState`, so
draining inside `snapshot_update` would hand an image to whichever consumer
asked first and nothing to the rest. A paint skipped by synchronised output
therefore loses nothing -- the pixels wait until somebody takes them.

Placements are refreshed on every `snapshot_update`, even one that returns
`Unchanged`, because they are cheap to copy (usually there are none at all) and
a painter must never hold a placement whose anchor has moved under it.

## Geometry, and the conhost agreement

An image covers `ceil(width / cell_width)` x `ceil(height / cell_height)` cells,
against **your** cell -- the one you passed to `Terminal::set_cell_pixels`, which
is also the number `CSI 14 t` reports. So a program that sizes an image from that
reply gets the cells it asked for.

**If you never call `set_cell_pixels`, you get VT340 sizing**: the virtual cell of
10x20 published as `VIRTUAL_CELL`. That is a documented fallback, not a
degradation -- it is what a DEC terminal does and what Windows conhost agrees
with -- but it means an image lands at `real_cell / (10, 20)` of the size a
DPI-aware program intended. One call per font change is the whole contract. Both
axes must be non-zero; a half-set `(9, 0)` falls back whole rather than dividing
by zero.

An image is placed at the cursor in the DEC scrolling style: every covered cell
takes the image's id, the cursor walks down `bands * 6 / cell_height` rows through
the ordinary line feed -- so the scroll region, the scrollback and the damage all
behave exactly as they do for text -- and the cursor keeps its column. Image rows
below the final cursor row are placed without further scrolling and are clipped
at the bottom of the screen.

The footprint and the cursor walk divide by the same number, which is what keeps a
prompt off the body of an image when a console host issues its absolute cursor
move. At the fallback cell that walk is the classic `bands * 6 / 20`, the
agreement with the Windows console host: not tunable, and changing it needs a
fresh capture from a real console rather than an argument from first principles.

**Where the cursor actually stops.** `bands * 6` is the height of the bands *above*
the last one, so with a payload that does not end in a graphics newline the cursor
lands on the image's **last row**, not past it, keeping its column; the shell's own
newline then carries the prompt clear. Real encoders (`libsixel`, `img2sixel`) do
end the payload with `-`, which makes `bands * 6` the full height and puts the
cursor genuinely below the image whenever your cell height divides it -- at 9x18 and
18x36 it does; at the fallback 20 px cell a 576 px image gives `576 / 20 = 28` against
29 rows, and the cursor is still on the last row. This is the DEC and conhost rule.
It is **not** xterm's: xterm moves to the left margin of the line below the image,
and carries a `sixelScrollsRight` resource to opt out of that -- stated here for
comparison only, since this repository holds no xterm capture to pin it against.

One more case the rule does not cover: `"Pan;Pad;Ph;Pv` overrides the measured size
while the cursor keeps following the bands. Declare a height the data does not fill
and the cursor stops high inside the image; declare one smaller than the data and it
walks past the bottom. Emit raster attributes that match what you draw.

`SnapshotPlacement::pixel_size` is the image's true size in pixels. Draw it at
exactly that size -- one image pixel to one device pixel -- anchored at the
placement's top-left cell, and clip it to `cols` x `rows`. Do not stretch it to
the footprint: the engine derived that footprint from these same pixels and your
cell size, so the two already agree to within the `ceil`. The clip earns its keep
when they do not -- on the fallback cell, and when your font size or DPI scale
changes after an image was placed. The footprint stays in cells across such a
change and the engine does not re-place; the picture is cropped or gains margin
instead.

## Ceilings

An image is clamped to 4096 pixels on each axis, and pixels past that are
dropped rather than allocated. The payload ceiling underneath -- 16 MiB per DCS
sequence -- is the binding one: it is not possible to reach the pixel clamp
through a legal sequence. A DCS abandoned by `CAN` or `SUB`, or one that runs
past the payload ceiling, discards any partial image and counts in
`FeedStats::aborted_dcs`.

The number of live placements is bounded too, because a hostile stream can emit
one image per row of a million-row scrollback and the release sweep is linear in
live placements. The oldest are released when the bound is reached, which reaches
you as the ordinary `GraphicReleased` event.

## On Windows, check your console host

The engine decodes what it is fed, but on Windows the console host between your
shell and the engine may never forward a Sixel payload at all: the inbox
`conhost.exe` swallows DCS. If you want images in a local shell, ship a matched
console-host pair beside your own executable. Chapter 13 says which files and
why only your build can place them.
