use thiserror::Error;

/// Shared error type for OneTerm.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    /// Build an error from an arbitrary message string.
    pub fn msg(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }
}
