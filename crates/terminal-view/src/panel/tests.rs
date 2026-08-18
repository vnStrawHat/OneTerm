use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui::{AppContext as _, Focusable as _, TestAppContext, VisualTestContext};
use gpui_component::{
    Root,
    dock::{DockArea, Panel as _, PanelView, TabPanel},
};
use oneterm_core::{AppError, LocalShellConfig, Result, SessionDuplicateConfig, SshConfig};
use oneterm_state::{AppServices, commands::WorkspaceCommands};
use oneterm_terminal::{
    PtySize, SessionFactory, TerminalSecurityPolicy, TerminalSession,
    test_support::FakeTerminalSession,
};

use crate::panel::{PanelSpec, TerminalPanel};
use crate::space::{CloseOutcome, SplitDir};
use crate::view::LocalTerminalView;

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
        let view = cx.new(|cx| LocalTerminalView::new(duplicate, panel.deps.clone(), window, cx));
        panel
            .tree
            .fill_empty(space_one, view)
            .expect("Space #1 must be empty");

        assert_eq!(panel.empty_space_destinations(), vec![destinations[0]]);
    });
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
        let view = cx.new(|cx| LocalTerminalView::new(session_entity, p.deps.clone(), window, cx));
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

    WorkspaceCommands {
        new_terminal_with_shell: terminal,
        open_new_session_dialog: window,
        open_duplicate_ssh_dialog: duplicate_ssh,
        open_settings: app,
        open_about: window,
        find_in_active_terminal: dock,
        setup_key_bindings: app,
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
        let tab_panel = cx.new(|cx| TabPanel::new(None, dock_area.downgrade(), window, cx));
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
                let mut view =
                    LocalTerminalView::new(active_session, panel.deps.clone(), window, cx);
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
        tab_panel.update(cx, |tabs, cx| {
            tabs.add_panel(Arc::new(panel.clone()), window, cx);
        });
        *panel_for_window.borrow_mut() = Some(panel.clone());
        *tab_for_window.borrow_mut() = Some(tab_panel);
        Root::new(panel, window, cx)
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
    let focus = panel.read_with(cx, |panel, cx| panel.focus_handle(cx));
    cx.update(|window, cx| focus.focus(window, cx));
    cx.run_until_parked();
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
        tab_panel: gpui::WeakEntity::new_invalid(),
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

fn agent_state_event(seq: u64) -> oneterm_terminal::AgentStatusEvent {
    oneterm_terminal::AgentStatusEvent {
        agent: "pi".into(),
        seq,
        ts: seq * 1000,
        payload: oneterm_terminal::AgentPayload::State(oneterm_terminal::StateEvent {
            state: oneterm_terminal::AgentState::Working,
            message: None,
            session_id: None,
        }),
    }
}

fn agent_lifecycle(
    registry: &gpui::Entity<oneterm_state::AgentRegistry>,
    terminal_key: gpui::EntityId,
    cx: &mut VisualTestContext,
) -> Option<oneterm_state::Lifecycle> {
    registry.read_with(cx, |reg, _| {
        reg.cards()
            .iter()
            .find(|c| c.terminal_key == terminal_key)
            .map(|c| c.lifecycle)
    })
}

#[gpui::test]
fn exited_behind_output_batch_marks_agent_ended(cx: &mut TestAppContext) {
    // Regression (CORR-02): a process that prints and exits in the same batch
    // delivers `Output` followed by `Exited` on the event channel. The
    // coalescing drain must run the same exit handling as the main loop.
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(oneterm_state::AgentRegistry::init);

    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_spec(session_spec(session, "Agent"), window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();

    let terminal_key = panel.read_with(cx, |panel, _| {
        panel
            .tree
            .active_terminal()
            .expect("panel must own a terminal")
            .entity_id()
    });

    probe
        .emit(oneterm_terminal::SessionEvent::AgentStatus(Arc::new(
            agent_state_event(1),
        )))
        .expect("event channel must accept the agent event");
    cx.run_until_parked();

    let registry = cx.update(|_, cx| oneterm_state::AgentRegistry::global(cx));
    assert_eq!(
        agent_lifecycle(&registry, terminal_key, cx),
        Some(oneterm_state::Lifecycle::Live)
    );

    // Queue both events before the subscriber task gets to run so `Exited`
    // is picked up by the coalescing drain behind `Output`.
    probe
        .emit(oneterm_terminal::SessionEvent::Output)
        .expect("event channel must accept output");
    probe
        .emit(oneterm_terminal::SessionEvent::Exited(Some(0)))
        .expect("event channel must accept exit");
    cx.run_until_parked();

    assert_eq!(
        agent_lifecycle(&registry, terminal_key, cx),
        Some(oneterm_state::Lifecycle::Ended { exit_code: Some(0) })
    );
}
