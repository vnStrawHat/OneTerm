//! The batch's contract: order, one arena, spans instead of vectors, and the
//! grid's scroll report turned into events.
//!
//! Named for the verification list in
//! `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md`.

use crate::event::{ClipboardKind, EVENT_ARENA_SOFT, EventBatch, StringTerm, VtEvent};
use crate::grid::{ScrollRegion, Size, TerminalGrid};

fn batch_with_a_title(title: &str) -> EventBatch {
    let mut batch = EventBatch::new();
    assert!(batch.push_title(title.as_bytes()));
    batch
}

#[test]
fn events_are_in_byte_order() {
    let mut batch = EventBatch::new();
    batch.push_osc(7, &[b"file:///home"], StringTerm::St, false);
    batch.push(VtEvent::Bell);
    batch.push_osc(133, &[b"A"], StringTerm::Bel, false);
    assert!(batch.push_title(b"shell"));
    batch.push_repaint();

    let kinds: Vec<&VtEvent> = batch.iter().collect();
    assert!(matches!(kinds[0], VtEvent::Osc { code: 7, .. }));
    assert!(matches!(kinds[1], VtEvent::Bell));
    assert!(matches!(kinds[2], VtEvent::Osc { code: 133, .. }));
    assert!(matches!(kinds[3], VtEvent::Title(_)));
    assert!(matches!(kinds[4], VtEvent::Repaint));
}

#[test]
fn repaint_appears_at_most_once_per_batch() {
    let mut batch = EventBatch::new();
    batch.push_repaint();
    batch.push_repaint();
    batch.push(VtEvent::Repaint);

    assert_eq!(
        batch
            .iter()
            .filter(|event| matches!(event, VtEvent::Repaint))
            .count(),
        1
    );
}

#[test]
fn repaint_is_absent_when_nothing_changed() {
    let batch = EventBatch::new();

    assert!(batch.is_empty());
    assert!(!batch.iter().any(|event| matches!(event, VtEvent::Repaint)));
}

#[test]
fn clear_starts_a_new_batch_and_frees_nothing() {
    let mut batch = batch_with_a_title("first");
    batch.push_repaint();

    batch.clear();

    assert!(batch.is_empty());
    // The repaint latch is per batch, not per batch object.
    batch.push_repaint();
    assert_eq!(batch.len(), 1);
}

#[test]
fn spans_read_back_what_was_pushed() {
    let mut batch = EventBatch::new();
    assert!(batch.push_title("OneTerm — vt".as_bytes()));
    batch.push_reply(b"\x1b[?62;4c");
    assert!(batch.push_clipboard_store(ClipboardKind::Clipboard, b"pasted text"));
    batch.push_osc(
        8,
        &[b"id=1", b"https://example.invalid/"],
        StringTerm::St,
        false,
    );

    let mut seen = 0;
    for event in batch.iter() {
        match event {
            VtEvent::Title(span) => {
                assert_eq!(batch.str(*span), "OneTerm — vt");
                seen += 1;
            }
            VtEvent::Reply(span) => {
                assert_eq!(batch.bytes(*span), b"\x1b[?62;4c");
                seen += 1;
            }
            VtEvent::ClipboardStore { selection, text } => {
                assert_eq!(*selection, ClipboardKind::Clipboard);
                assert_eq!(batch.str(*text), "pasted text");
                seen += 1;
            }
            VtEvent::Osc { code, params, .. } => {
                assert_eq!(*code, 8);
                let params: Vec<&[u8]> = batch.params(*params).collect();
                assert_eq!(params, vec![&b"id=1"[..], &b"https://example.invalid/"[..]]);
                seen += 1;
            }
            other => panic!("unexpected event {other:?}"),
        }
    }
    assert_eq!(seen, 4);
}

#[test]
fn non_utf8_text_is_dropped_at_insert() {
    let mut batch = EventBatch::new();

    assert!(!batch.push_title(&[0xff, 0xfe]));
    assert!(!batch.push_clipboard_store(ClipboardKind::Primary, &[0x80]));

    assert!(batch.is_empty());
}

#[test]
fn osc_params_are_spans_not_vectors() {
    let mut batch = EventBatch::new();
    // Warm up to the high-water mark.
    for _ in 0..16 {
        batch.clear();
        batch.push_osc(9, &[b"progress", b"4", b"50"], StringTerm::Bel, false);
    }
    let arena = batch.arena_capacity();
    let params = batch.param_capacity();
    let events = batch.event_capacity();

    for _ in 0..1_000 {
        batch.clear();
        batch.push_osc(9, &[b"progress", b"4", b"50"], StringTerm::Bel, false);
    }

    assert_eq!(batch.arena_capacity(), arena, "the arena reallocated");
    assert_eq!(batch.param_capacity(), params, "the param list reallocated");
    assert_eq!(batch.event_capacity(), events, "the event list reallocated");
}

#[test]
fn large_osc_grows_then_shrinks_the_arena() {
    let mut batch = EventBatch::new();
    let payload = vec![b'a'; EVENT_ARENA_SOFT * 2];

    assert!(batch.push_clipboard_store(ClipboardKind::Clipboard, &payload));
    assert!(batch.arena_capacity() >= payload.len());

    batch.clear();

    assert!(
        batch.arena_capacity() <= EVENT_ARENA_SOFT,
        "one hostile payload kept its arena: {} bytes",
        batch.arena_capacity()
    );
}

#[test]
fn rows_scrolled_is_emitted_for_in_region_motion() {
    let mut grid = TerminalGrid::new(Size { rows: 8, cols: 20 }, 100);
    let mut batch = EventBatch::new();
    grid.begin_batch();

    let report = grid.scroll_up(ScrollRegion { top: 1, bottom: 4 }, 1);
    batch.push_scroll_report(&report);

    let scrolled = batch
        .iter()
        .find_map(|event| match event {
            VtEvent::RowsScrolled(scrolled) => Some(*scrolled),
            _ => None,
        })
        .expect("an in-region scroll emitted no motion");
    assert_eq!(scrolled.delta, -1);
    assert!(scrolled.bottom >= scrolled.top);
}

#[test]
fn a_whole_screen_scroll_emits_no_motion() {
    let mut grid = TerminalGrid::new(Size { rows: 8, cols: 20 }, 100);
    let mut batch = EventBatch::new();
    grid.begin_batch();

    // Every surviving row keeps its id and its content, so a RowId-keyed cache
    // is already correct and a delta would corrupt it.
    let report = grid.scroll_up(ScrollRegion::full(8), 1);
    batch.push_scroll_report(&report);

    assert!(
        !batch
            .iter()
            .any(|event| matches!(event, VtEvent::RowsScrolled(_))),
        "a whole-screen scroll reported motion between row ids"
    );
}

#[test]
fn rows_trimmed_is_emitted_when_history_is_trimmed() {
    let mut grid = TerminalGrid::new(Size { rows: 4, cols: 10 }, 2);
    let mut batch = EventBatch::new();
    grid.begin_batch();
    grid.screen_mut().goto(3, 0);

    for _ in 0..8 {
        if let Some(report) = grid.linefeed() {
            batch.push_scroll_report(&report);
        }
    }

    let trimmed = batch
        .iter()
        .filter(|event| matches!(event, VtEvent::RowsTrimmed { .. }))
        .count();
    assert!(trimmed > 0, "history was trimmed without an event");
}
