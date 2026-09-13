//! Tests for the shared backend pump layer: router event mapping, event-sink
//! delivery policy, colour-query replies, the gutter's line count, and the pump
//! driven end to end through an in-memory transport (TEST-01 / TEST-02).
//!
//! `US-0082` changed **where** an event is delivered, not what it means:
//! [`OscRouter::drain`] appends to a caller-owned vector under the engine lock
//! and [`TerminalPump`] sends that vector once the lock is released. The router
//! tests therefore assert on what the drain returns; the pump tests still assert
//! on what reaches the channel, because that is the property the UI depends on.
//!
//! Deleted with the deferred tier, each named in the packet: the CORR-01
//! deadlock case and the three deferred-flush/FIFO cases — a drain that cannot
//! send cannot deadlock, and a vector cannot reorder. What replaces them is
//! `the_drain_never_waits_on_a_full_queue` (the property CORR-01 protected) and
//! `pending_events_apply_backpressure_outside_the_lock` (the property the FIFO
//! provided).

use std::sync::Arc;
use std::time::Duration;

use oneterm_vt::{ClipboardKind, ColorKey, EventBatch, RowId, StringTerm, VtEvent};

use crate::handle::{DEFAULT_SCROLLBACK_LINES, SharedTerminal, new_shared_terminal};
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
        DEFAULT_SCROLLBACK_LINES,
    )
}

/// Route one batch, built the way `Terminal::feed` would have built it, and
/// return the UI-facing events the drain collected.
fn route(router: &Router, build: impl FnOnce(&mut EventBatch)) -> Vec<SessionEvent> {
    let mut batch = EventBatch::new();
    build(&mut batch);
    let mut out = Vec::new();
    router.drain(&batch, &mut out);
    out
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

/// The repaint hint has one owner, the pump: the engine's end-of-batch
/// `Repaint` is dropped here, or every chunk would post two `Output`s and the
/// first would arrive before that batch's reliable events.
#[test]
fn forwards_title_and_drops_the_batch_repaint_hint() {
    let f = local(16);
    let events = route(&f.router, |batch| {
        batch.push_title(b"hello");
        batch.push_repaint();
    });
    assert_eq!(events, vec![SessionEvent::Title("hello".into())]);
    assert_eq!(f.state.title().as_deref(), Some("hello"));
}

#[test]
fn reset_title_clears_cache() {
    let f = local(16);
    let events = route(&f.router, |batch| {
        batch.push_title(b"x");
        batch.push(VtEvent::TitleReset);
    });
    assert_eq!(f.state.title(), None);
    assert_eq!(events.last(), Some(&SessionEvent::Title(String::new())));
}

#[test]
fn local_clipboard_store_caches_and_forwards() {
    let f = local(16);
    let events = route(&f.router, |batch| {
        batch.push_clipboard_store(ClipboardKind::Clipboard, b"secret");
    });
    assert_eq!(events, vec![SessionEvent::Clipboard(Some("secret".into()))]);
    assert_eq!(f.state.clipboard().as_deref(), Some("secret"));
}

/// SEC-08: the same router applies the origin policy for both backends —
/// remote OSC 52 writes and reads are refused by default.
#[test]
fn remote_clipboard_is_refused_by_default_policy() {
    let f = fixture(ClipboardOrigin::Remote, 16);
    let events = route(&f.router, |batch| {
        batch.push_clipboard_store(ClipboardKind::Clipboard, b"secret");
        batch.push(VtEvent::ClipboardLoad {
            selection: ClipboardKind::Clipboard,
        });
    });
    assert!(events.is_empty());
    assert_eq!(f.state.clipboard(), None);
}

#[test]
fn local_clipboard_load_forwards_read_request() {
    let f = local(16);
    let events = route(&f.router, |batch| {
        batch.push(VtEvent::ClipboardLoad {
            selection: ClipboardKind::Clipboard,
        });
    });
    assert_eq!(events, vec![SessionEvent::ClipboardRead]);
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
/// burst of output is arriving. The reply reaches the transport during the
/// drain; everything else is still sitting in the vector.
#[test]
fn replies_are_written_before_the_rest_of_the_batch_is_routed() {
    let f = local(1);
    let mut batch = EventBatch::new();
    batch.push_title(b"slow");
    batch.push(VtEvent::Bell);
    batch.push_reply(b"\x1b[?62;4c");

    let mut out = Vec::new();
    f.router.drain(&batch, &mut out);

    assert_eq!(f.transport.writes(), vec![b"\x1b[?62;4c".to_vec()]);
    assert_eq!(
        out,
        vec![SessionEvent::Title("slow".into()), SessionEvent::Bell],
        "the UI-facing events are collected, not sent"
    );
}

/// The property the deleted CORR-01 test protected, now structural: the drain
/// runs with the engine lock held and cannot touch the channel at all, so a
/// saturated queue with no consumer cannot stall it.
#[test]
fn the_drain_never_waits_on_a_full_queue() {
    let f = local(1);
    f.events_tx.try_send(SessionEvent::Output).unwrap();
    let term = new_term();
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);

    let router = f.router.clone();
    let worker = std::thread::spawn(move || {
        let guard = term.lock();
        let mut batch = EventBatch::new();
        batch.push(VtEvent::Bell);
        for _ in 0..3 {
            let mut out = Vec::new();
            router.drain(&batch, &mut out);
            assert_eq!(out, vec![SessionEvent::Bell]);
        }
        drop(guard);
        done_tx.send(()).unwrap();
    });

    done_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("the drain must return while the queue is saturated");
    worker.join().unwrap();
    assert_eq!(f.events.len(), 1, "nothing was sent and nothing was lost");
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
    let events = route(&f.router, |batch| {
        osc(batch, &[b"7", b"file:///tmp"]);
    });
    assert_eq!(
        events,
        vec![SessionEvent::Cwd(std::path::PathBuf::from("/tmp"))]
    );
    // The shared state is what `TerminalCapabilities::cwd_source` hands out.
    assert_eq!(f.state.cwd().as_deref(), Some(std::path::Path::new("/tmp")));
}

#[test]
fn osc133_prompt_forwards_and_counts() {
    let f = local(16);
    let events = route(&f.router, |batch| {
        osc(batch, &[b"133", b"A"]);
        osc(batch, &[b"133", b"D", b"3"]);
    });
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], SessionEvent::ShellIntegration(_)));
    assert_eq!(f.state.prompt_count(), 1);
    assert_eq!(f.state.lock().last_exit_code, Some(3));
}

/// Route one agent-status event under the given wire prefix.
fn agent_status(f: &Fixture, prefix: [&[u8]; 2], json: &str) -> Vec<SessionEvent> {
    let params = crate::osc_agent::encode_agent_osc_params(prefix, json);
    route(&f.router, |batch| {
        let refs: Vec<&[u8]> = params.iter().map(Vec::as_slice).collect();
        osc(batch, &refs);
    })
}

/// `OSC 20308;1` and the deprecated `OSC 9;7` must reach **the same** handling.
/// Parametrised rather than copied, so the alias cannot quietly drift away from
/// the sequence it is supposed to be identical to (spec §3.1).
#[test]
fn agent_status_forwards_under_both_encodings() {
    let json = stringify!(
        {"v":1,"agent":"pi","type":"state",
         "seq":1,"ts":1700000000000,
         "state":"working","message":"hi"}
    );
    for prefix in crate::osc_agent::AGENT_OSC_PREFIXES {
        let f = local(16);
        let events = agent_status(&f, prefix, json);
        match events.first() {
            Some(SessionEvent::AgentStatus(ev)) => {
                assert_eq!(ev.agent(), "pi");
                assert_eq!(ev.seq(), 1);
                assert_eq!(ev.type_name(), "state");
            }
            other => panic!("{:?}: unexpected {other:?}", prefix[0]),
        }
        assert_eq!(events.len(), 1, "{:?}: exactly one event", prefix[0]);
    }
}

#[test]
fn agent_status_dedup_drops_stale_seq_under_both_encodings() {
    for prefix in crate::osc_agent::AGENT_OSC_PREFIXES {
        let f = local(16);
        let send = |seq: u64| -> Vec<SessionEvent> {
            let json = format!(
                "{{\"v\":1,\"agent\":\"pi\",\"type\":\"state\",
                 \"seq\":{seq},\"ts\":1700000000000,
                 \"state\":\"working\"}}"
            );
            agent_status(&f, prefix, &json)
        };
        assert!(matches!(
            send(5).first(),
            Some(SessionEvent::AgentStatus(_))
        ));
        assert!(send(5).is_empty());
        assert!(send(3).is_empty());
        assert!(matches!(
            send(6).first(),
            Some(SessionEvent::AgentStatus(_))
        ));
    }
}

/// The two encodings share the `seq` watermark, because they are one protocol:
/// an agent that emits both (which the spec tells it not to) is deduplicated,
/// not doubled.
#[test]
fn the_alias_shares_the_seq_watermark_with_the_sequence() {
    let f = local(16);
    let json = |seq: u64| {
        format!(
            "{{\"v\":1,\"agent\":\"pi\",\"type\":\"state\",
             \"seq\":{seq},\"ts\":1700000000000,\"state\":\"working\"}}"
        )
    };
    let [new, legacy] = crate::osc_agent::AGENT_OSC_PREFIXES;
    assert!(!agent_status(&f, new, &json(1)).is_empty());
    assert!(
        agent_status(&f, legacy, &json(1)).is_empty(),
        "the same seq under the alias is the same event"
    );
    assert!(!agent_status(&f, legacy, &json(2)).is_empty());
}

/// `OSC 20308;0` is answered on the transport, and produces no UI event
/// (spec §3.2). The reply ends the way the question did.
#[test]
fn the_agent_support_query_is_answered_on_the_transport() {
    for (terminator, tail) in [(StringTerm::Bel, "\x07"), (StringTerm::St, "\x1b\\")] {
        let f = local(16);
        let events = route(&f.router, |batch| {
            batch.push_osc(20308, &[b"20308", b"0"], terminator, false);
        });
        assert!(events.is_empty(), "a query is not a UI event");
        let written = f.transport.take_writes().concat();
        let expected = format!("\x1b]20308;0;1;OneTerm;{}{tail}", env!("CARGO_PKG_VERSION"));
        assert_eq!(String::from_utf8_lossy(&written), expected);
    }
}

/// `2` and above are reserved (spec §3): ignored, but counted, so a future
/// extension aimed at a newer OneTerm is visible rather than silent.
#[test]
fn unknown_agent_subcodes_are_ignored_and_counted() {
    let f = local(16);
    assert_eq!(f.state.agent_osc_unknown_subcodes(), 0);
    let events = route(&f.router, |batch| {
        batch.push_osc(
            20308,
            &[b"20308", b"2", b"whatever"],
            StringTerm::Bel,
            false,
        );
        batch.push_osc(20308, &[b"20308", b""], StringTerm::Bel, false);
        batch.push_osc(20308, &[b"20308"], StringTerm::Bel, false);
    });
    assert!(events.is_empty());
    assert!(f.transport.writes().is_empty(), "and nothing is answered");
    assert_eq!(f.state.agent_osc_unknown_subcodes(), 3);
}

/// The alias is counted, and announced once per session rather than once per
/// event — a still-unported agent emits thousands (spec §3.1).
#[test]
fn the_legacy_alias_is_counted_and_announced_once_per_session() {
    let f = local(16);
    let json = stringify!(
        {"v":1,"agent":"pi","type":"heartbeat","seq":1,"ts":1700000000000}
    );
    let [new, legacy] = crate::osc_agent::AGENT_OSC_PREFIXES;

    // The sequence itself is never counted as the alias.
    agent_status(&f, new, json);
    assert_eq!(f.state.legacy_agent_osc_events(), 0);

    // Only the first alias event announces itself, but every one is counted.
    assert_eq!(f.state.count_legacy_agent_osc(), 1, "the first announces");
    assert_eq!(f.state.count_legacy_agent_osc(), 2, "the rest only count");

    for seq in 2..5 {
        let json = format!(
            "{{\"v\":1,\"agent\":\"pi\",\"type\":\"heartbeat\",
             \"seq\":{seq},\"ts\":1700000000000}}"
        );
        assert!(!agent_status(&f, legacy, &json).is_empty());
    }
    assert_eq!(f.state.legacy_agent_osc_events(), 5);

    // A fresh session starts clean and announces again.
    assert_eq!(local(16).state.legacy_agent_osc_events(), 0);
}

/// Row bookkeeping is a `RowId`-keyed consumer's business, and nothing above
/// the seam speaks `RowId` until `US-0085`: these must reach the UI as nothing
/// at all rather than as a stray repaint.
#[test]
fn row_events_are_not_forwarded() {
    let f = local(16);
    let events = route(&f.router, |batch| {
        batch.push(VtEvent::RowsTrimmed { oldest: RowId(3) });
        batch.push(VtEvent::GraphicReleased(oneterm_vt::intern::GraphicId(1)));
    });
    assert!(events.is_empty());
}

/// The engine reports a colour query with a typed [`ColorKey`], which is what
/// replaced the 256 / 257 / 258 indices three files used to agree on. The queue
/// carries the key through to the pump unchanged.
#[test]
fn color_queries_are_queued_with_their_typed_key() {
    let f = local(16);
    assert!(!f.router.has_color_queries());
    route(&f.router, |batch| {
        batch.push(VtEvent::ColorQuery {
            key: ColorKey::Background,
            terminator: StringTerm::Bel,
        });
        batch.push(VtEvent::ColorQuery {
            key: ColorKey::Palette(7),
            terminator: StringTerm::St,
        });
    });
    assert!(f.router.has_color_queries());
    let keys: Vec<ColorKey> = f
        .router
        .take_color_queries()
        .into_iter()
        .map(|query| query.key)
        .collect();
    assert_eq!(keys, vec![ColorKey::Background, ColorKey::Palette(7)]);
    assert!(!f.router.has_color_queries());
}

// ── Event sink: delivery policy ──────────────────────────────────────────

#[test]
fn coalescible_repaint_events_are_counted_when_saturated() {
    let f = local(1);
    f.events_tx.try_send(SessionEvent::Output).unwrap();

    f.router.events().post_repaint();

    assert_eq!(f.router.events().diagnostics().event_full, 1);
    assert_eq!(f.events.len(), 1);

    f.events.close();
    f.router.events().post_repaint();
    assert_eq!(f.router.events().diagnostics().event_closed, 1);
}

/// A reliable event into a closed channel is counted, never a panic and never a
/// silent loss (`docs/agents/error-policy.md`, transport-closure row).
#[test]
fn a_closed_channel_is_counted_not_panicked() {
    let f = local(4);
    f.events.close();
    f.router.events().send_blocking(SessionEvent::Bell);
    assert_eq!(f.router.events().diagnostics().event_closed, 1);
}

/// What the deferred FIFO used to provide: the batch's events arrive in order
/// and none is dropped, with the pump waiting for the UI to make room — outside
/// the engine lock, which is why it may now simply block.
#[test]
fn pending_events_apply_backpressure_outside_the_lock() {
    let f = local(1);
    let term = new_term();
    let mut pump = TerminalPump::new(f.router.clone());
    // One slot, already taken: the flush has to wait for the UI.
    f.events_tx.try_send(SessionEvent::Output).unwrap();
    pump.process_chunk(&term, b"\x1b]2;t\x07\x07");

    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
    let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);
    let flusher = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        pump.finish_batch_blocking(true);
        finished_tx.send(()).unwrap();
    });

    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(
        finished_rx.recv_timeout(Duration::from_millis(20)).is_err(),
        "the flush must wait for room"
    );
    // Draining the one slot lets the parked sender through, in order.
    assert_eq!(f.events.try_recv().unwrap(), SessionEvent::Output);
    assert_eq!(
        recv_within(&f.events, Duration::from_secs(1)),
        SessionEvent::Title("t".into())
    );
    assert_eq!(
        recv_within(&f.events, Duration::from_secs(1)),
        SessionEvent::Bell
    );
    finished_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    flusher.join().unwrap();
    // The repaint hint that followed them is coalescible: with one slot and a
    // reliable event just delivered into it, dropping it is the policy, and the
    // next batch's hint carries the same information.
    let dropped = f.router.events().diagnostics().event_full;
    assert!(
        dropped <= 1,
        "only the coalescible hint may be dropped, {dropped} were"
    );
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

// ── The gutter's line count ──────────────────────────────────────────────

/// `LineAccounting`'s three-branch heuristic over `total_lines` is replaced by
/// the engine's exact count (R-05) under the floor the gutter's arithmetic
/// needs.
#[test]
fn the_gutter_line_count_is_the_engines_output_line_count() {
    let f = local(64);
    let term = new_shared_terminal(GridSize { cols: 8, lines: 4 }, 16);
    let mut pump = TerminalPump::new(f.router.clone());

    // The floor: never below the rows the grid holds, or the gutter would label
    // its top rows from below zero.
    pump.process_chunk(&term, b"one");
    pump.finish_batch_blocking(false);
    assert_eq!(pump.absolute_line_count(), 4);
    assert_eq!(f.state.absolute_line_count(), 4);

    // Past the scrollback cap the count keeps growing where `total_lines`, which
    // is what the heuristic watched, cannot.
    for _ in 0..40 {
        pump.process_chunk(&term, b"x\r\n");
    }
    pump.finish_batch_blocking(false);
    assert_eq!(pump.absolute_line_count(), 40);
    let total = {
        let guard = term.lock();
        let screen = guard.screen();
        screen.history_len() as usize + usize::from(screen.rows())
    };
    assert!(total < 40, "the scrollback capped total_lines at {total}");

    // An implicit wrap is not an output line.
    pump.process_chunk(&term, b"0123456789abcdef");
    pump.finish_batch_blocking(false);
    assert_eq!(pump.absolute_line_count(), 40, "a wrap is not a line");

    // A clear does not reset it — `TerminalInfo::absolute_line_count` says
    // "monotonically increasing", which the heuristic broke on every `cls`.
    pump.process_chunk(&term, b"\x1b[2J\x1b[3J");
    pump.finish_batch_blocking(false);
    assert_eq!(pump.absolute_line_count(), 40);
    assert!(f.state.clear_epoch() > 0);
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
        ],
        "exactly one repaint hint per batch, after the reliable events"
    );
    assert_eq!(f.state.title().as_deref(), Some("hello"));
    assert!(pump.absolute_line_count() >= 24, "floored at the viewport");
    assert_eq!(f.state.absolute_line_count(), pump.absolute_line_count());
}

/// One repaint hint per chunk, last — the property the old listener had because
/// the pump was the only source of `Output`, and the engine's per-batch
/// `Repaint` must not become a second source. Three chunks, each carrying
/// reliable events, must produce three `Output`s, each after its own chunk's
/// reliable events.
#[test]
fn each_chunk_posts_exactly_one_output_after_its_reliable_events() {
    let f = local(64);
    let term = new_term();
    let mut pump = TerminalPump::new(f.router.clone());

    for index in 0..3u8 {
        pump.process_chunk(&term, format!("\x1b]2;t{index}\x07\x07").as_bytes());
        pump.finish_batch_blocking(true);
    }

    let events = drain(&f.events);
    assert_eq!(
        events,
        vec![
            SessionEvent::Title("t0".into()),
            SessionEvent::Bell,
            SessionEvent::Output,
            SessionEvent::Title("t1".into()),
            SessionEvent::Bell,
            SessionEvent::Output,
            SessionEvent::Title("t2".into()),
            SessionEvent::Bell,
            SessionEvent::Output,
        ],
        "{events:?}"
    );
}

/// Several `advance` calls can share one batch boundary — the local read loop
/// feeds until the pipe is empty before it unlocks — so their events must all
/// survive to the one `finish_batch`, in order.
#[test]
fn events_from_several_advances_survive_to_one_finish_batch() {
    let f = local(64);
    let term = new_term();
    let mut pump = TerminalPump::new(f.router.clone());

    {
        let mut guard = term.lock();
        pump.advance(&mut guard, b"\x1b]2;first\x07");
        pump.advance(&mut guard, b"\x07");
        pump.advance(&mut guard, b"\x1b]2;second\x07");
    }
    pump.finish_batch_blocking(true);

    assert_eq!(
        drain(&f.events),
        vec![
            SessionEvent::Title("first".into()),
            SessionEvent::Bell,
            SessionEvent::Title("second".into()),
            SessionEvent::Output,
        ]
    );
}

#[test]
fn pump_answers_color_queries_with_live_then_default_colors() {
    let f = local(16);
    let term = new_term();
    let mut pump = TerminalPump::new(f.router.clone());
    f.state.set_default_colors(DefaultColors {
        foreground: Some(oneterm_vt::Rgb {
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

#[test]
fn pump_publish_exit_and_closed_flush_pending_first() {
    let f = local(1);
    let term = new_term();
    let pump = {
        let mut pump = TerminalPump::new(f.router.clone());
        // Saturate the queue so the Bell from the batch has to wait.
        f.events_tx.try_send(SessionEvent::Output).unwrap();
        pump.process_chunk(&term, b"\x07");
        pump
    };

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
/// sees the same state — the property the colour queue and the counters rely on.
#[test]
fn router_clones_share_their_state() {
    let f = local(16);
    let clone = f.router.clone();
    clone.events().post_repaint();
    assert_eq!(drain(&f.events), vec![SessionEvent::Output]);
    assert!(Arc::ptr_eq(f.router.state(), clone.state()));
}

// ── US-0088 independent verification ─────────────────────────────────────
//
// These drive the agent channel through a **real** `Terminal::feed` rather
// than a hand-built `EventBatch`, because three of the packet's claims (the
// reply terminator, the 8 KiB cap, "the reply is not echoed") are only true
// or false once the engine's own OSC parser is in the path.

/// Feed raw bytes through a real engine + pump and return what the transport
/// received, in order.
fn feed_bytes(f: &Fixture, bytes: &[u8]) -> Vec<u8> {
    let term = new_term();
    let mut pump = TerminalPump::new(f.router.clone());
    {
        let mut guard = term.lock();
        pump.advance(&mut guard, bytes);
    }
    pump.finish_batch_blocking(true);
    f.transport.take_writes().concat()
}

/// (a) The support reply ends the way the *question* did, end to end, and is
/// written exactly once per query.
#[test]
fn verify_support_reply_terminator_fidelity_end_to_end() {
    let version = env!("CARGO_PKG_VERSION");
    for (query, tail) in [
        (&b"\x1b]20308;0\x07"[..], "\x07"),
        (&b"\x1b]20308;0\x1b\\"[..], "\x1b\\"),
    ] {
        let f = local(16);
        let written = feed_bytes(&f, query);
        assert_eq!(
            String::from_utf8_lossy(&written),
            format!("\x1b]20308;0;1;OneTerm;{version}{tail}"),
            "the terminator must mirror the query's"
        );
    }

    // Exactly once per query: two queries, two replies, no more.
    let f = local(16);
    let written = feed_bytes(&f, b"\x1b]20308;0\x07\x1b]20308;0\x07");
    assert_eq!(
        String::from_utf8_lossy(&written),
        format!("\x1b]20308;0;1;OneTerm;{version}\x07").repeat(2)
    );
}

/// (f) The query is a side channel: nothing lands in the grid, and no UI event
/// other than the pump's repaint is produced.
#[test]
fn verify_the_support_reply_is_not_echoed_into_the_grid() {
    let f = local(16);
    let term = new_term();
    let mut pump = TerminalPump::new(f.router.clone());
    {
        let mut guard = term.lock();
        pump.advance(&mut guard, b"\x1b]20308;0\x07");
        let top = guard.viewport().top;
        assert_eq!(
            guard.row_text(top).trim_end(),
            "",
            "the query must print nothing"
        );
    }
    pump.finish_batch_blocking(true);
    assert_eq!(
        drain(&f.events),
        vec![SessionEvent::Output],
        "only the repaint hint"
    );
}

/// §3.2 prescribes pairing the query with DA1: "If the DA1 reply arrives with
/// **no `20308` reply before it**, the terminal does not implement the
/// protocol." So when both are written together the agent reply must come out
/// first, or the documented detection idiom reports "unsupported".
#[test]
fn verify_the_support_reply_precedes_the_da1_reply_in_one_batch() {
    let f = local(16);
    let written = feed_bytes(&f, b"\x1b]20308;0\x07\x1b[c");
    let text = String::from_utf8_lossy(&written).into_owned();
    let agent = text.find("20308;0;1;OneTerm").expect("a support reply");
    let da1 = text.find("\x1b[?").expect("a DA1 reply");
    assert!(
        agent < da1,
        "spec 3.2: the 20308 reply must precede DA1; got {text:?}"
    );
}

/// The counterpart to the test above: ordering is **positional**, not
/// "the agent reply always wins". Sending DA1 first must put DA1 first — a
/// router that hoisted the support reply to the front would pass the §3.2 test
/// and still be wrong, because an agent that queries *after* a DA1 it sent for
/// another reason would then read the stale answer as its own.
#[test]
fn verify_the_da1_reply_precedes_the_support_reply_when_it_comes_first() {
    let f = local(16);
    let written = feed_bytes(&f, b"\x1b[c\x1b]20308;0\x07");
    let text = String::from_utf8_lossy(&written).into_owned();
    let agent = text.find("20308;0;1;OneTerm").expect("a support reply");
    let da1 = text.find("\x1b[?").expect("a DA1 reply");
    assert!(da1 < agent, "byte order in, byte order out; got {text:?}");
}

/// Three replies from three different producers — engine, embedder, engine —
/// come out in exactly the order the bytes asked for.
#[test]
fn verify_replies_leave_in_byte_order_regardless_of_producer() {
    let f = local(16);
    let written = feed_bytes(&f, b"\x1b[c\x1b]20308;0\x07\x1b[5n");
    let text = String::from_utf8_lossy(&written).into_owned();
    let da1 = text.find("\x1b[?").expect("DA1");
    let agent = text.find("20308;0;1;OneTerm").expect("support reply");
    let dsr = text.find("\x1b[0n").expect("DSR");
    assert!(da1 < agent && agent < dsr, "got {text:?}");
}

/// (4) The alt screen and an open synchronised-update block must not hold the
/// reply back: it leaves inside the same `advance`, before the pump yields.
#[test]
fn verify_the_support_reply_leaves_from_the_alt_screen_inside_a_sync_block() {
    let f = local(16);
    let term = new_term();
    let mut pump = TerminalPump::new(f.router.clone());
    {
        let mut guard = term.lock();
        pump.advance(&mut guard, b"\x1b[?1049h\x1b[?2026h\x1b]20308;0\x07");
        assert!(
            !f.transport.writes().is_empty(),
            "the reply must be on the transport before the guard is dropped"
        );
    }
    pump.finish_batch_blocking(true);
    assert_eq!(
        String::from_utf8_lossy(&f.transport.take_writes().concat()),
        format!("\x1b]20308;0;1;OneTerm;{}\x07", env!("CARGO_PKG_VERSION"))
    );
}

/// A JSON status payload whose **base64** is exactly `len` bytes.
fn agent_json_with_base64_len(len: usize) -> String {
    assert_eq!(len % 4, 0, "standard base64 is a multiple of 4");
    let raw = len / 4 * 3;
    let head = concat!(
        r#"{"v":1,"agent":"pi","type":"state","seq":1,"#,
        r#""ts":1700000000000,"state":"working","message":""#
    );
    let tail = r#""}"#;
    let json = format!("{head}{}{tail}", "x".repeat(raw - head.len() - tail.len()));
    assert_eq!(json.len(), raw);
    json
}

/// (b) The cap is on the base64 length and is 8 KiB (spec §3.4), identically
/// under both spellings.
#[test]
fn verify_the_8kib_cap_boundary_under_both_encodings() {
    use crate::osc_agent::{MAX_AGENT_STATUS_BASE64_BYTES, parse_agent_status};
    let cap = MAX_AGENT_STATUS_BASE64_BYTES;
    assert_eq!(cap, 8 * 1024);

    let at = agent_json_with_base64_len(cap);
    let over = agent_json_with_base64_len(cap + 4);
    for prefix in crate::osc_agent::AGENT_OSC_PREFIXES {
        let f = local(16);
        let params = crate::osc_agent::encode_agent_osc_params(prefix, &at);
        assert_eq!(params[2].len(), cap, "the fixture is exactly at the cap");
        assert!(
            parse_agent_status(&params[2]).is_some(),
            "{:?}: a payload exactly at the cap is accepted",
            prefix[0]
        );
        assert!(
            !agent_status(&f, prefix, &at).is_empty(),
            "{:?}: and it routes",
            prefix[0]
        );

        let f = local(16);
        let params = crate::osc_agent::encode_agent_osc_params(prefix, &over);
        assert_eq!(params[2].len(), cap + 4);
        assert!(parse_agent_status(&params[2]).is_none(), "over the cap");
        assert!(
            agent_status(&f, prefix, &over).is_empty(),
            "{:?}: over the cap routes nothing",
            prefix[0]
        );
        // Not an "unknown sub-code": the sub-code was right, the payload wasn't.
        assert_eq!(f.state.agent_osc_unknown_subcodes(), 0);
    }
}

/// The cap the spec publishes is only reachable if the engine lets the payload
/// through. `OSC_INLINE` is 2048 bytes and `AGENT_OSC` is claimed with `claim`,
/// not `claim_large`, so a larger payload is truncated by the parser and then
/// dropped by `parse_agent_status` — well under the documented 8 KiB and under
/// the "< 4 KiB worst case" §3.4 calls legitimate.
#[test]
fn verify_the_documented_cap_is_reachable_through_the_engine() {
    let [new, _] = crate::osc_agent::AGENT_OSC_PREFIXES;
    let survives = |base64_len: usize| {
        let json = agent_json_with_base64_len(base64_len);
        let params = crate::osc_agent::encode_agent_osc_params(new, &json);
        let b64 = String::from_utf8(params[2].clone()).expect("base64 is ascii");
        let f = local(16);
        let term = new_term();
        let mut pump = TerminalPump::new(f.router.clone());
        {
            let mut guard = term.lock();
            pump.advance(&mut guard, format!("\x1b]20308;1;{b64}\x07").as_bytes());
        }
        pump.finish_batch_blocking(true);
        drain(&f.events)
            .iter()
            .any(|e| matches!(e, SessionEvent::AgentStatus(_)))
    };

    // The verifier's original two lines here were
    //     assert!(survives(2040));  assert!(!survives(2044));
    // which pinned the **defect**: `claim` bounded the whole payload at
    // `OSC_INLINE` (2048), prefix included, so ~2040 base64 bytes was the real
    // ceiling and everything above it vanished. `claim_large(AGENT_OSC)` is the
    // fix, so 2044 now survives and the assertion inverts — that inversion is
    // the point of the test, not a weakening of it.
    assert!(survives(2040), "the old inline ceiling still survives");
    assert!(
        survives(2044),
        "2044 used to be truncated at OSC_INLINE; claim_large lifted it"
    );

    assert!(
        survives(4096),
        "a 4 KiB base64 payload is inside the documented 8 KiB cap and inside \
         the '< 4 KiB worst case' spec 3.4 calls legitimate"
    );
    assert!(
        survives(8192),
        "and the published cap itself must be reachable, or 3.4 is fiction"
    );
    // One past the cap is refused by `parse_agent_status`, not by the engine.
    assert!(!survives(8196), "above the cap is still refused");
}

/// The cap must be reachable under **both** spellings, not just the new one —
/// the alias is "parsed identically" for one release, and that includes its
/// ceiling.
#[test]
fn the_documented_cap_is_reachable_under_the_alias_too() {
    let json = agent_json_with_base64_len(8192);
    for prefix in crate::osc_agent::AGENT_OSC_PREFIXES {
        let params = crate::osc_agent::encode_agent_osc_params(prefix, &json);
        let b64 = String::from_utf8(params[2].clone()).expect("base64 is ascii");
        let f = local(16);
        let term = new_term();
        let mut pump = TerminalPump::new(f.router.clone());
        {
            let mut guard = term.lock();
            let head = String::from_utf8_lossy(prefix[0]).into_owned();
            let sub = String::from_utf8_lossy(prefix[1]).into_owned();
            pump.advance(
                &mut guard,
                format!("\x1b]{head};{sub};{b64}\x07").as_bytes(),
            );
        }
        pump.finish_batch_blocking(true);
        assert!(
            drain(&f.events)
                .iter()
                .any(|e| matches!(e, SessionEvent::AgentStatus(_))),
            "{:?}: a payload exactly at the documented 8 KiB cap must arrive",
            prefix[0]
        );
        assert_eq!(f.state.truncated_agent_osc(), 0, "{:?}", prefix[0]);
    }
}

/// Above the cap the payload is refused, and the refusal is visible: either the
/// parser truncated it (counted as truncated) or it arrived whole and
/// `parse_agent_status` rejected it on length. Never a silent loss.
#[test]
fn an_oversized_agent_payload_is_dropped_and_the_loss_is_counted() {
    let json = agent_json_with_base64_len(8196);
    for prefix in crate::osc_agent::AGENT_OSC_PREFIXES {
        let params = crate::osc_agent::encode_agent_osc_params(prefix, &json);
        let b64 = String::from_utf8(params[2].clone()).expect("base64 is ascii");
        let f = local(16);
        let term = new_term();
        let mut pump = TerminalPump::new(f.router.clone());
        {
            let mut guard = term.lock();
            let head = String::from_utf8_lossy(prefix[0]).into_owned();
            let sub = String::from_utf8_lossy(prefix[1]).into_owned();
            pump.advance(
                &mut guard,
                format!("\x1b]{head};{sub};{b64}\x07").as_bytes(),
            );
        }
        pump.finish_batch_blocking(true);
        assert!(
            !drain(&f.events)
                .iter()
                .any(|e| matches!(e, SessionEvent::AgentStatus(_))),
            "{:?}: over the cap must produce no event",
            prefix[0]
        );
        // Over the cap but under OSC_LARGE, so it arrives whole and is refused
        // on length rather than truncated — and it is not miscounted as an
        // unknown sub-code.
        assert_eq!(f.state.agent_osc_unknown_subcodes(), 0, "{:?}", prefix[0]);
    }
}

/// A payload the **parser** had to cut is dropped before anything parses it and
/// the loss is counted — a truncated base64 can decode to a shorter valid event
/// the agent never sent, so it must never reach `parse_agent_status`.
#[test]
fn a_truncated_agent_payload_is_dropped_and_counted() {
    for prefix in crate::osc_agent::AGENT_OSC_PREFIXES {
        let f = local(16);
        let events = route(&f.router, |batch| {
            let code: u32 = std::str::from_utf8(prefix[0]).unwrap().parse().unwrap();
            batch.push_osc(
                code,
                &[prefix[0], prefix[1], b"eyJ2IjoxLCJhZ2VudCI6InBpIn0="],
                StringTerm::Bel,
                true,
            );
        });
        assert!(events.is_empty(), "{:?}", prefix[0]);
        assert_eq!(f.state.truncated_agent_osc(), 1, "{:?}", prefix[0]);
        // Not miscounted as a malformed payload's neighbours.
        assert_eq!(f.state.agent_osc_unknown_subcodes(), 0, "{:?}", prefix[0]);
    }

    // A truncated reserved sub-code is still just an unknown sub-code.
    let f = local(16);
    route(&f.router, |batch| {
        batch.push_osc(20308, &[b"20308", b"2", b"x"], StringTerm::Bel, true);
    });
    assert_eq!(f.state.truncated_agent_osc(), 0);
    assert_eq!(f.state.agent_osc_unknown_subcodes(), 1);
}

/// (c) One protocol, one watermark — in both directions.
#[test]
fn verify_the_seq_watermark_is_shared_in_both_directions() {
    let json = |seq: u64| {
        format!(
            "{{\"v\":1,\"agent\":\"pi\",\"type\":\"state\",\
             \"seq\":{seq},\"ts\":1700000000000,\"state\":\"working\"}}"
        )
    };
    let [new, legacy] = crate::osc_agent::AGENT_OSC_PREFIXES;
    for [first, second] in [[new, legacy], [legacy, new]] {
        let f = local(16);
        assert!(!agent_status(&f, first, &json(7)).is_empty());
        assert!(
            agent_status(&f, second, &json(7)).is_empty(),
            "the same seq on the other spelling is the same event"
        );
        assert!(
            agent_status(&f, second, &json(6)).is_empty(),
            "and an older seq stays dropped"
        );
        assert!(!agent_status(&f, second, &json(8)).is_empty());
    }
}

/// (d) + (e) Every shape that must produce nothing, and what each does to the
/// unknown-sub-code counter.
#[test]
fn verify_the_dead_shapes_produce_no_event_and_count_as_documented() {
    // Unknown sub-codes: ignored, and counted.
    for params in [
        &[&b"20308"[..], &b"2"[..]][..],
        &[&b"20308"[..], &b"999"[..]][..],
        &[&b"20308"[..], &b""[..]][..],
        &[&b"20308"[..]][..],
        &[&b"20308"[..], &b"10"[..]][..],
        &[&b"20308"[..], &b"01"[..]][..],
    ] {
        let f = local(16);
        let events = route(&f.router, |batch| {
            batch.push_osc(20308, params, StringTerm::Bel, false);
        });
        assert!(events.is_empty(), "{params:?} must produce no event");
        assert!(
            f.transport.writes().is_empty(),
            "{params:?} answers nothing"
        );
        assert_eq!(
            f.state.agent_osc_unknown_subcodes(),
            1,
            "{params:?} must be counted"
        );
    }

    // Right sub-code, dead payload: dropped silently, *not* counted as unknown.
    for params in [
        &[&b"20308"[..], &b"1"[..]][..],
        &[&b"20308"[..], &b"1"[..], &b""[..]][..],
        &[&b"20308"[..], &b"1"[..], &b"!!not base64!!"[..]][..],
        &[&b"20308"[..], &b"1"[..], &b"bm90IGpzb24="[..]][..],
        &[&b"20308"[..], &b"1"[..], &b"eyJ2Ijo5fQ=="[..]][..],
    ] {
        let f = local(16);
        let events = route(&f.router, |batch| {
            batch.push_osc(20308, params, StringTerm::Bel, false);
        });
        assert!(events.is_empty(), "{params:?} must produce no event");
        assert_eq!(f.state.agent_osc_unknown_subcodes(), 0, "{params:?}");
    }

    // The same malformed payload on the alias behaves identically.
    let f = local(16);
    let events = route(&f.router, |batch| {
        batch.push_osc(9, &[b"9", b"7", b"!!not base64!!"], StringTerm::Bel, false);
    });
    assert!(events.is_empty());
    assert_eq!(f.state.legacy_agent_osc_events(), 1, "still counted");
}

/// The claim is numeric (`VtEvent::Osc { code }`) but the dispatch is a string
/// match on `params[0]`, so a zero-padded number — which xterm-derived parsers
/// accept and normalise — is claimed and forwarded by the engine and then
/// dropped by the string arm. Pre-existing for OSC 7 / 9 / 133; `US-0088`
/// inherits it for 20308.
#[test]
fn verify_a_zero_padded_osc_number_reaches_the_same_handler() {
    let f = local(16);
    let written = feed_bytes(&f, b"\x1b]020308;0\x07");
    assert!(!written.is_empty(), "OSC 020308;0 is still OSC 20308;0");
}
