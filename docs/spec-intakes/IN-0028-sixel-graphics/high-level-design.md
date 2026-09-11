# High-Level Design: Sixel graphics

Intake: IN-0028
Lane: normal
Date: 2026-09-11

## Idea

Sixel arrives as one DCS sequence. The vendored `vte` already parses DCS but drops it; a
patch forwards `hook`/`put`/`unhook` to the `Handler`. The vendored `Term` implements them:
`dcs_hook` with final byte `q` starts a `SixelParser`, `dcs_put` feeds bytes, `dcs_unhook`
finishes the image and places it the way xterm does: every cell the image covers, starting
at the cursor, gets a `GraphicCell { id, col, row }` in its `CellExtra`; the cursor moves down
one line per image row (`linefeed`, so the scroll region and scrollback apply) and ends on
the line below the image at column 0. The decoded RGBA image sits in `Term::graphics` until
the next snapshot takes it. Because the reference lives in the cells, scrolling, `clear`,
the alternate screen, overwriting text and resize all behave without extra code.

The view registers each new image once as a `gpui::RenderImage` in a bounded store keyed
by id. Per frame it walks the visible cells, finds the first cell of every image id, derives
the image origin from that cell's `(col, row)` offset, and paints the whole image once with
`paint_image`; the grid's content mask clips the parts that scrolled out.

## Diagram

```text
 PTY bytes ─▶ vte Processor ─▶ Performer::hook/put/unhook ─▶ Handler::dcs_hook/dcs_put/dcs_unhook (patch vte/0002)
                                                                    │
                                              Term<T> (patch alacritty_terminal/0003)
                                              ├─ dcs_hook('q', P1;P2;P3) ─▶ SixelParser::new
                                              ├─ dcs_put(b)              ─▶ parser.put(b)
                                              └─ dcs_unhook              ─▶ image = parser.finish()
                                                   place(image): cells[(cursor.line + r, cursor.col + c)].graphic = (id, c, r)
                                                                 linefeed() per row, carriage_return()
                                                                 graphics.pending.push(Arc<GraphicData>)
                                                                 damage rows
        snapshot (oneterm-terminal TerminalContent::refill, &mut Term)
              ├─ cells: IndexedCell { cell (extra.graphic) }
              └─ graphics: term.take_graphics()  ── new since last snapshot
        view (oneterm-terminal-view)
              ├─ Frame::graphics() ─▶ GraphicStore::insert(id, RenderImage)   (bounded, evicts oldest, drop_image)
              ├─ Cell.graphic: Option<GraphicRef { id, col, row }>
              └─ paint pass: first visible cell per id ─▶ origin = cell_origin - (col*cw, row*ch)
                                                      ─▶ window.paint_image(Bounds{origin, (w,h)}, ..)
        metrics change ─▶ session.set_cell_size(cw, ch) ─▶ Term::set_cell_size (image rows/cols)
```

## UI Wireframe

Terminal grid after `img2sixel cat.png` (cells marked; the image is 3 rows high):

```text
+----------------------------------------------+
| C:\> img2sixel cat.png                        |
| [########################]                    |  <- image row 0 (cells hold graphic refs)
| [########################]                    |  <- image row 1
| [########################]                    |  <- image row 2
| C:\> _                                        |  <- cursor: line below the image, column 0
+----------------------------------------------+
```

Scrolling two lines up moves the image with the text; the top band is clipped by the grid
bounds. `cls` erases the cells, so the image disappears.

## Data Flow

1. `vte` `Performer::hook(params, intermediates, ignore, action)` forwards to
   `Handler::dcs_hook`; `put(byte)` to `dcs_put`; `unhook()` to `dcs_unhook`. Defaults are
   no-ops, so other `Handler` implementors are unaffected.
2. `Term::dcs_hook` accepts `action == 'q'` only (Sixel). P1 (aspect, ignored), P2 (1 =
   transparent background, else opaque), P3 (ignored). A non-Sixel DCS leaves the parser
   `None` and `dcs_put` drops bytes.
3. `SixelParser` (LLD) decodes into `width x height` RGBA. Clamp: 4096 x 4096; bytes beyond
   are ignored. Missing raster attributes: size grows with the data.
4. `dcs_unhook`: `cols = ceil(width / cell_w)`, `rows = ceil(height / cell_h)` with the cell
   size from `Term::set_cell_size` (default 8 x 16 until the view pushes real metrics).
   Columns beyond the grid width are not placed (the image is clipped on the right). For each
   row: write `GraphicCell` into the covered cells (cell text and colours untouched), damage
   the line, `linefeed()` (scrolls when at the bottom of the scroll region). Finally
   `carriage_return()`. Cursor ends on the line after the image.
5. `Term::graphics.pending` receives `Arc<GraphicData { id, width, height, rgba }>`;
   `Term::take_graphics()` moves the vector out. `reset_state` (RIS) clears pending.
6. `TerminalContent::refill` calls `take_graphics()` after `renderable_content()` and stores
   the vector in `TerminalContent.graphics`. `IndexedCell.cell` already clones the
   `Arc<CellExtra>`, so the graphic reference reaches the view for free.
7. `TerminalInput::set_cell_size(width, height)` (default no-op) forwards to
   `TerminalModel::set_cell_size` -> `Term::set_cell_size`. The view calls it from `prepaint`
   whenever the metrics changed (same place as `resize`).
8. View `Frame`: `graphics()` returns the new images; `Cell` gains
   `graphic: Option<GraphicRef>`. `RenderState.graphics: GraphicStore` converts RGBA to BGRA
   (GPUI's `RenderImage` layout, as `img.rs` does), builds `RenderImage::new(vec![Frame::new(buf)])`,
   and keeps `Arc<RenderImage>` per id in insertion order. Cap: 64 images or 64 MB of pixels;
   eviction calls `window.drop_image` so the atlas tile is released.
9. Paint: after the background/shape pass and before decorations, `GridPainter::paint_graphics`
   scans visible rows for graphic cells, keeps the first hit per id (smallest row, then col),
   computes `origin = cell_origin(row, col) - (ref.col * cw, ref.row * ch)` and calls
   `paint_image(Bounds { origin, size: (w, h) }, Corners::default(), image, 0, false)`. Ids
   missing from the store (evicted) are skipped. Images are polychrome sprites, which GPUI
   draws after glyphs inside a layer; text typed over an image therefore sits under it, and
   selection quads under it too (documented limitation, see Risks).
10. DA1: `Term::identify_terminal(None)` answers `CSI ? 62 ; 4 c` (VT220 + Sixel) instead of
    `CSI ? 6 c` so `tmux`, `lsix`, `chafa` and `timg` detect Sixel.

## Limits and Risks

| Risk | Effect | Mitigation |
| --- | --- | --- |
| Huge or endless Sixel stream | memory / time under the `Term` lock | 4096 x 4096 clamp; bytes beyond ignored; decode is linear in bytes |
| Text written over an image | GPUI paints polychrome sprites after glyphs, so the image covers the text | accepted for v1 (Sixel programs do not overlay text); `ponytail:` note in the painter, upgrade: per-row image bands in a lower layer |
| Erased cells inside an image | whole image still painted from any surviving cell | accepted for v1; same upgrade path |
| Evicted image still referenced by cells | nothing painted for those cells | bounded store is the memory contract; scrollback images older than 64 images vanish |
| Cell size unknown at decode time | wrong row count | view pushes `set_cell_size` before the first byte can arrive (prepaint precedes the pump for a new session in practice); default 8 x 16 otherwise |
| DA1 change | programs treat OneTerm as VT220 | xterm/mintty/foot report 62/64 + 4 already; no known regression |
| Vendored patch drift | `refresh.sh --check` fails | patches regenerated with `git format-patch` per `vendor/README.md` § 4 |

## Detail Design

- [x] Detail design: `low-level-design/vendor-graphics.md` (decoder grammar, placement,
  patch layout).
- Reason: the vendored patch is the largest and least testable piece; its grammar and the
  cursor rules need a written spec before the patch is generated.
