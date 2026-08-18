//! Terminal palette + resolve `Color` → `Rgb` (no GPUI).
//!
//! The UI crate (`ui::TerminalTheme`) builds a `TerminalPalette` from the
//! gpui-component `Theme`, then maps `Rgb` → `gpui::Hsla` when rendering.
//! See Zed `convert_color` (`terminal_element.rs`), but core returns plain `Rgb`.

use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

/// 16-color ANSI palette + fg/bg/cursor to resolve `Color::Named`/`Indexed`.
///
/// `ansi[0..8]` = normal, `ansi[8..16]` = bright. Dim variants (DimBlack…)
/// are computed by mixing the normal color with `background` at 50% — no need to
/// declare them separately (themes rarely provide dim colors).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalPalette {
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    pub ansi: [Rgb; 16],
    /// OSC 4 dynamic overrides for palette indices 0-255 (`None` = use the
    /// default: `ansi[i]` for 0-15, computed cube/grayscale for 16-255).
    /// OSC 104 clears an entry back to `None`.
    pub indexed: [Option<Rgb>; 256],
}

impl TerminalPalette {
    /// Mix `c` with `bg` by ratio `t` (0.0 = c, 1.0 = bg). Used for dim.
    fn mix(c: Rgb, bg: Rgb, t: f32) -> Rgb {
        let lerp = |a: u8, b: u8| (a as f32 * (1.0 - t) + b as f32 * t).round() as u8;
        Rgb {
            r: lerp(c.r, bg.r),
            g: lerp(c.g, bg.g),
            b: lerp(c.b, bg.b),
        }
    }

    /// Dim version of `c` (mix 50% with the background).
    fn dim(&self, c: Rgb) -> Rgb {
        Self::mix(c, self.background, 0.5)
    }

    /// Effective ANSI color for index 0-15: an OSC 4 override if set, else the
    /// theme's `ansi[i]`.
    fn ansi_color(&self, i: usize) -> Rgb {
        self.indexed[i].unwrap_or(self.ansi[i])
    }
}

/// Color for the extended palette (indices 16-255): 6×6×6 RGB cube (16-231)
/// and 24-step grayscale ramp (232-255). These are theme-independent. For
/// indices 0-15 this returns black — callers must use the ANSI palette instead.
pub(crate) fn extended_indexed_color(n: u8) -> Rgb {
    match n {
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
        // Grayscale ramp (232..=255): 24 steps from near-black to near-white.
        232..=255 => {
            let v = 8 + 10 * (n - 232);
            Rgb { r: v, g: v, b: v }
        }
        // 0-15 are theme colors — not handled here.
        _ => Rgb { r: 0, g: 0, b: 0 },
    }
}

/// Default (non-OSC-overridden) color for palette index `n`:
/// - `0..=15` → `ansi[n]` (the theme's 16-color palette).
/// - `16..=231` → 6×6×6 RGB cube.
/// - `232..=255` → 24-step grayscale ramp.
///
/// Used by `resolve_indexed` as the fallback when no OSC 4 override is set.
fn indexed_default_color(n: u8, ansi: &[Rgb; 16]) -> Rgb {
    match n {
        0..=15 => ansi[n as usize],
        _ => extended_indexed_color(n),
    }
}

/// Resolve `Color` → `Rgb` using the palette.
///
/// - `Named`: 0-15 → `ansi[i]`; `Foreground`/`Background`/`Cursor` directly;
///   `Dim*` → dim(normal); `BrightForeground` → `foreground`;
///   `DimForeground` → dim(foreground).
/// - `Spec(rgb)`: returned verbatim (truecolor).
/// - `Indexed(n)`: 0-15 → `ansi`; 16-231 → 6×6×6 cube; 232-255 → grayscale.
pub fn resolve_color(c: &Color, palette: &TerminalPalette) -> Rgb {
    match c {
        Color::Named(nc) => resolve_named(*nc, palette),
        Color::Spec(rgb) => *rgb,
        Color::Indexed(n) => resolve_indexed(*n, palette),
    }
}

fn resolve_named(nc: NamedColor, palette: &TerminalPalette) -> Rgb {
    match nc {
        // 16 ANSI colors — honor OSC 4 overrides.
        NamedColor::Black
        | NamedColor::Red
        | NamedColor::Green
        | NamedColor::Yellow
        | NamedColor::Blue
        | NamedColor::Magenta
        | NamedColor::Cyan
        | NamedColor::White
        | NamedColor::BrightBlack
        | NamedColor::BrightRed
        | NamedColor::BrightGreen
        | NamedColor::BrightYellow
        | NamedColor::BrightBlue
        | NamedColor::BrightMagenta
        | NamedColor::BrightCyan
        | NamedColor::BrightWhite => palette.ansi_color(nc as usize),
        NamedColor::Foreground => palette.foreground,
        NamedColor::Background => palette.background,
        NamedColor::Cursor => palette.cursor,
        // Dim variants → dim of the corresponding normal color (using the
        // OSC 4 override if present).
        NamedColor::DimBlack
        | NamedColor::DimRed
        | NamedColor::DimGreen
        | NamedColor::DimYellow
        | NamedColor::DimBlue
        | NamedColor::DimMagenta
        | NamedColor::DimCyan
        | NamedColor::DimWhite => {
            palette.dim(palette.ansi_color(nc as usize - NamedColor::DimBlack as usize))
        }
        // No separate bright foreground in the palette.
        NamedColor::BrightForeground => palette.foreground,
        NamedColor::DimForeground => palette.dim(palette.foreground),
    }
}

fn resolve_indexed(n: u8, palette: &TerminalPalette) -> Rgb {
    // An OSC 4 override wins over the default for any index.
    palette.indexed[n as usize].unwrap_or_else(|| indexed_default_color(n, &palette.ansi))
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
            indexed: [None; 256],
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
