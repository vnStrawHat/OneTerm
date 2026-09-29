//! Main layout for OneTerm.

use gpui::{
    AnyElement, Entity, IntoElement, ParentElement as _, Render, StyleRefinement, Window, div,
};
use gpui_component::StyledExt as _;

pub mod app_menus;
pub mod statusbar;
pub mod title_bar;
pub mod workspace;

pub use workspace::OneTermWorkspace;

/// Embed `view` as a cached view laid out at `style` (`US-0150`), or, while
/// accessibility is active, uncached in a box of the same style.
///
/// gpui-pre 0.3.7 builds AccessKit nodes during prepaint and does not replay
/// them for a reused view (`Window::reuse_prepaint`), so a cached view's menus
/// and buttons would leave the accessibility tree on every frame that reuses
/// it. AccessKit activation refreshes the window, so the switch applies from
/// the next frame. Drop the fallback once gpui replays the nodes.
pub(crate) fn cached_unless_a11y<V: Render>(
    view: Entity<V>,
    style: StyleRefinement,
    window: &Window,
) -> AnyElement {
    embed_view(view, style, !window.is_a11y_active())
}

/// [`cached_unless_a11y`] with the choice made by the caller, so a test can
/// drive the uncached branch (the test platform never activates AccessKit).
///
/// Both branches lay the view out at `style`: the cached one as the view's own
/// box, the uncached one as a wrapper the view's root (`size_full`) fills.
pub(crate) fn embed_view<V: Render>(
    view: Entity<V>,
    style: StyleRefinement,
    cached: bool,
) -> AnyElement {
    if cached {
        view.cached(style).into_any_element()
    } else {
        div().refine_style(&style).child(view).into_any_element()
    }
}
