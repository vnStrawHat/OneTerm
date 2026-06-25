//! Cấu hình kết nối SSH — input cho [`crate::SshSession::connect`].
//!
//! `SshConfig` chứa thông tin host + credentials. Password **không** persist
//! ra disk — chỉ tồn tại trong RAM trong thời gian kết nối.

use std::fmt::{self, Debug, Formatter};
use std::path::PathBuf;

/// Phương thức xác thực SSH.
#[derive(Clone)]
pub enum SshAuthMethod {
    /// Xác thực bằng password.
    Password {
        /// Password plaintext (chỉ trong RAM — không log, không serialize).
        password: String,
    },
    /// Xác thực bằng private key file.
    PrivateKey {
        /// Đường dẫn tới file private key.
        key_path: PathBuf,
        /// Passphrase giải mã key (nếu key có mã hoá).
        passphrase: Option<String>,
    },
    /// Xác thực qua SSH agent.
    Agent,
}

/// Cấu hình kết nối SSH.
///
/// Tạo từ thông tin `SshSession` (UI store) + credentials do user nhập trong
/// connect dialog. Truyền vào [`crate::SshSession::connect`].
#[derive(Clone)]
pub struct SshConfig {
    /// Hostname hoặc IP.
    pub host: String,
    /// Cổng SSH (mặc định 22).
    pub port: u16,
    /// Username SSH.
    pub username: String,
    /// Phương thức xác thực.
    pub auth: SshAuthMethod,
}

// ── Debug impl (mask password) ──────────────────────────────────────

impl Debug for SshAuthMethod {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Password { .. } => f
                .debug_struct("Password")
                .field("password", &"***")
                .finish(),
            Self::PrivateKey { key_path, .. } => f
                .debug_struct("PrivateKey")
                .field("key_path", key_path)
                .field("passphrase", &"***")
                .finish(),
            Self::Agent => f.write_str("Agent"),
        }
    }
}

impl Debug for SshConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("SshConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth", &self.auth) // mask password qua impl ở trên
            .finish()
    }
}
