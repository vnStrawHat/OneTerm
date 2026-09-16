//! View-level tests: lifecycle, the events pump, the OSC-to-UI table, and the
//! render-engine baseline through a real `TerminalView`.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    ClipboardItem, EntityInputHandler as _, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers,
    TestAppContext,
};
use oneterm_core::InputChannel;
use oneterm_settings::TerminalBlink;
use oneterm_state::InputChannelRegistry;
use oneterm_terminal::security_policy::MAX_QUEUED_NOTIFICATIONS;
use oneterm_terminal::test_support::FakeTerminalSession;
use oneterm_terminal::{
    KeySpec, KeyboardFlags, NamedKey, SessionEvent, SessionKind, TerminalError,
};

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
    held_key_down(key, modifiers, false)
}

/// The same event with GPUI's own held flag set — the OS auto-repeat.
fn held_key_down(key: &str, modifiers: Modifiers, is_held: bool) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: None,
        },
        is_held,
        prefer_character_input: false,
    }
}

fn key_up(key: &str, modifiers: Modifiers) -> KeyUpEvent {
    KeyUpEvent {
        keystroke: Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: None,
        },
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

// ── Press, repeat and release (IN-0040, US-0108) ────────────────────────

/// With no keyboard flag pushed, a press, three OS repeats and a release write
/// exactly what the same key-downs wrote before releases existed: a repeat is
/// the press byte again, and a release contributes **no entry at all** because
/// the encoder answers `None` at rung 1.
#[gpui::test]
fn without_a_flag_press_repeat_and_release_write_todays_bytes(cx: &mut TestAppContext) {
    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);
    let ctrl = Modifiers::control();
    let table: [(&str, Modifiers, &[u8]); 6] = [
        ("enter", Modifiers::default(), b"\r"),
        ("a", Modifiers::default(), b"a"),
        ("up", Modifiers::default(), b"\x1b[A"),
        ("escape", Modifiers::default(), b"\x1b"),
        ("f5", Modifiers::default(), b"\x1b[15~"),
        ("a", ctrl, b"\x01"),
    ];

    let mut expected: Vec<Vec<u8>> = Vec::new();
    for (key, modifiers, bytes) in table {
        view.update_in(cx, |view, window, cx| {
            view.on_key_down(&held_key_down(key, modifiers, false), window, cx);
            for _ in 0..3 {
                view.on_key_down(&held_key_down(key, modifiers, true), window, cx);
            }
            view.on_key_up(&key_up(key, modifiers), window, cx);
        });
        // Four key-downs, one byte string each; the key-up adds nothing.
        expected.extend(std::iter::repeat_n(bytes.to_vec(), 4));
    }
    assert_eq!(probe.writes(), expected);
    assert!(
        view.read_with(cx, |view, _| view.held_keys.is_empty()),
        "every release cleared its key"
    );
}

/// The integration criterion: a real `Terminal` behind the fake session is fed
/// `CSI > 2 u`, and the view's repeat and release carry the `:2` / `:3`
/// event-type sub-fields the program asked for.
#[gpui::test]
fn report_event_types_produces_the_repeat_and_release_bytes(cx: &mut TestAppContext) {
    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);

    // The view reads its modes from the **last painted frame**, so the flags
    // only reach it through a repaint. Without the draw below the test would
    // silently assert the legacy bytes.
    let before = view.read_with(cx, |view, _| view.render_state.borrow().frame.modes());
    assert!(
        before.keyboard_flags.is_empty(),
        "nothing is negotiated before the program pushes"
    );
    probe.feed(b"\x1b[>2u");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let after = view.read_with(cx, |view, _| view.render_state.borrow().frame.modes());
    assert!(
        after
            .keyboard_flags
            .contains(KeyboardFlags::REPORT_EVENT_TYPES),
        "the repaint must carry the pushed flags, or the assertions below prove nothing"
    );

    probe.take_writes();
    view.update_in(cx, |view, window, cx| {
        view.on_key_down(
            &held_key_down("up", Modifiers::default(), false),
            window,
            cx,
        );
        view.on_key_down(&held_key_down("up", Modifiers::default(), true), window, cx);
        view.on_key_up(&key_up("up", Modifiers::default()), window, cx);
    });
    assert_eq!(
        probe.writes(),
        vec![
            // `REPORT_EVENT_TYPES` alone leaves a press on the legacy rung…
            b"\x1b[A".to_vec(),
            // …while a repeat and a release have no legacy spelling at all.
            b"\x1b[1;1:2A".to_vec(),
            b"\x1b[1;1:3A".to_vec(),
        ]
    );
}

/// A press the view swallowed is never owed a release, and neither is a key-up
/// that never had a press. Four swallow paths plus the stray key-up.
#[gpui::test]
fn a_swallowed_press_never_produces_a_release(cx: &mut TestAppContext) {
    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);
    let copy_chord = if cfg!(target_os = "macos") {
        ("c", Modifiers::command())
    } else {
        ("C", Modifiers::control_shift())
    };

    // 1. a view chord (copy), 2. `KeyAction::Ignore` — a printable key on the
    // primary screen, which the IME owns, 3. a chord with no encoding
    // (Ctrl + a multi-character name), 4. a key-up with no key-down at all.
    let swallowed: [(&str, Modifiers); 3] = [
        copy_chord,
        ("x", Modifiers::default()),
        ("print", Modifiers::control()),
    ];
    view.update_in(cx, |view, window, cx| {
        for (key, modifiers) in swallowed {
            let mut event = held_key_down(key, modifiers, false);
            // A printable key needs its layout text for the IME row to fire.
            if key == "x" {
                event.keystroke.key_char = Some("x".to_string());
            }
            view.on_key_down(&event, window, cx);
            view.on_key_up(&key_up(key, modifiers), window, cx);
        }
        view.on_key_up(&key_up("f7", Modifiers::default()), window, cx);
    });
    assert!(
        view.read_with(cx, |view, _| view.held_keys.is_empty()),
        "nothing the view swallowed entered the held set"
    );
    assert!(
        probe.writes().is_empty(),
        "a swallowed press writes nothing, and so does its release: {:?}",
        probe.writes()
    );

    // 5. the completion overlay consumed the key.
    let prompt = r"C:\Users\trunglt>d";
    probe.set_text(prompt);
    probe.set_cursor(0, prompt.chars().count());
    view.update(cx, |view, cx| view.update_completion(cx));
    view.update_in(cx, |view, window, cx| {
        view.on_key_down(
            &held_key_down("escape", Modifiers::default(), false),
            window,
            cx,
        );
        view.on_key_up(&key_up("escape", Modifiers::default()), window, cx);
    });
    assert!(
        probe.writes().is_empty(),
        "the overlay dismissal is not input"
    );
}

/// Focus left with a key still down: the program is owed the release, or a
/// `vim` in kitty mode believes the key is held forever.
#[gpui::test]
fn blur_drains_every_held_key(cx: &mut TestAppContext) {
    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);
    probe.feed(b"\x1b[>2u");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });

    view.update_in(cx, |view, window, cx| {
        view.on_key_down(
            &held_key_down("up", Modifiers::default(), false),
            window,
            cx,
        );
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.held_keys.len()),
        1,
        "the press reached the PTY, so it is owed a release"
    );

    probe.take_writes();
    view.update(cx, |view, cx| view.release_held_keys(cx));
    assert_eq!(probe.writes(), vec![b"\x1b[1;1:3A".to_vec()]);
    assert!(view.read_with(cx, |view, _| view.held_keys.is_empty()));

    // A second blur has nothing left to drain.
    probe.take_writes();
    view.update(cx, |view, cx| view.release_held_keys(cx));
    assert!(probe.writes().is_empty(), "a drained set stays drained");
}

/// A release reaches the origin session and no broadcast peer: a peer whose
/// program negotiated nothing must not receive a `:3` sequence it has never
/// seen a press for.
#[gpui::test]
fn a_release_is_never_fanned_out_to_channel_peers(cx: &mut TestAppContext) {
    test_support::init(cx);
    cx.update(InputChannelRegistry::init);
    let (origin_session, origin_probe) = FakeTerminalSession::boxed(24, 80, "");
    let (peer_session, peer_probe) = FakeTerminalSession::boxed(24, 80, "");
    let (views, cx) = open_views(cx, vec![origin_session, peer_session]);
    for view in &views {
        view.update(cx, |view, cx| view.join_channel(InputChannel::A, cx));
    }
    origin_probe.feed(b"\x1b[>2u");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });

    views[0].update_in(cx, |view, window, cx| {
        view.on_key_down(
            &held_key_down("up", Modifiers::default(), false),
            window,
            cx,
        );
        view.on_key_up(&key_up("up", Modifiers::default()), window, cx);
    });
    assert_eq!(
        origin_probe.writes(),
        vec![b"\x1b[A".to_vec(), b"\x1b[1;1:3A".to_vec()],
        "the origin receives both halves"
    );
    assert_eq!(
        peer_probe.writes(),
        vec![b"\x1b[A".to_vec()],
        "the peer receives the press and nothing else"
    );
}

// ── Adopted from US-0108's independent verification (IN-0040) ───────────

/// The packet's blur test calls `release_held_keys` directly, which does not
/// prove the subscription is wired — and this test records **why no view test
/// can**: GPUI dispatches focus events only from a draw, and builds the event's
/// `previous_focus_path` only `if previous_window_active` (`window.rs`, the
/// `DrawPhase::Focus` block). A `TestWindow::is_active` is hard-coded `false`,
/// so the path is always empty and `Context::on_blur`'s condition
/// (`previous_focus_path.last() == Some(&focus_id)`) can never hold. The
/// terminal really is blurred here, the held key really is still held, and the
/// subscription really did not run. Whether it runs on Windows is left to the
/// manual walk, which was not performed.
#[gpui::test]
fn verify_the_blur_drain_is_unprovable_in_a_test_window(cx: &mut TestAppContext) {
    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);
    probe.feed(b"\x1b[>2u");
    // Give the terminal real window focus first, or a blur cannot happen.
    let handle = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| handle.focus(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.run_until_parked();
    assert!(
        view.update_in(cx, |view, window, _| view.focus.is_focused(window)),
        "the terminal owns window focus before the blur"
    );
    view.update_in(cx, |view, window, cx| {
        view.on_key_down(
            &held_key_down("up", Modifiers::default(), false),
            window,
            cx,
        );
    });
    probe.take_writes();

    // A real focus change: the window loses its focused element, which is what
    // alt-tabbing away from a held key looks like.
    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.run_until_parked();
    let blur_seen = view.read_with(cx, |view, _| view.focused);
    let (still_held, still_focused) = view.update_in(cx, |view, window, _| {
        (view.held_keys.clone(), view.focus.is_focused(window))
    });
    assert!(
        !still_focused,
        "the window really did lose the terminal's focus"
    );
    assert!(
        blur_seen,
        "and the subscription still did not run: `focused` would be false if it had"
    );
    assert_eq!(
        still_held,
        vec![KeySpec::Named(NamedKey::ArrowUp)],
        "so the held key is left stranded in the harness, and the drain is unproven"
    );
    assert!(
        probe.writes().is_empty(),
        "no release was written, because no blur was observed"
    );
}

/// Two keys held at once, released in the reverse order.
#[gpui::test]
fn verify_two_held_keys_release_in_either_order(cx: &mut TestAppContext) {
    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);
    probe.feed(b"\x1b[>2u");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    view.update_in(cx, |view, window, cx| {
        view.on_key_down(
            &held_key_down("up", Modifiers::default(), false),
            window,
            cx,
        );
        view.on_key_down(
            &held_key_down("down", Modifiers::default(), false),
            window,
            cx,
        );
    });
    assert_eq!(view.read_with(cx, |view, _| view.held_keys.len()), 2);
    probe.take_writes();
    view.update_in(cx, |view, window, cx| {
        // Reverse order: the second key down is the first key up.
        view.on_key_up(&key_up("down", Modifiers::default()), window, cx);
        view.on_key_up(&key_up("up", Modifiers::default()), window, cx);
        // And a third key-up for a key that was never down.
        view.on_key_up(&key_up("left", Modifiers::default()), window, cx);
    });
    assert_eq!(
        probe.writes(),
        vec![b"\x1b[1;1:3B".to_vec(), b"\x1b[1;1:3A".to_vec()],
        "each held key gets exactly one release, and a stray key-up none"
    );
    assert!(view.read_with(cx, |view, _| view.held_keys.is_empty()));
}

/// Each view owns its set: a key-up delivered to a different view than the one
/// that saw the press writes nothing there.
#[gpui::test]
fn verify_a_key_up_at_another_view_writes_nothing(cx: &mut TestAppContext) {
    let (first, first_probe) = FakeTerminalSession::boxed(24, 80, "");
    let (second, second_probe) = FakeTerminalSession::boxed(24, 80, "");
    let (views, cx) = open_views(cx, vec![first, second]);
    first_probe.feed(b"\x1b[>2u");
    second_probe.feed(b"\x1b[>2u");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    views[0].update_in(cx, |view, window, cx| {
        view.on_key_down(
            &held_key_down("up", Modifiers::default(), false),
            window,
            cx,
        );
    });
    first_probe.take_writes();
    views[1].update_in(cx, |view, window, cx| {
        view.on_key_up(&key_up("up", Modifiers::default()), window, cx);
    });
    assert!(
        second_probe.writes().is_empty(),
        "the other view never saw the press"
    );
    assert_eq!(
        views[0].read_with(cx, |view, _| view.held_keys.len()),
        1,
        "and the press is still owed a release by the view that wrote it"
    );
}

/// Ctrl+C keeps its signal path with nothing negotiated, and a key-up after it
/// writes nothing: the press never reached the encoder, so nothing is owed.
#[gpui::test]
fn verify_ctrl_c_owes_no_release(cx: &mut TestAppContext) {
    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);
    probe.feed(b"\x1b[>2u");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    probe.take_writes();
    view.update_in(cx, |view, window, cx| {
        view.on_key_down(&key_down("c", Modifiers::control()), window, cx);
        view.on_key_up(&key_up("c", Modifiers::control()), window, cx);
    });
    assert!(
        view.read_with(cx, |view, _| view.held_keys.is_empty()),
        "an interrupt is not a held key"
    );
    assert_eq!(
        probe.writes(),
        vec![b"\x03".to_vec()],
        "the interrupt itself, and nothing from the key-up"
    );
}

/// `F2`: the packet blocked release fan-out because "a peer whose program
/// negotiated nothing must not receive a form it has never seen a press for",
/// and the Ctrl+C row it changed first reopened that hole. The rule now holds
/// for both: whatever the **origin** negotiated, every peer receives
/// `BroadcastInput::Interrupt`, the one form all of them understand.
#[gpui::test]
fn verify_ctrl_c_fans_an_interrupt_whatever_the_origin_negotiated(cx: &mut TestAppContext) {
    test_support::init(cx);
    cx.update(InputChannelRegistry::init);
    let (origin_session, origin_probe) = FakeTerminalSession::boxed(24, 80, "");
    let (peer_session, peer_probe) = FakeTerminalSession::boxed(24, 80, "");
    let (views, cx) = open_views(cx, vec![origin_session, peer_session]);
    for view in &views {
        view.update(cx, |view, cx| view.join_channel(InputChannel::A, cx));
    }

    // Nothing negotiated anywhere: the signal, to everyone.
    views[0].update_in(cx, |view, window, cx| {
        view.on_key_down(&key_down("c", Modifiers::control()), window, cx);
    });
    assert_eq!(origin_probe.take_writes(), vec![b"\x03".to_vec()]);
    assert_eq!(peer_probe.take_writes(), vec![b"\x03".to_vec()]);

    // Only the origin's program negotiates, and only the origin's bytes change.
    // `CSI > 1 u` is DISAMBIGUATE alone, which the kitty specification puts on
    // the `CSI u` rung for a ctrl chord.
    origin_probe.feed(b"\x1b[>1u");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    origin_probe.take_writes();
    peer_probe.take_writes();
    views[0].update_in(cx, |view, window, cx| {
        view.on_key_down(&key_down("c", Modifiers::control()), window, cx);
    });
    assert_eq!(
        origin_probe.writes(),
        vec![b"\x1b[99;5u".to_vec()],
        "the origin's program asked for the key, and gets it"
    );
    assert_eq!(
        peer_probe.writes(),
        vec![b"\x03".to_vec()],
        "the peer negotiated nothing and still gets the interrupt it understands"
    );
}

/// `F1` at the view level: a press named by its shifted glyph and a key-up
/// named by the unshifted one -- what the Windows backend produces when Shift
/// is released before the key. `canonical_key` pairs them, so the release is
/// written instead of being lost with the key left held.
#[gpui::test]
fn verify_a_shifted_digit_release_is_paired(cx: &mut TestAppContext) {
    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);
    // The alternate screen first, so the IME does not own the key — and only
    // then the flags, because the engine keeps one keyboard stack per screen
    // and swaps them on the alt-screen swap.
    probe.feed(b"\x1b[?1049h\x1b[>10u");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    probe.take_writes();
    view.update_in(cx, |view, window, cx| {
        // gpui-pre-windows resolves Shift+1 to key "!" with shift cleared.
        let mut press = held_key_down("!", Modifiers::default(), false);
        press.keystroke.key_char = Some("!".to_string());
        view.on_key_down(&press, window, cx);
        // Shift released first, so the key-up is named "1".
        view.on_key_up(&key_up("1", Modifiers::default()), window, cx);
    });
    assert!(
        view.read_with(cx, |view, _| view.held_keys.is_empty()),
        "the release paired, so nothing is left held"
    );
    assert_eq!(
        probe.writes(),
        vec![b"\x1b[49u".to_vec(), b"\x1b[49;1:3u".to_vec()],
        "both carry code point 49, so the program can match the release to the press"
    );
}

/// `F3`'s testable half: a fresh press of a key already held cannot compound
/// into a stuck entry, whichever way a release went missing.
#[gpui::test]
fn verify_a_repeated_press_leaves_one_held_entry(cx: &mut TestAppContext) {
    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);
    probe.feed(b"\x1b[>2u");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    view.update_in(cx, |view, window, cx| {
        for _ in 0..3 {
            view.on_key_down(
                &held_key_down("up", Modifiers::default(), false),
                window,
                cx,
            );
        }
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.held_keys.len()),
        1,
        "three presses, one entry"
    );
    probe.take_writes();
    view.update_in(cx, |view, window, cx| {
        view.on_key_up(&key_up("up", Modifiers::default()), window, cx);
        view.on_key_up(&key_up("up", Modifiers::default()), window, cx);
    });
    assert_eq!(
        probe.writes(),
        vec![b"\x1b[1;1:3A".to_vec()],
        "and one release, not three; the second key-up finds nothing"
    );
}

// ── Re-verification attacks at db8a059b. NOT part of the packet. ─────────

/// Two peers, one of which negotiated `REPORT_ALL_KEYS_AS_ESC` itself. The
/// rework fans an interrupt at every peer regardless, so the peer that *did*
/// negotiate receives a signal its program asked to see as a key.
#[gpui::test]
fn reverify_a_negotiating_peer_still_receives_the_interrupt(cx: &mut TestAppContext) {
    test_support::init(cx);
    cx.update(InputChannelRegistry::init);
    let (origin_session, origin_probe) = FakeTerminalSession::boxed(24, 80, "");
    let (esc_session, esc_probe) = FakeTerminalSession::boxed(24, 80, "");
    let (plain_session, plain_probe) = FakeTerminalSession::boxed(24, 80, "");
    let (views, cx) = open_views(cx, vec![origin_session, esc_session, plain_session]);
    for view in &views {
        view.update(cx, |view, cx| view.join_channel(InputChannel::A, cx));
    }
    origin_probe.feed(b"\x1b[>8u");
    esc_probe.feed(b"\x1b[>8u");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    origin_probe.take_writes();
    esc_probe.take_writes();
    plain_probe.take_writes();

    views[0].update_in(cx, |view, window, cx| {
        view.on_key_down(&key_down("c", Modifiers::control()), window, cx);
    });
    assert_eq!(
        origin_probe.writes(),
        vec![b"\x1b[99;5u".to_vec()],
        "the origin gets the key its own program negotiated"
    );
    assert_eq!(
        esc_probe.writes(),
        vec![b"\x03".to_vec()],
        "the peer that negotiated the same flag still gets a signal it asked to see as a key"
    );
    assert_eq!(
        plain_probe.writes(),
        vec![b"\x03".to_vec()],
        "and the peer that negotiated nothing gets the form it understands"
    );
}

/// Ctrl+C under `DISAMBIGUATE_ESC_CODES` alone at the view level: exactly one
/// write, the encoded key, with no `0x03` alongside it -- the view took the
/// encoded branch and did not also signal.
#[gpui::test]
fn reverify_ctrl_c_under_disambiguate_is_handled_once(cx: &mut TestAppContext) {
    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    let (view, cx) = open_view(cx, session);
    probe.feed(b"\x1b[>1u");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    probe.take_writes();
    view.update_in(cx, |view, window, cx| {
        view.on_key_down(&key_down("c", Modifiers::control()), window, cx);
        view.on_key_up(&key_up("c", Modifiers::control()), window, cx);
    });
    assert_eq!(
        probe.writes(),
        vec![b"\x1b[99;5u".to_vec()],
        "one write for the press; no release, because REPORT_EVENT_TYPES was not pushed"
    );
    assert!(
        view.read_with(cx, |view, _| view.held_keys.is_empty()),
        "and the key-up cleared the entry the encoded interrupt created"
    );
}

/// The stranding case the fix exists for, driven the way the platform produces
/// it, and the reverse of the case the packet's own test drives.
#[gpui::test]
fn reverify_a_digit_release_pairs_in_both_directions(cx: &mut TestAppContext) {
    for (down, up) in [("!", "1"), ("1", "!")] {
        let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
        let (view, cx) = open_view(cx, session);
        // The alternate screen first: the kitty flag stack is per-screen, so
        // pushing before the switch leaves the new screen with nothing.
        probe.feed(b"\x1b[?1049h\x1b[>10u");
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        probe.take_writes();
        view.update_in(cx, |view, window, cx| {
            let mut press = held_key_down(down, Modifiers::default(), false);
            press.keystroke.key_char = Some(down.to_string());
            view.on_key_down(&press, window, cx);
            let mut release = key_up(up, Modifiers::default());
            release.keystroke.key_char = Some(up.to_string());
            view.on_key_up(&release, window, cx);
        });
        assert!(
            view.read_with(cx, |view, _| view.held_keys.is_empty()),
            "press {down:?} / release {up:?}: nothing may be stranded"
        );
        assert_eq!(
            probe.writes(),
            vec![b"\x1b[49u".to_vec(), b"\x1b[49;1:3u".to_vec()],
            "press {down:?} / release {up:?}: both events name key 49, so they pair"
        );
    }
}
