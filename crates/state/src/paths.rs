//! Shared persistence paths + field keys for `docks.json`.
//!
//! Used by both the shell (dock-state save/load) and the SFTP feature (which
//! stores its table column state in the same file). Kept in this low crate so
//! neither depends on the other.

/// JSON field storing the SFTP table state (column widths + visibility) in
/// `docks.json`.
pub const SFTP_TABLE_STATE_FIELD: &str = "sftp_table_state";

/// Path to `docks.json` — resolved at runtime via `config_dir().join(...)`:
/// debug → `target/docks.json`, release → `~/.OneTerm/docks.json`.
pub fn state_file() -> std::path::PathBuf {
    oneterm_core::config_dir().join("docks.json")
}
