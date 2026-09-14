//! `oneterm-vt` — OneTerm's own VT engine.
//!
//! A leaf crate: it depends on no OneTerm crate, on no UI framework, and holds
//! no lock and no interior mutability. The embedder owns the lock and the event
//! loop; this crate owns the bytes-to-grid semantics.
//!
//! There is no atomic here at all: the render-demand flag the pump tests at a
//! chunk boundary is the **adapter's** primitive and lives with the lock policy
//! that uses it, in `crates/terminal/src/handle.rs` (`US-0090`).
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/high-level-design.md`.
//!
//! Terminal input is untrusted. Nothing here returns an error to the embedder
//! and nothing here panics on input: a malformed or hostile stream is dropped,
//! truncated or degraded to a documented fallback, and counted.

// Three modules are reachable by path from outside, and only three: `grid`
// (`crates/tools` replays a recording through `grid::Screen`, and the adapter
// reads `grid::{DEFAULT_SCROLLBACK, SCROLLBACK_MAX}`), `intern` (the adapter
// resolves `intern::Hyperlink`) and `parser` (the session logger drives its own
// parser). Everything else an embedder names is re-exported below, so the
// modules themselves are the crate's own business (`US-0090`).
pub(crate) mod cell;
// The module is `event` — the name the design and its test filters use — and
// its files live in `events/`, one concept each.
#[path = "events/mod.rs"]
pub(crate) mod event;
pub(crate) mod graphics;
pub mod grid;
pub mod intern;
pub mod parser;
pub(crate) mod reflow;
pub(crate) mod render;
pub(crate) mod selection;
pub(crate) mod terminal;
pub(crate) mod width;

pub use cell::{Attrs, Cell, CellContent, CellWidth, Color, NamedColor, Rgb, Semantic, Style};
// `FeedStats` has no external namer either, but it is what `Terminal::feed`
// returns, so it stays published.
pub use event::{ClipboardKind, EventBatch, FeedStats, StringTerm, VtEvent};
pub use graphics::{GraphicData, VIRTUAL_CELL};
pub use grid::{Pos, RowId, RowRef, SeqNo, Size, Viewport};
pub use intern::{Extras, ExtrasId, GraphicId, Hyperlink, HyperlinkId, Interner};
pub use reflow::ResizePolicy;
pub use render::{
    ModeSnapshot, MouseEncoding, MouseProtocol, MouseReporting, Palette, RenderCell, RenderContent,
    RenderCursor, RenderPlacement, RenderRow, RenderState, RenderUpdate, StyleRun,
};
pub use selection::{Selection, SelectionKind, SelectionRange, Side};
pub use terminal::{ColorKey, Config, CursorShape, KeyboardFlags, Mode, OscClaims, Terminal};
// `cluster_width` has no caller yet by design: it is the answer mode 2027 needs,
// implemented and tested ahead of the mode so landing it is a print-path change
// (see the module doc). It stays published for that reason, unlike the other 21
// names `US-0090` dropped from this block.
pub use width::{cluster_width, scalar_width};
