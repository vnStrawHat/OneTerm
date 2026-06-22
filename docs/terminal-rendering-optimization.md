# Terminal Rendering Optimization — Technical Document

> Tài liệu kỹ thuật toàn diện mô tả các phương pháp, công nghệ và kỹ thuật đã áp dụng trong `TerminalElement` của **myTerm2** để đạt chất lượng render terminal gần với **Windows Terminal AtlasEngine**.
>
> **Commits áp dụng**:
> - `1aab15c` — snap cell metrics to device-pixel grid + custom box-drawing primitive
> - `cae796d` — extend custom renderer for block elements U+2580-U+259F
> - `e8269ba` — Windows Terminal-grade cell metrics + cursor shape override
>
> **File triển khai chính**: `crates/ui/src/views/terminal/terminal_element.rs`  
> **Ngày áp dụng**: 2026-06-22

---

## Tổng quan

`TerminalElement` là custom `gpui::Element` render terminal grid từ `TerminalContent` snapshot. Vấn đề ban đầu:
- Cell metrics (width/height) là float logical px → subpixel jitter, đường kẻ nhòe.
- Box-drawing / block element dùng font glyph → anti-alias blur, không khít cell.
- Cell width auto đo `'m'` thay vì `'0'` → cell rộng hơn Windows Terminal.
- Line height factor có thể nhỏ hơn `ascent + descent` → clip glyph.
- Shell `DECSCUSR` override user config cursor shape.

Giải pháp gồm 3 trụ cột:
1. **Device-pixel grid snapping** — snap mọi metric/tọa độ sang device pixel.
2. **Custom primitive renderer** — vẽ box-drawing/block bằng fill rects thay vì font glyph.
3. **Font-metrics-based cell metrics + cursor override** — `ch_advance('0')`, line height ≥ `ascent+descent`, config shape thắng shell.

---



## 1. Tại sao cần snap?

`TerminalElement` vẽ một grid monospace: mỗi cell có width/height cố định, text được đặt tại `origin + (col * cell_width, line * line_height)`. Nếu `cell_width`/`line_height` là float logical px (ví dụ 9.0 px ở scale 1.5 → 13.5 device px), thì:

- Các đường kẻ (`─`, `│`, `┌` … U+2500–U+257F) nằm giữa hai device pixel → anti-alias theo chiều ngang/dọc → nhòe.
- Block cursor (`█`) rộng 13.5 px → khe hở subpixel giữa cursor và cell kế bên.
- Text baseline khác nhau giữa các dòng → rasterize glyph lệch pixel grid.
- Resize window làm cell metrics thay đổi 1 subpixel → toàn bộ grid *jitter*.

**Windows Terminal** và **Zed terminal** giải quyết bằng cách snap mọi tọa độ/metrics sang device-pixel grid nguyên. `myTerm2` áp dụng tưng tự ở tầng GPUI logical pixel — kết quả glyph rasterize khít pixel grid.

---

## 2. Công thức snap

GPUI dùng logical pixel; window cung cấp `scale_factor` (1.0 = 96 dpi, 1.5/2.0 trên HiDPI).

```rust
let scale_factor = window.scale_factor().max(1.0);

// Round sang device pixel gần nhất — dùng cho cell_width/line_height.
let snap_px = |value: f32| -> f32 { (value * scale_factor).round() / scale_factor };

// Floor — dùng cho origin, tọa độ bắt đầu (align trái/trên).
let snap_px_floor = |value: f32| -> f32 { (value * scale_factor).floor() / scale_factor };

// Ceil — dùng cho width/height cần fill đầy cell/khít cell kế bên.
let ceil_px = |value: f32| -> f32 { (value * scale_factor).ceil() / scale_factor };
```

- `value * scale_factor` chuyển logical px → device px.
- `round()` / `floor()` / `ceil()` snap sang integer device px.
- `/ scale_factor` chuyển ngược về logical px (vẫn nằm trên device-pixel grid).

Ví dụ với `scale_factor = 1.5`:

| Logical | Device | Snap round | Snap floor | Snap ceil | Logical sau snap |
|---|---|---|---|---|---|
| 9.0 | 13.5 | 14.0 | 13.0 | 14.0 | 9.333 (round/ceil) / 8.667 (floor) |
| 16.4 | 24.6 | 25.0 | 24.0 | 25.0 | 16.667 / 16.0 / 16.667 |

---

## 3. Áp dụng trong `prepaint`

### 3.1. `cell_width` và `line_height`

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
    px(snap_px(raw))        // round sang device pixel
};

let font_ascent = cx.text_system().ascent(font_id, font_px);
let font_descent = cx.text_system().descent(font_id, font_px);
let natural_line_height = f32::from(font_ascent) + f32::from(font_descent);
let factor_height = f32::from(font_px) * self.line_height_factor;
let line_height = px(snap_px(factor_height.max(natural_line_height)));
```

**Lý do**: `cell_width` và `line_height` là nền tảng của toàn bộ grid. Round sang device pixel đảm bảo mỗi dòng/cột nằm chính xác trên device-pixel grid.

### 3.2. Tính `rows` / `cols` bằng device pixel

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

**Lý do**: tính số hàng/cột trong device-pixel space (integer) tránh lỗi làm tròn khi chia logical px. Kết quả `rows`/`cols` là số nguyên cell vừa khít viewport.

### 3.3. Grid origin

```rust
let grid_origin = GpuiPoint {
    x: px(snap_px(f32::from(bounds.origin.x + gutter_width + pad_left))),
    y: px(snap_px(f32::from(bounds.origin.y + pad_top))),
};
```

Grid origin là điểm bắt đầu của vùng terminal (bên phải gutter). Snap sang device pixel đảm bảo dòng đầu tiên và cột đầu tiên khít grid — đặc biệt quan trọng khi gutter width không phải bội số của device pixel.

### 3.4. Gutter entry Y

```rust
y: px(snap_px(f32::from(bounds.origin.y + i as f32 * line_height))),
```

Mỗi dòng gutter text cũng snap Y theo `line_height` đã snap, tránh lệch baseline giữa các dòng.

---

## 4. Áp dụng trong `paint`

Trong `paint` dùng `floor` cho origin, `ceil` cho size để fill đầy cell.

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

- `pos`: floor để align trái/trên.
- `sz.width`: ceil để background fill đến tận cạnh phải cell cuối, không để khe hở subpixel.

### 4.2. Selection rects

Tương tự background, dùng `floor` + `ceil`.

### 4.3. Text runs

```rust
// Trong BatchedTextRun::paint
let snap_px = |value: f32| -> f32 { (value * scale_factor).floor() / scale_factor };
let pos = point(
    px(snap_px(f32::from(origin.x + self.start.column as f32 * cell_w))),
    px(snap_px(f32::from(origin.y + self.start.line as f32 * line_h))),
);
```

Snap text origin để glyph rasterize khít pixel grid. GPUI `paint_line` sau đó tự center text trong `line_height` dựa trên font metrics của shaped line.

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

- Geometry box-drawing được tính trong **device pixel integer** (xem Phần 2).
- Tọa độ cell origin snap floor; kích thước sub-rect chuyển từ device px về logical px bằng `/ scale_factor`.

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
        size(px(ceil_px(f32::from(cw))), lh)   // ceil width khít cell
    }
    ...
};
```

- Block cursor: `ceil_px(cw)` đảm bảo width ≥ cell width, lấp đầy khoảng trống subpixel giữa các cell.
- Bar/Underline: `ceil_px` cho thickness tránh mất hình thanh mỏng.

---

## 5. Tóm tắt quy tắc snap

| Đại lượng | Hàm snap | Lý do |
|---|---|---|
| `cell_width` | `round` | Metric nền tảng, cần gần đúng nhất device px |
| `line_height` | `round` | Metric nền tảng, cần gần đúng nhất device px |
| `rows`/`cols` | tính trong device px + `floor` | Số nguyên cell vừa khít viewport |
| `grid_origin` | `round` | Điểm neo grid |
| `bg`/`selection` pos | `floor` | Align trái/trên |
| `bg`/`selection` width | `ceil` | Fill đến cạnh cell kế, không khe hở |
| `text run` origin | `floor` | Glyph khít pixel grid |
| `box-drawing` geometry | integer device px | Primitive line 1 px sharp |
| `cursor` pos | `floor` | Align trong cell |
| `cursor` size | `ceil` | Khít/không khe hở |

---

## 6. Hiệu quả

- **HiDPI (scale 1.5, 2.0)**: đường kẻ 1 device px sharp, không anti-alias blur.
- **Resize liên tục**: grid không jitter vì metrics luôn nằm trên device-pixel grid.
- **Block cursor**: khít cell, không subpixel gap.
- **Consistency**: background, selection, text, cursor, box-drawing dùng chung origin snap → không lệch nhau.

---

## 7. Vấn đề: Box-drawing từ font glyph bị mờ / không khít

### Triệu chứng

Các ký tự đường khung `┌─┐`, `├┤`, `║` hay các khối `▀▄▌█` trong terminal:

- Bị anti-alias blur dọc theo cạnh.
- Đường nét mỏng có độ dày không nhất quán giữa các ký tự liền kề.
- Ở scale factor không nguyên (vd 1.25×, 1.5×) các đường nằm giữa pixel grid → mờ.
- Các khối `▀` và `▄` trong prompt (Nushell / pi CLI) không khít, để lại khe hở.

### Nguyên nhân

Mặc định renderer đưa box-drawing char vào text run, GPUI shape + rasterize
font glyph theo logical coordinate. Glyph của monospace font thường thiết kế
cho em-square chứ không phải cell device pixel cụ thể → khi rasterize với
cell width 9.6 px, hinting / anti-alias làm méo nét.

### Giải pháp: custom primitive renderer

Windows Terminal AtlasEngine và Zed terminal đều có bộ vẽ box-drawing riêng:
thay vì rasterize font, họ tính geometry các hình chữ nhật nhỏ trong cell và
vẽ bằng fill rects khít device pixel. myTerm2 áp dụng cùng phương pháp.

---

## 8. Kiến trúc data flow

```
TerminalContent::cells
        │
        ▼
TerminalElement::layout_grid()
        │
        ├─ cell có char ∈ U+2500–U+259F?
        │      └─ push BoxDrawCell { point, color, c }
        │         (không đưa vào BatchedTextRun)
        │
        └─ trả về (rects, runs, box_draws)
                  │
                  ▼
        LayoutState (stored in prepaint)
                  │
                  ▼
        TerminalElement::paint()
            │
            ├─ paint background rects
            ├─ paint text runs
            ├─ paint box-drawing primitives  ← phần này
            └─ paint cursor
```

### Struct `BoxDrawCell`

```rust
struct BoxDrawCell {
    point: LayoutPoint, // (display_line, column)
    color: Hsla,        // fg color đã resolve
    c: char,            // ký tự cần vẽ
}
```

Khi layout gặp box-drawing char, nó flush text batch hiện tại (nếu có),
đẩy `BoxDrawCell` vào vec riêng, rồi `continue`. Đảm bảo text run không chứa
box-drawing, và box-drawing được vẽ **trên nền, dưới cursor**.

---

## 9. Nhận diện ký tự cần custom render

```rust
fn is_box_drawing(c: char) -> bool {
    matches!(c, '\u{2500}'..='\u{257F}' | '\u{2580}'..='\u{259F}')
}
```

Phạm vi:

- `U+2500–U+257F`: 128 ký tự **Box Drawing** — đường thẳng, góc, ngã tư,
  đường đôi, đường nét đứt, góc bo.
- `U+2580–U+259F`: 32 ký tự **Block Elements** — nửa khối, phần tám khối,
  khối góc phần tư.

Tất cả đều là ký tự “geometric” — có thể biểu diễn chính xác bằng hình chữ
nhật axis-aligned.

---

## 10. Tính geometry: `box_drawing_rects`

Hàm trả về `Vec<(i32, i32, i32, i32)>` — mỗi tuple là `(x, y, w, h)` tính
bằng **device pixel** relative đến gốc cell. Hệ tọa độ device pixel là integer,
nên các nét line luôn dày đúng 1 px vật lý, không blur.

### Tham số

```rust
fn box_drawing_rects(c: char, cw_d: i32, lh_d: i32) -> Vec<(i32, i32, i32, i32)>
```

| Tham số | Ý nghĩa |
|---------|---------|
| `cw_d`  | Cell width làm tròn sang device pixel |
| `lh_d`  | Line height làm tròn sang device pixel |

### Các điểm gốc nhanh

```rust
let cx = cw_d / 2;          // tâm ngang
let cy = lh_d / 2;          // tâm dọc
let t  = 1;                 // độ dày nét mảnh (light) = 1 device px
let ht = 2;                 // độ dày nét đậm (heavy)  = 2 device px
let dl = (cw_d / 6).max(1); // khoảng cách 2 đường đôi theo ngang
let dv = (lh_d / 6).max(1); // khoảng cách 2 đường đôi theo dọc
```

### Macro helper

Các macro sinh rect theo hướng và vị trí, giúp code gọn và không sai số copy-paste:

```rust
macro_rules! h  { ($y:expr, $thick:expr) => { (0, $y, cw_d, $thick) }; }
macro_rules! v  { ($x:expr, $thick:expr) => { ($x, 0, $thick, lh_d) }; }
macro_rules! hr { ($y:expr, $thick:expr) => { (cx, $y, cw_d - cx, $thick) }; }
macro_rules! hl { ($y:expr, $thick:expr) => { (0, $y, cx, $thick) }; }
macro_rules! vd { ($x:expr, $thick:expr) => { ($x, cy, $thick, lh_d - cy) }; }
macro_rules! vu { ($x:expr, $thick:expr) => { ($x, 0, $thick, cy) }; }
```

| Macro | Ý nghĩa | Ví dụ |
|-------|---------|-------|
| `h!(y, thick)`  | Đường ngang toàn cell | `━` `─` |
| `v!(x, thick)`  | Đường dọc toàn cell   | `┃` `│` |
| `hr!(y, thick)` | Nửa phải đường ngang  | góc `┌` |
| `hl!(y, thick)` | Nửa trái đường ngang   | góc `┐` |
| `vd!(x, thick)` | Nửa dưới đường dọc     | góc `┌` |
| `vu!(x, thick)` | Nửa trên đường dọc     | góc `└` |

Ví dụ `┌` (U+250C): nét dọc xuống từ tâm + nét ngang sang phải từ tâm:

```rust
'\u{250C}' => vec![vd!(cx, t), hr!(cy, t)],
```

Ví dụ `┼` (U+253C): nét ngang toàn cell + nét dọc toàn cell:

```rust
'\u{253C}' => vec![h!(cy, t), v!(cx, t)],
```

Ví dụ `╋` (U+254B): nét đậm cả hai chiều:

```rust
'\u{254B}' => vec![h!(cy, ht), v!(cx, ht)],
```

---

## 11. Các nhóm ký tự được hỗ trợ

### 19.1 Light / Heavy / Double lines

- **Light** (`U+2500–U+254B`): nét 1 device px.
- **Heavy** (`U+2501`, `U+2503`, `U+2513`…): nét 2 device px.
- **Double** (`U+2550–U+256C`): hai nét song song, offset `dl` / `dv`.

Ví dụ đường đôi ngang `═`:

```rust
'\u{2550}' => vec![h!(cy - dv, t), h!(cy + dv, t)],
```

Ví dụ góc đôi `╔`:

```rust
'\u{2554}' => vec![
    vd!(cx - dl, t), vd!(cx + dl, t),
    hr!(cy - dv, t), hr!(cy + dv, t),
],
```

### 19.2 Corners, tees, crosses

Tất cả 128 ký tự U+2500–U+257F được map thành tổ hợp các macro trên. Các ký tự
phức tạp như ngã tư nặng/phải/trên/dưới (`├┤┬┴┼`) đều được xử lý bằng cách
kết hợp nét toàn cell + nét nửa cell với độ dày khác nhau.

### 19.3 Dashed lines (`U+2504–U+2509`)

Các đường nét đứt không thể vẽ một rect liền. Dùng helper rải đoạn 2 px on,
2 px off:

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

Áp dụng:

```rust
'\u{2504}' | '\u{2506}' => Self::dash_h(cy, cw_d, t),   // light dash
'\u{2505}' | '\u{2507}' => Self::dash_h(cy, cw_d, ht),  // heavy dash
'\u{2508}' => Self::dash_v(cx, lh_d, t),
'\u{2509}' => Self::dash_v(cx, lh_d, ht),
```

> ⚠️ Chú ý: pattern 2-on/2-off là xấp xỉ; Windows Terminal có thể dùng pattern
> phức tạp hơn. Hiện tại phù hợp cho terminal TUI đơn giản.

### 19.4 Block Elements `U+2580–U+259F`

Commit `cae796d` mở rộng phạm vi sang 32 ký tự block. Các ký tự này được dùng
rất nhiều bởi các CLI hiện đại (pi, Nushell, lazygit, …) để vẽ progress bar,
input box padding, diff markers.

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

Các ký tự góc phần tư dùng tâm cell làm ranh giới:

```rust
'\u{2596}' => vec![(0, cy, cx, lh_d - cy)],           // ▖ lower-left
'\u{2597}' => vec![(cx, cy, cw_d - cx, lh_d - cy)],  // ▗ lower-right
'\u{2598}' => vec![(0, 0, cx, cy)],                  // ▘ upper-left
'\u{259D}' => vec![(cx, 0, cw_d - cx, cy)],          // ▝ upper-right
'\u{2599}' => vec![                                // ▙ 3 góc
    (0, 0, cx, cy),
    (0, cy, cx, lh_d - cy),
    (cx, 0, cw_d - cx, cy),
],
```

---

## 12. Paint loop: từ device pixel về logical pixel

Trong `TerminalElement::paint`, sau khi text runs đã vẽ xong:

```rust
let cw_d = (f32::from(cw) * scale_factor).round() as i32;
let lh_d = (f32::from(lh) * scale_factor).round() as i32;

for bd in &layout.box_draws {
    // Snap cell origin sang device pixel grid.
    let cell_x_logical = snap_px(f32::from(origin.x + bd.point.column as f32 * cw));
    let cell_y_logical = snap_px(f32::from(origin.y + bd.point.line as f32 * lh));

    // Mỗi rect là device px → convert về logical px để paint_quad.
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

Tại sao tính trong device pixel rồi convert ngược?

- `cw_d`, `lh_d` là integer → chia cell làm các phần integer (cx, cy, dl, dv).
- Các rect nằm khít pixel grid vật lý.
- Convert sang logical px bằng `/ scale_factor` để GPUI nhận tọa độ không bị lỗi
  rounding ngầm.

---

## 13. Fallback font cho ký tự không hỗ trợ

Các ký tự box-drawing sau **không được custom render** và fallback về font glyph:

- Đường chéo (`╱` U+2571, `╲` U+2572, `╳` U+2573).
- Quadruple-dash (`U+250A`, `U+250B`).
- Các ký tự shade block (`U+2591–U+2593` ░▒▓) — vì cần pattern, không phải fill.

Trong code:

```rust
match c {
    // ... các trường hợp đã xử lý ...
    _ => vec![],  // empty → layout_grid sẽ không đẩy BoxDrawCell
}
```

Và trong `layout_grid`:

```rust
if Self::is_box_drawing(cell.c)
    && !Self::box_drawing_rects(cell.c, 16, 16).is_empty()
{
    box_draws.push(BoxDrawCell { ... });
    continue;
}
```

`box_drawing_rects(cell.c, 16, 16)` là probe nhanh: nếu trả về empty thì char
không có custom geometry, để lại cho text batch như bình thường.

---

## 14. Tại sao hiệu quả?

| Khía cạnh | Font glyph | Custom primitive (myTerm2) |
|-----------|-----------|---------------------------|
| Anti-alias | Có, theo font hinting | Không — rects axis-aligned |
| Line 1 px | Có thể mờ nếu subpixel | Sharp 1 device px |
| Heavy 2 px | Font-dependent | Luôn 2 device px |
| Double line | Có thể chồng chéo | Hai rects tách biệt |
| Block halves | Có thể hở gap | Khít cell grid |
| HiDPI (1.5×) | Dễ bị blur | Snap device px |
| Batch | GPUI shape_line batch | Nhiều paint_quad nhỏ |

Mặc dù custom primitive sinh thêm draw call, nhưng đối với terminal TUI hiện đại
số lượng box-drawing/block cell trên màn hình thường rất nhỏ so với text thường.
Đánh đổi sharpness rất đáng giá.

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

# Nushell prompt / pi CLI — sử dụng ▀ ▄ ▌ trong thực tế
```

### Kiểm tra

1. Các đường khung thẳng, không mờ.
2. Góc `┌` và `└` khít nhau khi ghép.
3. `▀` và `▄` không để lại khe hở.
4. Ở scale factor 1.5× hoặc 2.0×, đường 1 px vẫn sharp.

---

## 16. References

| Source | Path / URL | Relevance |
|--------|-----------|-----------|
| Windows Terminal AtlasEngine | `src/renderer/atlas/AtlasEngine.cpp` | Custom box-drawing primitive |
| Zed terminal_element | `crates/terminal_ui/src/terminal_element.rs` | Inspiration for layout + paint split |
| Unicode Box Drawing | `U+2500–U+257F` | 128 characters |
| Unicode Block Elements | `U+2580–U+259F` | 32 characters |
| myTerm2 `terminal_element.rs` | `crates/ui/src/views/terminal/terminal_element.rs` | Implementation |

---

---

## 17. Bối cảnh — Vấn đề cần giải quyết

### Triệu chứng

Prompt line / input area trong `myTerm2` trông khác Windows Terminal:

1. **Cursor mỏng thay vì block đầy.** Shell (Nushell / reedline) gửi `DECSCUSR`
   để set Beam cursor. `TerminalElement` trước đây dùng `snapshot.cursor.shape`
   từ shell → luôn Beam, bỏ qua `cursor.shape = "block"` trong `terminal.json`.
2. **Cell width sai.** Auto width đo advance của `'m'` (CSS `em`), thay vì `'0'`
   (CSS `ch`). `'m'` thường rộng hơn `'0'` ~10% → cell quá rộng, text bị co.
3. **Line height có thể clip text.** `line_height = font_size * factor` không
   đảm bảo ≥ `ascent + descent` → nếu factor thấp, glyph bị cắt đỉnh/đáy.
4. **Cursor block có subpixel gap.** `size(cw, lh)` không snap width lên device
   pixel → để lại khe hở giữa cursor và cell kế bên.

### So sánh với Windows Terminal

| Khía cạnh | Windows Terminal | myTerm2 (trước) | myTerm2 (sau) |
|---|---|---|---|
| Cell width | `round(advance('0'))` — CSS `ch` | `advance('m')` hoặc override `8.0` | `ch_advance('0')` ✅ |
| Line height | `round(ascent + descent + lineGap)` | `font_size * factor` | `max(factor * font_size, ascent + descent)` ✅ |
| Cursor shape | User config override shell | Shell snapshot (Beam) | Config override (trừ Hidden) ✅ |
| Cursor block fill | `ceil_px(cell_width)` snap | `cw` (logical, subpixel gap) | `ceil_px(cw)` ✅ |
| Baseline center | `round(ascent + (lineGap + adjustedHeight - advanceHeight) / 2)` | GPUI `paint_line` tự center | Không đổi ✅ |

---

## 18. Cell Width — CSS `ch` Unit (Advance Width của `'0'`)

### 36.1. Tại sao `'0'` thay vì `'m'`?

- CSS `ch` unit = advance width của `'0'` (CSS Values and Units § 4).
- Windows Terminal AtlasEngine dùng cùng ký tự `'0'` (comment trong
  `AtlasEngine.api.cpp`).
- Monospace font: `'0'` advance = cell width chuẩn; `'m'` có thể rộng hơn
  do stem dày.
- `'0'` luôn tồn tại trong mọi monospace font (ASCII 0x30).

### 36.2. Triển khai trong `TerminalElement::prepaint`

```rust
let scale_factor = window.scale_factor().max(1.0);
let snap_px = |value: f32| -> f32 { (value * scale_factor).round() / scale_factor };

let font_id = cx.text_system().resolve_font(&self.font);
let font_px = self.font_size;

let cell_width = if let Some(cw) = self.cell_width_override {
    // User override trong terminal.json → snap sang device pixel.
    px(snap_px(cw))
} else {
    // Windows Terminal / CSS ch unit: đo advance width của '0'.
    let raw = cx
        .text_system()
        .ch_advance(font_id, font_px)
        .map(|s| f32::from(s))
        .unwrap_or_else(|_| {
            // Fallback: đo 'm' advance nếu '0' không có glyph.
            cx.text_system()
                .advance(font_id, font_px, 'm')
                .map(|s| f32::from(s.width))
                .unwrap_or(8.0)
        });
    px(snap_px(raw))
};
```

### 36.3. Config default

Trong `crates/ui/src/state/terminal_config.rs`:

```rust
/// Cell width override in px (null = auto từ advance width của '0',
/// giống Windows Terminal / CSS ch unit).
#[serde(default = "default_cell_width")]
pub cell_width: Option<f32>,

fn default_cell_width() -> Option<f32> {
    None // auto: đo advance width của '0' (CSS ch unit, giống Windows Terminal)
}
```

Mặc định cũ là `Some(8.0)` — bị xóa. Giờ user config `null` sẽ tự động đo
font, khớp Windows Terminal.

---

## 19. Line Height — Font Metrics Minimum

### 37.1. Vấn đề

Cấu hình `layout.line_height` là multiplier (ví dụ `1.15`). Nếu tính đơn giản:

```rust
let line_height = px(font_size * line_height_factor);
```

với factor nhỏ (ví dụ `1.0`) có thể nhỏ hơn `ascent + descent` của glyph →
text bị cắt đỉnh/đáy.

### 37.2. Giải pháp: `max(factor, natural)`

GPUI expose `TextSystem::ascent` / `TextSystem::descent` (tương đương
`DWRITE_FONT_METRICS::ascent` / `descent`):

```rust
let font_ascent = cx.text_system().ascent(font_id, font_px);
let font_descent = cx.text_system().descent(font_id, font_px);
let natural_line_height = f32::from(font_ascent) + f32::from(font_descent);
let factor_height = f32::from(font_px) * self.line_height_factor;

// max(factor_height, natural_line_height) → không bao giờ clip.
let line_height = px(snap_px(factor_height.max(natural_line_height)));
```

### 37.3. Tại sao chỉ cần `ascent + descent`?

Windows Terminal tính `advanceHeight = ascent + descent + lineGap`. GPUI
không expose `lineGap`, nhưng `line_height_factor` mặc định (`1.15` ~ `1.2`)
bù khoảng trống line gap.

Hơn nữa, GPUI `paint_line` tự center text trong `line_height` dựa trên
`layout.ascent` / `layout.descent` của shaped line:

```rust
let padding_top = (line_height - layout.ascent - layout.descent) / 2.;
let baseline_offset = point(px(0.), padding_top + layout.ascent);
```

→ Chỉ cần `line_height >= ascent + descent` là text không bị clip và baseline
căn giữa tự động.

---

## 20. Cursor Shape Override — User Config Thắng Shell

### 38.1. Vấn đề

`alacritty_terminal` nhận escape sequence `DECSCUSR` (`\x1b[5 q`) từ shell và
lưu shape vào `TerminalContent::cursor.shape`. `TerminalElement` dùng shape
này để paint. Kết quả: `terminal.json` đặt `cursor.shape = "block"` nhưng
shell vẫn buộc Beam.

### 38.2. Nguyên lý giống Windows Terminal

Windows Terminal tôn trọng user setting `cursorShape` trong `profile.json` —
shell không thể override. `myTerm2` áp dụng cùng nguyên lý:

- `snapshot.cursor.shape == Hidden` → ẩn cursor (shell explicitly hide).
- Ngược lại → dùng `cursor_shape_override` từ config (Block / Bar / Underline).

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
        settings.cursor_shape,  // truyền xuống element
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

// Trong prepaint:
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

### 38.5. Lưu ý quan trọng

- Shell vẫn có thể **ẩn** cursor qua `Hidden`.
- Shell **không thể** set Beam / Block / Underline — quyền này thuộc về user config.
- Điều này khớp Windows Terminal: `cursorShape` trong profile là canonical.

---

## 21. Cursor Paint — Device Pixel Snap cho Block/Bar/Underline

Trong `TerminalElement::paint`:

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

### Tại sao `ceil_px` cho width?

- `snap_px` = `floor(x * scale) / scale` → align trái.
- `ceil_px` = `ceil(x * scale) / scale` → width ≥ logical width, khít cell bên
  phải, không subpixel gap.
- Đặc biệt quan trọng với cursor **Block** để fill đầy cell và khớp grid.

---

## 22. Cấu trúc Code — Data Flow Tổng Hợp

```
terminal.json (user config)
    │
    ▼
TerminalConfig (serde deserialize)
    │  layout.cell_width: Option<f32>     (None = auto)
    │  layout.line_height: f32            (factor, mặc định 1.15)
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
    └─ snap all metrics → device pixel grid (Phần 1)
    ▼
TerminalElement::paint()
    │
    ├─ bg rects (snap origin + ceil width)
    ├─ selection rects
    ├─ text runs (shape_line + paint, GPUI tự center)
    ├─ box-drawing primitives (Phần 2)
    └─ cursor (snap origin + ceil width)
```

---

## 23. Config Reference

### `terminal.json` — Layout + Cursor

```jsonc
{
  "cursor": {
    "shape": "block",      // "block" | "bar" | "underline" — override shell
    "blink": true,          // nhấp nháy khi focus
    "color": null           // null = theme caret, "#RRGGBB" = override
  },
  "layout": {
    "line_height": 1.15,    // factor × font_size, tối thiểu = ascent + descent
    "cell_width": null,     // null = auto (advance '0'), số = override px
    "padding": { "top": 0, "right": 5, "bottom": 0, "left": 10 }
  }
}
```

### Defaults

| Tham số | Giá trị mặc định | Ghi chú |
|---|---|---|
| `cursor.shape` | `"block"` | Override shell `DECSCUSR` |
| `layout.line_height` | `1.15` | Factor, bù lineGap |
| `layout.cell_width` | `null` | Auto = `ch_advance('0')` |
| `cursor.blink` | `true` | 500ms interval |

---

## 24. Testing

### Unit tests (`terminal_config.rs`)

```rust
// Default cell_width = None (auto)
assert_eq!(cfg.layout.cell_width, None);

// Custom override vẫn hoạt động
let json = r#"{ "layout": { "cell_width": 8.0 } }"#;
let cfg: TerminalConfig = serde_json::from_str(json).unwrap();
assert_eq!(cfg.layout.cell_width, Some(8.0));
```

### Visual verification

1. Cursor shape: set `cursor.shape = "block"` → cursor là block đầy, không
   phải Beam do shell set.
2. Cell width: set `cell_width = null` → cell khớp font advance, text không bị co.
3. Line height: set `line_height = 1.0` → text không bị clip.
4. Scale 1.5: tất cả metrics snap device pixel → không jitter khi resize.

---

## 25. Tham chiếu

| Nguồn | Path / URL | Phần liên quan |
|---|---|---|
| Windows Terminal AtlasEngine | `src/renderer/atlas/AtlasEngine.api.cpp` | `_resolveFontMetrics()` |
| GPUI TextSystem | `crates/gpui/src/text_system.rs` | `ch_advance`, `ascent`, `descent` |
| GPUI paint_line | `crates/gpui/src/text_system/line.rs` | `padding_top`, `baseline_offset` |
| CSS ch unit | CSS Values and Units § 4 | `ch` = advance width of '0' |
| DirectWrite metrics | `DWRITE_FONT_METRICS` | `ascent`, `descent`, `lineGap` |
| Alacritty CursorShape | `alacritty_terminal::vte::ansi::CursorShape` | Block/Beam/Underline/Hidden |

---

## 26. Data flow tổng hợp

```
terminal.json (user config)
    │
    ▼
TerminalConfig (serde deserialize)
    │  layout.cell_width: Option<f32>     (None = auto)
    │  layout.line_height: f32            (factor, mặc định 1.15)
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
    ├─ rows/cols tính trong device pixels
    ├─ grid_origin snap sang device pixel
    └─ all metrics snap → device pixel grid
    ▼
TerminalElement::paint()
    │
    ├─ bg rects (floor origin + ceil width)
    ├─ selection rects
    ├─ text runs (shape_line + paint, GPUI tự center)
    ├─ box-drawing / block primitives (integer device px)
    └─ cursor (floor origin + ceil width)
```

---

## 27. So sánh với Windows Terminal AtlasEngine

| Khía cạnh | Windows Terminal | myTerm2 (sau commits) |
|---|---|---|
| Device pixel snap | Tọa độ vertex integer device px | Snap logical px trước paint (tương đương) |
| Box-drawing | Custom AtlasEngine primitive | Custom fill-rect primitives |
| Block elements | Custom AtlasEngine primitive | Custom fill-rect primitives |
| Cell width | `round(advance('0'))` | `ch_advance('0')` + round |
| Line height | `round(ascent+descent+lineGap)` | `max(factor*font_size, ascent+descent)` |
| Cursor shape | User config override shell | Config override (trừ Hidden) |
| Cursor block fill | `ceil_px(cell_width)` | `ceil_px(cell_width)` |
| Baseline center | `round(ascent + ...)` | GPUI `paint_line` tự center |

---

## 28. Config reference

```jsonc
{
  "cursor": {
    "shape": "block",      // "block" | "bar" | "underline" — override shell
    "blink": true,          // nhấp nháy khi focus
    "color": null           // null = theme caret, "#RRGGBB" = override
  },
  "layout": {
    "line_height": 1.15,    // factor × font_size, tối thiểu = ascent + descent
    "cell_width": null,     // null = auto (advance '0'), số = override px
    "padding": { "top": 0, "right": 5, "bottom": 0, "left": 10 }
  }
}
```

### Defaults

| Tham số | Giá trị mặc định | Ghi chú |
|---|---|---|
| `cursor.shape` | `"block"` | Override shell `DECSCUSR` |
| `layout.line_height` | `1.15` | Factor, bù lineGap |
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
# Set cursor.shape = "block" → cursor phải là block đầy dù shell set Beam.
```

### Kiểm tra

1. Đường khung sharp, không mờ ở scale 1.5×/2.0×.
2. `▀`/`▄`/`▌` khít cell, không khe hở.
3. `cell_width = null` → cell khớp font advance, text không bị co.
4. `line_height = 1.0` → text không bị clip.
5. `cursor.shape = "block"` → block cursor, shell không override.

---

## 30. References

| Nguồn | Path / URL | Phần liên quan |
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
