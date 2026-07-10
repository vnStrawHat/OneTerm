//! Terminal rendering & input helpers (framework-agnostic).
//!
//! Depends on `alacritty_terminal` (types: `TermMode`, `Cell`, colors) but does
//! **not** depend on GPUI. The UI crate maps these types to GPUI when rendering.

pub mod colors_util;
pub mod content;
pub mod key_encode;
pub mod mouse_encode;
pub mod osc;
pub mod osc_color;
pub mod palette;
pub mod search;
pub mod session;
pub mod url;

pub use colors_util::{
    is_app_chosen_exact_color, is_decorative_character, is_default_background_color,
};
pub use content::{
    IndexedCell, TermDamageInfo, TerminalBounds, TerminalContent, is_blank_cell, last_content_line,
};
pub use key_encode::{KeyMods, KeySpec, NamedKey, encode_key};
pub use mouse_encode::{
    MouseModifiers, TerminalMouseButton, encode_mouse_move, encode_mouse_press,
    encode_mouse_release, encode_wheel_event,
};
pub use osc::{
    Osc133Kind, OscPayload, TerminalProgress, decode_osc52, encode_osc52, parse_cwd_url, parse_osc,
};
pub use osc_color::{
    BACKGROUND_INDEX, CURSOR_INDEX, ColorFormatter, DynamicColors, FOREGROUND_INDEX,
    PendingColorQuery, SharedColorQueries, default_color_for_index, new_color_queries,
};
pub use palette::{TerminalPalette, extended_indexed_color, indexed_default_color, resolve_color};
pub use search::{SearchMatch, SearchOptions, search_term};
pub use session::{
    CursorBounds, CwdSource, NetStats, SessionEvent, TerminalInfo, TerminalSession, parse_keystroke,
};
pub use url::{link_ranges, url_at};
