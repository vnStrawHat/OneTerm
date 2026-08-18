//! Unit tests for terminal config parsing, defaults, and persistence.

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
    assert_eq!(cfg.font.family.as_deref(), Some("Lilex"));
    assert_eq!(cfg.font.size, Some(15.0));
    assert_eq!(cfg.layout.line_height, 1.2);
    assert_eq!(cfg.layout.cell_width, None);
    assert_eq!(cfg.layout.padding.right, 5.0);
    assert_eq!(cfg.layout.padding.left, 10.0);
    assert!(cfg.bell.enabled);
    assert_eq!(cfg.scroll.multiplier, 1.0);
    assert_eq!(cfg.scroll.scrollback_history, 10_000);
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
                "alternate_scroll": false,
                "scrollback_history": 50000
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
    assert_eq!(cfg.scroll.scrollback_history, 50000);
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
    assert_eq!(cfg.font.family.as_deref(), Some("Lilex"));
    assert_eq!(cfg.font.weight, "normal");
    assert_eq!(cfg.cursor.shape, "block");
    assert_eq!(cfg.layout.line_height, 1.2);
    assert_eq!(cfg.colors.foreground.as_deref(), Some("#efefef"));
}

#[test]
fn tab_title_defaults_to_default() {
    // An empty/missing layout group → TabTitleMode::Default ("default").
    let cfg: TerminalConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(
        cfg.layout.tab_title,
        crate::terminal_config::TabTitleMode::Default
    );
}

#[test]
fn tab_title_osc_parses_and_round_trips() {
    let cfg: TerminalConfig =
        serde_json::from_str(r#"{ "layout": { "tab_title": "osc" } }"#).unwrap();
    assert_eq!(
        cfg.layout.tab_title,
        crate::terminal_config::TabTitleMode::Osc
    );
    // Round-trip back to JSON and parse again.
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(json.contains("\"tab_title\":\"osc\""), "got: {json}");
    let again: TerminalConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(
        again.layout.tab_title,
        crate::terminal_config::TabTitleMode::Osc
    );
}

#[test]
fn explicit_path_persistence_is_isolated_and_quarantines_corruption() {
    let directory = std::env::temp_dir().join(format!(
        "oneterm-terminal-config-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("terminal.json");
    let missing = TerminalConfig::load_from(&path).unwrap();
    assert_eq!(missing.font.family, FontConfig::default().family);
    assert!(path.exists());
    let config = TerminalConfig::default();
    config.save_to(&path).unwrap();
    assert_eq!(
        TerminalConfig::load_from(&path).unwrap().font.family,
        config.font.family
    );
    std::fs::write(&path, b"{not-json").unwrap();
    let loaded = TerminalConfig::load_from(&path).unwrap();
    assert_eq!(loaded.font.family, FontConfig::default().family);
    assert!(!path.exists());
    assert!(std::fs::read_dir(&directory).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".invalid-")
    }));
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn legacy_fixture_migrates_and_current_save_is_idempotent() {
    let legacy = include_str!("../../tests/fixtures/persistence/terminal-v0.json");
    let config = TerminalConfig::parse_document(legacy).unwrap();
    assert_eq!(config.font.family.as_deref(), Some("Legacy Mono"));
    let directory = std::env::temp_dir().join(format!(
        "oneterm-terminal-schema-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    let path = directory.join("terminal.json");
    config.save_to(&path).unwrap();
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(value["schema_version"], CURRENT_SCHEMA_VERSION);
    let restored = TerminalConfig::load_from(&path).unwrap();
    assert_eq!(restored.font.family, config.font.family);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn unreadable_document_is_a_typed_load_error_and_is_left_untouched() {
    let directory = std::env::temp_dir().join(format!(
        "oneterm-terminal-unreadable-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    // A directory in place of the file fails to read with something other than
    // NotFound on every platform, standing in for a permission failure.
    let path = directory.join("terminal.json");
    std::fs::create_dir_all(&path).unwrap();
    let error = TerminalConfig::load_from(&path).unwrap_err();
    assert!(
        matches!(&error, AppError::ConfigLoad { document, .. } if document == "terminal.json"),
        "expected ConfigLoad, got {error}"
    );
    assert!(path.is_dir(), "an unreadable document must not be replaced");
    let _ = std::fs::remove_dir_all(directory);
}
