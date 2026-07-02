//! SSH session store — load/save `ssh_session.json`.
//!
//! The list of SSH sessions (label, host, port, username, group) is persisted to
//! `ssh_session.json`. On startup, the store loads the file to render the list in
//! [`crate::views::SessionPanel`]. When the user adds a new session via the
//! "New Session" dialog, the store updates and re-saves the file.
//!
//! Path: `target/ssh_session.json` (debug) / `ssh_session.json` (release)
//! — same pattern as `terminal.json` and `docks.json`.

use std::path::PathBuf;

use gpui::{App, AppContext, Entity, Global};
use serde::{Deserialize, Serialize};

// ── Config path ──────────────────────────────────────────────────────

#[cfg(debug_assertions)]
const SESSION_FILE: &str = "target/ssh_session.json";
#[cfg(not(debug_assertions))]
const SESSION_FILE: &str = "ssh_session.json";

// ── SshSession ───────────────────────────────────────────────────────

/// A single SSH session entry — stored in `ssh_session.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshSession {
    /// Display label in the SessionPanel.
    pub label: String,
    /// Hostname or IP.
    pub host: String,
    /// SSH port (default 22).
    #[serde(default = "default_port")]
    pub port: u16,
    /// Username (optional — can be entered at connect time).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Display color (hex string, e.g. "#58c4dc"). Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Group (optional — used to group sessions in the Tree).
    pub group: Option<String>,
}

fn default_port() -> u16 {
    SshSession::DEFAULT_PORT
}

impl SshSession {
    /// Default SSH port.
    pub const DEFAULT_PORT: u16 = 22;
}

// ── Store ────────────────────────────────────────────────────────────

/// Global store holding the list of [`SshSession`].
///
/// Loaded from `ssh_session.json` at `init`, saved when a new session is added.
/// Rendered via [`crate::views::SessionPanel`] — the panel observes this entity
/// to re-render when the list changes.
pub struct SshSessionStore {
    sessions: Vec<SshSession>,
}

impl SshSessionStore {
    /// The session list (immutable).
    pub fn sessions(&self) -> &[SshSession] {
        &self.sessions
    }

    /// Add a new session + save the file + notify observers.
    pub fn add(&mut self, session: SshSession, cx: &mut gpui::Context<Self>) {
        self.sessions.push(session);
        cx.notify();
        self.save();
    }

    /// Update the session at `index` + save the file + notify observers.
    /// No-op if `index` is out of range.
    pub fn update(&mut self, index: usize, session: SshSession, cx: &mut gpui::Context<Self>) {
        if let Some(slot) = self.sessions.get_mut(index) {
            *slot = session;
            cx.notify();
            self.save();
        }
    }

    /// Rename a group — update all sessions with `group == old_name` to `new_name`
    /// + save the file + notify observers.
    /// If `new_name` is empty (or whitespace only) → set group = None (ungroup).
    pub fn rename_group(&mut self, old_name: &str, new_name: &str, cx: &mut gpui::Context<Self>) {
        let new_group = if new_name.trim().is_empty() {
            None
        } else {
            Some(new_name.trim().to_string())
        };
        let mut changed = false;
        for s in &mut self.sessions {
            if s.group.as_deref() == Some(old_name) {
                s.group = new_group.clone();
                changed = true;
            }
        }
        if changed {
            cx.notify();
            self.save();
        }
    }
    /// Remove the session at `index` + save the file + notify observers.
    /// No-op if `index` is out of range.
    pub fn remove(&mut self, index: usize, cx: &mut gpui::Context<Self>) {
        if index < self.sessions.len() {
            self.sessions.remove(index);
            cx.notify();
            self.save();
        }
    }

    /// Load the session list from `ssh_session.json`.
    /// If the file does not exist or fails to parse → return an empty list.
    fn load() -> Vec<SshSession> {
        let path = PathBuf::from(SESSION_FILE);
        match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<Vec<SshSession>>(&raw) {
                Ok(list) => list,
                Err(e) => {
                    log::error!("ssh_session.json parse error: {e} — starting empty");
                    Vec::new()
                }
            },
            Err(_) => {
                // File does not exist yet — no sessions; don't create an empty file
                // to avoid writing a blank file before the user adds any session.
                Vec::new()
            }
        }
    }

    /// Save the session list to `ssh_session.json` (pretty-printed).
    fn save(&self) {
        let path = PathBuf::from(SESSION_FILE);
        match serde_json::to_string_pretty(&self.sessions) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    log::error!("Failed to write ssh_session.json: {e}");
                }
            }
            Err(e) => log::error!("Failed to serialize ssh sessions: {e}"),
        }
    }
}

// ── Global wrapper (same pattern as `AppStateGlobal` / `TerminalSettingsGlobal`) ──

/// Global wrapper for `Entity<SshSessionStore>`.
pub struct SshSessionStoreGlobal(pub Entity<SshSessionStore>);

impl Global for SshSessionStoreGlobal {}

impl SshSessionStore {
    /// Get the global `Entity<SshSessionStore>` (panics if not initialized).
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<SshSessionStoreGlobal>().0.clone()
    }

    /// Initialize the global store — load `ssh_session.json` (called from `ui::init`).
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
            color: None,
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
            color: None,
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
