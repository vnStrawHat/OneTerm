# Low-Level Design: Render pipeline (frame → row plans → GPUI primitives)

Intake: IN-0018
HLD: ../high-level-design.md
Topic: render-pipeline
Date: 2026-09-08

> One concern per file. Keep this focused on implementation-level mechanics for a single area of the HLD so it stays reviewable. Do not restate the whole intake here.

## Concern

`src/render/{frame, metrics, glyphs, row_plan, plan_cache, state, element, cursor, overlay,
diagnostics}.rs`: how one `TerminalSession` snapshot becomes cached per-row plans and how those
plans are painted through GPUI 0.3.3 inside the phase rules, with zero steady-state allocation.

## Design

### Frame model (`frame.rs`, the only alacritty-typed file)

```rust
pub(crate) struct Frame { content: TerminalContent, size: GridSize }   // `content` is private
#[derive(Clone, Copy, PartialEq, Eq)] pub(crate) struct GridSize { pub rows: u16, pub cols: u16 }
pub(crate) enum Damage<'a> { Full, Rows(&'a [usize]) }                  // display rows

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Color {
    Foreground, Background, Cursor, BrightForeground, DimForeground,
    Ansi(u8),        // 0..16 (NamedColor::Black..BrightWhite)
    DimAnsi(u8),     // 0..8  (NamedColor::DimBlack..DimWhite)
    Indexed(u8),     // 0..=255
    Rgb(u8, u8, u8),
}
impl Color { pub fn is_app_chosen_exact(self) -> bool /* Rgb | Indexed(n >= 16) */ }

#[derive(Clone, Copy, PartialEq, Eq, Default)] pub(crate) struct CellFlags(u16);
impl CellFlags { const INVERSE, BOLD, ITALIC, DIM, HIDDEN, UNDERLINE /* any underline kind */,
                 UNDERCURL, STRIKEOUT, WIDE_CHAR, WIDE_CHAR_SPACER, LEADING_WIDE_CHAR_SPACER, WRAPLINE;
                 fn contains(self, f: Self) -> bool }

pub(crate) struct Cell<'a> {
    pub ch: char, pub fg: Color, pub bg: Color, pub flags: CellFlags,
    pub zerowidth: &'a [char],       // combining marks / ZWJ sequences
    pub hyperlink: Option<u64>,      // FNV of the OSC 8 id+uri, for hashing and span detection
}
impl Cell<'_> { pub fn is_blank(&self) -> bool /* ' ' or '\0', no zerowidth, default bg, no INVERSE/UNDERLINE/STRIKEOUT */ }

pub(crate) struct FrameRow<'a> { cells: &'a [IndexedCell], row: usize }
impl<'a> FrameRow<'a> {
    pub fn len(&self) -> usize;
    pub fn cell(&self, col: usize) -> Cell<'a>;
    pub fn cells(&self) -> impl Iterator<Item = Cell<'a>> + '_;
    pub fn wraps(&self) -> bool;                                  // WRAPLINE on the last cell
    pub fn hash(&self) -> u64;                                    // FNV-1a over ch, fg, bg, flags, zerowidth, hyperlink
    pub fn hyperlink_uri(&self, col: usize) -> Option<String>;    // rare path (click/hover)
    pub fn text_into(&self, text: &mut String, char_cols: &mut Vec<u16>, char_wide: &mut Vec<bool>);
}

pub(crate) enum CursorShape { Block, Beam, Underline, HollowBlock, Hidden }
pub(crate) struct Cursor { pub row: i32 /* display row */, pub col: u16, pub shape: CursorShape }
pub(crate) struct GridPoint { pub row: i32, pub col: u16 }
pub(crate) struct Selection { pub start: GridPoint, pub end: GridPoint /* inclusive */, pub block: bool }

impl Frame {
    pub fn new() -> Self;
    pub fn snapshot(&mut self, session: &dyn TerminalSession);      // snapshot_into; the frame's only call
    pub fn size(&self) -> GridSize;  pub fn display_offset(&self) -> usize;  pub fn total_lines(&self) -> usize;
    pub fn damage(&self) -> Damage<'_>;  pub fn row(&self, r: usize) -> FrameRow<'_>;
    pub fn cursor(&self) -> Cursor;  pub fn selection(&self) -> Option<Selection>;
    pub fn app_cursor(&self) -> bool;  pub fn alt_screen(&self) -> bool;
}
```

Row `r` is `cells[r * cols .. (r + 1) * cols]` (the snapshot is dense, display order;
`debug_assert_eq!(cells.len(), rows * cols)`; if the assertion would fail in release, `row()`
falls back to a linear scan by `point.line`). Conversions: `NamedColor → Color`,
`vte Flags → CellFlags` (bit-by-bit, `ALL_UNDERLINES → UNDERLINE`), `vte CursorShape →
CursorShape`, `SelectionRange { start, end, is_block } → Selection` with
`row = line.0 + display_offset as i32`. `cursor.row` is `point.line.0 + display_offset`; it may
fall outside `0..rows` when the user scrolled — callers treat that as "no cursor".

### Metrics (`metrics.rs`) — the hit-test contract

```rust
pub(crate) struct CellMetrics {
    pub cell_width: Pixels, pub line_height: Pixels, pub baseline: Pixels, pub x_height: Pixels,
    pub device: CellSizeDevicePx, pub scale_factor: f32,
}
pub(crate) struct GridGeometry {
    pub bounds: Bounds<Pixels>,     // element bounds
    pub origin: Point<Pixels>,      // top-left of cell (0, 0): bounds.origin + (gutter_width + pad_left, pad_top), pixel-snapped
    pub metrics: CellMetrics, pub size: GridSize, pub gutter_width: Pixels, pub padding: Edges<Pixels>,
}
impl GridGeometry {
    pub fn cell_origin(&self, row: usize, col: usize) -> Point<Pixels>;       // origin + col*cell_width, row*line_height
    pub fn pixel_to_grid(&self, p: Point<Pixels>) -> Option<(f32, f32)>;     // fractional (row, col); None outside the grid
    pub fn cell_at(&self, p: Point<Pixels>) -> Option<(usize, usize)>;
}
pub(crate) fn measure(font: &Font, font_size: Pixels, line_height_factor: f32,
                      cell_width_override: Option<f32>, window: &Window, cx: &App) -> CellMetrics;
pub(crate) fn grid_size_for(bounds: Size<Pixels>, gutter: Pixels, padding: Edges<Pixels>,
                            metrics: &CellMetrics) -> GridSize;   // floor at device px, ≥ 1×1
```

`measure` snaps `ch_advance` (fallback `advance('m')`, then 8 px) and `max(font_size * factor,
ascent + descent)` to whole device pixels: `device = round(logical * scale)`, `logical =
device / scale`. `grid_size_for` mirrors the old `grid_size_for` arithmetic (subtract gutter and
padding, floor at device pixels, min 1).

### Row plan (`row_plan.rs`)

```rust
pub(crate) struct RowPlan {
    pub hash: u64,                       // 0 = never built
    pub bg: Vec<BgSpan>,                 // { col: u16, cols: u16, color: Hsla }  non-default bg, merged
    pub text: Vec<TextRunPlan>,          // { col: u16, cols: u16, line: ShapedLine, colors: Vec<ColorSpan> }
    pub shapes: Vec<ShapeQuad>,          // { rect: DeviceRect /* x from grid left, y from row top */, color: Hsla }
    pub paths: Vec<ShapePathPlan>,       // { col: u16, color: Hsla, path: ShapePath }
    pub decorations: Vec<DecorationSpan>,// { col, cols, kind: Underline { wavy } | Strikethrough, color }
}
pub(crate) struct ColorSpan { pub byte_end: u32, pub color: Hsla }   // colors along the run text
```

`build_row_plan(row: FrameRow, ctx: &PlanContext, scratch: &mut Scratch, glyphs: &mut GlyphCache,
plan: &mut RowPlan)` (every `Vec` is `clear()`ed, never replaced):

1. **Classes.** If semantic highlighting is enabled: `row.text_into(scratch.line_text,
   char_cols, char_wide)`, `overlay.scan_into(line_text, row_index, &mut scratch.class_chars)`,
   flatten to `scratch.class[col]` (wide chars write both columns). Then overlay the URL mask:
   `mask[row][col] → Class::Url`. Disabled → `class` all `Default`.
2. **Cells.** For `col in 0..cols`:
   - `cell = row.cell(col)`; spacer cells (`WIDE_CHAR_SPACER`, `LEADING_WIDE_CHAR_SPACER`) only
     contribute bg.
   - Style: `(fg, bg) = (cell.fg, cell.bg)`; `INVERSE` → swap and `force_bg = true`; resolve to
     `Hsla` through `theme.color(c)`; class style `cs = theme.class_styles.style(class[col])`:
     if `cs.fg` and (`cell.fg == Foreground` or `cs.override_ansi`) → fg = cs.fg; `DIM` → fg
     alpha × 0.7; unless `cell.fg.is_app_chosen_exact()` or `is_decorative_character(ch)` →
     `fg = theme.ensure_contrast(fg, bg)`; weight = `BOLD || cs.font.bold`, italic = `ITALIC ||
     cs.font.italic`.
   - **bg span**: if `bg != default background || force_bg`: extend the last span when adjacent
     and same color, else push.
   - **shape**: if `is_shape_char(ch)`: `shape_quads(ch, device, &mut scratch.rects)`, offset
     by `col * device.w`, color fg, then **coalesce** (below); `shape_paths` → `paths`. Ends the
     current text run.
   - **hidden**: `HIDDEN` cells end the run and emit no glyph (deviation 4).
   - **text run**: a run is a maximal span of consecutive non-blank, non-shape, non-hidden cells
     with the same `(weight, italic)`; a `WIDE_CHAR` cell is always its own run of `cols = 2`
     (shaped with `force_width: None`); zero-width chars append to the run text; a blank cell
     directly after a cell with zero-width chars is the overflow slot (no run of its own).
     Color spans record `(byte_end, fg)` transitions inside the run.
   - **decorations**: `UNDERLINE || hyperlink.is_some()` → `Underline { wavy: UNDERCURL }`;
     else `cs.deco == Underline` → plain underline; `STRIKEOUT` → strikethrough; adjacent spans
     of the same kind and color merge.
3. **Shape.** Each run: `glyphs.shape(text, font_key, force_width: Some(cell_width) | None,
   window)` → `ShapedLine`.
4. `plan.hash = row.hash()`.

**Shape coalescing.** Rects of a cell are compared with the rects of the previous cell that touch
its right edge (`scratch.open: SmallVec<[usize; 8]>` of indices into `plan.shapes`): a new rect
with equal `y`, `h`, color and `x == prev.x + prev.w` extends `prev.w` instead of being pushed.
A run of 80 `█` cells or 80 `─` cells becomes one quad; a `▀▄` alternation stays two quads per
cell (different `y`).

### Plan cache (`plan_cache.rs`)

```rust
pub(crate) struct StyleKey { font_family: u32 /* interned */, font_size_bits: u32, weight_bits: u32,
    features_hash: u64, palette_hash: u64, min_contrast_bits: u32, semantic_enabled: bool,
    shell_profile: u8, show_gutter: bool }
pub(crate) struct PlanCache { rows: Vec<RowPlan>, candidate: Vec<bool>, style: Option<StyleKey>,
    grid: Option<GridSize>, display_offset: usize, mask_prev: Vec<Vec<bool>>, mask_cur: Vec<Vec<bool>> }
```

```text
update(frame, style_key, ctx, scratch, glyphs, stats):
  rows = frame.size().rows
  full = grid != frame.size() || style != style_key
  if full: rows.resize(rows, RowPlan::default()); every plan.hash = 0
  else:
    d = frame.display_offset() - display_offset
    if d != 0:
      if |d| >= rows: every plan.hash = 0
      else if d > 0: rows.rotate_right(d); plans[0..d].hash = 0          // scrolled into history: content moves down
      else:          rows.rotate_left(-d); plans[rows+d..rows].hash = 0
  candidate.fill(false)
  match frame.damage(): Full => candidate.fill(true); Rows(list) => for r in list { candidate[r] = true }
  if let cursor row in 0..rows: candidate[row] = true
  for r: if plans[r].hash == 0: candidate[r] = true
  any_dirty = candidate.any()
  if any_dirty: url_masks_into(frame, &mut mask_cur); for r: if mask_cur[r] != mask_prev[r] { candidate[r] = true }
  for r where candidate[r]:
    h = frame.row(r).hash()
    if h != plans[r].hash: build_row_plan(frame.row(r), ctx, scratch, glyphs, &mut plans[r]); stats.rows_planned += 1
  if any_dirty: swap(mask_prev, mask_cur)
  grid = frame.size(); style = style_key; display_offset = frame.display_offset()
  stats.rows_candidate = count(candidate)
```

Selection, hover, search, blink, and focus never enter `update`. A style-key change or a grid
change zeroes every hash, so rows rebuild without hashing.

### Glyph cache (`glyphs.rs`)

```rust
pub(crate) struct FontKey { family: u32, size_bits: u32, weight_bits: u32, italic: bool }
struct RunKey { text_hash: u64 /* FNV-1a over UTF-8 bytes */, len: u32, font: FontKey, forced: bool }
pub(crate) struct GlyphCache { map: HashMap<RunKey, Entry { line: ShapedLine, used: u32 }>, generation: u32, capacity: usize /* 4096 */ }
impl GlyphCache {
    pub fn begin_frame(&mut self)                                  // generation += 1
    pub fn shape(&mut self, text: &str, font: &Font, key: FontKey, font_size: Pixels,
                 force_width: Option<Pixels>, window: &Window, stats: &mut FrameStats) -> ShapedLine
    // hit: mark used = generation, return clone (Arc). miss: window.text_system().shape_line_by_hash(
    //   text_hash, len, font_size, &[TextRun { len, font, color: black, .. }], force_width,
    //   || SharedString::from(text)) ; stats.shape_calls += 1 ; insert ; if len > capacity { retain used >= generation - 2 }
}
```

`TextRun.color` is irrelevant to shaping (GPUI keys by `FontRun`), so one entry serves every
color; colors are applied at paint from `ColorSpan`s. Gutter labels use the same cache with
`forced = false`.

### Element (`element.rs`)

```rust
pub(crate) struct TerminalElement { spec: TerminalElementSpec }        // see HLD
impl Element for TerminalElement {
    type RequestLayoutState = (); type PrepaintState = PrepaintState;
    fn id(&self) -> Option<ElementId> { Some(spec.id.clone()) }
    fn request_layout(..) -> (LayoutId, ()) { window.request_layout(Style { size: Size::full(), .. }, [], cx) }
    fn prepaint(.., bounds, ..) -> PrepaintState { .. }
    fn paint(.., bounds, .., prepaint, ..) { .. }
}
pub(crate) struct PrepaintState { hitbox: Hitbox, cursor: Option<CursorPaint>, visible_rows: Range<usize>, ime: Option<Box<dyn FnOnce(..)>> }
```

| Step | Phase | GPUI call (legality) |
| --- | --- | --- |
| `geometry = GridGeometry::new(bounds, inputs)`; `grid_size_for` | prepaint | none |
| if `geometry.size != state.last_grid` → `session.update(cx, s.resize(rows, cols))` (log on error) | prepaint | entity update of the session (not the view) is legal |
| `frame.snapshot(session.read(cx))`; `stats.snapshot_calls += 1` | prepaint | none |
| `glyphs.begin_frame(); plans.update(..)` | prepaint | `shape_line_by_hash` (legal in prepaint) |
| overlays: `overlay::selection_rects`, `overlay::search_rects` into `state.overlays` | prepaint | none |
| `cursor::resolve(frame, inputs, geometry, glyphs)` | prepaint | may shape one glyph (cached) |
| gutter labels for `visible_rows` into `state.gutter` (text + `ShapedLine` via cache) | prepaint | shaping |
| `hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal)` | prepaint | prepaint-only |
| `window.paint_layer(bounds, ..)` grid pass | paint | paint-only primitives |
| `window.paint_layer(cursor_bounds, ..)` cursor pass (only when `cursor.is_some()`) | paint | |
| `window.set_cursor_style(IBeam | PointingHand when inputs.url_hovering, &hitbox)` | paint | paint-only |
| `if let Some(ime) = prepaint.ime.take() { ime(bounds, window, cx) }` → `window.handle_input(&focus, ElementInputHandler::new(bounds, view), cx)` inside the closure | paint | paint-only |
| `state.geometry = Some(geometry)`; stats/latency; throttled log | paint end | none |

Grid pass, per row `r` in `visible_rows` (`row_top = origin.y + r * line_height`, quads via
`Bounds::from_corners(cell_origin(c0, r), cell_origin(c1, r + 1))` or, for device rects,
`origin + px(rect.x / scale)` … `px((rect.x + rect.w) / scale)`):

1. bg spans → `paint_quad(fill(..))`
2. shape quads → `paint_quad(fill(..))`
3. search rects → `fill(theme.search_match | search_active)`; selection rects →
   `fill(theme.selection)` (translucent, insertion order keeps them above bg and shapes)
4. paths → `PathBuilder::fill()`/`stroke(px(width / scale))` with points converted to logical,
   `paint_path(path, color)`
5. decorations → `paint_underline(point(x0, row_top + baseline + px(1)), width, &UnderlineStyle
   { thickness: px(1), color: Some(color), wavy })`, `paint_strikethrough(point(x0, row_top +
   baseline - x_height / 2), width, &StrikethroughStyle { thickness: px(1), color })`
6. text runs → for each `ShapedRun`/`ShapedGlyph`: color from the `ColorSpan` cursor by
   `glyph.index`; `paint_glyph(point(run_x + g.position.x, row_top + baseline), run.font_id, g.id,
   font_size, color)` or `paint_emoji(..)` when `g.is_emoji`; `Err` is counted, not propagated
7. gutter: `theme.gutter_bg` quad over the gutter column once per frame; label glyphs with
   `clock_fg` for the first 11 bytes and `line_number_fg` after

Cursor pass (`cursor.rs`):

```rust
pub(crate) struct CursorPaint { bounds: Bounds<Pixels>, color: Hsla, shape: CursorShape, hollow: bool,
                                glyph: Option<(ShapedLine, Hsla /* cell bg */)> }
resolve: row = frame.cursor().row in 0..rows else None; shape = if snapshot Hidden { Hidden } else { config override or snapshot };
         Hidden → None; should_paint = !focused || blink_visible else None;
         color = inputs.cursor_color.unwrap_or(theme.color(Color::Cursor)); hollow = !focused || HollowBlock
         bounds: Block/HollowBlock = cell; Beam = width max(1 dev px, 20 % cell); Underline = height max(2 dev px, 15 % line), bottom-aligned
         glyph = (Block && !hollow && cell non-blank) → shape the cell text via GlyphCache, bg = theme.color(cell.bg after inverse)
paint:  hollow Block → four 1-device-px edge quads; else one quad; then glyph re-painted in cell-bg color over the block
```

### Overlays (`overlay.rs`)

- `selection_rects(sel: Selection, size, out: &mut Vec<RowSpan { row, col, cols }>)`: clamp rows
  to `0..rows`; block → same `[start.col, end.col]` on every row; linear → single row, or first
  row to EOL, full middle rows, last row from column 0 to `end.col`.
- `search_rects(highlights: &[SearchHighlight], size, out)`: already display-relative and
  viewport-clamped by the view; copied through unchanged with the `active` flag.
- `url_masks_into(frame, masks: &mut Vec<Vec<bool>>)`: three-pass algorithm from `url/mask.rs`
  ported to `FrameRow` (mark hyperlink cells + scheme matches; extend across `wraps()` rows until
  whitespace; strip trailing punctuation after extension), reusing the inner vectors.

### Diagnostics (`diagnostics.rs`, `cfg(any(test, feature = "terminal-diagnostics"))`)

```rust
#[derive(Default, Clone, Copy)] pub(crate) struct FrameStats {
    pub snapshot_calls: u32, pub rows_total: u32, pub rows_candidate: u32, pub rows_planned: u32,
    pub shape_calls: u32, pub glyph_hits: u32, pub url_scans: u32, pub quads: u32, pub paths: u32,
    pub glyphs: u32, pub layers: u32, pub prepaint_us: u32, pub paint_us: u32,
}
pub(crate) struct LatencySamples { samples: VecDeque<u32> /* 512 */ }  // p95/p99
```

The element resets `FrameStats` at prepaint start and, under the feature, emits one
`log::debug!` line at most every 5 s with the last frame's counters and p95/p99.

### Allocation-free steady state

Reused across frames: `Frame.content` buffers, every `RowPlan` vector, `PlanCache.candidate`,
mask double buffer, `Scratch { line_text: String, char_cols: Vec<u16>, char_wide: Vec<bool>,
class_chars: Vec<u8>, class: Vec<u8>, run_text: String, rects: Vec<DeviceRect>, open:
SmallVec<[usize; 8]>, label: String }`, `overlays.selection`, `overlays.search`, `gutter.labels`.
A `ShapedLine` clone is an `Arc` bump. Cache misses (new text) and grid growth allocate; that is
not steady state. `PrepaintState` holds only `Copy` data, the hitbox, and the optional IME
closure (allocated by the view once per frame only while focused — accepted, it is one `Box`).

## Interfaces

Summarized above; the crate-internal entry points are `Frame::snapshot`, `PlanCache::update`,
`build_row_plan`, `GlyphCache::shape`, `GridGeometry::{cell_origin, pixel_to_grid}`,
`cursor::resolve`, `overlay::{selection_rects, search_rects, url_masks_into}`, and
`TerminalElement::new(spec)`.

## Edge Cases and Failure Modes

- [ ] Cursor row outside the viewport (scrolled back) → no cursor, no glyph re-paint.
- [ ] Grid shrink: `plans.resize` drops trailing rows; grow: new rows have `hash = 0`.
- [ ] `Damage::Rows` lists a row ≥ `rows` (race with resize) → ignored.
- [ ] Wide char at the last column (`LEADING_WIDE_CHAR_SPACER`) → spacer contributes bg only.
- [ ] `paint_glyph` returns `Err` (missing glyph) → counted in stats, frame continues.
- [ ] `shape_line_by_hash` hit returns an empty `text`; nothing reads `ShapedLine.text`.
- [ ] Session `resize` error → `log::warn!`, `last_grid` still updated (no resize storm).
- [ ] `scale_factor < 1` is clamped to 1 in `measure`.
- [ ] Semantic scan on a row with only spacers → empty text, no scan call.

## Verification

`src/render/element_tests.rs` (`#[gpui::test]`, `FakeTerminalSession`, a headless window,
`RenderInputs` built directly) plus unit tests beside each module:

- [ ] `dirty_frame_plans_rows_and_shapes`: first frame `snapshot_calls == 1`, `rows_planned > 0`,
      `shape_calls > 0`, `quads > 0`, `layers == 1` (no cursor in the fake by default).
- [ ] `idle_frame_plans_nothing_but_paints`: second frame with no probe change: `rows_planned ==
      0`, `shape_calls == 0`, `url_scans == 0`, `quads > 0`.
- [ ] `idle_frame_allocates_nothing`: counting allocator around the second frame reports 0
      allocations from the crate's code path (GPUI scene allocations excluded by measuring around
      `PlanCache::update` + overlay computation only).
- [ ] `scroll_rotates_plans_and_replans_only_scrolled_in_rows`: probe scrolls by 3 →
      `rows_planned == 3` even with `Damage::Full`.
- [ ] `cursor_row_replans_on_undamaged_change`, `selection_change_does_not_replan`,
      `style_key_change_replans_all`, `resize_replans_all_and_resizes_session` (probe resize log).
- [ ] `block_run_coalesces_into_one_quad`: 40 `█` cells → one `ShapeQuad`; `─` likewise.
- [ ] `bg_spans_merge_adjacent_same_color`, `inverse_swaps_colors_and_forces_bg`,
      `dim_reduces_alpha`, `hidden_cells_have_no_text_run`, `undercurl_maps_to_wavy`,
      `class_underline_only_without_ansi_underline`, `url_mask_forces_url_class`.
- [ ] `wide_char_occupies_two_columns_one_run`, `zero_width_marks_join_base_run`,
      `overflow_slot_keeps_background`.
- [ ] `frame_selection_converts_to_display_rows`, `selection_block_and_linear_spans`.
- [ ] `grid_size_subtracts_gutter_and_padding`, `grid_size_rounds_at_device_pixels`,
      `grid_size_never_below_one_cell`, `metrics_snap_cell_to_device_pixels` (1.0/1.25/1.5/2.0).
- [ ] `glyph_cache_hits_across_rows`, `glyph_cache_evicts_stale_generation`.
- [ ] `cursor_override_respects_hidden`, `cursor_hollow_when_unfocused`,
      `cursor_glyph_repaint_only_for_filled_block`, `cursor_blink_gating`.
- [ ] `gutter_labels_use_fallbacks` (`[--:--:--]`, newest/oldest reuse).
