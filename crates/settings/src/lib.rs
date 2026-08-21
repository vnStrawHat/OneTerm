//! OneTerm settings.
//!
//! Holds both the on-disk schema (`terminal.json` via [`terminal_config`],
//! `ui_config.json` via [`ui_config`]) and the live global settings entities
//! ([`TerminalSettings`], [`UiConfig`]). No GPUI views live here — only the
//! settings data + globals that the shell and feature crates read/write.

pub mod terminal_config;
pub mod terminal_settings;
pub mod ui_config;

pub use terminal_config::{
    CompletionConfig, CompletionSources, EditorConfig, EditorMode, LoggingConfig,
    SemanticHighlightingMode, SftpConfig, SshSettingsConfig, TabTitleMode, TerminalConfig,
};
pub use terminal_settings::{
    ColorOverrides, TerminalBlink, TerminalCursorShape, TerminalPadding, TerminalSettings,
};
pub use ui_config::UiConfig;
