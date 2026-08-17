//! Local shell via PTY for OneTerm.
//!
//! Uses `alacritty_terminal::tty` + a custom poll loop (ConPTY on Windows) on
//! top of the shared pump layer in `oneterm_terminal::backend`. Supports
//! `cmd`/`powershell`/`pwsh`/custom shell config. See `docs/terminal-backend.md`.
//!
//! The only public item is [`LocalSession`]; everything else is crate-private.

mod event_loop;
mod session;
mod session_terminal;
mod transport;

pub use session::LocalSession;
