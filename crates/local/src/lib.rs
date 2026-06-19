//! Local shell via PTY for myTerm2.
//!
//! Dùng `alacritty_terminal::tty` + `EventLoop` (ConPTY trên Windows).
//! Shell `cmd`/`powershell`/`pwsh`/custom config được. Xem `docs/terminal-backend.md`.

pub mod listener;
pub mod session;
pub mod state;

pub use myterm2_core as core;
pub use listener::LocalListener;
pub use session::{LocalSession, PtySize};