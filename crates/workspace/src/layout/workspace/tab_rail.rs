//! The vertical tab rail (`IN-0055`, `US-0196`): a tab group's tabs drawn as a list down its
//! left side instead of the kit's strip above its content.
//!
//! GPUI Kit lays every tab group out as a column (tab bar, then content) and its `TabBar` has
//! no vertical mode, so the rail is OneTerm's own element: positioned absolutely in the group
//! frame, with the content frame pushed right by a margin of the rail's width
//! (`dock_skin.rs`). Each row wraps the panel's own `title()` element (drawn in the vertical
//! row layout), so every per-tab behaviour comes from the one row implementation the strip
//! also uses. Selection, drops and zoom go through the callbacks `TabGroupContext` hands a
//! skin; nothing here touches a panel or a session.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, ScrollHandle, SharedString, Size,
    StatefulInteractiveElement as _, Styled as _, Window, accesskit, div,
    prelude::FluentBuilder as _, px, size,
};
use gpui_base::{Tab, Tabs};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    dock::{BasePanelView, DragPanel, PanelHandle, PanelId, TabGroupContext, ToggleZoom},
    h_flex,
    scroll::{ScrollableElement as _, Scrollbar},
};
use oneterm_state::{TabRowLayout, panel_names};
use oneterm_theme::AppIcon;

/// The rail's width in `US-0196`: the strip's tab ceiling, so a label fits exactly as much as
/// it does today. `US-0197` replaces this with the stored, clamped preference.
pub(super) const RAIL_WIDTH: Pixels = px(220.);

/// Names in the debug-bounds map, so a test can ask a really drawn frame where things are. A
/// no-op outside test builds.
pub(super) const RAIL_SELECTOR: &str = "oneterm-tab-rail";
pub(super) const LIST_SELECTOR: &str = "oneterm-tab-rail-list";
pub(super) const ROW_SELECTOR: &str = "oneterm-tab-rail-row";
pub(super) const GRIP_SELECTOR: &str = "oneterm-tab-rail-grip";
pub(super) const GRIP_GLYPH_SELECTOR: &str = "oneterm-tab-rail-grip-glyph";
pub(super) const EMPTY_SELECTOR: &str = "oneterm-tab-rail-empty";

/// Height of the header (`+` and zoom): the strip's height.
const HEADER_HEIGHT: Pixels = px(32.);
/// Two lines (`TerminalPanel::title`'s vertical row): the title, then the directory and status.
pub(super) const ROW_HEIGHT: Pixels = px(TabRowLayout::VERTICAL_ROW_HEIGHT);
/// The reorder grip: the strip tab's leading padding.
const GRIP_WIDTH: Pixels = px(12.);
/// The grip glyph (`Icon::xsmall`).
const GRIP_GLYPH_SIZE: f32 = 12.;
const ACCENT_BAR_WIDTH: Pixels = px(2.);
/// What the drag preview occupies, reported to base so a drop placeholder knows where to fly
/// in from. The chip is content-sized between a floor and a ceiling; this is a typical size.
const DRAG_PREVIEW_SIZE: Size<Pixels> = size(px(140.), px(30.));

/// Whether a tab group draws the rail: the switch is on, the group is in the centre, and it
/// holds at least one panel and nothing but terminal panels. Every other group (the SSH Client
/// group, a right-dock group a terminal tab was dropped into) keeps what it draws today.
pub(super) fn draws_rail<'a>(
    enabled: bool,
    in_centre: bool,
    panels: impl IntoIterator<Item = &'a str>,
) -> bool {
    if !enabled || !in_centre {
        return false;
    }
    let mut panels = panels.into_iter().peekable();
    panels.peek().is_some() && panels.all(|name| name == panel_names::TERMINAL)
}

/// Whether the displayed tab changed since the last frame, recording `active` as seen: a
/// change scrolls the new row into view, while a clicked row is left where it is.
fn active_changed(last_active_ix: &Cell<Option<usize>>, active: usize) -> bool {
    last_active_ix.replace(Some(active)) != Some(active)
}

/// The slot a panel dropped below the last row lands in, `None` meaning "append". A panel
/// dropped past the last tab of its own group takes the last slot; one from elsewhere is
/// appended in the background (mirrors the kit strip's empty space).
fn slot_below_last_row<N: PartialEq>(source: N, group: N, tabs: usize) -> Option<usize> {
    (source == group).then(|| tabs.saturating_sub(1))
}

/// Which edge of a hovered row the drop line is drawn on.
///
/// `drop_panel` puts a dropped tab *in the hovered row's slot*: a tab dragged from an earlier
/// place in its own group lands after that row (the others shift up), and any other tab lands
/// before it. The line shows where the tab will actually land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DropEdge {
    /// Dropping on the dragged row itself changes nothing.
    Nowhere,
    Above,
    Below,
}

/// `from` is the dragged tab's place in this group (`None` for a tab from another group),
/// `target` the hovered row's.
fn drop_edge(from: Option<usize>, target: usize) -> DropEdge {
    match from {
        Some(from) if from == target => DropEdge::Nowhere,
        Some(from) if from < target => DropEdge::Below,
        _ => DropEdge::Above,
    }
}

/// Which zoom button the header offers, if any: Zoom Out whenever the group is zoomed,
/// whatever the panel's control says (a panel that stopped offering the control must not strand
/// the user), Zoom In when the control puts it in the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ZoomButton {
    In,
    Out,
}

fn zoom_button(zoomed: bool, toolbar_zoom: bool) -> Option<ZoomButton> {
    match (zoomed, toolbar_zoom) {
        (true, _) => Some(ZoomButton::Out),
        (false, true) => Some(ZoomButton::In),
        (false, false) => None,
    }
}

/// Where one row sits: the panel's index in the group, its place among the visible rows, how
/// many rows there are, and whether it is the displayed tab.
#[derive(Clone)]
struct RowPlace {
    ix: usize,
    position: usize,
    rows: usize,
    selected: bool,
    /// Every panel of the group in tab order, to place a dragged tab among the rows.
    order: Rc<Vec<PanelId>>,
}

/// One tab group's rail state: the list's scroll position and the displayed tab the last frame
/// drew. Built per group (the dock area asks for one renderer per container), like the kit
/// strip's own.
pub(super) struct TabRail {
    scroll_handle: ScrollHandle,
    last_active_ix: Rc<Cell<Option<usize>>>,
    /// Whether the rows overflowed the list last frame, so the scrollbar's gutter is reserved.
    overflowing: Cell<bool>,
}

impl TabRail {
    pub(super) fn new() -> Self {
        Self {
            scroll_handle: ScrollHandle::default(),
            last_active_ix: Rc::new(Cell::new(None)),
            overflowing: Cell::new(false),
        }
    }

    /// The rail for `group`: header, then one row per visible tab, then room to drop a panel
    /// past the last one.
    pub(super) fn render(
        &self,
        group: &TabGroupContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let visible: Vec<usize> = group
            .panels()
            .iter()
            .enumerate()
            .filter(|(_, panel)| panel.visible(cx))
            .map(|(ix, _)| ix)
            .collect();
        let active_ix = group.active_ix();
        // The panel on screen, not the stored index: a hidden displayed tab falls back to the
        // first visible one.
        let displayed_ix = group.active_panel().and_then(|displayed| {
            let displayed = displayed.panel_id(cx);
            group
                .panels()
                .iter()
                .position(|panel| panel.panel_id(cx) == displayed)
        });
        if active_changed(&self.last_active_ix, active_ix)
            && let Some(position) = visible.iter().position(|ix| *ix == active_ix)
        {
            self.scroll_handle.scroll_to_item(position);
        }

        let droppable = group.is_droppable();
        let tabs = group.panels().len();
        let rows = visible.len();
        let tab_bar = cx.theme().tokens.tab_bar;
        let border = cx.theme().border;

        // The scrollbar overlays the list's right edge, where each row's trailing status icon
        // sits, so the rows leave its gutter free once they overflow. Whether they do is only known after
        // a paint, so it is read from the previous frame; when it changes, the next frame is
        // requested with `request_animation_frame` (a `refresh` inside a draw is ignored).
        let overflowing = self.scroll_handle.max_offset().y > px(0.);
        if self.overflowing.replace(overflowing) != overflowing {
            window.request_animation_frame();
        }
        let order: Rc<Vec<PanelId>> = Rc::new(
            group
                .panels()
                .iter()
                .map(|panel| panel.panel_id(cx))
                .collect(),
        );

        let header = self.header(group, window, cx);
        let list = Tabs::new("tab-rail-list")
            .debug_selector(|| LIST_SELECTOR.to_string())
            .aria_orientation(accesskit::Orientation::Vertical)
            .size_full()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .when(overflowing, |this| this.pr(Scrollbar::width()))
            .track_scroll(&self.scroll_handle)
            .children(visible.into_iter().enumerate().map(|(position, ix)| {
                let place = RowPlace {
                    ix,
                    position,
                    rows,
                    selected: displayed_ix == Some(ix),
                    order: order.clone(),
                };
                self.row(group, place, window, cx)
            }))
            .child(
                // Room to move a panel past the last tab.
                div()
                    .id("tab-rail-empty-space")
                    .debug_selector(|| EMPTY_SELECTOR.to_string())
                    .flex_1()
                    .min_h(px(16.))
                    .when(droppable, |this| {
                        this.drag_over::<DragPanel>(|this, _, _, cx| {
                            this.bg(cx.theme().tokens.drop_target)
                        })
                        .on_drop({
                            let group = group.clone();
                            let node = group.node();
                            move |drag: &DragPanel, window, cx| {
                                let ix = slot_below_last_row(drag.source(), node, tabs);
                                group.drop_panel(drag.clone(), ix, false, window, cx);
                            }
                        })
                    }),
            );

        div()
            .id("tab-rail")
            .debug_selector(|| RAIL_SELECTOR.to_string())
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .w(RAIL_WIDTH)
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(tab_bar)
            .border_r_1()
            .border_color(border)
            .child(header)
            .child(
                div()
                    .id("tab-rail-scroll")
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_hidden()
                    .child(list)
                    .vertical_scrollbar(&self.scroll_handle),
            )
            .into_any_element()
    }

    /// `+` (the active panel's own title suffix: OneTerm's New Terminal menu) and the zoom
    /// button. No `...` menu (its tab list is the rail itself) and no dock toggle (the status
    /// bar and `Ctrl+Shift+B` own it).
    fn header(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> AnyElement {
        let border = cx.theme().border;
        let handle = group.active_panel().and_then(PanelHandle::of);
        let toolbar_zoom = group
            .active_panel()
            .filter(|panel| panel.zoomable(cx))
            .and_then(PanelHandle::of)
            .and_then(|handle| handle.zoom_control(cx))
            .is_some_and(|control| control.toolbar_visible());
        let zoomed = group.is_zoomed();
        let suffix = handle.and_then(|handle| handle.title_suffix(window, cx));

        h_flex()
            .h(HEADER_HEIGHT)
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .px_2()
            .border_b_1()
            .border_color(border)
            .child(h_flex().gap_1().children(suffix))
            .child(
                h_flex()
                    .gap_1()
                    .children(zoom_button(zoomed, toolbar_zoom).map(|button| {
                        let (id, icon, tooltip) = match button {
                            ZoomButton::Out => ("zoom-out", IconName::Minimize, "Zoom Out"),
                            ZoomButton::In => ("zoom-in", IconName::Maximize, "Zoom In"),
                        };
                        Button::new(id)
                            .icon(icon)
                            .xsmall()
                            .ghost()
                            .tab_stop(false)
                            .tooltip_with_action(tooltip, &ToggleZoom, None)
                            .selected(zoomed)
                            .debug_selector(move || format!("{RAIL_SELECTOR}-{id}"))
                            .on_click({
                                let group = group.clone();
                                move |_, window, cx| group.toggle_zoom(window, cx)
                            })
                    })),
            )
            .into_any_element()
    }

    /// The row for the panel at `place.ix`: the grip, then the panel's own tab element.
    fn row(
        &self,
        group: &TabGroupContext,
        place: RowPlace,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let RowPlace {
            ix,
            position,
            rows,
            selected,
            order,
        } = place;
        let panel = &group.panels()[ix];
        let handle = PanelHandle::of(panel);
        // The kit strip prefers a panel's short name over its title element; so does the rail.
        // The vertical layout holds only while the title element is built.
        let title = {
            let mut scope = TabRowLayout::Vertical.scope(cx);
            match handle {
                Some(handle) => match handle.tab_name(&scope) {
                    Some(name) => div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(name)
                        .into_any_element(),
                    None => handle.title(window, &mut scope),
                },
                None => SharedString::from(panel.panel_name(&scope)).into_any_element(),
            }
        };

        let droppable = group.is_droppable();
        let drag = group
            .is_draggable()
            .then(|| group.drag_panel(ix, cx))
            .flatten();
        let theme = cx.theme();
        let (foreground, active_foreground, active_background) = (
            theme.tab_foreground,
            theme.tab_active_foreground,
            theme.tokens.tab_active,
        );
        let muted = theme.muted_foreground;
        // Not `table_active_border`: OneTerm blanks that for the data tables after every theme
        // change, which made the bar vanish.
        let accent = oneterm_theme::selection_accent(cx);
        let hover_group = SharedString::from(format!("tab-rail-row-{ix}"));

        let grip = div()
            .id(("tab-rail-grip", ix))
            .debug_selector(move || format!("{GRIP_SELECTOR}-{ix}"))
            .w(GRIP_WIDTH)
            .h_full()
            .flex_shrink_0()
            .flex()
            .items_start()
            .justify_center()
            .when_some(drag, |this, drag| {
                this.cursor_grab()
                    .child(
                        div()
                            .id(("tab-rail-grip-glyph", ix))
                            .debug_selector(move || format!("{GRIP_GLYPH_SELECTOR}-{ix}"))
                            // The glyph is centred on the title line, not on the whole row.
                            .mt(px(
                                TabRowLayout::vertical_first_line_centre() - GRIP_GLYPH_SIZE / 2.
                            ))
                            .opacity(0.)
                            .group_hover(hover_group.clone(), |style| style.opacity(1.))
                            .child(Icon::new(AppIcon::GripVertical).xsmall().text_color(muted)),
                    )
                    .on_drag(drag, {
                        let panel = panel.clone();
                        move |drag, offset, _, cx| {
                            cx.stop_propagation();
                            drag.set_drag_offset(offset);
                            drag.set_preview_size(DRAG_PREVIEW_SIZE);
                            cx.new(|_| RailDragPreview {
                                panel: panel.clone(),
                            })
                        }
                    })
            });

        Tab::new(("tab-rail-row", ix))
            .debug_selector(move || format!("{ROW_SELECTOR}-{ix}"))
            .selected(selected)
            .set_position(position + 1, rows)
            .on_click({
                let group = group.clone();
                let last_active_ix = self.last_active_ix.clone();
                move |_: &ClickEvent, window, cx| {
                    // The rail reveals a clicked row itself, so the change must not snap it
                    // into view.
                    last_active_ix.set(Some(ix));
                    group.select_tab(ix, window, cx);
                }
            })
            .styles(|styles| {
                styles.selected(|style| style.text_color(active_foreground).bg(active_background))
            })
            .relative()
            .w_full()
            .h(ROW_HEIGHT)
            .flex_shrink_0()
            .justify_start()
            .text_sm()
            .text_color(foreground)
            .group(hover_group)
            .hover(move |style| style.text_color(active_foreground))
            .when(droppable, |this| {
                this.drag_over::<DragPanel>({
                    let node = group.node();
                    move |this, drag, _, cx| {
                        let from = (drag.source() == node)
                            .then(|| order.iter().position(|id| *id == drag.panel()))
                            .flatten();
                        let line = cx.theme().drag_border;
                        match drop_edge(from, ix) {
                            DropEdge::Nowhere => this,
                            DropEdge::Above => this.border_t_2().border_color(line),
                            DropEdge::Below => this.border_b_2().border_color(line),
                        }
                    }
                })
                .on_drop({
                    let group = group.clone();
                    move |drag: &DragPanel, window, cx| {
                        group.drop_panel(drag.clone(), Some(ix), true, window, cx);
                    }
                })
            })
            .when(selected, |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(ACCENT_BAR_WIDTH)
                        .bg(accent),
                )
            })
            .child(grip)
            .child(title)
            .into_any_element()
    }
}

/// The preview that follows the cursor while a row is dragged by its grip. The kit's own
/// preview has private fields, so the rail draws the same small chip itself.
struct RailDragPreview {
    panel: Arc<dyn BasePanelView>,
}

impl Render for RailDragPreview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = match PanelHandle::of(&self.panel) {
            Some(handle) => handle.title(window, cx),
            None => SharedString::from(self.panel.panel_name(cx)).into_any_element(),
        };
        div()
            .id("tab-rail-drag")
            .cursor_grab()
            .py_1()
            .px_3()
            .min_w_24()
            .max_w(px(240.))
            .overflow_hidden()
            .whitespace_nowrap()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            // The selected tab's text on its own fill: the pair the contrast check measures.
            .text_color(cx.theme().tab_active_foreground)
            .bg(cx.theme().tokens.tab_active)
            .opacity(0.95)
            .child(title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_centre_group_of_terminal_tabs_draws_the_rail() {
        let terminal = panel_names::TERMINAL;
        assert!(draws_rail(true, true, [terminal]));
        assert!(draws_rail(true, true, [terminal, terminal, terminal]));
        // The switch is off, the group is outside the centre, or it holds nothing.
        assert!(!draws_rail(false, true, [terminal]));
        assert!(!draws_rail(true, false, [terminal, terminal]));
        assert!(!draws_rail(true, true, []));
        // The SSH Client group, and a group a terminal tab shares with any other panel.
        assert!(!draws_rail(true, true, [panel_names::SSH_CLIENT]));
        assert!(!draws_rail(true, true, [terminal, panel_names::SFTP]));
        assert!(!draws_rail(true, true, [panel_names::SESSION, terminal]));
    }

    #[test]
    fn a_change_of_displayed_tab_is_reported_once() {
        let last = Cell::new(None);
        assert!(
            active_changed(&last, 0),
            "the first frame scrolls to the displayed tab"
        );
        assert!(!active_changed(&last, 0));
        assert!(active_changed(&last, 3));
        assert!(!active_changed(&last, 3));
        // A click records the row it reveals itself, so the selection it causes is not a change.
        last.set(Some(5));
        assert!(!active_changed(&last, 5));
    }

    #[test]
    fn a_panel_dropped_below_the_last_row_is_appended_or_takes_the_last_slot() {
        // From another group: appended (no slot), in the background.
        assert_eq!(slot_below_last_row(7, 1, 4), None);
        // From this group: moved to the last slot.
        assert_eq!(slot_below_last_row(1, 1, 4), Some(3));
        assert_eq!(slot_below_last_row(1, 1, 1), Some(0));
    }

    /// The drop line is where the tab lands under the insert rule `drop_panel` applies (take the
    /// tab out, put it in the hovered row's slot), for every pair of places in a small group.
    #[test]
    fn the_drop_line_is_where_the_tab_lands() {
        for len in 1..=5usize {
            for target in 0..len {
                // From another group: it lands before the hovered row.
                let mut order: Vec<usize> = (0..len).collect();
                order.insert(target, 99);
                let landed = order.iter().position(|tab| *tab == 99).unwrap();
                assert_eq!(drop_edge(None, target), DropEdge::Above);
                assert_eq!(order[landed + 1], target);

                for from in 0..len {
                    let mut order: Vec<usize> = (0..len).collect();
                    let moved = order.remove(from);
                    order.insert(target, moved);
                    let landed = order.iter().position(|tab| *tab == from).unwrap();
                    match drop_edge(Some(from), target) {
                        DropEdge::Nowhere => assert_eq!(from, target),
                        DropEdge::Below => {
                            assert_eq!(order[landed - 1], target, "{from}->{target}")
                        }
                        DropEdge::Above => {
                            assert_eq!(order[landed + 1], target, "{from}->{target}")
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_zoom_button_follows_the_kit_rule() {
        // Zoom Out whenever the group is zoomed, whatever the panel's control says.
        assert_eq!(zoom_button(true, true), Some(ZoomButton::Out));
        assert_eq!(zoom_button(true, false), Some(ZoomButton::Out));
        assert_eq!(zoom_button(false, true), Some(ZoomButton::In));
        assert_eq!(zoom_button(false, false), None);
    }
}
