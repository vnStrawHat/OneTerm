//! Palette terminal + resolve `Color` → `Rgb` (không GPUI).
//!
//! UI crate (`ui::TerminalTheme`) build `TerminalPalette` từ gpui-component
//! `Theme` rồi map `Rgb` → `gpui::Hsla` khi render. Tham chiếu Zed
//! `convert_color` (`terminal_element.rs`) nhưng core trả `Rgb` thuần.

use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

/// Palette 16-màu ANSI + fg/bg/cursor để resolve `Color::Named`/`Indexed`.
///
/// `ansi[0..8]` = normal, `ansi[8..16]` = bright. Dim variants (DimBlack…)
/// được tính bằng cách mix màu normal với `background` ở 50% — không cần
/// khai báo riêng (theme ít khi cung cấp dim).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalPalette {
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    pub ansi: [Rgb; 16],
}

impl TerminalPalette {
    /// Mix `c` với `bg` theo tỷ lệ `t` (0.0 = c, 1.0 = bg). Dùng cho dim.
    fn mix(c: Rgb, bg: Rgb, t: f32) -> Rgb {
        let lerp = |a: u8, b: u8| (a as f32 * (1.0 - t) + b as f32 * t).round() as u8;
        Rgb {
            r: lerp(c.r, bg.r),
            g: lerp(c.g, bg.g),
            b: lerp(c.b, bg.b),
        }
    }

    /// Dim version của `c` (mix 50% với background).
    fn dim(&self, c: Rgb) -> Rgb {
        Self::mix(c, self.background, 0.5)
    }
}

/// Resolve `Color` → `Rgb` theo palette.
///
/// - `Named`: 0-15 → `ansi[i]`; `Foreground`/`Background`/`Cursor` trực tiếp;
///   `Dim*` → dim(normal); `BrightForeground` → `foreground`;
///   `DimForeground` → dim(foreground).
/// - `Spec(rgb)`: trả nguyên bản (truecolor).
/// - `Indexed(n)`: 0-15 → `ansi`; 16-231 → cube 6×6×6; 232-255 → grayscale.
pub fn resolve_color(c: &Color, palette: &TerminalPalette) -> Rgb {
    match c {
        Color::Named(nc) => resolve_named(*nc, palette),
        Color::Spec(rgb) => *rgb,
        Color::Indexed(n) => resolve_indexed(*n, palette),
    }
}

fn resolve_named(nc: NamedColor, palette: &TerminalPalette) -> Rgb {
    let idx = nc as u32;
    match idx {
        // 16 màu ANSI (0-15).
        0..=15 => palette.ansi[idx as usize],
        // Foreground / Background / Cursor.
        256 => palette.foreground,
        257 => palette.background,
        258 => palette.cursor,
        // Dim variants (259-266) → dim của màu normal tương ứng.
        259..=266 => palette.dim(palette.ansi[(idx - 259) as usize]),
        // BrightForeground → foreground (không có bright fg riêng trong palette).
        267 => palette.foreground,
        // DimForeground → dim(foreground).
        268 => palette.dim(palette.foreground),
        _ => palette.foreground,
    }
}

fn resolve_indexed(n: u8, palette: &TerminalPalette) -> Rgb {
    match n {
        // 16 màu đầu map vào ANSI palette.
        0..=15 => palette.ansi[n as usize],
        // 6×6×6 RGB cube (16..=231).
        16..=231 => {
            let n = n - 16;
            let r = n / 36;
            let g = (n / 6) % 6;
            let b = n % 6;
            let conv = |c: u8| if c == 0 { 0 } else { 55 + 40 * c };
            Rgb {
                r: conv(r),
                g: conv(g),
                b: conv(b),
            }
        }
        // Grayscale ramp (232..=255): 24 bước từ gần đen đến gần trắng.
        232..=255 => {
            let v = 8 + 10 * (n - 232);
            Rgb { r: v, g: v, b: v }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pal() -> TerminalPalette {
        TerminalPalette {
            foreground: Rgb {
                r: 200,
                g: 200,
                b: 200,
            },
            background: Rgb {
                r: 20,
                g: 20,
                b: 20,
            },
            cursor: Rgb {
                r: 255,
                g: 255,
                b: 0,
            },
            ansi: [
                Rgb { r: 0, g: 0, b: 0 },   // 0 black
                Rgb { r: 200, g: 0, b: 0 }, // 1 red
                Rgb { r: 0, g: 200, b: 0 }, // 2 green
                Rgb {
                    r: 200,
                    g: 200,
                    b: 0,
                }, // 3 yellow
                Rgb { r: 0, g: 0, b: 200 }, // 4 blue
                Rgb {
                    r: 200,
                    g: 0,
                    b: 200,
                }, // 5 magenta
                Rgb {
                    r: 0,
                    g: 200,
                    b: 200,
                }, // 6 cyan
                Rgb {
                    r: 200,
                    g: 200,
                    b: 200,
                }, // 7 white
                Rgb {
                    r: 100,
                    g: 100,
                    b: 100,
                }, // 8 bright black
                Rgb { r: 255, g: 0, b: 0 }, // 9 bright red
                Rgb { r: 0, g: 255, b: 0 }, // 10 bright green
                Rgb {
                    r: 255,
                    g: 255,
                    b: 0,
                }, // 11 bright yellow
                Rgb { r: 0, g: 0, b: 255 }, // 12 bright blue
                Rgb {
                    r: 255,
                    g: 0,
                    b: 255,
                }, // 13 bright magenta
                Rgb {
                    r: 0,
                    g: 255,
                    b: 255,
                }, // 14 bright cyan
                Rgb {
                    r: 255,
                    g: 255,
                    b: 255,
                }, // 15 bright white
            ],
        }
    }

    #[test]
    fn named_ansi_index() {
        let p = pal();
        assert_eq!(
            resolve_color(&Color::Named(NamedColor::Red), &p),
            Rgb { r: 200, g: 0, b: 0 }
        );
        assert_eq!(
            resolve_color(&Color::Named(NamedColor::BrightBlue), &p),
            Rgb { r: 0, g: 0, b: 255 }
        );
    }

    #[test]
    fn named_fg_bg_cursor() {
        let p = pal();
        assert_eq!(
            resolve_color(&Color::Named(NamedColor::Foreground), &p),
            Rgb {
                r: 200,
                g: 200,
                b: 200
            }
        );
        assert_eq!(
            resolve_color(&Color::Named(NamedColor::Background), &p),
            Rgb {
                r: 20,
                g: 20,
                b: 20
            }
        );
        assert_eq!(
            resolve_color(&Color::Named(NamedColor::Cursor), &p),
            Rgb {
                r: 255,
                g: 255,
                b: 0
            }
        );
    }

    #[test]
    fn named_dim_is_mixed() {
        let p = pal();
        // DimRed = mix(red(200,0,0), bg(20,20,20), 0.5) = (110, 10, 10).
        let d = resolve_color(&Color::Named(NamedColor::DimRed), &p);
        assert_eq!(
            d,
            Rgb {
                r: 110,
                g: 10,
                b: 10
            }
        );
    }

    #[test]
    fn spec_truecolor_passthrough() {
        let p = pal();
        assert_eq!(
            resolve_color(&Color::Spec(Rgb { r: 1, g: 2, b: 3 }), &p),
            Rgb { r: 1, g: 2, b: 3 }
        );
    }

    #[test]
    fn indexed_low_maps_ansi() {
        let p = pal();
        assert_eq!(
            resolve_color(&Color::Indexed(2), &p),
            Rgb { r: 0, g: 200, b: 0 }
        );
    }

    #[test]
    fn indexed_cube_16() {
        let p = pal();
        // n=16: r=g=b=0 → (0,0,0).
        assert_eq!(
            resolve_color(&Color::Indexed(16), &p),
            Rgb { r: 0, g: 0, b: 0 }
        );
        // n=21: r=0,g=0,b=5 → (0,0,255). 21-16=5 → b=5 → conv(5)=255.
        assert_eq!(
            resolve_color(&Color::Indexed(21), &p),
            Rgb { r: 0, g: 0, b: 255 }
        );
        // n=196: 196-16=180 → r=5,g=0,b=0 → (255,0,0).
        assert_eq!(
            resolve_color(&Color::Indexed(196), &p),
            Rgb { r: 255, g: 0, b: 0 }
        );
    }

    #[test]
    fn indexed_grayscale() {
        let p = pal();
        // n=232: v=8.
        assert_eq!(
            resolve_color(&Color::Indexed(232), &p),
            Rgb { r: 8, g: 8, b: 8 }
        );
        // n=255: v = 8 + 10*23 = 238.
        assert_eq!(
            resolve_color(&Color::Indexed(255), &p),
            Rgb {
                r: 238,
                g: 238,
                b: 238
            }
        );
    }
}
