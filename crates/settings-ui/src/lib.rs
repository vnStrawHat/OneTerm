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

/// Open the General Settings window — command wrapper for the shell.
pub fn open_settings(cx: &mut gpui::App) {
    open_settings_window(cx).detach();
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

// ── Separator helpers ────────────────────────────────────────────────
//
// The gpui-component `SettingGroup` renders its items inside a `GroupBox`
// with only a `.gap_4()` between them — no visible divider. To make each item
// visually distinguishable we insert thin horizontal separator lines between
// consecutive items, built as `SettingItem::Element` (custom render) items.
//
// When the user searches, separators are hidden automatically: a
// `SettingItem::Element` with no keywords only matches when the query is
// empty (see `SettingItem::is_match`).

use gpui::{App, Styled, Window, div, px};
use gpui_component::{
    ActiveTheme as _,
    setting::{RenderOptions, SettingItem},
};

/// A thin horizontal divider line rendered between setting items.
///
/// Slots into the standard `SettingGroup::item` pipeline as a custom-element
/// `SettingItem`. Hidden during search (no keywords → only matches empty query).
pub(crate) fn separator() -> SettingItem {
    SettingItem::render(
        |_options: &RenderOptions, _window: &mut Window, cx: &mut App| {
            div().w_full().h(px(1.)).bg(cx.theme().border)
        },
    )
}

/// Interleave the given items with `separator()` between each consecutive pair.
///
/// A single item (or empty slice) is returned unchanged — no leading/trailing
/// separator is added.
pub(crate) fn items_with_separators(items: Vec<SettingItem>) -> Vec<SettingItem> {
    let mut out = Vec::with_capacity(items.len() * 2);
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            out.push(separator());
        }
        out.push(item);
    }
    out
}
