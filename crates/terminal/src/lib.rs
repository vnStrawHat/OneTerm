//! The terminal adapter: `oneterm-vt` behind `TerminalSession`.
//!
//! Since `US-0082` this crate speaks the engine's vocabulary — `RowId`,
//! `RenderState`, `EventBatch`, `ColorKey`, `ResizePolicy` — and owns the two
//! things the engine deliberately does not: the lock ([`TerminalHandle`]) and
//! the event-delivery policy ([`backend`]).
//!
//! Since `US-0085` the compatibility surface is gone with it: nothing this
//! crate publishes names a forked-engine type any more, and `US-0087` deleted
//! the fork itself along with the old-versus-new differential that was its
//! last consumer. No GPUI here either way.

// Modules nothing outside names by path are the crate's own business; the items
// other crates use are re-exported below (`US-0090`).
pub mod backend;
pub(crate) mod color_classification;
pub(crate) mod content;
pub(crate) mod factory;
pub mod handle;
pub(crate) mod key_encode;
pub mod logging;
pub mod model;
pub mod mouse_encode;
pub(crate) mod osc;
pub mod osc_agent;
pub(crate) mod osc_color;
pub(crate) mod palette;
pub(crate) mod paste;
pub mod security_policy;
pub mod session;
#[cfg(test)]
pub(crate) mod test_engine;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod url_policy;

pub use backend::{
    ByteBudget, GridSize, OscRouter, PtyTransport, SessionEventSink, SharedSessionState,
    SharedState, TerminalPump,
};
pub use color_classification::is_decorative_character;
pub use content::{LineRangeCells, SnapshotCell, TerminalContent, last_content_row};
pub use factory::{PtySize, SessionFactory};
pub use handle::{DEFAULT_SCROLLBACK_LINES, SharedTerminal, TerminalHandle, new_shared_terminal};
pub use key_encode::{KeyMods, KeySpec, NamedKey, encode_key};
pub use logging::{
    TerminalLogController, TerminalLogError, TerminalLogState, local_log_identity, ssh_log_identity,
};
pub use mouse_encode::{MouseModifiers, TerminalMouseButton};
pub use oneterm_vt::search::{SearchMatch, SearchOptions};
/// The engine vocabulary this crate's own API speaks, re-exported so a consumer
/// can name what [`TerminalContent`]'s native accessors return without taking a
/// direct dependency on `oneterm-vt` first (`US-0085`).
pub use oneterm_vt::{
    Attrs, CellWidth, Color, CursorShape, GraphicData, GraphicId, HyperlinkId, ModeSnapshot,
    MouseEncoding, MouseProtocol, MouseReporting, NamedColor, RenderCell, RenderContent,
    RenderCursor, RenderPlacement, RenderRow, RenderUpdate, ResizePolicy, Rgb, RowId,
    SelectionKind, SelectionRange, Semantic, SeqNo, Size, Style, StyleRun, Terminal,
    VIRTUAL_CELL as SIXEL_VIRTUAL_CELL,
};
pub use osc::{TerminalProgress, encode_osc52};
pub use osc_agent::{
    AgentPayload, AgentState, AgentStatusEvent, ApprovalChoice, ApprovalEvent, ApprovalKind,
    ApprovalRisk, FileAction, FileEvent, HeartbeatEvent, ModelEvent, StateEvent, ToolCallEvent,
    ToolCallPhase, should_apply,
};
pub use osc_color::DynamicColors;
pub use palette::{TerminalPalette, resolve_color};
pub use security_policy::{ClipboardOrigin, TerminalSecurityPolicy};
pub use session::{
    NetStats, PtyOwner, PtySession, SessionEvent, SessionKind, TerminalCapabilities, TerminalError,
    TerminalIme, TerminalInfo, TerminalInput, TerminalLifecycle, TerminalQueryState,
    TerminalRender, TerminalSession, report_generated_input,
};
pub use url_policy::TargetDecision;
