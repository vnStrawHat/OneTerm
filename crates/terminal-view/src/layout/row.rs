//! Per-row layout — turns the cells of one display row into background
//! rects, batched text runs, and box-drawing primitives, plus the per-cell
//! helpers it is built from (blank detection, colour resolution with the
//! semantic merge policy, `TextRun` construction, and the FNV-1a line hash
//! that detects content changes `Term::damage()` does not track).

use std::mem;

use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use gpui::{Font, FontStyle, FontWeight, Hsla, Pixels, ShapedLine, TextRun, UnderlineStyle};

use oneterm_highlight::{Class, Decoration};
use oneterm_terminal::{
    IndexedCell, is_app_chosen_exact_color, is_decorative_character, is_default_background_color,
};

use super::super::box_drawing::block::is_full_width_band;
use super::super::box_drawing::drawing::{has_box_geometry, is_box_drawing};
use super::super::highlight::to_gpui_hsla;
use super::super::theme::{TerminalTheme, ensure_minimum_contrast, resolve_cell_color};
use super::types::{BatchedTextRun, BoxDrawCell, LayoutPoint, LayoutRect, RowLayout};

/// Lay out a single display row — build rects + text runs + box draws for the
/// cells on one line.
///
/// `cell_class` is the per-column semantic class (from the scanner + URL mask).
/// It replaces the old `url_mask: &[bool]` — `Class::Url` is one variant.
pub(crate) fn layout_row(
    line_cells: Vec<&IndexedCell>,
    display_line: i32,
    theme: &TerminalTheme,
    base_font: &Font,
    cell_class: &[u8],
) -> RowLayout {
    let mut rects: Vec<LayoutRect> = Vec::new();
    let mut runs: Vec<BatchedTextRun> = Vec::new();
    let mut box_draws: Vec<BoxDrawCell> = Vec::new();
    let mut current_batch: Option<BatchedTextRun> = None;
    let mut prev_had_extras = false;
    // Reusable scratch buffer for the box-geometry probe — cleared and refilled
    // per cell, but keeps its backing allocation across all cells in this row
    // (no per-cell `Vec` allocation on full-screen block workloads).
    let mut box_probe: Vec<(i32, i32, i32, i32)> = Vec::new();

    for ic in line_cells {
        let point = ic.point;
        let cell = &ic.cell;
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        if cell.c == ' ' && prev_had_extras {
            prev_had_extras = false;
            continue;
        }
        prev_had_extras = matches!(cell.zerowidth(), Some(c) if !c.is_empty());

        let lp = LayoutPoint {
            line: display_line,
            column: point.column.0 as i32,
        };

        if is_blank(cell) {
            continue;
        }

        // ── Semantic class for this column ──
        let cls_byte = cell_class
            .get(point.column.0)
            .copied()
            .unwrap_or(Class::Default as u8);
        let class_style = theme.class_styles.style(cls_byte);

        let (fg, bg) = cell_colors(cell, theme, cls_byte);

        if !is_default_background_color(&cell.bg) || cell.flags.contains(Flags::INVERSE) {
            let col = point.column.0 as i32;
            let merged = if let Some(last) = rects.last_mut() {
                if last.color == bg
                    && last.point.line == display_line
                    && last.point.column + last.num_cells as i32 == col
                {
                    last.num_cells += 1;
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if !merged {
                rects.push(LayoutRect {
                    point: LayoutPoint {
                        line: display_line,
                        column: col,
                    },
                    num_cells: 1,
                    color: bg,
                });
            }
        }

        let mut style: TextRun = cell_style(cell, fg, base_font);

        // ── Apply class decorations (additive) ──
        // Underline for Error/Warn/Url etc. — additive on top of ANSI fg.
        if class_style.deco == Decoration::Underline && style.underline.is_none() {
            style.underline = Some(UnderlineStyle {
                color: Some(style.color),
                thickness: gpui::px(1.0),
                wavy: false,
            });
        }

        // ── Apply class font style (additive OR, never removes) ──
        if class_style.font.bold {
            style.font.weight = FontWeight::BOLD;
        }
        if class_style.font.italic {
            style.font.style = FontStyle::Italic;
        }

        let zw = cell.zerowidth();

        if is_box_drawing(cell.c) && has_box_geometry(&mut box_probe, cell.c) {
            // Coalesce runs of identical full-width band glyphs (▀▄█…) sharing the
            // same colour into one stretched rect — the block analogue of the bg
            // run merge above. Partial-width/quadrant glyphs are never merged.
            let merged = if is_full_width_band(cell.c) {
                if let Some(last) = box_draws.last_mut() {
                    if last.c == cell.c
                        && last.color == style.color
                        && last.point.line == display_line
                        && last.point.column + last.num_cells as i32 == lp.column
                    {
                        last.num_cells += 1;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            if !merged {
                box_draws.push(BoxDrawCell {
                    point: lp,
                    color: style.color,
                    c: cell.c,
                    num_cells: 1,
                });
            }
            // Flush the active text batch so the next real-text segment starts a
            // fresh run at its own absolute column. Do NOT emit a space-only run
            // for the block cell: each run is painted at an absolute column
            // origin (`cell_x(run.start.column)` in paint), so terminating the
            // run here keeps positioning correct. A space filler would instead
            // force one `shape_line` per block cell — the dominant cost on
            // full-screen block workloads (DOOM-fire), where the fire gradient
            // gives every cell a unique color so the spaces never batch.
            if let Some(old) = current_batch.take() {
                runs.push(old);
            }
            continue;
        }

        match current_batch.as_mut() {
            // Extend the active run when the next cell shares its style and is
            // immediately adjacent on the same line.
            Some(b)
                if b.can_append(&style)
                    && b.start.line == lp.line
                    && b.start.column + b.cell_count as i32 == lp.column =>
            {
                b.append_char(cell.c);
                if let Some(cs) = zw {
                    for &c in cs {
                        b.append_zw(c);
                    }
                }
            }
            // Otherwise flush the active run (if any) and start a fresh one.
            _ => {
                if let Some(old) = current_batch.take() {
                    runs.push(old);
                }
                let mut nb = BatchedTextRun::new(lp, cell.c, style);
                if let Some(cs) = zw {
                    for &c in cs {
                        nb.append_zw(c);
                    }
                }
                current_batch = Some(nb);
            }
        }
    }
    if let Some(b) = current_batch {
        runs.push(b);
    }
    RowLayout {
        rects,
        runs,
        box_draws,
        shaped_lines: Vec::new(),
        prev_hash: 0,
    }
}

// ── Per-cell helpers ──────────────────────────────────────────────────

/// Blank cell = space + default bg + no extras.
pub(crate) fn is_blank(cell: &Cell) -> bool {
    cell.c == ' '
        && is_default_background_color(&cell.bg)
        && cell.hyperlink().is_none()
        && !cell.flags.intersects(
            Flags::INVERSE | Flags::ALL_UNDERLINES | Flags::STRIKEOUT | Flags::WIDE_CHAR_SPACER,
        )
}

/// Whether the cell's foreground is the terminal default (no explicit SGR fg).
fn is_default_foreground(fg: &Color) -> bool {
    matches!(
        fg,
        Color::Named(NamedColor::Foreground)
            | Color::Named(NamedColor::BrightForeground)
            | Color::Named(NamedColor::DimForeground)
    )
}

/// Convert cell → (fg Hsla, bg Hsla) after inverse + contrast + dim + semantic merge.
///
/// After ANSI/SGR resolution, applies the **semantic merge policy** (Layer 2):
/// class fg overrides only the *default* foreground (no SGR); explicit ANSI fg
/// is kept. Decorations and font styles are applied additively in `layout_row`.
///
/// `class` is the semantic class byte (from `cell_class`). The merge policy:
/// - Default fg + non-Default class → class fg (the headline case).
/// - Explicit ANSI fg + non-Default class → keep ANSI fg (unless `override_ansi`).
/// - Default fg + Default class → theme fg.
pub(crate) fn cell_colors(cell: &Cell, theme: &TerminalTheme, class: u8) -> (Hsla, Hsla) {
    let mut fg = cell.fg;
    let mut bg = cell.bg;
    if cell.flags.contains(Flags::INVERSE) {
        mem::swap(&mut fg, &mut bg);
    }
    let mut fg_h = resolve_cell_color(&fg, theme);
    let bg_h = resolve_cell_color(&bg, theme);

    // ── Semantic merge (Layer 2) ──
    let class_style = theme.class_styles.style(class);
    if let Some(class_fg) = class_style.fg {
        let is_default = is_default_foreground(&fg);
        if is_default || class_style.override_ansi {
            fg_h = to_gpui_hsla(class_fg);
        }
    }

    if !is_app_chosen_exact_color(&fg) && !is_decorative_character(cell.c) {
        fg_h = ensure_minimum_contrast(fg_h, bg_h, theme.min_contrast);
    }
    if cell.flags.contains(Flags::DIM) {
        fg_h.a *= 0.7;
    }
    (fg_h, bg_h)
}

/// Build a `TextRun` for a cell (bold/italic/underline/strikethrough).
pub(crate) fn cell_style(cell: &Cell, fg: Hsla, base_font: &Font) -> TextRun {
    let underline = (cell.flags.intersects(Flags::ALL_UNDERLINES) || cell.hyperlink().is_some())
        .then(|| UnderlineStyle {
            color: Some(fg),
            thickness: gpui::px(1.0),
            wavy: cell.flags.contains(Flags::UNDERCURL),
        });
    let strikethrough = cell
        .flags
        .contains(Flags::STRIKEOUT)
        .then(|| gpui::StrikethroughStyle {
            color: Some(fg),
            thickness: gpui::px(1.0),
        });
    let weight = if cell.flags.contains(Flags::BOLD) {
        FontWeight::BOLD
    } else {
        base_font.weight
    };
    let style = if cell.flags.contains(Flags::ITALIC) {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };
    TextRun {
        len: cell.c.len_utf8(),
        color: fg,
        background_color: None,
        font: Font {
            weight,
            style,
            ..base_font.clone()
        },
        underline,
        strikethrough,
    }
}

/// Hash includes: char, fg, bg, flags, zerowidth, hyperlink.
pub(crate) fn line_hash(cells: &[&IndexedCell]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x100_0000_01b3;
    let mut h = FNV_OFFSET;
    for ic in cells {
        let cell = &ic.cell;
        h ^= cell.c as u64;
        h = h.wrapping_mul(FNV_PRIME);
        h ^= color_hash(cell.fg);
        h = h.wrapping_mul(FNV_PRIME);
        h ^= color_hash(cell.bg);
        h = h.wrapping_mul(FNV_PRIME);
        h ^= cell.flags.bits() as u64;
        h = h.wrapping_mul(FNV_PRIME);
        if let Some(zw) = cell.zerowidth() {
            for &c in zw {
                h ^= c as u64;
                h = h.wrapping_mul(FNV_PRIME);
            }
        }
        if let Some(hl) = cell.hyperlink() {
            for b in hl.uri().bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(FNV_PRIME);
            }
        }
    }
    h
}

fn color_hash(c: Color) -> u64 {
    match c {
        Color::Named(n) => n as u64,
        Color::Spec(rgb) => {
            0x1_0000 | (rgb.r as u64) | ((rgb.g as u64) << 8) | ((rgb.b as u64) << 16)
        }
        Color::Indexed(i) => 0x2_0000 | i as u64,
    }
}

impl BatchedTextRun {
    pub(crate) fn new(start: LayoutPoint, c: char, mut style: gpui::TextRun) -> Self {
        let text = c.to_string();
        debug_assert_eq!(style.len, c.len_utf8());
        let _ = &mut style;
        Self {
            start,
            text,
            cell_count: 1,
            style,
        }
    }

    pub(crate) fn can_append(&self, other: &gpui::TextRun) -> bool {
        self.style.font == other.font
            && self.style.color == other.color
            && self.style.background_color == other.background_color
            && self.style.underline == other.underline
            && self.style.strikethrough == other.strikethrough
    }

    pub(crate) fn append_char(&mut self, c: char) {
        self.text.push(c);
        self.cell_count += 1;
        self.style.len += c.len_utf8();
    }

    pub(crate) fn append_zw(&mut self, c: char) {
        self.text.push(c);
        self.style.len += c.len_utf8();
    }

    /// Paint the text run using the cached `ShapedLine`.
    pub(crate) fn paint(
        &self,
        shaped: &ShapedLine,
        x: Pixels,
        y: Pixels,
        line_h: Pixels,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        let pos = gpui::point(x, y);
        let _ = shaped.paint(pos, line_h, gpui::TextAlign::Left, None, window, cx);
    }
}
