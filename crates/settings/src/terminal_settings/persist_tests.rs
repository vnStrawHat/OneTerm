//! Roundtrip tests for the settings <-> config mapping (TEST-12).
//!
//! `apply_config` (config -> settings) and `to_config` (settings -> config)
//! are hand-written inverses; these tests keep them from drifting apart.

use gpui::FontWeight;

use crate::terminal_config::{SemanticHighlightingMode, TabTitleMode, TerminalConfig};

use super::super::font::parse_weight;
use super::super::{
    ColorOverrides, TerminalBlink, TerminalCursorShape, TerminalPadding, TerminalSettings,
    parse_hex_color,
};
use super::weight_to_string;

/// Compare two configs structurally via their JSON form (`TerminalConfig`
/// does not implement `PartialEq`).
fn config_json(cfg: &TerminalConfig) -> serde_json::Value {
    serde_json::to_value(cfg).expect("TerminalConfig serializes")
}

/// A settings instance with every persisted field moved off its default.
fn non_default_settings() -> TerminalSettings {
    let mut s = TerminalSettings::default();
    s.font_family = Some("Cascadia Mono".into());
    s.font_size = Some(15.0);
    s.base_font_size = Some(15.0);
    s.font_weight = FontWeight::SEMIBOLD;
    s.font_features = vec!["calt".into(), "liga".into()];
    s.cursor_shape = TerminalCursorShape::Bar;
    s.cursor_blink = TerminalBlink::Off;
    s.cursor_color = parse_hex_color("#FF8800");
    s.line_height_factor = 1.5;
    s.cell_width = Some(9.0);
    s.padding = TerminalPadding {
        top: 1.0,
        right: 2.0,
        bottom: 3.0,
        left: 4.0,
    };
    s.show_gutter = true;
    s.semantic_highlighting = SemanticHighlightingMode::Off;
    s.tab_title_mode = TabTitleMode::Osc;
    s.shell.args = vec!["--login".into()];
    s.shell.utf8 = false;
    s.scroll_multiplier = 3.0;
    s.alternate_scroll = false;
    s.scrollback_history = 1234;
    s.show_context_menu = false;
    s.bell_enabled = false;
    s.allow_clipboard_read = true;
    s.completion.enabled = false;
    s.completion.max_history = 42;
    s.completion.force_family = Some("bash".into());
    s.color_overrides = ColorOverrides {
        foreground: parse_hex_color("#111111"),
        background: parse_hex_color("#222222"),
        cursor: parse_hex_color("#333333"),
        selection: parse_hex_color("#444444"),
        gutter_fg: parse_hex_color("#555555"),
        gutter_bg: parse_hex_color("#666666"),
        clock_fg: parse_hex_color("#777777"),
        line_number_fg: parse_hex_color("#888888"),
        min_contrast: 2.5,
        ansi: (0..16u32)
            .map(|i| parse_hex_color(&format!("#{:02X}{:02X}{:02X}", i * 10, i * 5, i)))
            .map(|c| c.expect("valid hex"))
            .collect(),
    };
    s
}

#[test]
fn settings_config_roundtrip_is_stable() {
    let original = non_default_settings();
    let cfg = original.to_config();

    let mut restored = TerminalSettings::default();
    restored.apply_config(&cfg);

    assert_eq!(config_json(&restored.to_config()), config_json(&cfg));
    // Spot-check live fields that are not directly visible through the config.
    assert_eq!(restored.font_size, Some(15.0));
    assert_eq!(restored.base_font_size, Some(15.0));
    assert_eq!(restored.font_weight, FontWeight::SEMIBOLD);
    assert_eq!(restored.color_overrides.ansi.len(), 16);
}

#[test]
fn default_settings_roundtrip_is_stable() {
    let cfg = TerminalSettings::default().to_config();
    let mut restored = TerminalSettings::default();
    restored.apply_config(&cfg);
    assert_eq!(config_json(&restored.to_config()), config_json(&cfg));
}

#[test]
fn zoomed_font_size_persists_base_size() {
    // Regression for CORR-12: a zoomed session must not overwrite the
    // configured size in terminal.json.
    let mut s = non_default_settings();
    s.zoom_in(15.0);
    s.zoom_in(15.0);
    assert_eq!(s.font_size, Some(17.0));

    let cfg = s.to_config();
    assert_eq!(cfg.font.size, Some(15.0));

    // Reloading the persisted config restores the base size on both fields.
    let mut restored = TerminalSettings::default();
    restored.apply_config(&cfg);
    assert_eq!(restored.font_size, Some(15.0));
    assert_eq!(restored.base_font_size, Some(15.0));
}

#[test]
fn weight_string_roundtrip() {
    for name in [
        "thin",
        "extra_light",
        "light",
        "normal",
        "medium",
        "semibold",
        "bold",
        "extra_bold",
        "black",
    ] {
        assert_eq!(weight_to_string(parse_weight(name)), name);
    }
    // Aliases normalize to the canonical spelling.
    assert_eq!(weight_to_string(parse_weight("regular")), "normal");
    assert_eq!(weight_to_string(parse_weight("ExtraBold")), "extra_bold");
    // Unknown input falls back to the default weight.
    assert_eq!(weight_to_string(parse_weight("wat")), "normal");
}
