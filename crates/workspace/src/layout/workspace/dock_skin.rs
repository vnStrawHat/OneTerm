//! OneTerm's dock skin wrapper.
//!
//! The center uses GPUI Kit's tab chrome. The right dock's SSH Client panel
//! renders its own headers, so their single-panel tab group intentionally omits the
//! extra outer title bar. With Vertical Tabs on (`IN-0055`), an all-terminal center
//! group draws the [`tab_rail`](super::tab_rail) beside its content instead of the
//! kit's strip above it.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, AnyView, App, AppContext as _, Axis, Div, Empty, IntoElement as _, Role,
    SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, WeakEntity, Window,
    prelude::FluentBuilder as _,
};
use gpui_base::ResizeHandleContext;
use gpui_component::dock::{
    BasePanelView, DockArea, DockAreaRenderer, DockContext, DockPlacement, DockSkin, DropIndicator,
    NodeId, PanelState, TabGroupContext, TabGroupRenderer,
};
use oneterm_settings::UiConfig;
use oneterm_state::panel_names;

use super::tab_rail::{self, TabRail};

/// Names the drop placeholder in the debug-bounds map. A no-op outside test builds.
#[cfg(test)]
pub(super) const DROP_INDICATOR_SELECTOR: &str = "oneterm-drop-indicator";

pub(super) fn dock_area(
    id: impl Into<SharedString>,
    version: Option<usize>,
    window: &mut Window,
    cx: &mut App,
) -> (gpui::Entity<DockArea>, Rc<DockSkin>) {
    let mut skin = None;
    let area = cx.new(|cx| {
        let inner = DockSkin::new(cx);
        skin = Some(inner.clone());
        let area = cx.weak_entity();
        DockArea::new(id, version, window, cx)
            .with_renderer(Rc::new(OneTermDockSkin { inner, area }))
    });
    (
        area,
        skin.expect("DockSkin::new runs inside the DockArea constructor"),
    )
}

struct OneTermDockSkin {
    inner: Rc<DockSkin>,
    /// The dock area this skin draws, so a tab group can tell whether it is in the center.
    area: WeakEntity<DockArea>,
}

impl DockAreaRenderer for OneTermDockSkin {
    fn frame(&self, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        self.inner
            .frame(window, cx)
            .role(Role::Pane)
            .aria_label("OneTerm workspace")
    }

    fn split_frame(
        &self,
        node: NodeId,
        axis: Axis,
        window: &mut Window,
        cx: &mut App,
    ) -> Stateful<Div> {
        self.inner.split_frame(node, axis, window, cx)
    }

    fn center_frame(&self, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        self.inner.center_frame(window, cx)
    }

    fn render_split_handle(
        &self,
        handle: &ResizeHandleContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        self.inner.render_split_handle(handle, window, cx)
    }

    fn render_dock(
        &self,
        dock: &DockContext,
        content: AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        self.inner.render_dock(dock, content, window, cx)
    }

    fn build_placeholder(
        &self,
        state: &PanelState,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Arc<dyn BasePanelView>> {
        self.inner.build_placeholder(state, window, cx)
    }

    fn tab_group_renderer(&self) -> Rc<dyn TabGroupRenderer> {
        Rc::new(OneTermTabGroupSkin {
            inner: self.inner.tab_group_renderer(),
            area: self.area.clone(),
            rail: TabRail::new(),
            terminal_content: Cell::new(false),
        })
    }
}

struct OneTermTabGroupSkin {
    inner: Rc<dyn TabGroupRenderer>,
    area: WeakEntity<DockArea>,
    /// The rail's scroll position and last displayed tab; unused while the strip is drawn.
    rail: TabRail,
    /// Whether the panel this group displays is a terminal, worked out by `frame` (which base calls
    /// before the drop indicator is asked for): a terminal's content takes no dock drop, so the
    /// kit's split placeholder must not promise one.
    terminal_content: Cell<bool>,
}

impl OneTermTabGroupSkin {
    fn owns_header(group: &TabGroupContext, cx: &App) -> bool {
        group.panels().len() == 1
            && group
                .active_panel()
                .is_some_and(|panel| matches!(panel.panel_name(cx), panel_names::SSH_CLIENT))
    }

    /// Whether this group draws the vertical rail now: the switch is on and the group is an
    /// all-terminal group of the center. Read every frame, so the layout flips live.
    fn draws_rail(&self, group: &TabGroupContext, cx: &App) -> bool {
        let enabled = UiConfig::read_vertical_tabs(cx).enabled;
        if !enabled || group.is_collapsed() {
            return false;
        }
        let in_centre = self.area.upgrade().is_some_and(|area| {
            area.read(cx)
                .layout(DockPlacement::Center)
                .is_some_and(|tree| tree.find_node(group.node()).is_some())
        });
        tab_rail::draws_rail(
            enabled,
            in_centre,
            group.panels().iter().map(|panel| panel.panel_name(cx)),
        )
    }

    /// A group that draws the rail is never left zoomed. The rail has no zoom button (owner
    /// rework 4), a lone centre group looks the same zoomed or not, and the kit refuses to drag
    /// from or drop on a zoomed group, so a stray `ToggleZoom` (saved in `docks.json` and
    /// restored on every start) silently killed reorder and the drop into a Space, with no way
    /// out (`US-0196` rework 9, owner test failure). Every way into a zoom ends in a frame of
    /// this group, so this is the one place that lets go of it; the dock's own zoom of another
    /// group (the right dock's SFTP browser) is not this group's and is left alone. Deferred:
    /// the dock area is mid-render here.
    fn release_zoom(&self, node: NodeId, window: &mut Window, cx: &mut App) {
        let area = self.area.clone();
        window.defer(cx, move |window, cx| {
            _ = area.update(cx, |area, cx| {
                if area.zoomed_group() == Some(node) {
                    area.set_zoomed_out(window, cx);
                }
            });
        });
    }
}

impl TabGroupRenderer for OneTermTabGroupSkin {
    fn frame(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        // Base calls `frame`, `render_tab_bar` and `content_frame` in that order within one
        // render, so the rail rule and the width are settled here and the other two read them.
        let draws = self.draws_rail(group, cx);
        if draws && group.is_zoomed() {
            self.release_zoom(group.node(), window, cx);
        }
        self.terminal_content.set(
            group
                .active_panel()
                .is_some_and(|panel| panel.panel_name(cx) == panel_names::TERMINAL),
        );
        self.rail.begin_frame(draws, cx);
        let frame = self
            .inner
            .frame(group, window, cx)
            .role(Role::Pane)
            .aria_label("Terminal tabs");
        if draws {
            self.rail.track_group(frame.relative())
        } else {
            frame
        }
    }

    fn content_frame(
        &self,
        group: &TabGroupContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Stateful<Div> {
        // A margin, not padding: the rail must stay outside the bounds base installs drop
        // handling on, or a panel dragged over the rail would be measured as a split of the
        // content region under it.
        self.inner
            .content_frame(group, window, cx)
            .when(self.rail.draws(), |this| this.ml(self.rail.width()))
    }

    fn render_tab_bar(
        &self,
        group: &TabGroupContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        if Self::owns_header(group, cx) {
            Empty.into_any_element()
        } else if self.rail.draws() {
            self.rail.render(group, window, cx)
        } else {
            self.inner.render_tab_bar(group, window, cx)
        }
    }

    fn render_active_panel(
        &self,
        panel: AnyView,
        group: &TabGroupContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        self.inner.render_active_panel(panel, group, window, cx)
    }

    fn render_drop_indicator(
        &self,
        indicator: DropIndicator,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        // The terminal panel consumes a `DragPanel` released on its content (it neither splits nor
        // merges the centre, `US-0196` rework 9), so no split placeholder is drawn over it either.
        if self.terminal_content.get() {
            return None;
        }
        let placeholder = self.inner.render_drop_indicator(indicator, window, cx);
        // Lets a test ask a drawn frame whether the placeholder is there; same box as the content
        // frame, so the placeholder's own position is unchanged.
        #[cfg(test)]
        let placeholder = placeholder.map(|placeholder| {
            use gpui::{InteractiveElement as _, ParentElement as _, div};
            div()
                .debug_selector(|| DROP_INDICATOR_SELECTOR.to_string())
                .absolute()
                .size_full()
                .child(placeholder)
                .into_any_element()
        });
        placeholder
    }

    fn render_empty(
        &self,
        group: &TabGroupContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        self.inner.render_empty(group, window, cx)
    }
}
