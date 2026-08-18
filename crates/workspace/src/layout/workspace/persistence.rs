//! Layout persistence — load / save dock state.
//!
//! `docks.json` is owned by `oneterm_state::dock_persistence`: this module
//! reads the typed document and writes the dock fields through the shared
//! update transaction. It never quarantines the file itself — an unreadable
//! document is moved aside by the owner during the next update (see
//! `docs/agents/persistence.md`), so nothing blocks the UI thread at startup.

use std::path::Path;

use anyhow::{Context as _, Result};
use gpui::{App, Edges, Entity, PromptLevel, Window};
use gpui_component::dock::{DockArea, DockAreaState};
use oneterm_state::dock_persistence::{DockDocument, DockUpdateOutcome, update_dock_document_at};

use super::{MAIN_DOCK_VERSION, state_file};

/// Read and parse the persisted dock document (blocking; startup only).
pub(crate) fn read_dock_document() -> std::io::Result<DockDocument> {
    oneterm_state::dock_persistence::read_dock_document()
}

/// Load the layout from an already-read document into `dock_area` — used to
/// keep right dock + settings. Fails when the document holds no valid dock
/// layout. A layout saved by an older `MAIN_DOCK_VERSION` is still loaded, and
/// the user is offered a reset to the default layout.
pub(crate) fn load_layout(
    dock_area: &Entity<DockArea>,
    document: &DockDocument,
    window: &mut Window,
    cx: &mut App,
) -> Result<()> {
    let state = document
        .dock_state::<DockAreaState>()
        .map_err(|error| anyhow::anyhow!("parse dock layout: {error}"))?;

    if state.version != Some(MAIN_DOCK_VERSION) {
        let answer = window.prompt(
            PromptLevel::Info,
            "The default main layout has been updated.\n\
            Do you want to reset the layout to default?",
            None,
            &["Yes", "No"],
            cx,
        );

        let weak_dock_area = dock_area.downgrade();
        window
            .spawn(cx, async move |cx| {
                if answer.await == Ok(0) {
                    // The window may have closed while the prompt was open.
                    _ = cx.update(|window, cx| {
                        super::layout::reset_default_layout(weak_dock_area, window, cx);
                    });
                }
            })
            .detach();
    }

    dock_area.update(cx, |dock_area, cx| {
        dock_area.load(state, window, cx).context("load layout")?;
        dock_area.set_dock_collapsible(
            Edges {
                right: true,
                ..Default::default()
            },
            window,
            cx,
        );
        Ok::<(), anyhow::Error>(())
    })
}

/// Save the dock state to a file.
///
/// `zoomed_panel`: name of the panel currently zoomed (fullscreen) — injected into the
/// JSON value of `docks.json` (field `zoomed_panel`). `None` → removes the field (no panel zoomed).
/// Does not modify the `DockAreaState` struct.
/// `trigger`: a string describing what triggered the write (e.g. "debounce", "on_app_quit",
/// "on_close", "zoom_in", "zoom_out", "reset_center_only", "reset_default_layout").
pub(crate) fn save_state(
    state: &DockAreaState,
    zoomed_panel: Option<&str>,
    trigger: &str,
) -> Result<()> {
    save_state_to(&state_file(), state, zoomed_panel, trigger)
}

/// [`save_state`] against an explicit document path (tests use an isolated file).
pub(crate) fn save_state_to(
    path: &Path,
    state: &DockAreaState,
    zoomed_panel: Option<&str>,
    trigger: &str,
) -> Result<()> {
    let state_value = serde_json::to_value(state)?;
    let right_dock_open = state_value
        .get("right_dock")
        .and_then(|dock| dock.get("open"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    // Debounced saves fire on every layout change; keep them out of the info log (ERR-11).
    log::debug!(
        "Save layout [trigger={trigger}] → zoomed_panel={zoomed_panel:?}, right_dock_open={right_dock_open}",
    );
    let mut next_document = DockDocument::from_dock_state(state)?;
    next_document.zoomed_panel = zoomed_panel.map(str::to_owned);
    let outcome = update_dock_document_at(path, move |current| {
        next_document.sftp_table_state = current.sftp_table_state.take();
        *current = next_document;
        Ok(())
    })?;
    if let DockUpdateOutcome::RecoveredFromInvalidData { quarantined } = outcome {
        log::warn!(
            "docks.json was invalid and has been reset while saving the layout [trigger={trigger}] (quarantined copy: {quarantined:?})"
        );
    }
    Ok(())
}

/// Persist a background snapshot and retain an actionable diagnostic on failure.
pub(crate) fn save_state_logged(state: &DockAreaState, zoomed_panel: Option<&str>, trigger: &str) {
    if let Err(error) = save_state(state, zoomed_panel, trigger) {
        log::error!("failed to persist dock state [trigger={trigger}]: {error:#}");
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use gpui_component::dock::DockAreaState;
    use oneterm_core::SftpTableState;
    use oneterm_state::dock_persistence::DockDocument;

    #[test]
    fn one_term_fields_roundtrip_with_dock_state() {
        let state = DockAreaState::default();
        let mut document = DockDocument::from_dock_state(&state).unwrap();
        document.zoomed_panel = Some("session".into());
        document.sftp_table_state = Some(SftpTableState {
            column_widths: HashMap::from([("name".into(), 320.0)]),
            column_visibility: HashMap::from([("permissions".into(), false)]),
        });

        let json = serde_json::to_string_pretty(&document).unwrap();
        let restored: DockDocument = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.dock_state::<DockAreaState>().unwrap(), state);
        assert_eq!(restored.zoomed_panel.as_deref(), Some("session"));
        assert_eq!(
            restored.sftp_table_state.unwrap().column_widths.get("name"),
            Some(&320.0)
        );
    }
}
