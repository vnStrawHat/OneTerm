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

Pixels are measured in a virtual cell of 10x20, published as `VIRTUAL_CELL`.
An image is placed at the cursor in the DEC scrolling style: every covered cell
takes the image's id, the cursor walks down `bands * 6 / 20` rows through the
ordinary line feed -- so the scroll region, the scrollback and the damage all
behave exactly as they do for text -- and the cursor keeps its column. Image rows
below the final cursor row are placed without further scrolling and are clipped
at the bottom of the screen.

The 10x20 cell and the `bands * 6 / 20` rule are an agreement with the Windows
console host, and they are why a prompt lands below an image rather than inside
it when that host issues its absolute cursor move. They are not tunable, and
changing them needs a fresh capture from a real console rather than an argument
from first principles.

`SnapshotPlacement::pixel_size` is the image's true size in pixels, so a
renderer that knows its own cell metrics can scale rather than assume the
virtual cell. Tell the engine your real cell size with
`Terminal::set_cell_pixels` whenever the font changes; that is the number
`CSI 14 t` reports to the program.

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
