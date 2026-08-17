//! SSH session store — load/save `ssh_session.json`.
//!
//! The list of SSH sessions (label, host, port, username, group) is persisted to
//! `ssh_session.json`. On startup, the store loads the file to render the list in
//! [`crate::SessionPanel`]. When the user adds a new session via the
//! "New Session" dialog, the store updates and re-saves the file.
//!
//! Path: `target/ssh_session.json` (debug) / `~/.OneTerm/ssh_session.json` (release)
//! — same pattern as `terminal.json` and `docks.json`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use gpui::{App, AppContext, Entity, Global};
use oneterm_core::{
    atomic_write, config_dir, migrate_json_value, quarantine_file, set_schema_version,
    versioned_object,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// File path is resolved at runtime via config_dir().join("ssh_session.json") —
// debug → target/, release → ~/.OneTerm/ (see oneterm_core::config_dir).

const CURRENT_SCHEMA_VERSION: u32 = 1;

// ── SshSession ───────────────────────────────────────────────────────

/// Authentication preference persisted for an SSH session.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshAuthPreference {
    /// Ask for an optional password when connecting.
    #[default]
    Password,
    /// Use a private-key file and ask for its optional passphrase when connecting.
    PrivateKey,
}

fn is_password_auth(auth: &SshAuthPreference) -> bool {
    *auth == SshAuthPreference::Password
}

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
    /// Preferred authentication method. Missing values preserve password behavior.
    #[serde(default, skip_serializing_if = "is_password_auth")]
    pub auth_method: SshAuthPreference,
    /// Private-key path. Passphrases are never part of this persisted model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<PathBuf>,
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
/// Rendered via [`crate::SessionPanel`] — the panel observes this entity
/// to re-render when the list changes.
pub struct SshSessionStore {
    sessions: Vec<SshSession>,
    /// Coalescing single-flight queue so background writes never complete
    /// out of order: only the newest pending snapshot reaches disk.
    persist_queue: Arc<Mutex<SessionPersistQueue>>,
}

/// Pending snapshot plus the "a worker is draining" flag.
///
/// Every mutation replaces `pending`; one background worker drains the queue
/// until it is empty. A stale snapshot therefore can never overwrite a newer
/// one, whichever order the executor runs the writes in.
#[derive(Default)]
struct SessionPersistQueue {
    pending: Option<Vec<SshSession>>,
    saving: bool,
}

/// Replace the pending snapshot; returns `true` when the caller must start a
/// drain worker because none is running.
fn enqueue_snapshot(queue: &Arc<Mutex<SessionPersistQueue>>, sessions: Vec<SshSession>) -> bool {
    let mut state = lock_persist_queue(queue);
    state.pending = Some(sessions);
    if state.saving {
        return false;
    }
    state.saving = true;
    true
}

/// Write pending snapshots to `path` until the queue is empty.
fn drain_persist_queue(queue: &Arc<Mutex<SessionPersistQueue>>, path: &Path) {
    loop {
        let snapshot = {
            let mut state = lock_persist_queue(queue);
            match state.pending.take() {
                Some(snapshot) => snapshot,
                None => {
                    state.saving = false;
                    return;
                }
            }
        };
        SshSessionStore::save_snapshot(&snapshot, path);
    }
}

fn lock_persist_queue(
    queue: &Arc<Mutex<SessionPersistQueue>>,
) -> MutexGuard<'_, SessionPersistQueue> {
    match queue.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("ssh session persist queue was poisoned; continuing");
            poisoned.into_inner()
        }
    }
}

impl SshSessionStore {
    fn with_sessions(sessions: Vec<SshSession>) -> Self {
        Self {
            sessions,
            persist_queue: Arc::new(Mutex::new(SessionPersistQueue::default())),
        }
    }

    /// The session list (immutable).
    pub fn sessions(&self) -> &[SshSession] {
        &self.sessions
    }

    /// Add a new session + save the file + notify observers.
    pub fn add(&mut self, session: SshSession, cx: &mut gpui::Context<Self>) {
        self.sessions.push(session);
        cx.notify();
        self.save(cx);
    }

    /// Update the session at `index` + save the file + notify observers.
    /// No-op if `index` is out of range.
    pub fn update(&mut self, index: usize, session: SshSession, cx: &mut gpui::Context<Self>) {
        if let Some(slot) = self.sessions.get_mut(index) {
            *slot = session;
            cx.notify();
            self.save(cx);
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
            self.save(cx);
        }
    }
    /// Remove the session at `index` + save the file + notify observers.
    /// No-op if `index` is out of range.
    pub fn remove(&mut self, index: usize, cx: &mut gpui::Context<Self>) {
        if index < self.sessions.len() {
            self.sessions.remove(index);
            cx.notify();
            self.save(cx);
        }
    }

    /// Load the session list from `ssh_session.json`.
    /// If the file does not exist or fails to parse → return an empty list.
    fn load() -> Vec<SshSession> {
        Self::load_from(&config_dir().join("ssh_session.json"))
    }

    /// Load sessions from an explicit path for deterministic callers and tests.
    fn load_from(path: &Path) -> Vec<SshSession> {
        fn parse_document(raw: &str) -> std::io::Result<Vec<SshSession>> {
            let value: Value = serde_json::from_str(raw).map_err(std::io::Error::other)?;
            let value = migrate_json_value(
                value,
                CURRENT_SCHEMA_VERSION,
                "ssh_session.json",
                |_, value| match value {
                    Value::Array(sessions) => {
                        let mut document = versioned_object(0);
                        document["sessions"] = Value::Array(sessions);
                        Ok(document)
                    }
                    Value::Object(_) => Ok(value),
                    _ => Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "ssh_session.json schema must be an object or legacy array",
                    )),
                },
            )?;
            let sessions = value.get("sessions").cloned().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "ssh_session.json is missing sessions",
                )
            })?;
            serde_json::from_value(sessions).map_err(std::io::Error::other)
        }
        match std::fs::read_to_string(&path) {
            Ok(raw) => match parse_document(&raw) {
                Ok(list) => list,
                Err(e) => {
                    log::error!("ssh_session.json parse error: {e} — starting empty");
                    if let Err(quarantine_error) = quarantine_file(&path) {
                        log::warn!("failed to quarantine ssh_session.json: {quarantine_error}");
                    }
                    Vec::new()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // A missing file means there are no saved sessions yet.
                Vec::new()
            }
            Err(error) => {
                log::error!("failed to read ssh_session.json: {error}; starting empty");
                Vec::new()
            }
        }
    }

    /// Schedule saving a snapshot of the session list off the UI thread.
    ///
    /// Snapshots are coalesced through the single-flight queue so back-to-back
    /// mutations always leave the newest state on disk.
    fn save(&self, cx: &gpui::Context<Self>) {
        let queue = self.persist_queue.clone();
        if !enqueue_snapshot(&queue, self.sessions.clone()) {
            return;
        }
        cx.background_executor()
            .spawn(async move {
                drain_persist_queue(&queue, &config_dir().join("ssh_session.json"));
            })
            .detach();
    }

    fn save_snapshot(sessions: &[SshSession], path: &Path) {
        let mut document = versioned_object(CURRENT_SCHEMA_VERSION);
        document["sessions"] = match serde_json::to_value(sessions) {
            Ok(value) => value,
            Err(error) => {
                log::error!("Failed to serialize ssh sessions: {error}");
                return;
            }
        };
        if let Err(error) = set_schema_version(&mut document, CURRENT_SCHEMA_VERSION) {
            log::error!("Failed to version ssh sessions: {error}");
            return;
        }
        match serde_json::to_string_pretty(&document) {
            Ok(json) => {
                if let Err(e) = atomic_write(path, json.as_bytes()) {
                    log::error!("Failed to write ssh_session.json: {e}");
                }
            }
            Err(e) => log::error!("Failed to serialize ssh sessions: {e}"),
        }
    }

    #[cfg(test)]
    /// Save sessions to an explicit path for deterministic callers and tests.
    fn save_to(&self, path: &Path) {
        Self::save_snapshot(&self.sessions, path);
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
        let entity = cx.new(|_| Self::with_sessions(sessions));
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
        assert_eq!(list[0].auth_method, SshAuthPreference::Password);
        assert!(list[0].key_path.is_none());
    }

    #[test]
    fn serialize_skips_none_username() {
        let session = SshSession {
            label: "a".into(),
            host: "b".into(),
            port: 22,
            username: None,
            auth_method: SshAuthPreference::Password,
            key_path: None,
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
            auth_method: SshAuthPreference::Password,
            key_path: None,
            color: None,
            group: None,
        };
        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"username\":\"root\""));
    }

    #[test]
    fn private_key_metadata_serializes_without_secret_fields() {
        let session = SshSession {
            label: "key host".into(),
            host: "example.test".into(),
            port: 22,
            username: Some("user".into()),
            auth_method: SshAuthPreference::PrivateKey,
            key_path: Some(PathBuf::from("/home/user/.ssh/id_ed25519")),
            color: None,
            group: None,
        };

        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"auth_method\":\"private_key\""));
        assert!(json.contains("\"key_path\""));
        assert!(!json.contains("passphrase"));
        assert!(!json.contains("password"));
    }

    #[test]
    fn empty_array_parses() {
        let list: Vec<SshSession> = serde_json::from_str("[]").unwrap();
        assert!(list.is_empty());
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;

    #[test]
    fn explicit_path_roundtrip_and_corruption_quarantine_are_isolated() {
        let directory = std::env::temp_dir().join(format!(
            "oneterm-session-store-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("ssh_session.json");
        let store = SshSessionStore::with_sessions(vec![SshSession {
            label: "test".into(),
            host: "example.test".into(),
            port: 2222,
            username: Some("user".into()),
            auth_method: SshAuthPreference::PrivateKey,
            key_path: Some(PathBuf::from("/keys/test")),
            color: None,
            group: Some("group".into()),
        }]);
        store.save_to(&path);
        let loaded = SshSessionStore::load_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].auth_method, SshAuthPreference::PrivateKey);
        assert_eq!(loaded[0].key_path.as_deref(), Some(Path::new("/keys/test")));
        std::fs::write(&path, b"not-json").unwrap();
        assert!(SshSessionStore::load_from(&path).is_empty());
        assert!(!path.exists());
        assert!(std::fs::read_dir(&directory).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".invalid-")
        }));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn legacy_fixture_migrates_to_versioned_session_document() {
        let directory = std::env::temp_dir().join(format!(
            "oneterm-session-schema-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("ssh_session.json");
        std::fs::write(
            &path,
            include_str!("../tests/fixtures/persistence/ssh-session-v0.json"),
        )
        .unwrap();
        let sessions = SshSessionStore::load_from(&path);
        assert_eq!(sessions[0].label, "legacy");
        SshSessionStore::save_snapshot(&sessions, &path);
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["schema_version"], CURRENT_SCHEMA_VERSION);
        assert_eq!(value["sessions"][0]["host"], "legacy.example.test");
        assert_eq!(SshSessionStore::load_from(&path)[0].label, "legacy");
        let _ = std::fs::remove_dir_all(directory);
    }

    fn session(label: &str) -> SshSession {
        SshSession {
            label: label.into(),
            host: format!("{label}.example.test"),
            port: SshSession::DEFAULT_PORT,
            username: None,
            auth_method: SshAuthPreference::Password,
            key_path: None,
            color: None,
            group: None,
        }
    }

    #[test]
    fn back_to_back_saves_keep_only_the_newest_snapshot_on_disk() {
        let directory = std::env::temp_dir().join(format!(
            "oneterm-session-queue-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("ssh_session.json");
        let queue = Arc::new(Mutex::new(SessionPersistQueue::default()));

        // add(): the first mutation starts a worker; remove(): a second mutation
        // scheduled before the worker ran only replaces the pending snapshot.
        assert!(enqueue_snapshot(&queue, vec![session("a"), session("b")]));
        assert!(!enqueue_snapshot(&queue, vec![session("a")]));

        drain_persist_queue(&queue, &path);

        let loaded = SshSessionStore::load_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].label, "a");
        // Exactly one write happened, so the older snapshot never reached disk.
        assert!(!directory.join("ssh_session.bak").exists());
        // The drained queue starts a new worker for the next mutation.
        assert!(enqueue_snapshot(&queue, Vec::new()));
        drain_persist_queue(&queue, &path);
        assert!(SshSessionStore::load_from(&path).is_empty());
        let _ = std::fs::remove_dir_all(directory);
    }
}
