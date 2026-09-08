#[cfg(test)]
mod tests {
    use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb as VteRgb};
    use gpui::Hsla;
    use oneterm_terminal::TerminalPalette;

    use oneterm_settings::ColorOverrides;

    use super::super::contrast::contrast_ratio;
    use super::super::palette::{ANSI_16, ColorTable, rgba_from_vte, vte_from_rgba};
    use super::super::terminal_theme::DEFAULT_MIN_CONTRAST;
    use super::super::*;
    use crate::render::frame::Color as FrameColor;

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
            class_styles: crate::highlight::load_default_styles(),
            colors: ColorTable::from_palette(&pal()),
        };
        let h = resolve_cell_color(&Color::Named(NamedColor::Red), &t);
        assert_eq!(
            t.color(FrameColor::Ansi(1)),
            h,
            "table lookup must equal palette resolution"
        );
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
            class_styles: crate::highlight::load_default_styles(),
            colors: ColorTable::from_palette(&pal()),
        };
        let h = resolve_cell_color(&Color::Spec(VteRgb { r: 1, g: 2, b: 3 }), &t);
        assert_eq!(t.color(FrameColor::Rgb(1, 2, 3)), h);
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

    fn themed(min_contrast: f32) -> TerminalTheme {
        let co = ColorOverrides {
            min_contrast,
            ..ColorOverrides::default()
        };
        apply_color_overrides(build_terminal_theme(&gpui_component::Theme::default()), &co)
    }

    #[test]
    fn contrast_ratio_uses_wcag_exponent() {
        let bw = contrast_ratio(Hsla::black(), Hsla::white());
        assert!((bw - 21.0).abs() < 0.01, "black/white = {bw}");
        let grey = gpui::Hsla::from(gpui::rgb(0x777777));
        let gw = contrast_ratio(grey, Hsla::white());
        assert!((gw - 4.48).abs() < 0.02, "#777777 on white = {gw}");
    }

    #[test]
    fn min_contrast_zero_keeps_theme_default() {
        assert_eq!(themed(0.0).min_contrast, DEFAULT_MIN_CONTRAST);
        assert_eq!(themed(-1.0).min_contrast, DEFAULT_MIN_CONTRAST);
        assert_eq!(themed(7.0).min_contrast, 7.0);
    }

    #[test]
    fn min_contrast_one_disables() {
        let fg = gpui::hsla(0.0, 0.0, 0.5, 1.0);
        let bg = gpui::hsla(0.0, 0.0, 0.5, 1.0);
        assert_eq!(
            themed(1.0).ensure_contrast(fg, bg),
            fg,
            "min_contrast 1.0 must not adjust"
        );
        assert_ne!(
            themed(0.0).ensure_contrast(fg, bg),
            fg,
            "theme default must adjust"
        );
    }

    #[test]
    fn color_table_tracks_override_passes() {
        let co = ColorOverrides {
            ansi: vec![Some(gpui::hsla(0.0, 1.0, 0.5, 1.0))],
            ..ColorOverrides::default()
        };
        let plain = build_terminal_theme(&gpui_component::Theme::default());
        let t = apply_color_overrides(plain.clone(), &co);
        assert_eq!(
            t.color(FrameColor::Ansi(0)),
            resolve_cell_color(&Color::Named(NamedColor::Black), &t)
        );
        assert_ne!(plain.colors.hash, t.colors.hash);
    }
}
