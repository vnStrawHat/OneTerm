//! `oneterm-vt` — OneTerm's own VT engine.
//!
//! A leaf crate: it depends on no OneTerm crate, on no UI framework, and holds
//! no lock and no interior mutability. The embedder owns the lock and the event
//! loop; this crate owns the bytes-to-grid semantics.
//!
//! There is no atomic here at all: a "something changed, come and draw"
//! flag belongs with the lock policy that reads it, which is the embedder's.
//!
//! Design: <https://github.com/vnStrawHat/OneTerm/blob/main/docs/spec-intakes/IN-0029-vt-engine/high-level-design.md>.
//!
//! Terminal input is untrusted. Nothing here returns an error to the embedder
//! and nothing here panics on input: a malformed or hostile stream is dropped,
//! truncated or degraded to a documented fallback, and counted.

#![warn(missing_docs)]

// Every Rust block in the README is compiled and run by `cargo test --doc`, so
// the front page cannot drift away from the API it advertises.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

// Six modules of API are reachable by path from outside, and only six: `grid`
// (`crates/tools` replays a recording through `grid::Screen`, and the adapter
// reads `grid::{DEFAULT_SCROLLBACK, SCROLLBACK_MAX}`), `input` (the key and
// mouse encoders, a family an embedder takes wholesale), `intern` (the adapter
// resolves `intern::Hyperlink`), `parser` (the session logger drives its own
// parser), `search` (a namespace of its own, because its two phases only make
// sense together) and `pty`, the transport. Everything else an embedder names
// is re-exported below, so the modules themselves are the crate's own business.
// `guide` is the seventh public module and carries no item at all: it is the
// embedder's guide, rendered by rustdoc.
pub(crate) mod cell;
// The module is `event` — the name the design and its test filters use — and
// its files live in `events/`, one concept each.
#[path = "events/mod.rs"]
pub(crate) mod event;
pub(crate) mod graphics;
pub mod grid;
// Prose, not API: thirteen empty modules, each carrying one Markdown chapter of
// the embedder's guide. Public so `cargo doc` renders them and `cargo test
// --doc` compiles the Rust blocks inside them.
pub mod guide;
pub mod input;
pub mod intern;
pub mod parser;
// The transport. On by default and the only feature-gated module, so
// `--no-default-features` is the build with no platform code in it; its own
// header says the rest.
#[cfg(feature = "pty")]
pub mod pty;
pub(crate) mod reflow;
pub mod search;
pub(crate) mod selection;
pub(crate) mod snapshot;
pub(crate) mod terminal;
pub(crate) mod width;

pub use cell::{Attrs, Cell, CellContent, CellWidth, Color, NamedColor, Rgb, Semantic, Style};
// `FeedStats` has no external namer either, but it is what `Terminal::feed`
// returns, so it stays published.
pub use event::{ClipboardKind, EventBatch, FeedStats, Progress, ShellMark, StringTerm, VtEvent};
pub use graphics::{GraphicData, VIRTUAL_CELL};
pub use grid::{Pos, RowId, RowRef, SeqNo, Size, Viewport};
pub use intern::{Extras, ExtrasId, GraphicId, Hyperlink, HyperlinkId, Interner};
pub use reflow::ResizePolicy;
pub use selection::{Selection, SelectionKind, SelectionRange, Side};
pub use snapshot::{
    ModeSnapshot, MouseEncoding, MouseProtocol, MouseReporting, Palette, SnapshotCell,
    SnapshotContent, SnapshotCursor, SnapshotPlacement, SnapshotRow, SnapshotState, SnapshotUpdate,
    StyleRun,
};
pub use terminal::{
    ColorKey, Config, CursorShape, KeyboardFlags, Mode, OscRoute, OscRoutes, Terminal,
};
// Both halves of the width decision are published: `scalar_width` is what the
// print path uses by default, `cluster_width` what it uses under mode `? 2027`,
// and an embedder measuring its own text has to make the same choice.
pub use width::{cluster_width, scalar_width};
