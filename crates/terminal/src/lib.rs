//! Terminal rendering & input helpers (framework-agnostic).
//!
//! Depends on `alacritty_terminal` (types: `TermMode`, `Cell`, colors) but does
//! **not** depend on GPUI. The UI crate maps these types to GPUI when rendering.

pub mod backend;
pub mod color_classification;
pub mod content;
pub mod factory;
pub mod key_encode;
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
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod url_policy;

pub use backend::{
    DefaultColors, GridSize, OscRouter, PtyTransport, SessionEventSink, SharedSessionState,
    SharedState, TerminalPump,
};
pub use color_classification::{
    is_app_chosen_exact_color, is_decorative_character, is_default_background_color,
};
pub use content::{IndexedCell, TermDamageInfo, TerminalContent, last_content_line};
pub use factory::{PtySize, SessionFactory};
pub use key_encode::{KeyMods, KeySpec, NamedKey, encode_key};
pub use mouse_encode::{MouseModifiers, TerminalMouseButton};
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
