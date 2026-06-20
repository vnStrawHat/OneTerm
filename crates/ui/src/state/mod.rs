//! AppState chia sẻ — Entity<T> state toàn cục.

pub mod app_state;
pub mod terminal_settings;

pub use app_state::AppState;
pub use terminal_settings::{
    TerminalBlink, TerminalCursorShape, TerminalSettings,
};