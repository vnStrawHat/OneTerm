//! Shared AppState — global Entity<T> state.

pub mod app_state;
pub mod session_state;
pub mod terminal_config;
pub mod terminal_settings;
pub mod ui_config;

pub use app_state::AppState;
pub use session_state::{SshSession, SshSessionStore};
pub use terminal_config::TabTitleMode;
pub use terminal_settings::{
    ColorOverrides, TerminalBlink, TerminalCursorShape, TerminalPadding, TerminalSettings,
};
pub use ui_config::UiConfig;
