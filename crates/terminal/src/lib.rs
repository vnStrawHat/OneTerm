//! The terminal adapter: `oneterm-vt` behind `TerminalSession`.
//!
//! Since `US-0082` this crate speaks the engine's vocabulary — `RowId`,
//! `RenderState`, `EventBatch`, `ColorKey`, `ResizePolicy` — and owns the two
//! things the engine deliberately does not: the lock ([`TerminalHandle`]) and
//! the event-delivery policy ([`backend`]).
//!
//! `alacritty_terminal` is still a dependency and still **runs nothing**. What
//! is left of it is one **compatibility surface**: the legacy shape of
//! [`TerminalContent`] and the value types around it — `TermMode`, `Cell`,
//! `Point`, `SelectionRange`, `SelectionType`, the colour types — which
//! `crates/terminal-view` reads directly out of this crate's API. It is named in
//! exactly five files ([`engine_shim`], [`content`], [`session`], [`palette`],
//! [`osc_color`]) and nowhere on the native path. `US-0085` moves the view onto
//! the engine's own types, which is what makes `US-0087`'s manifest deletion
//! possible. No GPUI here either way.

pub mod backend;
pub mod color_classification;
pub mod content;
pub(crate) mod engine_shim;
pub mod factory;
pub mod handle;
pub mod key_encode;
pub mod logging;
pub mod model;
pub mod mouse_encode;
pub mod osc;
pub mod osc_agent;
pub mod osc_color;
pub mod palette;
pub(crate) mod paste;
pub mod search;
pub mod security_policy;
pub mod session;
/// R-44: the ten Sixel tests drive `alacritty_terminal::Term` directly and are
/// deleted in this packet's last commit, once the engine's own
/// `graphics::tests::*` (`US-0080`) have been green beside them.
#[cfg(test)]
mod sixel_tests;
#[cfg(test)]
pub(crate) mod test_engine;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod url_policy;

pub use alacritty_terminal::term::graphics::{
    GraphicCell, GraphicData, GraphicId, VIRTUAL_CELL as SIXEL_VIRTUAL_CELL,
};
pub use backend::{
    DefaultColors, GridSize, OscRouter, PtyTransport, SessionEventSink, SharedSessionState,
    SharedState, TerminalPump,
};
pub use color_classification::{
    is_app_chosen_exact_color, is_decorative_character, is_default_background_color,
};
pub use content::{IndexedCell, TermDamageInfo, TerminalContent, last_content_line};
pub use factory::{PtySize, SessionFactory};
pub use handle::{
    DEFAULT_SCROLLBACK_LINES, Engine, SharedTerminal, TerminalHandle, new_shared_terminal,
};
pub use key_encode::{KeyMods, KeySpec, NamedKey, encode_key};
pub use logging::{
    TerminalLogController, TerminalLogError, TerminalLogState, local_log_identity, ssh_log_identity,
};
pub use model::ResizePolicy;
pub use mouse_encode::{MouseModifiers, TerminalMouseButton};
/// The engine vocabulary this crate's own API speaks, re-exported so a consumer
/// can name what [`TerminalContent`]'s native accessors return without taking a
/// direct dependency on `oneterm-vt` first (`US-0085`).
pub use oneterm_vt::{
    ModeSnapshot, RenderCell, RenderContent, RenderCursor, RenderPlacement, RenderRow,
    RenderUpdate, RowId, SelectionKind, SeqNo, StyleRun, Terminal,
};
pub use osc::{TerminalProgress, encode_osc52};
pub use osc_agent::{
    AgentPayload, AgentState, AgentStatusEvent, ApprovalChoice, ApprovalEvent, ApprovalKind,
    ApprovalRisk, FileAction, FileEvent, HeartbeatEvent, ModelEvent, StateEvent, ToolCallEvent,
    ToolCallPhase, should_apply,
};
pub use osc_color::DynamicColors;
pub use palette::{TerminalPalette, resolve_color};
pub use search::{SearchMatch, SearchOptions};
pub use security_policy::{ClipboardOrigin, TerminalSecurityPolicy};
pub use session::{
    LineRangeCells, NetStats, SessionEvent, SessionKind, TerminalCapabilities, TerminalError,
    TerminalIme, TerminalInfo, TerminalInput, TerminalLifecycle, TerminalQueryState,
    TerminalRender, TerminalSession, report_generated_input,
};
pub use url_policy::TargetDecision;
