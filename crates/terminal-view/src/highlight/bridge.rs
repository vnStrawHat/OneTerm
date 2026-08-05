//! Bridge: `highlight::Hsla` → `gpui::Hsla` + default semantic style loading.
//!
//! The pure `oneterm_highlight` crate uses a plain `Hsla` mirror (no GPUI
//! dependency). This module converts those to `gpui::Hsla` and loads the
//! default semantic style asset (`assets/highlight/default.json`) into a
//! `highlight::ClassStyles` that the `TerminalTheme` carries.

use gpui::Hsla as GpHsla;

use oneterm_highlight::{Class, ClassStyles, Decoration, Hsla, parse_hex};

/// Convert a `highlight::Hsla` → `gpui::Hsla` (same field layout → trivial copy).
pub fn to_gpui_hsla(c: Hsla) -> GpHsla {
    gpui::hsla(c.h / 360.0, c.s, c.l, c.a)
}

/// Load the default semantic style block from the embedded asset JSON into a
/// `ClassStyles`. This is merged under any per-theme overrides (callers apply
/// overrides on top).
pub fn load_default_styles() -> ClassStyles {
    let json = include_str!("../../assets/highlight/default.json");
    parse_semantic_json(json)
}

/// Parse a semantic style JSON block into `ClassStyles`.
///
/// Expected format (see `docs/terminal-semantic-highlighting.md` §7):
/// ```jsonc
/// {
///   "promptLineBg": "#262626",
///   "styles": {
///     "promptSign": { "foreground": "#F92672" },
///     "url": { "foreground": "#66D9EF", "decoration": "underline" }
///   }
/// }
/// ```
pub fn parse_semantic_json(json: &str) -> ClassStyles {
    let mut styles = ClassStyles::empty();

    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return styles;
    };

    if let Some(bg) = v.get("promptLineBg").and_then(|s| s.as_str()) {
        if let Some(c) = parse_hex(bg) {
            styles.prompt_line_bg = Some(c);
        }
    }

    if let Some(obj) = v.get("styles").and_then(|s| s.as_object()) {
        for (key, val) in obj {
            let Some(class) = class_from_key(key) else {
                continue;
            };
            if let Some(fg) = val.get("foreground").and_then(|s| s.as_str()) {
                if let Some(c) = parse_hex(fg) {
                    styles.set_fg(class, c);
                }
            }
            if let Some(bg) = val.get("background").and_then(|s| s.as_str()) {
                if let Some(c) = parse_hex(bg) {
                    styles.bg[class as usize] = Some(c);
                }
            }
            if let Some(deco) = val.get("decoration").and_then(|s| s.as_str()) {
                if deco == "underline" {
                    styles.set_deco(class, Decoration::Underline);
                }
            }
            if let Some(bold) = val.get("bold").and_then(|s| s.as_bool()) {
                styles.font[class as usize].bold = bold;
            }
            if let Some(italic) = val.get("italic").and_then(|s| s.as_bool()) {
                styles.font[class as usize].italic = italic;
            }
        }
    }

    styles
}

/// Map a JSON style key → `Class`.
fn class_from_key(key: &str) -> Option<Class> {
    Some(match key {
        "promptSign" => Class::PromptSign,
        "command" => Class::Command,
        "option" => Class::Option,
        "error" => Class::Error,
        "success" => Class::Success,
        "warn" => Class::Warn,
        "info" => Class::Info,
        "debug" => Class::Debug,
        "path" => Class::Path,
        "ip" => Class::Ip,
        "mac" => Class::Mac,
        "dateTime" => Class::DateTime,
        "number" => Class::Number,
        "string" => Class::String,
        "operator" => Class::Operator,
        "bracket" => Class::Bracket,
        "url" => Class::Url,
        "permission" => Class::Permission,
        "permType" => Class::PermType,
        "permRead" => Class::PermRead,
        "permWrite" => Class::PermWrite,
        "permExec" => Class::PermExec,
        "permSpecial" => Class::PermSpecial,
        "permNone" => Class::PermNone,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_styles_loaded() {
        let s = load_default_styles();
        assert!(s.is_active());
        assert!(s.fg[Class::Error as usize].is_some());
        assert!(s.fg[Class::Url as usize].is_some());
        assert_eq!(s.deco[Class::Url as usize], Decoration::Underline);
        assert!(s.prompt_line_bg.is_some());
    }

    #[test]
    fn hsla_conversion() {
        let h = Hsla::new(120.0, 0.5, 0.5, 1.0);
        let g = to_gpui_hsla(h);
        assert!((g.h - 120.0 / 360.0).abs() < 0.01);
        assert!((g.s - 0.5).abs() < 0.01);
    }

    #[test]
    fn parse_empty_json_is_noop() {
        let s = parse_semantic_json("{}");
        assert!(!s.is_active());
    }

    #[test]
    fn parse_invalid_json_is_noop() {
        let s = parse_semantic_json("not json");
        assert!(!s.is_active());
    }
}
