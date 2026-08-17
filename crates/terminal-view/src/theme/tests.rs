#[cfg(test)]
mod tests {
    use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb as VteRgb};
    use gpui::Hsla;
    use oneterm_terminal::TerminalPalette;

    use super::super::contrast::contrast_ratio;
    use super::super::palette::{ANSI_16, rgba_from_vte, vte_from_rgba};
    use super::super::*;

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
            indexed: [None; 256],
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
            search_match: gpui::hsla(0.13, 0.85, 0.5, 0.35),
            search_active: gpui::hsla(0.13, 0.9, 0.55, 0.7),
            class_styles: oneterm_highlight::ClassStyles::default(),
        };
        let h = resolve_cell_color(&Color::Named(NamedColor::Red), &t);
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
            search_match: gpui::hsla(0.13, 0.85, 0.5, 0.35),
            search_active: gpui::hsla(0.13, 0.9, 0.55, 0.7),
            class_styles: oneterm_highlight::ClassStyles::default(),
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
        let fg = gpui::hsla(0.0, 0.0, 0.5, 1.0);
        let bg = gpui::hsla(0.0, 0.0, 0.5, 1.0);
        let out = ensure_minimum_contrast(fg, bg, 4.5);
        assert!(out.l != 0.5, "lightness must change");
        assert!(contrast_ratio(out, bg) >= 4.4);
    }

    #[test]
    fn ensure_contrast_keeps_good_pair() {
        let fg = Hsla::white();
        let bg = Hsla::black();
        let out = ensure_minimum_contrast(fg, bg, 4.5);
        assert_eq!(out, fg, "already enough contrast → unchanged");
    }
}
