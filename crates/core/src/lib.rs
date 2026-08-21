//! Domain model & business logic for OneTerm.
//!
//! Leaf crate: does not depend on `gpui` or `alacritty_terminal`. Contains the
//! error type, local-shell configuration, and the `SftpBackend` file-transfer
//! trait. The terminal engine (`TerminalSession` + alacritty coupling) lives in
//! the separate `oneterm-terminal` crate.

pub mod config;
pub mod editor_launcher;
pub mod error;
pub mod persistence;
pub mod schema;
pub mod session_duplicate;
pub mod sftp;
pub mod ssh_config;
pub mod terminal_logging;

pub use config::{LocalShellConfig, RightDockMode, ShellKind, config_dir, home_dir};
pub use editor_launcher::{EditorChoice, launch_editor};
pub use error::{AppError, ConnectPhase, SftpStatus};
pub use persistence::{atomic_write, quarantine_file, update_json_file};
pub use schema::{
    SCHEMA_VERSION_FIELD, migrate_json_value, parse_versioned_document, schema_version,
    set_schema_version, versioned_object,
};
pub use session_duplicate::{SessionDuplicateConfig, SshDuplicateAuth, SshDuplicateConfig};
pub use sftp::{
    FileEntry, RemotePath, SftpBackend, SftpFuture, SftpSessionId, SftpTableState, TransferEvent,
    TransferHandle,
};
pub use ssh_config::{
    ConnectionCancellation, DEFAULT_SSH_KEEPALIVE_INTERVAL_SECS, DEFAULT_SSH_KEEPALIVE_MAX,
    HostKeyPolicy, MAX_SSH_KEEPALIVE_INTERVAL_SECS, MAX_SSH_KEEPALIVE_MAX,
    MIN_SSH_KEEPALIVE_INTERVAL_SECS, MIN_SSH_KEEPALIVE_MAX, SecretString, SshAuthMethod, SshConfig,
    SshKeepaliveConfig,
};
pub use terminal_logging::{
    LOG_CONTENT_FORMAT, LOG_FILE_NAME_FORMAT, LogWriteMode, TerminalLogConfig,
    default_terminal_log_dir,
};

/// Shared result type for the `core` crate.
pub type Result<T> = std::result::Result<T, AppError>;

/// Log (at `warn`) and discard the failure of a best-effort `operation`.
///
/// `docs/agents/error-policy.md` forbids a bare `let _ =` on a runtime
/// operation: when a failure is deliberately tolerated (cleanup of
/// temporaries, closing an already-closed channel, notifying a consumer that
/// may have gone away), the operation name and the error must still reach the
/// log so the failure can be diagnosed without reproducing the action.
pub fn report_best_effort<T, E: std::fmt::Display>(
    operation: &str,
    result: std::result::Result<T, E>,
) {
    if let Err(error) = result {
        log::warn!("{operation}: best-effort operation failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::report_best_effort;

    #[test]
    fn best_effort_accepts_any_result_shape() {
        report_best_effort("unit test ok", Ok::<u8, String>(1));
        report_best_effort("unit test err", Err::<(), _>("boom"));
    }
}
