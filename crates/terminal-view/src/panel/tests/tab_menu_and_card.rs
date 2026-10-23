//! `US-0199` (the tab menu's Input Channel rows) and `US-0198` rework 5 (the collapsed cell's
//! hover card).

use gpui::{AppContext as _, TestAppContext, VisualTestContext};
use oneterm_core::InputChannel;
use oneterm_state::InputChannelRegistry;
use oneterm_terminal::test_support::FakeTerminalSession;

use super::{report_done, session_spec};
use crate::panel::TerminalPanel;

/// The tab menu's "Input Channel" rows act on the tab that was right-clicked and on its active
/// Space, not on whichever tab is active, and the marker follows the join.
#[gpui::test]
fn the_tab_menus_channel_items_act_on_the_right_clicked_tab(cx: &mut TestAppContext) {
    use crate::panel::channel_menu::ChannelMenuItem;

    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(InputChannelRegistry::init);

    let (session_a, _) = FakeTerminalSession::boxed(24, 80, "A");
    let (first, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_spec(session_spec(session_a, "A"), window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    let (session_b, _) = FakeTerminalSession::boxed(24, 80, "B");
    let second = first.update_in(cx, |_, window, cx| {
        cx.new(|cx| TerminalPanel::from_spec(session_spec(session_b, "B"), window, cx))
    });
    cx.run_until_parked();

    let channels = |panel: &gpui::Entity<TerminalPanel>, cx: &mut VisualTestContext| {
        panel.read_with(cx, |panel, cx| panel.tab_channels(cx))
    };
    // Right-click the second tab while the first is the one in front.
    second.update(cx, |panel, cx| {
        panel.apply_channel_item(ChannelMenuItem::Join(InputChannel::B, false), cx)
    });
    assert_eq!(channels(&second, cx), vec![InputChannel::B]);
    assert!(
        channels(&first, cx).is_empty(),
        "the other tab is untouched"
    );

    // The marker is the active Space's channel.
    let entries = second
        .read_with(cx, |panel, cx| panel.channel_entries(cx))
        .expect("a tab with a terminal offers the submenu");
    assert!(entries.contains(&ChannelMenuItem::Join(InputChannel::B, true)));
    assert!(entries.contains(&ChannelMenuItem::Close(InputChannel::B)));
    let untouched = first
        .read_with(cx, |panel, cx| panel.channel_entries(cx))
        .expect("the other tab offers it too");
    assert!(
        untouched
            .iter()
            .all(|item| !matches!(item, ChannelMenuItem::Join(_, true))),
        "no channel is marked on a tab that is in none"
    );

    // Leave from one tab leaves the other alone; Close empties the channel everywhere.
    first.update(cx, |panel, cx| {
        panel.apply_channel_item(ChannelMenuItem::Join(InputChannel::B, false), cx)
    });
    assert_eq!(channels(&first, cx), vec![InputChannel::B]);
    second.update(cx, |panel, cx| {
        panel.apply_channel_item(ChannelMenuItem::Leave, cx)
    });
    assert!(channels(&second, cx).is_empty());
    assert_eq!(channels(&first, cx), vec![InputChannel::B]);
    second.update(cx, |panel, cx| {
        panel.apply_channel_item(ChannelMenuItem::Close(InputChannel::B), cx)
    });
    assert!(
        channels(&first, cx).is_empty(),
        "closing a channel removes its members in every tab"
    );
}

/// Renders the collapsed cell's tooltip the way gpui does: its root sits 1 px right of and below
/// the pointer, and the card is placed from there.
struct CardHost {
    panel: gpui::Entity<TerminalPanel>,
    tooltip: gpui::Entity<crate::panel::tab_title::CellTooltip>,
    pointer: gpui::Point<gpui::Pixels>,
}

impl gpui::Render for CardHost {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{ParentElement as _, Styled as _};
        gpui::div().size_full().child(
            gpui::div()
                .absolute()
                .left(self.pointer.x + gpui::px(1.))
                .top(self.pointer.y + gpui::px(1.))
                .child(self.tooltip.clone()),
        )
    }
}

/// The strip's cell the card is shown for, in the tests' window: 43 x 44 px, 8 px from its edge.
fn test_cell() -> gpui::Bounds<gpui::Pixels> {
    gpui::Bounds::new(
        gpui::point(gpui::px(8.), gpui::px(120.)),
        gpui::size(gpui::px(43.), gpui::px(44.)),
    )
}

/// A window showing the hover card of a tab titled `title` for `test_cell()` with the pointer at
/// `pointer`, and the probe of its session.
fn card_window<'a>(
    cx: &'a mut TestAppContext,
    title: &str,
    pointer: gpui::Point<gpui::Pixels>,
) -> (
    gpui::Entity<TerminalPanel>,
    oneterm_terminal::test_support::FakeSessionProbe,
    &'a mut VisualTestContext,
) {
    use oneterm_terminal::SessionKind;

    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(InputChannelRegistry::init);
    let (session, probe) =
        FakeTerminalSession::boxed_with_kind(24, 80, "first", SessionKind::Local);
    let title = title.to_string();
    let (host, cx) = cx.add_window_view(move |window, cx| {
        let panel =
            cx.new(|cx| TerminalPanel::from_spec(session_spec(session, &title), window, cx));
        let tooltip = cx.new(|_| crate::panel::tab_title::CellTooltip {
            panel: panel.downgrade(),
            cell: test_cell(),
            pointer,
            hidden: Default::default(),
        });
        CardHost {
            panel,
            tooltip,
            pointer,
        }
    });
    let panel = host.read_with(cx, |host, _| host.panel.clone());
    (panel, probe, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

/// The collapsed cell's hover card carries the expanded row's content: the title, then the
/// directory and the status icon on the second line, as wide as that content.
#[gpui::test]
fn the_collapsed_cells_hover_card_has_the_expanded_rows_content(cx: &mut TestAppContext) {
    use gpui::{point, px};

    use crate::panel::tab_title::{TAB_CARD_LABEL_SELECTOR, TAB_CWD_SELECTOR, TAB_STATUS_SELECTOR};

    let (_panel, probe, cx) = card_window(cx, "First", point(px(20.), px(130.)));
    probe.set_cwd(Some(std::path::PathBuf::from("Z:/oneterm-test/x")));
    draw(cx);

    let title = cx
        .debug_bounds(TAB_CARD_LABEL_SELECTOR)
        .expect("the card has the title");
    let cwd = cx
        .debug_bounds(TAB_CWD_SELECTOR)
        .expect("the card has the directory");
    assert!(
        cx.debug_bounds(TAB_STATUS_SELECTOR).is_none(),
        "no status yet"
    );
    assert!(
        cwd.top() >= title.bottom() - px(1.),
        "the directory {cwd:?} is on the line below the title {title:?}"
    );

    report_done(&probe, cx);
    draw(cx);
    let status = cx
        .debug_bounds(TAB_STATUS_SELECTOR)
        .expect("the card has the status icon");
    assert!(
        (status.center().y - cwd.center().y).abs() <= px(1.) && cwd.right() <= status.left(),
        "the icon {status:?} follows the directory {cwd:?} on line 2"
    );
    assert_eq!(
        title.left(),
        cwd.left(),
        "both lines start at the same edge"
    );
    // Content-sized: the title is as wide as its text, not stretched across the card.
    assert!(title.size.width < px(120.), "{title:?}");
}

/// Draw the card and read its frame and its directory line.
fn drawn_card(
    cx: &mut VisualTestContext,
) -> (
    gpui::Bounds<gpui::Pixels>,
    Option<gpui::Bounds<gpui::Pixels>>,
) {
    use crate::panel::tab_title::{TAB_CARD_SELECTOR, TAB_CWD_SELECTOR};

    draw(cx);
    (
        cx.debug_bounds(TAB_CARD_SELECTOR)
            .expect("the card is drawn"),
        cx.debug_bounds(TAB_CWD_SELECTOR),
    )
}

/// Owner rework 7: the hover card is one line when the tab has no directory (no reserved blank
/// second line) and two with one, as wide as its content, and never wider than 280 px.
#[gpui::test]
fn the_hover_card_is_one_line_without_a_directory_and_hugs_its_content(cx: &mut TestAppContext) {
    use gpui::{point, px};

    use crate::panel::tab_title::{HOVER_CARD_MAX_WIDTH, hover_card_height};

    let pointer = point(px(20.), px(130.));
    let (_panel, probe, window) = card_window(cx, "PowerShell", pointer);
    let (one, cwd) = drawn_card(window);
    assert!(cwd.is_none(), "no directory, no second line");
    assert_eq!(one.size.height, hover_card_height(false));
    assert!(one.size.width < px(160.), "a short title: {one:?}");

    probe.set_cwd(Some(std::path::PathBuf::from("Z:/oneterm-test/x")));
    let (two, cwd) = drawn_card(window);
    assert!(cwd.is_some());
    assert_eq!(two.size.height, hover_card_height(true));
    assert!(
        two.size.height > one.size.height,
        "the directory adds a line"
    );

    // A long title and a long directory cap the card at 280 px.
    let long = "a-rather-long-tab-title ".repeat(12);
    let (_panel, probe, window) = card_window(cx, &long, pointer);
    probe.set_cwd(Some(std::path::PathBuf::from(
        "Z:/oneterm-test/a-rather-long-directory/with/many/nested/folders/src/panel/more/and/more",
    )));
    let (wide, _) = drawn_card(window);
    assert_eq!(wide.size.width, px(HOVER_CARD_MAX_WIDTH), "{wide:?}");
}

/// Owner rework 7: the card is a popover (the kit's tooltip look) with a 6 px radius, from the
/// theme's popover, foreground and border tokens.
#[gpui::test]
fn the_hover_card_is_a_popover_with_a_six_pixel_radius(cx: &mut TestAppContext) {
    use gpui::{Styled as _, point, px};
    use gpui_component::ActiveTheme as _;

    let (panel, _probe, cx) = card_window(cx, "First", point(px(20.), px(130.)));
    cx.update(|_, cx| {
        let mut card =
            crate::panel::tab_title::tab_hover_card(panel.read(cx), panel.entity_id(), cx);
        let theme = cx.theme();
        let style = card.style();
        assert_eq!(
            style.corner_radii.top_left,
            Some(px(6.).into()),
            "a 6 px radius, not the theme's 4 px"
        );
        assert_eq!(style.background, Some(theme.tokens.popover.into()));
        assert_eq!(style.border_color, Some(theme.border));
        assert_eq!(style.text.color, Some(theme.popover_foreground));
        assert!(style.box_shadow.is_some(), "the popover shadow");
    });
}

/// Owner rework 7: the New Terminal `+` is a 24 px button in the rail (whose header has the
/// room) and keeps the strip's 20 px there.
#[test]
fn the_new_terminal_button_is_larger_in_the_rail() {
    use gpui_component::Size;
    use oneterm_state::TabRowLayout;

    use crate::panel::tab_title::new_tab_button_size;

    assert_eq!(new_tab_button_size(TabRowLayout::Horizontal), Size::XSmall);
    assert_eq!(new_tab_button_size(TabRowLayout::Vertical), Size::Small);
    assert_eq!(new_tab_button_size(TabRowLayout::Collapsed), Size::Small);
}

/// Owner rework 7, drawn: the card's left edge is 6 px right of the strip's outer edge (the
/// cell's right edge and the rail's 1 px border) and its middle is level with the cell's,
/// wherever in the cell the pointer is, with and without a directory.
#[gpui::test]
fn the_drawn_hover_card_is_six_pixels_from_the_strip_wherever_the_pointer_is(
    cx: &mut TestAppContext,
) {
    use gpui::{point, px};
    use oneterm_state::TabRowLayout;

    use crate::panel::tab_title::HOVER_CARD_GAP;

    let cell = test_cell();
    for pointer in [
        point(cell.left() + px(1.), cell.top() + px(1.)),
        point(cell.center().x, cell.center().y),
        point(cell.right() - px(1.), cell.bottom() - px(1.)),
    ] {
        for cwd in [None, Some("Z:/oneterm-test/x")] {
            let (_panel, probe, window) = card_window(cx, "First", pointer);
            probe.set_cwd(cwd.map(std::path::PathBuf::from));
            let (card, _) = drawn_card(window);
            assert_eq!(
                card.left(),
                cell.right() + px(TabRowLayout::RAIL_BORDER_WIDTH) + px(HOVER_CARD_GAP),
                "pointer {pointer:?}, directory {cwd:?}"
            );
            assert!(
                (card.center().y - cell.center().y).abs() <= px(0.5),
                "level with the cell: {card:?}, pointer {pointer:?}"
            );
        }
    }
}
/// Owner rework 7: the card's left edge is 6 px right of the strip's outer edge whatever the
/// pointer's position in the cell, and it is level with the cell's middle. gpui puts a native
/// tooltip 1 px right of and below the pointer; the offset is what moves it from there.
#[test]
fn the_hover_card_goes_six_pixels_from_the_strip_wherever_the_pointer_is() {
    use gpui::{Bounds, point, px, size};
    use oneterm_state::TabRowLayout;

    use crate::panel::tab_title::{HOVER_CARD_GAP, hover_card_height, hover_card_offset};

    let cell = Bounds::new(point(px(8.), px(120.)), size(px(43.), px(44.)));
    for has_cwd in [false, true] {
        let height = hover_card_height(has_cwd);
        for pointer in [
            point(px(9.), px(121.)),
            point(px(30.), px(140.)),
            point(px(50.), px(163.)),
        ] {
            let at = pointer + point(px(1.), px(1.)) + hover_card_offset(cell, pointer, height);
            assert_eq!(
                at.x,
                cell.right() + px(TabRowLayout::RAIL_BORDER_WIDTH) + px(HOVER_CARD_GAP),
                "{pointer:?}"
            );
            assert_eq!(at.y + height / 2., cell.center().y, "{pointer:?}");
        }
    }
}

/// A tab with no terminal (the shell failed to spawn, or its Space was emptied) or whose
/// session has exited (`US-0200`) has no submenu to offer (its row is shown disabled); a tab
/// with a running terminal has the live one.
#[gpui::test]
fn only_a_tab_without_a_running_terminal_loses_the_channel_submenu(cx: &mut TestAppContext) {
    use oneterm_terminal::SessionEvent;

    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(InputChannelRegistry::init);
    let (session, probe) = FakeTerminalSession::boxed(24, 80, "A");
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_spec(session_spec(session, "A"), window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();

    let entries = |cx: &mut VisualTestContext| panel.read_with(cx, |p, cx| p.channel_entries(cx));
    assert!(entries(cx).is_some());

    // A real session clears `alive` before it publishes `Exited`; the fake needs both.
    probe.set_alive(false);
    probe
        .emit(SessionEvent::Exited(Some(0)))
        .expect("event channel must accept the exit");
    cx.run_until_parked();
    assert!(
        entries(cx).is_none(),
        "an exited session cannot join a channel, so its row is disabled"
    );

    panel.update(cx, |panel, cx| {
        panel.tree = crate::space::SpaceTree::new_empty(cx.focus_handle());
    });
    assert!(
        entries(cx).is_none(),
        "a tab with no terminal offers none (its row is disabled)"
    );
}

/// Advisor check (`US-0198` rework 7): the right-press hide still works through the placed
/// card. While `hidden` is set the tooltip draws no card at all; clearing it brings the card
/// back at its place.
#[gpui::test]
fn the_hover_card_is_not_drawn_while_a_right_press_hides_it(cx: &mut TestAppContext) {
    use gpui::{point, px};

    use crate::panel::tab_title::TAB_CARD_SELECTOR;

    let (_panel, _probe, cx) = card_window(cx, "First", point(px(20.), px(130.)));
    let (card, _) = drawn_card(cx);
    assert!(card.size.width > px(0.));

    let hidden = cx.update(|window, cx| {
        let host = window.root::<CardHost>().flatten().expect("the card host");
        host.read(cx).tooltip.read(cx).hidden.clone()
    });
    hidden.set(true);
    cx.update(|window, _| window.refresh());
    draw(cx);
    assert!(
        cx.debug_bounds(TAB_CARD_SELECTOR).is_none(),
        "a hidden tooltip draws no card"
    );

    hidden.set(false);
    cx.update(|window, _| window.refresh());
    let (card, _) = drawn_card(cx);
    assert!(card.size.width > px(0.), "the card is back");
}

/// Open the `...` menu of a real kit tab strip holding two terminal tabs and read the labels of
/// the rows it draws. `override_installed` runs `crate::init`, which installs the rebuild of that
/// menu (`US-0203`); without it the kit's own menu is what opens.
fn strip_menu_rows(cx: &mut TestAppContext, override_installed: bool) -> Vec<String> {
    use gpui::{Modifiers, point, px};
    use gpui_component::{
        Root,
        dock::{DockLayout, DockSkin, panel_handle},
    };

    cx.update(gpui_component::init);
    if override_installed {
        cx.update(crate::init);
    }
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(oneterm_state::AppState::init);

    let (first_session, _) = FakeTerminalSession::boxed(24, 80, "first");
    let (second_session, _) = FakeTerminalSession::boxed(24, 80, "second");
    let handed = std::rc::Rc::new(std::cell::RefCell::new(None));
    let handed_in = handed.clone();
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        let (dock_area, _skin) = DockSkin::dock_area("strip-menu-test", None, window, cx);
        let first =
            cx.new(|cx| TerminalPanel::from_spec(session_spec(first_session, "First"), window, cx));
        let second = cx
            .new(|cx| TerminalPanel::from_spec(session_spec(second_session, "Second"), window, cx));
        dock_area.update(cx, |dock_area, cx| {
            dock_area.set_center(
                DockLayout::tabs()
                    .panel_view(panel_handle(first), cx)
                    .panel_view(panel_handle(second), cx),
                window,
                cx,
            );
        });
        *handed_in.borrow_mut() = Some(dock_area.clone());
        Root::new(dock_area, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    let dock_area = handed.borrow().clone().expect("the window was built");
    let draw = |cx: &mut VisualTestContext| {
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };
    draw(cx);
    draw(cx);

    // The strip's `...` button is the last control of the bar, 8 px of padding from its end.
    let area = dock_area.read_with(cx, |area, _| area.bounds());
    let tab = cx.debug_bounds("dock-tab-1").expect("the strip is drawn");
    cx.simulate_click(
        point(area.right() - px(18.), tab.center().y),
        Modifiers::none(),
    );
    // The menu is drawn as the kit built it; a rebuild is an effect of its creation and shows
    // in the next frame.
    for _ in 0..3 {
        draw(cx);
    }
    cx.update(|window, _| {
        gpui_base::test_support::snapshots(window)
            .into_iter()
            .filter(|element| element.role() == Some(gpui::Role::MenuItem))
            .filter_map(|element| Some(element.label()?.to_string()))
            .collect()
    })
}

/// `US-0203` (ui-verifier D1): the strip's `...` menu kept a greyed "Zoom In" row, which the kit
/// appends after a panel's rows and gives a panel no way to omit. With the rebuild installed the
/// menu lists the tabs and nothing else (the terminal panel is not closable, so no Close row).
#[gpui::test]
fn the_strip_ellipsis_menu_has_the_tab_list_and_no_zoom_row(cx: &mut TestAppContext) {
    let rows = strip_menu_rows(cx, true);
    assert!(
        rows.len() >= 2,
        "the menu opened and lists the two tabs: {rows:?}"
    );
    assert!(
        rows.iter().all(|row| !row.contains("Zoom")),
        "no zoom row: {rows:?}"
    );
}

/// The control for the test above: without the rebuild the kit's own menu opens with its zoom
/// row, so the click does reach the menu and the rows are read.
#[gpui::test]
fn without_the_rebuild_the_kits_strip_menu_still_has_its_zoom_row(cx: &mut TestAppContext) {
    let rows = strip_menu_rows(cx, false);
    assert!(
        rows.iter().any(|row| row.contains("Zoom In")),
        "control: the kit's menu has its zoom row: {rows:?}"
    );
}

/// Pinned by the advisor (`US-0203` rework D1 review, defect 1), not run by the advisor: opening
/// the `...` menu with no rebuild installed arms the one-shot in `tab_list_menu` and nothing takes
/// it, so it stays armed in this thread's local with a strong handle to the panel. The next
/// `PopupMenu` created on the same thread with the rebuild installed, in another test, is then
/// rewritten as this panel's tab list through an entity of a dropped `App`. Passes once the slot is
/// disarmed when no observer takes it (or the helper clears it after the control walk).
#[gpui::test]
fn the_control_walk_leaves_no_strip_menu_rebuild_armed(cx: &mut TestAppContext) {
    let _ = strip_menu_rows(cx, false);
    assert!(
        !crate::panel::tab_title::strip_menu_is_armed(),
        "the one-shot rebuild is still armed after a walk with no observer installed"
    );
}
