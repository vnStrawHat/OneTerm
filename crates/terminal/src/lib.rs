//! Terminal rendering & input helpers (framework-agnostic).
//!
//! Depends on `alacritty_terminal` (types: `TermMode`, `Cell`, colors) but does
//! **not** depend on GPUI. The UI crate maps these types to GPUI when rendering.

pub mod colors_util;
pub mod content;
pub mod contracts;
pub mod factory;
pub mod key_encode;
pub mod model;
pub mod mouse_encode;
pub mod osc;
pub mod osc_agent;
pub mod osc_color;
pub mod palette;
pub mod paste;
pub mod search;
pub mod security_policy;
pub mod session;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod url;
pub mod url_policy;

pub use colors_util::{
    is_app_chosen_exact_color, is_decorative_character, is_default_background_color,
};
pub use content::{
    IndexedCell, TermDamageInfo, TerminalBounds, TerminalContent, is_blank_cell, last_content_line,
};
pub use contracts::{
    TerminalError, TerminalInput, TerminalLifecycle, TerminalRenderer, report_generated_input,
};
pub use factory::{PtySize, SessionFactory, install_session_factory, session_factory};
pub use key_encode::{KeyMods, KeySpec, NamedKey, encode_key};
pub use mouse_encode::{
    MouseModifiers, TerminalMouseButton, encode_mouse_move, encode_mouse_press,
    encode_mouse_release, encode_wheel_event,
};
pub use osc::{
    Osc133Kind, OscPayload, TerminalProgress, decode_osc52, encode_osc52, parse_cwd_url, parse_osc,
};
#[cfg(any(test, feature = "test-support"))]
pub use osc_agent::encode_osc97_params;
pub use osc_agent::{
    AgentState, AgentStatusEvent, ApprovalChoice, ApprovalEvent, ApprovalKind, ApprovalRisk,
    FileAction, FileEvent, HeartbeatEvent, ModelEvent, ModelSource, SessionIdentityEvent,
    StateEvent, ToolCallEvent, ToolCallPhase, parse_agent_status, should_apply,
};
pub use osc_color::{
    BACKGROUND_INDEX, CURSOR_INDEX, ColorFormatter, DynamicColors, FOREGROUND_INDEX,
    PendingColorQuery, SharedColorQueries, default_color_for_index, new_color_queries,
};
pub use palette::{TerminalPalette, extended_indexed_color, indexed_default_color, resolve_color};
pub use paste::{PastePolicy, PasteResult, encode_paste};
pub use search::{SearchMatch, SearchOptions, search_term};
pub use security_policy::{NotificationRateLimiter, TerminalSecurityPolicy};
pub use session::{
    CursorBounds, CwdSource, NetStats, SessionEvent, TerminalInfo, TerminalQueryState,
    TerminalSession, parse_keystroke,
};
pub use url::{link_ranges, url_at};
pub use url_policy::{ConfirmReason, DenyReason, ExternalTargetPolicy, TargetDecision};
