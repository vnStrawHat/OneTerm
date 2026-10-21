//! The vertical tab rail drawn by the real `OneTermTabGroupSkin` (`IN-0055`, `US-0196`).

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    App, AppContext as _, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, Modifiers, MouseButton, ParentElement as _, Pixels,
    Point, Render, SharedString, StatefulInteractiveElement as _, Styled as _, TestAppContext,
    VisualTestContext, Window, div, point, px,
};
use gpui_component::dock::{
    DockArea, DockLayout, DockPlacement, PaneRef, Panel, PanelControl, PanelEvent, PanelId,
    panel_handle,
};
use gpui_component::h_flex;
use oneterm_settings::{UiConfig, VerticalTabs, ui_config::UiConfigGlobal};
use oneterm_state::{TabRowLayout, panel_names};

use super::tab_rail::{
    COLLAPSED_ROW_HEIGHT, EMPTY_SELECTOR, GRIP_GLYPH_SELECTOR, GRIP_SELECTOR, HANDLE_SELECTOR,
    LIST_SELECTOR, RAIL_SELECTOR, ROW_HEIGHT, ROW_SELECTOR,
};
use super::{MAIN_DOCK_VERSION, dock_skin};

/// The default width, which a roomy test window leaves unclamped.
const RAIL_WIDTH: Pixels = px(VerticalTabs::DEFAULT_WIDTH as f32);

/// What a probe panel reports to the test.
#[derive(Clone, Default)]
struct Report {
    /// The box its tab group gave the panel.
    bounds: Rc<Cell<Bounds<Pixels>>>,
    /// How many times the panel was taken out of its group (a rebuild would do this).
    removed: Rc<Cell<usize>>,
    /// How many times its title row started its own drag.
    title_drags: Rc<Cell<usize>>,
    /// What the row layout was while its title was built.
    title_layout: Rc<Cell<Option<TabRowLayout>>>,
}

/// A panel that records the box its tab group gave it, drawn under any registered name.
struct Probe {
    name: &'static str,
    label: SharedString,
    /// Draw a title element of its own (a draggable row ending in a status icon) instead of
    /// offering a short name, the way the terminal panel's tab row is built.
    title_row: bool,
    report: Report,
    focus_handle: FocusHandle,
}

/// The payload of the probe title row's own drag, standing in for `DragTerminalTab`.
#[derive(Clone)]
struct ProbeDrag;

impl Render for ProbeDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_4()
    }
}

impl Probe {
    fn view(
        name: &'static str,
        label: &str,
        title_row: bool,
        report: &Report,
        cx: &mut App,
    ) -> Entity<Self> {
        let label = SharedString::from(label.to_owned());
        let report = report.clone();
        cx.new(|cx| Self {
            name,
            label,
            title_row,
            report,
            focus_handle: cx.focus_handle(),
        })
    }
}

impl gpui_base::dock::Panel for Probe {
    fn panel_name(&self) -> &'static str {
        self.name
    }

    fn on_removed(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.report.removed.set(self.report.removed.get() + 1);
    }
}

impl Panel for Probe {
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        (!self.title_row).then(|| self.label.clone())
    }

    fn title(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.report
            .title_layout
            .set(Some(TabRowLayout::current(cx)));
        let started = self.report.title_drags.clone();
        let label = self.label.clone();
        let selector = format!("probe-row-{label}");
        let status = format!("probe-status-{label}");
        h_flex()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .flex_1()
            .h_full()
            .items_center()
            .on_drag(ProbeDrag, move |drag, _, _, cx| {
                started.set(started.get() + 1);
                cx.new(|_| drag.clone())
            })
            .child(div().flex_1().child(label))
            // Like the real row's trailing status icon: plain row content, so a drag that starts
            // on it is the row's own.
            .child(
                div()
                    .id("probe-status")
                    .debug_selector(move || status.clone())
                    .size_4(),
            )
    }

    // Like the terminal panel: the toolbar offers zoom, and the content frame adds no top
    // padding for several tabs.
    fn zoom_control(&self, _: &App) -> Option<PanelControl> {
        Some(PanelControl::Both)
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

impl EventEmitter<PanelEvent> for Probe {}

impl Focusable for Probe {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::ElementExt as _;
        let bounds = self.report.bounds.clone();
        div()
            .size_full()
            .on_prepaint(move |measured, _, _| bounds.set(measured))
    }
}

/// The window's root: just the dock area, drawn the way the workspace draws it.
struct Host {
    area: Entity<DockArea>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.area.clone())
    }
}

struct RailWindow {
    area: Entity<DockArea>,
    config: Entity<UiConfig>,
}

/// A window with the OneTerm dock skin and a `UiConfig` (never written: `persist_blocked`).
fn rail_window(cx: &mut TestAppContext, vertical: bool) -> (RailWindow, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    let config = cx.update(|cx| {
        let config = cx.new(|_| UiConfig {
            persist_blocked: true,
            vertical_tabs: VerticalTabs {
                enabled: vertical,
                ..VerticalTabs::default()
            },
            ..UiConfig::default()
        });
        cx.set_global(UiConfigGlobal(config.clone()));
        config
    });
    let (host, cx) = cx.add_window_view(|window, cx| {
        let (area, _skin) = dock_skin::dock_area("rail-test", Some(MAIN_DOCK_VERSION), window, cx);
        Host { area }
    });
    let area = host.read_with(cx, |host, _| host.area.clone());
    (RailWindow { area, config }, cx)
}

/// A tab group's panels as (registered name, label).
type Tabs<'a> = &'a [(&'static str, &'a str)];

fn tab_group(tabs: Tabs, title_row: bool, report: &Report, cx: &mut App) -> DockLayout {
    let mut group = DockLayout::tabs();
    for (name, label) in tabs {
        let probe = Probe::view(name, label, title_row, report, cx);
        group = group.panel_view(panel_handle(probe), cx);
    }
    group
}

/// Put `tabs` in one centre tab group, in order.
fn set_centre(
    area: &Entity<DockArea>,
    tabs: Tabs,
    title_row: bool,
    report: &Report,
    cx: &mut VisualTestContext,
) {
    cx.update(|window, cx| {
        let layout = DockLayout::v_split().child(tab_group(tabs, title_row, report, cx), None);
        area.update(cx, |area, cx| area.set_center(layout, window, cx));
    });
    settle(cx);
}

/// Put `tabs` in the right dock and open it.
fn set_right_dock(
    area: &Entity<DockArea>,
    tabs: Tabs,
    report: &Report,
    cx: &mut VisualTestContext,
) {
    cx.update(|window, cx| {
        let layout = tab_group(tabs, false, report, cx);
        area.update(cx, |area, cx| {
            area.set_dock(DockPlacement::Right, layout, window, cx);
            area.set_dock_size(DockPlacement::Right, px(300.), window, cx);
            if !area.is_dock_open(DockPlacement::Right) {
                area.toggle_dock(DockPlacement::Right, window, cx);
            }
        });
    });
    settle(cx);
}

/// Draw twice: scrolling reads the previous frame's child bounds.
fn settle(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

/// Flip the switch the way `UiConfig::set_vertical_tabs_enabled` does, without the write.
fn set_vertical(config: &Entity<UiConfig>, enabled: bool, cx: &mut VisualTestContext) {
    config.update(cx, |config, cx| {
        config.vertical_tabs.enabled = enabled;
        cx.notify();
    });
    cx.update(|_, cx| cx.refresh_windows());
    settle(cx);
}

/// `debug_bounds` takes a `'static` selector.
fn bounds_of(selector: impl Into<String>, cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    cx.debug_bounds(selector.into().leak())
}

fn row(ix: usize, cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    bounds_of(format!("{ROW_SELECTOR}-{ix}"), cx)
}

/// The panel ids of the centre tab group in tab order, and its active index.
fn centre_tabs(area: &Entity<DockArea>, cx: &mut VisualTestContext) -> (Vec<PanelId>, usize) {
    area.read_with(cx, |area, _| {
        let tree = area.layout(DockPlacement::Center).expect("centre layout");
        let mut found = None;
        tree.root().walk(&mut |node| {
            if let PaneRef::Tabs { panels, active_ix } = node.kind() {
                found.get_or_insert((panels.to_vec(), active_ix));
            }
        });
        found.expect("a tab group in the centre")
    })
}

fn centre_node(
    area: &Entity<DockArea>,
    cx: &mut VisualTestContext,
) -> gpui_component::dock::NodeId {
    area.read_with(cx, |area, _| {
        let tree = area.layout(DockPlacement::Center).expect("centre layout");
        let mut found = None;
        tree.root().walk(&mut |node| {
            if matches!(node.kind(), PaneRef::Tabs { .. }) {
                found.get_or_insert(node.id());
            }
        });
        found.expect("a tab group in the centre")
    })
}

fn terminals(labels: &[&'static str]) -> Vec<(&'static str, &'static str)> {
    labels
        .iter()
        .map(|label| (panel_names::TERMINAL, *label))
        .collect()
}

/// A left press, a first move past the drag threshold, then the move onto `target`; the button
/// stays down.
fn drag_to(from: Point<Pixels>, target: Point<Pixels>, cx: &mut VisualTestContext) {
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        from + point(px(0.), px(6.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(target, MouseButton::Left, Modifiers::none());
}

fn release(at: Point<Pixels>, cx: &mut VisualTestContext) {
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    settle(cx);
}

fn has_active_drag(cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| cx.has_active_drag())
}

/// Q8 (`IN-0055`): a rail placed absolutely in the group frame, with the content frame pushed
/// right by a margin of the rail's width, lays out inside base's forced column, and the
/// terminal is resized rather than overlapped when the switch flips.
#[gpui::test]
fn the_rail_is_beside_the_content_and_the_terminal_is_resized_not_overlapped(
    cx: &mut TestAppContext,
) {
    let (window, cx) = rail_window(cx, false);
    let report = Report::default();
    set_centre(
        &window.area,
        &terminals(&["one", "two"]),
        false,
        &report,
        cx,
    );

    // Horizontal: the kit strip, no rail, the terminal spans the group. Everything below is
    // measured against this, whatever the test window's size.
    assert!(
        bounds_of(RAIL_SELECTOR, cx).is_none(),
        "no rail while the switch is off"
    );
    let strip = report.bounds.get();
    assert!(strip.size.width > RAIL_WIDTH * 2., "{strip:?}");
    assert_eq!(strip.origin.x, px(0.));

    // Vertical: a 220 px rail on the left, the terminal laid out in what is left.
    set_vertical(&window.config, true, cx);
    let rail = bounds_of(RAIL_SELECTOR, cx).expect("the rail is drawn");
    assert_eq!(rail.size.width, RAIL_WIDTH);
    assert_eq!(rail.origin.x, px(0.));
    let terminal = report.bounds.get();
    assert_eq!(
        terminal.origin.x, RAIL_WIDTH,
        "the terminal starts where the rail ends"
    );
    assert_eq!(
        terminal.size.width,
        strip.size.width - RAIL_WIDTH,
        "resized, not overlapped"
    );
    assert!(
        rail.right() <= terminal.left(),
        "the rail {rail:?} and the terminal {terminal:?} must not overlap"
    );
    // The rail is out of the column: it takes no height from the content, and the tab bar's
    // height is gone from the terminal's.
    assert_eq!(rail.size.height, terminal.size.height);

    // And back: the strip returns and the terminal takes the whole group again.
    set_vertical(&window.config, false, cx);
    assert!(bounds_of(RAIL_SELECTOR, cx).is_none());
    assert_eq!(report.bounds.get(), strip);
}

/// The Settings switch's setter is what flips the layout in the running app.
#[gpui::test]
fn the_settings_setter_flips_the_layout_on_the_next_frame(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, false);
    let report = Report::default();
    set_centre(
        &window.area,
        &terminals(&["one", "two"]),
        false,
        &report,
        cx,
    );
    assert!(bounds_of(RAIL_SELECTOR, cx).is_none());

    cx.update(|_, cx| UiConfig::set_vertical_tabs_enabled(true, cx));
    settle(cx);
    assert!(bounds_of(RAIL_SELECTOR, cx).is_some());
    assert!(
        window
            .config
            .read_with(cx, |config, _| config.vertical_tabs.enabled)
    );

    cx.update(|_, cx| UiConfig::set_vertical_tabs_enabled(false, cx));
    settle(cx);
    assert!(bounds_of(RAIL_SELECTOR, cx).is_none());
}

/// Switching layout draws the same panels in a different chrome: nothing is rebuilt and no
/// panel leaves its group (which is what ends a session).
#[gpui::test]
fn the_live_switch_keeps_every_panel_and_removes_none(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, false);
    let report = Report::default();
    set_centre(
        &window.area,
        &terminals(&["a", "b", "c"]),
        false,
        &report,
        cx,
    );
    let before = centre_tabs(&window.area, cx);

    for enabled in [true, false, true, false] {
        set_vertical(&window.config, enabled, cx);
        assert_eq!(centre_tabs(&window.area, cx), before, "vertical: {enabled}");
        assert_eq!(report.removed.get(), 0, "vertical: {enabled}");
    }
}

#[gpui::test]
fn the_rail_has_one_stacked_row_per_tab_and_a_click_selects_it(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    set_centre(
        &window.area,
        &terminals(&["a", "b", "c"]),
        false,
        &report,
        cx,
    );

    let rows: Vec<_> = (0..3)
        .map(|ix| row(ix, cx).expect("a row per tab"))
        .collect();
    assert!(row(3, cx).is_none(), "no row beyond the tabs");
    for pair in rows.windows(2) {
        assert_eq!(pair[0].size.height, px(64.), "twice the strip's 32 px");
        assert_eq!(pair[0].size.height, ROW_HEIGHT);
        assert_eq!(pair[1].top(), pair[0].bottom(), "rows stack");
    }
    let rail = bounds_of(RAIL_SELECTOR, cx).unwrap();
    assert!(
        rows[0].top() > rail.top(),
        "the header sits above the first row"
    );
    // Rows that fit leave no scrollbar gutter.
    let list = bounds_of(LIST_SELECTOR, cx).unwrap();
    assert_eq!(rows[0].right(), list.right());
    // A 1 px divider sits on each row's bottom edge, inside the 64 px: the row's content box
    // (the grip is as tall as it) is one pixel shorter.
    let grip = bounds_of(format!("{GRIP_SELECTOR}-0"), cx).expect("the grip column");
    assert_eq!(grip.size.height, ROW_HEIGHT - px(1.));

    assert_eq!(centre_tabs(&window.area, cx).1, 0);
    cx.simulate_click(rows[2].center(), Modifiers::none());
    settle(cx);
    assert_eq!(
        centre_tabs(&window.area, cx).1,
        2,
        "a click on a row selects its tab"
    );
}

/// The reorder grip's glyph is centred on the title line (line 1 of the two-line row), not on
/// the whole 64 px row, and the header keeps the strip's 32 px.
#[gpui::test]
fn the_grip_glyph_sits_on_the_title_line_and_the_header_keeps_its_height(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    set_centre(&window.area, &terminals(&["a", "b"]), false, &report, cx);

    let row = row(0, cx).unwrap();
    let glyph = bounds_of(format!("{GRIP_GLYPH_SELECTOR}-0"), cx).expect("the grip glyph");
    assert_eq!(
        glyph.center().y - row.top(),
        px(TabRowLayout::vertical_first_line_centre())
    );
    let rail = bounds_of(RAIL_SELECTOR, cx).unwrap();
    assert_eq!(
        row.top() - rail.top(),
        px(32.),
        "the header is the strip's height"
    );
}

/// The rail rule: only a centre group of terminal panels.
#[gpui::test]
fn other_groups_keep_what_they_draw_today(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();

    set_centre(
        &window.area,
        &[(panel_names::SSH_CLIENT, "ssh")],
        false,
        &report,
        cx,
    );
    assert!(
        bounds_of(RAIL_SELECTOR, cx).is_none(),
        "the SSH Client group draws no rail"
    );

    set_centre(
        &window.area,
        &[
            (panel_names::TERMINAL, "term"),
            (panel_names::SFTP, "files"),
        ],
        false,
        &report,
        cx,
    );
    assert!(
        bounds_of(RAIL_SELECTOR, cx).is_none(),
        "a group holding another kind of panel keeps the strip"
    );

    // A terminal tab dropped into the right dock: the centre keeps its rail (one row), the
    // dock's group of two terminal tabs keeps the strip (no second row 1 anywhere).
    set_centre(&window.area, &terminals(&["centre"]), false, &report, cx);
    set_right_dock(
        &window.area,
        &terminals(&["docked one", "docked two"]),
        &report,
        cx,
    );
    let rail = bounds_of(RAIL_SELECTOR, cx).expect("the centre group draws its rail");
    assert_eq!(rail.origin.x, px(0.), "it is the centre's rail");
    assert!(row(0, cx).is_some());
    assert!(
        row(1, cx).is_none(),
        "the right dock's terminal group must not draw rows"
    );
}

/// The row asks for its vertical layout around the rail's own `title()` call only.
#[gpui::test]
fn the_rail_draws_its_rows_in_the_vertical_layout_and_nothing_else_does(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, false);
    let report = Report::default();
    set_centre(&window.area, &terminals(&["a", "b"]), true, &report, cx);
    // The kit strip builds the same title element in the default (horizontal) layout.
    assert_eq!(report.title_layout.get(), Some(TabRowLayout::Horizontal));

    set_vertical(&window.config, true, cx);
    assert_eq!(report.title_layout.get(), Some(TabRowLayout::Vertical));
    cx.update(|_, cx| {
        assert_eq!(
            TabRowLayout::current(cx),
            TabRowLayout::Horizontal,
            "the rail puts the layout back"
        );
    });
}

/// With 40 tabs the rows scroll under the header, and a tab that becomes active from
/// elsewhere (a new tab from `+`) is scrolled into view.
#[gpui::test]
fn forty_tabs_scroll_and_the_active_one_is_brought_into_view(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    let labels: Vec<&'static str> = (0..40).map(|ix| &*format!("tab {ix}").leak()).collect();
    set_centre(&window.area, &terminals(&labels), true, &report, cx);

    let list = bounds_of(LIST_SELECTOR, cx).expect("the list is drawn");
    let first = row(0, cx).unwrap();
    let last = row(39, cx).unwrap();
    // The overflowing list leaves the scrollbar's gutter free of row content (the trailing
    // status icons would sit under it), but the row itself, and so its divider, spans the
    // whole rail.
    assert_eq!(first.right(), list.right(), "the row spans the list");
    let content = bounds_of("probe-row-tab 0", cx).expect("the row's content");
    assert!(
        content.right() <= list.right() - gpui_component::scroll::Scrollbar::width(),
        "content {content:?} must clear the scrollbar gutter of the list {list:?}"
    );
    assert!(first.top() >= list.top() && first.bottom() <= list.bottom());
    assert!(
        last.bottom() > list.bottom(),
        "40 rows of 64 px do not fit the list ({list:?}), so the last is out of view"
    );

    let (panels, _) = centre_tabs(&window.area, cx);
    let newest = panels[39];
    cx.update(|w, app| {
        window
            .area
            .update(app, |area, cx| area.select_panel(newest, w, cx))
    });
    settle(cx);
    let list = bounds_of(LIST_SELECTOR, cx).unwrap();
    let last = row(39, cx).unwrap();
    assert!(
        last.top() >= list.top() && last.bottom() <= list.bottom(),
        "the newly active row {last:?} must be inside the list {list:?}"
    );
    assert_eq!(centre_tabs(&window.area, cx).1, 39);
}

/// Zoom keeps the rail (it is the group's chrome); the button reads Zoom Out while zoomed.
#[gpui::test]
fn zoom_keeps_the_rail_and_its_button_restores(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    set_centre(&window.area, &terminals(&["a", "b"]), false, &report, cx);
    assert!(bounds_of(format!("{RAIL_SELECTOR}-zoom-in"), cx).is_some());
    assert!(bounds_of(format!("{RAIL_SELECTOR}-zoom-out"), cx).is_none());

    let node = centre_node(&window.area, cx);
    cx.update(|w, app| {
        window
            .area
            .update(app, |area, cx| area.set_zoomed_in(node, w, cx))
    });
    settle(cx);
    assert!(
        bounds_of(RAIL_SELECTOR, cx).is_some(),
        "the zoomed group keeps its rail"
    );
    assert!(bounds_of(format!("{RAIL_SELECTOR}-zoom-out"), cx).is_some());
    assert!(bounds_of(format!("{RAIL_SELECTOR}-zoom-in"), cx).is_none());

    let button = bounds_of(format!("{RAIL_SELECTOR}-zoom-out"), cx).unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    settle(cx);
    assert!(window.area.read_with(cx, |area, _| !area.is_zoomed()));
    assert!(bounds_of(format!("{RAIL_SELECTOR}-zoom-in"), cx).is_some());
}

/// Drag priority (`IN-0055` HLD § Interactions): the title row's own drag wins over the row's
/// reorder drag (also from the trailing status icon, which is row content), and only the grip
/// starts the reorder.
#[gpui::test]
fn the_title_row_drags_into_a_space_and_only_the_grip_reorders(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    set_centre(
        &window.area,
        &terminals(&["a", "b", "c"]),
        true,
        &report,
        cx,
    );
    let (original, _) = centre_tabs(&window.area, cx);
    let target = row(2, cx).unwrap().center();

    // The title row: its own drag starts, and dropping it on another row reorders nothing.
    let title = bounds_of("probe-row-a", cx).expect("the title row is drawn in the rail");
    drag_to(title.origin + point(px(20.), px(10.)), target, cx);
    assert!(has_active_drag(cx), "the title row starts a drag");
    assert_eq!(
        report.title_drags.get(),
        1,
        "it is the title row's own drag"
    );
    release(target, cx);
    assert_eq!(
        centre_tabs(&window.area, cx).0,
        original,
        "no reorder from the title row"
    );

    // The trailing status icon is part of the title row: its drag is the row's own, and it
    // reorders nothing either.
    let status = bounds_of("probe-status-a", cx).unwrap();
    drag_to(status.center(), target, cx);
    assert!(has_active_drag(cx), "the status icon is row content");
    assert_eq!(report.title_drags.get(), 2, "it is the title row's drag");
    release(target, cx);
    assert_eq!(centre_tabs(&window.area, cx).0, original);

    // The grip starts the reorder drag, not the title row's, and dropping it on a row moves
    // the tab to that slot.
    let grip = bounds_of(format!("{GRIP_SELECTOR}-2"), cx).expect("a grip per row");
    let first = row(0, cx).unwrap().center();
    drag_to(grip.center(), first, cx);
    assert!(has_active_drag(cx), "the grip starts a drag");
    assert_eq!(report.title_drags.get(), 2, "not the title row's drag");
    release(first, cx);
    let (reordered, _) = centre_tabs(&window.area, cx);
    assert_eq!(
        reordered,
        vec![original[2], original[0], original[1]],
        "the dragged tab took the slot of the row it was dropped on"
    );
}

/// A row dragged by its grip and dropped below the last row goes to the last slot.
#[gpui::test]
fn a_grip_drag_dropped_below_the_last_row_moves_the_tab_to_the_end(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    set_centre(
        &window.area,
        &terminals(&["a", "b", "c"]),
        false,
        &report,
        cx,
    );
    let (original, _) = centre_tabs(&window.area, cx);

    let grip = bounds_of(format!("{GRIP_SELECTOR}-0"), cx).unwrap();
    let below = bounds_of(EMPTY_SELECTOR, cx)
        .expect("room below the last row")
        .center();
    drag_to(grip.center(), below, cx);
    assert!(has_active_drag(cx));
    release(below, cx);
    assert_eq!(
        centre_tabs(&window.area, cx).0,
        vec![original[1], original[2], original[0]]
    );
}

/// A zoomed group is neither draggable nor droppable, as on the strip; and a lone tab has
/// nowhere to go.
#[gpui::test]
fn a_lone_tab_has_nothing_to_reorder(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    set_centre(&window.area, &terminals(&["only"]), false, &report, cx);
    let grip = bounds_of(format!("{GRIP_SELECTOR}-0"), cx).unwrap();
    let below = bounds_of(EMPTY_SELECTOR, cx).unwrap().center();
    drag_to(grip.center(), below, cx);
    assert!(
        !has_active_drag(cx),
        "the last panel of the dock is not draggable"
    );
    release(below, cx);
}

/// The drop line sits where the tab will land (`drop_edge`): dragged forward it lands after the
/// hovered row, dragged backward before it. Pins base's insert rule the line is drawn for.
#[gpui::test]
fn a_forward_reorder_lands_after_the_hovered_row_and_a_backward_one_before_it(
    cx: &mut TestAppContext,
) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    set_centre(
        &window.area,
        &terminals(&["a", "b", "c"]),
        false,
        &report,
        cx,
    );
    let (original, _) = centre_tabs(&window.area, cx);

    // Forward: the first tab dropped on the second row ends up after it.
    let from = bounds_of(format!("{GRIP_SELECTOR}-0"), cx)
        .unwrap()
        .center();
    let onto = row(1, cx).unwrap().center();
    drag_to(from, onto, cx);
    release(onto, cx);
    assert_eq!(
        centre_tabs(&window.area, cx).0,
        vec![original[1], original[0], original[2]]
    );

    // Backward: the last tab dropped on the first row ends up before it.
    let from = bounds_of(format!("{GRIP_SELECTOR}-2"), cx)
        .unwrap()
        .center();
    let onto = row(0, cx).unwrap().center();
    drag_to(from, onto, cx);
    release(onto, cx);
    assert_eq!(
        centre_tabs(&window.area, cx).0,
        vec![original[2], original[1], original[0]]
    );
}

// ── Resizing the rail (`US-0197`) ────────────────────────────────────────

fn rail_width(cx: &mut VisualTestContext) -> f32 {
    f32::from(
        bounds_of(RAIL_SELECTOR, cx)
            .expect("the rail is drawn")
            .size
            .width,
    )
}

/// Count the updates `UiConfig` announces (a setter notifies once; a drag in memory never does).
fn count_config_updates(
    config: &Entity<UiConfig>,
    cx: &mut VisualTestContext,
) -> (Rc<Cell<usize>>, gpui::Subscription) {
    let updates = Rc::new(Cell::new(0));
    let counted = updates.clone();
    let subscription =
        cx.update(|_, cx| cx.observe(config, move |_, _| counted.set(counted.get() + 1)));
    (updates, subscription)
}

fn stored_width(window: &RailWindow, cx: &mut VisualTestContext) -> u32 {
    window
        .config
        .read_with(cx, |config, _| config.vertical_tabs.width)
}

fn move_to(at: Point<Pixels>, cx: &mut VisualTestContext) {
    cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
    settle(cx);
}

/// Dragging the handle moves the rail's edge with the pointer, in memory; the release stores
/// the preference, once. A press and release that never moved stores nothing.
#[gpui::test]
fn a_drag_resizes_live_and_stores_the_width_once_on_release(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    set_centre(&window.area, &terminals(&["a", "b"]), false, &report, cx);
    let (updates, _subscription) = count_config_updates(&window.config, cx);
    assert_eq!(rail_width(cx), 220.);

    // A click on the handle stores nothing.
    let handle = bounds_of(HANDLE_SELECTOR, cx).expect("the rail has a handle");
    assert_eq!(handle.size.width, px(4.));
    assert_eq!(
        handle.right(),
        bounds_of(RAIL_SELECTOR, cx).unwrap().right()
    );
    cx.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::none());
    settle(cx);
    release(handle.center(), cx);
    assert_eq!(updates.get(), 0, "a click is not a resize");
    assert_eq!(stored_width(&window, cx), 220);

    // A drag: the edge follows the pointer frame by frame, and nothing is stored until release.
    let left = bounds_of(RAIL_SELECTOR, cx).unwrap().left();
    cx.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::none());
    settle(cx);
    move_to(point(left + px(260.), handle.center().y), cx);
    assert_eq!(rail_width(cx), 260.);
    assert_eq!(
        report.bounds.get().origin.x,
        left + px(260.),
        "the terminal follows"
    );
    move_to(point(left + px(300.), handle.center().y), cx);
    assert_eq!(rail_width(cx), 300.);
    assert_eq!(
        updates.get(),
        0,
        "no write, no update, while the pointer moves"
    );
    assert_eq!(stored_width(&window, cx), 220);

    release(point(left + px(300.), handle.center().y), cx);
    assert_eq!(updates.get(), 1, "one store at the release");
    assert_eq!(stored_width(&window, cx), 300);
    assert_eq!(rail_width(cx), 300.);
    // The pointer no longer drives the width.
    move_to(point(left + px(180.), handle.center().y), cx);
    assert_eq!(rail_width(cx), 300.);
    assert_eq!(updates.get(), 1);
}

/// Dragging past the ceiling stops at it; the preference stored is the drag clamped to the
/// absolute bounds, so a wider window honours it. Below the floor it stops at 140.
#[gpui::test]
fn a_drag_stops_at_the_ceiling_and_at_the_floor(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    set_centre(&window.area, &terminals(&["a", "b"]), false, &report, cx);
    // A window narrow enough for the group's share to bind below 400 px.
    let height = cx.update(|window, _| window.viewport_size().height);
    cx.simulate_resize(gpui::size(px(800.), height));
    settle(cx);
    let group = rail_width(cx) + f32::from(report.bounds.get().size.width);
    let ceiling = (group * 0.35).clamp(140., 400.);
    assert!(
        ceiling < 400. && ceiling > 220.,
        "the test window ({group} px) puts the ceiling between the default and 400"
    );

    let handle = bounds_of(HANDLE_SELECTOR, cx).unwrap();
    let left = bounds_of(RAIL_SELECTOR, cx).unwrap().left();
    let far = point(left + px(group - 1.), handle.center().y);
    cx.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::none());
    settle(cx);
    move_to(far, cx);
    assert!(
        (rail_width(cx) - ceiling).abs() < 0.01,
        "stops at the ceiling"
    );
    release(far, cx);
    assert_eq!(
        stored_width(&window, cx),
        400,
        "the preference is the drag, clamped to 400"
    );
    assert!((rail_width(cx) - ceiling).abs() < 0.01);

    // Below the floor.
    let handle = bounds_of(HANDLE_SELECTOR, cx).unwrap();
    let near = point(left + px(20.), handle.center().y);
    cx.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::none());
    settle(cx);
    move_to(near, cx);
    assert_eq!(rail_width(cx), 140.);
    release(near, cx);
    assert_eq!(stored_width(&window, cx), 140);
}

/// Narrowing the window narrows the rail to its group's ceiling; widening it again gives the
/// preference back, and nothing is stored on the way.
#[gpui::test]
fn narrowing_the_window_clamps_the_rail_and_widening_it_restores_the_preference(
    cx: &mut TestAppContext,
) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    set_centre(&window.area, &terminals(&["a", "b"]), false, &report, cx);
    cx.update(|_, cx| UiConfig::set_vertical_tabs_width(300, cx));
    settle(cx);
    let original = cx.update(|window, _| window.viewport_size());
    assert!(
        f32::from(original.width) * 0.35 >= 300.,
        "the window is wide enough for 300"
    );
    assert_eq!(rail_width(cx), 300.);
    let (updates, _subscription) = count_config_updates(&window.config, cx);

    cx.simulate_resize(gpui::size(px(700.), original.height));
    cx.run_until_parked();
    // The first frame at the new size still measures the old group; it asks for another.
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.update(|window, cx| window.simulate_next_frame(cx)) > 0,
        "a changed group width asks for one more frame"
    );
    settle(cx);
    let narrowed = rail_width(cx);
    assert!(
        (narrowed - 245.).abs() < 0.5,
        "35 % of 700 px, got {narrowed}"
    );

    cx.simulate_resize(original);
    settle(cx);
    assert_eq!(rail_width(cx), 300., "the preference comes back");
    assert_eq!(stored_width(&window, cx), 300);
    assert_eq!(updates.get(), 0, "narrowing the window stored nothing");
}

/// A split centre bounds each rail by its own group, not by the window.
#[gpui::test]
fn each_rail_of_a_split_centre_is_bounded_by_its_own_group(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let (first, second) = (Report::default(), Report::default());
    cx.update(|window_, app| {
        let layout = DockLayout::h_split()
            .child(tab_group(&terminals(&["a", "b"]), false, &first, app), None)
            .child(
                tab_group(&terminals(&["c", "d"]), false, &second, app),
                None,
            );
        window
            .area
            .update(app, |area, cx| area.set_center(layout, window_, cx));
    });
    let height = cx.update(|window, _| window.viewport_size().height);
    cx.simulate_resize(gpui::size(px(1000.), height));
    settle(cx);

    // The first group starts at the window's left edge, so its rail is its terminal's left edge.
    let rail = f32::from(first.bounds.get().origin.x);
    let group = rail + f32::from(first.bounds.get().size.width);
    let viewport = f32::from(cx.update(|window, _| window.viewport_size()).width);
    assert!(
        group < viewport * 0.6,
        "two groups share the window: {group} of {viewport} px"
    );
    assert!(rail < 220., "bounded by its group, not the window: {rail}");
    let expected = (group * 0.35).clamp(140., 400.);
    assert!(
        (rail - expected).abs() < 0.01,
        "35 % of the group: {rail} vs {expected}"
    );
    // The other group is the same width, so its terminal is laid out beside a rail of the same
    // size.
    let widths = (
        first.bounds.get().size.width,
        second.bounds.get().size.width,
    );
    assert!(
        (f32::from(widths.0) - f32::from(widths.1)).abs() < 2.,
        "{widths:?}"
    );
}

/// The row is exactly as wide as the live rail (less the scrollbar's gutter when the rows
/// overflow), which is what a path on its second line is cut against.
#[gpui::test]
fn the_rows_follow_the_live_width_of_the_rail(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    set_centre(&window.area, &terminals(&["a", "b"]), false, &report, cx);
    let handle = bounds_of(HANDLE_SELECTOR, cx).unwrap();
    let left = bounds_of(RAIL_SELECTOR, cx).unwrap().left();
    cx.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::none());
    settle(cx);
    for width in [180., 240., 300.] {
        move_to(point(left + px(width), handle.center().y), cx);
        let row = row(0, cx).unwrap();
        // The rail has a 1 px right border.
        assert_eq!(f32::from(row.size.width), width - 1., "at {width}");
    }
    release(point(left + px(300.), handle.center().y), cx);
}

/// The width setter keeps the preference inside 140..=400 and writes nothing for a value that
/// is already stored.
#[gpui::test]
fn the_width_setter_clamps_and_skips_an_unchanged_value(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let (updates, _subscription) = count_config_updates(&window.config, cx);
    for (given, stored, count) in [(220, 220, 0), (9999, 400, 1), (400, 400, 1), (0, 140, 2)] {
        cx.update(|_, cx| UiConfig::set_vertical_tabs_width(given, cx));
        assert_eq!(stored_width(&window, cx), stored, "given {given}");
        assert_eq!(updates.get(), count, "given {given}");
    }
}

fn is_collapsed(window: &RailWindow, cx: &mut VisualTestContext) -> bool {
    window
        .config
        .read_with(cx, |config, _| config.vertical_tabs.collapsed)
}

/// The header button collapses the rail to the 44 px strip and expands it again, live, with
/// no panel rebuilt; the content frame follows, the resize handle exists only while expanded,
/// and the preference's width is back on expanding (`US-0198`).
#[gpui::test]
fn the_header_button_collapses_the_rail_to_a_strip_and_expands_it(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    set_centre(
        &window.area,
        &terminals(&["a", "b", "c"]),
        false,
        &report,
        cx,
    );
    cx.update(|_, cx| UiConfig::set_vertical_tabs_width(300, cx));
    settle(cx);
    assert_eq!(rail_width(cx), 300.);
    assert!(bounds_of(HANDLE_SELECTOR, cx).is_some());
    assert!(bounds_of(format!("{RAIL_SELECTOR}-expand-rail"), cx).is_none());

    let collapse = bounds_of(format!("{RAIL_SELECTOR}-collapse-rail"), cx).expect("collapse");
    cx.simulate_click(collapse.center(), Modifiers::none());
    settle(cx);
    assert!(is_collapsed(&window, cx));
    assert_eq!(rail_width(cx), 44., "the strip, whatever the preference");
    assert_eq!(
        report.bounds.get().origin.x,
        bounds_of(RAIL_SELECTOR, cx).unwrap().right(),
        "the terminal starts where the strip ends"
    );
    assert!(
        bounds_of(HANDLE_SELECTOR, cx).is_none(),
        "no resizing the strip"
    );
    assert!(bounds_of(format!("{RAIL_SELECTOR}-collapse-rail"), cx).is_none());
    let rows: Vec<_> = (0..3)
        .map(|ix| row(ix, cx).expect("a cell per tab"))
        .collect();
    for pair in rows.windows(2) {
        assert_eq!(pair[0].size.height, COLLAPSED_ROW_HEIGHT);
        assert_eq!(pair[1].top(), pair[0].bottom(), "cells stack");
    }
    // The cell is as wide as the strip, less its 1 px border.
    assert_eq!(f32::from(rows[0].size.width), 43.);
    // The stacked header (three 20 px buttons) has a fixed least height.
    let rail = bounds_of(RAIL_SELECTOR, cx).unwrap();
    assert!(rows[0].top() - rail.top() >= px(76.));

    let expand = bounds_of(format!("{RAIL_SELECTOR}-expand-rail"), cx).expect("expand");
    cx.simulate_click(expand.center(), Modifiers::none());
    settle(cx);
    assert!(!is_collapsed(&window, cx));
    assert_eq!(rail_width(cx), 300., "the preference comes back");
    assert!(bounds_of(HANDLE_SELECTOR, cx).is_some());
    assert_eq!(report.removed.get(), 0, "no panel was rebuilt");
}

/// A cell asks the panel for its collapsed layout with the tab's 1-based position, and a click
/// on it selects the tab.
#[gpui::test]
fn a_collapsed_cell_is_built_with_its_position_and_a_click_selects_it(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    cx.update(|_, cx| UiConfig::set_vertical_tabs_collapsed(true, cx));
    set_centre(
        &window.area,
        &terminals(&["a", "b", "c"]),
        true,
        &report,
        cx,
    );
    assert_eq!(
        report.title_layout.get(),
        Some(TabRowLayout::Collapsed(3)),
        "the last cell drawn is the third"
    );
    cx.update(|_, cx| {
        assert_eq!(TabRowLayout::current(cx), TabRowLayout::Horizontal);
    });

    let rows: Vec<_> = (0..3).map(|ix| row(ix, cx).unwrap()).collect();
    assert_eq!(centre_tabs(&window.area, cx).1, 0);
    cx.simulate_click(rows[2].center(), Modifiers::none());
    settle(cx);
    assert_eq!(centre_tabs(&window.area, cx).1, 2);
}

/// Collapsed, the title row's own drag and the grip's reorder still start from the right place:
/// the cell's content drags the tab, and the grip (a 12 px overlay on its left edge) reorders.
#[gpui::test]
fn a_collapsed_cell_keeps_the_content_drag_and_the_grip_reorder(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    cx.update(|_, cx| UiConfig::set_vertical_tabs_collapsed(true, cx));
    set_centre(
        &window.area,
        &terminals(&["a", "b", "c"]),
        true,
        &report,
        cx,
    );
    let (original, _) = centre_tabs(&window.area, cx);
    let target = row(2, cx).unwrap().center();

    let title = bounds_of("probe-row-a", cx).expect("the title row fills the cell");
    drag_to(title.center() + point(px(8.), px(0.)), target, cx);
    assert!(has_active_drag(cx), "the cell's content starts a drag");
    assert_eq!(report.title_drags.get(), 1, "it is the title row's drag");
    release(target, cx);
    assert_eq!(centre_tabs(&window.area, cx).0, original);

    let grip = bounds_of(format!("{GRIP_SELECTOR}-2"), cx).expect("a grip per cell");
    assert_eq!(grip.size.width, px(12.));
    let first = row(0, cx).unwrap().center();
    drag_to(grip.center(), first, cx);
    assert!(has_active_drag(cx), "the grip starts a drag");
    assert_eq!(report.title_drags.get(), 1, "not the title row's drag");
    release(first, cx);
    assert_eq!(
        centre_tabs(&window.area, cx).0,
        vec![original[2], original[0], original[1]]
    );
}

/// Turning the Vertical Tabs switch off keeps the collapsed state, and on again returns to the
/// strip.
#[gpui::test]
fn the_switch_off_keeps_the_rail_collapsed(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    set_centre(&window.area, &terminals(&["a", "b"]), false, &report, cx);
    cx.update(|_, cx| UiConfig::set_vertical_tabs_collapsed(true, cx));
    settle(cx);
    assert_eq!(rail_width(cx), 44.);

    set_vertical(&window.config, false, cx);
    assert!(bounds_of(RAIL_SELECTOR, cx).is_none(), "the strip is back");
    assert!(is_collapsed(&window, cx), "and the collapse is remembered");

    set_vertical(&window.config, true, cx);
    assert_eq!(rail_width(cx), 44.);
}

/// The collapse setter stores the flag, redraws, and does nothing for a value already stored.
#[gpui::test]
fn the_collapse_setter_skips_an_unchanged_value(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let (updates, _subscription) = count_config_updates(&window.config, cx);
    for (given, count) in [(false, 0), (true, 1), (true, 1), (false, 2)] {
        cx.update(|_, cx| UiConfig::set_vertical_tabs_collapsed(given, cx));
        assert_eq!(is_collapsed(&window, cx), given);
        assert_eq!(updates.get(), count, "given {given}");
    }
}

/// Advisor (`US-0198` verify): 40 collapsed cells overflow the list without a scrollbar gutter
/// (each cell is the strip less its 1 px border), and a tab that becomes active from elsewhere
/// is scrolled into view, as expanded rows are.
#[gpui::test]
fn forty_collapsed_cells_scroll_without_a_gutter_and_reveal_the_active_one(
    cx: &mut TestAppContext,
) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    cx.update(|_, cx| UiConfig::set_vertical_tabs_collapsed(true, cx));
    let labels: Vec<&'static str> = (0..40).map(|ix| &*format!("tab {ix}").leak()).collect();
    set_centre(&window.area, &terminals(&labels), false, &report, cx);

    let list = bounds_of(LIST_SELECTOR, cx).expect("the list is drawn");
    let first = row(0, cx).unwrap();
    let last = row(39, cx).unwrap();
    assert_eq!(first.size.height, COLLAPSED_ROW_HEIGHT);
    assert_eq!(f32::from(first.size.width), 43., "no gutter is reserved");
    assert!(last.bottom() > list.bottom(), "40 cells of 44 px overflow");

    let (panels, _) = centre_tabs(&window.area, cx);
    let newest = panels[39];
    cx.update(|w, app| {
        window
            .area
            .update(app, |area, cx| area.select_panel(newest, w, cx))
    });
    settle(cx);
    let list = bounds_of(LIST_SELECTOR, cx).unwrap();
    let last = row(39, cx).unwrap();
    assert!(
        last.top() >= list.top() && last.bottom() <= list.bottom(),
        "the newly active cell {last:?} must be inside the list {list:?}"
    );
}

/// Advisor pin (`US-0198` verify, finding 1): the scroll offset is kept in pixels across a
/// collapse or an expand, so with many tabs the active row would leave the viewport when the
/// row height changes (44 -> 64 px here). The rail re-reveals the active row when the layout
/// flips.
#[gpui::test]
fn expanding_the_rail_keeps_the_active_row_in_view(cx: &mut TestAppContext) {
    let (window, cx) = rail_window(cx, true);
    let report = Report::default();
    cx.update(|_, cx| UiConfig::set_vertical_tabs_collapsed(true, cx));
    let labels: Vec<&'static str> = (0..40).map(|ix| &*format!("tab {ix}").leak()).collect();
    set_centre(&window.area, &terminals(&labels), false, &report, cx);
    let (panels, _) = centre_tabs(&window.area, cx);
    let newest = panels[39];
    cx.update(|w, app| {
        window
            .area
            .update(app, |area, cx| area.select_panel(newest, w, cx))
    });
    settle(cx);
    let list = bounds_of(LIST_SELECTOR, cx).unwrap();
    let last = row(39, cx).unwrap();
    assert!(last.top() >= list.top() && last.bottom() <= list.bottom());

    let expand = bounds_of(format!("{RAIL_SELECTOR}-expand-rail"), cx).expect("expand");
    cx.simulate_click(expand.center(), Modifiers::none());
    settle(cx);
    assert_eq!(rail_width(cx), 220.);
    let list = bounds_of(LIST_SELECTOR, cx).unwrap();
    let last = row(39, cx).expect("row 39 is drawn");
    assert!(
        last.top() >= list.top() && last.bottom() <= list.bottom(),
        "the active row {last:?} must still be inside the list {list:?} after expanding"
    );
}
