//! The caller-owned event batch and its one reusable arena.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md`
//! section "Events are values in a caller-owned batch".
//!
//! The fork allocates one `Vec` per OSC parameter plus an outer `Vec` on the hot
//! path, purely so an event can cross a channel, and the consumer immediately
//! re-borrows them. Here every payload is a span into one byte arena that is
//! cleared — not freed — at the top of each `feed`, so the steady state grows to
//! a high-water mark and then allocates nothing.

use crate::event::vt_event::{ClipboardKind, StringTerm, VtEvent};
use crate::grid::ScrollReport;

/// A batch that held more than this shrinks back afterwards, so one hostile
/// 8 MiB OSC 52 payload does not keep its arena for the session.
pub const EVENT_ARENA_SOFT: usize = 1 << 20;

/// UTF-8 text in the batch's arena. Validated at insert, so reading it back
/// never fails.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StrSpan {
    start: u32,
    len: u32,
}

/// Raw bytes in the batch's arena.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ByteSpan {
    start: u32,
    len: u32,
}

/// A run of OSC parameters, as a window into the batch's parallel span list.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ParamSpans {
    first: u32,
    count: u16,
}

/// Events and their payloads, owned by the caller and reused across batches.
///
/// `feed` clears it first, so a caller that has not drained the previous batch
/// loses it — a programming error, not a recoverable one.
#[derive(Debug, Default)]
pub struct EventBatch {
    events: Vec<VtEvent>,
    arena: Vec<u8>,
    params: Vec<ByteSpan>,
    /// `Repaint` is appended at most once per batch.
    repaint: bool,
}

impl EventBatch {
    pub fn new() -> EventBatch {
        EventBatch::default()
    }

    /// Start a batch. Called at the top of `feed`.
    pub fn clear(&mut self) {
        self.events.clear();
        self.params.clear();
        self.arena.clear();
        self.repaint = false;
        if self.arena.capacity() > EVENT_ARENA_SOFT {
            self.arena.shrink_to(EVENT_ARENA_SOFT);
        }
    }

    /// Every event in this batch, in byte order.
    pub fn events(&self) -> &[VtEvent] {
        &self.events
    }

    pub fn iter(&self) -> std::slice::Iter<'_, VtEvent> {
        self.events.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// An event with no payload, or one whose spans this batch already owns.
    pub fn push(&mut self, event: VtEvent) {
        if event == VtEvent::Repaint {
            self.push_repaint();
            return;
        }
        self.events.push(event);
    }

    /// The end-of-batch hint. Idempotent: the second call in a batch is a no-op,
    /// so no caller has to remember whether it already asked for one.
    pub fn push_repaint(&mut self) {
        if self.repaint {
            return;
        }
        self.repaint = true;
        self.events.push(VtEvent::Repaint);
    }

    /// `false` when the title was dropped because it was not UTF-8, which is
    /// what the fork does and what keeps [`EventBatch::str`] infallible.
    pub fn push_title(&mut self, text: &[u8]) -> bool {
        let Some(span) = self.push_str(text) else {
            return false;
        };
        self.events.push(VtEvent::Title(span));
        true
    }

    /// `false` when the payload was dropped because it was not UTF-8 (trap 25).
    pub fn push_clipboard_store(&mut self, selection: ClipboardKind, text: &[u8]) -> bool {
        let Some(span) = self.push_str(text) else {
            return false;
        };
        self.events.push(VtEvent::ClipboardStore {
            selection,
            text: span,
        });
        true
    }

    pub fn push_reply(&mut self, bytes: &[u8]) {
        let span = self.push_bytes(bytes);
        self.events.push(VtEvent::Reply(span));
    }

    pub fn push_osc(
        &mut self,
        code: u32,
        params: &[&[u8]],
        terminator: StringTerm,
        truncated: bool,
    ) {
        let first = self.params.len() as u32;
        for param in params {
            let span = self.push_bytes(param);
            self.params.push(span);
        }
        self.events.push(VtEvent::Osc {
            code,
            params: ParamSpans {
                first,
                count: params.len().min(u16::MAX as usize) as u16,
            },
            terminator,
            truncated,
        });
    }

    /// Turn what a scroll primitive did into the events a consumer needs: the
    /// motion between row ids, and the new oldest row when history was trimmed.
    pub fn push_scroll_report(&mut self, report: &ScrollReport) {
        if let Some(scrolled) = report.scrolled {
            self.events.push(VtEvent::RowsScrolled(scrolled));
        }
        if let Some(oldest) = report.trimmed {
            self.events.push(VtEvent::RowsTrimmed { oldest });
        }
    }

    /// An out-of-range span reads as empty rather than panicking.
    ///
    /// A span is an index, not a borrow, so a consumer that squirrels one away
    /// across a `clear` reads whatever now sits at that offset — wrong bytes,
    /// never unsafety, and still valid UTF-8 because the arena only ever holds
    /// what was validated at insert. Holding an *event* across a `clear` is what
    /// the borrow checker prevents, and that is the case worth preventing.
    pub fn str(&self, span: StrSpan) -> &str {
        let start = span.start as usize;
        let end = start + span.len as usize;
        self.arena
            .get(start..end)
            .map(|bytes| std::str::from_utf8(bytes).unwrap_or(""))
            .unwrap_or("")
    }

    pub fn bytes(&self, span: ByteSpan) -> &[u8] {
        let start = span.start as usize;
        let end = start + span.len as usize;
        self.arena.get(start..end).unwrap_or(&[])
    }

    pub fn params(&self, spans: ParamSpans) -> impl Iterator<Item = &[u8]> {
        let first = spans.first as usize;
        let end = first + spans.count as usize;
        self.params
            .get(first..end)
            .unwrap_or(&[])
            .iter()
            .map(|span| self.bytes(*span))
    }

    /// Capacity of the byte arena. One of three probes the allocation tests
    /// watch instead of installing a counting global allocator, which would
    /// need `unsafe` — the same reason the grid measures its heap from its own
    /// capacities.
    pub fn arena_capacity(&self) -> usize {
        self.arena.capacity()
    }

    pub fn param_capacity(&self) -> usize {
        self.params.capacity()
    }

    pub fn event_capacity(&self) -> usize {
        self.events.capacity()
    }

    fn push_str(&mut self, text: &[u8]) -> Option<StrSpan> {
        let text = std::str::from_utf8(text).ok()?;
        let span = self.push_bytes(text.as_bytes());
        Some(StrSpan {
            start: span.start,
            len: span.len,
        })
    }

    fn push_bytes(&mut self, bytes: &[u8]) -> ByteSpan {
        let start = self.arena.len() as u32;
        self.arena.extend_from_slice(bytes);
        ByteSpan {
            start,
            len: bytes.len() as u32,
        }
    }
}
