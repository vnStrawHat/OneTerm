//! View-level tests: lifecycle, the events pump, the OSC-to-UI table, and the
//! render-engine baseline through a real `TerminalView`.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    ClipboardItem, EntityInputHandler as _, KeyDownEvent, Keystroke, Modifiers, TestAppContext,
};
use oneterm_core::InputChannel;
use oneterm_settings::TerminalBlink;
use oneterm_state::InputChannelRegistry;
use oneterm_terminal::security_policy::MAX_QUEUED_NOTIFICATIONS;
use oneterm_terminal::test_support::FakeTerminalSession;
use oneterm_terminal::{SessionEvent, SessionKind, TerminalError};

use super::TerminalViewEvent;
use super::test_support::{self, open_view, open_views};
use crate::input::paste_clipboard;

#[gpui::test]
fn ssh_close_shows_banner_once(cx: &mut TestAppContext) {
    let (session, probe) =
        FakeTerminalSession::boxed_with_kind(24, 80, "previous output", SessionKind::Ssh);
    let (view, cx) = open_view(cx, session);
    probe.set_alive(false);

    // Read the flags before the frame that follows the update drains the
    // toast into the window.
    let first = view.update(cx, |view, cx| {
        view.handle_event(SessionEvent::Closed, cx);
        (view.ssh_closed, view.pending_ssh_closed_notification)
    });
    assert_eq!(first, (true, true));

    let second = view.update(cx, |view, cx| {
        view.handle_event(SessionEvent::Closed, cx);
        (view.ssh_closed, view.pending_ssh_closed_notification)
    });
    assert_eq!(
        second,
        (true, false),
        "a repeated close must not enqueue another toast"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.session.read(cx).write(b"ignored")),
        Err(TerminalError::Closed)
    );
    assert!(probe.writes().is_empty());
}

#[gpui::test]
fn notification_queue_is_bounded(cx: &mut TestAppContext) {
    let (session, _) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);

    // Queue and inspect inside one update: the frame that follows drains the
    // queue into window toasts.
    let (queued, dropped, oldest) = view.update(cx, |view, cx| {
        for i in 0..=MAX_QUEUED_NOTIFICATIONS {
            view.handle_event(SessionEvent::Notification(format!("n{i}")), cx);
        }
        (
            view.pending_notifications.len(),
            view.dropped_notifications,
            view.pending_notifications.front().cloned(),
        )
    });
    assert_eq!(queued, MAX_QUEUED_NOTIFICATIONS);
    assert_eq!(dropped, 1);
    assert_eq!(oldest.as_deref(), Some("n1"));
}

/// A blink tick on an unfocused view (or with blinking off) keeps the cursor
/// steady instead of toggling + repainting.
#[gpui::test]
fn blink_tick_gated_by_focus_and_setting(cx: &mut TestAppContext) {
    let (session, _) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);

    view.update(cx, |view, cx| {
        view.deps.settings.update(cx, |s, _| {
            s.cursor_blink = TerminalBlink::On;
        });
        view.focused = true;
        view.blink_visible = true;
        view.blink_tick(cx);
        assert!(!view.blink_visible, "focused + On toggles");

        view.focused = false;
        view.blink_tick(cx);
        assert!(view.blink_visible, "unfocused resets to visible");
        view.blink_tick(cx);
        assert!(view.blink_visible, "unfocused never hides");

        view.focused = true;
        view.deps.settings.update(cx, |s, _| {
            s.cursor_blink = TerminalBlink::Off;
        });
        view.blink_tick(cx);
        assert!(view.blink_visible, "blink Off keeps the cursor steady");
    });
}

/// The events pump coalesces a burst of `Output` into one handling, and the
/// reliable events queued behind it (a bell, then the remote close) are still
/// processed in order by the same drain — the `Exited` variant of this path
/// is covered end to end by `panel::tests::exited_behind_output_batch_marks_agent_ended`.
#[gpui::test]
fn output_batch_coalesces_and_keeps_exited(cx: &mut TestAppContext) {
    let (session, probe) = FakeTerminalSession::boxed_with_kind(24, 80, "", SessionKind::Ssh);
    let (view, cx) = open_view(cx, session);

    // Queue the whole batch before the pump task gets to run.
    for _ in 0..3 {
        probe
            .emit(SessionEvent::Output)
            .expect("event channel must accept output");
    }
    probe
        .emit(SessionEvent::Bell)
        .expect("event channel must accept the bell");
    probe
        .emit(SessionEvent::Exited(Some(0)))
        .expect("event channel must accept exit");
    probe
        .emit(SessionEvent::Closed)
        .expect("event channel must accept close");
    cx.run_until_parked();

    let (has_bell, ssh_closed, stamped) = view.read_with(cx, |view, _| {
        (
            view.has_bell,
            view.ssh_closed,
            !view.gutter_times.times().is_empty(),
        )
    });
    assert!(stamped, "the coalesced Output still stamps the gutter");
    assert!(has_bell, "a Bell behind the Output batch is handled");
    assert!(ssh_closed, "a Closed behind the Output batch is handled");
}

#[gpui::test]
fn clipboard_read_reply_gated_by_setting(cx: &mut TestAppContext) {
    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("hi".into())));

    view.update(cx, |view, cx| {
        view.deps
            .settings
            .update(cx, |s, _| s.allow_clipboard_read = false);
        view.handle_event(SessionEvent::ClipboardRead, cx);
    });
    assert!(probe.writes().is_empty(), "refused by default");

    view.update(cx, |view, cx| {
        view.deps
            .settings
            .update(cx, |s, _| s.allow_clipboard_read = true);
        view.handle_event(SessionEvent::ClipboardRead, cx);
    });
    let writes = probe.writes();
    assert_eq!(writes.len(), 1);
    assert!(
        writes[0].starts_with(b"\x1b]52;c;") && writes[0].ends_with(b"\x07"),
        "OSC 52 reply: {:?}",
        String::from_utf8_lossy(&writes[0])
    );
}

#[gpui::test]
fn title_event_emits_title_changed(cx: &mut TestAppContext) {
    let (session, _) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);
    let seen = Rc::new(Cell::new(0));
    let _subscription = cx.update(|_, cx| {
        let seen = seen.clone();
        cx.subscribe(&view, move |_, event: &TerminalViewEvent, _| {
            assert_eq!(*event, TerminalViewEvent::TitleChanged);
            seen.set(seen.get() + 1);
        })
    });

    view.update(cx, |view, cx| {
        view.handle_event(SessionEvent::Title("vim".into()), cx);
        view.handle_event(SessionEvent::Bell, cx);
    });
    cx.run_until_parked();
    assert_eq!(seen.get(), 1, "only the title event notifies the panel");
}

#[gpui::test]
fn completion_overlay_shows_when_typing_d_at_cmd_prompt(cx: &mut TestAppContext) {
    // A cmd prompt with the user having typed `d`. Cursor sits just after `d`.
    let prompt = r"C:\Users\trunglt>d";
    let (session, probe) = FakeTerminalSession::boxed(24, 80, prompt);
    probe.set_cursor(0, prompt.chars().count());
    let (view, cx) = open_view(cx, session);

    // Drive the same path `render` uses.
    view.update(cx, |v, cx| v.update_completion(cx));

    let (visible, texts) = view.read_with(cx, |v, _| {
        let c = v
            .completion
            .controller
            .as_ref()
            .expect("controller must be initialized");
        (
            c.is_visible(),
            c.suggestions()
                .iter()
                .map(|s| s.text.clone())
                .collect::<Vec<_>>(),
        )
    });
    assert!(visible, "overlay should be visible for 'd' at a cmd prompt");
    assert!(
        texts
            .iter()
            .any(|t| t == "dir" || t == "date" || t == "del"),
        "expected dir/date/del among suggestions, got {texts:?}"
    );
}

#[gpui::test]
fn completion_resumes_after_initial_non_prompt_render(cx: &mut TestAppContext) {
    // Regression: the pre-grid gate must NOT depend on `in_prompt_region` (only
    // known after reading the line). An initial empty/non-prompt render once
    // left the region gate stuck false, permanently blocking completion.
    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    probe.set_cursor(0, 0);
    let (view, cx) = open_view(cx, session);

    // Frame 1: empty grid / no prompt → not visible.
    view.update(cx, |v, cx| v.update_completion(cx));
    assert!(
        !view.read_with(cx, |v, _| v.completion.is_visible()),
        "empty prompt must not show an overlay"
    );

    // Frame 2: the prompt is drawn and the user typed `d`.
    let prompt = r"C:\Users\trunglt>d";
    probe.set_text(prompt);
    probe.set_cursor(0, prompt.chars().count());
    view.update(cx, |v, cx| v.update_completion(cx));
    assert!(
        view.read_with(cx, |v, _| v.completion.is_visible()),
        "overlay must resume after an initial non-prompt render (pre-grid gate bug)"
    );
}

#[gpui::test]
fn phase0_renderer_baseline_counts_dirty_and_idle_frames(cx: &mut TestAppContext) {
    let (session, probe) = FakeTerminalSession::boxed(
        24,
        80,
        "OneTerm Phase 0 renderer baseline\nhttps://example.com/diagnostics",
    );
    let (view, cx) = open_view(cx, session);

    // Draw once to warm the plan and glyph caches.
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let warm = view.read_with(cx, |view, _| view.render_diagnostics());
    assert_eq!(warm.snapshot_calls, 1, "one snapshot per frame: {warm:?}");
    assert!(warm.rows_total > 0);
    assert!(warm.quads > 0);

    // Dirty frame: change the session text and immediately redraw. No
    // run_until_parked in between — the blink task could otherwise draw first
    // and consume the damage.
    probe.set_text("Changed content forces a dirty render\nnew line here");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let dirty = view.read_with(cx, |view, _| view.render_diagnostics());
    assert!(probe.snapshot_calls() >= 2);
    assert!(
        dirty.rows_planned > 0,
        "dirty frame should re-plan changed rows: {dirty:?}"
    );
    assert!(
        dirty.shape_calls > 0,
        "dirty frame should shape new text: {dirty:?}"
    );
    assert!(dirty.quads > 0);

    // Idle frame: no content change, every row plan is reused.
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let idle = view.read_with(cx, |view, _| view.render_diagnostics());
    assert_eq!(idle.rows_total, dirty.rows_total);
    assert_eq!(
        idle.rows_planned, 0,
        "idle frame should not re-plan: {idle:?}"
    );
    assert_eq!(
        idle.shape_calls, 0,
        "idle frame should not re-shape: {idle:?}"
    );
    assert_eq!(
        idle.url_scans, 0,
        "idle frame should not rescan URLs: {idle:?}"
    );
    assert!(idle.quads > 0, "idle frame still paints quads");
}

/// A focused view in its visible blink phase paints the cursor layer on top
/// of the grid layer (the fake reports a block cursor at the set position).
#[gpui::test]
fn focused_view_paints_the_cursor_layer(cx: &mut TestAppContext) {
    let (session, probe) = FakeTerminalSession::boxed(24, 80, "C:>");
    probe.set_cursor(0, 3);
    let (view, cx) = open_view(cx, session);
    view.update(cx, |view, _| view.blink_visible = true);
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let (stats, config) = view.read_with(cx, |v, _| {
        let state = v.render_state.borrow();
        (state.stats, state.inputs.cursor)
    });
    assert!(config.focused, "the view takes focus on creation");
    assert_eq!(stats.layers, 2, "grid layer + cursor layer: {stats:?}");
}

/// Closing the search bar hands keyboard focus back to the terminal: the
/// bar's input owned it, so without an explicit refocus the window is left
/// with nothing focused and typing goes nowhere until the user clicks.
#[gpui::test]
fn closing_search_refocuses_the_terminal(cx: &mut TestAppContext) {
    let (session, _) = FakeTerminalSession::boxed(24, 80, "C:>");
    let (view, cx) = open_view(cx, session);

    view.update_in(cx, |view, window, cx| view.toggle_search(window, cx));
    cx.run_until_parked();
    let input_focused = view.update_in(cx, |view, window, cx| {
        view.search.input_is_focused(window, cx)
    });
    assert!(input_focused, "opening the bar focuses its input");

    view.update_in(cx, |view, window, cx| view.toggle_search(window, cx));
    cx.run_until_parked();
    let terminal_focused = view.update_in(cx, |view, window, _| view.focus.is_focused(window));
    assert!(
        terminal_focused,
        "the terminal must own focus again after the search bar closes"
    );
}

// ── Broadcast input channels (IN-0022) ──────────────────────────────────

fn key_down(key: &str, modifiers: Modifiers) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: None,
        },
        is_held: false,
        prefer_character_input: false,
    }
}

/// Every way a Space produces input reaches the other members of its channel,
/// and only those: the third view stays outside and the origin is never
/// written to twice.
#[gpui::test]
fn member_input_reaches_the_channel_peers_only(cx: &mut TestAppContext) {
    test_support::init(cx);
    cx.update(InputChannelRegistry::init);
    let (origin_session, origin_probe) = FakeTerminalSession::boxed(24, 80, "");
    let (first_peer, first_probe) = FakeTerminalSession::boxed(24, 80, "");
    let (second_peer, second_probe) = FakeTerminalSession::boxed(24, 80, "");
    let (outsider_session, outsider_probe) = FakeTerminalSession::boxed(24, 80, "");
    let (views, cx) = open_views(
        cx,
        vec![origin_session, first_peer, second_peer, outsider_session],
    );
    let origin = views[0].clone();

    for view in views.iter().take(3) {
        view.update(cx, |view, cx| view.join_channel(InputChannel::A, cx));
    }
    assert_eq!(
        origin.update(cx, |view, cx| view.channel(cx)),
        Some(InputChannel::A)
    );
    assert_eq!(views[3].update(cx, |view, cx| view.channel(cx)), None);

    // A key, typed (IME) text, a paste and Ctrl+C — the four write paths.
    origin.update_in(cx, |view, window, cx| {
        view.on_key_down(&key_down("enter", Modifiers::default()), window, cx);
        view.replace_text_in_range(None, "hi", window, cx);
    });
    let (session, broadcast) = origin.update(cx, |view, cx| {
        (view.session.clone(), view.broadcast_origin(cx))
    });
    cx.update(|window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("ls -al".into()));
        paste_clipboard(&session, &broadcast, window, cx);
    });
    origin.update_in(cx, |view, window, cx| {
        view.on_key_down(&key_down("c", Modifiers::control()), window, cx);
    });

    let expected = vec![
        b"\r".to_vec(),
        b"hi".to_vec(),
        b"ls -al".to_vec(),
        b"\x03".to_vec(),
    ];
    assert_eq!(origin_probe.writes(), expected, "the origin writes once");
    assert_eq!(first_probe.writes(), expected, "the first peer receives it");
    assert_eq!(second_probe.writes(), expected, "so does the second");
    assert!(
        outsider_probe.writes().is_empty(),
        "a Space outside the channel receives nothing"
    );
}

/// Without the registry (tests, tools) every hook is a no-op and the terminal
/// behaves exactly as before the channels existed.
#[gpui::test]
fn a_view_without_a_registry_still_writes_to_its_own_session(cx: &mut TestAppContext) {
    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);

    view.update_in(cx, |view, window, cx| {
        view.on_key_down(&key_down("enter", Modifiers::default()), window, cx);
    });
    assert_eq!(probe.writes(), vec![b"\r".to_vec()]);
    assert_eq!(view.update(cx, |view, cx| view.channel(cx)), None);
}

#[gpui::test]
fn shutdown_leaves_the_channel(cx: &mut TestAppContext) {
    test_support::init(cx);
    cx.update(InputChannelRegistry::init);
    let (session, _) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);
    let id = view.entity_id();
    let registry = cx.update(|_, cx| InputChannelRegistry::global(cx));

    view.update(cx, |view, cx| view.join_channel(InputChannel::A, cx));
    assert_eq!(
        registry.read_with(cx, |registry, _| registry.channel_of(id)),
        Some(InputChannel::A)
    );

    view.update(cx, |view, cx| view.shutdown(cx));
    assert_eq!(
        registry.read_with(cx, |registry, _| registry.channel_of(id)),
        None,
        "a closed Space is no longer a broadcast target"
    );
}
