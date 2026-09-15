//! What the engine tells the embedder, and what it counts while doing it.
//!
//! An event is a **value**, never a callback: no embedder code runs inside the
//! engine, so the caller may hold its lock across `feed` and drain afterwards.
//! Payloads are spans into the batch's arena ([`super::EventBatch`]), so an OSC
//! costs no `Vec` per parameter.

use crate::event::batch::{ByteSpan, ParamSpans, StrSpan};
use crate::grid::{RowId, RowsScrolled};
use crate::intern::GraphicId;
use crate::terminal::ColorKey;

/// Which OSC 52 selection a clipboard event names.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ClipboardKind {
    /// `OSC 52` selection `c`: the system clipboard.
    Clipboard,
    /// `OSC 52` selection `p`: the X11 primary selection.
    Primary,
    /// `OSC 52` selection `s`: the X11 secondary selection.
    Secondary,
}

/// How a string sequence ended. Carried because the answer to a query has to be
/// terminated the same way the question was.
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
    /// Something changed, so a consumer that draws should ask for an update.
    ///
    /// At most one per batch, appended last. It is a hint, not damage: to learn
    /// what actually changed, call
    /// [`Terminal::render_update`](crate::Terminal::render_update) with your own
    /// [`RenderState`](crate::RenderState), which returns only the rows whose
    /// sequence number moved since that state last asked.
    Repaint,
    /// `OSC 0` / `OSC 2`: the window title the program asks for. Read the text
    /// with [`EventBatch::str`](super::EventBatch::str); sanitising it before
    /// display is the embedder's job.
    Title(StrSpan),
    /// `OSC 0` / `OSC 2` with an empty string: go back to the default title.
    TitleReset,
    /// `BEL` (`0x07`), outside a string sequence. Ring, flash, or ignore.
    Bell,
    /// `OSC 52` store, already base64-decoded and validated as UTF-8. Whether a
    /// program may write the clipboard is the embedder's policy, not the
    /// engine's.
    ClipboardStore {
        /// Which selection the program asked to write.
        selection: ClipboardKind,
        /// The text to store, in the batch's arena.
        text: StrSpan,
    },
    /// `OSC 52` with a `?` payload: the program is reading the clipboard. The
    /// engine has no clipboard, so the embedder formats and sends the reply.
    ClipboardLoad {
        /// Which selection the program asked to read.
        selection: ClipboardKind,
    },
    /// Bytes the terminal owes the program: the answers to `DA`, `DSR`,
    /// `DECRQM`, `XTVERSION` and friends. Write them to the process input
    /// verbatim, in the order they arrive.
    Reply(ByteSpan),
    /// `OSC 4 / 10 / 11 / 12` with a `?` value: the program is asking what a
    /// colour currently is. The engine holds only the escape-sequence override
    /// layer, so the embedder, which owns the palette, formats the reply, and
    /// must terminate it the way the question was terminated.
    ColorQuery {
        /// Which colour was asked about.
        key: ColorKey,
        /// The terminator the question used, and therefore the one the answer
        /// must use.
        terminator: StringTerm,
    },
    /// The whole screen was cleared: `ED 2`, `ED 3` or `RIS`. Never `ED 0` or
    /// `ED 1`, which clear only part of it.
    ScreenCleared,
    /// An OSC the engine does not handle itself, forwarded in byte order so an
    /// embedder can implement its own.
    Osc {
        /// The OSC number, as parsed from the leading numeric parameter.
        code: u32,
        /// Every `;`-separated parameter as raw bytes in the batch's arena,
        /// **the leading number included**, so parameter 0 is `code` again.
        /// Read them with [`EventBatch::params`](super::EventBatch::params).
        params: ParamSpans,
        /// How the sequence ended, `BEL` or `ST`.
        terminator: StringTerm,
        /// `true` when the payload hit the parser's size ceiling and the tail
        /// was dropped, so a consumer can reject it rather than parse a prefix.
        truncated: bool,
    },
    /// Content moved between row ids, so a cache keyed by row id can shift
    /// instead of rebuilding.
    RowsScrolled(RowsScrolled),
    /// History was trimmed, so rows older than this one no longer exist.
    RowsTrimmed {
        /// The new oldest live row.
        oldest: RowId,
    },
    /// The last cell referencing this image is gone; a consumer holding a
    /// texture for it should drop it.
    GraphicReleased(GraphicId),
}

/// What one `feed` did, and everything it had to degrade while doing it.
///
/// Terminal input is untrusted, so nothing here is an error: every malformed
/// case moves a counter and parsing continues. Every counter is per `feed`
/// call, not cumulative.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct FeedStats {
    /// Bytes handed to this `feed` call, which is exactly the length of the
    /// slice passed in.
    pub bytes: usize,
    /// Rows this call pushed into scrollback. Rows scrolled inside the screen
    /// without reaching history are not counted.
    pub rows_scrolled: u32,
    /// Sequences whose payload was well-formed enough to route but not to use:
    /// today, an `OSC 52` body that is not valid base64 or not valid UTF-8.
    pub malformed_sequences: u32,
    /// OSC payloads that hit the parser's size ceiling and lost their tail.
    pub truncated_osc: u32,
    /// `DCS` sequences abandoned by `CAN` or `SUB`, or for exceeding the
    /// maximum payload length. Any partial image is discarded.
    pub aborted_dcs: u32,
    /// Reserved, and always `0` today. Over-long grapheme clusters are
    /// truncated and counted inside the string interner, which does not yet
    /// report the count here.
    pub grapheme_truncated: u32,
    /// Sequences the engine parsed but does not implement. A rising count on a
    /// real workload is the signal that a sequence is worth implementing.
    pub unhandled_sequences: u32,
    /// Reserved, and always `0` today. The style table counts its own
    /// exhaustion internally but does not yet report the count here.
    pub style_table_exhausted: u32,
    /// `OSC 8` links dropped because the hyperlink table was full. The text
    /// still renders; the link is simply not clickable.
    pub hyperlink_table_exhausted: u32,
}
