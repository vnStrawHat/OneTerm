//! The caller-owned event batch and its one reusable arena.
//!
//! Every payload is a span into one byte arena that is cleared, not freed, at
//! the top of each `feed`, so the steady state grows to a high-water mark and
//! then allocates nothing. A `Vec` per OSC parameter would be the obvious
//! alternative and would allocate on the hot path for no gain, because the
//! consumer immediately re-borrows what it was handed.

use crate::event::vt_event::{ClipboardKind, StringTerm, VtEvent};
use crate::grid::ScrollReport;

/// A batch that held more than this shrinks back afterwards, so one hostile
/// 8 MiB OSC 52 payload does not keep its arena for the session.
pub(crate) const EVENT_ARENA_SOFT: usize = 1 << 20;

/// UTF-8 text in the batch's arena. Validated at insert, so reading it back
/// never fails.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StrSpan {
    start: u32,
    len: u32,
}

impl StrSpan {
    /// The same text without its first `bytes` bytes. The caller must know the
    /// cut lands on a character boundary; an over-long cut reads as empty.
    pub(crate) fn skip(self, bytes: u32) -> StrSpan {
        let bytes = bytes.min(self.len);
        StrSpan {
            start: self.start + bytes,
            len: self.len - bytes,
        }
    }
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
    /// An empty batch. Build one, keep it, and pass it to every `feed`.
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

    /// Every event in this batch, in byte order.
    pub fn iter(&self) -> std::slice::Iter<'_, VtEvent> {
        self.events.iter()
    }

    /// `true` when the last `feed` produced no events at all.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// How many events the last `feed` produced.
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

    /// Push a title, returning `false` when it was dropped because the bytes
    /// were not UTF-8. Validating here is what keeps [`EventBatch::str`]
    /// infallible.
    pub fn push_title(&mut self, text: &[u8]) -> bool {
        let Some(span) = self.push_str(text) else {
            return false;
        };
        self.events.push(VtEvent::Title(span));
        true
    }

    /// Push a clipboard write, returning `false` when the payload was dropped
    /// because the decoded bytes were not UTF-8.
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

    /// Intern one string payload and push the event that names it, or drop
    /// both when the bytes are not UTF-8. Keeping the validation here is what
    /// keeps [`EventBatch::str`] infallible.
    pub(crate) fn push_text(
        &mut self,
        text: &[u8],
        event: impl FnOnce(StrSpan) -> VtEvent,
    ) -> bool {
        let Some(span) = self.push_str(text) else {
            return false;
        };
        self.events.push(event(span));
        true
    }

    // ── Assembling a payload in place ──────────────────────────────────────
    //
    // An arm whose payload is not one contiguous parameter — a title rejoined
    // on `;`, a percent-decoded path — would otherwise build a `String` per
    // sequence on the hot path. These three let it build the payload directly
    // in the arena the event was going to be copied into anyway: `mark` before,
    // `extend` per piece, one `finish_*` after.

    /// Where a payload assembled with [`EventBatch::extend`] starts.
    pub(crate) fn mark(&self) -> usize {
        self.arena.len()
    }

    /// Append one piece of the payload under construction.
    pub(crate) fn extend(&mut self, bytes: &[u8]) {
        self.arena.extend_from_slice(bytes);
    }

    /// Finish the payload at `mark`, trimmed. `None`, and the arena rewound,
    /// when what was assembled is not valid UTF-8.
    pub(crate) fn finish_trimmed(&mut self, mark: usize) -> Option<StrSpan> {
        let (offset, len) = {
            let Ok(text) = std::str::from_utf8(&self.arena[mark..]) else {
                // The rewind is the point: the caller is dropping the payload,
                // and half of it must not stay in the arena for the rest of the
                // batch.
                self.arena.truncate(mark);
                return None;
            };
            let trimmed = text.trim();
            (
                trimmed.as_ptr() as usize - text.as_ptr() as usize,
                trimmed.len(),
            )
        };
        self.arena.truncate(mark + offset + len);
        Some(StrSpan {
            start: (mark + offset) as u32,
            len: len as u32,
        })
    }

    /// Finish the payload at `mark`, repairing invalid UTF-8 in place.
    ///
    /// Allocates **only** when the bytes are not already valid UTF-8 — a
    /// percent escape that decoded to a lone continuation byte, say — which is
    /// the case that was going to cost an allocation whatever happened.
    pub(crate) fn finish_lossy(&mut self, mark: usize) -> StrSpan {
        if std::str::from_utf8(&self.arena[mark..]).is_err() {
            let repaired = String::from_utf8_lossy(&self.arena[mark..]).into_owned();
            self.arena.truncate(mark);
            self.arena.extend_from_slice(repaired.as_bytes());
        }
        StrSpan {
            start: mark as u32,
            len: (self.arena.len() - mark) as u32,
        }
    }

    /// Push bytes the terminal owes the program, to be written to its input.
    pub fn push_reply(&mut self, bytes: &[u8]) {
        let span = self.push_bytes(bytes);
        self.events.push(VtEvent::Reply(span));
    }

    /// Push an OSC the engine does not handle itself, parameters and all.
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
    pub(crate) fn push_scroll_report(&mut self, report: &ScrollReport) {
        if let Some(scrolled) = report.scrolled {
            self.events.push(VtEvent::RowsScrolled(scrolled));
        }
        if let Some(oldest) = report.trimmed {
            self.events.push(VtEvent::RowsTrimmed { oldest });
        }
    }

    /// The text a `StrSpan` names. An out-of-range span reads as empty rather
    /// than panicking.
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

    /// The bytes a `ByteSpan` names, empty if the span is out of range.
    pub fn bytes(&self, span: ByteSpan) -> &[u8] {
        let start = span.start as usize;
        let end = start + span.len as usize;
        self.arena.get(start..end).unwrap_or(&[])
    }

    /// The `;`-separated parameters of a forwarded OSC, in order.
    pub fn params(&self, spans: ParamSpans) -> impl Iterator<Item = &[u8]> {
        let first = spans.first as usize;
        let end = first + spans.count as usize;
        self.params
            .get(first..end)
            .unwrap_or(&[])
            .iter()
            .map(|span| self.bytes(*span))
    }

    // These three capacities are what the allocation tests watch, instead of
    // installing a counting global allocator, which would need `unsafe`.

    /// Capacity of the payload arena, in bytes. Stable across batches once the
    /// high-water mark is reached, which is the property worth asserting.
    pub fn arena_capacity(&self) -> usize {
        self.arena.capacity()
    }

    /// Capacity of the OSC parameter span list, in spans.
    pub fn param_capacity(&self) -> usize {
        self.params.capacity()
    }

    /// Capacity of the event list, in events.
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
