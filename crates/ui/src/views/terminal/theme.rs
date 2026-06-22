//! Terminal theme: map gpui-component `Theme` → `core::TerminalPalette`,
//! resolve `Color` → `gpui::Hsla`, và `ensure_minimum_contrast`.
//!
//! Thuần tiện ích (không GPUI Element). `TerminalElement` (#15) dùng các hàm
//! này khi paint. Tham chiếu Zed `convert_color` + `ensure_minimum_contrast`.

use alacritty_terminal::vte::ansi::{Color, Rgb as VteRgb};
use gpui::{Hsla, Rgba};
use gpui_component::Theme;

use myterm2_core::terminal::{TerminalPalette, resolve_color};

/// ANSI 16-color palette cố định (GNOME/Tango default) — đọc tốt trên dark &
/// light theme. bg/fg/cursor lấy từ active theme. (Sau này có thể derive
/// per-theme nếu muốn tinh chỉnh.)
const ANSI_16: [VteRgb; 16] = [
    VteRgb {
        r: 0x00,
        g: 0x00,
        b: 0x00,
    }, // 0 black
    VteRgb {
        r: 0xcc,
        g: 0x00,
        b: 0x00,
    }, // 1 red
    VteRgb {
        r: 0x4e,
        g: 0x9a,
        b: 0x06,
    }, // 2 green
    VteRgb {
        r: 0xc4,
        g: 0xa0,
        b: 0x00,
    }, // 3 yellow
    VteRgb {
        r: 0x34,
        g: 0x65,
        b: 0xa4,
    }, // 4 blue
    VteRgb {
        r: 0x75,
        g: 0x50,
        b: 0x7b,
    }, // 5 magenta
    VteRgb {
        r: 0x06,
        g: 0x98,
        b: 0x9a,
    }, // 6 cyan
    VteRgb {
        r: 0xd3,
        g: 0xd7,
        b: 0xcf,
    }, // 7 white
    VteRgb {
        r: 0x55,
        g: 0x57,
        b: 0x53,
    }, // 8 bright black
    VteRgb {
        r: 0xef,
        g: 0x29,
        b: 0x29,
    }, // 9 bright red
    VteRgb {
        r: 0x8a,
        g: 0xe2,
        b: 0x34,
    }, // 10 bright green
    VteRgb {
        r: 0xfc,
        g: 0xe9,
        b: 0x4f,
    }, // 11 bright yellow
    VteRgb {
        r: 0x72,
        g: 0x9f,
        b: 0xcf,
    }, // 12 bright blue
    VteRgb {
        r: 0xad,
        g: 0x7f,
        b: 0xa8,
    }, // 13 bright magenta
    VteRgb {
        r: 0x34,
        g: 0xe2,
        b: 0xe2,
    }, // 14 bright cyan
    VteRgb {
        r: 0xee,
        g: 0xee,
        b: 0xec,
    }, // 15 bright white
];

/// Theme terminal đã build sẵn palette + bg/fg (Hsla) + ngưỡng contrast.
#[derive(Clone)]
pub struct TerminalTheme {
    pub palette: TerminalPalette,
    /// Nền mặc định (theme background) — để paint nền element.
    pub bg: Hsla,
    /// FG mặc định (theme foreground).
    pub fg: Hsla,
    /// Màu nền selection (highlight text đang chọn).
    pub selection: Hsla,
    /// Ngưỡng contrast tối thiểu (WCAG, mặc định 4.5 ≈ AA).
    pub min_contrast: f32,
    /// Màu chữ gutter (timestamp + line number). Default = dim fg (50% lightness).
    pub gutter_fg: Hsla,
    /// Màu nền gutter. Default = cùng nền terminal.
    pub gutter_bg: Hsla,
    /// Màu chữ clock [HH:MM:SS]. Default = gutter_fg.
    pub clock_fg: Hsla,
    /// Màu chữ line number. Default = gutter_fg.
    pub line_number_fg: Hsla,
}

/// Build `TerminalTheme` từ gpui-component active `Theme`.
pub fn build_terminal_theme(theme: &Theme) -> TerminalTheme {
    let c = &theme.colors;
    let fg = c.foreground;
    let bg = c.background;
    // cursor = caret color (fallback foreground).
    let cursor_rgba = if c.caret.a > 0.0 {
        c.caret.to_rgb()
    } else {
        c.foreground.to_rgb()
    };
    let palette = TerminalPalette {
        foreground: vte_from_rgba(c.foreground.to_rgb()),
        background: vte_from_rgba(c.background.to_rgb()),
        cursor: vte_from_rgba(cursor_rgba),
        ansi: ANSI_16,
    };
    TerminalTheme {
        palette,
        bg,
        fg,
        // Zed: blue().dark().step_3() = #0d2847 (dark) / blue().light().step_3() = #e6f4fe (light).
        // Solid color — text paint trên selection với màu gốc (không inverse video).
        selection: if bg.l < 0.5 {
            gpui::hsla(0.589, 0.69, 0.165, 1.0) // #0d2847 dark blue
        } else {
            gpui::hsla(0.569, 0.92, 0.949, 1.0) // #e6f4fe light blue
        },
        min_contrast: 4.5,
        // Gutter defaults: dim fg (50% lightness) cho text, same bg cho nền.
        gutter_fg: gpui::hsla(fg.h, fg.s, fg.l * 0.5, fg.a),
        gutter_bg: bg,
        // Clock + line number default = gutter_fg (override qua config nếu muốn).
        clock_fg: gpui::hsla(fg.h, fg.s, fg.l * 0.5, fg.a),
        line_number_fg: gpui::hsla(fg.h, fg.s, fg.l * 0.5, fg.a),
    }
}

// ── Conversion ────────────────────────────────────────────────────────
/// `vte::ansi::Rgb` (u8) → `gpui::Rgba` (0..1, alpha 1).
pub fn rgba_from_vte(c: VteRgb) -> Rgba {
    Rgba {
        r: c.r as f32 / 255.0,
        g: c.g as f32 / 255.0,
        b: c.b as f32 / 255.0,
        a: 1.0,
    }
}

/// `gpui::Rgba` (0..1) → `vte::ansi::Rgb` (u8).
pub fn vte_from_rgba(c: Rgba) -> VteRgb {
    VteRgb {
        r: (c.r * 255.0).round().clamp(0.0, 255.0) as u8,
        g: (c.g * 255.0).round().clamp(0.0, 255.0) as u8,
        b: (c.b * 255.0).round().clamp(0.0, 255.0) as u8,
    }
}

/// `vte::ansi::Rgb` → `gpui::Hsla` (qua `Rgba`).
pub fn hsla_from_vte(c: VteRgb) -> Hsla {
    Hsla::from(rgba_from_vte(c))
}

/// Resolve `Color` (alacritty) → `Hsla` theo palette.
pub fn resolve_cell_color(c: &Color, theme: &TerminalTheme) -> Hsla {
    hsla_from_vte(resolve_color(c, &theme.palette))
}

// ── Contrast ─────────────────────────────────────────────────────────
/// Relative luminance (WCAG) từ `Hsla`.
fn relative_luminance(c: Hsla) -> f32 {
    let rgba = c.to_rgb();
    let lin = |ch: f32| {
        if ch <= 0.03928 {
            ch / 12.92
        } else {
            ((ch + 0.055) / 1.055).powi(2)
        }
    };
    let r = lin(rgba.r);
    let g = lin(rgba.g);
    let b = lin(rgba.b);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// WCAG contrast ratio giữa hai màu (≥1.0).
pub fn contrast_ratio(a: Hsla, b: Hsla) -> f32 {
    let la = relative_luminance(a);
    let lb = relative_luminance(b);
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Điều chỉnh `fg` để đạt contrast ≥ `min` với `bg`: thử đẩy lightness lên và
/// xuống, chọn hướng đạt min (hoặc contrast cao nhất nếu không đạt). Giữ
/// alpha. Tham chiếu Zed `ensure_minimum_contrast`.
pub fn ensure_minimum_contrast(fg: Hsla, bg: Hsla, min: f32) -> Hsla {
    if contrast_ratio(fg, bg) >= min || min <= 1.0 {
        return fg;
    }
    let mut up = fg;
    let mut down = fg;
    let mut up_ok = false;
    let mut down_ok = false;
    for _ in 0..40 {
        if !up_ok {
            up.l = (up.l + 0.03).clamp(0.0, 1.0);
            if contrast_ratio(up, bg) >= min {
                up_ok = true;
            }
        }
        if !down_ok {
            down.l = (down.l - 0.03).clamp(0.0, 1.0);
            if contrast_ratio(down, bg) >= min {
                down_ok = true;
            }
        }
        if up_ok && down_ok {
            break;
        }
    }
    match (up_ok, down_ok) {
        (true, true) => {
            // Chọn hướng thay đổi ít hơn.
            if (up.l - fg.l).abs() <= (fg.l - down.l).abs() {
                up
            } else {
                down
            }
        }
        (true, false) => up,
        (false, true) => down,
        (false, false) => {
            // Không đạt min → chọn contrast cao nhất.
            if contrast_ratio(up, bg) >= contrast_ratio(down, bg) {
                up
            } else {
                down
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pal() -> TerminalPalette {
        TerminalPalette {
            foreground: VteRgb {
                r: 200,
                g: 200,
                b: 200,
            },
            background: VteRgb {
                r: 20,
                g: 20,
                b: 20,
            },
            cursor: VteRgb {
                r: 255,
                g: 255,
                b: 0,
            },
            ansi: ANSI_16,
        }
    }

    #[test]
    fn rgb_roundtrip() {
        let c = VteRgb {
            r: 12,
            g: 34,
            b: 56,
        };
        let rgba = rgba_from_vte(c);
        assert_eq!(vte_from_rgba(rgba), c);
    }

    #[test]
    fn resolve_named_red_to_hsla() {
        let t = TerminalTheme {
            palette: pal(),
            bg: Hsla::black(),
            fg: Hsla::white(),
            min_contrast: 4.5,
            selection: gpui::hsla(0.6, 0.5, 0.5, 0.3),
            gutter_fg: Hsla::white(),
            gutter_bg: Hsla::black(),
            clock_fg: Hsla::white(),
            line_number_fg: Hsla::white(),
        };
        let h = resolve_cell_color(
            &Color::Named(alacritty_terminal::vte::ansi::NamedColor::Red),
            &t,
        );
        // Red ANSI = #cc0000 → r≈0.8.
        let rgba = h.to_rgb();
        assert!((rgba.r - 0xCC as f32 / 255.0).abs() < 0.01);
    }

    #[test]
    fn resolve_spec_truecolor_passthrough() {
        let t = TerminalTheme {
            palette: pal(),
            bg: Hsla::black(),
            fg: Hsla::white(),
            min_contrast: 4.5,
            selection: gpui::hsla(0.6, 0.5, 0.5, 0.3),
            gutter_fg: Hsla::white(),
            gutter_bg: Hsla::black(),
            clock_fg: Hsla::white(),
            line_number_fg: Hsla::white(),
        };
        let h = resolve_cell_color(&Color::Spec(VteRgb { r: 1, g: 2, b: 3 }), &t);
        let rgba = h.to_rgb();
        assert!((rgba.r - 1.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn contrast_ratio_black_white_is_max() {
        let r = contrast_ratio(Hsla::black(), Hsla::white());
        assert!(r > 20.0, "black/white contrast = {r}");
    }

    #[test]
    fn contrast_ratio_same_color_is_one() {
        assert!((contrast_ratio(Hsla::red(), Hsla::red()) - 1.0).abs() < 0.001);
    }

    #[test]
    fn ensure_contrast_pushes_lightness() {
        // fg = bg (no contrast) → phải đẩy ra xa.
        let fg = gpui::hsla(0.0, 0.0, 0.5, 1.0);
        let bg = gpui::hsla(0.0, 0.0, 0.5, 1.0);
        let out = ensure_minimum_contrast(fg, bg, 4.5);
        assert!(out.l != 0.5, "lightness phải đổi");
        assert!(contrast_ratio(out, bg) >= 4.4);
    }

    #[test]
    fn ensure_contrast_keeps_good_pair() {
        let fg = Hsla::white();
        let bg = Hsla::black();
        let out = ensure_minimum_contrast(fg, bg, 4.5);
        assert_eq!(out, fg, "đã đủ contrast → giữ nguyên");
    }
}
