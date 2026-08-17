use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use oneterm_core::{
    atomic_write, config_dir, migrate_json_value, quarantine_file, set_schema_version,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Release channel used by the updater.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Preview,
}

/// Cached update candidate restored when GitHub returns `304 Not Modified`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachedUpdateCandidate {
    pub version: String,
    pub tag_name: String,
    pub release_name: Option<String>,
    pub release_notes_url: String,
    pub body: Option<String>,
    pub asset_name: String,
    pub asset_url: String,
    pub asset_digest: String,
    pub asset_size: Option<u64>,
    pub target_triple: String,
}

/// HTTP cache metadata recorded by the release checker.
///
/// These fields share `update_config.json` with the user preferences but are
/// written only through field-level merges, so a check that finishes after
/// the user edited a preference can never overwrite that edit.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateCheckCache {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_checked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_checked_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cached_candidate: Option<CachedUpdateCandidate>,
}

/// Document keys owned by the preference writer (settings UI).
const PREFERENCE_FIELDS: &[&str] = &[
    "auto_check",
    "channel",
    "check_interval_hours",
    "proxy_url",
    "verify_certificates",
    "skipped_version",
];

/// Document keys owned by the release checker.
const CACHE_FIELDS: &[&str] = &[
    "last_checked_at",
    "last_etag",
    "last_checked_version",
    "cached_candidate",
];

impl UpdateCheckCache {
    /// Merge the cache fields into the default update config document.
    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&config_dir().join("update_config.json"))
    }

    /// Merge the cache fields into the document at `path`, leaving every
    /// preference field untouched.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        let values = serde_json::to_value(self).map_err(std::io::Error::other)?;
        merge_owned_fields(path, CACHE_FIELDS, &values)
    }
}

/// Persisted auto-update preferences and HTTP cache metadata.
///
/// The settings UI owns the preference fields; [`UpdateCheckCache`] fields are
/// owned by the release checker. Each owner persists only its own fields.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateConfig {
    #[serde(default = "default_auto_check")]
    pub auto_check: bool,
    #[serde(default)]
    pub channel: UpdateChannel,
    #[serde(default = "default_interval_hours")]
    pub check_interval_hours: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub proxy_url: Option<String>,
    #[serde(default = "default_verify_certificates")]
    pub verify_certificates: bool,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_checked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_checked_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub skipped_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cached_candidate: Option<CachedUpdateCandidate>,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            auto_check: default_auto_check(),
            channel: UpdateChannel::Stable,
            check_interval_hours: default_interval_hours(),
            proxy_url: None,
            verify_certificates: default_verify_certificates(),
            last_checked_at: None,
            last_etag: None,
            last_checked_version: None,
            skipped_version: None,
            cached_candidate: None,
        }
    }
}

impl UpdateConfig {
    /// Load persisted update preferences.
    pub fn load() -> Self {
        Self::load_from(&config_dir().join("update_config.json"))
    }

    /// Load persisted update preferences from an explicit path.
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(raw) => Self::parse_document(&raw).unwrap_or_else(|error| {
                log::error!("update_config.json parse or migration error: {error}; using defaults");
                if let Err(quarantine_error) = quarantine_file(path) {
                    log::warn!("failed to quarantine update_config.json: {quarantine_error}");
                }
                Self::default()
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let config = Self::default();
                if let Err(write_error) = config.write_default_document(path) {
                    log::warn!(
                        "failed to create default update_config.json at {path:?}: {write_error}"
                    );
                }
                config
            }
            Err(error) => {
                log::error!("failed to read update_config.json: {error}; using defaults");
                Self::default()
            }
        }
    }

    /// Write the complete default document for a config file that does not
    /// exist yet. Runtime writers must use the field-level merges instead.
    fn write_default_document(&self, path: &Path) -> std::io::Result<()> {
        let mut value = serde_json::to_value(self).map_err(std::io::Error::other)?;
        set_schema_version(&mut value, CURRENT_SCHEMA_VERSION)?;
        let json = serde_json::to_string_pretty(&value).map_err(std::io::Error::other)?;
        atomic_write(path, json.as_bytes())
    }

    /// Merge the preference fields into the default update config document.
    pub fn save_preferences(&self) -> std::io::Result<()> {
        self.save_preferences_to(&config_dir().join("update_config.json"))
    }

    /// Merge the preference fields into the document at `path`, leaving the
    /// checker-owned cache fields untouched.
    pub fn save_preferences_to(&self, path: &Path) -> std::io::Result<()> {
        let values = serde_json::to_value(self).map_err(std::io::Error::other)?;
        merge_owned_fields(path, PREFERENCE_FIELDS, &values)
    }

    /// Snapshot the checker-owned cache fields.
    pub fn check_cache(&self) -> UpdateCheckCache {
        UpdateCheckCache {
            last_checked_at: self.last_checked_at.clone(),
            last_etag: self.last_etag.clone(),
            last_checked_version: self.last_checked_version.clone(),
            cached_candidate: self.cached_candidate.clone(),
        }
    }

    /// Replace only the checker-owned cache fields, keeping every preference.
    pub fn apply_check_cache(&mut self, cache: UpdateCheckCache) {
        self.last_checked_at = cache.last_checked_at;
        self.last_etag = cache.last_etag;
        self.last_checked_version = cache.last_checked_version;
        self.cached_candidate = cache.cached_candidate;
    }

    /// Return whether a startup auto-check should run now.
    pub fn should_auto_check(&self) -> bool {
        if !self.auto_check {
            return false;
        }
        let Some(last_checked_at) = &self.last_checked_at else {
            return true;
        };
        let Ok(last_checked_at) = DateTime::parse_from_rfc3339(last_checked_at) else {
            return true;
        };
        let interval = Duration::hours(self.check_interval_hours.max(1) as i64);
        Utc::now().signed_duration_since(last_checked_at.with_timezone(&Utc)) >= interval
    }

    /// Store cache metadata after a successful GitHub response.
    pub fn record_success(&mut self, etag: Option<String>, current_version: &str) {
        self.last_checked_at = Some(Utc::now().to_rfc3339());
        self.last_checked_version = Some(current_version.to_owned());
        if etag.is_some() {
            self.last_etag = etag;
        }
    }

    /// Return whether the cached ETag still matches the current app version.
    pub fn should_reuse_cached_etag(&self, current_version: &str) -> bool {
        self.last_etag.is_some() && self.last_checked_version.as_deref() == Some(current_version)
    }

    fn parse_document(raw: &str) -> std::io::Result<Self> {
        let value: Value = serde_json::from_str(raw).map_err(std::io::Error::other)?;
        let value = migrate_json_value(
            value,
            CURRENT_SCHEMA_VERSION,
            "update_config.json",
            |_, value| {
                if !value.is_object() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "update_config.json schema must be an object",
                    ));
                }
                Ok(value)
            },
        )?;
        serde_json::from_value(value).map_err(std::io::Error::other)
    }
}

fn default_auto_check() -> bool {
    true
}

fn default_interval_hours() -> u64 {
    24
}

fn default_verify_certificates() -> bool {
    true
}

/// Read-modify-write only the `keys` owned by one writer.
///
/// Keys absent from `values` (serialized `None`) are removed from the document
/// so a cleared field does not survive on disk. Fields owned by the other
/// writer are copied through unchanged. A document that no longer parses is
/// quarantined once and the merge restarts from an empty document, so a
/// corrupt file cannot block preference or cache persistence until restart.
fn merge_owned_fields(path: &Path, keys: &[&str], values: &Value) -> std::io::Result<()> {
    let merge = |document: &mut Value| -> std::io::Result<()> {
        if !document.is_object() {
            *document = Value::Object(serde_json::Map::new());
        }
        if let Value::Object(fields) = document {
            for key in keys {
                match values.get(key) {
                    Some(value) => {
                        fields.insert((*key).to_owned(), value.clone());
                    }
                    None => {
                        fields.remove(*key);
                    }
                }
            }
        }
        set_schema_version(document, CURRENT_SCHEMA_VERSION)
    };
    match oneterm_core::update_json_file(path, &merge) {
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
            log::error!("update_config.json is unreadable: {error}; quarantining before rewrite");
            quarantine_file(path)?;
            oneterm_core::update_json_file(path, &merge)
        }
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_uses_defaults_and_creates_document() {
        let dir = test_dir("missing");
        let path = dir.join("update_config.json");
        let config = UpdateConfig::load_from(&path);
        assert_eq!(config.check_interval_hours, 24);
        assert!(config.auto_check);
        assert_eq!(config.proxy_url, None);
        assert!(config.verify_certificates);
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn invalid_document_is_quarantined() {
        let dir = test_dir("invalid");
        let path = dir.join("update_config.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, b"not-json").unwrap();
        let config = UpdateConfig::load_from(&path);
        assert_eq!(config.channel, UpdateChannel::Stable);
        assert_eq!(config.proxy_url, None);
        assert!(config.verify_certificates);
        assert!(!path.exists());
        assert!(std::fs::read_dir(&dir).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".invalid-")
        }));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn preference_edit_during_check_survives_check_completion() {
        let dir = test_dir("merge-check");
        let path = dir.join("update_config.json");
        // The UI entity is the in-memory truth; the checker works on a copy.
        let mut entity = UpdateConfig::load_from(&path);
        let mut checker_copy = entity.clone();

        // User edits a preference while the check is still running.
        entity.proxy_url = Some("https://proxy.example".to_owned());
        entity.auto_check = false;
        entity.save_preferences_to(&path).unwrap();

        // The check completes on the background thread with the stale copy.
        checker_copy.record_success(Some("etag-1".to_owned()), "0.3.0");
        checker_copy.check_cache().save_to(&path).unwrap();
        entity.apply_check_cache(checker_copy.check_cache());

        assert_eq!(entity.proxy_url.as_deref(), Some("https://proxy.example"));
        assert!(!entity.auto_check);
        assert_eq!(entity.last_etag.as_deref(), Some("etag-1"));

        let on_disk = UpdateConfig::load_from(&path);
        assert_eq!(on_disk, entity);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn preference_and_cache_writers_do_not_lose_each_others_fields() {
        let dir = test_dir("merge-concurrent");
        let path = dir.join("update_config.json");
        std::fs::create_dir_all(&dir).unwrap();

        let mut writers = Vec::new();
        for index in 0..4 {
            let preference_path = path.clone();
            writers.push(std::thread::spawn(move || {
                let path = preference_path;
                let config = UpdateConfig {
                    proxy_url: Some(format!("https://proxy-{index}.example")),
                    verify_certificates: false,
                    ..Default::default()
                };
                config.save_preferences_to(&path).unwrap();
            }));
            let path = path.clone();
            writers.push(std::thread::spawn(move || {
                let cache = UpdateCheckCache {
                    last_etag: Some(format!("etag-{index}")),
                    last_checked_version: Some("0.3.0".to_owned()),
                    ..Default::default()
                };
                cache.save_to(&path).unwrap();
            }));
        }
        for writer in writers {
            writer.join().unwrap();
        }

        let on_disk = UpdateConfig::load_from(&path);
        assert!(
            on_disk
                .proxy_url
                .as_deref()
                .unwrap()
                .starts_with("https://proxy-")
        );
        assert!(!on_disk.verify_certificates);
        assert!(on_disk.last_etag.as_deref().unwrap().starts_with("etag-"));
        assert_eq!(on_disk.last_checked_version.as_deref(), Some("0.3.0"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cleared_fields_are_removed_and_corrupt_documents_are_quarantined() {
        let dir = test_dir("merge-clear");
        let path = dir.join("update_config.json");
        let mut config = UpdateConfig {
            proxy_url: Some("https://proxy.example".to_owned()),
            ..Default::default()
        };
        config.save_preferences_to(&path).unwrap();
        config.proxy_url = None;
        config.save_preferences_to(&path).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("proxy_url"));

        std::fs::write(&path, b"not-json").unwrap();
        UpdateCheckCache {
            last_etag: Some("etag".to_owned()),
            ..Default::default()
        }
        .save_to(&path)
        .unwrap();
        let on_disk = UpdateConfig::load_from(&path);
        assert_eq!(on_disk.last_etag.as_deref(), Some("etag"));
        assert!(std::fs::read_dir(&dir).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".invalid-")
        }));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cached_etag_is_only_reused_for_same_version() {
        let mut config = UpdateConfig::default();
        config.record_success(Some("etag-123".to_owned()), "0.3.0");
        assert!(config.should_reuse_cached_etag("0.3.0"));
        assert!(!config.should_reuse_cached_etag("0.2.1"));
    }

    fn test_dir(name: &str) -> std::path::PathBuf {
        // A process-wide sequence keeps directories distinct even when parallel
        // tests read the same coarse timestamp (as on macOS).
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);

        let nonce = Utc::now().timestamp_nanos_opt().unwrap_or_default();
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "oneterm-update-config-{name}-{}-{nonce}-{sequence}",
            std::process::id()
        ))
    }
}
