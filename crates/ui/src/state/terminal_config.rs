//! Terminal config JSON — load/save `terminal.json`.
//!
//! Config file được nhóm thành các nhóm logic: font, cursor, layout, shell,
//! scroll, bell, colors. Nếu file không tồn tại → tạo file default để user
//! biết các option có thể config.
//!
//! Path: `target/terminal.json` (debug) / `terminal.json` (release).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use myterm2_core::LocalShellConfig;

// ── Config path ──────────────────────────────────────────────────────

#[cfg(debug_assertions)]
const CONFIG_FILE: &str = "target/terminal.json";
#[cfg(not(debug_assertions))]
const CONFIG_FILE: &str = "terminal.json";

// ── Top-level config ─────────────────────────────────────────────────

/// Toàn bộ config terminal — parse từ `terminal.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TerminalConfig {
    #[serde(default)]
    pub font: FontConfig,
    #[serde(default)]
    pub cursor: CursorConfig,
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default)]
    pub shell: LocalShellConfig,
    #[serde(default)]
    pub scroll: ScrollConfig,
    #[serde(default)]
    pub bell: BellConfig,
    #[serde(default)]
    pub colors: ColorsConfig,
}

// ── Font ─────────────────────────────────────────────────────────────

/// Nhóm Font: family, fallback, size, weight, features.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    /// Font family chính (null = dùng theme mono font).
    #[serde(default = "default_font_family")]
    pub family: Option<String>,
    /// Font fallback stack (rỗng = platform defaults).
    #[serde(default)]
    pub fallback_fonts: Vec<String>,
    /// Font size in px (null = dùng theme mono font size).
    #[serde(default = "default_font_size")]
    pub size: Option<f32>,
    /// Font weight: "thin" | "extra_light" | "light" | "normal" | "medium"
    /// | "semibold" | "bold" | "extra_bold" | "black".
    #[serde(default = "default_weight")]
    pub weight: String,
    /// Font features (OpenType): vd ["calt", "liga"] → bật ligatures.
    /// Terminal mặc định tắt calt; thêm "calt" vào list để bật lại.
    #[serde(default)]
    pub features: Vec<String>,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: default_font_family(),
            fallback_fonts: Vec::new(),
            size: default_font_size(),
            weight: default_weight(),
            features: Vec::new(),
        }
    }
}

fn default_font_family() -> Option<String> {
    Some("Cascadia Mono".into())
}

fn default_font_size() -> Option<f32> {
    Some(15.0)
}

fn default_weight() -> String {
    "normal".into()
}

// ── Cursor ───────────────────────────────────────────────────────────

/// Nhóm Cursor: shape, color, blink.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorConfig {
    /// Hình dáng: "block" | "bar" | "underline".
    #[serde(default = "default_cursor_shape")]
    pub shape: String,
    /// Màu con trỏ (null = theme caret, "#RRGGBB" để override).
    #[serde(default)]
    pub color: Option<String>,
    /// Có nhấp nháy khi focus không.
    #[serde(default = "default_true")]
    pub blink: bool,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            shape: default_cursor_shape(),
            color: None,
            blink: true,
        }
    }
}

fn default_cursor_shape() -> String {
    "block".into()
}

// ── Layout ───────────────────────────────────────────────────────────

/// Nhóm Layout: line height, cell width, padding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    /// Line height multiplier (1.15 = 115% font size).
    #[serde(default = "default_line_height")]
    pub line_height: f32,
    /// Cell width override in px (null = auto từ advance width của '0',
    /// giống Windows Terminal / CSS ch unit).
    #[serde(default = "default_cell_width")]
    pub cell_width: Option<f32>,
    /// Padding quanh terminal content (px).
    #[serde(default)]
    pub padding: PaddingConfig,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            line_height: default_line_height(),
            cell_width: default_cell_width(),
            padding: PaddingConfig::default(),
        }
    }
}

fn default_line_height() -> f32 {
    1.15
}

fn default_cell_width() -> Option<f32> {
    None // auto: đo advance width của '0' (CSS ch unit, giống Windows Terminal)
}

/// Padding 4 phía (px).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PaddingConfig {
    #[serde(default)]
    pub top: f32,
    #[serde(default = "default_padding_right")]
    pub right: f32,
    #[serde(default)]
    pub bottom: f32,
    #[serde(default = "default_padding_left")]
    pub left: f32,
}

impl Default for PaddingConfig {
    fn default() -> Self {
        Self {
            top: 0.0,
            right: default_padding_right(),
            bottom: 0.0,
            left: default_padding_left(),
        }
    }
}

fn default_padding_right() -> f32 {
    5.0
}

fn default_padding_left() -> f32 {
    10.0
}

// ── Scroll ───────────────────────────────────────────────────────────

/// Nhóm Scroll: multiplier, alternate scroll.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollConfig {
    /// Scroll multiplier cho mouse wheel (1.0 = default, 3.0 = nhanh 3x).
    #[serde(default = "default_scroll_multiplier")]
    pub multiplier: f32,
    /// Alternate scroll: trong alt-screen (vim/less/htop), mouse wheel
    /// gửi arrow keys thay vì scroll scrollback.
    #[serde(default = "default_true")]
    pub alternate_scroll: bool,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            multiplier: default_scroll_multiplier(),
            alternate_scroll: true,
        }
    }
}

fn default_scroll_multiplier() -> f32 {
    1.0
}

// ── Bell ─────────────────────────────────────────────────────────────

/// Nhóm Bell: enable/disable bell indicator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BellConfig {
    /// Bật/tắt bell indicator (🔔 khi nhận \x07).
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for BellConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

// ── Colors ───────────────────────────────────────────────────────────

/// Nhóm Colors: override foreground, background, cursor, selection,
/// ANSI 16 colors, min contrast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorsConfig {
    /// Override foreground (null = theme foreground, "#RRGGBB" để override).
    #[serde(default = "default_color_foreground")]
    pub foreground: Option<String>,
    /// Override background.
    #[serde(default)]
    pub background: Option<String>,
    /// Override cursor color.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Override selection highlight color (null = auto dark/light).
    #[serde(default = "default_color_selection")]
    pub selection: Option<String>,
    /// Override gutter text color (timestamp + line number). null = dim foreground.
    #[serde(default)]
    pub gutter_fg: Option<String>,
    /// Override gutter background color. null = same as terminal background.
    #[serde(default)]
    pub gutter_bg: Option<String>,
    /// Override clock text color [HH:MM:SS]. null = gutter_fg.
    #[serde(default)]
    pub clock_fg: Option<String>,
    /// Override line number color. null = gutter_fg.
    #[serde(default = "default_color_line_number_fg")]
    pub line_number_fg: Option<String>,
    /// Ngưỡng contrast tối thiểu (WCAG, 0.0 = tắt).
    #[serde(default = "default_min_contrast")]
    pub min_contrast: f32,
    /// Override ANSI 16 colors (tối đa 16, thiếu = dùng default).
    /// Vd: ["#000000", "#cc0000", ...]
    #[serde(default)]
    pub ansi: Vec<String>,
}

impl Default for ColorsConfig {
    fn default() -> Self {
        Self {
            foreground: default_color_foreground(),
            background: None,
            cursor: None,
            selection: default_color_selection(),
            gutter_fg: None,
            gutter_bg: None,
            clock_fg: None,
            line_number_fg: default_color_line_number_fg(),
            min_contrast: default_min_contrast(),
            ansi: Vec::new(),
        }
    }
}

fn default_color_foreground() -> Option<String> {
    Some("#efefef".into())
}

fn default_color_selection() -> Option<String> {
    Some("#343b48".into())
}

fn default_color_line_number_fg() -> Option<String> {
    Some("#2b7f99".into())
}

fn default_min_contrast() -> f32 {
    0.0
}

// ── Load / Save ─────────────────────────────────────────────────────

fn default_true() -> bool {
    true
}

/// Strip `//` line comments và `/* */` block comments khỏi JSON string.
/// JSON chuẩn không hỗ trợ comments, nhưng user thường thêm ghi chú.
/// Phải xử lý cẩn thận để không strip `//` bên trong string values.
fn strip_json_comments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut in_string = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            result.push(c);
            if c == '\\' {
                // Escape char — push next char blindly.
                if i + 1 < chars.len() {
                    result.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
        } else if c == '"' {
            in_string = true;
            result.push(c);
            i += 1;
        } else if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            // Line comment — skip to end of line.
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            // Block comment — skip to */.
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2; // skip */
        } else {
            result.push(c);
            i += 1;
        }
    }
    result
}

impl TerminalConfig {
    /// Load config từ file. Nếu file không tồn tại → tạo default + return default.
    /// Hỗ trợ `//` và `/* */` comments trong JSON.
    pub fn load() -> Self {
        let path = PathBuf::from(CONFIG_FILE);
        match std::fs::read_to_string(&path) {
            Ok(raw) => {
                let json = strip_json_comments(&raw);
                match serde_json::from_str::<TerminalConfig>(&json) {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        tracing::error!("terminal.json parse error: {e} — using defaults");
                        Self::default()
                    }
                }
            }
            Err(_) => {
                // File không tồn tại → tạo default file để user biết các option.
                let cfg = Self::default();
                if let Ok(json) = serde_json::to_string_pretty(&cfg) {
                    if std::fs::write(&path, json).is_ok() {
                        tracing::info!("Created default terminal.json at {path:?}");
                    }
                }
                cfg
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_line_comments() {
        let input = r#"{ "a": 1, // this is a comment
 "b": 2 }"#;
        let stripped = strip_json_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["a"], 1);
        assert_eq!(parsed["b"], 2);
    }

    #[test]
    fn strip_block_comments() {
        let input = r#"{ "a": 1 /* block */, "b": 2 }"#;
        let stripped = strip_json_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["a"], 1);
        assert_eq!(parsed["b"], 2);
    }

    #[test]
    fn strip_comments_preserves_strings_with_slashes() {
        let input = r#"{ "url": "https://example.com", "a": 1 // comment
 }"#;
        let stripped = strip_json_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["url"], "https://example.com");
        assert_eq!(parsed["a"], 1);
    }

    #[test]
    fn strip_comments_preserves_escaped_quotes() {
        let input = r#"{ "a": "say \"hi\" // not a comment", "b": 2 // real comment
 }"#;
        let stripped = strip_json_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["a"], "say \"hi\" // not a comment");
        assert_eq!(parsed["b"], 2);
    }

    #[test]
    fn default_config_serializes_all_groups() {
        let cfg = TerminalConfig::default();
        let json = serde_json::to_string_pretty(&cfg).unwrap();
        // Verify all groups present.
        assert!(json.contains("\"font\""));
        assert!(json.contains("\"cursor\""));
        assert!(json.contains("\"layout\""));
        assert!(json.contains("\"shell\""));
        assert!(json.contains("\"scroll\""));
        assert!(json.contains("\"bell\""));
        assert!(json.contains("\"colors\""));
    }

    #[test]
    fn empty_json_uses_defaults() {
        let json = "{}";
        let cfg: TerminalConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.cursor.shape, "block");
        assert_eq!(cfg.font.family.as_deref(), Some("Cascadia Mono"));
        assert_eq!(cfg.font.size, Some(15.0));
        assert_eq!(cfg.layout.line_height, 1.15);
        assert_eq!(cfg.layout.cell_width, None);
        assert_eq!(cfg.layout.padding.right, 5.0);
        assert_eq!(cfg.layout.padding.left, 10.0);
        assert!(cfg.bell.enabled);
        assert_eq!(cfg.scroll.multiplier, 1.0);
        assert_eq!(cfg.colors.foreground.as_deref(), Some("#efefef"));
        assert_eq!(cfg.colors.selection.as_deref(), Some("#343b48"));
        assert_eq!(cfg.colors.min_contrast, 0.0);
        assert_eq!(cfg.colors.line_number_fg.as_deref(), Some("#2b7f99"));
    }

    #[test]
    fn custom_config_parses() {
        let json = r##"{
            "font": {
                "family": "JetBrains Mono",
                "size": 15.0,
                "weight": "bold",
                "features": ["calt", "liga"]
            },
            "cursor": {
                "shape": "bar",
                "color": "#ff0000",
                "blink": false
            },
            "layout": {
                "line_height": 1.5,
                "cell_width": 8.0,
                "padding": { "top": 4.0, "right": 8.0, "bottom": 4.0, "left": 8.0 }
            },
            "scroll": {
                "multiplier": 3.0,
                "alternate_scroll": false
            },
            "bell": { "enabled": false },
            "colors": {
                "foreground": "#cccccc",
                "min_contrast": 3.0,
                "ansi": ["#000000", "#cc0000"]
            }
        }"##;
        let cfg: TerminalConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.font.family.as_deref(), Some("JetBrains Mono"));
        assert_eq!(cfg.font.size, Some(15.0));
        assert_eq!(cfg.font.weight, "bold");
        assert_eq!(cfg.cursor.shape, "bar");
        assert_eq!(cfg.cursor.color.as_deref(), Some("#ff0000"));
        assert!(!cfg.cursor.blink);
        assert_eq!(cfg.layout.line_height, 1.5);
        assert_eq!(cfg.layout.cell_width, Some(8.0));
        assert_eq!(cfg.layout.padding.top, 4.0);
        assert_eq!(cfg.scroll.multiplier, 3.0);
        assert!(!cfg.scroll.alternate_scroll);
        assert!(!cfg.bell.enabled);
        assert_eq!(cfg.colors.foreground.as_deref(), Some("#cccccc"));
        assert_eq!(cfg.colors.min_contrast, 3.0);
        assert_eq!(cfg.colors.ansi.len(), 2);
    }

    #[test]
    fn partial_config_uses_defaults_for_missing() {
        let json = r#"{ "font": { "size": 18.0 } }"#;
        let cfg: TerminalConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.font.size, Some(18.0));
        // Missing fields use defaults.
        assert_eq!(cfg.font.family.as_deref(), Some("Cascadia Mono"));
        assert_eq!(cfg.font.weight, "normal");
        assert_eq!(cfg.cursor.shape, "block");
        assert_eq!(cfg.layout.line_height, 1.15);
        assert_eq!(cfg.colors.foreground.as_deref(), Some("#efefef"));
    }
}
