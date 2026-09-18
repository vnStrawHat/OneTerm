//! SSH session store — load/save `ssh_session.json`.
//!
//! The list of SSH sessions (label, host, port, username, group) is persisted to
//! `ssh_session.json`. On startup, the store loads the file to render the list in
//! [`crate::SessionPanel`]. When the user adds a new session via the
//! "New Session" dialog, the store updates and re-saves the file.
//!
//! Path: `target/ssh_session.json` (debug) / `~/.OneTerm/ssh_session.json` (release)
//! — same pattern as `terminal.json` and `docks.json`.
//!
//! # Schema
//!
//! ```json
//! { "schema_version": 2, "next_session_id": 4,
//!   "sessions": [ { "id": 1, "label": "prod", "host": "10.0.0.1", "port": 22, ... } ] }
//! ```
//!
//! - **v0** (no version field): a bare array of sessions.
//! - **v1**: `{ "schema_version": 1, "sessions": [...] }`.
//! - **v2**: every session carries a stable `id` and the document records
//!   `next_session_id`, the id the next added session receives. Ids never
//!   change and are never reused within one document, so the panel, its
//!   dialogs and the tree address a session by id — reordering or deleting
//!   another session while a dialog is open cannot retarget it.
//!
//! Loading migrates v0/v1 in memory (ids are assigned by position) and re-saves
//! the file as v2. A v2 document whose entries lack an `id` (hand-edited) or
//! repeat one is repaired the same way. Nothing else is dropped.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gpui::{App, AppContext, Entity, Global};
use oneterm_core::{
    AppError, atomic_write, config_dir, migrate_json_value, quarantine_file, set_schema_version,
    versioned_object,
};
use oneterm_core::{MAX_JUMP_HOPS, PortForward};
use oneterm_state::PersistQueue;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

// File path is resolved at runtime via config_dir().join("ssh_session.json") —
// debug → target/, release → ~/.OneTerm/ (see oneterm_core::config_dir).

const CURRENT_SCHEMA_VERSION: u32 = 2;
const DOCUMENT_NAME: &str = "ssh_session.json";
const NEXT_ID_FIELD: &str = "next_session_id";
const SESSIONS_FIELD: &str = "sessions";
const ID_FIELD: &str = "id";

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
    /// Sign with an identity held by the local SSH agent; nothing to ask for.
    Agent,
}

fn is_password_auth(auth: &SshAuthPreference) -> bool {
    *auth == SshAuthPreference::Password
}

/// Saved-session override for automatic terminal logging.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshLoggingOverride {
    /// Use the global SSH logging setting.
    #[default]
    Inherit,
    /// Always start logging this saved session.
    On,
    /// Never start logging this saved session.
    Off,
}

impl SshLoggingOverride {
    /// Resolve this override against the global SSH setting.
    pub const fn resolve(self, global: bool) -> bool {
        match self {
            Self::Inherit => global,
            Self::On => true,
            Self::Off => false,
        }
    }
}

fn is_inherited_logging(value: &SshLoggingOverride) -> bool {
    *value == SshLoggingOverride::Inherit
}

/// Stable identity of a stored SSH session (schema v2). Assigned by the store,
/// never reused within one `ssh_session.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SshSessionId(u64);

impl SshSessionId {
    /// Parse the id from its decimal text (as used in tree item ids).
    pub fn parse(text: &str) -> Option<Self> {
        text.parse::<u64>().ok().map(Self)
    }

    /// The raw id, for a boundary that cannot name this type — the
    /// `WorkspaceCommands` fn pointers in `oneterm-state` sit below this crate.
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Rebuild an id that crossed such a boundary. Like [`Self::parse`], this
    /// does not assert the id exists; the store lookup does.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

impl fmt::Display for SshSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The user-editable connection record of one SSH session — everything the
/// session and connect dialogs read and write. Persisted inside
/// [`SshSessionEntry`], which adds the stable id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Per-session automatic logging behavior.
    #[serde(default, skip_serializing_if = "is_inherited_logging")]
    pub logging: SshLoggingOverride,
    /// The saved session to connect through first (DEC-0010); that session's
    /// own `jump_host` extends the chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump_host: Option<SshSessionId>,
    /// Port forwards started with every connection of this session (US-0059).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub port_forwards: Vec<PortForward>,
    /// Forward the local SSH agent to the remote host (US-0060); off by default.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub agent_forwarding: bool,
}

/// Why a `jump_host` chain cannot be followed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JumpChainError {
    /// A referenced session no longer exists.
    Missing(SshSessionId),
    /// The chain returns to a session already on it (or to the target).
    Cycle(SshSessionId),
    /// More than `MAX_JUMP_HOPS` hops.
    TooLong(usize),
}

impl fmt::Display for JumpChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(id) => write!(f, "jump host session {id} no longer exists"),
            Self::Cycle(_) => f.write_str("the jump host chain loops back on itself"),
            Self::TooLong(count) => write!(
                f,
                "the jump host chain has {count} hops; at most {MAX_JUMP_HOPS} are supported"
            ),
        }
    }
}

fn default_port() -> u16 {
    SshSession::DEFAULT_PORT
}

impl SshSession {
    /// Default SSH port.
    pub const DEFAULT_PORT: u16 = 22;
    /// Colour tag a new session gets in the session dialog (persisted per
    /// session, so it is a data default rather than a theme token).
    pub const DEFAULT_COLOR_HEX: &'static str = "#56B6C2";
}

/// One row of `ssh_session.json`: a stable id plus the session record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshSessionEntry {
    pub id: SshSessionId,
    #[serde(flatten)]
    pub session: SshSession,
}

// ── Store ────────────────────────────────────────────────────────────

/// Global store holding the list of [`SshSessionEntry`].
///
/// Loaded from `ssh_session.json` at `init`, saved on every mutation.
/// Rendered via [`crate::SessionPanel`] — the panel observes this entity
/// to re-render when the list changes.
pub struct SshSessionStore {
    entries: Vec<SshSessionEntry>,
    /// The id handed to the next added session (persisted as `next_session_id`).
    next_id: u64,
    /// Coalescing single-flight queue so background writes never complete
    /// out of order: only the newest pending snapshot reaches disk.
    persist_queue: Arc<Mutex<PersistQueue<SessionDocument>>>,
    /// `ssh_session.json` existed but could not be read at startup, so this
    /// store started empty and must not overwrite the possibly valid file
    /// (CORR-61).
    persist_blocked: bool,
}

/// What one save writes: the complete session list plus the id counter.
#[derive(Debug, Clone, PartialEq)]
struct SessionDocument {
    entries: Vec<SshSessionEntry>,
    next_id: u64,
}

/// Outcome of reading `ssh_session.json`.
#[derive(Debug)]
struct LoadedSessions {
    document: SessionDocument,
    /// The on-disk file was an older schema or was repaired; write it back.
    needs_resave: bool,
}

impl LoadedSessions {
    /// No saved sessions yet.
    fn empty() -> Self {
        Self {
            document: SessionDocument {
                entries: Vec::new(),
                next_id: 1,
            },
            needs_resave: false,
        }
    }
}

/// Write pending snapshots to `path` until the queue is empty.
fn drain_persist_queue(queue: &Arc<Mutex<PersistQueue<SessionDocument>>>, path: &Path) {
    PersistQueue::drain(queue, |snapshot| {
        SshSessionStore::save_snapshot(snapshot, path)
    });
}

impl SshSessionStore {
    fn with_document(document: SessionDocument) -> Self {
        Self {
            entries: document.entries,
            next_id: document.next_id,
            persist_queue: PersistQueue::new(),
            persist_blocked: false,
        }
    }

    /// The stored sessions with their ids, in storage order.
    pub fn sessions(&self) -> &[SshSessionEntry] {
        &self.entries
    }

    /// The session with `id`, if it still exists.
    pub fn get(&self, id: SshSessionId) -> Option<&SshSession> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| &entry.session)
    }

    /// Follow `jump_host` links from `first` and return the hops outermost
    /// first. `target` is the session being connected or edited: a chain that
    /// reaches it is a cycle. `None` for `first` is the empty chain.
    pub fn jump_chain(
        &self,
        first: Option<SshSessionId>,
        target: Option<SshSessionId>,
    ) -> Result<Vec<SshSessionEntry>, JumpChainError> {
        let mut chain: Vec<SshSessionEntry> = Vec::new();
        let mut next = first;
        while let Some(id) = next {
            if Some(id) == target || chain.iter().any(|entry| entry.id == id) {
                return Err(JumpChainError::Cycle(id));
            }
            if chain.len() == MAX_JUMP_HOPS {
                return Err(JumpChainError::TooLong(chain.len() + 1));
            }
            let entry = self
                .entries
                .iter()
                .find(|entry| entry.id == id)
                .ok_or(JumpChainError::Missing(id))?;
            next = entry.session.jump_host;
            chain.push(entry.clone());
        }
        chain.reverse();
        Ok(chain)
    }

    /// Add a new session under a fresh id + save the file + notify observers.
    /// Returns the id the session was stored under.
    pub fn add(&mut self, session: SshSession, cx: &mut gpui::Context<Self>) -> SshSessionId {
        let id = SshSessionId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.entries.push(SshSessionEntry { id, session });
        cx.notify();
        self.save(cx);
        id
    }

    /// Replace the session stored under `id` + save the file + notify observers.
    /// No-op (logged) when `id` no longer exists.
    pub fn update(&mut self, id: SshSessionId, session: SshSession, cx: &mut gpui::Context<Self>) {
        match self.entries.iter_mut().find(|entry| entry.id == id) {
            Some(entry) => {
                entry.session = session;
                cx.notify();
                self.save(cx);
            }
            None => log::warn!("SshSessionStore::update: session {id} no longer exists"),
        }
    }

    /// Duplicate the saved session `id`: a copy of every field under a fresh id,
    /// placed right after its source in storage order (`US-0129`). Returns the
    /// new id, or `None` when `id` no longer exists.
    ///
    /// This copies a **saved** session. The live tab's "Duplicate" is a
    /// different thing entirely ([`oneterm_core::SshDuplicateConfig`]): it
    /// reopens a running connection. Nothing here connects.
    pub fn duplicate(
        &mut self,
        id: SshSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> Option<SshSessionId> {
        let new_id = SshSessionId(self.next_id);
        if !duplicate_in(&mut self.entries, id, new_id) {
            log::warn!("SshSessionStore::duplicate: session {id} no longer exists");
            return None;
        }
        self.next_id = self.next_id.saturating_add(1);
        cx.notify();
        self.save(cx);
        Some(new_id)
    }

    /// Move the saved session `id` into `group` (`None`, or a blank name, means
    /// ungrouped) + save the file + notify observers. No-op when `id` no longer
    /// exists or the group would not change.
    pub fn set_group(
        &mut self,
        id: SshSessionId,
        group: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        if set_group_in(&mut self.entries, id, group) {
            cx.notify();
            self.save(cx);
        }
    }

    /// Rename a group — update all sessions with `group == old_name` to `new_name`
    /// + save the file + notify observers.
    /// If `new_name` is empty (or whitespace only) → set group = None (ungroup).
    pub fn rename_group(&mut self, old_name: &str, new_name: &str, cx: &mut gpui::Context<Self>) {
        if rename_group_in(&mut self.entries, old_name, new_name) {
            cx.notify();
            self.save(cx);
        }
    }

    /// Remove the session stored under `id` + save the file + notify observers.
    /// No-op when `id` no longer exists.
    pub fn remove(&mut self, id: SshSessionId, cx: &mut gpui::Context<Self>) {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        if self.entries.len() != before {
            cx.notify();
            self.save(cx);
        }
    }

    /// Load the session list from `ssh_session.json`. See [`Self::load_from`].
    fn load() -> Result<LoadedSessions, AppError> {
        Self::load_from(&config_dir().join(DOCUMENT_NAME))
    }

    /// Load sessions from an explicit path for deterministic callers and tests.
    ///
    /// A missing file means no saved sessions yet; a file that does not parse
    /// or migrate is quarantined (with a recovery log) and an empty list is
    /// returned. Any other read failure is returned as [`AppError::ConfigLoad`]
    /// so the caller does not overwrite a possibly valid file (CORR-61).
    fn load_from(path: &Path) -> Result<LoadedSessions, AppError> {
        match std::fs::read_to_string(path) {
            Ok(raw) => match parse_document(&raw) {
                Ok(loaded) => Ok(loaded),
                Err(e) => {
                    log::error!("{e} — starting empty");
                    if let Err(quarantine_error) = quarantine_file(path) {
                        log::warn!("failed to quarantine {DOCUMENT_NAME}: {quarantine_error}");
                    }
                    Ok(LoadedSessions::empty())
                }
            },
            // A missing file means there are no saved sessions yet.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(LoadedSessions::empty())
            }
            Err(error) => Err(AppError::config_load(DOCUMENT_NAME, error)),
        }
    }

    fn document(&self) -> SessionDocument {
        SessionDocument {
            entries: self.entries.clone(),
            next_id: self.next_id,
        }
    }

    /// Schedule saving a snapshot of the session list off the UI thread.
    ///
    /// Snapshots are coalesced through the single-flight queue so back-to-back
    /// mutations always leave the newest state on disk.
    fn save(&self, cx: &gpui::Context<Self>) {
        if self.persist_blocked {
            log::warn!(
                "{DOCUMENT_NAME} could not be read at startup; refusing to overwrite it with the in-memory list"
            );
            return;
        }
        let queue = self.persist_queue.clone();
        if !PersistQueue::enqueue(&queue, self.document()) {
            return;
        }
        cx.background_executor()
            .spawn(async move {
                drain_persist_queue(&queue, &config_dir().join(DOCUMENT_NAME));
            })
            .detach();
    }

    fn save_snapshot(document: &SessionDocument, path: &Path) {
        let mut value = versioned_object(CURRENT_SCHEMA_VERSION);
        value[SESSIONS_FIELD] = match serde_json::to_value(&document.entries) {
            Ok(value) => value,
            Err(error) => {
                log::error!("Failed to serialize ssh sessions: {error}");
                return;
            }
        };
        value[NEXT_ID_FIELD] = Value::from(document.next_id);
        if let Err(error) = set_schema_version(&mut value, CURRENT_SCHEMA_VERSION) {
            log::error!("Failed to version ssh sessions: {error}");
            return;
        }
        match serde_json::to_string_pretty(&value) {
            Ok(json) => {
                if let Err(e) = atomic_write(path, json.as_bytes()) {
                    log::error!("Failed to write {DOCUMENT_NAME}: {e}");
                }
            }
            Err(e) => log::error!("Failed to serialize ssh sessions: {e}"),
        }
    }
}

/// Move every session in group `old_name` to `new_name` (trimmed; empty →
/// ungrouped). Returns whether any session changed.
fn rename_group_in(entries: &mut [SshSessionEntry], old_name: &str, new_name: &str) -> bool {
    let new_group = if new_name.trim().is_empty() {
        None
    } else {
        Some(new_name.trim().to_string())
    };
    let mut changed = false;
    for entry in entries {
        if entry.session.group.as_deref() == Some(old_name) {
            entry.session.group = new_group.clone();
            changed = true;
        }
    }
    changed
}

/// The label a duplicate of `label` gets: `"<label> (copy)"`, or
/// `"<label> (copy 2)"`, `"(copy 3)"`, … when an earlier candidate is already a
/// label in `entries` (`US-0129`).
///
/// The rule appends to whatever label it is handed, so duplicating a copy gives
/// `prod (copy) (copy)` rather than `prod (copy 2)`: one rule instead of two,
/// and the user is about to rename it in the dialog the duplicate opens.
/// Comparison is exact — two sessions may legitimately share a label (the "+"
/// menu says so), and only the candidate this function invents must be unused.
fn copy_label(label: &str, entries: &[SshSessionEntry]) -> String {
    let taken = |candidate: &str| entries.iter().any(|entry| entry.session.label == candidate);
    let mut candidate = format!("{label} (copy)");
    let mut suffix = 2u32;
    while taken(&candidate) {
        candidate = format!("{label} (copy {suffix})");
        suffix += 1;
    }
    candidate
}

/// Insert a copy of the session `id` — every field, relabelled by
/// [`copy_label`] — right after it, under `new_id`. Returns whether it happened.
///
/// The session is cloned whole rather than rebuilt field by field: `SshSession`
/// has gained three fields since schema v2 and a hand-written list would have
/// silently stopped copying each of them.
fn duplicate_in(
    entries: &mut Vec<SshSessionEntry>,
    id: SshSessionId,
    new_id: SshSessionId,
) -> bool {
    let Some(index) = entries.iter().position(|entry| entry.id == id) else {
        return false;
    };
    let mut session = entries[index].session.clone();
    session.label = copy_label(&session.label, entries);
    entries.insert(
        index + 1,
        SshSessionEntry {
            id: new_id,
            session,
        },
    );
    true
}

/// A group name as the store keeps it: trimmed, and `None` when blank — the
/// convention [`rename_group_in`] and the tree builder already share.
///
/// The "Move to Group" submenu checks its rows through this too, so the row it
/// marks as current is decided by the same rule the click then writes: a
/// hand-edited `"group": " infra "` checks `infra` rather than nothing
/// (`US-0129` verification, F2).
pub(crate) fn normalized_group(group: Option<&str>) -> Option<String> {
    group
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// Move the session `id` into `group` (see [`normalized_group`]). Returns
/// whether anything changed.
fn set_group_in(entries: &mut [SshSessionEntry], id: SshSessionId, group: Option<&str>) -> bool {
    let group = normalized_group(group);
    match entries.iter_mut().find(|entry| entry.id == id) {
        Some(entry) if entry.session.group != group => {
            entry.session.group = group;
            true
        }
        _ => false,
    }
}

/// Parse and migrate one `ssh_session.json` document.
fn parse_document(raw: &str) -> Result<LoadedSessions, AppError> {
    parse_document_inner(raw).map_err(|error| AppError::config_load(DOCUMENT_NAME, error))
}

fn parse_document_inner(raw: &str) -> std::io::Result<LoadedSessions> {
    let invalid = |message: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, message);
    let value: Value = serde_json::from_str(raw).map_err(std::io::Error::other)?;
    let source_version = oneterm_core::schema_version(&value).unwrap_or(0);
    let value = migrate_json_value(
        value,
        CURRENT_SCHEMA_VERSION,
        DOCUMENT_NAME,
        |version, value| match (version, value) {
            // v0: a bare array → wrap it in the versioned envelope.
            (0, Value::Array(sessions)) => {
                let mut document = versioned_object(0);
                document[SESSIONS_FIELD] = Value::Array(sessions);
                Ok(document)
            }
            (0, value @ Value::Object(_)) => Ok(value),
            // v1 → v2: assign ids by position and record the counter.
            (1, mut value @ Value::Object(_)) => {
                assign_missing_ids(&mut value)?;
                Ok(value)
            }
            _ => Err(invalid(
                "ssh_session.json schema must be an object or legacy array",
            )),
        },
    )?;

    // A v2 file may still have hand-edited rows without an id (or with a
    // duplicated one); repair them the same way a migration would.
    let mut value = value;
    let repaired = assign_missing_ids(&mut value)?;

    let sessions = value
        .get(SESSIONS_FIELD)
        .cloned()
        .ok_or_else(|| invalid("ssh_session.json is missing sessions"))?;
    let entries: Vec<SshSessionEntry> =
        serde_json::from_value(sessions).map_err(std::io::Error::other)?;
    let next_id = value
        .get(NEXT_ID_FIELD)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("ssh_session.json is missing next_session_id"))?;
    Ok(LoadedSessions {
        document: SessionDocument { entries, next_id },
        needs_resave: source_version < CURRENT_SCHEMA_VERSION || repaired,
    })
}

/// Give every session without a valid, unique `id` a fresh one and make
/// `next_session_id` exceed every id in use. Returns whether anything changed.
fn assign_missing_ids(document: &mut Value) -> std::io::Result<bool> {
    let invalid = |message: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, message);
    let recorded_next = document
        .get(NEXT_ID_FIELD)
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let sessions = document
        .get_mut(SESSIONS_FIELD)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| invalid("ssh_session.json sessions must be an array"))?;

    let mut used: HashSet<u64> = HashSet::new();
    let mut max_id = 0u64;
    let mut missing = Vec::new();
    for (index, session) in sessions.iter().enumerate() {
        match session.get(ID_FIELD).and_then(Value::as_u64) {
            Some(id) if id > 0 && used.insert(id) => max_id = max_id.max(id),
            _ => missing.push(index),
        }
    }

    let mut next_id = recorded_next.max(max_id.saturating_add(1)).max(1);
    let mut changed = false;
    for index in missing {
        let Some(session) = sessions.get_mut(index).and_then(Value::as_object_mut) else {
            return Err(invalid("ssh_session.json sessions must be objects"));
        };
        session.insert(ID_FIELD.to_string(), json!(next_id));
        next_id = next_id.saturating_add(1);
        changed = true;
    }
    if document.get(NEXT_ID_FIELD).and_then(Value::as_u64) != Some(next_id) {
        document[NEXT_ID_FIELD] = json!(next_id);
        changed = true;
    }
    Ok(changed)
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
    /// A file in an older schema is written back in the current one right away.
    pub fn init(cx: &mut App) {
        let (loaded, persist_blocked) = match Self::load() {
            Ok(loaded) => (loaded, false),
            Err(error) => {
                log::error!("{error}; starting empty and refusing to overwrite the file");
                (LoadedSessions::empty(), true)
            }
        };
        let needs_resave = loaded.needs_resave;
        let entity = cx.new(|_| Self {
            persist_blocked,
            ..Self::with_document(loaded.document)
        });
        if needs_resave {
            log::info!("{DOCUMENT_NAME}: migrated to schema v{CURRENT_SCHEMA_VERSION}; re-saving");
            entity.update(cx, |store, cx| store.save(cx));
        }
        cx.set_global(SshSessionStoreGlobal(entity));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_logging_override_has_explicit_precedence() {
        assert!(!SshLoggingOverride::Inherit.resolve(false));
        assert!(SshLoggingOverride::Inherit.resolve(true));
        assert!(SshLoggingOverride::On.resolve(false));
        assert!(!SshLoggingOverride::Off.resolve(true));
    }

    #[test]
    fn missing_ssh_logging_override_inherits() {
        let session: SshSession =
            serde_json::from_str(r#"{ "label": "dev", "host": "localhost" }"#).unwrap();
        assert_eq!(session.logging, SshLoggingOverride::Inherit);
        assert!(!serde_json::to_string(&session).unwrap().contains("logging"));
    }

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
            logging: SshLoggingOverride::Inherit,
            jump_host: None,
            port_forwards: Vec::new(),
            agent_forwarding: false,
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
            logging: SshLoggingOverride::Inherit,
            jump_host: None,
            port_forwards: Vec::new(),
            agent_forwarding: false,
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
            logging: SshLoggingOverride::Inherit,
            jump_host: None,
            port_forwards: Vec::new(),
            agent_forwarding: false,
        };

        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"auth_method\":\"private_key\""));
        assert!(json.contains("\"key_path\""));
        assert!(!json.contains("passphrase"));
        assert!(!json.contains("password"));
    }

    #[test]
    fn agent_auth_round_trips_and_carries_no_key_path() {
        let session = SshSession {
            label: "agent host".into(),
            host: "example.test".into(),
            port: 22,
            username: Some("user".into()),
            auth_method: SshAuthPreference::Agent,
            key_path: None,
            color: None,
            group: None,
            logging: SshLoggingOverride::Inherit,
            jump_host: None,
            port_forwards: Vec::new(),
            agent_forwarding: false,
        };

        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"auth_method\":\"agent\""));
        assert!(!json.contains("key_path"));
        let back: SshSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back.auth_method, SshAuthPreference::Agent);
        assert_eq!(back, session);
    }

    #[test]
    fn jump_host_round_trips_and_is_omitted_when_absent() {
        let mut session = SshSession {
            label: "target".into(),
            host: "10.0.5.20".into(),
            port: 22,
            username: Some("deploy".into()),
            auth_method: SshAuthPreference::Password,
            key_path: None,
            color: None,
            group: None,
            logging: SshLoggingOverride::Inherit,
            jump_host: None,
            port_forwards: Vec::new(),
            agent_forwarding: false,
        };
        assert!(
            !serde_json::to_string(&session)
                .unwrap()
                .contains("jump_host")
        );

        session.jump_host = Some(SshSessionId(3));
        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"jump_host\":3"), "{json}");
        let back: SshSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back.jump_host, Some(SshSessionId(3)));
    }

    fn chained_store(links: &[(u64, Option<u64>)]) -> SshSessionStore {
        let mut entries = Vec::new();
        for (id, jump) in links {
            entries.push(SshSessionEntry {
                id: SshSessionId(*id),
                session: SshSession {
                    label: format!("s{id}"),
                    host: "h".into(),
                    port: 22,
                    username: Some("u".into()),
                    auth_method: SshAuthPreference::Password,
                    key_path: None,
                    color: None,
                    group: None,
                    logging: SshLoggingOverride::Inherit,
                    jump_host: jump.map(SshSessionId),
                    port_forwards: Vec::new(),
                    agent_forwarding: false,
                },
            });
        }
        SshSessionStore::with_document(SessionDocument {
            entries,
            next_id: 100,
        })
    }

    #[test]
    fn jump_chain_lists_hops_outermost_first() {
        // target 1 -> bastion 2 -> outer relay 3 (no further hop)
        let store = chained_store(&[(1, Some(2)), (2, Some(3)), (3, None)]);
        let chain = store
            .jump_chain(Some(SshSessionId(2)), Some(SshSessionId(1)))
            .unwrap();
        let ids: Vec<u64> = chain.iter().map(|entry| entry.id.0).collect();
        assert_eq!(ids, vec![3, 2]);
        assert!(
            store
                .jump_chain(None, Some(SshSessionId(1)))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn jump_chain_rejects_missing_cycles_and_long_chains() {
        let store = chained_store(&[(1, Some(2)), (2, Some(1)), (3, Some(9))]);
        assert_eq!(
            store.jump_chain(Some(SshSessionId(2)), Some(SshSessionId(1))),
            Err(JumpChainError::Cycle(SshSessionId(1)))
        );
        // A self-contained loop that never reaches the target is a cycle too.
        assert_eq!(
            store.jump_chain(Some(SshSessionId(1)), None),
            Err(JumpChainError::Cycle(SshSessionId(1)))
        );
        assert_eq!(
            store.jump_chain(Some(SshSessionId(3)), None),
            Err(JumpChainError::Missing(SshSessionId(9)))
        );

        let long = chained_store(&[
            (1, Some(2)),
            (2, Some(3)),
            (3, Some(4)),
            (4, Some(5)),
            (5, None),
        ]);
        assert_eq!(
            long.jump_chain(Some(SshSessionId(1)), None),
            Err(JumpChainError::TooLong(5))
        );
        assert_eq!(
            long.jump_chain(Some(SshSessionId(2)), None).unwrap().len(),
            4
        );
    }

    #[test]
    fn port_forwards_round_trip_and_are_omitted_when_empty() {
        let mut session = SshSession {
            label: "target".into(),
            host: "10.0.5.20".into(),
            port: 22,
            username: Some("deploy".into()),
            auth_method: SshAuthPreference::Password,
            key_path: None,
            color: None,
            group: None,
            logging: SshLoggingOverride::Inherit,
            jump_host: None,
            port_forwards: Vec::new(),
            agent_forwarding: false,
        };
        assert!(
            !serde_json::to_string(&session)
                .unwrap()
                .contains("port_forwards")
        );

        session.port_forwards = vec![PortForward::Dynamic {
            bind: oneterm_core::loopback(),
            bind_port: 1080,
        }];
        let json = serde_json::to_string(&session).unwrap();
        assert!(
            json.contains("\"port_forwards\":[{\"kind\":\"dynamic\""),
            "{json}"
        );
        let back: SshSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back, session);
    }

    #[test]
    fn agent_forwarding_is_omitted_when_off_and_round_trips_when_on() {
        let mut session = SshSession {
            label: "target".into(),
            host: "10.0.5.20".into(),
            port: 22,
            username: Some("deploy".into()),
            auth_method: SshAuthPreference::Password,
            key_path: None,
            color: None,
            group: None,
            logging: SshLoggingOverride::Inherit,
            jump_host: None,
            port_forwards: Vec::new(),
            agent_forwarding: false,
        };
        assert!(
            !serde_json::to_string(&session)
                .unwrap()
                .contains("agent_forwarding")
        );
        session.agent_forwarding = true;
        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"agent_forwarding\":true"), "{json}");
        let back: SshSession = serde_json::from_str(&json).unwrap();
        assert!(back.agent_forwarding);
    }

    #[test]
    fn entry_flattens_the_session_next_to_its_id() {
        let entry = SshSessionEntry {
            id: SshSessionId(7),
            session: SshSession {
                label: "a".into(),
                host: "b".into(),
                port: 22,
                username: None,
                auth_method: SshAuthPreference::Password,
                key_path: None,
                color: None,
                group: Some("g".into()),
                logging: SshLoggingOverride::Inherit,
                jump_host: None,
                port_forwards: Vec::new(),
                agent_forwarding: false,
            },
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["id"], 7);
        assert_eq!(json["label"], "a");
        assert_eq!(json["group"], "g");
        let restored: SshSessionEntry = serde_json::from_value(json).unwrap();
        assert_eq!(restored.id, SshSessionId(7));
        assert_eq!(restored.session.host, "b");
    }

    #[test]
    fn empty_array_parses() {
        let list: Vec<SshSession> = serde_json::from_str("[]").unwrap();
        assert!(list.is_empty());
    }

    /// Adopted from the independent verification of `US-0094`: the "+" menu
    /// routes a click by the row's stable id, so deleting an *earlier* session
    /// between the menu opening and the click must still open the session the
    /// user pointed at — the guarantee a positional row id would not give.
    ///
    /// This resolves the id through [`SshSessionStore::get`], which is the call
    /// `oneterm_session_ui::open_saved_ssh_session` makes.
    #[test]
    fn verify_menu_row_id_survives_a_delete_of_an_earlier_session() {
        let host = |id: u64, host: &str| SshSessionEntry {
            id: SshSessionId(id),
            session: SshSession {
                label: "alpha".into(),
                host: host.into(),
                port: SshSession::DEFAULT_PORT,
                username: None,
                auth_method: SshAuthPreference::Password,
                key_path: None,
                color: None,
                group: None,
                logging: SshLoggingOverride::Inherit,
                jump_host: None,
                port_forwards: Vec::new(),
                agent_forwarding: false,
            },
        };
        let before = SshSessionStore::with_document(SessionDocument {
            entries: vec![host(1, "10.9.0.1"), host(2, "10.9.0.2")],
            next_id: 3,
        });
        let sections = crate::tree_builder::menu_entries(before.sessions());
        assert_eq!(sections.len(), 1, "neither session is grouped");
        let rows = &sections[0].1;
        assert_eq!(rows.len(), 2);
        // The row the user is about to click: the second one, id 2.
        let clicked = SshSessionId::from_raw(rows[1].0);
        assert_eq!(clicked, SshSessionId(2));

        // The first session is deleted while the menu is open.
        let after = SshSessionStore::with_document(SessionDocument {
            entries: vec![host(2, "10.9.0.2")],
            next_id: 3,
        });
        assert_eq!(
            after.get(clicked).map(|s| s.host.as_str()),
            Some("10.9.0.2"),
            "the clicked row must still resolve to the session it named"
        );
        assert!(
            after.get(SshSessionId(1)).is_none(),
            "the deleted session resolves to nothing, the documented no-op"
        );
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;

    /// Removes the per-test directory when the test ends — on failure too, so a
    /// panicking assertion never leaks temporary files (ERR-15).
    struct TempDirGuard(std::path::PathBuf);

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            // Best effort: a directory that is already gone must not fail the test.
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl std::ops::Deref for TempDirGuard {
        type Target = std::path::Path;
        fn deref(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl AsRef<std::path::Path> for TempDirGuard {
        fn as_ref(&self) -> &std::path::Path {
            &self.0
        }
    }

    fn temporary_dir(tag: &str) -> TempDirGuard {
        let directory = std::env::temp_dir().join(format!(
            "oneterm-session-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&directory).unwrap();
        TempDirGuard(directory)
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
            logging: SshLoggingOverride::Inherit,
            jump_host: None,
            port_forwards: Vec::new(),
            agent_forwarding: false,
        }
    }

    fn document(entries: Vec<(u64, SshSession)>, next_id: u64) -> SessionDocument {
        SessionDocument {
            entries: entries
                .into_iter()
                .map(|(id, session)| SshSessionEntry {
                    id: SshSessionId(id),
                    session,
                })
                .collect(),
            next_id,
        }
    }

    #[test]
    fn unreadable_document_is_a_typed_load_error_and_is_left_untouched() {
        let directory = temporary_dir("unreadable");
        // A directory in place of the file fails to read with something other
        // than NotFound on every platform, standing in for a permission failure.
        let path = directory.join("ssh_session.json");
        std::fs::create_dir_all(&path).unwrap();
        let error = SshSessionStore::load_from(&path).unwrap_err();
        assert!(
            matches!(&error, AppError::ConfigLoad { document, .. } if document == DOCUMENT_NAME),
            "expected ConfigLoad, got {error}"
        );
        assert!(path.is_dir(), "an unreadable document must not be replaced");
    }

    #[test]
    fn explicit_path_roundtrip_and_corruption_quarantine_are_isolated() {
        let directory = temporary_dir("store");
        let path = directory.join("ssh_session.json");
        let mut key_session = session("test");
        key_session.port = 2222;
        key_session.username = Some("user".into());
        key_session.auth_method = SshAuthPreference::PrivateKey;
        key_session.key_path = Some(PathBuf::from("/keys/test"));
        key_session.group = Some("group".into());
        SshSessionStore::save_snapshot(&document(vec![(3, key_session)], 4), &path);

        let loaded = SshSessionStore::load_from(&path).unwrap();
        assert!(!loaded.needs_resave);
        assert_eq!(loaded.document.entries.len(), 1);
        assert_eq!(loaded.document.entries[0].id, SshSessionId(3));
        assert_eq!(loaded.document.next_id, 4);
        let session = &loaded.document.entries[0].session;
        assert_eq!(session.auth_method, SshAuthPreference::PrivateKey);
        assert_eq!(session.key_path.as_deref(), Some(Path::new("/keys/test")));

        std::fs::write(&path, b"not-json").unwrap();
        assert!(
            SshSessionStore::load_from(&path)
                .unwrap()
                .document
                .entries
                .is_empty()
        );
        assert!(!path.exists());
        assert!(std::fs::read_dir(&directory).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".invalid-")
        }));
    }

    #[test]
    fn legacy_array_migrates_to_versioned_document_with_ids() {
        let directory = temporary_dir("schema-v0");
        let path = directory.join("ssh_session.json");
        std::fs::write(
            &path,
            include_str!("../tests/fixtures/persistence/ssh-session-v0.json"),
        )
        .unwrap();
        let loaded = SshSessionStore::load_from(&path).unwrap();
        assert!(loaded.needs_resave);
        assert_eq!(loaded.document.entries[0].session.label, "legacy");
        assert_eq!(loaded.document.entries[0].id, SshSessionId(1));
        assert_eq!(loaded.document.next_id, 2);

        SshSessionStore::save_snapshot(&loaded.document, &path);
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["schema_version"], CURRENT_SCHEMA_VERSION);
        assert_eq!(value["next_session_id"], 2);
        assert_eq!(value["sessions"][0]["id"], 1);
        assert_eq!(value["sessions"][0]["host"], "legacy.example.test");
        let reloaded = SshSessionStore::load_from(&path).unwrap();
        assert!(!reloaded.needs_resave);
        assert_eq!(reloaded.document, loaded.document);
    }

    /// ARCH-35: a v1 file (no ids) loads without loss, gets ids by position,
    /// and reads back identically once re-saved as v2.
    #[test]
    fn v1_document_gains_ids_and_is_idempotent_after_resave() {
        let directory = temporary_dir("schema-v1");
        let path = directory.join("ssh_session.json");
        std::fs::write(
            &path,
            include_str!("../tests/fixtures/persistence/ssh-session-v1.json"),
        )
        .unwrap();

        let loaded = SshSessionStore::load_from(&path).unwrap();
        assert!(loaded.needs_resave);
        let ids: Vec<_> = loaded.document.entries.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![SshSessionId(1), SshSessionId(2), SshSessionId(3)]);
        assert_eq!(loaded.document.next_id, 4);
        let labels: Vec<_> = loaded
            .document
            .entries
            .iter()
            .map(|e| e.session.label.as_str())
            .collect();
        assert_eq!(labels, vec!["alpha", "beta", "gamma"]);
        // Every field survives: the key-based entry keeps its auth metadata.
        let beta = &loaded.document.entries[1].session;
        assert_eq!(beta.auth_method, SshAuthPreference::PrivateKey);
        assert_eq!(beta.key_path.as_deref(), Some(Path::new("/keys/beta")));
        assert_eq!(beta.group.as_deref(), Some("infra"));
        assert_eq!(beta.color.as_deref(), Some("#112233"));

        SshSessionStore::save_snapshot(&loaded.document, &path);
        let reloaded = SshSessionStore::load_from(&path).unwrap();
        assert!(!reloaded.needs_resave);
        assert_eq!(reloaded.document, loaded.document);
    }

    /// A v2 file with a hand-edited row lacking `id` (and a duplicate id) is
    /// repaired without touching the valid ids.
    #[test]
    fn v2_rows_without_unique_ids_are_repaired() {
        let raw = r#"{
            "schema_version": 2,
            "next_session_id": 3,
            "sessions": [
                { "id": 5, "label": "keep", "host": "a", "group": null },
                { "label": "no-id", "host": "b", "group": null },
                { "id": 5, "label": "dup", "host": "c", "group": null }
            ]
        }"#;
        let loaded = parse_document(raw).unwrap();
        assert!(loaded.needs_resave);
        let ids: Vec<_> = loaded.document.entries.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![SshSessionId(5), SshSessionId(6), SshSessionId(7)]);
        assert_eq!(loaded.document.next_id, 8);
    }

    /// TEST-19: renaming a group moves exactly its members; an empty new name
    /// ungroups them; an unknown group changes nothing.
    #[test]
    fn rename_group_moves_only_its_members() {
        let mut infra = session("infra-1");
        infra.group = Some("infra".into());
        let mut other = session("other");
        other.group = Some("other".into());
        let mut entries = document(vec![(1, infra), (2, other), (3, session("solo"))], 4).entries;

        assert!(rename_group_in(&mut entries, "infra", "  platform  "));
        assert_eq!(entries[0].session.group.as_deref(), Some("platform"));
        assert_eq!(entries[1].session.group.as_deref(), Some("other"));
        assert_eq!(entries[2].session.group, None);

        assert!(rename_group_in(&mut entries, "platform", "   "));
        assert_eq!(entries[0].session.group, None);
        assert!(!rename_group_in(&mut entries, "missing", "x"));
    }

    /// `US-0129`: " (copy)", then " (copy 2)", " (copy 3)" — and the rule
    /// appends to whatever label it is handed.
    #[test]
    fn copy_label_numbers_only_the_candidates_already_in_use() {
        let entries = |labels: &[&str]| -> Vec<SshSessionEntry> {
            labels
                .iter()
                .enumerate()
                .map(|(index, label)| SshSessionEntry {
                    id: SshSessionId(index as u64 + 1),
                    session: session(label),
                })
                .collect()
        };
        assert_eq!(copy_label("prod", &entries(&["prod"])), "prod (copy)");
        assert_eq!(
            copy_label("prod", &entries(&["prod", "prod (copy)"])),
            "prod (copy 2)"
        );
        assert_eq!(
            copy_label("prod", &entries(&["prod", "prod (copy)", "prod (copy 2)"])),
            "prod (copy 3)"
        );
        // A gap is filled rather than skipped past.
        assert_eq!(
            copy_label("prod", &entries(&["prod (copy)", "prod (copy 3)"])),
            "prod (copy 2)"
        );
        // Duplicating a copy appends again; the user renames it in the dialog.
        assert_eq!(
            copy_label("prod (copy)", &entries(&["prod (copy)"])),
            "prod (copy) (copy)"
        );
        // Another session's label is no obstacle, and an empty store is fine.
        assert_eq!(copy_label("prod", &entries(&["staging"])), "prod (copy)");
        assert_eq!(copy_label("prod", &[]), "prod (copy)");
    }

    /// The copy carries every field of its source, sits directly after it, and
    /// takes the id it was given — not the source's, and not a reused one.
    #[test]
    fn duplicate_copies_every_field_next_to_its_source() {
        let mut source = session("prod");
        source.port = 2222;
        source.username = Some("deploy".into());
        source.auth_method = SshAuthPreference::PrivateKey;
        source.key_path = Some(PathBuf::from("/keys/prod"));
        source.color = Some("#E06C75".into());
        source.group = Some("infra".into());
        source.logging = SshLoggingOverride::On;
        source.jump_host = Some(SshSessionId(9));
        source.port_forwards = vec![PortForward::Dynamic {
            bind: oneterm_core::loopback(),
            bind_port: 1080,
        }];
        source.agent_forwarding = true;

        let mut entries = document(vec![(3, source.clone()), (4, session("staging"))], 5).entries;
        assert!(duplicate_in(&mut entries, SshSessionId(3), SshSessionId(5)));

        let ids: Vec<_> = entries.iter().map(|entry| entry.id).collect();
        assert_eq!(
            ids,
            vec![SshSessionId(3), SshSessionId(5), SshSessionId(4)],
            "the copy sits directly after its source, not at the end"
        );
        let copy = entries[1].session.clone();
        assert_eq!(copy.label, "prod (copy)");
        // Field for field, the clone equals the source once the label matches —
        // so a field added to SshSession later cannot be forgotten here.
        assert_eq!(
            SshSession {
                label: source.label.clone(),
                ..copy
            },
            source
        );

        // An id that is gone changes nothing.
        let before = entries.clone();
        assert!(!duplicate_in(
            &mut entries,
            SshSessionId(99),
            SshSessionId(6)
        ));
        assert_eq!(entries, before);
    }

    /// `US-0129`: Move to Group rewrites one session's group; blank ungroups;
    /// an unchanged value and an unknown id are both no-ops.
    #[test]
    fn set_group_moves_exactly_one_session() {
        let mut infra = session("web");
        infra.group = Some("infra".into());
        let mut entries = document(vec![(1, infra), (2, session("solo"))], 3).entries;

        assert!(set_group_in(
            &mut entries,
            SshSessionId(2),
            Some("  infra  ")
        ));
        assert_eq!(entries[1].session.group.as_deref(), Some("infra"));
        assert_eq!(entries[0].session.group.as_deref(), Some("infra"));

        // Same value again: nothing to write.
        assert!(!set_group_in(&mut entries, SshSessionId(2), Some("infra")));

        // "No group": None, and a blank name means the same.
        assert!(set_group_in(&mut entries, SshSessionId(1), None));
        assert_eq!(entries[0].session.group, None);
        assert!(set_group_in(&mut entries, SshSessionId(2), Some("   ")));
        assert_eq!(entries[1].session.group, None);

        assert!(!set_group_in(&mut entries, SshSessionId(99), Some("infra")));
    }

    /// `US-0129` verification F2: the submenu's checked row and the write it
    /// performs go through one rule, so a hand-edited `" infra "` checks the
    /// `infra` row instead of checking nothing at all.
    #[test]
    fn a_padded_stored_group_normalizes_to_the_row_it_should_check() {
        assert_eq!(normalized_group(Some(" infra ")).as_deref(), Some("infra"));
        assert_eq!(normalized_group(Some("infra")).as_deref(), Some("infra"));
        assert_eq!(normalized_group(Some("   ")), None);
        assert_eq!(normalized_group(Some("")), None);
        assert_eq!(normalized_group(None), None);

        // ...and it is the same rule the click writes, so the checked row and
        // the stored value cannot disagree.
        let mut padded = session("web");
        padded.group = Some(" infra ".into());
        let mut entries = document(vec![(1, padded)], 2).entries;
        assert_eq!(
            normalized_group(entries[0].session.group.as_deref()).as_deref(),
            Some("infra"),
            "`infra` is the row the submenu checks, not `No group` and not nothing"
        );
        // Clicking that row is still a write, because the stored text differs
        // from what it means: one click repairs the hand-edited padding.
        assert!(set_group_in(&mut entries, SshSessionId(1), Some("infra")));
        assert_eq!(entries[0].session.group.as_deref(), Some("infra"));
        assert!(!set_group_in(&mut entries, SshSessionId(1), Some("infra")));
    }

    #[test]
    fn newer_schema_versions_are_rejected_not_truncated() {
        let raw = r#"{ "schema_version": 99, "sessions": [] }"#;
        assert!(parse_document(raw).is_err());
    }

    #[test]
    fn back_to_back_saves_keep_only_the_newest_snapshot_on_disk() {
        let directory = temporary_dir("queue");
        let path = directory.join("ssh_session.json");
        let queue = PersistQueue::new();

        // add(): the first mutation starts a worker; remove(): a second mutation
        // scheduled before the worker ran only replaces the pending snapshot.
        assert!(PersistQueue::enqueue(
            &queue,
            document(vec![(1, session("a")), (2, session("b"))], 3)
        ));
        assert!(!PersistQueue::enqueue(
            &queue,
            document(vec![(1, session("a"))], 3)
        ));

        drain_persist_queue(&queue, &path);

        let loaded = SshSessionStore::load_from(&path).unwrap();
        assert_eq!(loaded.document.entries.len(), 1);
        assert_eq!(loaded.document.entries[0].session.label, "a");
        assert_eq!(loaded.document.next_id, 3);
        // Exactly one write happened, so the older snapshot never reached disk.
        assert!(!directory.join("ssh_session.bak").exists());
        // The drained queue starts a new worker for the next mutation.
        assert!(PersistQueue::enqueue(&queue, document(Vec::new(), 3)));
        drain_persist_queue(&queue, &path);
        assert!(
            SshSessionStore::load_from(&path)
                .unwrap()
                .document
                .entries
                .is_empty()
        );
    }
}
