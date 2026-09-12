//! What the engine tells the embedder, and what it counts while doing it.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md`.
//!
//! An event is a **value**, never a callback: nothing runs inside the engine
//! while the caller holds the lock, which is what deletes the deferred/reliable
//! event tier the current backend needs. Payloads are spans into the batch's
//! arena ([`super::EventBatch`]), so an OSC costs no `Vec` per parameter.

use crate::event::batch::{ByteSpan, ParamSpans, StrSpan};
use crate::grid::{RowId, RowsScrolled};
use crate::intern::GraphicId;

/// Which OSC 52 selection a clipboard event names.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ClipboardKind {
    Clipboard,
    Primary,
    Secondary,
}

/// How a string sequence ended. The fork's `bell_terminated`, as an enum: the
/// answer to a query has to be terminated the same way the question was.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum StringTerm {
    /// `BEL`.
    Bel,
    /// `ST` (`ESC \`).
    St,
}

/// One thing that happened while parsing a chunk.
///
/// Deliberately **not** `Clone`: a payload is a span into the batch that
/// produced it, so an event outliving its batch is a bug the borrow checker
/// should catch rather than a copy that silently reads the next batch's bytes.
#[derive(PartialEq, Eq, Debug)]
pub enum VtEvent {
    /// Something changed. At most one per batch, appended last; it is a hint,
    /// not damage — damage is the per-row sequence number
    /// (`crate::render::RenderState`).
    Repaint,
    Title(StrSpan),
    TitleReset,
    Bell,
    /// OSC 52 store, already base64-decoded and validated as UTF-8.
    ClipboardStore {
        selection: ClipboardKind,
        text: StrSpan,
    },
    /// OSC 52 load; the embedder formats the reply.
    ClipboardLoad {
        selection: ClipboardKind,
    },
    /// Bytes the terminal owes the host: DA / DSR / DECRQM / XTVERSION.
    Reply(ByteSpan),
    /// `ED 2`, `ED 3` and `RIS` only — never `ED 0` / `ED 1`.
    ScreenCleared,
    /// An OSC the engine does not implement itself, forwarded in byte order.
    Osc {
        code: u32,
        params: ParamSpans,
        terminator: StringTerm,
        truncated: bool,
    },
    /// Content moved between row ids, so a `RowId`-keyed cache can shift
    /// instead of rebuilding. The grid's contract, wrapped
    /// (`crate::grid::RowsScrolled`).
    RowsScrolled(RowsScrolled),
    /// History was trimmed; `oldest` is the new oldest live row.
    RowsTrimmed {
        oldest: RowId,
    },
    /// The last cell referencing this image is gone; the view evicts its tile.
    GraphicReleased(GraphicId),
}

/// What one `feed` did, and everything it had to degrade while doing it.
///
/// Terminal input is untrusted, so nothing here is an error: every malformed
/// case moves a counter and parsing continues
/// (`docs/agents/error-policy.md`, the "optional telemetry" row).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct FeedStats {
    pub bytes: usize,
    pub rows_scrolled: u32,
    pub malformed_sequences: u32,
    pub truncated_osc: u32,
    pub aborted_dcs: u32,
    pub grapheme_truncated: u32,
    pub unhandled_sequences: u32,
    pub style_table_exhausted: u32,
}
