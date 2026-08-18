//! Persistence for the SFTP table state — save/read column state (width, visibility)
//! to `docks.json` (the `sftp_table_state` field).
//!
//! Both the shell and this feature use `oneterm_state::dock_persistence`, which
//! owns the complete typed document and serializes updates under one file lock.
//! `docks.json` is a shell/state-owned document: this crate only reads and
//! writes its own field and never quarantines or replaces the file itself —
//! recovery of an invalid document belongs to `dock_persistence`.
//!
//! Both functions block on the filesystem and must run on the background
//! executor, never on the UI thread.

use anyhow::{Context as _, Result};

use oneterm_core::SftpTableState;
use oneterm_state::dock_persistence::{
    DockUpdateOutcome, read_dock_document, update_dock_document,
};

/// Read `sftp_table_state` from `docks.json`. `None` if the file/field does not exist
/// or fails to parse.
pub(crate) fn read_sftp_table_state() -> Option<SftpTableState> {
    match read_dock_document() {
        Ok(document) => document.and_then(|document| document.sftp_table_state),
        Err(error) => {
            log::warn!("failed to read SFTP table state: {error}");
            None
        }
    }
}

/// Write the typed SFTP table state while preserving the dock document's
/// layout and shell-owned fields.
pub(crate) fn write_sftp_table_state(state: &SftpTableState) -> Result<()> {
    let state = state.clone();
    let outcome = update_dock_document(move |document| {
        document.sftp_table_state = Some(state);
        Ok(())
    })
    .context("write docks.json")?;
    if let DockUpdateOutcome::RecoveredFromInvalidData { quarantined } = outcome {
        log::warn!(
            "docks.json was invalid and has been reset while saving the SFTP table state (quarantined copy: {quarantined:?})"
        );
    }
    Ok(())
}
