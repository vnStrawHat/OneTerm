//! Tests for `LocalListener`.

use async_channel::bounded;
use oneterm_terminal::test_support::FakeTransport;

use super::*;
use crate::state::new_shared;

fn listener() -> (LocalListener, async_channel::Receiver<SessionEvent>) {
    let (tx, rx) = bounded::<SessionEvent>(16);
    (LocalListener::new(tx, new_shared()), rx)
}

#[cfg(unix)]
fn exit_status(code: i32) -> std::process::ExitStatus {
    use std::os::unix::process::ExitStatusExt;

    std::process::ExitStatus::from_raw(code << 8)
}

#[cfg(windows)]
fn exit_status(code: u32) -> std::process::ExitStatus {
    use std::os::windows::process::ExitStatusExt;

    std::process::ExitStatus::from_raw(code)
}

#[test]
fn forwards_title_and_wakeup() {
    let (l, rx) = listener();
    l.send_event(Event::Title("hello".into()));
    l.send_event(Event::Wakeup);
    assert_eq!(rx.try_recv().unwrap(), SessionEvent::Title("hello".into()));
    assert_eq!(rx.try_recv().unwrap(), SessionEvent::Output);
    assert_eq!(l.state.lock().unwrap().title.as_deref(), Some("hello"));
}

#[test]
fn reset_title_clears_cache() {
    let (l, _rx) = listener();
    l.send_event(Event::Title("x".into()));
    l.send_event(Event::ResetTitle);
    assert_eq!(l.state.lock().unwrap().title, None);
}

#[test]
fn clipboard_store_caches_and_forwards() {
    let (l, rx) = listener();
    l.send_event(Event::ClipboardStore(
        alacritty_terminal::term::ClipboardType::Clipboard,
        "secret".into(),
    ));
    assert_eq!(
        rx.try_recv().unwrap(),
        SessionEvent::Clipboard(Some("secret".into()))
    );
    assert_eq!(l.state.lock().unwrap().clipboard.as_deref(), Some("secret"));
}

#[test]
fn child_exit_sets_alive_false_and_code() {
    let (l, rx) = listener();
    let status = exit_status(0);
    l.send_event(Event::ChildExit(status));
    match rx.try_recv().unwrap() {
        SessionEvent::Exited(code) => assert_eq!(code, Some(0)),
        other => panic!("unexpected {other:?}"),
    }
    let st = l.state.lock().unwrap();
    assert!(!st.alive);
    assert_eq!(st.exit_code, Some(0));
}

#[test]
fn pty_write_without_notifier_logs_not_panics() {
    let (l, _rx) = listener();
    l.send_event(Event::PtyWrite("x".into()));
}

#[test]
fn clear_screen_bumps_clear_epoch() {
    let (l, _rx) = listener();
    let before = l.state.lock().unwrap().clear_epoch;
    l.send_event(Event::ClearScreen);
    assert_eq!(l.state.lock().unwrap().clear_epoch, before + 1);
}

#[test]
fn osc7_cwd_forwards_and_caches() {
    let (l, rx) = listener();
    l.send_event(Event::Osc {
        params: vec![b"7".to_vec(), b"file:///tmp".to_vec()],
        bell_terminated: true,
    });
    assert_eq!(
        rx.try_recv().unwrap(),
        SessionEvent::Cwd(std::path::PathBuf::from("/tmp"))
    );
    assert_eq!(
        l.state.lock().unwrap().cwd.as_deref(),
        Some(std::path::Path::new("/tmp"))
    );
}

#[test]
fn osc133_prompt_forwards() {
    let (l, rx) = listener();
    l.send_event(Event::Osc {
        params: vec![b"133".to_vec(), b"A".to_vec()],
        bell_terminated: true,
    });
    assert!(matches!(
        rx.try_recv().unwrap(),
        SessionEvent::ShellIntegration(_)
    ));
}

#[test]
fn osc97_agent_status_forwards() {
    // OSC 9;7;<base64-json> — the engine forwards it as
    // `Event::Osc { params: [b"9", b"7", <base64>] }`. The listener
    // must parse + dedup + forward a `SessionEvent::AgentStatus`.
    let (l, rx) = listener();
    let json = stringify!(
        {"v":1,"agent":"pi","type":"state",
         "seq":1,"ts":1700000000000,
         "state":"working","message":"hi"}
    );
    let params = oneterm_terminal::encode_osc97_params(json);
    l.send_event(Event::Osc {
        params,
        bell_terminated: true,
    });
    match rx.try_recv().unwrap() {
        SessionEvent::AgentStatus(ev) => {
            assert_eq!(ev.agent(), "pi");
            assert_eq!(ev.seq(), 1);
            assert_eq!(ev.type_name(), "state");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn osc97_dedup_drops_stale_seq() {
    // A second event with seq <= the last applied seq is dropped (spec §8.3).
    let (l, rx) = listener();
    let mk = |seq: u64| {
        let json = format!(
            "{{\"v\":1,\"agent\":\"pi\",\"type\":\"state\",
             \"seq\":{seq},\"ts\":1700000000000,
             \"state\":\"working\"}}"
        );
        oneterm_terminal::encode_osc97_params(&json)
    };
    l.send_event(Event::Osc {
        params: mk(5),
        bell_terminated: true,
    });
    assert!(matches!(
        rx.try_recv().unwrap(),
        SessionEvent::AgentStatus(_)
    ));
    // seq=5 again (<= last applied) — dropped, nothing forwarded.
    l.send_event(Event::Osc {
        params: mk(5),
        bell_terminated: true,
    });
    assert!(rx.try_recv().is_err());
    // seq=3 (< last applied) — also dropped.
    l.send_event(Event::Osc {
        params: mk(3),
        bell_terminated: true,
    });
    assert!(rx.try_recv().is_err());
    // seq=6 (> last applied) — forwarded.
    l.send_event(Event::Osc {
        params: mk(6),
        bell_terminated: true,
    });
    assert!(matches!(
        rx.try_recv().unwrap(),
        SessionEvent::AgentStatus(_)
    ));
}

#[test]
fn clipboard_load_forwards_read_request() {
    let (l, rx) = listener();
    l.send_event(Event::ClipboardLoad(
        alacritty_terminal::term::ClipboardType::Clipboard,
        std::sync::Arc::new(|s: &str| s.to_string()),
    ));
    assert_eq!(rx.try_recv().unwrap(), SessionEvent::ClipboardRead);
}

#[test]
fn coalescible_local_repaint_events_are_counted_when_saturated() {
    let events = FakeTransport::bounded(1);
    let listener = LocalListener::new(events.sender(), new_shared());
    events.try_send(SessionEvent::Output).unwrap();

    listener.forward(SessionEvent::Output);

    assert_eq!(listener.queue_diagnostics().event_full, 1);
    assert_eq!(events.len(), 1);

    events.close();
    listener.forward(SessionEvent::Output);
    assert_eq!(listener.queue_diagnostics().event_closed, 1);
}

#[test]
fn reliable_local_events_wait_for_queue_capacity() {
    let events = FakeTransport::bounded(1);
    let listener = LocalListener::new(events.sender(), new_shared());
    events.try_send(SessionEvent::Output).unwrap();
    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
    let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);

    let sender = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        listener.forward(SessionEvent::Bell);
        finished_tx.send(()).unwrap();
    });

    started_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();
    assert!(
        finished_rx
            .recv_timeout(std::time::Duration::from_millis(20))
            .is_err()
    );
    assert_eq!(events.try_recv().unwrap(), SessionEvent::Output);
    finished_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();
    sender.join().unwrap();
    assert_eq!(events.try_recv().unwrap(), SessionEvent::Bell);
}
