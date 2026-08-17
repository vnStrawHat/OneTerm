//! SSH + SFTP implementation for OneTerm.
//!
//! Implements a `russh` client with a hidden tokio runtime. `SshSession`
//! implements `TerminalSession` — the UI uses it through the trait, unaware of
//! the internals.
//!
//! See `docs/terminal-backend.md` §7.

pub mod config;
pub(crate) mod counting_stream;
pub(crate) mod handler;
pub mod listener;
pub mod session;
pub(crate) mod session_terminal;
pub mod sftp;
pub(crate) mod sftp_task;
pub(crate) mod state;
pub(crate) mod task;

pub use config::{SshAuthMethod, SshConfig};
pub use listener::{Cmd, SshListener};
pub use oneterm_core::FileEntry;
pub use oneterm_terminal::PtySize;
pub use session::{SshSession, connect};
pub use sftp::{SftpCmd, SftpEvent, SftpSession};

pub use oneterm_core as core;
