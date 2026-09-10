//! SSH + SFTP implementation for OneTerm.
//!
//! Implements a `russh` client on a shared tokio runtime. `SshSession`
//! implements `TerminalSession` — the UI uses it through the trait, unaware of
//! the internals; SFTP is reached through `TerminalCapabilities::sftp`.
//! Terminal parsing / OSC routing / event delivery come from the shared pump
//! layer in `oneterm_terminal::backend`.
//!
//! The only public items are [`SshSession`] and [`connect`]; everything else
//! is crate-private. See `docs/terminal-backend.md` §7.

mod agent;
mod counting_stream;
mod handler;
mod route;
mod session;
mod session_terminal;
mod sftp;
mod sftp_task;
mod task;
#[cfg(test)]
mod test_support;
mod transport;
mod tunnel;

pub use session::{SshSession, connect};
