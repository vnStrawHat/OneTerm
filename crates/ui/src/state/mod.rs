//! AppState chia sẻ — Entity<T> state toàn cục.

pub mod app_state;
pub mod terminal_config;
pub mod terminal_settings;

pub use app_state::AppState;
pub use terminal_settings::{
    ColorOverrides, TerminalBlink, TerminalCursorShape, TerminalPadding, TerminalSettings,
};
