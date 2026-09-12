//! `oneterm-vt` — OneTerm's own VT engine.
//!
//! A leaf crate: it depends on no OneTerm crate, on no UI framework, and holds
//! no lock, atomic or interior mutability. The embedder owns the lock and the
//! event loop; this crate owns the bytes-to-grid semantics.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/high-level-design.md`.
//!
//! Terminal input is untrusted. Nothing here returns an error to the embedder
//! and nothing here panics on input: a malformed or hostile stream is dropped,
//! truncated or degraded to a documented fallback, and counted.

pub mod cell;
// The module is `event` — the name the design and its test filters use — and
// its files live in `events/`, one concept each.
#[path = "events/mod.rs"]
pub mod event;
pub mod grid;
pub mod intern;
pub mod render;
pub mod width;

pub use cell::{Attrs, Cell, CellContent, CellWidth, Color, NamedColor, Rgb, Semantic, Style};
pub use event::{
    ByteSpan, ClipboardKind, EventBatch, FeedStats, ParamSpans, StrSpan, StringTerm, VtEvent,
};
pub use grid::{Pos, RowId, RowRef, SeqNo, Size, TerminalGrid, Viewport};
pub use intern::{
    Extras, ExtrasId, GRAPHEME_MAX_LEN, GraphemeId, GraphicId, HyperlinkId, Interner, StyleId,
};
pub use render::{
    Demand, EngineView, ModeSnapshot, MouseEncoding, MouseProtocol, MouseReporting, Palette,
    RenderCell, RenderContent, RenderCursor, RenderRow, RenderState, RenderUpdate, StyleRun,
    SyncState, Watermark,
};
pub use width::{cluster_width, scalar_width};
