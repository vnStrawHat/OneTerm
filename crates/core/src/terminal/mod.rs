//! Terminal rendering & input helpers (framework-agnostic).
//!
//! Phụ thuộc `alacritty_terminal` (types: `TermMode`, `Cell`, colors) nhưng
//! **không** phụ thuộc GPUI. UI crate map các type này sang GPUI khi render.

pub mod colors_util;
pub mod content;
pub mod key_encode;
pub mod mouse_encode;
pub mod osc;
pub mod palette;
pub mod session;
pub mod url;

pub use colors_util::{
    is_app_chosen_exact_color, is_decorative_character, is_default_background_color,
};
pub use content::{IndexedCell, TerminalBounds, TerminalContent};
pub use key_encode::{KeyMods, KeySpec, NamedKey, encode_key};
pub use mouse_encode::{
    MouseModifiers, TerminalMouseButton, encode_mouse_move, encode_mouse_press,
    encode_mouse_release, encode_wheel_event,
};
pub use osc::{OscPayload, OscSink, decode_osc52, encode_osc52, parse_cwd_url};
pub use palette::{TerminalPalette, resolve_color};
pub use session::{CursorBounds, SessionEvent, TerminalSession};
pub use url::{link_ranges, url_at};
