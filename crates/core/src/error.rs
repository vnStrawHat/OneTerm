use thiserror::Error;

/// Lỗi dùng chung cho OneTerm.
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
    /// Tạo lỗi từ chuỗi thông điệp bất kỳ.
    pub fn msg(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }
}
