//! SSH + SFTP implementation for OneTerm.
//!
//! Implements a `russh` client with a hidden tokio runtime. `SshSession`
//! implements `TerminalSession` — the UI uses it through the trait, unaware of
//! the internals.
//!
//! See `docs/terminal-backend.md` §7.

pub mod config;
pub(crate) mod counting_stream;
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
pub use oneterm_core::{FileEntry, FileStat};
pub use session::{PtySize, SshSession, connect};
pub use sftp::{SftpCmd, SftpEvent, SftpSession};

pub use oneterm_core as core;
