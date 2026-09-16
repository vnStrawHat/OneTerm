//! The other direction: a key press or a mouse click → the bytes the program
//! on the far end of the PTY expects.
//!
//! The engine answers this because only the engine knows the modes the answer
//! depends on — DECCKM for the cursor keys, `? 1005` / `? 1006` for the mouse
//! report. Both encoders read a [`crate::ModeSnapshot`] and return bytes; they
//! have no side effects, so scrolling the viewport back to the live screen,
//! clearing a selection or tracking a held shift stays with the embedder's
//! input handling, where the platform event lives.
//!
//! The types here are deliberately framework-neutral: the embedder maps its own
//! key and mouse events onto [`KeySpec`] and [`TerminalMouseButton`].

mod key;
mod kitty;
mod mouse;

pub use key::{KeyEvent, KeyEventKind, KeyMods, KeySpec, NamedKey, encode_key, encode_key_event};
pub use mouse::{
    MouseModifiers, TerminalMouseButton, encode_mouse_move, encode_mouse_press,
    encode_mouse_release, encode_wheel_event,
};
