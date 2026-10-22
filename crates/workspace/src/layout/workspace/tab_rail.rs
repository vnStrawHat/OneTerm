//! The vertical tab rail (`IN-0055`, `US-0196`): a tab group's tabs drawn as a list down its
//! left side instead of the kit's strip above its content.
//!
//! GPUI Kit lays every tab group out as a column (tab bar, then content) and its `TabBar` has
//! no vertical mode, so the rail is OneTerm's own element: positioned absolutely in the group
//! frame, with the content frame pushed right by a margin of the rail's width
//! (`dock_skin.rs`). Each row wraps the panel's own `title()` element (drawn in the vertical
//! row layout), so every per-tab behaviour comes from the one row implementation the strip
//! also uses. Selection and drops go through the callbacks `TabGroupContext` hands a
//! skin; nothing here touches a panel or a session.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, App, AppContext as _, Bounds, ClickEvent, Context, CursorStyle, Div,
    InteractiveElement as _, IntoElement, MouseButton, MouseMoveEvent, MouseUpEvent,
    ParentElement as _, Pixels, Render, ScrollHandle, SharedString, Size, Stateful,
    StatefulInteractiveElement as _, Styled as _, Window, accesskit, canvas, div,
    prelude::FluentBuilder as _, px, size,
};
use gpui_base::{Tab, Tabs};
use gpui_component::{
    ActiveTheme as _, ElementExt as _, Icon, Sizable as _,
    dock::{BasePanelView, DragPanel, PanelHandle, PanelId, TabGroupContext},
    h_flex,
};
use oneterm_settings::{UiConfig, VerticalTabs};
use oneterm_state::{TabRowLayout, panel_names};
use oneterm_theme::AppIcon;

/// The narrowest and widest a rail may be, and the share of its own tab group's width it may
/// take: the right dock's share (`RIGHT_DOCK_MAX_SHARE`), so a split centre or an open dock
/// narrows each rail instead of starving the terminals (`IN-0055` Q3).
const MIN_WIDTH: f32 = *VerticalTabs::WIDTH_RANGE.start() as f32;
const MAX_WIDTH: f32 = *VerticalTabs::WIDTH_RANGE.end() as f32;
const GROUP_SHARE: f32 = 0.35;

/// The width the rail is drawn at: `width` (the stored preference, or the width being dragged)
/// kept within 140 px at least and `max(140, min(400, 35 % of the group))` at most. `group` is
/// the painted width of the rail's own tab group, rail included, so the rail's width does not
/// feed back into its own bound; zero means the group has not been painted yet, and only the
/// absolute bounds apply.
pub(super) fn clamp_tab_rail_width(width: f32, group: f32) -> f32 {
    if group <= 0. {
        return width.clamp(MIN_WIDTH, MAX_WIDTH);
    }
    let ceiling = (group * GROUP_SHARE).clamp(MIN_WIDTH, MAX_WIDTH);
    width.clamp(MIN_WIDTH, ceiling)
}

/// The width the rail is drawn at: the collapsed strip whatever the preference or the group is,
/// otherwise [`clamp_tab_rail_width`] of the preference (or the width being dragged).
pub(super) fn rail_width(collapsed: bool, requested: f32, group: f32) -> f32 {
    if collapsed {
        VerticalTabs::COLLAPSED_WIDTH as f32
    } else {
        clamp_tab_rail_width(requested, group)
    }
}

/// Names in the debug-bounds map, so a test can ask a really drawn frame where things are. A
/// no-op outside test builds.
pub(super) const RAIL_SELECTOR: &str = "oneterm-tab-rail";
pub(super) const LIST_SELECTOR: &str = "oneterm-tab-rail-list";
pub(super) const ROW_SELECTOR: &str = "oneterm-tab-rail-row";
pub(super) const HEADER_SELECTOR: &str = "oneterm-tab-rail-header";
pub(super) const GRIP_SELECTOR: &str = "oneterm-tab-rail-grip";
pub(super) const GRIP_GLYPH_SELECTOR: &str = "oneterm-tab-rail-grip-glyph";
pub(super) const HEADER_LABEL_SELECTOR: &str = "oneterm-tab-rail-header-label";
/// The selected row's accent pill (the row index follows).
pub(super) const PILL_SELECTOR: &str = "oneterm-tab-rail-pill";
/// The hover fill behind an unselected row (the row index follows).
pub(super) const HOVER_SELECTOR: &str = "oneterm-tab-rail-hover";
pub(super) const EMPTY_SELECTOR: &str = "oneterm-tab-rail-empty";
pub(super) const HANDLE_SELECTOR: &str = "oneterm-tab-rail-handle";
/// The drag handle on the rail's right edge.
const HANDLE_WIDTH: Pixels = px(4.);
/// The cursor over the handle, and over the whole window while it is dragged.
const RESIZE_CURSOR: CursorStyle = CursorStyle::ResizeColumn;

/// Height of the header (the label and the `+`): the strip's height, expanded and collapsed.
const HEADER_HEIGHT: Pixels = px(32.);
/// The header's label, which sits where the rows' titles do.
const HEADER_LABEL: &str = "Terminals";
/// Two lines (`TerminalPanel::title`'s vertical row): the title, then the directory and status;
/// one centred line when the tab has no directory.
pub(super) const ROW_HEIGHT: Pixels = px(TabRowLayout::VERTICAL_ROW_HEIGHT);
/// A cell of the collapsed rail: a square as wide as the strip (the shell icon needs no second
/// line).
pub(super) const COLLAPSED_ROW_HEIGHT: Pixels = px(VerticalTabs::COLLAPSED_WIDTH as f32);
/// The reorder grip: a strip on the row's left edge that is drawn over the row and takes no
/// room from it, so the title never starts under it.
const GRIP_WIDTH: Pixels = px(8.);
/// How far the glyph's box (`Icon::xsmall`, 12 px) starts left of the grip's edge: the six dots
/// of the glyph occupy the middle of their box, so this puts them in the grip's 0..6 px.
const GRIP_GLYPH_SHIFT: f32 = 2.;
const ACCENT_BAR_WIDTH: Pixels = px(TabRowLayout::ACCENT_BAR_WIDTH);
const ACCENT_BAR_INSET: Pixels = px(TabRowLayout::ACCENT_BAR_INSET);
/// How much of the selected fill a hovered row gets (as alpha over the rail): half of it lies
/// between the rail and the selected row whatever the theme, so hover never outranks selected.
const HOVER_FILL_STRENGTH: f32 = 0.5;
/// What the drag preview occupies, reported to base so a drop placeholder knows where to fly
/// in from. The chip is content-sized between a floor and a ceiling; this is a typical size.
const DRAG_PREVIEW_SIZE: Size<Pixels> = size(px(140.), px(30.));

/// The fills of a rail row: `(selected, hover)`. Selected is the list hover colour, which is
/// what OneTerm draws a selected list item in everywhere (the session tree, the SFTP rows;
/// the owner's choice; `Theme::list_active` is the kit's token for the same thing, clamped to
/// 0.2 alpha in the kit's own `tokens`, which the override did not reach before `BUG-0088`, so
/// the rail reads the colour it means directly).
/// `tab.active.background` is the terminal's own colour in the built-in themes, so a row filled
/// with it would read as the page. Hover is the same colour at half strength, so
/// selected > hover > idle in every theme.
fn row_fills(theme: &gpui_component::Theme) -> (gpui::Hsla, gpui::Hsla) {
    (
        theme.list_hover,
        theme.list_hover.opacity(HOVER_FILL_STRENGTH),
    )
}

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

/// End a drag of the rail's edge: store the width it reached, once. A press that never moved
/// stores nothing.
fn finish_resize(resize: &Cell<Resize>, cx: &mut App) {
    if let Resize::Dragging(width) = resize.replace(Resize::Idle) {
        UiConfig::set_vertical_tabs_width(width.round().max(0.) as u32, cx);
    }
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

/// Where one row sits: the panel's index in the group, its place among the visible rows, how
/// many rows there are, and whether it is the displayed tab.
#[derive(Clone)]
struct RowPlace {
    ix: usize,
    position: usize,
    rows: usize,
    selected: bool,
    /// The rail is the 44 px strip: the cell shows the shell icon, not the two lines.
    collapsed: bool,
    /// Every panel of the group in tab order, to place a dragged tab among the rows.
    order: Rc<Vec<PanelId>>,
}

/// What the group frame decided this frame, read by the two later hooks of the same render
/// (base calls `frame`, `render_tab_bar`, `content_frame` in that order), so the rail rule and
/// the width are worked out once per group per frame.
#[derive(Clone, Copy)]
struct RailFrame {
    draws: bool,
    collapsed: bool,
    width: Pixels,
}

/// Where a drag of the rail's edge is.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Resize {
    Idle,
    /// The button went down on the handle and the pointer has not moved yet: a click, which
    /// stores nothing.
    Pressed,
    /// The requested width, from the pointer; applied clamped, stored once on release.
    Dragging(f32),
}

/// One tab group's rail state: the list's scroll position and the displayed tab the last frame
/// drew. Built per group (the dock area asks for one renderer per container), like the kit
/// strip's own.
pub(super) struct TabRail {
    scroll_handle: ScrollHandle,
    last_active_ix: Rc<Cell<Option<usize>>>,
    /// Whether the last frame drew the collapsed strip: a flip changes every row's height, so
    /// the pixel scroll offset no longer points at the displayed tab.
    last_collapsed: Cell<bool>,
    frame: Cell<RailFrame>,
    /// The group's painted bounds last frame, rail included: the bound on the rail's width and
    /// the origin a drag measures from.
    group: Rc<Cell<Bounds<Pixels>>>,
    resize: Rc<Cell<Resize>>,
}

impl TabRail {
    pub(super) fn new() -> Self {
        Self {
            scroll_handle: ScrollHandle::default(),
            last_active_ix: Rc::new(Cell::new(None)),
            last_collapsed: Cell::new(false),
            frame: Cell::new(RailFrame {
                draws: false,
                collapsed: false,
                width: px(VerticalTabs::DEFAULT_WIDTH as f32),
            }),
            group: Rc::new(Cell::new(Bounds::default())),
            resize: Rc::new(Cell::new(Resize::Idle)),
        }
    }

    /// Start a frame of this group: whether it draws the rail (the caller worked that out),
    /// whether it is collapsed, and the width it draws at: the strip's, or the stored preference
    /// or the width being dragged kept within the group's own bound.
    pub(super) fn begin_frame(&self, draws: bool, cx: &App) {
        let stored = UiConfig::read_vertical_tabs(cx);
        let requested = match self.resize.get() {
            Resize::Dragging(width) => width,
            Resize::Idle | Resize::Pressed => stored.width as f32,
        };
        let group = f32::from(self.group.get().size.width);
        self.frame.set(RailFrame {
            draws,
            collapsed: stored.collapsed,
            width: px(rail_width(stored.collapsed, requested, group)),
        });
    }

    pub(super) fn draws(&self) -> bool {
        self.frame.get().draws
    }

    /// The width of this frame's rail, and so the content frame's left margin.
    pub(super) fn width(&self) -> Pixels {
        self.frame.get().width
    }

    /// Measure the group's frame: its painted width bounds the rail, so a window resize, the
    /// right dock opening or a centre split asks for one more frame to re-apply the clamp. The
    /// rail's own width is inside the group's, so this cannot feed back.
    pub(super) fn track_group(&self, frame: Stateful<Div>) -> Stateful<Div> {
        let group = self.group.clone();
        frame.on_prepaint(move |bounds, window, _| {
            if group.replace(bounds).size.width != bounds.size.width {
                window.request_animation_frame();
            }
        })
    }

    /// The 4 px handle on the rail's right edge. Pressing it arms a drag; the rest is the
    /// tracker, which sees the pointer wherever it goes.
    fn handle(&self, cx: &App) -> Stateful<Div> {
        let resize = self.resize.clone();
        let ring = cx.theme().ring;
        div()
            .id("tab-rail-resize")
            .debug_selector(|| HANDLE_SELECTOR.to_string())
            .occlude()
            .absolute()
            .top_0()
            .bottom_0()
            // Over the rail's 1 px border, which is the rail's outermost pixel.
            .right(-px(1.))
            .w(HANDLE_WIDTH)
            .cursor(RESIZE_CURSOR)
            .hover(move |style| style.bg(ring))
            .when(resize.get() != Resize::Idle, |this| this.bg(ring))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                resize.set(Resize::Pressed);
                window.refresh();
            })
    }

    /// Follows a drag of the handle anywhere in the window (a window-level listener can only be
    /// registered while painting, as the dock's own resize does): the width tracks the pointer
    /// in memory, and the release stores the preference once.
    fn tracker(&self) -> impl IntoElement {
        let resize = self.resize.clone();
        let group = self.group.clone();
        canvas(
            |_, _, _| (),
            move |_, (), window, _| {
                // Held for the whole drag: the pointer leaves the 4 px band at once.
                window.set_window_cursor_style(RESIZE_CURSOR);
                window.on_mouse_event({
                    let resize = resize.clone();
                    let group = group.clone();
                    move |event: &MouseMoveEvent, phase, window, cx| {
                        if !phase.bubble() || resize.get() == Resize::Idle {
                            return;
                        }
                        // The button came up outside the window.
                        if event.pressed_button != Some(MouseButton::Left) {
                            finish_resize(&resize, cx);
                        } else {
                            let from_left = event.position.x - group.get().origin.x;
                            resize.set(Resize::Dragging(f32::from(from_left)));
                        }
                        window.refresh();
                    }
                });
                window.on_mouse_event({
                    let resize = resize.clone();
                    move |_: &MouseUpEvent, phase, window, cx| {
                        if phase.bubble() && resize.get() != Resize::Idle {
                            finish_resize(&resize, cx);
                            window.refresh();
                        }
                    }
                });
            },
        )
        .absolute()
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
        let collapsed = self.frame.get().collapsed;
        // Both are recorded every frame, so evaluate both before testing either.
        let tab_changed = active_changed(&self.last_active_ix, active_ix);
        let layout_changed = self.last_collapsed.replace(collapsed) != collapsed;
        if (tab_changed || layout_changed)
            && let Some(position) = visible.iter().position(|ix| *ix == active_ix)
        {
            self.scroll_handle.scroll_to_item(position);
        }

        let droppable = group.is_droppable();
        let tabs = group.panels().len();
        let rows = visible.len();
        let tab_bar = cx.theme().tokens.tab_bar;
        let border = cx.theme().border;

        let order: Rc<Vec<PanelId>> = Rc::new(
            group
                .panels()
                .iter()
                .map(|panel| panel.panel_id(cx))
                .collect(),
        );

        let header = self.header(group, collapsed, window, cx);
        let list = Tabs::new("tab-rail-list")
            .debug_selector(|| LIST_SELECTOR.to_string())
            .aria_orientation(accesskit::Orientation::Vertical)
            .size_full()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .children(visible.into_iter().enumerate().map(|(position, ix)| {
                let place = RowPlace {
                    ix,
                    position,
                    rows,
                    selected: displayed_ix == Some(ix),
                    collapsed,
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
            .w(self.width())
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
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_hidden()
                    // No scrollbar, expanded or collapsed (owner request, 2026-10-21; the kit's
                    // track also pages the list on any press inside it and passes nothing on,
                    // `US-0198` D3, D4). The wheel and the auto-reveal of the displayed tab
                    // scroll it.
                    .child(list),
            )
            // No resizing the strip: it has one width.
            .when(!collapsed, |this| this.child(self.handle(cx)))
            .when(self.resize.get() != Resize::Idle, |this| {
                this.child(self.tracker())
            })
            .into_any_element()
    }

    /// The header: the label "Terminals" on the left and `+` (the active panel's own title
    /// suffix: OneTerm's New Terminal menu) on the right, with no rule under it; the collapsed
    /// strip has the `+` alone, centred. No zoom button, no `...` menu (its tab list is the rail
    /// itself), no dock toggle (the status bar and `Ctrl+Shift+B` own it) and no collapse button
    /// (the status bar's, just left of the clock). The same 32 px line in both states, so the
    /// rows below start at the same place.
    fn header(
        &self,
        group: &TabGroupContext,
        collapsed: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let layout = if collapsed {
            TabRowLayout::Collapsed
        } else {
            TabRowLayout::Vertical
        };
        // The `+` is built for the rail's layout (a larger target than the strip's).
        let suffix = {
            let mut scope = layout.scope(cx);
            group
                .active_panel()
                .and_then(PanelHandle::of)
                .and_then(|handle| handle.title_suffix(window, &mut scope))
        };
        h_flex()
            .debug_selector(|| HEADER_SELECTOR.to_string())
            .h(HEADER_HEIGHT)
            .flex_shrink_0()
            .items_center()
            .when(collapsed, |this| this.justify_center().px_2())
            .when(!collapsed, |this| {
                this.justify_between()
                    .pl(px(TabRowLayout::VERTICAL_ROW_PADDING_LEFT))
                    .pr_2()
                    .child(
                        div()
                            .debug_selector(|| HEADER_LABEL_SELECTOR.to_string())
                            .text_xs()
                            .text_color(muted)
                            .whitespace_nowrap()
                            .child(HEADER_LABEL),
                    )
            })
            .children(suffix)
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
            collapsed,
            order,
        } = place;
        let panel = &group.panels()[ix];
        let handle = PanelHandle::of(panel);
        // The kit strip prefers a panel's short name over its title element; so does the rail.
        // The vertical layout holds only while the title element is built.
        let layout = if collapsed {
            TabRowLayout::Collapsed
        } else {
            TabRowLayout::Vertical
        };
        let title = {
            let mut scope = layout.scope(cx);
            match handle {
                // The cell shows the shell icon on the status colour, which only the panel's own
                // row knows how to draw.
                Some(handle) if collapsed => handle.title(window, &mut scope),
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
        let (foreground, active_foreground) = (theme.tab_foreground, theme.tab_active_foreground);
        let (selected_fill, hover_fill) = row_fills(theme);
        let grip_color = theme.muted_foreground;
        // Not `table_active_border`: OneTerm blanks that for the data tables after every theme
        // change, which made the pill vanish.
        let accent = oneterm_theme::selection_accent(cx);
        let hover_group = SharedString::from(format!("tab-rail-row-{ix}"));
        let reorder = {
            let panel = panel.clone();
            move |drag: &DragPanel, offset, _: &mut Window, cx: &mut App| {
                cx.stop_propagation();
                drag.set_drag_offset(offset);
                drag.set_preview_size(DRAG_PREVIEW_SIZE);
                cx.new(|_| RailDragPreview {
                    panel: panel.clone(),
                })
            }
        };

        // An expanded row's reorder grip: a strip over the row's left edge, drawn after the
        // title so it takes the press there, reserving no room (a collapsed cell has none: the
        // whole cell outside its icon is the handle).
        let grip = (!collapsed).then(|| {
            div()
                .id(("tab-rail-grip", ix))
                .debug_selector(move || format!("{GRIP_SELECTOR}-{ix}"))
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(GRIP_WIDTH)
                .flex()
                .items_center()
                .when_some(drag.clone(), |this, drag| {
                    this.cursor_grab()
                        .child(
                            div()
                                .id(("tab-rail-grip-glyph", ix))
                                .debug_selector(move || format!("{GRIP_GLYPH_SELECTOR}-{ix}"))
                                .flex_shrink_0()
                                .ml(-px(GRIP_GLYPH_SHIFT))
                                .opacity(0.)
                                .group_hover(hover_group.clone(), |style| style.opacity(1.))
                                .child(
                                    Icon::new(AppIcon::GripVertical)
                                        .xsmall()
                                        .text_color(grip_color),
                                ),
                        )
                        .on_drag(drag, reorder.clone())
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
                styles.selected(|style| style.text_color(active_foreground).bg(selected_fill))
            })
            .relative()
            .w_full()
            .h(if collapsed {
                COLLAPSED_ROW_HEIGHT
            } else {
                ROW_HEIGHT
            })
            .flex_shrink_0()
            .justify_start()
            .text_sm()
            .text_color(foreground)
            .group(hover_group.clone())
            .hover(move |style| style.text_color(active_foreground))
            // A collapsed cell has no grip: the cell is what is dragged. Its icon, the cell's
            // content, starts the tab drag first and wins the press where it is.
            .when_some(drag.filter(|_| collapsed), |this, drag| {
                this.cursor_grab().on_drag(drag, reorder)
            })
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
            // The accent: a rounded pill inset from the row's top and bottom, not a bar the
            // row's height.
            .when(selected, |this| {
                this.child(
                    div()
                        .debug_selector(move || format!("{PILL_SELECTOR}-{ix}"))
                        .absolute()
                        .left_0()
                        .top(ACCENT_BAR_INSET)
                        .bottom(ACCENT_BAR_INSET)
                        .w(ACCENT_BAR_WIDTH)
                        .rounded(ACCENT_BAR_WIDTH / 2.)
                        .bg(accent),
                )
            })
            // The whole row's hover fill, behind the content. The row's own `hover` above sets
            // only the text colour (nothing ever gave it a background), and a second `hover` on
            // one element is refused by gpui, so the fill is a child that follows the row's hover
            // group. The selected row keeps its own, stronger fill and takes none.
            .when(!selected, |this| {
                this.child(
                    div()
                        .debug_selector(move || format!("{HOVER_SELECTOR}-{ix}"))
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .group_hover(hover_group, move |style| style.bg(hover_fill)),
                )
            })
            .child(title)
            .children(grip)
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

    /// The worked examples of `high-level-design.md` § Width, row for row.
    #[test]
    fn the_rail_width_is_clamped_to_its_group_as_the_design_works_out() {
        // (group width, requested, applied)
        let cases = [
            (1920., 220., 220.),
            (1920., 520., 400.),
            (800., 300., 280.),
            (600., 220., 210.),
            (380., 220., 140.),
            // Not painted yet: only the absolute bounds apply.
            (0., 100., 140.),
            (0., 520., 400.),
            (0., 260., 260.),
        ];
        for (group, requested, applied) in cases {
            let got = clamp_tab_rail_width(requested, group);
            assert!(
                (got - applied).abs() < 0.01,
                "group {group}, requested {requested}: {got} is not {applied}"
            );
        }
    }

    /// The window table of the same section: the right dock at its own ceiling (35 % of the
    /// window, 240 px floor) leaves the centre groups below.
    #[test]
    fn the_rail_width_in_a_window_with_the_right_dock_open() {
        // (centre groups, each one's width, rail applied to a 220 px preference)
        let cases = [
            (1, 650., 220.),
            (2, 325., 140.),
            (1, 585., 204.75),
            (1, 360., 140.),
        ];
        for (groups, group, rail) in cases {
            let got = clamp_tab_rail_width(220., group);
            assert!(
                (got - rail).abs() < 0.01,
                "{groups} group(s) of {group}: {got} is not {rail}"
            );
        }
        // A terminal keeps at least 65 % of its group once the group is 400 px or wider.
        for group in (400..2000).step_by(7) {
            let group = group as f32;
            let rail = clamp_tab_rail_width(400., group);
            assert!(group - rail >= group * 0.65 - 0.001, "group {group}");
        }
    }

    /// WCAG contrast of two opaque colours, to put the three fills of a rail row in order.
    fn contrast(a: gpui::Hsla, b: gpui::Hsla) -> f32 {
        fn luminance(color: gpui::Hsla) -> f32 {
            let rgba = gpui::Rgba::from(color);
            let channel = |c: f32| {
                if c <= 0.03928 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(rgba.r) + 0.7152 * channel(rgba.g) + 0.0722 * channel(rgba.b)
        }
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// Owner rework 7: selected > hover > idle in the dark and the light theme. The selected
    /// fill must stand out from the rail and from the terminal (`tab.active.background` is the
    /// terminal's own colour, which is why the rail does not use it); a hovered row, half of
    /// that fill over the rail, sits between the rail and the selected row, never beyond it.
    #[gpui::test]
    fn selected_outranks_hover_outranks_idle_in_both_zed_themes(cx: &mut gpui::TestAppContext) {
        use gpui_component::{ActiveTheme as _, Theme, ThemeRegistry};

        cx.update(|cx| {
            gpui_component::init(cx);
            let config = cx.new(|_| UiConfig {
                persist_blocked: true,
                ..UiConfig::default()
            });
            cx.set_global(oneterm_settings::ui_config::UiConfigGlobal(config));
            oneterm_theme::init(cx);
            for name in ["Zed One Dark", "Zed One Light"] {
                let theme_config = ThemeRegistry::global(cx)
                    .themes()
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| panic!("{name} is not registered"));
                Theme::global_mut(cx).apply_config(&theme_config);

                let theme = cx.theme();
                let (selected, hover) = row_fills(theme);
                let (rail, terminal) = (*theme.tokens.tab_bar, theme.background);
                assert_eq!(selected, theme.list_hover, "{name}");
                assert_eq!(hover, theme.list_hover.opacity(0.5), "{name}");
                let hovered = rail.blend(hover);
                let (selected_vs_rail, hover_vs_rail) =
                    (contrast(selected, rail), contrast(hovered, rail));
                assert!(
                    selected_vs_rail > hover_vs_rail && hover_vs_rail > 1.0,
                    "{name}: selected {selected_vs_rail:.3} > hover {hover_vs_rail:.3} > idle 1.0"
                );
                assert!(
                    contrast(selected, terminal) > 1.05,
                    "{name}: the selected row reads as a row, not as the terminal"
                );
            }
        });
    }

    /// Collapsed is the 44 px strip for any preference and any group; expanded is the clamp.
    #[test]
    fn the_collapsed_rail_is_44_px_whatever_the_preference_and_the_group() {
        for requested in [0., 140., 220., 400., 9999.] {
            for group in [0., 100., 380., 1920.] {
                assert_eq!(rail_width(true, requested, group), 44.);
                assert_eq!(
                    rail_width(false, requested, group),
                    clamp_tab_rail_width(requested, group)
                );
            }
        }
    }

    #[gpui::test]
    fn the_handle_has_the_resize_cursor(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            let mut handle = TabRail::new().handle(cx);
            assert_eq!(handle.style().mouse_cursor, Some(CursorStyle::ResizeColumn));
        });
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
}
