//! General Settings UI — a standalone window wrapping the gpui-component
//! `Settings` widget.
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
mod separators;
mod terminal;
mod updates;
mod window;

pub use window::open_settings_window;

// Re-exported for `OneTermWorkspace::bind_keys` (snapshot + apply key bindings).
pub(crate) use key_bindings::{KeyBindingsSnapshotGlobal, apply_key_bindings, init_state};
pub(crate) use separators::{items_with_separators, separator};

/// Open the General Settings window — command wrapper for the shell.
pub fn open_settings(cx: &mut gpui::App) {
    open_settings_window(cx).detach();
}

/// Open the About dialog with update status and install action.
pub fn open_about_dialog(window: &mut gpui::Window, cx: &mut gpui::App) {
    about::open_about_dialog(window, cx);
}

/// Snapshot the currently-registered key bindings, then apply OneTerm's
/// overrides. Owns the logic the shell's `bind_keys` used to inline; the shell
/// calls this via the workspace command registry so it needs no `views::settings`
/// dependency.
pub fn setup_key_bindings(cx: &mut gpui::App) {
    let snapshot: Vec<gpui::KeyBinding> = cx.key_bindings().borrow().bindings().cloned().collect();
    cx.set_global(KeyBindingsSnapshotGlobal(snapshot));
    init_state(cx);
    apply_key_bindings(cx);
}

/// Initialize Settings UI globals before startup auto-checks run.
pub fn init(cx: &mut gpui::App) {
    updates::UpdateUiState::init(cx);
}

/// Start the startup update check and notify when an update is available.
pub fn start_auto_check(window: &mut gpui::Window, cx: &mut gpui::App) {
    updates::start_auto_check(window, cx);
}
