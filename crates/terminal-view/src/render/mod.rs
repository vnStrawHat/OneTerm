//! Rendering for [`LocalTerminalView`] — the per-frame element tree plus the
//! color-override and overlay helpers.
//!
//! - [`view_render`] — the `Render`/`Focusable` impls
//! - [`overlays`] — bell / progress overlay elements
//! - [`theme_apply`] — apply OSC dynamic-color overrides to the theme

pub(crate) mod overlays;
pub(crate) mod theme_apply;
mod view_render;
