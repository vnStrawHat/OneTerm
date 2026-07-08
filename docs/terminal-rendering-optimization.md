# Terminal Rendering Optimization — Technical Document

> A comprehensive technical document describing the methods, technologies, and techniques applied in OneTerm's `TerminalElement` to achieve terminal rendering quality close to **Windows Terminal AtlasEngine**.
>
> **Applied commits**:
> - `1aab15c` — snap cell metrics to device-pixel grid + custom box-drawing primitive
> - `cae796d` — extend custom renderer for block elements U+2580-U+259F
> - `e8269ba` — Windows Terminal-grade cell metrics + cursor shape override
>
> **Main implementation file**: `crates/ui/src/views/terminal/terminal_element.rs`  
> **Date applied**: 2026-06-22

---

## Overview

`TerminalElement` is a custom `gpui::Element` that renders the terminal grid from a `TerminalContent` snapshot. The initial problems:
- Cell metrics (width/height) were float logical px → subpixel jitter, blurry lines.
- Box-drawing / block elements used font glyphs → anti-alias blur, didn't fill the cell.
- Cell width auto-measured from `'m'` instead of `'0'` → cell wider than Windows Terminal.
- Line height factor could be smaller than `ascent + descent` → clipped glyphs.
- The shell `DECSCUSR` overrode the user-configured cursor shape.

The solution has three pillars:
1. **Device-pixel grid snapping** — snap every metric/coordinate to device pixels.
2. **Custom primitive renderer** — draw box-drawing/block via fill rects instead of font glyphs.
3. **Font-metrics-based cell metrics + cursor override** — `ch_advance('0')`, line height ≥ `ascent+descent`, config shape wins over shell.

---



## 1. Why snap?

`TerminalElement` paints a monospace grid: each cell has a fixed width/height, text is placed at `origin + (col * cell_width, line * line_height)`. If `cell_width`/`line_height` are float logical px (e.g. 9.0 px at scale 1.5 → 13.5 device px), then:

- Line characters (`─`, `│`, `┌` … U+2500–U+257F) fall between two device pixels → horizontal/vertical anti-aliasing → blur.
- A block cursor (`█`) 13.5 px wide → subpixel gap between the cursor and the neighboring cell.
- Text baselines differ between rows → glyph rasterization is off the pixel grid.
- Resizing the window changes cell metrics by a subpixel → the entire grid *jitters*.

**Windows Terminal** and **Zed terminal** solve this by snapping every coordinate/metric to an integer device-pixel grid. `OneTerm` applies the same approach at the GPUI logical-pixel layer — the result is glyphs rasterized on the pixel grid.

---

## 2. Snap formulas

GPUI uses logical px; the window provides `scale_factor` (1.0 = 96 dpi, 1.5/2.0 on HiDPI).

```rust
let scale_factor = window.scale_factor().max(1.0);

// Round to nearest device pixel — used for cell_width/line_height.
let snap_px = |value: f32| -> f32 { (value * scale_factor).round() / scale_factor };

// Floor — used for origins, start coordinates (align left/top).
let snap_px_floor = |value: f32| -> f32 { (value * scale_factor).floor() / scale_factor };

// Ceil — used for width/height that must fill the cell / meet the next cell.
let ceil_px = |value: f32| -> f32 { (value * scale_factor).ceil() / scale_factor };
```

- `value * scale_factor` converts logical px → device px.
- `round()` / `floor()` / `ceil()` snap to integer device px.
- `/ scale_factor` converts back to logical px (still on the device-pixel grid).

Example with `scale_factor = 1.5`:

| Logical | Device | Snap round | Snap floor | Snap ceil | Logical after snap |
|---|---|---|---|---|---|
| 9.0 | 13.5 | 14.0 | 13.0 | 14.0 | 9.333 (round/ceil) / 8.667 (floor) |
| 16.4 | 24.6 | 25.0 | 24.0 | 25.0 | 16.667 / 16.0 / 16.667 |

---

## 3. Application in `prepaint`

### 3.1. `cell_width` and `line_height`

```rust
let scale_factor = window.scale_factor().max(1.0);
let snap_px = |value: f32| -> f32 { (value * scale_factor).round() / scale_factor };

let font_id = cx.text_system().resolve_font(&self.font);
let font_px = self.font_size;

let cell_width = if let Some(cw) = self.cell_width_override {
    px(snap_px(cw))
} else {
    let raw = cx.text_system()
        .ch_advance(font_id, font_px)
        .map(|s| f32::from(s))
        .unwrap_or_else(|_| {
            // fallback 'm'
            cx.text_system()
                .advance(font_id, font_px, 'm')
                .map(|s| f32::from(s.width))
                .unwrap_or(8.0)
        });
    px(snap_px(raw))        // round to device pixel
};

let font_ascent = cx.text_system().ascent(font_id, font_px);
let font_descent = cx.text_system().descent(font_id, font_px);
let natural_line_height = f32::from(font_ascent) + f32::from(font_descent);
let factor_height = f32::from(font_px) * self.line_height_factor;
let line_height = px(snap_px(factor_height.max(natural_line_height)));
```

**Reason**: `cell_width` and `line_height` are the foundation of the entire grid. Rounding to device pixel ensures each row/column lands exactly on the device-pixel grid.

### 3.2. Compute `rows` / `cols` in device pixels

```rust
let grid_width = (f32::from(bounds.size.width)
    - f32::from(gutter_width)
    - f32::from(pad_left)
    - f32::from(pad_right))
    .max(f32::from(cell_width));

let grid_width_device = (grid_width * scale_factor).floor().max(1.0);
let cell_width_device = f32::from(cell_width) * scale_factor;
let cols = ((grid_width_device / cell_width_device).floor() as u16).max(1);

let avail_height = f32::from(bounds.size.height)
    - f32::from(pad_top)
    - f32::from(pad_bottom);
let avail_height_device = (avail_height * scale_factor).floor().max(0.0);
let line_height_device = f32::from(line_height) * scale_factor;
let rows = ((avail_height_device / line_height_device).floor() as u16).max(1);
```

**Reason**: computing the number of rows/columns in device-pixel space (integer) avoids rounding errors when dividing logical px. The resulting `rows`/`cols` is an integer number of cells that exactly fit the viewport.

### 3.3. Grid origin

```rust
let grid_origin = GpuiPoint {
    x: px(snap_px(f32::from(bounds.origin.x + gutter_width + pad_left))),
    y: px(snap_px(f32::from(bounds.origin.y + pad_top))),
};
```

The grid origin is the start point of the terminal region (right of the gutter). Snapping to device pixel ensures the first row and first column land on the grid — especially important when the gutter width is not a multiple of a device pixel.

### 3.4. Gutter entry Y

```rust
y: px(snap_px(f32::from(bounds.origin.y + i as f32 * line_height))),
```

Each gutter text row also snaps Y to the snapped `line_height`, avoiding baseline drift between rows.

---

## 4. Application in `paint`

In `paint`, use `floor` for the origin, `ceil` for the size, to fill the cell completely.

```rust
let scale_factor = window.scale_factor().max(1.0);
let snap_px = |value: f32| -> f32 { (value * scale_factor).floor() / scale_factor };
let ceil_px = |value: f32| -> f32 { (value * scale_factor).ceil() / scale_factor };
```

### 4.1. Background rects

```rust
for r in &layout.rects {
    let pos = point(
        px(snap_px(f32::from(origin.x + r.point.column as f32 * cw))),
        px(snap_px(f32::from(origin.y + r.point.line as f32 * lh))),
    );
    let sz = size(px(ceil_px(f32::from(cw * r.num_cells as f32))), lh);
    window.paint_quad(fill(Bounds::new(pos, sz), r.color));
}
```

- `pos`: floor to align left/top.
- `sz.width`: ceil so the background fills up to the right edge of the last cell, leaving no subpixel gap.

### 4.2. Selection rects

Same as background, using `floor` + `ceil`.

### 4.3. Text runs

```rust
// In BatchedTextRun::paint
let snap_px = |value: f32| -> f32 { (value * scale_factor).floor() / scale_factor };
let pos = point(
    px(snap_px(f32::from(origin.x + self.start.column as f32 * cell_w))),
    px(snap_px(f32::from(origin.y + self.start.line as f32 * line_h))),
);
```

Snap the text origin so glyphs rasterize on the pixel grid. GPUI `paint_line` then centers the text within `line_height` based on the shaped line's font metrics.

### 4.4. Box-drawing primitives

```rust
let cw_d = (f32::from(cw) * scale_factor).round() as i32;   // device px
let lh_d = (f32::from(lh) * scale_factor).round() as i32;   // device px
for bd in &layout.box_draws {
    let cell_x_logical = snap_px(f32::from(origin.x + bd.point.column as f32 * cw));
    let cell_y_logical = snap_px(f32::from(origin.y + bd.point.line as f32 * lh));
    for (rx, ry, rw, rh) in Self::box_drawing_rects(bd.c, cw_d, lh_d) {
        let pos = point(
            px(cell_x_logical + rx as f32 / scale_factor),
            px(cell_y_logical + ry as f32 / scale_factor),
        );
        let sz = size(px(rw as f32 / scale_factor), px(rh as f32 / scale_factor));
        window.paint_quad(fill(Bounds::new(pos, sz), bd.color));
    }
}
```

- Box-drawing geometry is computed in **integer device pixels** (see Part 2).
- The cell origin is snapped with floor; sub-rect dimensions are converted from device px back to logical px via `/ scale_factor`.

### 4.5. Cursor

```rust
let pos = point(
    px(snap_px(f32::from(origin.x + cur.point.column as f32 * cw))),
    px(snap_px(f32::from(origin.y + cur.point.line as f32 * lh))),
);
let sz = match cur.shape {
    CursorShape::Beam => {
        let bar_w = (cw * 0.2).max(px(1.0));
        size(px(ceil_px(f32::from(bar_w))), lh)
    }
    CursorShape::Underline => {
        let ul_h = (lh * 0.15).max(px(2.0));
        size(px(ceil_px(f32::from(cw))), px(ceil_px(f32::from(ul_h))))
    }
    CursorShape::Block => {
        size(px(ceil_px(f32::from(cw))), lh)   // ceil width to fill cell
    }
    ...
};
```

- Block cursor: `ceil_px(cw)` ensures width ≥ cell width, filling the subpixel gap between cells.
- Bar/Underline: `ceil_px` for thickness avoids losing a thin bar.

---

## 5. Snap rule summary

| Quantity | Snap function | Reason |
|---|---|---|
| `cell_width` | `round` | Foundational metric, needs closest device px |
| `line_height` | `round` | Foundational metric, needs closest device px |
| `rows`/`cols` | compute in device px + `floor` | Integer cells exactly fit the viewport |
| `grid_origin` | `round` | Grid anchor point |
| `bg`/`selection` pos | `floor` | Align left/top |
| `bg`/`selection` width | `ceil` | Fill to the next cell edge, no gap |
| `text run` origin | `floor` | Glyphs land on pixel grid |
| `box-drawing` geometry | integer device px | Primitive lines stay sharp at 1 px |
| `cursor` pos | `floor` | Align inside the cell |
| `cursor` size | `ceil` | Fills / no gap |

---

## 6. Effects

- **HiDPI (scale 1.5, 2.0)**: lines are sharp at 1 device px, no anti-alias blur.
- **Continuous resize**: the grid doesn't jitter because metrics always sit on the device-pixel grid.
- **Block cursor**: fills the cell, no subpixel gap.
- **Consistency**: background, selection, text, cursor, box-drawing all share the same origin snap → they don't drift apart.

---

## 7. Problem: box-drawing from font glyphs is blurry / doesn't fill

### Symptoms

Frame-line characters `┌─┐`, `├┤`, `║` or blocks `▀▄▌█` in the terminal:

- Suffer from anti-alias blur along the edges.
- Thin strokes have inconsistent thickness between adjacent characters.
- At non-integer scale factors (e.g. 1.25×, 1.5×) lines fall between the pixel grid → blurry.
- `▀` and `▄` blocks in prompts (Nushell / pi CLI) don't fill, leaving gaps.

### Cause

By default the renderer puts box-drawing chars into a text run, and GPUI shapes + rasterizes
the font glyph by logical coordinate. A monospace font's glyph is usually designed for the
em-square, not a specific cell device pixel → when rasterized with a cell width of 9.6 px,
hinting / anti-aliasing distorts the strokes.

### Solution: custom primitive renderer

Windows Terminal AtlasEngine and Zed terminal both have their own box-drawing renderer:
instead of rasterizing the font, they compute the geometry of small rectangles inside the cell
and draw them with fill rects aligned to device pixels. OneTerm applies the same approach.

---

## 8. Architecture data flow

```
TerminalContent::cells
        │
        ▼
TerminalElement::layout_grid()
        │
        ├─ cell char ∈ U+2500–U+259F?
        │      └─ push BoxDrawCell { point, color, c }
        │         (not put into BatchedTextRun)
        │
        └─ returns (rects, runs, box_draws)
                  │
                  ▼
        LayoutState (stored in prepaint)
                  │
                  ▼
        TerminalElement::paint()
            │
            ├─ paint background rects
            ├─ paint text runs
            ├─ paint box-drawing primitives  ← this part
            └─ paint cursor
```

### `BoxDrawCell` struct

```rust
struct BoxDrawCell {
    point: LayoutPoint, // (display_line, column)
    color: Hsla,        // resolved fg color
    c: char,            // the char to draw
}
```

When layout encounters a box-drawing char, it flushes the current text batch (if any),
pushes a `BoxDrawCell` into a separate vec, then `continue`. This ensures text runs never
contain box-drawing, and box-drawing is painted **above the background, below the cursor**.

---

## 9. Detecting chars that need custom rendering

```rust
fn is_box_drawing(c: char) -> bool {
    matches!(c, '\u{2500}'..='\u{257F}' | '\u{2580}'..='\u{259F}')
}
```

Ranges:

- `U+2500–U+257F`: 128 **Box Drawing** characters — lines, corners, tees,
  double lines, dashed lines, rounded corners.
- `U+2580–U+259F`: 32 **Block Elements** — half blocks, eighths blocks,
  quadrant blocks.

All are "geometric" characters — representable exactly by axis-aligned rectangles.

---

## 10. Computing geometry: `box_drawing_rects`

The function returns `Vec<(i32, i32, i32, i32)>` — each tuple is `(x, y, w, h)` in
**device pixels** relative to the cell origin. The device-pixel coordinate system is integer,
so strokes are always exactly 1 physical px thick, no blur.

### Parameters

```rust
fn box_drawing_rects(c: char, cw_d: i32, lh_d: i32) -> Vec<(i32, i32, i32, i32)>
```

| Parameter | Meaning |
|---------|---------|
| `cw_d`  | Cell width rounded to device pixels |
| `lh_d`  | Line height rounded to device pixels |

### Quick anchor points

```rust
let cx = cw_d / 2;          // horizontal center
let cy = lh_d / 2;          // vertical center
let t  = 1;                 // light stroke thickness = 1 device px
let ht = 2;                 // heavy stroke thickness = 2 device px
let dl = (cw_d / 6).max(1); // horizontal gap between double lines
let dv = (lh_d / 6).max(1); // vertical gap between double lines
```

### Macro helpers

Macros generate rects by direction and position, keeping the code compact and avoiding copy-paste errors:

```rust
macro_rules! h  { ($y:expr, $thick:expr) => { (0, $y, cw_d, $thick) }; }
macro_rules! v  { ($x:expr, $thick:expr) => { ($x, 0, $thick, lh_d) }; }
macro_rules! hr { ($y:expr, $thick:expr) => { (cx, $y, cw_d - cx, $thick) }; }
macro_rules! hl { ($y:expr, $thick:expr) => { (0, $y, cx, $thick) }; }
macro_rules! vd { ($x:expr, $thick:expr) => { ($x, cy, $thick, lh_d - cy) }; }
macro_rules! vu { ($x:expr, $thick:expr) => { ($x, 0, $thick, cy) }; }
```

| Macro | Meaning | Example |
|-------|---------|-------|
| `h!(y, thick)`  | Full-cell horizontal line | `━` `─` |
| `v!(x, thick)`  | Full-cell vertical line   | `┃` `│` |
| `hr!(y, thick)` | Right-half horizontal line  | corner `┌` |
| `hl!(y, thick)` | Left-half horizontal line   | corner `┐` |
| `vd!(x, thick)` | Lower-half vertical line     | corner `┌` |
| `vu!(x, thick)` | Upper-half vertical line     | corner `└` |

Example `┌` (U+250C): vertical line down from center + horizontal line right from center:

```rust
'\u{250C}' => vec![vd!(cx, t), hr!(cy, t)],
```

Example `┼` (U+253C): full-cell horizontal + full-cell vertical:

```rust
'\u{253C}' => vec![h!(cy, t), v!(cx, t)],
```

Example `╋` (U+254B): heavy both directions:

```rust
'\u{254B}' => vec![h!(cy, ht), v!(cx, ht)],
```

---

## 11. Supported character groups

### 19.1 Light / Heavy / Double lines

- **Light** (`U+2500–U+254B`): 1 device px stroke.
- **Heavy** (`U+2501`, `U+2503`, `U+2513`…): 2 device px stroke.
- **Double** (`U+2550–U+256C`): two parallel lines, offset `dl` / `dv`.

Example double horizontal line `═`:

```rust
'\u{2550}' => vec![h!(cy - dv, t), h!(cy + dv, t)],
```

Example double corner `╔`:

```rust
'\u{2554}' => vec![
    vd!(cx - dl, t), vd!(cx + dl, t),
    hr!(cy - dv, t), hr!(cy + dv, t),
],
```

### 19.2 Corners, tees, crosses

All 128 characters U+2500–U+257F are mapped to combinations of the macros above. Complex
characters like heavy left/right/top/bottom tees (`├┤┬┴┼`) are handled by combining
full-cell strokes with half-cell strokes of varying thickness.

### 19.3 Dashed lines (`U+2504–U+2509`)

Dashed lines can't be drawn as one continuous rect. A helper scatters 2 px on, 2 px off segments:

```rust
fn dash_h(y: i32, w: i32, thick: i32) -> Vec<(i32, i32, i32, i32)> {
    let mut out = Vec::new();
    let mut x = 0;
    while x < w {
        let ew = 2.min(w - x);
        out.push((x, y, ew, thick));
        x += 4; // 2 on + 2 off
    }
    out
}

fn dash_v(x: i32, h: i32, thick: i32) -> Vec<(i32, i32, i32, i32)> {
    let mut out = Vec::new();
    let mut y = 0;
    while y < h {
        let eh = 2.min(h - y);
        out.push((x, y, thick, eh));
        y += 4;
    }
    out
}
```

Applied:

```rust
'\u{2504}' | '\u{2506}' => Self::dash_h(cy, cw_d, t),   // light dash
'\u{2505}' | '\u{2507}' => Self::dash_h(cy, cw_d, ht),  // heavy dash
'\u{2508}' => Self::dash_v(cx, lh_d, t),
'\u{2509}' => Self::dash_v(cx, lh_d, ht),
```

> ⚠️ Note: the 2-on/2-off pattern is an approximation; Windows Terminal may use a more complex
> pattern. Currently adequate for simple TUI terminals.

### 19.4 Block Elements `U+2580–U+259F`

Commit `cae796d` extended the range to 32 block characters. These are heavily used by modern
CLIs (pi, Nushell, lazygit, …) to draw progress bars, input box padding, diff markers.

#### Full / Half / Quarter / Eighth blocks

```rust
'\u{2580}' => vec![(0, 0, cw_d, cy)],                    // ▀ upper half
'\u{2584}' => vec![(0, cy, cw_d, lh_d - cy)],           // ▄ lower half
'\u{2588}' => vec![(0, 0, cw_d, lh_d)],                  // █ full block
'\u{258C}' => vec![(0, 0, cx, lh_d)],                   // ▌ left half
'\u{2595}' => vec![(cw_d - cw_d / 8, 0, cw_d / 8, lh_d)], // ▕ right 1/8
'\u{2581}' => vec![(0, lh_d - lh_d / 8, cw_d, lh_d / 8)], // ▁ lower 1/8
'\u{2594}' => vec![(0, 0, cw_d, lh_d / 8)],             // ▔ upper 1/8
```

#### Quadrant blocks

Quadrant characters use the cell center as the boundary:

```rust
'\u{2596}' => vec![(0, cy, cx, lh_d - cy)],           // ▖ lower-left
'\u{2597}' => vec![(cx, cy, cw_d - cx, lh_d - cy)],  // ▗ lower-right
'\u{2598}' => vec![(0, 0, cx, cy)],                  // ▘ upper-left
'\u{259D}' => vec![(cx, 0, cw_d - cx, cy)],          // ▝ upper-right
'\u{2599}' => vec![                                // ▙ 3 quadrants
    (0, 0, cx, cy),
    (0, cy, cx, lh_d - cy),
    (cx, 0, cw_d - cx, cy),
],
```

---

## 12. Paint loop: from device pixel back to logical pixel

In `TerminalElement::paint`, after text runs are done:

```rust
let cw_d = (f32::from(cw) * scale_factor).round() as i32;
let lh_d = (f32::from(lh) * scale_factor).round() as i32;

for bd in &layout.box_draws {
    // Snap cell origin to the device-pixel grid.
    let cell_x_logical = snap_px(f32::from(origin.x + bd.point.column as f32 * cw));
    let cell_y_logical = snap_px(f32::from(origin.y + bd.point.line as f32 * lh));

    // Each rect is device px → convert to logical px for paint_quad.
    for (rx, ry, rw, rh) in Self::box_drawing_rects(bd.c, cw_d, lh_d) {
        let pos = point(
            px(cell_x_logical + rx as f32 / scale_factor),
            px(cell_y_logical + ry as f32 / scale_factor),
        );
        let sz = size(px(rw as f32 / scale_factor), px(rh as f32 / scale_factor));
        window.paint_quad(fill(Bounds::new(pos, sz), bd.color));
    }
}
```

Why compute in device pixels then convert back?

- `cw_d`, `lh_d` are integers → the cell is split into integer parts (cx, cy, dl, dv).
- The rects sit flush on the physical pixel grid.
- Converting to logical px via `/ scale_factor` gives GPUI coordinates without hidden rounding.

---

## 13. Font fallback for unsupported characters

These box-drawing characters are **not custom-rendered** and fall back to font glyphs:

- Diagonals (`╱` U+2571, `╲` U+2572, `╳` U+2573).
- Quadruple-dash (`U+250A`, `U+250B`).
- Shade blocks (`U+2591–U+2593` ░▒▓) — because they need a pattern, not a fill.

In code:

```rust
match c {
    // ... handled cases ...
    _ => vec![],  // empty → layout_grid won't push a BoxDrawCell
}
```

And in `layout_grid`:

```rust
if Self::is_box_drawing(cell.c)
    && !Self::box_drawing_rects(cell.c, 16, 16).is_empty()
{
    box_draws.push(BoxDrawCell { ... });
    continue;
}
```

`box_drawing_rects(cell.c, 16, 16)` is a fast probe: if it returns empty the char has no
custom geometry, so it's left to the text batch as usual.

---

## 14. Why it works

| Aspect | Font glyph | Custom primitive (OneTerm) |
|-----------|-----------|---------------------------|
| Anti-alias | Yes, per font hinting | No — axis-aligned rects |
| 1 px line | Can be blurry on subpixel | Sharp 1 device px |
| Heavy 2 px | Font-dependent | Always 2 device px |
| Double line | May overlap | Two separate rects |
| Block halves | May have gaps | Flush with cell grid |
| HiDPI (1.5×) | Prone to blur | Device-px snap |
| Batching | GPUI shape_line batch | Many small paint_quad calls |

Although custom primitives add draw calls, in modern TUI terminals the number of
box-drawing/block cells on screen is usually tiny compared to regular text. The sharpness
trade-off is well worth it.

---

## 15. Testing / Verification

### Visual test commands

```bash
# Box drawing light/heavy/double
echo -e '\xe2\x94\x8c\xe2\x94\x80\xe2\x94\x90'
echo -e '\xe2\x94\x82A\xe2\x94\x82'
echo -e '\xe2\x94\x94\xe2\x94\x80\xe2\x94\x98'

# Double-line frame
echo -e '\xe2\x95\x94\xe2\x95\x90\xe2\x95\x97'
echo -e '\xe2\x95\x91 \xe2\x95\x91'
echo -e '\xe2\x95\x9a\xe2\x95\x90\xe2\x95\x9d'

# Block elements
echo -e '\xe2\x96\x80\xe2\x96\x84\xe2\x96\x88\xe2\x96\x8c'

# Nushell prompt / pi CLI — use ▀ ▄ ▌ in practice
```

### Checks

1. Frame lines are straight, not blurry.
2. Corners `┌` and `└` meet flush when joined.
3. `▀` and `▄` leave no gaps.
4. At scale factor 1.5× or 2.0×, 1 px lines stay sharp.

---

## 16. References

| Source | Path / URL | Relevance |
|--------|-----------|-----------|
| Windows Terminal AtlasEngine | `src/renderer/atlas/AtlasEngine.cpp` | Custom box-drawing primitive |
| Zed terminal_element | `crates/terminal_ui/src/terminal_element.rs` | Inspiration for layout + paint split |
| Unicode Box Drawing | `U+2500–U+257F` | 128 characters |
| Unicode Block Elements | `U+2580–U+259F` | 32 characters |
| OneTerm `terminal_element.rs` | `crates/ui/src/views/terminal/terminal_element.rs` | Implementation |

---

---

## 17. Context — The problem to solve

### Symptoms

The prompt line / input area in `OneTerm` looked different from Windows Terminal:

1. **Thin cursor instead of a full block.** The shell (Nushell / reedline) sends `DECSCUSR`
   to set a Beam cursor. `TerminalElement` previously used `snapshot.cursor.shape`
   from the shell → always Beam, ignoring `cursor.shape = "block"` in `terminal.json`.
2. **Wrong cell width.** Auto width measured the advance of `'m'` (CSS `em`), instead of `'0'`
   (CSS `ch`). `'m'` is usually ~10% wider than `'0'` → cell too wide, text got squeezed.
3. **Line height could clip text.** `line_height = font_size * factor` doesn't
   guarantee ≥ `ascent + descent` → if the factor is low, glyphs get clipped at top/bottom.
4. **Block cursor had subpixel gap.** `size(cw, lh)` didn't snap width up to device
   pixels → left a gap between the cursor and the neighboring cell.

### Comparison with Windows Terminal

| Aspect | Windows Terminal | OneTerm (before) | OneTerm (after) |
|---|---|---|---|
| Cell width | `round(advance('0'))` — CSS `ch` | `advance('m')` or override `8.0` | `ch_advance('0')` ✅ |
| Line height | `round(ascent + descent + lineGap)` | `font_size * factor` | `max(factor * font_size, ascent + descent)` ✅ |
| Cursor shape | User config overrides shell | Shell snapshot (Beam) | Config override (except Hidden) ✅ |
| Cursor block fill | `ceil_px(cell_width)` snap | `cw` (logical, subpixel gap) | `ceil_px(cw)` ✅ |
| Baseline center | `round(ascent + (lineGap + adjustedHeight - advanceHeight) / 2)` | GPUI `paint_line` centers itself | Unchanged ✅ |

---

## 18. Cell Width — CSS `ch` Unit (advance width of `'0'`)

### 36.1. Why `'0'` instead of `'m'`?

- CSS `ch` unit = advance width of `'0'` (CSS Values and Units § 4).
- Windows Terminal AtlasEngine uses the same `'0'` character (comment in
  `AtlasEngine.api.cpp`).
- Monospace font: `'0'` advance = standard cell width; `'m'` can be wider
  due to thick stems.
- `'0'` always exists in every monospace font (ASCII 0x30).

### 36.2. Implementation in `TerminalElement::prepaint`

```rust
let scale_factor = window.scale_factor().max(1.0);
let snap_px = |value: f32| -> f32 { (value * scale_factor).round() / scale_factor };

let font_id = cx.text_system().resolve_font(&self.font);
let font_px = self.font_size;

let cell_width = if let Some(cw) = self.cell_width_override {
    // User override in terminal.json → snap to device pixel.
    px(snap_px(cw))
} else {
    // Windows Terminal / CSS ch unit: measure the advance width of '0'.
    let raw = cx
        .text_system()
        .ch_advance(font_id, font_px)
        .map(|s| f32::from(s))
        .unwrap_or_else(|_| {
            // Fallback: measure 'm' advance if '0' has no glyph.
            cx.text_system()
                .advance(font_id, font_px, 'm')
                .map(|s| f32::from(s.width))
                .unwrap_or(8.0)
        });
    px(snap_px(raw))
};
```

### 36.3. Config default

In `crates/ui/src/state/terminal_config.rs`:

```rust
/// Cell width override in px (null = auto from advance width of '0',
/// like Windows Terminal / CSS ch unit).
#[serde(default = "default_cell_width")]
pub cell_width: Option<f32>,

fn default_cell_width() -> Option<f32> {
    None // auto: measure advance width of '0' (CSS ch unit, like Windows Terminal)
}
```

The old default was `Some(8.0)` — removed. Now a user config of `null` auto-measures
the font, matching Windows Terminal.

---

## 19. Line Height — font metrics minimum

### 37.1. The problem

The `layout.line_height` config is a multiplier (e.g. `1.2`). A naive computation:

```rust
let line_height = px(font_size * line_height_factor);
```

with a small factor (e.g. `1.0`) can be smaller than the glyph's `ascent + descent` →
text gets clipped at top/bottom.

### 37.2. Solution: `max(factor, natural)`

GPUI exposes `TextSystem::ascent` / `TextSystem::descent` (equivalent to
`DWRITE_FONT_METRICS::ascent` / `descent`):

```rust
let font_ascent = cx.text_system().ascent(font_id, font_px);
let font_descent = cx.text_system().descent(font_id, font_px);
let natural_line_height = f32::from(font_ascent) + f32::from(font_descent);
let factor_height = f32::from(font_px) * self.line_height_factor;

// max(factor_height, natural_line_height) → never clips.
let line_height = px(snap_px(factor_height.max(natural_line_height)));
```

### 37.3. Why only `ascent + descent`?

Windows Terminal computes `advanceHeight = ascent + descent + lineGap`. GPUI
doesn't expose `lineGap`, but the default `line_height_factor` (`1.2`)
covers the line gap.

Furthermore, GPUI `paint_line` centers text within `line_height` based on
the shaped line's `layout.ascent` / `layout.descent`:

```rust
let padding_top = (line_height - layout.ascent - layout.descent) / 2.;
let baseline_offset = point(px(0.), padding_top + layout.ascent);
```

→ As long as `line_height >= ascent + descent`, text isn't clipped and the baseline
is centered automatically.

---

## 20. Cursor Shape Override — user config wins over shell

### 38.1. The problem

`alacritty_terminal` receives the `DECSCUSR` escape sequence (`\x1b[5 q`) from the shell and
stores the shape in `TerminalContent::cursor.shape`. `TerminalElement` used this shape to paint.
Result: `terminal.json` set `cursor.shape = "block"` but the shell still forced Beam.

### 38.2. Same principle as Windows Terminal

Windows Terminal respects the user setting `cursorShape` in `profile.json` — the shell cannot override it. `OneTerm` applies the same principle:

- `snapshot.cursor.shape == Hidden` → hide the cursor (shell explicitly hides it).
- Otherwise → use `cursor_shape_override` from config (Block / Bar / Underline).

### 38.3. Data flow

```
terminal.json
    │  cursor.shape: "block"
    ▼
TerminalConfig
    │  cursor: CursorConfig { shape: String }
    ▼
TerminalSettings::apply_config
    │  cursor_shape: TerminalCursorShape::from_str(...)
    ▼
LocalTerminalView::render
    │  cursor_shape → TerminalElement::new(...)
    ▼
TerminalElement::prepaint
    │  map TerminalCursorShape → alacritty::CursorShape
    │  build CursorPaint { point, color, shape }
```

### 38.4. Code

`crates/ui/src/state/terminal_settings.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalCursorShape {
    #[default]
    Block,
    Bar,
    Underline,
}

impl TerminalCursorShape {
    fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "bar" => Self::Bar,
            "underline" => Self::Underline,
            _ => Self::Block,
        }
    }
}
```

`crates/ui/src/views/terminal/terminal_view.rs`:

```rust
let (
    font, font_size, line_height_factor,
    cursor_visible, bell_enabled, has_bell,
    cursor_color, padding, cell_width_override,
    color_overrides, cursor_shape,
) = {
    let settings = settings_entity.read(cx);
    (
        ...,
        settings.cursor_shape,  // passed down to the element
    )
};
// ...
TerminalElement::new(
    ...,
    padding,
    cell_width_override,
    cursor_color,
    cursor_shape,  // config override
)
```

`crates/ui/src/views/terminal/terminal_element.rs`:

```rust
pub(crate) struct TerminalElement {
    ...
    cursor_shape_override: crate::state::TerminalCursorShape,
}

// In prepaint:
let cursor = {
    let c = &snapshot.cursor;
    if c.shape == CursorShape::Hidden {
        None
    } else {
        ...
        let shape = match self.cursor_shape_override {
            crate::state::TerminalCursorShape::Block => CursorShape::Block,
            crate::state::TerminalCursorShape::Bar => CursorShape::Beam,
            crate::state::TerminalCursorShape::Underline => CursorShape::Underline,
        };
        Some(CursorPaint { point, color, shape })
    }
};
```

### 38.5. Important notes

- The shell can still **hide** the cursor via `Hidden`.
- The shell **cannot** set Beam / Block / Underline — that right belongs to user config.
- This matches Windows Terminal: `cursorShape` in the profile is canonical.

---

## 21. Cursor Paint — device pixel snap for Block/Bar/Underline

In `TerminalElement::paint`:

```rust
let scale_factor = window.scale_factor().max(1.0);
let snap_px = |value: f32| -> f32 { (value * scale_factor).floor() / scale_factor };
let ceil_px = |value: f32| -> f32 { (value * scale_factor).ceil() / scale_factor };

let pos = point(
    px(snap_px(f32::from(origin.x + cur.point.column as f32 * cw))),
    px(snap_px(f32::from(origin.y + cur.point.line as f32 * lh))),
);

let sz = match cur.shape {
    CursorShape::Beam => {
        let bar_w = (cw * 0.2).max(px(1.0));
        size(px(ceil_px(f32::from(bar_w))), lh)
    }
    CursorShape::Underline => {
        let ul_h = (lh * 0.15).max(px(2.0));
        size(px(ceil_px(f32::from(cw))), px(ceil_px(f32::from(ul_h))))
    }
    CursorShape::Block | CursorShape::HollowBlock => {
        size(px(ceil_px(f32::from(cw))), lh)
    }
    CursorShape::Hidden => return,
};
window.paint_quad(fill(Bounds::new(pos, sz), cur.color));
```

### Why `ceil_px` for width?

- `snap_px` = `floor(x * scale) / scale` → align left.
- `ceil_px` = `ceil(x * scale) / scale` → width ≥ logical width, flush with the right
  neighbor, no subpixel gap.
- Especially important for the **Block** cursor to fill the cell and match the grid.

---

## 22. Code structure — consolidated data flow

```
terminal.json (user config)
    │
    ▼
TerminalConfig (serde deserialize)
    │  layout.cell_width: Option<f32>     (None = auto)
    │  layout.line_height: f32            (factor, default 1.15)
    │  cursor.shape: String               ("block" | "bar" | "underline")
    ▼
TerminalSettings::apply_config
    │  cell_width: Option<f32>
    │  line_height_factor: f32
    │  cursor_shape: TerminalCursorShape
    ▼
LocalTerminalView::render
    │  settings.cursor_shape   → cursor_shape_override
    │  settings.cell_width     → cell_width_override
    │  settings.line_height_factor → line_height_factor
    ▼
TerminalElement::new(..., cursor_shape_override, cell_width_override, ...)
    ▼
TerminalElement::prepaint()
    │
    ├─ font_id = cx.text_system().resolve_font(&self.font)
    ├─ cell_width = override ?? ch_advance(font_id, font_px) // '0'
    ├─ line_height = max(font_px * factor, ascent + descent)
    ├─ cursor.shape = match cursor_shape_override { ... }  // config override
    └─ snap all metrics → device pixel grid (Part 1)
    ▼
TerminalElement::paint()
    │
    ├─ bg rects (snap origin + ceil width)
    ├─ selection rects
    ├─ text runs (shape_line + paint, GPUI centers itself)
    ├─ box-drawing primitives (Part 2)
    └─ cursor (snap origin + ceil width)
```

---

## 23. Config reference

### `terminal.json` — Layout + Cursor

```jsonc
{
  "cursor": {
    "shape": "block",      // "block" | "bar" | "underline" — overrides shell
    "blink": true,          // blinks when focused
    "color": null           // null = theme caret, "#RRGGBB" = override
  },
  "layout": {
    "line_height": 1.2,    // factor × font_size, minimum = ascent + descent
    "cell_width": null,     // null = auto (advance '0'), number = override px
    "padding": { "top": 0, "right": 5, "bottom": 0, "left": 10 }
  }
}
```

### Defaults

| Parameter | Default value | Notes |
|---|---|---|
| `cursor.shape` | `"block"` | Overrides shell `DECSCUSR` |
| `layout.line_height` | `1.2` | Factor, covers lineGap |
| `layout.cell_width` | `null` | Auto = `ch_advance('0')` |
| `cursor.blink` | `true` | 500ms interval |

---

## 24. Testing

### Unit tests (`terminal_config.rs`)

```rust
// Default cell_width = None (auto)
assert_eq!(cfg.layout.cell_width, None);

// Custom override still works
let json = r#"{ "layout": { "cell_width": 8.0 } }"#;
let cfg: TerminalConfig = serde_json::from_str(json).unwrap();
assert_eq!(cfg.layout.cell_width, Some(8.0));
```

### Visual verification

1. Cursor shape: set `cursor.shape = "block"` → cursor is a full block, not
   Beam as the shell set.
2. Cell width: set `cell_width = null` → cell matches font advance, text isn't squeezed.
3. Line height: set `line_height = 1.0` → text isn't clipped.
4. Scale 1.5: all metrics snap to device pixels → no jitter on resize.

---

## 25. References

| Source | Path / URL | Relevant part |
|---|---|---|
| Windows Terminal AtlasEngine | `src/renderer/atlas/AtlasEngine.api.cpp` | `_resolveFontMetrics()` |
| GPUI TextSystem | `crates/gpui/src/text_system.rs` | `ch_advance`, `ascent`, `descent` |
| GPUI paint_line | `crates/gpui/src/text_system/line.rs` | `padding_top`, `baseline_offset` |
| CSS ch unit | CSS Values and Units § 4 | `ch` = advance width of '0' |
| DirectWrite metrics | `DWRITE_FONT_METRICS` | `ascent`, `descent`, `lineGap` |
| Alacritty CursorShape | `alacritty_terminal::vte::ansi::CursorShape` | Block/Beam/Underline/Hidden |

---

## 26. Consolidated data flow

```
terminal.json (user config)
    │
    ▼
TerminalConfig (serde deserialize)
    │  layout.cell_width: Option<f32>     (None = auto)
    │  layout.line_height: f32            (factor, default 1.2)
    │  cursor.shape: String               ("block" | "bar" | "underline")
    ▼
TerminalSettings::apply_config
    │  cell_width: Option<f32>
    │  line_height_factor: f32
    │  cursor_shape: TerminalCursorShape
    ▼
LocalTerminalView::render
    │  settings.cursor_shape   → cursor_shape_override
    │  settings.cell_width     → cell_width_override
    │  settings.line_height_factor → line_height_factor
    ▼
TerminalElement::new(..., cursor_shape_override, cell_width_override, ...)
    ▼
TerminalElement::prepaint()
    │
    ├─ scale_factor = window.scale_factor()
    ├─ font_id = cx.text_system().resolve_font(&self.font)
    ├─ cell_width = override ?? ch_advance(font_id, font_px)
    ├─ line_height = max(font_px * factor, ascent + descent)
    ├─ cursor.shape = match cursor_shape_override { ... }
    ├─ rows/cols computed in device pixels
    ├─ grid_origin snapped to device pixels
    └─ all metrics snap → device pixel grid
    ▼
TerminalElement::paint()
    │
    ├─ bg rects (floor origin + ceil width)
    ├─ selection rects
    ├─ text runs (shape_line + paint, GPUI centers itself)
    ├─ box-drawing / block primitives (integer device px)
    └─ cursor (floor origin + ceil width)
```

---

## 27. Comparison with Windows Terminal AtlasEngine

| Aspect | Windows Terminal | OneTerm (after commits) |
|---|---|---|
| Device pixel snap | Integer device px vertex coordinates | Snap logical px before paint (equivalent) |
| Box-drawing | Custom AtlasEngine primitive | Custom fill-rect primitives |
| Block elements | Custom AtlasEngine primitive | Custom fill-rect primitives |
| Cell width | `round(advance('0'))` | `ch_advance('0')` + round |
| Line height | `round(ascent+descent+lineGap)` | `max(factor*font_size, ascent+descent)` |
| Cursor shape | User config overrides shell | Config override (except Hidden) |
| Cursor block fill | `ceil_px(cell_width)` | `ceil_px(cell_width)` |
| Baseline center | `round(ascent + ...)` | GPUI `paint_line` centers itself |

---

## 28. Config reference

```jsonc
{
  "cursor": {
    "shape": "block",      // "block" | "bar" | "underline" — overrides shell
    "blink": true,          // blinks when focused
    "color": null           // null = theme caret, "#RRGGBB" = override
  },
  "layout": {
    "line_height": 1.2,    // factor × font_size, minimum = ascent + descent
    "cell_width": null,     // null = auto (advance '0'), number = override px
    "padding": { "top": 0, "right": 5, "bottom": 0, "left": 10 }
  }
}
```

### Defaults

| Parameter | Default value | Notes |
|---|---|---|
| `cursor.shape` | `"block"` | Overrides shell `DECSCUSR` |
| `layout.line_height` | `1.2` | Factor, covers lineGap |
| `layout.cell_width` | `null` | Auto = `ch_advance('0')` |
| `cursor.blink` | `true` | 500ms interval |

---

## 29. Testing & verification

### Visual tests

```bash
# Box drawing
$ echo -e '\xe2\x94\x8c\xe2\x94\x80\xe2\x94\x90'
$ echo -e '\xe2\x95\x94\xe2\x95\x90\xe2\x95\x97\n\xe2\x95\x91 \xe2\x95\x91\n\xe2\x95\x9a\xe2\x95\x90\xe2\x95\x9d'

# Block elements
$ echo -e '\xe2\x96\x80\xe2\x96\x84\xe2\x96\x88\xe2\x96\x8c'

# Cursor shape
# Set cursor.shape = "block" → cursor must be a full block even if the shell sets Beam.
```

### Checks

1. Frame lines sharp, not blurry at scale 1.5×/2.0×.
2. `▀`/`▄`/`▌` fill the cell, no gaps.
3. `cell_width = null` → cell matches font advance, text isn't squeezed.
4. `line_height = 1.0` → text isn't clipped.
5. `cursor.shape = "block"` → block cursor, shell doesn't override.

---

## 30. References

| Source | Path / URL | Relevant part |
|---|---|---|
| Windows Terminal AtlasEngine | `src/renderer/atlas/AtlasEngine.api.cpp` | `_resolveFontMetrics()`, box-drawing primitive |
| Zed terminal_element | `crates/terminal_ui/src/terminal_element.rs` | Layout + paint split |
| GPUI TextSystem | `crates/gpui/src/text_system.rs` | `ch_advance`, `ascent`, `descent` |
| GPUI paint_line | `crates/gpui/src/text_system/line.rs` | `padding_top`, `baseline_offset` |
| CSS ch unit | CSS Values and Units § 4 | `ch` = advance width of '0' |
| DirectWrite metrics | `DWRITE_FONT_METRICS` | `ascent`, `descent`, `lineGap` |
| Alacritty CursorShape | `alacritty_terminal::vte::ansi::CursorShape` | Block/Beam/Underline/Hidden |
| Unicode Box Drawing | `U+2500–U+257F` | 128 characters |
| Unicode Block Elements | `U+2580–U+259F` | 32 characters |

---

*Document merged from part1 + part2 + part3. See commits `1aab15c`, `cae796d`, `e8269ba` for implementation details.*