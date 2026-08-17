use std::fmt;

use thiserror::Error;

/// SFTP status code reported by the server (`SSH_FX_*` from the SFTP protocol
/// drafts). Lets callers distinguish, for example, permission-denied from
/// not-found without parsing a message string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SftpStatus {
    /// `SSH_FX_EOF` — end of file or directory listing.
    Eof,
    /// `SSH_FX_NO_SUCH_FILE` — the path does not exist.
    NoSuchFile,
    /// `SSH_FX_PERMISSION_DENIED`.
    PermissionDenied,
    /// `SSH_FX_FAILURE` — generic server-side failure (also used for
    /// "directory not empty" and "file exists" by many servers).
    Failure,
    /// `SSH_FX_BAD_MESSAGE` — malformed request.
    BadMessage,
    /// `SSH_FX_NO_CONNECTION` / `SSH_FX_CONNECTION_LOST`.
    ConnectionLost,
    /// `SSH_FX_OP_UNSUPPORTED` — the server lacks the operation or extension.
    OpUnsupported,
    /// Any other status code, kept verbatim.
    Other(u32),
}

impl fmt::Display for SftpStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Eof => "end of file",
            Self::NoSuchFile => "no such file",
            Self::PermissionDenied => "permission denied",
            Self::Failure => "failure",
            Self::BadMessage => "bad message",
            Self::ConnectionLost => "connection lost",
            Self::OpUnsupported => "operation unsupported",
            Self::Other(code) => return write!(f, "status {code}"),
        };
        f.write_str(name)
    }
}

/// The step of an SSH connection attempt that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectPhase {
    /// TCP connect and SSH transport handshake.
    Transport,
    /// User authentication (password, key, keyboard-interactive).
    Authentication,
    /// Opening the session channel.
    ChannelOpen,
    /// PTY request on the session channel.
    PtyRequest,
    /// Shell request on the session channel.
    ShellRequest,
    /// Sending the shell-integration bootstrap.
    ShellIntegration,
    /// Opening the SFTP subsystem channel.
    SftpSetup,
}

impl fmt::Display for ConnectPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Transport => "connect",
            Self::Authentication => "authentication",
            Self::ChannelOpen => "channel open",
            Self::PtyRequest => "PTY request",
            Self::ShellRequest => "shell request",
            Self::ShellIntegration => "shell integration bootstrap",
            Self::SftpSetup => "SFTP setup",
        };
        f.write_str(name)
    }
}

/// Shared error type for OneTerm.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// The server key is not present in known_hosts and requires explicit approval.
    #[error(
        "Unknown SSH host key for {host}:{port} ({algorithm}), SHA-256 fingerprint: {fingerprint}"
    )]
    HostKeyUnknown {
        /// Hostname presented by the connection configuration.
        host: String,
        /// Port presented by the connection configuration.
        port: u16,
        /// Server host-key algorithm.
        algorithm: String,
        /// OpenSSH SHA-256 fingerprint.
        fingerprint: String,
    },

    /// The server presented a key different from the recorded key.
    #[error(
        "SSH host key changed for {host}:{port}; refusing connection (SHA-256 fingerprint: {fingerprint})"
    )]
    HostKeyChanged {
        /// Hostname presented by the connection configuration.
        host: String,
        /// Port presented by the connection configuration.
        port: u16,
        /// OpenSSH SHA-256 fingerprint of the unexpected key.
        fingerprint: String,
    },

    /// The SFTP server answered a request with a non-OK status.
    ///
    /// `message` is the server's own text (may be empty); `status` is what
    /// callers should branch on.
    #[error("SFTP {status}{}", format_detail(.message))]
    Sftp {
        /// Status code sent by the server.
        status: SftpStatus,
        /// Human-readable text sent by the server.
        message: String,
    },

    /// One phase of an SSH connection attempt failed (or timed out).
    #[error("SSH {phase} failed: {message}")]
    Connect {
        /// The phase that failed.
        phase: ConnectPhase,
        /// What went wrong, in the transport's words.
        message: String,
    },

    /// A shell program could not be resolved on this machine.
    #[error("cannot start shell {shell:?}: {reason}")]
    ShellResolution {
        /// The shell that was requested (program name or path).
        shell: String,
        /// Why it could not be resolved (e.g. "not found in PATH").
        reason: String,
    },

    /// A persisted configuration document could not be loaded.
    #[error("cannot load {document}: {message}")]
    ConfigLoad {
        /// File name of the document (e.g. `terminal.json`).
        document: String,
        /// Parse or I/O detail.
        message: String,
    },

    /// The operation was intentionally cancelled by the user or its owner.
    #[error("operation cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

/// `: <message>` when the server sent one, nothing otherwise.
fn format_detail(message: &str) -> String {
    if message.is_empty() {
        String::new()
    } else {
        format!(": {message}")
    }
}

impl AppError {
    /// Build an error from an arbitrary message string.
    pub fn msg(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    /// The SFTP status carried by this error, if it is an SFTP status error.
    pub fn sftp_status(&self) -> Option<SftpStatus> {
        match self {
            Self::Sftp { status, .. } => Some(*status),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sftp_error_displays_status_and_optional_message() {
        let with_message = AppError::Sftp {
            status: SftpStatus::PermissionDenied,
            message: "Permission denied".into(),
        };
        assert_eq!(
            with_message.to_string(),
            "SFTP permission denied: Permission denied"
        );
        let without_message = AppError::Sftp {
            status: SftpStatus::NoSuchFile,
            message: String::new(),
        };
        assert_eq!(without_message.to_string(), "SFTP no such file");
        assert_eq!(without_message.sftp_status(), Some(SftpStatus::NoSuchFile));
        assert_eq!(AppError::Cancelled.sftp_status(), None);
    }

    #[test]
    fn typed_variants_name_their_context() {
        let connect = AppError::Connect {
            phase: ConnectPhase::Authentication,
            message: "rejected".into(),
        };
        assert_eq!(connect.to_string(), "SSH authentication failed: rejected");
        let shell = AppError::ShellResolution {
            shell: "pwsh".into(),
            reason: "not found in PATH".into(),
        };
        assert_eq!(
            shell.to_string(),
            "cannot start shell \"pwsh\": not found in PATH"
        );
        let config = AppError::ConfigLoad {
            document: "terminal.json".into(),
            message: "invalid JSON".into(),
        };
        assert_eq!(
            config.to_string(),
            "cannot load terminal.json: invalid JSON"
        );
    }
}
