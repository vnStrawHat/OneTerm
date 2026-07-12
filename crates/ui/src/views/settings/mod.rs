//! General Settings UI — a dock panel wrapping the gpui-component `Settings`
//! widget.
//!
//! The original `terminal/settings_panel.rs` only exposed the shell picker. This
//! module is the full General Settings (font, theme, key bindings, terminal
//! options, about), split into one file per page:
//!
//! - [`general`] — UI font size.
//! - [`key_bindings`] — configurable key bindings (grouped by origin).
//! - [`terminal`] — shell, font, cursor, layout, scroll, bell, security.
//! - [`appearance`] — theme mode + theme list.
//! - [`about`] — version info + links.
//!
//! See the roadmap entry "General Settings UI" in `docs/agents/structure.md`.

mod about;
mod appearance;
mod general;
mod key_bindings;
mod panel;
mod terminal;
mod terminal_options;
mod window;

pub use panel::SettingsPanel;
pub use window::open_settings_window;

// Re-exported for `OneTermWorkspace::bind_keys` (snapshot + apply key bindings).
pub(crate) use key_bindings::{KeyBindingsSnapshotGlobal, apply_key_bindings, init_state};
