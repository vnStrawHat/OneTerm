//! Layout-level tests for the workspace shell (TEST-20): right-dock mode
//! switching and the load → reset-center → save round trip against an
//! isolated `docks.json`.

use std::collections::HashMap;

use gpui::{Entity, TestAppContext, VisualTestContext, px};
use gpui_component::dock::{DockArea, DockAreaState, DockItem};
use oneterm_actions::RightDockMode;
use oneterm_core::SftpTableState;
use oneterm_state::dock_persistence::{read_dock_document_from, update_dock_document_at};
use oneterm_state::panel_names;

use super::test_panels::{NamedPanel, register_test_panels};
use super::{MAIN_DOCK_VERSION, OneTermWorkspace, layout, persistence};

/// Removes the per-test directory when the test ends — on failure too.
struct TempDirGuard(std::path::PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        // Best effort: a directory that is already gone must not fail the test.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(label: &str) -> TempDirGuard {
    let dir = std::env::temp_dir().join(format!(
        "oneterm-workspace-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    TempDirGuard(dir)
}

/// A dock area in its own window with the test panels registered.
fn dock_area(cx: &mut TestAppContext) -> (Entity<DockArea>, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    cx.update(register_test_panels);
    cx.add_window_view(|window, cx| {
        DockArea::new("layout-test", Some(MAIN_DOCK_VERSION), window, cx)
    })
}

/// `(size, open, panel name)` of the right dock.
fn right_dock(dock_area: &Entity<DockArea>, cx: &mut VisualTestContext) -> (f32, bool, String) {
    dock_area.read_with(cx, |dock_area, cx| {
        let dock = dock_area.right_dock().expect("right dock").read(cx);
        (
            dock.size().as_f32(),
            dock.is_open(),
            dock.panel().view().panel_name(cx).to_string(),
        )
    })
}

/// TEST-20: switching the right-dock mode swaps the panel, keeps the dock
/// width, forces the dock open for SSH Client / Agent, and only hides it for None.
#[gpui::test]
fn switch_right_dock_mode_swaps_panel_and_keeps_width(cx: &mut TestAppContext) {
    let (dock_area, cx) = dock_area(cx);
    dock_area.update_in(cx, |dock_area, window, cx| {
        let panel = DockItem::panel(NamedPanel::view(panel_names::SSH_CLIENT, cx));
        dock_area.set_right_dock(panel, Some(px(333.)), false, window, cx);
    });
    assert_eq!(
        right_dock(&dock_area, cx),
        (333., false, panel_names::SSH_CLIENT.to_string())
    );

    cx.update(|window, cx| {
        OneTermWorkspace::switch_right_dock_mode(&dock_area, RightDockMode::Agent, window, cx)
    });
    assert_eq!(
        right_dock(&dock_area, cx),
        (333., true, panel_names::AGENT.to_string())
    );

    // None hides the dock without rebuilding its content.
    cx.update(|window, cx| {
        OneTermWorkspace::switch_right_dock_mode(&dock_area, RightDockMode::None, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        right_dock(&dock_area, cx),
        (333., false, panel_names::AGENT.to_string())
    );

    cx.update(|window, cx| {
        OneTermWorkspace::switch_right_dock_mode(&dock_area, RightDockMode::SshClient, window, cx)
    });
    assert_eq!(
        right_dock(&dock_area, cx),
        (333., true, panel_names::SSH_CLIENT.to_string())
    );
}

/// TEST-20: a saved layout is loaded, the center reset to one terminal tab
/// while the right dock keeps its width and collapsed state, and the result
/// is written back — with the zoom cleared and the SFTP field preserved.
#[gpui::test]
fn load_reset_center_and_save_round_trip(cx: &mut TestAppContext) {
    let dir = temp_dir("roundtrip");
    let path = dir.0.join("docks.json");

    // Another owner's field must survive the shell's writes.
    update_dock_document_at(&path, |document| {
        document.sftp_table_state = Some(SftpTableState {
            column_widths: HashMap::from([("name".to_string(), 321.0)]),
            column_visibility: HashMap::new(),
        });
        Ok(())
    })
    .unwrap();

    // 1. A previous run: two center tabs, a collapsed 333px right dock, a zoomed panel.
    let (source, cx) = dock_area(cx);
    let saved: DockAreaState = source.update_in(cx, |dock_area, window, cx| {
        let weak = cx.entity().downgrade();
        let center = DockItem::tabs(
            vec![
                NamedPanel::view(panel_names::TERMINAL, cx),
                NamedPanel::view(panel_names::TERMINAL, cx),
            ],
            &weak,
            window,
            cx,
        );
        dock_area.set_center(center, window, cx);
        let right = DockItem::panel(NamedPanel::view(panel_names::SSH_CLIENT, cx));
        dock_area.set_right_dock(right, Some(px(333.)), false, window, cx);
        dock_area.dump(cx)
    });
    persistence::save_state_to(&path, &saved, Some(panel_names::TERMINAL), "test-first-run")
        .unwrap();

    // 2. Next launch: read once, load, reset the center, persist.
    let document = read_dock_document_from(&path).unwrap();
    assert_eq!(
        document.zoomed_panel.as_deref(),
        Some(panel_names::TERMINAL)
    );

    let (target, cx) = {
        let (target, cx) = cx.add_window_view(|window, cx| {
            DockArea::new("layout-test-2", Some(MAIN_DOCK_VERSION), window, cx)
        });
        (target, cx)
    };
    cx.update(|window, cx| persistence::load_layout(&target, &document, window, cx).unwrap());
    // gpui-component's `PanelInfo::Panel` load path wraps the panel in a tab
    // container; the width and collapsed state come back verbatim.
    let (size, open, _) = right_dock(&target, cx);
    assert_eq!((size, open), (333., false));

    let reset = cx
        .update(|window, cx| layout::apply_center_reset(target.downgrade(), window, cx))
        .expect("dock area alive");
    // The center is one terminal tab again; the right dock is untouched.
    let center_terminals = target.read_with(cx, |dock_area, cx| {
        let DockItem::Split { items, .. } = dock_area.center() else {
            panic!("center must be a split");
        };
        items
            .iter()
            .map(|item| match item {
                DockItem::Tabs { items, .. } => items
                    .iter()
                    .filter(|panel| panel.panel_name(cx) == panel_names::TERMINAL)
                    .count(),
                _ => 0,
            })
            .sum::<usize>()
    });
    assert_eq!(center_terminals, 1);
    assert_eq!(
        right_dock(&target, cx),
        (333., false, panel_names::SSH_CLIENT.to_string())
    );

    persistence::save_state_to(&path, &reset, None, "test-reset").unwrap();
    let written = read_dock_document_from(&path).unwrap();
    assert_eq!(written.zoomed_panel, None);
    assert_eq!(written.dock_state::<DockAreaState>().unwrap(), reset);
    assert_eq!(
        written.sftp_table_state.unwrap().column_widths.get("name"),
        Some(&321.0)
    );
}
