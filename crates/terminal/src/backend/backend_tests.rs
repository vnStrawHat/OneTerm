//! Tests for the shared backend pump layer: router event mapping, event-sink
//! delivery policy, colour-query replies, line accounting, and the pump
//! driven end to end through an in-memory transport (TEST-01 / TEST-02).
//!
//! `US-0081` changed how an event reaches the router — the engine returns a
//! batch of values instead of calling back — so every test that used to build
//! an `alacritty_terminal::event::Event` now builds the same thing as a
//! `VtEvent` in an [`EventBatch`]. The **coverage is unchanged**: one test per
//! event kind, the colour-query deferral, the delivery policy and the ordering
//! guarantees. `migration.md` § "Tests that change" records the two deletions.

use std::sync::Arc;
use std::time::Duration;

use alacritty_terminal::vte::ansi::Rgb;
use oneterm_vt::{ClipboardKind, ColorKey, EventBatch, StringTerm, VtEvent};

use crate::engine::{SharedTerminal, new_shared_terminal};
use crate::security_policy::ClipboardOrigin;
use crate::session::SessionEvent;
use crate::test_support::FakePtyTransport;

use super::*;

type Router = OscRouter<FakePtyTransport>;

struct Fixture {
    router: Router,
    transport: FakePtyTransport,
    events_tx: async_channel::Sender<SessionEvent>,
    events: async_channel::Receiver<SessionEvent>,
    state: SharedState,
}

fn fixture(origin: ClipboardOrigin, capacity: usize) -> Fixture {
    let (events_tx, events) = async_channel::bounded(capacity);
    let transport = FakePtyTransport::new();
    let state = SharedSessionState::new_alive();
    let router = OscRouter::new(
        transport.clone(),
        SessionEventSink::new(events_tx.clone()),
        state.clone(),
        origin,
    );
    Fixture {
        router,
        transport,
        events_tx,
        events,
        state,
    }
}

fn local(capacity: usize) -> Fixture {
    fixture(ClipboardOrigin::Local, capacity)
}

fn new_term() -> SharedTerminal {
    new_shared_terminal(
        GridSize {
            cols: 80,
            lines: 24,
        },
        crate::engine::DEFAULT_SCROLLBACK_LINES,
    )
}

/// Route one batch, built the way `Terminal::feed` would have built it.
fn route(router: &Router, build: impl FnOnce(&mut EventBatch)) {
    let mut batch = EventBatch::new();
    build(&mut batch);
    router.drain(&batch);
}

fn osc(batch: &mut EventBatch, params: &[&[u8]]) {
    let code = std::str::from_utf8(params[0])
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);
    batch.push_osc(code, params, StringTerm::Bel, false);
}

fn drain(events: &async_channel::Receiver<SessionEvent>) -> Vec<SessionEvent> {
    std::iter::from_fn(|| events.try_recv().ok()).collect()
}

/// Poll a bounded transport until an item arrives or `timeout` elapses.
fn recv_within(events: &async_channel::Receiver<SessionEvent>, timeout: Duration) -> SessionEvent {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Ok(event) = events.try_recv() {
            return event;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no event within {timeout:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

// ── Router: event → state + SessionEvent ─────────────────────────────────

#[test]
fn forwards_title_and_wakeup() {
    let f = local(16);
    route(&f.router, |batch| {
        batch.push_title(b"hello");
        batch.push_repaint();
    });
    assert_eq!(
        drain(&f.events),
        vec![SessionEvent::Title("hello".into()), SessionEvent::Output]
    );
    assert_eq!(f.state.title().as_deref(), Some("hello"));
}

#[test]
fn reset_title_clears_cache() {
    let f = local(16);
    route(&f.router, |batch| {
        batch.push_title(b"x");
        batch.push(VtEvent::TitleReset);
    });
    assert_eq!(f.state.title(), None);
    assert_eq!(
        drain(&f.events).last(),
        Some(&SessionEvent::Title(String::new()))
    );
}

#[test]
fn local_clipboard_store_caches_and_forwards() {
    let f = local(16);
    route(&f.router, |batch| {
        batch.push_clipboard_store(ClipboardKind::Clipboard, b"secret");
    });
    assert_eq!(
        drain(&f.events),
        vec![SessionEvent::Clipboard(Some("secret".into()))]
    );
    assert_eq!(f.state.clipboard().as_deref(), Some("secret"));
}

/// SEC-08: the same router applies the origin policy for both backends —
/// remote OSC 52 writes and reads are refused by default.
#[test]
fn remote_clipboard_is_refused_by_default_policy() {
    let f = fixture(ClipboardOrigin::Remote, 16);
    route(&f.router, |batch| {
        batch.push_clipboard_store(ClipboardKind::Clipboard, b"secret");
        batch.push(VtEvent::ClipboardLoad {
            selection: ClipboardKind::Clipboard,
        });
    });
    assert!(drain(&f.events).is_empty());
    assert_eq!(f.state.clipboard(), None);
}

#[test]
fn local_clipboard_load_forwards_read_request() {
    let f = local(16);
    route(&f.router, |batch| {
        batch.push(VtEvent::ClipboardLoad {
            selection: ClipboardKind::Clipboard,
        });
    });
    assert_eq!(drain(&f.events), vec![SessionEvent::ClipboardRead]);
}

#[test]
fn reply_goes_to_the_transport() {
    let f = local(16);
    route(&f.router, |batch| batch.push_reply(b"\x1b[?1;2c"));
    assert_eq!(f.transport.writes(), vec![b"\x1b[?1;2c".to_vec()]);
}

#[test]
fn reply_failure_is_logged_not_panicked() {
    let f = local(16);
    f.transport.fail_writes(true);
    route(&f.router, |batch| batch.push_reply(b"x"));
    assert!(f.transport.writes().is_empty());
}

/// R-37: the DA1 answer must not wait behind the rest of the batch — conhost
/// blocks for up to a second for it at session start, which is exactly when a
/// burst of output is arriving.
#[test]
fn replies_are_written_before_the_rest_of_the_batch_is_routed() {
    let f = local(1);
    // One slot, already taken: every reliable event in the batch defers.
    f.events_tx.try_send(SessionEvent::Output).unwrap();
    route(&f.router, |batch| {
        batch.push_title(b"slow");
        batch.push(VtEvent::Bell);
        batch.push_reply(b"\x1b[?62;4c");
    });
    assert_eq!(f.transport.writes(), vec![b"\x1b[?62;4c".to_vec()]);
    assert!(f.router.events().has_deferred_reliable());
}

#[test]
fn clear_screen_bumps_clear_epoch() {
    let f = local(16);
    let before = f.state.clear_epoch();
    route(&f.router, |batch| batch.push(VtEvent::ScreenCleared));
    assert_eq!(f.state.clear_epoch(), before + 1);
}

#[test]
fn osc7_cwd_forwards_and_caches() {
    let f = local(16);
    route(&f.router, |batch| {
        osc(batch, &[b"7", b"file:///tmp"]);
    });
    assert_eq!(
        drain(&f.events),
        vec![SessionEvent::Cwd(std::path::PathBuf::from("/tmp"))]
    );
    // The shared state is what `TerminalCapabilities::cwd_source` hands out.
    assert_eq!(f.state.cwd().as_deref(), Some(std::path::Path::new("/tmp")));
}

#[test]
fn osc133_prompt_forwards_and_counts() {
    let f = local(16);
    route(&f.router, |batch| {
        osc(batch, &[b"133", b"A"]);
        osc(batch, &[b"133", b"D", b"3"]);
    });
    let events = drain(&f.events);
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], SessionEvent::ShellIntegration(_)));
    assert_eq!(f.state.prompt_count(), 1);
    assert_eq!(f.state.lock().last_exit_code, Some(3));
}

#[test]
fn osc97_agent_status_forwards() {
    let f = local(16);
    let json = stringify!(
        {"v":1,"agent":"pi","type":"state",
         "seq":1,"ts":1700000000000,
         "state":"working","message":"hi"}
    );
    let params = crate::osc_agent::encode_osc97_params(json);
    route(&f.router, |batch| {
        let refs: Vec<&[u8]> = params.iter().map(Vec::as_slice).collect();
        osc(batch, &refs);
    });
    match f.events.try_recv().unwrap() {
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
    let f = local(16);
    let send = |seq: u64| {
        let json = format!(
            "{{\"v\":1,\"agent\":\"pi\",\"type\":\"state\",
             \"seq\":{seq},\"ts\":1700000000000,
             \"state\":\"working\"}}"
        );
        let params = crate::osc_agent::encode_osc97_params(&json);
        route(&f.router, |batch| {
            let refs: Vec<&[u8]> = params.iter().map(Vec::as_slice).collect();
            osc(batch, &refs);
        });
    };
    send(5);
    assert!(matches!(
        f.events.try_recv().unwrap(),
        SessionEvent::AgentStatus(_)
    ));
    send(5);
    send(3);
    assert!(f.events.try_recv().is_err());
    send(6);
    assert!(matches!(
        f.events.try_recv().unwrap(),
        SessionEvent::AgentStatus(_)
    ));
}

/// Row bookkeeping is a `RowId`-keyed consumer's business, and nothing above
/// the seam speaks `RowId` until `US-0085`: these three must reach the UI as
/// nothing at all rather than as a stray repaint.
#[test]
fn row_events_are_not_forwarded() {
    let f = local(16);
    route(&f.router, |batch| {
        batch.push(VtEvent::RowsTrimmed {
            oldest: oneterm_vt::grid::RowId(3),
        });
        batch.push(VtEvent::GraphicReleased(oneterm_vt::intern::GraphicId(1)));
    });
    assert!(drain(&f.events).is_empty());
}

// ── Event sink: delivery policy ──────────────────────────────────────────

#[test]
fn coalescible_repaint_events_are_counted_when_saturated() {
    let f = local(1);
    f.events_tx.try_send(SessionEvent::Output).unwrap();

    f.router.forward(SessionEvent::Output);

    assert_eq!(f.router.events().diagnostics().event_full, 1);
    assert_eq!(f.events.len(), 1);

    f.events.close();
    f.router.forward(SessionEvent::Output);
    assert_eq!(f.router.events().diagnostics().event_closed, 1);
}

/// TEST-06 / CORR-01: the drain still runs with the engine lock held (the
/// backends' read loops are `US-0083` / `US-0084`'s), so `forward` must return
/// even when the queue is saturated and nobody drains it — the UI thread needs
/// that lock to drain the queue, so blocking here would deadlock the app.
#[test]
fn reliable_events_do_not_block_while_the_engine_lock_is_held() {
    let f = local(1);
    let term = new_term();
    f.events_tx.try_send(SessionEvent::Output).unwrap();
    let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);

    let pump = {
        let term = term.clone();
        let router = f.router.clone();
        std::thread::spawn(move || {
            let guard = term.lock();
            // Drain context: lock held, queue full, no consumer.
            for _ in 0..3 {
                router.forward(SessionEvent::Bell);
            }
            drop(guard);
            finished_tx.send(()).unwrap();
        })
    };

    finished_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("forward must return while the queue is saturated");
    pump.join().unwrap();
    assert!(f.router.events().has_deferred_reliable());
    // Nothing was lost: the queue still holds the repaint hint only.
    assert_eq!(f.events.len(), 1);
    assert_eq!(f.router.events().diagnostics().event_closed, 0);
}

/// Deferred reliable events are delivered by the flush after the batch, which
/// waits for queue capacity (backpressure) outside the engine lock.
#[test]
fn deferred_events_flush_in_order_once_the_queue_drains() {
    let f = local(1);
    f.events_tx.try_send(SessionEvent::Output).unwrap();
    f.router.forward(SessionEvent::Bell);
    f.router.forward(SessionEvent::Title("t".into()));
    assert!(f.router.events().has_deferred_reliable());
    assert_eq!(f.events.len(), 1);

    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
    let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);
    let flusher = {
        let sink = f.router.events().clone();
        std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            sink.flush_reliable_blocking();
            finished_tx.send(()).unwrap();
        })
    };

    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(finished_rx.recv_timeout(Duration::from_millis(20)).is_err());
    assert_eq!(f.events.try_recv().unwrap(), SessionEvent::Output);
    assert_eq!(
        recv_within(&f.events, Duration::from_secs(1)),
        SessionEvent::Bell
    );
    assert_eq!(
        recv_within(&f.events, Duration::from_secs(1)),
        SessionEvent::Title("t".into())
    );
    finished_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    flusher.join().unwrap();
    assert!(!f.router.events().has_deferred_reliable());
}

/// Once an event is deferred, later reliable events queue behind it even if the
/// channel has room again — FIFO order must survive the deferral.
#[test]
fn deferred_events_keep_fifo_order() {
    let f = local(2);
    f.events_tx.try_send(SessionEvent::Output).unwrap();
    f.events_tx.try_send(SessionEvent::Output).unwrap();
    f.router.forward(SessionEvent::Bell);
    assert_eq!(f.events.try_recv().unwrap(), SessionEvent::Output);
    assert_eq!(f.events.try_recv().unwrap(), SessionEvent::Output);
    // Room again, but Bell is still pending: Title must not jump ahead.
    f.router.forward(SessionEvent::Title("t".into()));
    assert_eq!(f.events.len(), 0);
    f.router.events().flush_reliable_blocking();
    assert_eq!(f.events.try_recv().unwrap(), SessionEvent::Bell);
    assert_eq!(
        f.events.try_recv().unwrap(),
        SessionEvent::Title("t".into())
    );
}

#[test]
fn async_flush_delivers_deferred_events() {
    let f = local(1);
    f.events_tx.try_send(SessionEvent::Output).unwrap();
    f.router.forward(SessionEvent::Bell);
    assert!(f.router.events().has_deferred_reliable());
    assert_eq!(f.events.try_recv().unwrap(), SessionEvent::Output);
    futures_lite_block_on(f.router.events().flush_reliable());
    assert_eq!(f.events.try_recv().unwrap(), SessionEvent::Bell);
    assert!(!f.router.events().has_deferred_reliable());
}

/// Minimal executor for the async sink/pump variants: the futures only await
/// `async_channel::send`, which completes without a reactor once the queue
/// has room, so a spin-poll is enough for tests.
fn futures_lite_block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    let mut context = Context::from_waker(Waker::noop());
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        std::thread::yield_now();
    }
}

// ── Line accounting ──────────────────────────────────────────────────────

#[test]
fn line_accounting_tracks_growth_saturation_and_reset() {
    let mut lines = LineAccounting::new();
    // Fresh grid: total == screen.
    lines.observe(24, 24, b"");
    assert_eq!(lines.absolute(), 24);
    // Scrollback growing.
    lines.observe(30, 24, b"a\nb\nc\nd\ne\nf\n");
    assert_eq!(lines.absolute(), 30);
    // Scrollback full: total unchanged, count newlines.
    lines.observe(30, 24, b"x\ny\n");
    assert_eq!(lines.absolute(), 32);
    // Clear: total shrank, restart from it.
    lines.observe(24, 24, b"");
    assert_eq!(lines.absolute(), 24);
}

// ── Pump: end to end through the in-memory transport ─────────────────────

#[test]
fn pump_batch_orders_reliable_events_before_repaint() {
    let f = local(16);
    let term = new_term();
    let mut pump = TerminalPump::new(f.router.clone());

    pump.process_chunk(&term, b"\x1b]2;hello\x07\x07line\r\n");
    pump.finish_batch_blocking(true);

    assert_eq!(
        drain(&f.events),
        vec![
            SessionEvent::Title("hello".into()),
            SessionEvent::Bell,
            SessionEvent::Output,
            SessionEvent::Output,
        ],
        "the batch's own Repaint hint, then the pump's"
    );
    assert_eq!(f.state.title().as_deref(), Some("hello"));
    assert!(pump.absolute_line_count() >= 24);
    assert_eq!(f.state.absolute_line_count(), pump.absolute_line_count());
}

#[test]
fn pump_answers_color_queries_with_live_then_default_colors() {
    let f = local(16);
    let term = new_term();
    let mut pump = TerminalPump::new(f.router.clone());
    f.state.set_default_colors(DefaultColors {
        foreground: Some(Rgb {
            r: 0x11,
            g: 0x22,
            b: 0x33,
        }),
        background: None,
        cursor: None,
        ansi: None,
    });

    // OSC 11 sets the background, then OSC 10 and 11 are queried; OSC 12 has
    // neither a live value nor a default and must be skipped.
    pump.process_chunk(
        &term,
        b"\x1b]11;rgb:aaaa/bbbb/cccc\x07\x1b]10;?\x07\x1b]11;?\x07\x1b]12;?\x07",
    );
    pump.finish_batch_blocking(true);

    let writes: Vec<String> = f
        .transport
        .writes()
        .into_iter()
        .map(|w| String::from_utf8(w).unwrap())
        .collect();
    assert_eq!(writes.len(), 2, "{writes:?}");
    assert!(
        writes[0].starts_with("\x1b]10;rgb:1111/2222/3333"),
        "{writes:?}"
    );
    assert!(
        writes[1].starts_with("\x1b]11;rgb:aaaa/bbbb/cccc"),
        "{writes:?}"
    );
}

#[test]
fn pump_split_color_reply_steps_match_process_chunk() {
    let f = local(16);
    let term = new_term();
    let mut pump = TerminalPump::new(f.router.clone());
    assert!(!pump.has_color_queries());
    {
        let mut guard = term.lock();
        pump.advance(&mut guard, b"\x1b]11;rgb:0000/1111/2222\x07\x1b]11;?\x07");
        assert!(pump.has_color_queries());
        let queries = pump.take_color_queries();
        let replies = pump.color_replies(&guard, queries);
        drop(guard);
        pump.write_color_replies(replies);
    }
    assert!(!pump.has_color_queries());
    let writes = f.transport.writes();
    assert_eq!(writes.len(), 1);
    assert!(writes[0].starts_with(b"\x1b]11;rgb:0000/1111/2222"));
}

/// The typed `ColorKey` the engine reports is the index space `osc_color.rs`
/// still publishes, so a query round-trips without the 256/257/258 constants
/// drifting apart (`US-0082` deletes them).
#[test]
fn color_key_indices_match_the_adapter_constants() {
    assert_eq!(
        ColorKey::Foreground.index(),
        crate::osc_color::FOREGROUND_INDEX
    );
    assert_eq!(
        ColorKey::Background.index(),
        crate::osc_color::BACKGROUND_INDEX
    );
    assert_eq!(ColorKey::Cursor.index(), crate::osc_color::CURSOR_INDEX);
    assert_eq!(ColorKey::Palette(7).index(), 7);
}

#[test]
fn pump_publish_exit_and_closed_flush_deferred_first() {
    let f = local(1);
    let term = new_term();
    let mut pump = TerminalPump::new(f.router.clone());
    // Saturate the queue so the Bell from the batch is deferred.
    f.events_tx.try_send(SessionEvent::Output).unwrap();
    pump.process_chunk(&term, b"\x07");
    assert!(f.router.events().has_deferred_reliable());

    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
    let publisher = std::thread::spawn(move || {
        pump.publish_exit_blocking(Some(7));
        pump.publish_closed_blocking();
        done_tx.send(()).unwrap();
    });
    assert_eq!(
        recv_within(&f.events, Duration::from_secs(1)),
        SessionEvent::Output
    );
    assert_eq!(
        recv_within(&f.events, Duration::from_secs(1)),
        SessionEvent::Bell
    );
    assert_eq!(
        recv_within(&f.events, Duration::from_secs(1)),
        SessionEvent::Exited(Some(7))
    );
    assert_eq!(
        recv_within(&f.events, Duration::from_secs(1)),
        SessionEvent::Closed
    );
    done_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    publisher.join().unwrap();
    assert!(!f.state.alive());
    assert_eq!(f.state.exit_code(), Some(7));
}

#[test]
fn pump_async_variants_publish_lifecycle_in_order() {
    let f = local(16);
    let term = new_term();
    let mut pump = TerminalPump::new(f.router.clone());
    pump.process_chunk(&term, b"\x07");
    futures_lite_block_on(async {
        pump.finish_batch(true).await;
        pump.publish_exit(None).await;
        pump.publish_closed().await;
    });
    assert_eq!(
        drain(&f.events),
        vec![
            SessionEvent::Bell,
            SessionEvent::Output,
            SessionEvent::Output,
            SessionEvent::Exited(None),
            SessionEvent::Closed
        ]
    );
    assert!(!f.state.alive());
}

#[test]
fn shared_state_counters_are_lock_free_and_visible() {
    let state = SharedSessionState::new_alive();
    state.add_rx_bytes(10);
    state.add_tx_bytes(4);
    state.add_rx_bytes(5);
    let stats = state.net_stats();
    assert_eq!((stats.rx_bytes, stats.tx_bytes), (15, 4));
    state.set_absolute_line_count(99);
    assert_eq!(state.absolute_line_count(), 99);
    assert!(state.alive());
    state.record_exit(Some(2));
    assert!(!state.alive());
    assert_eq!(state.exit_code(), Some(2));
}

/// `Arc` is still what the pump and the session share, so a clone of the router
/// sees the same state — the property the deferred queue and the counters rely
/// on.
#[test]
fn router_clones_share_their_state() {
    let f = local(16);
    let clone = f.router.clone();
    clone.forward(SessionEvent::Bell);
    assert_eq!(drain(&f.events), vec![SessionEvent::Bell]);
    assert!(Arc::ptr_eq(f.router.state(), clone.state()));
}
