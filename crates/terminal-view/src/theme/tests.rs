#[cfg(test)]
mod tests {
    use gpui::{Hsla, Rgba};
    use oneterm_settings::ColorOverrides;
    use oneterm_terminal::{TerminalPalette, resolve_color};

    use super::super::contrast::{contrast_ratio, ensure_minimum_contrast};
    use super::super::palette::{ColorTable, hsla_from_rgb, rgb_from_rgba, rgba_from_rgb};
    use super::super::terminal_theme::DEFAULT_MIN_CONTRAST;
    use super::super::*;
    use crate::render::frame::Color as FrameColor;

    fn rgb(r: u8, g: u8, b: u8) -> Rgba {
        Rgba {
            r: f32::from(r) / 255.0,
            g: f32::from(g) / 255.0,
            b: f32::from(b) / 255.0,
            a: 1.0,
        }
    }

    /// A palette with known fg/bg/cursor over the default ANSI 16.
    fn pal() -> TerminalPalette {
        let mut palette = build_terminal_theme(&gpui_component::Theme::default()).palette;
        palette.foreground = rgb_from_rgba(rgb(200, 200, 200));
        palette.background = rgb_from_rgba(rgb(20, 20, 20));
        palette.cursor = rgb_from_rgba(rgb(255, 255, 0));
        palette.indexed = [None; 256];
        palette
    }

    fn themed_with(palette: TerminalPalette) -> TerminalTheme {
        TerminalTheme {
            colors: ColorTable::from_palette(&palette),
            palette,
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
        }
    }

    #[test]
    fn rgb_roundtrip() {
        let c = rgb_from_rgba(rgb(12, 34, 56));
        let rgba = rgba_from_rgb(c);
        assert_eq!(rgb_from_rgba(rgba), c);
        assert!((rgba.g - 34.0 / 255.0).abs() < 0.001);
    }

    #[test]
    fn resolve_named_red_to_hsla() {
        let t = themed_with(pal());
        // The table lookup must equal the engine's palette resolution.
        let h = hsla_from_rgb(resolve_color(&FrameColor::Ansi(1).to_engine(), &t.palette));
        assert_eq!(t.color(FrameColor::Ansi(1)), h);
        let rgba = h.to_rgb();
        assert!((rgba.r - 0xCC as f32 / 255.0).abs() < 0.01);
    }

    #[test]
    fn resolve_spec_truecolor_passthrough() {
        let t = themed_with(pal());
        let h = t.color(FrameColor::Rgb(1, 2, 3));
        assert_eq!(
            h,
            hsla_from_rgb(resolve_color(
                &FrameColor::Rgb(1, 2, 3).to_engine(),
                &t.palette
            ))
        );
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
            hsla_from_rgb(t.palette.ansi[0])
        );
        assert_ne!(
            t.color(FrameColor::Ansi(0)),
            plain.color(FrameColor::Ansi(0))
        );
        assert_ne!(plain.colors.hash, t.colors.hash);
    }

    // ── The prompt-line band (`US-0134`) ───────────────────────────────────

    /// The classes a prompt row can carry, which is the set of foregrounds that
    /// has to stay legible on the band.
    const PROMPT_ROW_CLASSES: &[oneterm_highlight::Class] = &[
        oneterm_highlight::Class::PromptSign,
        oneterm_highlight::Class::Command,
        oneterm_highlight::Class::Option,
        oneterm_highlight::Class::Path,
        // `US-0133`'s exit-code tint substitutes these two at the sign.
        oneterm_highlight::Class::Success,
        oneterm_highlight::Class::Error,
    ];

    /// The band lies **strictly between** the background and the foreground, on
    /// the foreground's side, by a distance nobody can miss.
    ///
    /// Stated as three separate properties on purpose. The earlier form of this
    /// test — "nearer the background than the foreground", plus `band != bg` —
    /// was satisfied *more* comfortably by a band moved the wrong way, so
    /// inverting the direction left the whole suite green (verification MAJ-3).
    /// The direction assertion is what pins it; the floor is what pins MED-2,
    /// the low-contrast pair whose band used to land past the text.
    fn assert_band_between(bg: Hsla, fg: Hsla, band: Hsla, label: &str) {
        let gap = fg.l - bg.l;
        if gap.abs() < f32::EPSILON {
            // No pair to sit between: the text is invisible on its own
            // background, and nothing this function asserts is meaningful.
            return;
        }
        let moved = band.l - bg.l;
        assert_eq!(
            moved.signum(),
            gap.signum(),
            "{label}: the band moved away from the text (bg.l={:.4} fg.l={:.4} band.l={:.4})",
            bg.l,
            fg.l,
            band.l
        );
        assert!(
            moved.abs() < gap.abs(),
            "{label}: the band reached or passed the text (bg.l={:.4} fg.l={:.4} band.l={:.4})",
            bg.l,
            fg.l,
            band.l
        );
        let floor = (gap.abs() / 2.0).min(0.05);
        assert!(
            moved.abs() >= floor - f32::EPSILON,
            "{label}: the band is {:.4} from the background, under the {floor:.4} floor",
            moved.abs()
        );
        assert_eq!(band.h, bg.h, "{label}: the band gained a hue of its own");
    }

    /// Synthetic pairs, including the low-contrast probes the verification found
    /// the band crossing (`bg .5 / fg .55` and its mirror).
    #[test]
    fn the_band_sits_between_the_background_and_the_text() {
        for (bg_l, fg_l) in [
            (0.08, 0.90),
            (0.98, 0.10),
            (0.50, 0.55),
            (0.50, 0.45),
            (0.50, 0.52),
            (0.00, 1.00),
            (1.00, 0.00),
        ] {
            let mut t = build_terminal_theme(&gpui_component::Theme::default());
            t.bg = gpui::hsla(0.0, 0.0, bg_l, 1.0);
            t.fg = gpui::hsla(0.0, 0.0, fg_l, 1.0);
            assert_band_between(
                t.bg,
                t.fg,
                t.prompt_line_bg(false),
                &format!("{bg_l}/{fg_l}"),
            );
            // `DECSCNM` draws with the pair swapped, and the band follows it.
            assert_band_between(
                t.fg,
                t.bg,
                t.prompt_line_bg(true),
                &format!("reversed {bg_l}/{fg_l}"),
            );
        }
    }

    /// An explicit `promptLineBg` still wins, which is §7's rule for the whole
    /// `terminal.semantic` block.
    #[test]
    fn the_shipped_asset_leaves_the_band_to_the_theme() {
        let mut t = build_terminal_theme(&gpui_component::Theme::default());
        assert!(
            t.class_styles.prompt_line_bg.is_none(),
            "one fixed hex for every theme is what `US-0134` removed"
        );
        t.bg = gpui::hsla(0.0, 0.0, 0.08, 1.0);
        t.fg = gpui::hsla(0.0, 0.0, 0.9, 1.0);
        // `DECSCNM` swaps the pair the screen is drawn with, so the band follows.
        assert_ne!(t.prompt_line_bg(true), t.prompt_line_bg(false));
    }

    /// Every foreground a prompt row can carry clears 4.5:1 against the resolved
    /// band, in every theme variant the user can select.
    ///
    /// The repository's contrast gate (`scripts/check-theme-contrast.py`)
    /// measures **kit UI tokens** read out of `crates/theme/themes/*.json`.
    /// Terminal grid text is none of those — it is an ANSI palette entry or a
    /// semantic `Class` foreground, and the semantic palette is not in a theme
    /// file at all (`crates/terminal-view/assets/highlight/default.json`), which
    /// the script never opens. So the same WCAG floor is applied here, where the
    /// colours actually are, over the whole pipeline the renderer runs:
    /// `resolve_style` measures against what is behind the glyph, which on a
    /// prompt row is the band. Extending `SURFACES` to terminal tokens is a
    /// change to that gate's scope and is an owner question in `IN-0044`.
    #[gpui::test]
    fn every_prompt_row_foreground_clears_the_band(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let registry = gpui_component::ThemeRegistry::global_mut(cx);
            for (name, content) in oneterm_theme::embedded_theme_files() {
                registry
                    .load_themes_from_str(content)
                    .unwrap_or_else(|e| panic!("embedded theme {name}: {e}"));
            }
            let configs: Vec<_> = gpui_component::ThemeRegistry::global(cx)
                .themes()
                .values()
                .cloned()
                .collect();
            assert!(configs.len() > 20, "expected every embedded theme variant");
            for config in configs {
                let name = config.name.clone();
                gpui_component::Theme::global_mut(cx).apply_config(&config);
                let theme = build_terminal_theme(gpui_component::Theme::global(cx));
                let band = theme.prompt_line_bg(false);
                assert_band_between(theme.bg, theme.fg, band, &name);
                let mut foregrounds = vec![("foreground", theme.fg)];
                for class in PROMPT_ROW_CLASSES {
                    let style = theme.class_styles.style(*class as u8);
                    let fg = style.fg.expect("every prompt-row class has a colour");
                    foregrounds.push((class_name(*class), crate::highlight::to_gpui_hsla(fg)));
                }
                for (label, fg) in foregrounds {
                    // What the renderer actually paints: the contrast pass runs
                    // against the band, because that is what is behind the glyph.
                    let painted = theme.ensure_contrast(fg, band);
                    let ratio = contrast_ratio(painted, band);
                    assert!(
                        ratio >= DEFAULT_MIN_CONTRAST,
                        "{name}: {label} on the prompt band is {ratio:.2}:1"
                    );
                }
            }
        });
    }

    fn class_name(class: oneterm_highlight::Class) -> &'static str {
        use oneterm_highlight::Class;
        match class {
            Class::PromptSign => "promptSign",
            Class::Command => "command",
            Class::Option => "option",
            Class::Path => "path",
            Class::Success => "success",
            Class::Error => "error",
            _ => "class",
        }
    }
}
