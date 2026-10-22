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

/// Renders a tab's hover card on its own.
struct CardHost {
    panel: gpui::Entity<TerminalPanel>,
}

impl gpui::Render for CardHost {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{ParentElement as _, Styled as _};
        let panel = self.panel.clone();
        gpui::div()
            .size_full()
            .child(crate::panel::tab_title::tab_hover_card(
                panel.read(cx),
                panel.entity_id(),
                cx,
            ))
    }
}

/// The collapsed cell's hover card carries the expanded row's content: the title, then the
/// directory and the status icon on the second line, at the rail's default width.
#[gpui::test]
fn the_collapsed_cells_hover_card_has_the_expanded_rows_content(cx: &mut TestAppContext) {
    use gpui::px;
    use oneterm_terminal::SessionKind;

    use crate::panel::tab_title::{TAB_CARD_LABEL_SELECTOR, TAB_CWD_SELECTOR, TAB_STATUS_SELECTOR};

    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(InputChannelRegistry::init);
    let (session, probe) =
        FakeTerminalSession::boxed_with_kind(24, 80, "first", SessionKind::Local);
    let (_host, cx) = cx.add_window_view(move |window, cx| {
        let panel =
            cx.new(|cx| TerminalPanel::from_spec(session_spec(session, "First"), window, cx));
        CardHost { panel }
    });
    probe.set_cwd(Some(std::path::PathBuf::from("Z:/oneterm-test/x")));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

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
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let status = cx
        .debug_bounds(TAB_STATUS_SELECTOR)
        .expect("the card has the status icon");
    assert!(
        (status.center().y - cwd.center().y).abs() <= px(1.) && cwd.right() <= status.left(),
        "the icon {status:?} follows the directory {cwd:?} on line 2"
    );
    // 220 px in all: the card's content (202 px) plus the kit tooltip's border and padding.
    assert_eq!(
        title.left(),
        cwd.left(),
        "both lines start at the same edge"
    );
    assert!(title.size.width <= px(202.), "{title:?}");
}

/// A tab with no terminal (the shell failed to spawn, or its Space was emptied) has no submenu
/// to offer (its row is shown disabled), while a tab whose process has exited still has its
/// terminal and therefore the live submenu.
#[gpui::test]
fn only_a_tab_without_a_terminal_loses_the_channel_submenu(cx: &mut TestAppContext) {
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

    probe
        .emit(SessionEvent::Exited(Some(0)))
        .expect("event channel must accept the exit");
    cx.run_until_parked();
    assert!(
        entries(cx).is_some(),
        "an exited session is still a terminal and offers the submenu"
    );

    panel.update(cx, |panel, cx| {
        panel.tree = crate::space::SpaceTree::new_empty(cx.focus_handle());
    });
    assert!(
        entries(cx).is_none(),
        "a tab with no terminal offers none (its row is disabled)"
    );
}
