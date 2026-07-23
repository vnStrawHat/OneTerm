use thiserror::Error;

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

    /// The operation was intentionally cancelled by the user or its owner.
    #[error("operation cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

impl AppError {
    /// Build an error from an arbitrary message string.
    pub fn msg(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }
}
