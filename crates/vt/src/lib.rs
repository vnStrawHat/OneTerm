//! `oneterm-vt` — OneTerm's own VT engine.
//!
//! A leaf crate: it depends on no OneTerm crate, on no UI framework, and holds
//! no lock and no interior mutability. The embedder owns the lock and the event
//! loop; this crate owns the bytes-to-grid semantics.
//!
//! The one atomic in the crate is [`render::Demand`], the render-demand flag
//! the pump tests at a chunk boundary. It is the **adapter's** primitive — the
//! design places it in `crates/terminal` — and it is reachable from no engine
//! type: nothing here reads or writes it. It lives here until `US-0081` moves
//! the pump loop over (see the `US-0079` packet's deviation list).
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
pub mod parser;
pub mod reflow;
pub mod render;
pub mod selection;
pub mod terminal;
pub mod width;

pub use cell::{Attrs, Cell, CellContent, CellWidth, Color, NamedColor, Rgb, Semantic, Style};
pub use event::{
    ByteSpan, ClipboardKind, EventBatch, FeedStats, ParamSpans, StrSpan, StringTerm, VtEvent,
};
pub use grid::{Pos, RowId, RowRef, SeqNo, Size, TerminalGrid, Viewport};
pub use intern::{
    Extras, ExtrasId, GRAPHEME_MAX_LEN, GraphemeId, GraphicId, HyperlinkId, Interner, StyleId,
};
pub use reflow::{ResizeOutcome, ResizePolicy};
pub use render::{
    Demand, EngineView, ModeSnapshot, MouseEncoding, MouseProtocol, MouseReporting, Palette,
    RenderCell, RenderContent, RenderCursor, RenderRow, RenderState, RenderUpdate, StyleRun,
    SyncState, Watermark,
};
pub use selection::{
    Invalidation, SEMANTIC_ESCAPE_CHARS, Selection, SelectionKind, SelectionRange, Side,
};
pub use terminal::{
    ColorKey, ColorOverrides, Config, CursorShape, CursorStyle, KeyboardFlags, Mode, ModeState,
    OscClaims, Terminal, ThemeColors,
};
pub use width::{cluster_width, scalar_width};
