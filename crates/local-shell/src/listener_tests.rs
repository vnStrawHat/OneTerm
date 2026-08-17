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

/// Poll a bounded transport until an item arrives or `timeout` elapses.
fn recv_within(events: &FakeTransport<SessionEvent>, timeout: std::time::Duration) -> SessionEvent {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Ok(event) = events.try_recv() {
            return event;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no event within {timeout:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// TEST-06 / CORR-01: `forward` runs from `Term` callbacks with the `Term`
/// lock held. Even when the event queue is saturated and nobody drains it, it
/// must return — the UI thread needs that lock to drain the queue, so blocking
/// here would deadlock the app.
#[test]
fn reliable_local_events_do_not_block_while_term_lock_is_held() {
    use alacritty_terminal::sync::FairMutex;
    use alacritty_terminal::term::{Config, Term};

    let events = FakeTransport::bounded(1);
    let listener = LocalListener::new(events.sender(), new_shared());
    let term = Arc::new(FairMutex::new(Term::new(
        Config::default(),
        &crate::session::TermSize {
            cols: 80,
            lines: 24,
        },
        listener.clone(),
    )));
    events.try_send(SessionEvent::Output).unwrap();
    let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);

    let pump = {
        let term = term.clone();
        let listener = listener.clone();
        std::thread::spawn(move || {
            let guard = term.lock();
            // Term callback context: lock held, queue full, no consumer.
            for _ in 0..3 {
                listener.forward(SessionEvent::Bell);
            }
            drop(guard);
            finished_tx.send(()).unwrap();
        })
    };

    finished_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("forward must return while the queue is saturated");
    pump.join().unwrap();
    assert!(listener.has_deferred_reliable());
    // Nothing was lost: the queue still holds the repaint hint only.
    assert_eq!(events.len(), 1);
    assert_eq!(listener.queue_diagnostics().event_closed, 0);
}

/// Deferred reliable events are delivered by the flush after the batch, which
/// waits for queue capacity (backpressure) outside the `Term` lock.
#[test]
fn deferred_local_events_flush_in_order_once_the_queue_drains() {
    let events = FakeTransport::bounded(1);
    let listener = LocalListener::new(events.sender(), new_shared());
    events.try_send(SessionEvent::Output).unwrap();
    listener.forward(SessionEvent::Bell);
    listener.forward(SessionEvent::Title("t".into()));
    assert!(listener.has_deferred_reliable());
    assert_eq!(events.len(), 1);

    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
    let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);
    let flusher = {
        let listener = listener.clone();
        std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            listener.flush_reliable();
            finished_tx.send(()).unwrap();
        })
    };

    started_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();
    assert!(
        finished_rx
            .recv_timeout(std::time::Duration::from_millis(20))
            .is_err()
    );
    assert_eq!(events.try_recv().unwrap(), SessionEvent::Output);
    assert_eq!(
        recv_within(&events, std::time::Duration::from_secs(1)),
        SessionEvent::Bell
    );
    assert_eq!(
        recv_within(&events, std::time::Duration::from_secs(1)),
        SessionEvent::Title("t".into())
    );
    finished_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();
    flusher.join().unwrap();
    assert!(!listener.has_deferred_reliable());
}

/// Once an event is deferred, later reliable events queue behind it even if the
/// channel has room again — FIFO order must survive the deferral.
#[test]
fn deferred_local_events_keep_fifo_order() {
    let events = FakeTransport::bounded(2);
    let listener = LocalListener::new(events.sender(), new_shared());
    events.try_send(SessionEvent::Output).unwrap();
    events.try_send(SessionEvent::Output).unwrap();
    listener.forward(SessionEvent::Bell);
    assert_eq!(events.try_recv().unwrap(), SessionEvent::Output);
    assert_eq!(events.try_recv().unwrap(), SessionEvent::Output);
    // Room again, but Bell is still pending: Title must not jump ahead.
    listener.forward(SessionEvent::Title("t".into()));
    assert_eq!(events.len(), 0);
    listener.flush_reliable();
    assert_eq!(events.try_recv().unwrap(), SessionEvent::Bell);
    assert_eq!(events.try_recv().unwrap(), SessionEvent::Title("t".into()));
}
