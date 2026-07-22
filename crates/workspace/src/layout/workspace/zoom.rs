//! Persist the Zoom (fullscreen) state of panels in the Dock.
//!
//! `gpui-component` does not serialize `TabPanel.zoomed` (a private field) into
//! `DockAreaState`, so the zoom is lost on restart. This module compensates by:
//!
//! 1. Subscribing to `PanelEvent::ZoomIn`/`ZoomOut` on each `TabPanel` to track
//!    which panel is zoomed (a mirror state — since `zoomed` is not readable from outside).
//! 2. On save (`save_layout` / `on_app_quit`), writing the `panel_name` of the zoomed
//!    panel into `docks.json` (field `zoomed_panel`, injected into the JSON value — without
//!    touching the `DockAreaState` struct).
//! 3. On load, reading `zoomed_panel` → finding the TabPanel whose active panel matches the
//!    name → focus + dispatch `ToggleZoom` to zoom it again (via the proper code path, with
//!    consistent toolbar state).

// Dock traversal helpers now live in the low `oneterm-state` crate so both the
// shell and feature crates can share them without a shell <-> feature edge.
pub(crate) use oneterm_state::dock_util::{collect_tab_panels, find_tab_by_panel_name};
