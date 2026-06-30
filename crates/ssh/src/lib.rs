//! SSH + SFTP implementation for myTerm2.
//!
//! Triển khai `russh` client + tokio runtime ẩn. `SshSession` implement
//! `TerminalSession` — UI dùng qua trait, không biết internals.
//!
//! Tham chiếu `docs/terminal-backend.md` §7.

pub(crate) mod counting_stream;
pub mod config;
pub mod handler;
pub mod listener;
pub mod session;
pub mod session_terminal;
pub mod sftp;
pub mod sftp_task;
pub mod state;
pub mod task;

pub use config::{SshAuthMethod, SshConfig};
pub use listener::{Cmd, SshListener};
pub use myterm2_core::{FileEntry, FileStat};
pub use session::{PtySize, SshSession, connect};
pub use sftp::{SftpCmd, SftpEvent, SftpSession};

pub use myterm2_core as core;
