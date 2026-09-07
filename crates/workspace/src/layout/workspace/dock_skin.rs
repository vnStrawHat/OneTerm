//! OneTerm's dock skin wrapper.
//!
//! The center uses GPUI Kit's tab chrome. The right-dock mode panels render
//! their own headers, so their single-panel tab group intentionally omits the
//! extra outer title bar.

use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, AnyView, App, AppContext as _, Axis, Div, Empty, IntoElement as _, Pixels, Role,
    SharedString, Size, Stateful, StatefulInteractiveElement as _, Window,
};
use gpui_base::ResizeHandleContext;
use gpui_component::dock::{
    BasePanelView, DockArea, DockAreaRenderer, DockContext, DockSkin, DropIndicator, NodeId,
    PanelState, TabGroupContext, TabGroupRenderer, TileContext, TilesRenderer,
};
use oneterm_state::panel_names;

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
        DockArea::new(id, version, window, cx).with_renderer(Rc::new(OneTermDockSkin { inner }))
    });
    (
        area,
        skin.expect("DockSkin::new runs inside the DockArea constructor"),
    )
}

struct OneTermDockSkin {
    inner: Rc<DockSkin>,
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
        })
    }

    fn tiles_renderer(&self) -> Rc<dyn TilesRenderer> {
        Rc::new(OneTermTilesSkin {
            inner: self.inner.tiles_renderer(),
        })
    }
}

struct OneTermTabGroupSkin {
    inner: Rc<dyn TabGroupRenderer>,
}

impl OneTermTabGroupSkin {
    fn owns_header(group: &TabGroupContext, cx: &App) -> bool {
        group.panels().len() == 1
            && group.active_panel().is_some_and(|panel| {
                matches!(
                    panel.panel_name(cx),
                    panel_names::SSH_CLIENT | panel_names::AGENT
                )
            })
    }
}

impl TabGroupRenderer for OneTermTabGroupSkin {
    fn frame(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        self.inner
            .frame(group, window, cx)
            .role(Role::Pane)
            .aria_label("Terminal tabs")
    }

    fn content_frame(
        &self,
        group: &TabGroupContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Stateful<Div> {
        self.inner.content_frame(group, window, cx)
    }

    fn render_tab_bar(
        &self,
        group: &TabGroupContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        if Self::owns_header(group, cx) {
            Empty.into_any_element()
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
        self.inner.render_drop_indicator(indicator, window, cx)
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

struct OneTermTilesSkin {
    inner: Rc<dyn TilesRenderer>,
}

impl TilesRenderer for OneTermTilesSkin {
    fn frame(&self, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        self.inner
            .frame(window, cx)
            .role(Role::Pane)
            .aria_label("Terminal tiles")
    }

    fn tile_frame(&self, tile: &TileContext, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        self.inner.tile_frame(tile, window, cx)
    }

    fn render_drag_bar(&self, tile: &TileContext, window: &mut Window, cx: &mut App) -> AnyElement {
        self.inner.render_drag_bar(tile, window, cx)
    }

    fn render_resize_handles(
        &self,
        tile: &TileContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        self.inner.render_resize_handles(tile, window, cx)
    }

    fn panel_frame(&self, tile: &TileContext, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        self.inner.panel_frame(tile, window, cx)
    }

    fn render_overlay(
        &self,
        content: Size<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        self.inner.render_overlay(content, window, cx)
    }

    fn grid_size(&self, cx: &App) -> Pixels {
        self.inner.grid_size(cx)
    }
}
