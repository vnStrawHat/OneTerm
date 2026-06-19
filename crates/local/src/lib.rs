//! Local shell via PTY for myTerm2.
//!
//! Dùng `alacritty_terminal::tty` + `EventLoop` (ConPTY trên Windows).
//! Shell `cmd`/`powershell`/`pwsh`/custom config được. Xem `docs/terminal-backend.md`.
//! Hiện chưa có code.

pub use myterm2_core as core;
