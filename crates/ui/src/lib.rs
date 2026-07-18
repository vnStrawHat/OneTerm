//! OneTerm vendored fork of the `gpui-component` modules that the dock needs to
//! touch: `dock`, `resizable`, `history`, and `tab`.
//!
//! The upstream versions live in `gpui-component`'s `ui` crate. We vendor local
//! copies here so OneTerm can patch dock behavior (layout, panels, tabs, tiles)
//! without forking the whole `gpui-component` repository.
//!
//! ## Why these four modules together
//!
//! The dock uses a few `pub(crate)` items from its sibling modules that are
//! only reachable from *inside* the upstream `ui` crate:
//!
//! - `resizable`: `PANEL_MIN_SIZE`, `resize_handle`, `ResizablePanelState` and
//!   its methods (`insert_panel` / `remove_panel` / `replace_panel` / `clear` /
//!   `container_size`).
//! - `history`: the `History::ignore` field.
//! - `tab`: the `Tab::ix` / `Tab::tab_bar_prefix` builder setters.
//!
//! Vendoring the dock into a *separate* crate makes those `pub(crate)` items
//! unreachable, so we vendor the four modules together — they stay
//! in-crate-visible to one another exactly as upstream intended.
//!
//! ## How they stay in sync with the rest of `gpui-component`
//!
//! Every other sibling module the dock touches (`button`, `menu`, `scroll`,
//! `icon`, `animation`, plus the shared `ActiveTheme` / `h_flex` / `v_flex` /
//! `Placement` / `AxisExt` / `ElementExt` / `StyledExt` / `Selectable` /
//! `Sizable` / `Icon` / `IconName` helpers) is still referenced through the
//! `gpui_component` crate — so any `crate::<sibling>` import from the upstream
//! source has been rewritten to `gpui_component::<sibling>`.
//!
//! Only the cross-references between the four vendored modules
//! (`crate::dock::*`, `crate::resizable::*`, `crate::history::*`,
//! `crate::tab::*`, and `super::*`) keep resolving inside this crate. As a
//! result the vendored `PanelRegistry` is a **distinct global** from
//! upstream's: every OneTerm crate that talks to the dock (registering panels,
//! holding a `DockArea`, implementing `Panel`) MUST go through
//! `oneterm_ui::dock` — mixing the two registries would silently drop panel
//! registrations.
//!
//! OneTerm crates that use `resizable` / `tab` / `history` for their *own* UI
//! (e.g. `terminal-view`'s Space split tree uses `gpui_component::resizable`)
//! keep using the upstream versions: those types never cross into the dock's
//! private fields, so the two parallel copies do not collide.
//!
//! ## Translations
//!
//! The vendored `dock` / `tab` modules use `rust_i18n::t!` for a few visible
//! strings ("Dock.Unnamed", "Dock.Zoom In", etc.). The `t!` macro resolves
//! against the catalog of the crate where the call appears, so this crate
//! registers its own copy of the upstream `locales/ui.yml` via
//! `rust_i18n::i18n!` below.

// Register the translation catalog for this crate (mirrors the upstream
// `gpui-component` `ui` crate, which calls the same macro with the same
// `locales/ui.yml`). This MUST be at the crate root so `t!` calls inside the
// vendored modules resolve against this crate's catalog.
rust_i18n::i18n!("locales", fallback = "en");

pub mod dock;
pub mod history;
pub mod resizable;
pub mod tab;
