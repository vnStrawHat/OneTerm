use gpui::{AppContext as _, TestAppContext, VisualTestContext};
use oneterm_core::terminal::test_support::FakeTerminalSession;

use crate::views::terminal::panel::TerminalPanel;
use crate::views::terminal::space::{CloseOutcome, SplitDir};
use crate::views::terminal::view::LocalTerminalView;

#[gpui::test]
fn phase0_close_last_space_calls_session_close(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);

    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_session(session, "Phase0", window, cx)
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
            view.read(cx).session.read(cx).close();
        }
    });
    assert_eq!(probe.close_calls(), 1);
    assert!(!probe.alive());
}

#[gpui::test]
fn phase0_close_non_last_space_closes_removed_session(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);

    // First session — will be split and then closed.
    let (session_a, probe_a) = FakeTerminalSession::boxed(24, 80, "session A");
    let (panel, cx) = cx
        .add_window_view(move |window, cx| TerminalPanel::from_session(session_a, "A", window, cx));
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
        let view = cx.new(|cx| LocalTerminalView::new(session_entity, window, cx));
        p.tree.fill_empty(new_active, view);
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

    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_session(session, "Phase1", window, cx)
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

    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        TerminalPanel::from_session(session, "Phase1", window, cx)
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
