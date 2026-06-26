//! SSH session store — load/save `ssh_session.json`.
//!
//! Danh sách SSH session (label, host, port, username, group) được persist
//! vào file `ssh_session.json`. Khi app khởi động, store load file để render
//! list trong [`crate::views::SessionPanel`]. Khi user thêm session mới qua
//! "New Session" dialog, store update + save lại file.
//!
//! Path: `target/ssh_session.json` (debug) / `ssh_session.json` (release)
//! — cùng pattern với `terminal.json` và `docks.json`.

use std::path::PathBuf;

use gpui::{App, AppContext, Entity, Global};
use serde::{Deserialize, Serialize};

// ── Config path ──────────────────────────────────────────────────────

#[cfg(debug_assertions)]
const SESSION_FILE: &str = "target/ssh_session.json";
#[cfg(not(debug_assertions))]
const SESSION_FILE: &str = "ssh_session.json";

// ── SshSession ───────────────────────────────────────────────────────

/// Một SSH session entry — lưu trong `ssh_session.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshSession {
    /// Nhãn hiển thị trong SessionPanel.
    pub label: String,
    /// Hostname hoặc IP.
    pub host: String,
    /// Cổng SSH (mặc định 22).
    #[serde(default = "default_port")]
    pub port: u16,
    /// Username (optional — có thể nhập lúc connect).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Nhóm (optional — dùng để gom session trong Tree).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

fn default_port() -> u16 {
    SshSession::DEFAULT_PORT
}

impl SshSession {
    /// Port mặc định của SSH.
    pub const DEFAULT_PORT: u16 = 22;
}

// ── Store ────────────────────────────────────────────────────────────

/// Store toàn cục chứa danh sách [`SshSession`].
///
/// Load từ `ssh_session.json` lúc `init`, save khi thêm session mới.
/// Render qua [`crate::views::SessionPanel`] — panel `observe` entity này
/// để re-render khi list thay đổi.
pub struct SshSessionStore {
    sessions: Vec<SshSession>,
}

impl SshSessionStore {
    /// Danh sách session (immutable).
    pub fn sessions(&self) -> &[SshSession] {
        &self.sessions
    }

    /// Thêm 1 session mới + lưu file + notify observers.
    pub fn add(&mut self, session: SshSession, cx: &mut gpui::Context<Self>) {
        self.sessions.push(session);
        cx.notify();
        self.save();
    }

    /// Cập nhật session tại `index` + lưu file + notify observers.
    /// Nếu `index` ngoài range thì no-op.
    pub fn update(&mut self, index: usize, session: SshSession, cx: &mut gpui::Context<Self>) {
        if let Some(slot) = self.sessions.get_mut(index) {
            *slot = session;
            cx.notify();
            self.save();
        }
    }

    /// Xoá session tại `index` + lưu file + notify observers.
    /// Nếu `index` ngoài range thì no-op.
    pub fn remove(&mut self, index: usize, cx: &mut gpui::Context<Self>) {
        if index < self.sessions.len() {
            self.sessions.remove(index);
            cx.notify();
            self.save();
        }
    }

    /// Load danh sách session từ `ssh_session.json`.
    /// Nếu file không tồn tại hoặc parse lỗi → return empty list.
    fn load() -> Vec<SshSession> {
        let path = PathBuf::from(SESSION_FILE);
        match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<Vec<SshSession>>(&raw) {
                Ok(list) => list,
                Err(e) => {
                    tracing::error!("ssh_session.json parse error: {e} — starting empty");
                    Vec::new()
                }
            },
            Err(_) => {
                // File chưa tồn tại — chưa có session nào, không tạo file rỗng
                // để tránh ghi file trắng khi user chưa thêm session nào.
                Vec::new()
            }
        }
    }

    /// Save danh sách session ra `ssh_session.json` (pretty-print).
    fn save(&self) {
        let path = PathBuf::from(SESSION_FILE);
        match serde_json::to_string_pretty(&self.sessions) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    tracing::error!("Failed to write ssh_session.json: {e}");
                }
            }
            Err(e) => tracing::error!("Failed to serialize ssh sessions: {e}"),
        }
    }
}

// ── Global wrapper (pattern như `AppStateGlobal` / `TerminalSettingsGlobal`) ──

/// Global wrapper cho `Entity<SshSessionStore>`.
pub struct SshSessionStoreGlobal(pub Entity<SshSessionStore>);

impl Global for SshSessionStoreGlobal {}

impl SshSessionStore {
    /// Lấy `Entity<SshSessionStore>` toàn cục (panic nếu chưa init).
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<SshSessionStoreGlobal>().0.clone()
    }

    /// Khởi tạo store toàn cục — load `ssh_session.json` (gọi ở `ui::init`).
    pub fn init(cx: &mut App) {
        let sessions = Self::load();
        let entity = cx.new(|_| Self { sessions });
        cx.set_global(SshSessionStoreGlobal(entity));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_full_session() {
        let json =
            r#"[{ "label": "prod", "host": "10.0.0.1", "port": 2222, "username": "ubuntu" }]"#;
        let list: Vec<SshSession> = serde_json::from_str(json).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].label, "prod");
        assert_eq!(list[0].host, "10.0.0.1");
        assert_eq!(list[0].port, 2222);
        assert_eq!(list[0].username.as_deref(), Some("ubuntu"));
    }

    #[test]
    fn deserialize_uses_default_port_when_missing() {
        let json = r#"[{ "label": "dev", "host": "localhost" }]"#;
        let list: Vec<SshSession> = serde_json::from_str(json).unwrap();
        assert_eq!(list[0].port, SshSession::DEFAULT_PORT);
        assert!(list[0].username.is_none());
    }

    #[test]
    fn serialize_skips_none_username() {
        let session = SshSession {
            label: "a".into(),
            host: "b".into(),
            port: 22,
            username: None,
            group: None,
        };
        let json = serde_json::to_string(&session).unwrap();
        assert!(!json.contains("username"));
    }

    #[test]
    fn serialize_keeps_some_username() {
        let session = SshSession {
            label: "a".into(),
            host: "b".into(),
            port: 22,
            username: Some("root".into()),
            group: None,
        };
        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"username\":\"root\""));
    }

    #[test]
    fn empty_array_parses() {
        let list: Vec<SshSession> = serde_json::from_str("[]").unwrap();
        assert!(list.is_empty());
    }
}
