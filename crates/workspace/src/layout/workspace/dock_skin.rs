//! OneTerm's dock skin wrapper.
//!
//! The center uses GPUI Kit's tab chrome. The right dock's SSH Client panel
//! renders its own headers, so their single-panel tab group intentionally omits the
//! extra outer title bar. With Vertical Tabs on (`IN-0055`), an all-terminal center
//! group draws the [`tab_rail`](super::tab_rail) beside its content instead of the
//! kit's strip above it.

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
        })
    }
}

struct OneTermTabGroupSkin {
    inner: Rc<dyn TabGroupRenderer>,
    area: WeakEntity<DockArea>,
    /// The rail's scroll position and last displayed tab; unused while the strip is drawn.
    rail: TabRail,
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
}

impl TabGroupRenderer for OneTermTabGroupSkin {
    fn frame(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        // Base calls `frame`, `render_tab_bar` and `content_frame` in that order within one
        // render, so the rail rule and the width are settled here and the other two read them.
        let draws = self.draws_rail(group, cx);
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
