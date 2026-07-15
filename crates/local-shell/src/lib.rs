//! Local shell via PTY for OneTerm.
//!
//! Uses `alacritty_terminal::tty` + `EventLoop` (ConPTY on Windows).
//! Supports `cmd`/`powershell`/`pwsh`/custom shell config. See
//! `docs/terminal-backend.md`.

pub mod event_loop;
pub mod listener;
pub mod session;
pub mod session_terminal;
pub mod state;

#[cfg(test)]
mod session_tests;

pub use event_loop::{ShellEventLoop, ShellNotifier};
pub use listener::LocalListener;
pub use oneterm_core as core;
pub use oneterm_terminal::PtySize;
pub use session::LocalSession;
