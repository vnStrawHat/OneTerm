# Cell Metrics & Cursor Rendering — Kỹ thuật đạt chất lượng Windows Terminal

> Tài liệu kỹ thuật mô tả phương pháp tính cell width / line height / cursor shape
> dựa trên font metrics thật (giống Windows Terminal AtlasEngine), thay vì heuristic
> thủ công. Áp dụng cho `TerminalElement` — custom GPUI Element render terminal grid.
>
> **Tham chiếu chính**:
> - Windows Terminal: `src/renderer/atlas/AtlasEngine.api.cpp` — `_resolveFontMetrics()`.
> - GPUI: `crates/gpui/src/text_system.rs` — `TextSystem::ch_advance`, `ascent`, `descent`.
> - CSS: `ch` unit = advance width của `'0'`.
>
> **Ngày áp dụng**: 2025-07-15 (commit sau `2f2f318`).

---

## 1. Bối cảnh — Vấn đề cần giải quyết

### Triệu chứng

Khu vực ô user input (prompt line) trông khác Windows Terminal:

1. **Cursor mỏng thay vì block đầy** — Shell (Nushell/reedline) gửi `DECSCUSR`
   (Set Cursor Style) để set Beam cursor. `TerminalElement` dùng `snapshot.cursor.shape`
   từ shell → luôn Beam, bỏ qua `cursor.shape = "block"` trong `terminal.json`.

2. **Cell width sai** — Auto cell width đo advance của `'m'` (CSS `em`), thay vì `'0'`
   (CSS `ch`). `'m'` thường rộng hơn `'0'` ~10% → cell quá rộng, text bị co lại.

3. **Line height có thể clip text** — `line_height = font_size * factor` không đảm bảo
   ≥ `ascent + descent` → nếu factor quá thấp, glyph bị cắt đỉnh/đáy.

4. **Cursor block có subpixel gap** — `size(cw, lh)` không snap width lên device pixel
   → rasterize để lại khe hở 1 subpixel giữa cursor và cell kế bên.

### So sánh với Windows Terminal

| Khía cạnh | Windows Terminal | myTerm2 (trước) | myTerm2 (sau) |
|---|---|---|---|
| Cell width | `round(advance('0'))` — CSS `ch` | `advance('m')` hoặc override `8.0` | `ch_advance('0')` — ✅ khớp |
| Line height | `round(ascent + descent + lineGap)` | `font_size * factor` | `max(factor * font_size, ascent + descent)` — ✅ không clip |
| Cursor shape | User config override shell | Shell snapshot (Beam) | Config override (trừ Hidden) — ✅ |
| Cursor block fill | `ceil_px(cell_width)` snap | `cw` (logical, subpixel gap) | `ceil_px(cw)` — ✅ khít |
| Baseline centering | `round(ascent + (lineGap + adjustedHeight - advanceHeight) / 2)` | GPUI `paint_line` tự center | Không đổi (GPUI đã center) — ✅ |

---

## 2. Phương pháp — Font Metrics từ GPUI TextSystem

### 2.1. Cell Width: CSS `ch` unit (advance của `'0'`)

**Windows Terminal** (`AtlasEngine._resolveFontMetrics`):
```cpp
// We use the same character to determine the advance width as CSS for its "ch" unit ("0").
auto advanceWidth = 0.5f * fontSizeInPx;  // fallback
{
    static constexpr u32 codePoint = '0';
    u16 glyphIndex;
    primaryFontFace->GetGlyphIndicesW(&codePoint, 1, &glyphIndex);
    if (glyphIndex) {
        DWRITE_GLYPH_METRICS glyphMetrics{};
        primaryFontFace->GetDesignGlyphMetrics(&glyphIndex, 1, &glyphMetrics, FALSE);
        advanceWidth = static_cast<f32>(glyphMetrics.advanceWidth) * designUnitsPerPx;
    }
}
auto adjustedWidth = std::roundf(advanceWidth);
```

**myTerm2** (GPUI `TextSystem::ch_advance`):
```rust
// text_system.rs — GPUI đã expose sẵn:
pub fn ch_advance(&self, font_id: FontId, font_size: Pixels) -> Result<Pixels> {
    Ok(self.advance(font_id, font_size, '0')?.width)
}
```

**TerminalElement::prepaint**:
```rust
let cell_width = if let Some(cw) = self.cell_width_override {
    px(snap_px(cw))  // user override
} else {
    // Windows Terminal / CSS ch unit: advance width của '0'
    let raw = cx.text_system()
        .ch_advance(font_id, font_px)
        .map(|s| f32::from(s))
        .unwrap_or_else(|_| {
            // Fallback: 'm' advance nếu '0' không có glyph
            cx.text_system()
                .advance(font_id, font_px, 'm')
                .map(|s| f32::from(s.width))
                .unwrap_or(8.0)
        });
    px(snap_px(raw))  // snap sang device pixel
};
```

**Tại sao `'0'` thay vì `'m'`?**
- CSS `ch` unit = advance width của `'0'` (CSS Values and Units §4).
- Windows Terminal dùng cùng ký tự `'0'` (xem comment trong code).
- Monospace font: `'0'` advance = cell width chuẩn, `'m'` có thể rộng hơn do stem.
- `'0'` luôn tồn tại trong mọi monospace font (ASCII 0x30).

### 2.2. Line Height: Font Metrics Minimum

**Windows Terminal**:
```cpp
const auto advanceHeight = ascent + descent + lineGap;
auto adjustedHeight = std::roundf(fontInfoDesired.GetCellHeight()
    .Resolve(advanceHeight, dpi, fontSizeInPx, advanceWidth));
// Protection: cell size >= 1
adjustedHeight = std::max(1.0f, adjustedHeight);
```

**myTerm2** (GPUI không expose `line_gap`, nhưng có `ascent` + `descent`):
```rust
// TextSystem::ascent / descent
let font_ascent = cx.text_system().ascent(font_id, font_px);
let font_descent = cx.text_system().descent(font_id, font_px);
let natural_line_height = f32::from(font_ascent) + f32::from(font_descent);
let factor_height = f32::from(font_px) * self.line_height_factor;
// max(factor_height, natural_line_height) → không bao giờ clip
let line_height = px(snap_px(factor_height.max(natural_line_height)));
```

**Tại sao `max(factor, natural)`?**
- `line_height_factor` (config, mặc định 1.2) cho phép user tăng line spacing.
- `natural_line_height = ascent + descent` là tối thiểu để glyph không bị clip.
- `max()` đảm bảo cả hai: spacing tùy chỉnh + không clip.
- Windows Terminal thêm `lineGap` (font-reported line gap), GPUI không expose.
  `line_height_factor` bù khoảng trống này — 1.2 ≈ 1.0 + lineGap/em.

### 2.3. Baseline Centering

**Windows Terminal**:
```cpp
const auto baseline = std::roundf(ascent + (lineGap + adjustedHeight - advanceHeight) / 2.0f);
```

**GPUI** (`paint_line` trong `text_system/line.rs`):
```rust
let padding_top = (line_height - layout.ascent - layout.descent) / 2.;
let baseline_offset = point(px(0.), padding_top + layout.ascent);
```

→ GPUI tự center text trong `line_height` dựa trên `layout.ascent` + `layout.descent`
của shaped line. Không cần can thiệp thủ công — chỉ cần đảm bảo `line_height >= ascent + descent`.

---

## 3. Kỹ thuật — Cursor Shape Override

### 3.1. Vấn đề

Shell (Nushell, bash, vim) gửi escape sequence `DECSCUSR` (\x1b[5 q) để set cursor
shape. `alacritty_terminal` lưu shape vào `TerminalContent::cursor.shape`.
`TerminalElement` đọc shape này để paint.

**Vấn đề**: User config `cursor.shape = "block"` bị bỏ qua — shell override.

### 3.2. Giải pháp: Config Override (giống Windows Terminal)

Windows Terminal tôn trọng user setting `cursorShape` trong `profile.json` —
shell không thể override. myTerm2 áp dụng cùng nguyên lý:

```rust
// TerminalElement struct — thêm field
cursor_shape_override: crate::state::TerminalCursorShape,

// TerminalElement::prepaint — build CursorPaint
let shape = match self.cursor_shape_override {
    TerminalCursorShape::Block => CursorShape::Block,
    TerminalCursorShape::Bar => CursorShape::Beam,
    TerminalCursorShape::Underline => CursorShape::Underline,
};
Some(CursorPaint { point, color, shape })
// Chỉ giữ shape = Hidden từ shell (shell có thể ẩn cursor khi không cần)
if c.shape == CursorShape::Hidden { None } else { ... }
```

**Quy tắc**:
- `snapshot.cursor.shape == Hidden` → ẩn cursor (shell explicitly hide).
- Ngược lại → dùng `cursor_shape_override` từ config (Block/Bar/Underline).
- Shell không thể set Beam/Block/Underline — chỉ user config quyết định.

### 3.3. Cursor Block Fill — Device Pixel Snap

```rust
CursorShape::Block => {
    // Snap width lên device pixel để khít grid
    size(px(ceil_px(f32::from(cw))), lh)
}
```

`snap_px` = `floor(x * scale) / scale` (align trái).
`ceil_px` = `ceil(x * scale) / scale` (align phải, đảm bảo width ≥ cell width).

→ Block cursor fill khít cell, không subpixel gap giữa cursor và cell kế bên.

---

## 4. Device Pixel Snapping — Cơ chế

### 4.1. Nguyên lý

Terminal rendering là grid-based. Nếu cell metrics là float logical px (vd 9.0px ở
scale 1.5 → 13.5 device px), các dòng/cột nằm ở tọa độ subpixel → glyph rasterize
bị anti-alias không nhất quán → đường kẻ/box-drawing nhòe.

**Giải pháp**: Snap tất cả cell metrics + tọa độ paint sang device pixel grid.

### 4.2. Triển khai

```rust
let scale_factor = window.scale_factor().max(1.0);

// Snap xuống (floor) — cho origin, line_height, cell_width
let snap_px = |value: f32| -> f32 { (value * scale_factor).round() / scale_factor };

// Snap lên (ceil) — cho width/height cần khít cell (cursor, bg rect)
let ceil_px = |value: f32| -> f32 { (value * scale_factor).ceil() / scale_factor };
```

**Ứng dụng**:
- `cell_width`: `snap_px(raw)` — round → nearest device pixel.
- `line_height`: `snap_px(factor_height.max(natural))` — round.
- `cursor pos`: `snap_px(origin + col * cw)` — floor để align trái.
- `cursor size`: `ceil_px(cw)` — ceil để width ≥ cell width, khít cell kế bên.
- `bg rect size`: `ceil_px(cw * num_cells)` — ceil để fill đầy.
- `box-drawing`: `(cw * scale).round() as i32` — device pixel integer.

### 4.3. So sánh với Windows Terminal AtlasEngine

Windows Terminal snap ở tầng DirectX (vertex coordinates = integer device px).
myTerm2 snap ở tầng logical px (trước khi gửi GPUI paint). Kết quả tương đương:
- Glyph rasterize khít pixel grid → không AA blur.
- Box-drawing line 1 device px → sharp.
- Cursor block không có khe hở subpixel.

---

## 5. Cấu trúc Code — Data Flow

```
terminal.json (user config)
    │
    ▼
TerminalConfig (serde deserialize)
    │  layout.cell_width: Option<f32>  (None = auto)
    │  layout.line_height: f32        (factor, mặc định 1.15)
    │  cursor.shape: String           ("block" | "bar" | "underline")
    │
    ▼
TerminalSettings (apply_config)
    │  cell_width: Option<f32>
    │  line_height_factor: f32
    │  cursor_shape: TerminalCursorShape
    │
    ▼
TerminalView::render()
    │  settings.cursor_shape → cursor_shape
    │  settings.cell_width → cell_width_override
    │  settings.line_height_factor → line_height_factor
    │
    ▼
TerminalElement::new(..., cursor_shape_override, cell_width_override, ...)
    │
    ▼
TerminalElement::prepaint()
    │
    ├─ font_id = cx.text_system().resolve_font(&self.font)
    │
    ├─ cell_width = cell_width_override
    │              ?? ch_advance(font_id, font_px)  // '0' advance
    │              ?? advance('m')                  // fallback
    │
    ├─ line_height = max(font_px * line_height_factor,
    │                     ascent(font_id) + descent(font_id))
    │
    ├─ cursor.shape = match cursor_shape_override { ... }  // config override
    │
    └─ snap all metrics → device pixel grid
    │
    ▼
TerminalElement::paint()
    │
    ├─ bg rects (snap origin + ceil width)
    ├─ selection rects
    ├─ text runs (shape_line + paint, GPUI tự center)
    ├─ box-drawing primitives (device px integer)
    └─ cursor (snap origin + ceil width for Block)
```

---

## 6. Config Reference

### `terminal.json` — Layout + Cursor

```jsonc
{
  "cursor": {
    "shape": "block",      // "block" | "bar" | "underline" — override shell
    "blink": true,          // nhấp nháy khi focus
    "color": null           // null = theme caret, "#RRGGBB" = override
  },
  "layout": {
    "line_height": 1.2,     // factor × font_size, tối thiểu = ascent + descent
    "cell_width": null,     // null = auto (advance '0'), số = override px
    "padding": { "top": 0, "right": 5, "bottom": 0, "left": 10 }
  }
}
```

### Defaults

| Tham số | Giá trị mặc định | Ghi chú |
|---|---|---|
| `cursor.shape` | `"block"` | Override shell DECSCUSR |
| `layout.line_height` | `1.15` | Factor, bù lineGap |
| `layout.cell_width` | `null` | Auto = `ch_advance('0')` |
| `cursor.blink` | `true` | 500ms interval |

---

## 7. Testing

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

1. Cursor shape: Set `cursor.shape = "block"` → cursor phải là block đầy, không
   phải thanh mỏng (Beam) do shell set.
2. Cell width: Set `cell_width = null` → cell khớp font advance, text không bị co.
3. Line height: Set `line_height = 1.0` → text không bị clip (minimum = ascent + descent).
4. Box-drawing: `echo -e '\xe2\x94\x8c\xe2\x94\x80\xe2\x94\x90'` (┌─┐) → đường sharp,
   không AA blur.
5. Scale 1.5: Tất cả metrics snap device pixel → không jitter khi resize.

---

## 8. Tham chiếu

| Nguồn | URL / Path | Phần liên quan |
|---|---|---|
| Windows Terminal AtlasEngine | `src/renderer/atlas/AtlasEngine.api.cpp` | `_resolveFontMetrics()` |
| GPUI TextSystem | `crates/gpui/src/text_system.rs` | `ch_advance`, `ascent`, `descent` |
| GPUI paint_line | `crates/gpui/src/text_system/line.rs` | `padding_top`, `baseline_offset` |
| CSS ch unit | CSS Values and Units §4 | `ch` = advance width of '0' |
| DirectWrite metrics | `DWRITE_FONT_METRICS` | `ascent`, `descent`, `lineGap` |
| Alacritty CursorShape | `alacritty_terminal::vte::ansi::CursorShape` | Block/Beam/Underline/Hidden |