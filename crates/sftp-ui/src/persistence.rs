//! Persistence for the SFTP table state — save/read column state (width, visibility)
//! to `docks.json` (the `sftp_table_state` field).
//!
//! Both the shell and this feature use `oneterm_state::dock_persistence`, which
//! owns the complete typed document and serializes updates under one file lock.

use anyhow::{Context as _, Result};

use oneterm_core::{SftpTableState, quarantine_file};
use oneterm_state::dock_persistence::{read_dock_document, update_dock_document};
use oneterm_state::paths::state_file;

/// Read `sftp_table_state` from `docks.json`. `None` if the file/field does not exist
/// or fails to parse.
pub(crate) fn read_sftp_table_state() -> Option<SftpTableState> {
    match read_dock_document() {
        Ok(document) => document.sftp_table_state,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::InvalidData {
                if let Err(quarantine_error) = quarantine_file(&state_file()) {
                    log::warn!("failed to quarantine docks.json: {quarantine_error}");
                }
            }
            log::warn!("failed to read SFTP table state: {error}");
            None
        }
    }
}

/// Write the typed SFTP table state while preserving the dock document's
/// layout and shell-owned fields.
pub(crate) fn write_sftp_table_state(state: &SftpTableState) -> Result<()> {
    let state = state.clone();
    update_dock_document(move |document| {
        document.sftp_table_state = Some(state);
        Ok(())
    })
    .context("write docks.json")
}
