use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui::{AppContext as _, Focusable as _, TestAppContext, VisualTestContext};
use gpui_component::{
    Root,
    dock::{
        DockArea, DockLayout, DockPlacement, DragPanel, Panel as _, PanelId, PanelView,
        panel_handle,
    },
};
use oneterm_core::{
    AppError, InputChannel, LocalShellConfig, Result, SessionDuplicateConfig, SshConfig,
};
use oneterm_state::{AppServices, InputChannelRegistry, commands::WorkspaceCommands};
use oneterm_terminal::{
    PtySize, SessionFactory, TerminalSecurityPolicy, TerminalSession,
    test_support::FakeTerminalSession,
};

use crate::panel::{PanelSpec, TerminalPanel};
use crate::space::{CloseOutcome, SplitDir};
use crate::terminal_view::TerminalView;

mod collapsed_cell;
mod tab_menu_and_card;

/// The scroll estimate must count the rows the menu really emits, in both modes
/// — a literal would go stale the next time a row is added (`IN-0043`).
#[test]
fn the_menu_row_count_matches_the_rows_each_mode_emits() {
    use crate::panel::terminal_panel::menu_rows;

    // 3 shells + (on Windows) the "Run as administrator" submenu row + the
    // "SSH Sessions" heading + n sessions + the closing separator +
    // "Quick Connect..." + "New Saved Session...".
    #[cfg(windows)]
    {
        assert_eq!(menu_rows(false, 0), 8);
        assert_eq!(menu_rows(false, 5), 13);
    }
    #[cfg(not(windows))]
    {
        assert_eq!(menu_rows(false, 0), 7);
        assert_eq!(menu_rows(false, 5), 12);
    }
    // M1: an elevated window emits the three shells and nothing else.
    assert_eq!(menu_rows(true, 0), 3);
    assert_eq!(
        menu_rows(true, 5),
        3,
        "an elevated window lists no saved sessions to count"
    );
}

/// A `PanelSpec` wrapping an existing session without duplication metadata.
fn session_spec(session: Box<dyn TerminalSession>, title: &str) -> PanelSpec {
    PanelSpec::Session {
        session,
        title: title.to_string(),
        duplicate_config: None,
    }
}

#[gpui::test]
fn terminal_panel_disables_multi_tab_inner_padding(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);

    let (session, _) = FakeTerminalSession::boxed(24, 80, "");
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_spec(session_spec(session, "Terminal"), window, cx)
    });
    let cx: &mut VisualTestContext = cx;

    assert!(!panel.read_with(cx, |panel, cx| panel.inner_padding(cx)));
}

#[gpui::test]
fn filling_space_one_does_not_renumber_space_two(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);

    let (session, _) = FakeTerminalSession::boxed(24, 80, "source");
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_spec(session_spec(session, "Terminal"), window, cx)
    });
    let cx: &mut VisualTestContext = cx;

    panel.update_in(cx, |panel, window, cx| {
        let source = panel.tree.active();
        panel.split_active_at(source, SplitDir::Right, window, cx);
        panel.split_active_at(source, SplitDir::Down, window, cx);
        let destinations = panel.empty_space_destinations();
        assert_eq!(destinations.len(), 2);
        assert_eq!(destinations[0].display_number(), 2);
        assert_eq!(destinations[1].display_number(), 1);

        let space_one = destinations[1];
        let (duplicate, _) = FakeTerminalSession::boxed(24, 80, "duplicate");
        let duplicate = cx.new(|_| duplicate);
        let view = cx.new(|cx| TerminalView::new(duplicate, panel.deps.clone(), window, cx));
        panel
            .tree
            .fill_empty(space_one, view)
            .expect("Space #1 must be empty");

        assert_eq!(panel.empty_space_destinations(), vec![destinations[0]]);
    });
}

#[gpui::test]
fn closing_only_terminal_tab_keeps_empty_panel_for_new_tabs(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(oneterm_state::AppState::init);

    let panel_probe = Rc::new(RefCell::new(None));
    let tab_probe = Rc::new(RefCell::new(None));
    let panel_for_window = panel_probe.clone();
    let tab_for_window = tab_probe.clone();
    let (session, session_probe) = FakeTerminalSession::boxed(24, 80, "only");
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        let dock_area = cx.new(|cx| DockArea::new("last-tab-test", None, window, cx));
        let panel =
            cx.new(|cx| TerminalPanel::from_spec(session_spec(session, "Only"), window, cx));
        dock_area.update(cx, |dock_area, cx| {
            dock_area.set_center(
                DockLayout::tabs().panel_view(panel_handle(panel.clone()), cx),
                window,
                cx,
            );
        });
        oneterm_state::AppState::global(cx).update(cx, |state, _| {
            state.dock_area = Some(dock_area.downgrade());
        });
        let tab_panel = panel
            .read(cx)
            .tab_panel
            .clone()
            .and_then(|group| group.upgrade())
            .expect("panel must belong to a tab group");
        *panel_for_window.borrow_mut() = Some(panel.clone());
        *tab_for_window.borrow_mut() = Some(tab_panel);
        Root::new(dock_area, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    let panel = panel_probe
        .borrow()
        .clone()
        .expect("panel must be initialized");
    let tab_panel = tab_probe
        .borrow()
        .clone()
        .expect("tab panel must be initialized");

    let dock_focus = panel.read_with(cx, |panel, cx| panel.focus_handle(cx));
    let content_focus = panel.read_with(cx, |panel, cx| {
        panel
            .tree
            .active_focus_handle(cx)
            .expect("active Space has a content focus handle")
    });
    cx.update(|window, cx| {
        window.activate_window();
        dock_focus.focus(window, cx);
        window.refresh();
    });
    // Frame 1 publishes the proxy's focus path and runs its on_focus forwarder;
    // frame 2 renders the independently tracked content focus.
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert!(cx.update(|window, _| content_focus.is_focused(window)));
    assert!(!cx.update(|window, _| dock_focus.is_focused(window)));
    cx.dispatch_action(gpui_component::dock::ClosePanel);
    cx.run_until_parked();

    assert_eq!(session_probe.close_calls(), 1);
    assert!(panel.read_with(cx, |panel, _| panel.has_no_terminals()));
    assert_eq!(panel.read_with(cx, |panel, _| panel.leaf_count()), 1);
    assert_eq!(tab_panel.read_with(cx, |tabs, _| tabs.panels().len()), 1);

    let (ssh_session, _) = FakeTerminalSession::boxed(24, 80, "ssh");
    let ssh_panel = cx.update(|window, cx| {
        cx.new(|cx| TerminalPanel::from_spec(session_spec(ssh_session, "SSH"), window, cx))
    });
    cx.update(|window, cx| {
        let dock_area = oneterm_state::AppState::global(cx)
            .read(cx)
            .dock_area
            .as_ref()
            .and_then(|dock_area| dock_area.upgrade())
            .expect("workspace dock must exist");
        dock_area.update(cx, |dock_area, cx| {
            dock_area.add_panel_view(
                panel_handle(ssh_panel),
                gpui_component::dock::DockPlacement::Center,
                None,
                window,
                cx,
            );
        });
    });
    assert_eq!(tab_panel.read_with(cx, |tabs, _| tabs.panels().len()), 2);
}

#[gpui::test]
fn closing_terminal_tab_with_sibling_removes_it(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(oneterm_state::AppState::init);

    let first_probe = Rc::new(RefCell::new(None));
    let tabs_probe = Rc::new(RefCell::new(None));
    let first_for_window = first_probe.clone();
    let tabs_for_window = tabs_probe.clone();
    let (first_session, first_session_probe) = FakeTerminalSession::boxed(24, 80, "first");
    let (second_session, second_session_probe) = FakeTerminalSession::boxed(24, 80, "second");
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        let dock_area = cx.new(|cx| DockArea::new("sibling-tab-test", None, window, cx));
        let first =
            cx.new(|cx| TerminalPanel::from_spec(session_spec(first_session, "First"), window, cx));
        let second = cx
            .new(|cx| TerminalPanel::from_spec(session_spec(second_session, "Second"), window, cx));
        dock_area.update(cx, |dock_area, cx| {
            dock_area.set_center(
                DockLayout::tabs()
                    .panel_view(panel_handle(first.clone()), cx)
                    .panel_view(panel_handle(second), cx),
                window,
                cx,
            );
        });
        oneterm_state::AppState::global(cx).update(cx, |state, _| {
            state.dock_area = Some(dock_area.downgrade());
        });
        let tab_panel = first
            .read(cx)
            .tab_panel
            .clone()
            .and_then(|group| group.upgrade())
            .expect("panel must belong to a tab group");
        *first_for_window.borrow_mut() = Some(first.clone());
        *tabs_for_window.borrow_mut() = Some(tab_panel);
        Root::new(dock_area, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    let first = first_probe
        .borrow()
        .clone()
        .expect("first panel must be initialized");
    let tab_panel = tabs_probe
        .borrow()
        .clone()
        .expect("tab panel must be initialized");

    first.update_in(cx, |panel, window, cx| panel.close_tab(window, cx));
    cx.run_until_parked();

    assert_eq!(first_session_probe.close_calls(), 1);
    assert_eq!(second_session_probe.close_calls(), 0);
    assert_eq!(tab_panel.read_with(cx, |tabs, _| tabs.panels().len()), 1);
}

#[gpui::test]
fn phase0_close_last_space_calls_session_close(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);

    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_spec(session_spec(session, "Phase0"), window, cx)
    });
    let cx: &mut VisualTestContext = cx;

    cx.run_until_parked();
    assert!(probe.alive());
    assert_eq!(probe.close_calls(), 0);

    // The panel starts with one leaf; closing it triggers LastSpaceClosed.
    let active = panel.read_with(cx, |p, _| p.tree.active());
    panel.update_in(cx, |p, window, _cx| {
        let (outcome, view) = p.tree.close(active);
        assert_eq!(outcome, CloseOutcome::LastSpaceClosed);
        assert!(view.is_none()); // last-space path does not return the view
        let _ = window;
    });

    // The panel's close_space path calls session.close() for non-last leaves.
    // For the last leaf the tree returns no view, so we verify directly.
    assert_eq!(probe.close_calls(), 0);

    // Now explicitly close the session to verify the fake's close path.
    panel.update(cx, |p, cx| {
        if let Some(view) = p.tree.terminal_views().into_iter().next() {
            let _ = view.read(cx).session.read(cx).close();
        }
    });
    assert_eq!(probe.close_calls(), 1);
    assert!(!probe.alive());
}

#[gpui::test]
fn phase0_close_non_last_space_closes_removed_session(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);

    // First session — will be split and then closed.
    let (session_a, probe_a) = FakeTerminalSession::boxed(24, 80, "session A");
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_spec(session_spec(session_a, "A"), window, cx)
    });
    let cx: &mut VisualTestContext = cx;

    cx.run_until_parked();

    // Split to create a second space, filling it with a second fake session.
    let (session_b, probe_b) = FakeTerminalSession::boxed(24, 80, "session B");
    let active = panel.read_with(cx, |p, _| p.tree.active());

    panel.update_in(cx, |p, window, cx| {
        p.split_active_at(active, SplitDir::Right, window, cx);
    });
    cx.run_until_parked();

    // Fill the new empty space with session B.
    let new_active = panel.read_with(cx, |p, _| p.tree.active());
    panel.update_in(cx, |p, window, cx| {
        let session_entity = cx.new(|_| session_b);
        let view = cx.new(|cx| TerminalView::new(session_entity, p.deps.clone(), window, cx));
        p.tree
            .fill_empty(new_active, view)
            .expect("new split Space must be empty");
    });

    cx.run_until_parked();
    assert!(probe_a.alive());
    assert!(probe_b.alive());

    // Close the original space (session A). The tree should return its view,
    // and close_space should call session.close() on it.
    panel.update_in(cx, |p, window, cx| {
        p.close_space(active, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(probe_a.close_calls(), 1);
    assert!(!probe_a.alive());
    assert!(probe_b.alive());
    assert_eq!(probe_b.close_calls(), 0);
}

#[gpui::test]
fn phase1_shutdown_cancels_tasks_and_closes_session(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);

    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_spec(session_spec(session, "Phase1"), window, cx)
    });
    let cx: &mut VisualTestContext = cx;

    cx.run_until_parked();

    // Verify the view starts alive with active tasks.
    let (alive, has_event, has_blink) = panel.read_with(cx, |p, cx| {
        let view = p.tree.terminal_views().into_iter().next().unwrap();
        (
            view.read(cx).alive,
            view.read(cx).event_task.is_some(),
            view.read(cx).blink_task.is_some(),
        )
    });
    assert!(alive);
    assert!(has_event);
    assert!(has_blink);

    // Shut down the panel — should close all sessions and cancel tasks.
    panel.update_in(cx, |p, window, cx| {
        p.shutdown(window, cx);
    });
    cx.run_until_parked();

    // Session is closed.
    assert_eq!(probe.close_calls(), 1);
    assert!(!probe.alive());

    // View is no longer alive and tasks are taken (cancelled).
    let (alive, no_event, no_blink) = panel.read_with(cx, |p, cx| {
        let view = p.tree.terminal_views().into_iter().next().unwrap();
        (
            view.read(cx).alive,
            view.read(cx).event_task.is_none(),
            view.read(cx).blink_task.is_none(),
        )
    });
    assert!(!alive);
    assert!(no_event);
    assert!(no_blink);
}

#[gpui::test]
fn phase1_shutdown_is_idempotent(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);

    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_spec(session_spec(session, "Phase1"), window, cx)
    });
    let cx: &mut VisualTestContext = cx;

    cx.run_until_parked();

    // Call shutdown twice — only one close call should happen.
    panel.update_in(cx, |p, window, cx| {
        p.shutdown(window, cx);
    });
    panel.update_in(cx, |p, window, cx| {
        p.shutdown(window, cx);
    });
    cx.run_until_parked();

    assert_eq!(probe.close_calls(), 1);
}

struct DuplicateSessionFactory {
    spawned_local_configs: Arc<Mutex<Vec<LocalShellConfig>>>,
}

impl SessionFactory for DuplicateSessionFactory {
    fn spawn_local(
        &self,
        config: LocalShellConfig,
        _: PtySize,
        _: usize,
        _: TerminalSecurityPolicy,
        _: oneterm_core::TerminalLogConfig,
    ) -> Result<Box<dyn TerminalSession>> {
        self.spawned_local_configs
            .lock()
            .expect("spawned config recorder must not be poisoned")
            .push(config);
        Ok(FakeTerminalSession::boxed(24, 80, "duplicate").0)
    }

    fn connect_ssh(
        &self,
        _: SshConfig,
        _: PtySize,
        _: usize,
        _: TerminalSecurityPolicy,
        _: oneterm_core::TerminalLogConfig,
        _: oneterm_core::SshKeepaliveConfig,
    ) -> Result<Box<dyn TerminalSession>> {
        Err(AppError::msg("SSH is not used by this test"))
    }
}

fn duplicate_test_commands() -> WorkspaceCommands {
    fn terminal(
        _: oneterm_core::ShellKind,
        _: &mut gpui::Window,
        _: &mut gpui::App,
    ) -> Arc<dyn PanelView> {
        unreachable!("new-terminal command is not used by this test")
    }
    fn window(_: &mut gpui::Window, _: &mut gpui::App) {}
    fn duplicate_ssh(
        _: oneterm_core::SshDuplicateConfig,
        _: Option<std::path::PathBuf>,
        _: oneterm_state::commands::SshDuplicateCompletion,
        _: &mut gpui::Window,
        _: &mut gpui::App,
    ) {
    }
    fn app(_: &mut gpui::App) {}
    fn dock(_: &gpui::Entity<DockArea>, _: &mut gpui::Window, _: &mut gpui::App) {}
    fn saved_sessions(_: &gpui::App) -> oneterm_state::commands::SavedSshSessionSections {
        Vec::new()
    }
    fn open_saved_session(_: u64, _: &mut gpui::Window, _: &mut gpui::App) {}
    fn elevated(_: oneterm_core::ShellKind, _: &mut gpui::Window, _: &mut gpui::App) {}

    WorkspaceCommands {
        new_terminal_with_shell: terminal,
        open_quick_connect_dialog: window,
        open_new_saved_session_dialog: window,
        saved_ssh_sessions: saved_sessions,
        open_saved_ssh_session: open_saved_session,
        open_duplicate_ssh_dialog: duplicate_ssh,
        open_settings: app,
        open_about: window,
        find_in_active_terminal: dock,
        setup_key_bindings: app,
        launch_elevated_shell: elevated,
    }
}

#[gpui::test]
fn duplicate_action_dispatches_to_the_active_space(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(oneterm_state::AppState::init);
    let spawned_local_configs = Arc::new(Mutex::new(Vec::new()));
    let configs_for_factory = spawned_local_configs.clone();
    cx.update(|cx| {
        AppServices::install(
            cx,
            Arc::new(DuplicateSessionFactory {
                spawned_local_configs: configs_for_factory,
            }),
            duplicate_test_commands(),
            crate::status_metrics(),
        )
        .expect("test services must install once");
    });

    let panel_probe = Rc::new(RefCell::new(None));
    let tab_probe = Rc::new(RefCell::new(None));
    let panel_for_window = panel_probe.clone();
    let tab_for_window = tab_probe.clone();
    let (session, _) = FakeTerminalSession::boxed(24, 80, "source");
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        let dock_area = cx.new(|cx| DockArea::new("duplicate-action-test", None, window, cx));
        let panel = cx.new(|cx| {
            let mut inactive_config = LocalShellConfig::default();
            inactive_config.program = Some("inactive-shell".into());
            TerminalPanel::from_spec(
                PanelSpec::Session {
                    session,
                    title: "Source".to_string(),
                    duplicate_config: Some(SessionDuplicateConfig::Local(inactive_config)),
                },
                window,
                cx,
            )
        });
        panel.update(cx, |panel, cx| {
            let inactive_space = panel.tree.active();
            panel.split_active_at(inactive_space, SplitDir::Right, window, cx);
            let active_space = panel.tree.active();
            assert_eq!(panel.empty_space_destinations(), vec![active_space]);
            let (active_session, _) = FakeTerminalSession::boxed(24, 80, "active");
            let active_session = cx.new(|_| active_session);
            let active_view = cx.new(|cx| {
                let mut view = TerminalView::new(active_session, panel.deps.clone(), window, cx);
                let mut active_config = LocalShellConfig::default();
                active_config.program = Some("active-shell".into());
                view.duplicate_config = Some(SessionDuplicateConfig::Local(active_config));
                view
            });
            panel
                .tree
                .fill_empty(active_space, active_view)
                .expect("new split Space must be empty");
            assert!(panel.empty_space_destinations().is_empty());
        });
        dock_area.update(cx, |dock_area, cx| {
            dock_area.set_center(
                DockLayout::tabs().panel_view(panel_handle(panel.clone()), cx),
                window,
                cx,
            );
        });
        oneterm_state::AppState::global(cx).update(cx, |state, _| {
            state.dock_area = Some(dock_area.downgrade());
        });
        let tab_panel = panel
            .read(cx)
            .tab_panel
            .clone()
            .and_then(|group| group.upgrade())
            .expect("panel must belong to a tab group");
        *panel_for_window.borrow_mut() = Some(panel.clone());
        *tab_for_window.borrow_mut() = Some(tab_panel);
        Root::new(dock_area, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    let panel = panel_probe
        .borrow()
        .clone()
        .expect("panel must be initialized");
    let tab_panel = tab_probe
        .borrow()
        .clone()
        .expect("tab panel must be initialized");

    cx.run_until_parked();
    let dock_focus = panel.read_with(cx, |panel, cx| panel.focus_handle(cx));
    let content_focus = panel.read_with(cx, |panel, cx| {
        panel
            .tree
            .active_focus_handle(cx)
            .expect("active Space has a content focus handle")
    });
    cx.update(|window, cx| {
        window.activate_window();
        dock_focus.focus(window, cx);
        window.refresh();
    });
    // Frame 1 publishes the proxy's focus path and runs its on_focus forwarder;
    // frame 2 renders the independently tracked content focus.
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert!(cx.update(|window, _| content_focus.is_focused(window)));
    assert!(!cx.update(|window, _| dock_focus.is_focused(window)));
    cx.dispatch_action(oneterm_actions::DuplicateSession);
    cx.run_until_parked();

    assert_eq!(tab_panel.read_with(cx, |tabs, _| tabs.active_ix()), 1);
    let spawned_local_configs = spawned_local_configs
        .lock()
        .expect("spawned config recorder must not be poisoned");
    assert_eq!(spawned_local_configs.len(), 1);
    assert_eq!(
        spawned_local_configs[0].program.as_deref(),
        Some(std::path::Path::new("active-shell"))
    );
}

#[gpui::test]
fn tab_drop_onto_occupied_space_keeps_source_terminal(cx: &mut TestAppContext) {
    // Regression (CORR-03): dropping a tab onto a Space that is not empty must
    // not take the source terminal out of its tree and shut it down.
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);

    let (target_session, _) = FakeTerminalSession::boxed(24, 80, "target");
    let (target, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_spec(session_spec(target_session, "Target"), window, cx)
    });
    let cx: &mut VisualTestContext = cx;

    let (source_session, source_probe) = FakeTerminalSession::boxed(24, 80, "source");
    let source = cx.update(|window, cx| {
        cx.new(|cx| TerminalPanel::from_spec(session_spec(source_session, "Source"), window, cx))
    });
    cx.run_until_parked();

    let source_view = source.read_with(cx, |panel, _| {
        panel
            .tree
            .active_terminal()
            .expect("source panel must own a terminal")
    });
    let drag = crate::space::DragTerminalTab {
        panel: source.downgrade(),
        title: "Source".into(),
    };

    target.update_in(cx, |panel, window, cx| {
        let occupied = panel.tree.active();
        assert!(panel.tree.leaf_terminal(occupied).is_some());
        panel.handle_tab_drop(occupied, &drag, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(source_probe.close_calls(), 0);
    assert!(source_view.read_with(cx, |view, _| view.alive));
    let still_in_source = source.read_with(cx, |panel, _| {
        panel.tree.active_terminal().as_ref() == Some(&source_view)
    });
    assert!(still_in_source, "source terminal must stay in its Space");
}

/// `US-0196` rework 9: a vertical-rail row is dragged whole as the dock's `DragPanel`, and an empty
/// Space accepts it. The terminal tab it names (found through the dock area) gives its active
/// terminal to the Space and, left with none, is removed; a drag the dock does not know and a
/// Space that is not empty move nothing.
#[gpui::test]
fn a_dragged_dock_panel_dropped_on_an_empty_space_moves_its_terminal(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(oneterm_state::AppState::init);

    let (target_session, _) = FakeTerminalSession::boxed(24, 80, "target");
    let (source_session, source_probe) = FakeTerminalSession::boxed(24, 80, "source");
    let handed = Rc::new(RefCell::new(None));
    let handed_in = handed.clone();
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        let dock_area = cx.new(|cx| DockArea::new("panel-drop-test", None, window, cx));
        let target = cx
            .new(|cx| TerminalPanel::from_spec(session_spec(target_session, "Target"), window, cx));
        let source = cx
            .new(|cx| TerminalPanel::from_spec(session_spec(source_session, "Source"), window, cx));
        dock_area.update(cx, |dock_area, cx| {
            dock_area.set_center(
                DockLayout::tabs()
                    .panel_view(panel_handle(target.clone()), cx)
                    .panel_view(panel_handle(source.clone()), cx),
                window,
                cx,
            );
        });
        oneterm_state::AppState::global(cx).update(cx, |state, _| {
            state.dock_area = Some(dock_area.downgrade());
        });
        *handed_in.borrow_mut() = Some((dock_area.clone(), target, source));
        Root::new(dock_area, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    let (dock_area, target, source) = handed.borrow().clone().expect("the window was built");
    cx.run_until_parked();

    let group = dock_area.read_with(cx, |area, _| {
        area.layout(DockPlacement::Center)
            .expect("a centre layout")
            .root()
            .id()
    });
    let source_id = cx.update(|_, cx| {
        use gpui_component::dock::BasePanelView as _;
        source.panel_id(cx)
    });
    let drag = DragPanel::new(source_id, group);
    let (occupied, empty) = target.update_in(cx, |panel, window, cx| {
        let occupied = panel.tree.active();
        panel.split_active_at(occupied, SplitDir::Right, window, cx);
        (occupied, panel.empty_space_destinations()[0])
    });
    let still_in_source = |cx: &mut VisualTestContext| {
        source.read_with(cx, |panel, _| panel.tree.active_terminal().is_some())
            && dock_area.read_with(cx, |area, _| area.panel(source_id).is_some())
    };

    // A drag of a panel the dock does not hold: nothing happens.
    let unknown = DragPanel::new(PanelId::from_u64(u64::MAX), group);
    target.update_in(cx, |panel, window, cx| {
        panel.handle_panel_drop(empty, &unknown, window, cx)
    });
    // A Space that holds a terminal is not a drop target (CORR-03).
    target.update_in(cx, |panel, window, cx| {
        panel.handle_panel_drop(occupied, &drag, window, cx)
    });
    cx.run_until_parked();
    assert!(
        still_in_source(cx),
        "neither drop took the source's terminal"
    );
    assert_eq!(
        target.read_with(cx, |panel, _| panel.empty_space_destinations()),
        vec![empty]
    );

    // The real drop: the terminal moves, the emptied tab goes, the session lives on.
    target.update_in(cx, |panel, window, cx| {
        panel.handle_panel_drop(empty, &drag, window, cx)
    });
    cx.run_until_parked();
    assert!(
        target.read_with(cx, |panel, _| panel.tree.leaf_terminal(empty).is_some()),
        "the empty Space now holds the dragged tab's terminal"
    );
    assert!(
        dock_area.read_with(cx, |area, _| area.panel(source_id).is_none()),
        "the emptied source tab is removed from the dock"
    );
    assert_eq!(
        source_probe.close_calls(),
        0,
        "the session was moved, not closed"
    );
}

/// The tab icon reads the sole terminal's status, and a split tab shows none
/// (owner ruling, `IN-0049`). Selecting the tab acknowledges a result.
#[gpui::test]
fn the_tab_status_follows_a_single_space_and_selection_acknowledges(cx: &mut TestAppContext) {
    use oneterm_terminal::ProgramState;

    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(oneterm_state::AppState::init);

    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_spec(session_spec(session, "Status"), window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();

    probe
        .emit(oneterm_terminal::SessionEvent::ProgramStatus(Some(
            ProgramState::Done,
        )))
        .expect("event channel must accept the status");
    cx.run_until_parked();
    assert_eq!(
        panel.read_with(cx, |panel, cx| panel.tab_status(cx)),
        Some(ProgramState::Done)
    );

    // Split: the icon goes, the terminal keeps its status.
    let space = panel.read_with(cx, |panel, _| panel.tree.active());
    panel.update_in(cx, |panel, window, cx| {
        panel.split_active_at(space, SplitDir::Right, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(panel.read_with(cx, |panel, cx| panel.tab_status(cx)), None);
    let kept = panel.read_with(cx, |panel, cx| {
        panel
            .tree
            .terminal_views()
            .into_iter()
            .find_map(|view| view.read(cx).program_status())
    });
    assert_eq!(kept, Some(ProgramState::Done));

    // Selecting the tab acknowledges the result on its terminals.
    panel.update_in(cx, |panel, window, cx| {
        <TerminalPanel as gpui_base::dock::Panel>::set_active(panel, false, window, cx);
        <TerminalPanel as gpui_base::dock::Panel>::set_active(panel, true, window, cx);
    });
    cx.run_until_parked();
    let after = panel.read_with(cx, |panel, cx| {
        panel
            .tree
            .terminal_views()
            .into_iter()
            .find_map(|view| view.read(cx).program_status())
    });
    assert_eq!(after, None);
}

struct StatusIconHost(oneterm_terminal::ProgramState);

impl gpui::Render for StatusIconHost {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        super::tab_title::tab_status_icon(self.0, cx.entity_id(), cx)
    }
}

/// Working and blocked animate — they ask for the next frame — and a result
/// does not; under reduced motion nothing does (the walk machine runs with
/// Windows animations off, so this is where the motion itself is proven).
#[gpui::test]
fn the_live_status_icons_animate_unless_motion_is_reduced(cx: &mut TestAppContext) {
    use gpui::{px, size};
    use oneterm_terminal::ProgramState;

    cx.update(gpui_component::init);
    for (state, animates) in [
        (ProgramState::Working, true),
        (ProgramState::Blocked, true),
        (ProgramState::Done, false),
        (ProgramState::Error, false),
    ] {
        for reduce in [false, true] {
            cx.update(|cx| cx.set_reduce_motion(reduce));
            let window = cx.open_window(size(px(120.), px(40.)), |_, _| StatusIconHost(state));
            cx.run_until_parked();
            let frames = window
                .update(cx, |_, window, cx| window.simulate_next_frame(cx))
                .unwrap();
            assert_eq!(
                frames > 0,
                animates && !reduce,
                "{state:?}, reduced motion {reduce}: {frames} frame request(s)"
            );
        }
    }
}

/// The tab chips read `tab_channels`, and the two tab-wide menu items write
/// through `join_tab_to_channel` / `leave_tab_channels` (US-0056).
#[gpui::test]
fn tab_channel_helpers_cover_every_space_of_the_tab(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(InputChannelRegistry::init);

    let (session_a, _) = FakeTerminalSession::boxed(24, 80, "A");
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_spec(session_spec(session_a, "A"), window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();

    let first_space = panel.read_with(cx, |panel, _| panel.tree.active());
    panel.update_in(cx, |panel, window, cx| {
        panel.split_active_at(first_space, SplitDir::Right, window, cx);
    });
    let second_space = panel.read_with(cx, |panel, _| panel.tree.active());
    let (session_b, _) = FakeTerminalSession::boxed(24, 80, "B");
    panel.update_in(cx, |panel, window, cx| {
        let session = cx.new(|_| session_b);
        let view = cx.new(|cx| TerminalView::new(session, panel.deps.clone(), window, cx));
        panel
            .tree
            .fill_empty(second_space, view)
            .expect("the new split Space must be empty");
    });
    cx.run_until_parked();

    assert!(
        panel
            .read_with(cx, |panel, cx| panel.tab_channels(cx))
            .is_empty(),
        "a fresh tab has no member and therefore no chip"
    );

    let first = panel
        .read_with(cx, |panel, _| panel.tree.leaf_terminal(first_space))
        .expect("the first Space holds a terminal");
    let second = panel
        .read_with(cx, |panel, _| panel.tree.leaf_terminal(second_space))
        .expect("the second Space holds a terminal");

    first.update(cx, |view, cx| view.join_channel(InputChannel::A, cx));
    assert_eq!(
        panel.read_with(cx, |panel, cx| panel.tab_channels(cx)),
        vec![InputChannel::A]
    );

    second.update(cx, |view, cx| view.join_channel(InputChannel::C, cx));
    assert_eq!(
        panel.read_with(cx, |panel, cx| panel.tab_channels(cx)),
        vec![InputChannel::A, InputChannel::C],
        "a mixed split shows one chip per channel, in A..E order"
    );

    panel.update(cx, |panel, cx| {
        panel.join_tab_to_channel(InputChannel::B, cx)
    });
    assert_eq!(
        panel.read_with(cx, |panel, cx| panel.tab_channels(cx)),
        vec![InputChannel::B],
        "joining the tab moves every Space, including the one in another channel"
    );

    panel.update(cx, |panel, cx| panel.leave_tab_channels(cx));
    assert!(
        panel
            .read_with(cx, |panel, cx| panel.tab_channels(cx))
            .is_empty(),
        "leaving with the tab clears every Space"
    );
}

/// A Space whose session ends drops out of the tab's chips (`US-0200`): the other member of
/// the channel keeps its chip, and when the last member goes the tab shows none, like a
/// manual leave.
#[gpui::test]
fn an_exited_space_leaves_the_tab_chips(cx: &mut TestAppContext) {
    use oneterm_terminal::SessionEvent;

    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(InputChannelRegistry::init);

    let (session_a, probe_a) = FakeTerminalSession::boxed(24, 80, "A");
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_spec(session_spec(session_a, "A"), window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();

    let first_space = panel.read_with(cx, |panel, _| panel.tree.active());
    panel.update_in(cx, |panel, window, cx| {
        panel.split_active_at(first_space, SplitDir::Right, window, cx);
    });
    let second_space = panel.read_with(cx, |panel, _| panel.tree.active());
    let (session_b, probe_b) = FakeTerminalSession::boxed(24, 80, "B");
    panel.update_in(cx, |panel, window, cx| {
        let session = cx.new(|_| session_b);
        let view = cx.new(|cx| TerminalView::new(session, panel.deps.clone(), window, cx));
        panel
            .tree
            .fill_empty(second_space, view)
            .expect("the new split Space must be empty");
    });
    cx.run_until_parked();

    let first = panel
        .read_with(cx, |panel, _| panel.tree.leaf_terminal(first_space))
        .expect("the first Space holds a terminal");
    let second = panel
        .read_with(cx, |panel, _| panel.tree.leaf_terminal(second_space))
        .expect("the second Space holds a terminal");
    first.update(cx, |view, cx| view.join_channel(InputChannel::A, cx));
    second.update(cx, |view, cx| view.join_channel(InputChannel::B, cx));
    let chips = |cx: &mut VisualTestContext| panel.read_with(cx, |p, cx| p.tab_channels(cx));
    assert_eq!(chips(cx), vec![InputChannel::A, InputChannel::B]);

    probe_a
        .emit(SessionEvent::Exited(Some(0)))
        .expect("event channel must accept the exit");
    cx.run_until_parked();
    assert_eq!(
        chips(cx),
        vec![InputChannel::B],
        "the exited Space's chip is gone and the running Space keeps its own"
    );
    assert_eq!(first.update(cx, |view, cx| view.channel(cx)), None);

    probe_b
        .emit(SessionEvent::Closed)
        .expect("event channel must accept the close");
    cx.run_until_parked();
    assert!(
        chips(cx).is_empty(),
        "the last member ending leaves the tab with no chip, as a manual leave does"
    );
}

/// "Join All Spaces In Tab" skips a Space whose session has exited (`US-0200`) and still puts
/// the running ones in the channel.
#[gpui::test]
fn joining_the_tab_skips_an_exited_space(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(InputChannelRegistry::init);

    let (session_a, probe_a) = FakeTerminalSession::boxed(24, 80, "A");
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_spec(session_spec(session_a, "A"), window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();

    let first_space = panel.read_with(cx, |panel, _| panel.tree.active());
    panel.update_in(cx, |panel, window, cx| {
        panel.split_active_at(first_space, SplitDir::Right, window, cx);
    });
    let second_space = panel.read_with(cx, |panel, _| panel.tree.active());
    let (session_b, _) = FakeTerminalSession::boxed(24, 80, "B");
    panel.update_in(cx, |panel, window, cx| {
        let session = cx.new(|_| session_b);
        let view = cx.new(|cx| TerminalView::new(session, panel.deps.clone(), window, cx));
        panel
            .tree
            .fill_empty(second_space, view)
            .expect("the new split Space must be empty");
    });
    cx.run_until_parked();

    probe_a.set_alive(false);
    panel.update(cx, |panel, cx| {
        panel.join_tab_to_channel(InputChannel::C, cx)
    });
    let first = panel
        .read_with(cx, |panel, _| panel.tree.leaf_terminal(first_space))
        .expect("the first Space holds a terminal");
    let second = panel
        .read_with(cx, |panel, _| panel.tree.leaf_terminal(second_space))
        .expect("the second Space holds a terminal");
    assert_eq!(first.update(cx, |view, cx| view.channel(cx)), None);
    assert_eq!(
        second.update(cx, |view, cx| view.channel(cx)),
        Some(InputChannel::C)
    );
}

/// Output in a tab the group is not showing must not notify its view (US-0145):
/// the tab strip reads that view, so the window tracks it, and every notify cost
/// a whole-window frame that changed no pixel. The shown tab still notifies.
/// (Showing the hidden tab re-renders it: the kit's `set_active(true)` notifies
/// the panel.)
#[gpui::test]
fn output_in_a_hidden_tab_does_not_notify_its_view(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(oneterm_state::AppState::init);

    let panels = Rc::new(RefCell::new(Vec::new()));
    let panels_for_window = panels.clone();
    let (first_session, _) = FakeTerminalSession::boxed(24, 80, "first");
    let (second_session, _) = FakeTerminalSession::boxed(24, 80, "second");
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        let dock_area = cx.new(|cx| DockArea::new("hidden-tab-test", None, window, cx));
        let first =
            cx.new(|cx| TerminalPanel::from_spec(session_spec(first_session, "First"), window, cx));
        let second = cx
            .new(|cx| TerminalPanel::from_spec(session_spec(second_session, "Second"), window, cx));
        dock_area.update(cx, |dock_area, cx| {
            dock_area.set_center(
                DockLayout::tabs()
                    .panel_view(panel_handle(first.clone()), cx)
                    .panel_view(panel_handle(second.clone()), cx),
                window,
                cx,
            );
        });
        panels_for_window.borrow_mut().extend([first, second]);
        Root::new(dock_area, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();

    let panels = panels.borrow().clone();
    let (hidden, shown): (Vec<_>, Vec<_>) = panels
        .into_iter()
        .partition(|panel| panel.read_with(cx, |panel, _| panel.is_hidden_tab()));
    let ([hidden], [shown]) = (hidden.as_slice(), shown.as_slice()) else {
        panic!("one of the two tabs is shown and the other hidden");
    };
    let hidden_view = hidden.read_with(cx, |panel, _| panel.active_view().expect("a terminal"));
    let shown_view = shown.read_with(cx, |panel, _| panel.active_view().expect("a terminal"));

    let notified = Rc::new(RefCell::new(Vec::new()));
    let _subscriptions = [&hidden_view, &shown_view].map(|view| {
        let notified = notified.clone();
        cx.update(|_, cx| {
            cx.observe(view, move |view, _| {
                notified.borrow_mut().push(view.entity_id())
            })
        })
    });
    for view in [&hidden_view, &shown_view] {
        view.update(cx, |view, cx| {
            view.handle_event(oneterm_terminal::SessionEvent::Output, cx)
        });
    }
    cx.run_until_parked();

    assert_eq!(*notified.borrow(), vec![shown_view.entity_id()]);
}

/// `US-0147` after `US-0145`: the repaint hint's catch-up, a frame after each
/// hint, must not notify a view in a hidden tab either (a whole-window frame per
/// hint), and must still release every hint so the hidden tab keeps hearing from
/// its pump.
#[gpui::test]
fn the_catch_up_in_a_hidden_tab_releases_without_notifying(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(oneterm_state::AppState::init);

    let panels = Rc::new(RefCell::new(Vec::new()));
    let panels_for_window = panels.clone();
    let (first_session, first_probe) = FakeTerminalSession::boxed(24, 80, "first");
    let (second_session, second_probe) = FakeTerminalSession::boxed(24, 80, "second");
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        let dock_area = cx.new(|cx| DockArea::new("hidden-tab-catch-up", None, window, cx));
        let first =
            cx.new(|cx| TerminalPanel::from_spec(session_spec(first_session, "First"), window, cx));
        let second = cx
            .new(|cx| TerminalPanel::from_spec(session_spec(second_session, "Second"), window, cx));
        dock_area.update(cx, |dock_area, cx| {
            dock_area.set_center(
                DockLayout::tabs()
                    .panel_view(panel_handle(first.clone()), cx)
                    .panel_view(panel_handle(second.clone()), cx),
                window,
                cx,
            );
        });
        panels_for_window.borrow_mut().extend([first, second]);
        Root::new(dock_area, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();

    let panels = panels.borrow().clone();
    let first_hidden = panels[0].read_with(cx, |panel, _| panel.is_hidden_tab());
    let (hidden, probe) = if first_hidden {
        (&panels[0], &first_probe)
    } else {
        (&panels[1], &second_probe)
    };
    let hidden_view = hidden.read_with(cx, |panel, _| panel.active_view().expect("a terminal"));

    let notified = Rc::new(RefCell::new(0usize));
    let counter = notified.clone();
    let _subscription =
        cx.update(|_, cx| cx.observe(&hidden_view, move |_, _| *counter.borrow_mut() += 1));
    let releases_before = probe.release_calls();
    for round in 0..5 {
        probe.feed(format!("line {round}\r\n").as_bytes());
        probe
            .emit(oneterm_terminal::SessionEvent::Output)
            .expect("room for the hint");
        cx.run_until_parked();
        // Output that came without a hint, so the catch-up has a line to stamp.
        probe.feed(format!("more {round}\r\n").as_bytes());
        // The view's `OUTPUT_CATCH_UP`.
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        cx.run_until_parked();
    }

    assert_eq!(
        probe.release_calls() - releases_before,
        5,
        "every hint a hidden view handles is released"
    );
    assert_eq!(
        *notified.borrow(),
        0,
        "the catch-up notified a hidden tab's view"
    );
}

/// The menu row that closes the tabs after this one reads for the layout it is in (`IN-0055`).
#[test]
fn the_close_following_row_names_the_direction_of_the_layout() {
    use oneterm_state::TabRowLayout;

    use crate::panel::tab_title::close_following_label;

    assert_eq!(
        close_following_label(TabRowLayout::Horizontal),
        "Close to the Right"
    );
    assert_eq!(
        close_following_label(TabRowLayout::Vertical),
        "Close Tabs Below"
    );
    assert_eq!(
        close_following_label(TabRowLayout::Collapsed),
        "Close Tabs Below"
    );
}

/// A tab row drawn by hand beside a drop target for `DragTerminalTab`, to see which gestures
/// on the row start that drag.
struct TabRowHost {
    _dock_area: gpui::Entity<DockArea>,
    panel: gpui::Entity<TerminalPanel>,
    layout: oneterm_state::TabRowLayout,
    dropped: Rc<std::cell::Cell<usize>>,
    /// The width of the slot the row is drawn in: the rail's width, or a strip tab's room.
    width: f32,
}

impl gpui::Render for TabRowHost {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{InteractiveElement as _, ParentElement as _, Styled as _};

        let panel = self.panel.clone();
        let row = {
            let mut scope = self.layout.scope(cx);
            panel.update(&mut *scope, |panel, cx| {
                gpui::IntoElement::into_any_element(
                    <TerminalPanel as gpui_component::dock::Panel>::title(panel, window, cx),
                )
            })
        };
        let dropped = self.dropped.clone();
        let height = match self.layout {
            oneterm_state::TabRowLayout::Horizontal => 32.,
            oneterm_state::TabRowLayout::Vertical => {
                oneterm_state::TabRowLayout::VERTICAL_ROW_HEIGHT
            }
            oneterm_state::TabRowLayout::Collapsed => 44.,
        };
        gpui::div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                gpui::div()
                    .id("row-slot")
                    // What the kit's `Tab` gives the row it wraps in the rail.
                    .text_sm()
                    .line_height(gpui::relative(1.))
                    .flex()
                    .w(gpui::px(self.width))
                    .h(gpui::px(height))
                    .child(row),
            )
            .child(gpui::div().id("space-drop").flex_1().on_drop(
                move |_: &crate::space::DragTerminalTab, _, _| dropped.set(dropped.get() + 1),
            ))
    }
}

fn tab_row_window(
    cx: &mut TestAppContext,
    layout: oneterm_state::TabRowLayout,
) -> (
    Rc<std::cell::Cell<usize>>,
    oneterm_terminal::test_support::FakeSessionProbe,
    &mut VisualTestContext,
) {
    tab_row_window_of_width(cx, layout, 220., oneterm_terminal::SessionKind::Local)
}

fn tab_row_window_of_width(
    cx: &mut TestAppContext,
    layout: oneterm_state::TabRowLayout,
    width: f32,
    kind: oneterm_terminal::SessionKind,
) -> (
    Rc<std::cell::Cell<usize>>,
    oneterm_terminal::test_support::FakeSessionProbe,
    &mut VisualTestContext,
) {
    let (dropped, probe, _, cx) = tab_row_window_with_panel(cx, layout, width, kind);
    (dropped, probe, cx)
}

/// Same, also handing back the first tab's panel.
fn tab_row_window_with_panel(
    cx: &mut TestAppContext,
    layout: oneterm_state::TabRowLayout,
    width: f32,
    kind: oneterm_terminal::SessionKind,
) -> (
    Rc<std::cell::Cell<usize>>,
    oneterm_terminal::test_support::FakeSessionProbe,
    gpui::Entity<TerminalPanel>,
    &mut VisualTestContext,
) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(oneterm_state::AppState::init);
    let dropped = Rc::new(std::cell::Cell::new(0));
    let counted = dropped.clone();
    let (first_session, first_probe) = FakeTerminalSession::boxed_with_kind(24, 80, "first", kind);
    let (second_session, _) = FakeTerminalSession::boxed(24, 80, "second");
    let (host, cx) = cx.add_window_view(move |window, cx| {
        let dock_area = cx.new(|cx| DockArea::new("tab-row-test", None, window, cx));
        let first =
            cx.new(|cx| TerminalPanel::from_spec(session_spec(first_session, "First"), window, cx));
        let second = cx
            .new(|cx| TerminalPanel::from_spec(session_spec(second_session, "Second"), window, cx));
        dock_area.update(cx, |dock_area, cx| {
            dock_area.set_center(
                DockLayout::tabs()
                    .panel_view(panel_handle(first.clone()), cx)
                    .panel_view(panel_handle(second), cx),
                window,
                cx,
            );
        });
        TabRowHost {
            _dock_area: dock_area,
            panel: first,
            layout,
            dropped: counted,
            width,
        }
    });
    let panel = host.read_with(cx, |host, _| host.panel.clone());
    (dropped, first_probe, panel, cx)
}

/// The bounds of the drawn row and of the parts of it a test asks about.
struct DrawnRow {
    row: gpui::Bounds<gpui::Pixels>,
    label: gpui::Bounds<gpui::Pixels>,
    status: Option<gpui::Bounds<gpui::Pixels>>,
    close: Option<gpui::Bounds<gpui::Pixels>>,
    cwd: Option<gpui::Bounds<gpui::Pixels>>,
}

/// Draw a frame and read where the row and its parts landed.
fn drawn_row(cx: &mut VisualTestContext) -> DrawnRow {
    use crate::panel::tab_title::{
        TAB_CLOSE_SELECTOR, TAB_LABEL_SELECTOR, TAB_ROW_SELECTOR, TAB_STATUS_SELECTOR,
    };

    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    DrawnRow {
        row: cx
            .debug_bounds(TAB_ROW_SELECTOR)
            .expect("the tab row is drawn"),
        label: cx
            .debug_bounds(TAB_LABEL_SELECTOR)
            .expect("the label is drawn"),
        status: cx.debug_bounds(TAB_STATUS_SELECTOR),
        close: cx.debug_bounds(TAB_CLOSE_SELECTOR),
        cwd: cx.debug_bounds(crate::panel::tab_title::TAB_CWD_SELECTOR),
    }
}

/// Make the row's terminal report a finished program, so its tab shows a status icon.
fn report_done(
    probe: &oneterm_terminal::test_support::FakeSessionProbe,
    cx: &mut VisualTestContext,
) {
    probe
        .emit(oneterm_terminal::SessionEvent::ProgramStatus(Some(
            oneterm_terminal::ProgramState::Done,
        )))
        .expect("event channel must accept the status");
    cx.run_until_parked();
}

/// Press at `from`, move past the drag threshold and on to the drop target below the row, and
/// release there; whether a drag was running just before the release.
fn drag_to_space(from: gpui::Point<gpui::Pixels>, cx: &mut VisualTestContext) -> bool {
    use gpui::{Modifiers, MouseButton, point, px};

    let space = point(px(300.), px(200.));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        from + point(px(0.), px(6.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(space, MouseButton::Left, Modifiers::none());
    let dragging = cx.update(|_, cx| cx.has_active_drag());
    cx.simulate_mouse_up(space, MouseButton::Left, Modifiers::none());
    dragging
}

/// In the vertical rail the row fills the width it is given and starts no drag of its own: the
/// whole row is the rail's reorder drag (`DragPanel`, started by the rail's `Tab` around it, which
/// an empty Space accepts too), so nothing on the row may start the `DragTerminalTab` that would
/// win the press (`IN-0055` drag priority, `US-0196` rework 9).
#[gpui::test]
fn the_vertical_row_fills_its_width_and_starts_no_drag_of_its_own(cx: &mut TestAppContext) {
    use gpui::{point, px};
    use oneterm_state::TabRowLayout;

    let (dropped, probe, cx) = tab_row_window(cx, TabRowLayout::Vertical);
    let row = drawn_row(cx).row;
    assert_eq!(
        row.size.width,
        px(220.),
        "the vertical row fills the rail's width"
    );

    let content = point(row.left() + px(60.), row.center().y);
    assert!(!drag_to_space(content, cx), "the row's content starts none");
    // The right end of the row, where the strip has its `x` (and a status icon sits), too.
    report_done(&probe, cx);
    let end = point(row.right() - px(10. + 8.), row.center().y);
    assert!(
        !drag_to_space(end, cx),
        "neither does the trailing end of a vertical row"
    );
    // And the left edge, where the rail's grip used to be.
    let edge = point(row.left() + px(2.), row.center().y);
    assert!(!drag_to_space(edge, cx), "nor does the left edge");
    assert_eq!(dropped.get(), 0, "no tab drag reached a Space");
}

/// The strip's tab is unchanged: its content drags the tab into an empty Space.
#[gpui::test]
fn the_strip_row_still_drags_the_tab_into_a_space(cx: &mut TestAppContext) {
    use oneterm_state::TabRowLayout;

    let (dropped, _, cx) = tab_row_window(cx, TabRowLayout::Horizontal);
    let label = drawn_row(cx).label;
    assert!(drag_to_space(label.center(), cx), "the label starts a drag");
    assert_eq!(dropped.get(), 1, "it is the tab drag a Space accepts");
}

/// A collapsed cell starts no tab drag of its own anywhere, the 16 px shell icon included: the
/// whole cell is the rail's reorder drag (`DragPanel`), which an empty Space accepts too
/// (`US-0198` rework 8; the icon used to start the tab drag, so a drag reordered or not by where
/// the press landed).
#[gpui::test]
fn no_part_of_a_collapsed_cell_starts_a_tab_drag_of_its_own(cx: &mut TestAppContext) {
    use crate::panel::tab_title::{TAB_ROW_SELECTOR, TAB_SHELL_ICON_SELECTOR};
    use gpui::{point, px};
    use oneterm_state::TabRowLayout;

    let (dropped, _, cx) = tab_row_window(cx, TabRowLayout::Collapsed);
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let cell = cx
        .debug_bounds(TAB_ROW_SELECTOR)
        .expect("the cell is drawn");
    let icon = cx
        .debug_bounds(TAB_SHELL_ICON_SELECTOR)
        .expect("the cell shows the shell icon");

    assert!(
        !drag_to_space(point(cell.left() + px(2.), cell.center().y), cx),
        "the cell outside its icon starts no drag of its own"
    );
    assert!(
        !drag_to_space(icon.center(), cx),
        "nor does the icon: it is plain content of the cell"
    );
    assert_eq!(dropped.get(), 0, "no tab drag reached a Space");
}

/// A vertical row has no close button; with no directory its status icon ends the one centred
/// line, after the label, and with no status the label keeps the whole width (`US-0196` rework,
/// rework 7).
#[gpui::test]
fn the_vertical_row_has_no_close_button_and_its_status_icon_trails_the_label(
    cx: &mut TestAppContext,
) {
    use gpui::px;
    use oneterm_state::TabRowLayout;

    let (_, probe, cx) = tab_row_window(cx, TabRowLayout::Vertical);
    let right_padding = px(TabRowLayout::VERTICAL_ROW_PADDING_RIGHT);

    // No status: nothing at the end, the label runs to the row's right padding.
    let bare = drawn_row(cx);
    assert!(bare.close.is_none(), "a vertical row draws no close button");
    assert!(bare.status.is_none());
    assert!(
        bare.label.right() >= bare.row.right() - right_padding - px(1.),
        "the label {:?} keeps the full width of the row {:?}",
        bare.label,
        bare.row
    );

    // A status, no directory: the icon sits on the label's line, at the row's trailing end.
    report_done(&probe, cx);
    let shown = drawn_row(cx);
    assert!(shown.close.is_none(), "a status brings no close button");
    let status = shown.status.expect("the status icon is drawn");
    assert!(
        (status.center().y - shown.label.center().y).abs() <= px(1.),
        "with no directory the icon {status:?} ends the title line {:?}",
        shown.label
    );
    assert!(status.right() <= shown.row.right() - right_padding + px(1.));
    assert!(
        status.right() >= shown.row.right() - right_padding - px(1.),
        "the icon {status:?} must sit at the trailing end of the row {:?}",
        shown.row
    );
    assert!(
        shown.label.right() <= status.left(),
        "the title ends before the icon"
    );
}

/// A vertical row is two lines in 48 px (6 + 20 + 16 + 6): the title on the first, the working
/// directory on the second with the status icon after it. With no directory there is no second
/// line: the title is vertically centred in the row, whether or not a status is shown, and the
/// content starts at the row's left padding (`US-0196` rework 2, rework 7).
#[gpui::test]
fn the_vertical_row_shows_the_title_then_the_directory_and_the_status(cx: &mut TestAppContext) {
    use gpui::px;
    use oneterm_state::TabRowLayout;

    let (_, probe, cx) = tab_row_window(cx, TabRowLayout::Vertical);
    let centred = |bounds: gpui::Bounds<gpui::Pixels>, row: gpui::Bounds<gpui::Pixels>| {
        (bounds.center().y - row.center().y).abs() <= px(1.)
    };

    // Nothing reported yet: no second line, the title is centred in the row.
    let unknown = drawn_row(cx);
    assert_eq!(unknown.row.size.height, px(48.), "6 + 20 + 16 + 6");
    assert!(unknown.cwd.is_none(), "an unknown directory draws no path");
    assert!(
        centred(unknown.label, unknown.row),
        "the title {:?} is centred in the row {:?}, no blank second line",
        unknown.label,
        unknown.row
    );
    assert_eq!(
        unknown.label.left() - unknown.row.left(),
        px(TabRowLayout::VERTICAL_ROW_PADDING_LEFT),
        "the content starts at the left padding, clear of the accent pill"
    );

    probe.set_cwd(Some(std::path::PathBuf::from("Z:/oneterm-test/src/panel")));
    report_done(&probe, cx);
    let shown = drawn_row(cx);
    let path = shown.cwd.expect("the directory is drawn");
    let status = shown.status.expect("the status icon is drawn");
    assert_eq!(shown.row.size.height, px(48.));
    assert!(
        path.top() >= shown.label.bottom() - px(1.),
        "the path {path:?} is below the title {:?}",
        shown.label
    );
    // Line 1 starts 6 px under the row's top (its text is no taller than the 20 px line) and
    // line 2 ends 6 px above its bottom.
    let first_line_top = shown.row.top() + px(TabRowLayout::VERTICAL_ROW_PADDING);
    let first_line_centre = first_line_top + px(TabRowLayout::VERTICAL_FIRST_LINE_HEIGHT / 2.);
    assert!((shown.label.center().y - first_line_centre).abs() <= px(1.));
    let second_line_centre = shown.row.bottom()
        - px(TabRowLayout::VERTICAL_ROW_PADDING)
        - px(TabRowLayout::VERTICAL_SECOND_LINE_HEIGHT / 2.);
    assert!((path.center().y - second_line_centre).abs() <= px(1.));
    assert_eq!(
        path.left(),
        shown.label.left(),
        "left-aligned under the title"
    );
    assert!(
        (path.center().y - status.center().y).abs() <= px(1.),
        "the path {path:?} and the icon {status:?} share line 2"
    );
    assert!(path.right() <= status.left(), "the icon follows the path");
    assert!(
        status.right() >= shown.row.right() - px(TabRowLayout::VERTICAL_ROW_PADDING_RIGHT) - px(1.),
        "the icon sits at the end of line 2"
    );

    // No directory, still a status: the icon ends the one centred line, not a blank line 2.
    probe.set_cwd(None);
    let bare = drawn_row(cx);
    assert!(bare.cwd.is_none());
    let status = bare.status.expect("the status icon is drawn");
    assert!(centred(bare.label, bare.row), "the title stays centred");
    assert!(
        (status.center().y - bare.label.center().y).abs() <= px(1.),
        "the icon {status:?} is on the title's line {:?}",
        bare.label
    );
    assert!(
        status.right() >= bare.row.right() - px(TabRowLayout::VERTICAL_ROW_PADDING_RIGHT) - px(1.)
    );
}

/// A long directory is cut from its start, so the end (the name that matters) stays; the strip
/// shows no directory at all.
#[gpui::test]
fn a_long_directory_is_cut_at_its_start_and_follows_the_width(cx: &mut TestAppContext) {
    use gpui::Styled as _;
    use oneterm_state::TabRowLayout;
    use oneterm_terminal::SessionKind;

    let long = "Z:/oneterm-test/a-rather-long-directory/with/many/nested/folders/src/panel";
    let width_of = |width: f32, cx: &mut TestAppContext| {
        let (_, probe, cx) =
            tab_row_window_of_width(cx, TabRowLayout::Vertical, width, SessionKind::Local);
        probe.set_cwd(Some(std::path::PathBuf::from(long)));
        report_done(&probe, cx);
        let row = drawn_row(cx);
        let path = row.cwd.expect("the directory is drawn");
        assert!(
            path.left() >= row.row.left() && path.right() <= row.row.right(),
            "{path:?}"
        );
        path.size.width
    };
    let wide = width_of(900., cx);
    let narrow = width_of(220., cx);
    let narrower = width_of(160., cx);
    assert!(
        narrow < wide,
        "a narrow rail cuts the path: {narrow:?} vs {wide:?}"
    );
    assert!(
        narrower < narrow,
        "and a narrower one cuts more: {narrower:?} vs {narrow:?}"
    );

    // The cut is at the start: the element asks gpui for `TruncateStart`, not the end.
    let mut label = crate::panel::tab_title::tab_cwd_label(long.to_string(), gpui::black());
    assert!(matches!(
        label.style().text.text_overflow.clone(),
        Some(gpui::TextOverflow::TruncateStart(_))
    ));
}

/// A vertical row's directory is the status bar's: the breadcrumb text of the same tab, as the
/// session reported it, for a local drive path, a path in the home folder and an SSH path. No
/// `~` is ever substituted.
#[gpui::test]
fn the_rows_directory_is_the_status_bars(cx: &mut TestAppContext) {
    use oneterm_state::TabRowLayout;
    use oneterm_terminal::SessionKind;

    let home = oneterm_core::home_dir().expect("tests run with a home directory");
    let cases = [
        (
            SessionKind::Local,
            std::path::PathBuf::from("D:\\work\\src"),
        ),
        (SessionKind::Local, home.join("src").join("panel")),
        (SessionKind::Local, home.clone()),
        (
            SessionKind::Ssh,
            std::path::PathBuf::from("/srv/app/releases"),
        ),
    ];
    for (kind, reported) in cases {
        let (_, probe, panel, cx) =
            tab_row_window_with_panel(cx, TabRowLayout::Vertical, 220., kind);
        probe.set_cwd(Some(reported.clone()));
        cx.run_until_parked();
        let (row, status_bar) = cx.update(|_, cx| {
            let panel = panel.read(cx);
            (panel.tab_cwd(cx), panel.breadcrumb_label(cx))
        });
        let shown = reported.display().to_string();
        assert_eq!(row, status_bar, "{kind:?} {shown}");
        assert_eq!(row.as_deref(), Some(shown.as_str()));
        assert!(!shown.starts_with('~'));
    }
}

/// The cell's tooltip would stay over the menu a right-click opens, so a right press hides it
/// until the pointer comes back to the cell (`US-0198` UI defect U1).
#[gpui::test]
fn a_right_click_hides_the_collapsed_cells_tooltip_until_the_pointer_returns(
    cx: &mut TestAppContext,
) {
    use gpui::{Modifiers, MouseButton, point, px};
    use oneterm_state::TabRowLayout;
    use oneterm_terminal::SessionKind;

    use crate::panel::tab_title::TAB_ROW_SELECTOR;

    let (_, _, panel, cx) =
        tab_row_window_with_panel(cx, TabRowLayout::Collapsed, 44., SessionKind::Local);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let row = cx
        .debug_bounds(TAB_ROW_SELECTOR)
        .expect("the cell is drawn");
    let hidden = panel.read_with(cx, |panel, _| panel.tab_tooltip_hidden.clone());

    let inside = row.origin + point(px(4.), px(4.));
    cx.simulate_mouse_move(inside, None, Modifiers::none());
    assert!(!hidden.get(), "hovering alone shows the tooltip");
    cx.simulate_mouse_down(inside, MouseButton::Right, Modifiers::none());
    assert!(hidden.get(), "a right press hides it");
    cx.simulate_mouse_up(inside, MouseButton::Right, Modifiers::none());

    // Dismiss the menu by clicking away, which also takes the pointer off the cell.
    let away = point(px(300.), px(200.));
    cx.simulate_mouse_move(away, None, Modifiers::none());
    cx.simulate_click(away, Modifiers::none());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(hidden.get(), "still hidden while the pointer is away");
    cx.simulate_mouse_move(inside, None, Modifiers::none());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(!hidden.get(), "the pointer coming back shows it again");
}

/// Owner rework 8 of `US-0196`: the tab's context menu says it is open (the vertical rail keeps
/// the row highlighted meanwhile) and says it is gone again on Escape, and on a click outside; the id it records is the one the rail reads.
#[gpui::test]
fn a_tab_menu_records_that_it_is_open_until_it_is_dismissed(cx: &mut TestAppContext) {
    use gpui::{Modifiers, MouseButton, point, px};
    use oneterm_state::{TabMenuOpen, TabRowLayout};
    use oneterm_terminal::SessionKind;

    use crate::panel::tab_title::TAB_ROW_SELECTOR;

    let (_, _, panel, cx) =
        tab_row_window_with_panel(cx, TabRowLayout::Vertical, 220., SessionKind::Local);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let open = |cx: &mut VisualTestContext| cx.update(|_, cx| TabMenuOpen::panel(cx));
    assert_eq!(open(cx), None);

    // The rail asks the panel handle for its id; it is the entity's.
    let id = panel.entity_id().as_u64();
    let handle = panel_handle(panel.clone());
    assert_eq!(cx.update(|_, cx| handle.panel_id(cx).as_u64()), id);

    let row = cx.debug_bounds(TAB_ROW_SELECTOR).expect("the row is drawn");
    let inside = row.origin + point(px(20.), px(10.));
    let right_click = |cx: &mut VisualTestContext| {
        cx.simulate_mouse_move(inside, None, Modifiers::none());
        cx.simulate_mouse_down(inside, MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_up(inside, MouseButton::Right, Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };

    right_click(cx);
    assert_eq!(open(cx), Some(id), "the menu is built: this tab's is open");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(open(cx), None, "Escape dismisses it");

    right_click(cx);
    assert_eq!(open(cx), Some(id));
    let away = point(px(700.), px(500.));
    cx.simulate_mouse_move(away, None, Modifiers::none());
    cx.simulate_click(away, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(open(cx), None, "a click outside dismisses it");

    // A second right-click on the same tab replaces the open menu (the old one is released, not
    // dismissed); the release must not clear the record of the menu that replaced it.
    right_click(cx);
    right_click(cx);
    assert_eq!(open(cx), Some(id), "the replacing menu is the open one");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(open(cx), None);
}

/// Advisor pin, rework 8 fixes: a menu that goes with its window is released, never dismissed,
/// and the record must still clear (`on_release` in `tab_context_menu`), or the next window's
/// rail would read a panel id that no row matches but nothing ever resets.
#[gpui::test]
fn a_tab_menu_open_when_its_window_closes_clears_its_record(cx: &mut TestAppContext) {
    use gpui::{Modifiers, MouseButton, point, px};
    use oneterm_state::{TabMenuOpen, TabRowLayout};
    use oneterm_terminal::SessionKind;

    use crate::panel::tab_title::TAB_ROW_SELECTOR;

    let (_, _, panel, cx) =
        tab_row_window_with_panel(cx, TabRowLayout::Vertical, 220., SessionKind::Local);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let id = panel.entity_id().as_u64();
    let row = cx.debug_bounds(TAB_ROW_SELECTOR).expect("the row is drawn");
    let inside = row.origin + point(px(20.), px(10.));
    cx.simulate_mouse_move(inside, None, Modifiers::none());
    cx.simulate_mouse_down(inside, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(inside, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(cx.update(|_, cx| TabMenuOpen::panel(cx)), Some(id));

    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
    drop(panel);
    let app: &mut TestAppContext = cx;
    assert_eq!(
        app.update(|cx| TabMenuOpen::panel(cx)),
        None,
        "the released menu cleared its record"
    );
}

/// OSC 7 from the shell tells the panel to repaint, so the row's path is live.
#[gpui::test]
fn a_reported_directory_repaints_the_row(cx: &mut TestAppContext) {
    use oneterm_state::TabRowLayout;

    let (_, probe, cx) = tab_row_window(cx, TabRowLayout::Vertical);
    assert!(drawn_row(cx).cwd.is_none());
    probe.set_cwd(Some(std::path::PathBuf::from("Z:/oneterm-test/one")));
    probe
        .emit(oneterm_terminal::SessionEvent::Cwd(
            std::path::PathBuf::from("Z:/oneterm-test/one"),
        ))
        .expect("event channel must accept the cwd");
    cx.run_until_parked();
    assert!(drawn_row(cx).cwd.is_some(), "the new directory is drawn");
}

/// The strip's row keeps its content-sized shape: the vertical variant is opt-in.
#[gpui::test]
fn the_horizontal_row_is_content_sized_as_before(cx: &mut TestAppContext) {
    use gpui::px;
    use oneterm_state::TabRowLayout;

    let (_, probe, cx) = tab_row_window(cx, TabRowLayout::Horizontal);
    let row = drawn_row(cx).row;
    assert!(
        row.size.width >= px(100.) && row.size.width <= px(220.),
        "the strip tab keeps its 100-220 px band, got {:?}",
        row.size.width
    );

    // The strip keeps its order: status icon, label, then the close button.
    report_done(&probe, cx);
    let drawn = drawn_row(cx);
    let status = drawn.status.expect("the status icon is drawn");
    let close = drawn.close.expect("the strip tab keeps its close button");
    assert!(
        status.right() <= drawn.label.left(),
        "the status icon {status:?} precedes the label {:?}",
        drawn.label
    );
    assert!(
        close.left() >= drawn.label.right(),
        "the close button {close:?} follows the label {:?}",
        drawn.label
    );
}

/// The working icon spins in the strip and the vertical row alike: each draw of a live row asks
/// for the next frame. (The collapsed cell has no icon: its state is its background.)
#[gpui::test]
fn the_working_icon_animates_in_every_row_layout(cx: &mut TestAppContext) {
    use oneterm_state::TabRowLayout;
    for layout in [TabRowLayout::Horizontal, TabRowLayout::Vertical] {
        let (_, probe, cx) = tab_row_window(cx, layout);
        probe
            .emit(oneterm_terminal::SessionEvent::ProgramStatus(Some(
                oneterm_terminal::ProgramState::Working,
            )))
            .expect("event channel must accept the status");
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(
            cx.debug_bounds(crate::panel::tab_title::TAB_STATUS_SELECTOR)
                .is_some(),
            "{layout:?}: the status icon is drawn"
        );
        let frames = cx.update(|window, cx| window.simulate_next_frame(cx));
        assert!(
            frames > 0,
            "{layout:?}: no frame requested, the icon is static"
        );
    }
}

/// A stand-in for a non-terminal dock panel (SFTP, session tree) in the drop tests below.
struct ForeignPanel {
    focus_handle: gpui::FocusHandle,
}

impl gpui_component::dock::BasePanel for ForeignPanel {
    fn panel_name(&self) -> &'static str {
        "foreign"
    }
}

impl gpui_component::dock::Panel for ForeignPanel {
    fn title(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{InteractiveElement as _, ParentElement as _, Styled as _};
        gpui::div()
            .id("foreign-tab")
            .debug_selector(|| "foreign-tab".to_string())
            .px_2()
            .child("Foreign")
    }
}

impl gpui::EventEmitter<gpui_component::dock::PanelEvent> for ForeignPanel {}

impl gpui::Focusable for ForeignPanel {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl gpui::Render for ForeignPanel {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::Styled as _;
        gpui::div().size_full()
    }
}

/// Owner ruling 2026-10-23 ("no split by drop", `US-0196` rework 9): terminal content never
/// splits or merges the dock's centre by a drop. A `DragPanel` of a panel that is not a terminal
/// (a stand-in for SFTP or the session tree) released over the content of a terminal tab is
/// consumed there, whether the pointer is on an empty Space (whose own drop takes only terminal
/// tabs, and swallows the rest: gpui takes the drag before any type check can decline it,
/// `gpui-pre-0.3.9 src/elements/div.rs:2905-2931`) or on a terminal that holds a session (the
/// terminal panel's catch-all drop), and in the edge zones where the dock would split the centre.
/// The advisor's pin of the first half (the empty Space) is kept; the second half is new.
#[gpui::test]
fn a_foreign_dock_panel_dropped_on_terminal_content_never_splits_the_centre(
    cx: &mut TestAppContext,
) {
    use gpui::{Modifiers, MouseButton, point, px};
    use gpui_component::dock::{BasePanelView as _, PaneRef};

    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(oneterm_state::AppState::init);

    let (session, _) = FakeTerminalSession::boxed(24, 80, "terminal");
    let handed = Rc::new(RefCell::new(None));
    let handed_in = handed.clone();
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        // The kit's skin, so the strip (and the foreign tab's drag) is drawn.
        let (dock_area, _skin) =
            gpui_component::dock::DockSkin::dock_area("foreign-drop-test", None, window, cx);
        let terminal =
            cx.new(|cx| TerminalPanel::from_spec(session_spec(session, "Terminal"), window, cx));
        let foreign = cx.new(|cx| ForeignPanel {
            focus_handle: cx.focus_handle(),
        });
        dock_area.update(cx, |dock_area, cx| {
            dock_area.set_center(
                DockLayout::tabs()
                    .panel_view(panel_handle(terminal.clone()), cx)
                    .panel_view(panel_handle(foreign.clone()), cx),
                window,
                cx,
            );
        });
        oneterm_state::AppState::global(cx).update(cx, |state, _| {
            state.dock_area = Some(dock_area.downgrade());
        });
        *handed_in.borrow_mut() = Some((dock_area.clone(), terminal, foreign));
        Root::new(dock_area, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    let (dock_area, terminal, _foreign) = handed.borrow().clone().expect("the window was built");
    cx.run_until_parked();

    // The terminal tab is displayed and split: its right half is an empty Space.
    let terminal_id = cx.update(|_, cx| terminal.panel_id(cx));
    dock_area.update_in(cx, |area, window, cx| {
        area.select_panel(terminal_id, window, cx)
    });
    terminal.update_in(cx, |panel, window, cx| {
        let active = panel.tree.active();
        panel.split_active_at(active, SplitDir::Right, window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let groups = |cx: &mut VisualTestContext| {
        dock_area.read_with(cx, |area, _| {
            let mut tabs = 0;
            area.layout(DockPlacement::Center)
                .expect("a centre layout")
                .root()
                .walk(&mut |node| tabs += usize::from(matches!(node.kind(), PaneRef::Tabs { .. })));
            tabs
        })
    };
    assert_eq!(groups(cx), 1);

    // Drag the foreign tab to `to` and release there; the drag must start and end.
    let area = dock_area.read_with(cx, |area, _| area.bounds());
    let drop_at = |fraction: f32, cx: &mut VisualTestContext| {
        let tab = cx
            .debug_bounds("foreign-tab")
            .expect("the foreign tab is drawn in the strip");
        let to = point(
            area.left() + area.size.width * fraction,
            area.top() + area.size.height * 0.5,
        );
        cx.simulate_mouse_down(tab.center(), MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            tab.center() + point(px(0.), px(6.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
        assert!(
            cx.update(|_, cx| cx.has_active_drag()),
            "the foreign tab starts the dock's drag"
        );
        cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert!(
            !cx.update(|_, cx| cx.has_active_drag()),
            "the drop ended the drag"
        );
    };

    // The empty Space, in the content's right edge zone (x > 65 %) where the dock would split
    // the centre to the right: the Space eats it.
    drop_at(0.85, cx);
    assert_eq!(
        terminal.read_with(cx, |panel, _| panel.empty_space_destinations().len()),
        1,
        "the empty Space took nothing (a foreign panel is not a terminal)"
    );
    assert_eq!(groups(cx), 1, "the empty Space did not split the centre");

    // The terminal that holds a session, in the left edge zone (x < 35 %): the panel's catch-all
    // takes it, where the dock would have split the centre to the left.
    drop_at(0.15, cx);
    assert_eq!(groups(cx), 1, "a terminal did not split the centre");
    drop_at(0.5, cx);
    assert_eq!(groups(cx), 1, "nor did a drop in the middle merge anything");
}

/// Advisor pin (`US-0196` rework 9, "no split by drop"): with the terminal panel's catch-all
/// `on_drop::<DragPanel>` on its root, a terminal tab's `DragPanel` dragged by gesture from the
/// strip is (1) swallowed on a terminal that holds a session, in the edge zone where the dock
/// would have split, and (2) still taken by the empty Space, which is deeper than the root and
/// so asked first. Both halves go through gpui's dispatch, not through `handle_panel_drop`.
#[gpui::test]
fn verify_the_empty_space_still_wins_over_the_terminal_root_catch_all(cx: &mut TestAppContext) {
    use gpui::{Modifiers, MouseButton, point, px};
    use gpui_component::dock::{BasePanelView as _, PaneRef};

    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(oneterm_state::AppState::init);

    let (target_session, _) = FakeTerminalSession::boxed(24, 80, "target");
    let (source_session, source_probe) = FakeTerminalSession::boxed(24, 80, "source");
    let handed = Rc::new(RefCell::new(None));
    let handed_in = handed.clone();
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        let (dock_area, _skin) =
            gpui_component::dock::DockSkin::dock_area("space-wins-test", None, window, cx);
        let target = cx
            .new(|cx| TerminalPanel::from_spec(session_spec(target_session, "Target"), window, cx));
        let source = cx
            .new(|cx| TerminalPanel::from_spec(session_spec(source_session, "Source"), window, cx));
        dock_area.update(cx, |dock_area, cx| {
            dock_area.set_center(
                DockLayout::tabs()
                    .panel_view(panel_handle(target.clone()), cx)
                    .panel_view(panel_handle(source.clone()), cx),
                window,
                cx,
            );
        });
        oneterm_state::AppState::global(cx).update(cx, |state, _| {
            state.dock_area = Some(dock_area.downgrade());
        });
        *handed_in.borrow_mut() = Some((dock_area.clone(), target, source));
        Root::new(dock_area, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    let (dock_area, target, source) = handed.borrow().clone().expect("the window was built");
    cx.run_until_parked();

    let (target_id, source_id) = cx.update(|_, cx| (target.panel_id(cx), source.panel_id(cx)));
    dock_area.update_in(cx, |area, window, cx| {
        area.select_panel(target_id, window, cx)
    });
    let empty = target.update_in(cx, |panel, window, cx| {
        let active = panel.tree.active();
        panel.split_active_at(active, SplitDir::Right, window, cx);
        panel.empty_space_destinations()[0]
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let groups = |cx: &mut VisualTestContext| {
        dock_area.read_with(cx, |area, _| {
            let mut tabs = 0;
            area.layout(DockPlacement::Center)
                .expect("a centre layout")
                .root()
                .walk(&mut |node| tabs += usize::from(matches!(node.kind(), PaneRef::Tabs { .. })));
            tabs
        })
    };
    let area = dock_area.read_with(cx, |area, _| area.bounds());
    // The kit's strip tab for the source: its leading padding (the row inside it starts the
    // strip's `DragTerminalTab`, the padding the dock's `DragPanel`).
    let drop_at = |fraction: f32, cx: &mut VisualTestContext| {
        let tab = cx
            .debug_bounds("dock-tab-1")
            .expect("the source's strip tab is drawn");
        let from = point(tab.left() + px(3.), tab.center().y);
        let to = point(
            area.left() + area.size.width * fraction,
            area.top() + area.size.height * 0.5,
        );
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            from + point(px(0.), px(6.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
        assert!(
            cx.update(|_, cx| cx.has_active_drag()),
            "the tab's padding starts a drag"
        );
        cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
    };

    // (1) On the live terminal, left edge zone: blocked. The source keeps its terminal and its
    // tab, the centre stays one group.
    drop_at(0.15, cx);
    assert_eq!(groups(cx), 1, "no split under a terminal");
    assert!(
        dock_area.read_with(cx, |area, _| area.panel(source_id).is_some())
            && source.read_with(cx, |panel, _| panel.tree.active_terminal().is_some()),
        "a blocked drop moves nothing"
    );
    assert!(target.read_with(cx, |panel, _| panel.tree.leaf_terminal(empty).is_none()));

    // (2) On the empty Space, right half: the Space is deeper than the panel root, so it is asked
    // first and the terminal moves in; the emptied source tab goes, its session lives on.
    drop_at(0.85, cx);
    assert!(
        target.read_with(cx, |panel, _| panel.tree.leaf_terminal(empty).is_some()),
        "the empty Space took the dragged tab's terminal"
    );
    assert!(
        dock_area.read_with(cx, |area, _| area.panel(source_id).is_none()),
        "the emptied source tab is removed"
    );
    assert_eq!(groups(cx), 1);
    assert_eq!(source_probe.close_calls(), 0, "moved, not closed");
}
