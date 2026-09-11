# DEC-0012 Terminal graphics are cell-anchored inside the vendored `Term`

Date: 2026-09-11

## Status

accepted

## Context

IN-0028 adds Sixel images. An image must move with the text it was printed next to
(scrollback, scroll regions, alternate screen, resize), disappear when the screen or its
cells are erased, and advance the cursor by its height. Two models exist: an overlay list of
placements the embedder keeps and repositions on every scroll event (the Kitty protocol's
placement model), or references stored in the grid cells so the existing grid machinery
carries them (xterm's Sixel model). The Kitty protocol is a later intake and will need to
fit whichever model is chosen now.

## Decision

- **Images are decoded and placed inside the vendored `alacritty_terminal::Term`**, under the
  same lock and in the same byte order as text, through `Handler` DCS hooks added to the
  vendored `vte`. The embedder never re-parses the stream.
- **Placement is a per-cell reference** `GraphicCell { id, col, row }` in `CellExtra`. Cells
  carry their image fragment through scroll, reflow, erase and alternate-screen swaps; no
  separate placement list is maintained.
- **Pixel data leaves the `Term` at the next snapshot** (`take_graphics`) and lives in the view's
  bounded store; a reference whose image was evicted paints nothing.
- **Painting anchors the whole image at its first visible cell** (origin = cell origin minus the
  fragment offset), clipped by the grid bounds. Per-fragment painting is the upgrade path
  when text-over-image or partial erase must be exact.
- **Future protocols (Kitty, iTerm2) reuse this model**: their placements are converted to
  cell references when the image is placed; z-index below text or virtual placements, if ever
  needed, extend `GraphicCell`, not a parallel structure.

## Alternatives

- [x] Selected approach described above.
- [ ] Overlay placements kept by the embedder, repositioned from scroll/clear events: every
  grid operation (scroll region, reflow on resize, `ED`, `IL`/`DL`, alternate screen) would
  need an event and matching bookkeeping; the vendored `Term` already performs all of them
  on cells.
- [ ] Decode in the embedder from a raw-bytes event: the cursor must move by the image
  height inside the byte stream, before the next bytes are parsed, which only the `Term`
  can do consistently.
- [ ] Depend on a Sixel crate: none is maintained for `no_std`-style streaming use and the
  grammar is a few hundred lines; a dependency inside the vendored fork would also need
  patching into its manifest.

## Consequences

- [ ] Benefit to confirm: images survive scrolling, `clear`, and resize with no code in the
  view beyond painting.
- [ ] Tradeoff: text typed over an image is hidden by it and a partially erased image is
  still painted whole until per-fragment painting lands.
- [ ] Follow-up: the Kitty protocol needs APC dispatch in `vte` and a chunked-upload command
  set; it maps onto `GraphicCell` for placement.
