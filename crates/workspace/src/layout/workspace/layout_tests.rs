//! Layout-level tests for the workspace shell (TEST-20): right-dock mode
//! switching and the load → reset-center → save round trip against an
//! isolated `docks.json`.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext, px};
use gpui_component::dock::{
    DockArea, DockAreaState, DockLayout, DockPlacement, PanelHandle, panel_handle, register_panel,
};
use oneterm_actions::RightDockMode;
use oneterm_core::SftpTableState;
use oneterm_state::dock_persistence::{read_dock_document_from, update_dock_document_at};
use oneterm_state::panel_names;

use super::actions::{DockModeAction, reopen_or_rebuild};
use super::test_panels::{NamedPanel, register_test_panels};
use super::{
    DEFAULT_RIGHT_DOCK_WIDTH, MAIN_DOCK_VERSION, OneTermWorkspace, clamp_right_dock_width, layout,
    persistence, restore_zoom_in_dock, right_dock_mode_for,
};

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

fn set_right_dock(
    dock_area: &Entity<DockArea>,
    panel_name: &'static str,
    size: gpui::Pixels,
    open: bool,
    cx: &mut VisualTestContext,
) {
    dock_area.update_in(cx, |dock_area, window, cx| {
        let layout = DockLayout::tabs().panel_view(NamedPanel::view(panel_name, cx), cx);
        dock_area.set_dock(DockPlacement::Right, layout, window, cx);
        dock_area.set_dock_size(DockPlacement::Right, size, window, cx);
        if dock_area.is_dock_open(DockPlacement::Right) != open {
            dock_area.toggle_dock(DockPlacement::Right, window, cx);
        }
    });
}

/// Re-registers [`panel_names::TERMINAL`] with a builder that counts its builds.
///
/// Building the real terminal panel is what spawns a local shell, so this count
/// is the number of shells the code under test would have started (`BUG-0054`).
fn count_terminal_builds(cx: &mut VisualTestContext) -> Rc<Cell<usize>> {
    let builds = Rc::new(Cell::new(0));
    let counter = Rc::clone(&builds);
    cx.update(|_, cx| {
        register_panel(cx, panel_names::TERMINAL, move |_, _, cx| {
            counter.set(counter.get() + 1);
            panel_handle(cx.new(|cx| NamedPanel::new(panel_names::TERMINAL, cx)))
        });
    });
    builds
}

/// The `PanelId` of the right dock's panel — a new id means it was rebuilt.
fn right_dock_panel_id(
    dock_area: &Entity<DockArea>,
    cx: &mut VisualTestContext,
) -> gpui_component::dock::PanelId {
    dock_area.read_with(cx, |dock_area, _| {
        dock_area
            .layout(DockPlacement::Right)
            .expect("right dock")
            .panels()
            .next()
            .expect("right dock panel")
    })
}

/// `(size, open, panel name)` of the right dock.
fn right_dock(dock_area: &Entity<DockArea>, cx: &mut VisualTestContext) -> (f32, bool, String) {
    dock_area.read_with(cx, |dock_area, cx| {
        let tree = dock_area.layout(DockPlacement::Right).expect("right dock");
        let panel = tree
            .panels()
            .next()
            .and_then(|id| dock_area.panel(id))
            .expect("right dock panel");
        (
            dock_area
                .dock_size(DockPlacement::Right)
                .expect("right dock size")
                .as_f32(),
            dock_area.is_dock_open(DockPlacement::Right),
            panel.panel_name(cx).to_string(),
        )
    })
}

/// TEST-20: switching the right-dock mode swaps the panel, keeps the dock
/// width, forces the dock open for SSH Client / Agent, and only hides it for None.
#[gpui::test]
fn every_persisted_panel_name_resolves_with_its_presentation_handle(cx: &mut TestAppContext) {
    let (dock_area, cx) = dock_area(cx);
    for name in [
        panel_names::TERMINAL,
        panel_names::SFTP,
        panel_names::SESSION,
        panel_names::SSH_CLIENT,
        panel_names::AGENT,
    ] {
        let panel = cx.update(|window, cx| {
            super::build_named_panel(name, &dock_area.downgrade(), window, cx)
                .expect("test panel is registered")
        });
        let title = cx.update(|_, cx| {
            PanelHandle::of(&panel)
                .expect("registered panel must stay wrapped")
                .tab_name(cx)
        });
        assert_eq!(title.as_deref(), Some(format!("{name} title").as_str()));
    }
}

#[gpui::test]
fn switch_right_dock_mode_swaps_panel_and_keeps_width(cx: &mut TestAppContext) {
    let (dock_area, cx) = dock_area(cx);
    set_right_dock(&dock_area, panel_names::SSH_CLIENT, px(333.), false, cx);
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

/// US-0113: the dock's width is bounded by a share of the window, so the
/// terminal keeps the majority at laptop widths.
#[test]
fn the_right_dock_width_is_clamped_to_a_share_of_the_window() {
    // The ~900 px window of `research/before/50-narrow-900.png`: the saved
    // ~490 px dock comes back as 35% of the window, leaving the terminal 585.
    assert_eq!(clamp_right_dock_width(px(490.), px(900.)), px(315.));
    assert_eq!(
        clamp_right_dock_width(DEFAULT_RIGHT_DOCK_WIDTH, px(900.)),
        px(315.)
    );
    // Wide window: the user's dragged width is under the ceiling and survives.
    assert_eq!(clamp_right_dock_width(px(490.), px(1900.)), px(490.));
    // A width at the ceiling is not moved.
    assert_eq!(clamp_right_dock_width(px(315.), px(900.)), px(315.));
    // A narrow dock is never widened — the clamp only caps.
    assert_eq!(clamp_right_dock_width(px(120.), px(1900.)), px(120.));
    // Too narrow for a third of the window to be a usable panel: the floor
    // wins over the share, and the dock stays on screen.
    assert_eq!(clamp_right_dock_width(px(490.), px(500.)), px(240.));
    // Before the first layout pass the window has no size and constrains nothing.
    assert_eq!(clamp_right_dock_width(px(490.), px(0.)), px(490.));
}

/// BUG-0067: the persisted mode follows the dock's open state, so the title
/// bar's segmented control can never claim a mode over a collapsed dock.
#[test]
fn right_dock_mode_follows_the_dock_state() {
    // Collapsed by a dock button: whatever was persisted, the truth is None.
    assert_eq!(
        right_dock_mode_for(false, RightDockMode::SshClient, false),
        RightDockMode::None
    );
    assert_eq!(
        right_dock_mode_for(false, RightDockMode::Agent, true),
        RightDockMode::None
    );
    // Reopened by the same button: the mode names the panel that came back.
    assert_eq!(
        right_dock_mode_for(true, RightDockMode::None, false),
        RightDockMode::SshClient
    );
    assert_eq!(
        right_dock_mode_for(true, RightDockMode::None, true),
        RightDockMode::Agent
    );
    // An open dock showing the mode it says it shows is left alone.
    assert_eq!(
        right_dock_mode_for(true, RightDockMode::Agent, true),
        RightDockMode::Agent
    );
}

/// BUG-0067: an explicit mode click decides against what the dock *contains*,
/// not against the persisted mode, so reopening never rebuilds.
#[test]
fn a_mode_click_only_rebuilds_a_panel_the_dock_does_not_have() {
    // Collapsed by a dock button: the dock still holds the SSH Client panel, so
    // clicking SSH Client shows it — the persisted mode is None by then.
    assert_eq!(
        reopen_or_rebuild(Some(RightDockMode::SshClient), RightDockMode::SshClient),
        DockModeAction::Show
    );
    assert_eq!(
        reopen_or_rebuild(Some(RightDockMode::Agent), RightDockMode::Agent),
        DockModeAction::Show
    );
    // A different mode is the only thing that builds a panel.
    assert_eq!(
        reopen_or_rebuild(Some(RightDockMode::SshClient), RightDockMode::Agent),
        DockModeAction::Rebuild
    );
    assert_eq!(
        reopen_or_rebuild(None, RightDockMode::SshClient),
        DockModeAction::Rebuild
    );
    // None always hides, whatever is in there — an explicit click never toggles.
    assert_eq!(
        reopen_or_rebuild(Some(RightDockMode::SshClient), RightDockMode::None),
        DockModeAction::Hide
    );
    assert_eq!(
        reopen_or_rebuild(None, RightDockMode::None),
        DockModeAction::Hide
    );
}

/// BUG-0067: a dock collapsed by the tab bar's dock button reopens with the
/// *same panel instance* and the same width — a rebuild would drop the SFTP
/// connection and the session list's state.
#[gpui::test]
fn reopening_a_collapsed_right_dock_keeps_its_panel_instance(cx: &mut TestAppContext) {
    let (dock_area, cx) = dock_area(cx);
    set_right_dock(&dock_area, panel_names::SSH_CLIENT, px(333.), true, cx);
    let panel_before = right_dock_panel_id(&dock_area, cx);

    // The tab bar's dock button toggles the dock without touching the mode.
    dock_area.update_in(cx, |dock_area, window, cx| {
        dock_area.toggle_dock(DockPlacement::Right, window, cx)
    });
    assert_eq!(
        right_dock(&dock_area, cx),
        (333., false, panel_names::SSH_CLIENT.to_string())
    );

    let action = cx.update(|window, cx| {
        OneTermWorkspace::apply_right_dock_mode(&dock_area, RightDockMode::SshClient, window, cx)
    });
    assert_eq!(action, DockModeAction::Show);
    assert_eq!(
        right_dock(&dock_area, cx),
        (333., true, panel_names::SSH_CLIENT.to_string())
    );
    assert_eq!(
        right_dock_panel_id(&dock_area, cx),
        panel_before,
        "reopening must not rebuild the panel"
    );

    // Switching to another mode is what builds a new panel.
    let action = cx.update(|window, cx| {
        OneTermWorkspace::apply_right_dock_mode(&dock_area, RightDockMode::Agent, window, cx)
    });
    assert_eq!(action, DockModeAction::Rebuild);
    assert_ne!(right_dock_panel_id(&dock_area, cx), panel_before);
}

/// US-0113: a drag inside the allowed range is left alone and remembered; one
/// past the ceiling is remembered *and* capped on the spot, not left standing
/// until the next window resize.
#[gpui::test]
fn a_drag_past_the_ceiling_is_capped_where_it_happens(cx: &mut TestAppContext) {
    let (dock_area, cx) = dock_area(cx);
    let window_width = cx.update(|window, _| window.viewport_size().width);
    let ceiling = clamp_right_dock_width(px(f32::MAX), window_width);
    let inside = ceiling - px(40.);

    // A drag inside the range: the dock keeps exactly what was dragged.
    set_right_dock(&dock_area, panel_names::SSH_CLIENT, inside, true, cx);
    let preferred = cx.update(|window, cx| {
        OneTermWorkspace::absorb_dragged_right_dock_width(
            &dock_area,
            DEFAULT_RIGHT_DOCK_WIDTH,
            window,
            cx,
        )
    });
    assert_eq!(preferred, inside, "the dragged width is the preference");
    assert_eq!(right_dock(&dock_area, cx).0, inside.as_f32());

    // A drag past the ceiling: remembered whole, but the dock is capped now.
    let past = ceiling + px(100.);
    dock_area.update_in(cx, |dock_area, window, cx| {
        dock_area.set_dock_size(DockPlacement::Right, past, window, cx)
    });
    let preferred = cx.update(|window, cx| {
        OneTermWorkspace::absorb_dragged_right_dock_width(&dock_area, preferred, window, cx)
    });
    assert_eq!(
        preferred, past,
        "the width the user asked for is remembered"
    );
    assert_eq!(
        right_dock(&dock_area, cx).0,
        ceiling.as_f32(),
        "the dock is held to the ceiling without waiting for a resize"
    );

    // Nothing more to do once the dock already holds the applied width.
    let settled = cx.update(|window, cx| {
        OneTermWorkspace::absorb_dragged_right_dock_width(&dock_area, preferred, window, cx)
    });
    assert_eq!(settled, preferred);
    assert_eq!(right_dock(&dock_area, cx).0, ceiling.as_f32());
}

/// US-0113: what reaches `docks.json` is the user's preferred width, not the
/// width the clamp applied to this window.
#[gpui::test]
fn the_saved_layout_carries_the_preferred_width_not_the_clamped_one(cx: &mut TestAppContext) {
    let (dock_area, cx) = dock_area(cx);
    // What a 700 px window would have applied to a 480 px preference.
    set_right_dock(&dock_area, panel_names::SSH_CLIENT, px(245.), true, cx);

    let state = dock_area.read_with(cx, |dock_area, cx| dock_area.dump(cx));
    assert_eq!(
        state.right_dock.as_ref().map(|dock| dock.size()),
        Some(px(245.))
    );

    let rewritten = super::state_with_preferred_width(state.clone(), px(480.));
    assert_eq!(
        rewritten.right_dock.as_ref().map(|dock| dock.size()),
        Some(px(480.)),
        "the preference is what survives the session"
    );
    // Nothing else about the layout is rewritten.
    let (before, after) = (
        state.right_dock.expect("right dock"),
        rewritten.right_dock.expect("right dock"),
    );
    assert_eq!(after.open(), before.open());
    assert_eq!(after.placement(), before.placement());
    assert_eq!(after.panel(), before.panel());
    assert_eq!(rewritten.center, state.center);
}

/// A pre-migration layout keeps its panel tree, active tabs, dock size/open
/// state, and legacy single-panel right-dock encoding across load and save.
#[gpui::test]
fn pre_migration_fixture_loads_and_saves_without_semantic_drift(cx: &mut TestAppContext) {
    let document: oneterm_state::dock_persistence::DockDocument = serde_json::from_str(
        include_str!("../../../../state/src/fixtures/docks-0.5.2.json"),
    )
    .unwrap();
    let expected: DockAreaState = document.dock_state().unwrap();
    let expected_zoom = document.zoomed_panel.clone().expect("fixture zoom");
    let expected_sftp = document
        .sftp_table_state
        .clone()
        .expect("fixture SFTP table state");
    let (dock_area, cx) = dock_area(cx);

    cx.update(|window, cx| {
        dock_area.update(cx, |dock_area, cx| {
            dock_area.load(expected.clone(), window, cx).unwrap();
        });
        assert!(restore_zoom_in_dock(&dock_area, &expected_zoom, window, cx));
    });
    assert!(dock_area.read_with(cx, |dock_area, _| dock_area.is_zoomed()));

    let dumped = dock_area.read_with(cx, |dock_area, cx| dock_area.dump(cx));
    let mut expected_center = serde_json::to_value(&expected.center).unwrap();
    let mut dumped_center = serde_json::to_value(&dumped.center).unwrap();
    expected_center["info"]["stack"]["sizes"] = serde_json::Value::Null;
    dumped_center["info"]["stack"]["sizes"] = serde_json::Value::Null;
    assert_eq!(dumped_center, expected_center);
    assert_eq!(
        dumped
            .right_dock
            .as_ref()
            .map(|dock| (dock.size(), dock.open())),
        expected
            .right_dock
            .as_ref()
            .map(|dock| (dock.size(), dock.open())),
    );

    let dir = temp_dir("pre-migration-fixture");
    let path = dir.0.join("docks.json");
    std::fs::write(
        &path,
        include_bytes!("../../../../state/src/fixtures/docks-0.5.2.json"),
    )
    .unwrap();
    persistence::save_state_to(&path, &dumped, Some(&expected_zoom), "fixture-roundtrip").unwrap();
    let stored_document = read_dock_document_from(&path)
        .unwrap()
        .expect("saved document");
    assert_eq!(
        stored_document.zoomed_panel.as_deref(),
        Some(expected_zoom.as_str())
    );
    // Reading the saved document migrates its SFTP column layout (docks.json
    // v1 -> v2, US-0124): the widths go and only the columns the user hid by
    // hand survive. The fixture hides `owner` and shows `permissions`, so one
    // entry is kept and the other falls back to the current defaults.
    let stored_sftp = stored_document
        .sftp_table_state
        .as_ref()
        .expect("saved SFTP table state");
    assert!(
        !expected_sftp.column_widths.is_empty(),
        "fixture has widths"
    );
    assert!(stored_sftp.column_widths.is_empty());
    assert_eq!(
        stored_sftp.column_visibility,
        std::collections::HashMap::from([("owner".to_string(), false)])
    );
    let stored = stored_document.dock_state::<DockAreaState>().unwrap();
    assert_eq!(stored.right_dock, expected.right_dock);
}

#[gpui::test]
fn load_reset_center_and_save_round_trip(cx: &mut TestAppContext) {
    let dir = temp_dir("roundtrip");
    let path = dir.0.join("docks.json");

    update_dock_document_at(&path, |document| {
        document.sftp_table_state = Some(SftpTableState {
            column_widths: HashMap::from([("name".to_string(), 321.0)]),
            column_visibility: HashMap::new(),
            ..SftpTableState::default()
        });
        Ok(())
    })
    .unwrap();

    let (source, cx) = dock_area(cx);
    let saved: DockAreaState = source.update_in(cx, |dock_area, window, cx| {
        let center = DockLayout::tabs()
            .panel_view(NamedPanel::view(panel_names::TERMINAL, cx), cx)
            .panel_view(NamedPanel::view(panel_names::TERMINAL, cx), cx);
        dock_area.set_center(center, window, cx);
        let right =
            DockLayout::tabs().panel_view(NamedPanel::view(panel_names::SSH_CLIENT, cx), cx);
        dock_area.set_dock(DockPlacement::Right, right, window, cx);
        dock_area.set_dock_size(DockPlacement::Right, px(333.), window, cx);
        dock_area.toggle_dock(DockPlacement::Right, window, cx);
        dock_area.dump(cx)
    });
    persistence::save_state_to(&path, &saved, Some(panel_names::TERMINAL), "test-first-run")
        .unwrap();

    let document = read_dock_document_from(&path)
        .unwrap()
        .expect("document exists");
    assert_eq!(
        document.zoomed_panel.as_deref(),
        Some(panel_names::TERMINAL)
    );

    let (target, cx) = cx.add_window_view(|window, cx| {
        DockArea::new("layout-test-2", Some(MAIN_DOCK_VERSION), window, cx)
    });
    cx.update(|window, cx| persistence::load_layout(&target, &document, window, cx).unwrap());
    let (size, open, _) = right_dock(&target, cx);
    assert_eq!((size, open), (333., false));

    let reset = cx
        .update(|window, cx| layout::apply_center_reset(target.downgrade(), window, cx))
        .expect("dock area alive");
    let center_terminals = target.read_with(cx, |dock_area, cx| {
        dock_area
            .layout(DockPlacement::Center)
            .expect("center layout")
            .panels()
            .filter_map(|id| dock_area.panel(id))
            .filter(|panel| panel.panel_name(cx) == panel_names::TERMINAL)
            .count()
    });
    assert_eq!(center_terminals, 1);
    assert_eq!(
        right_dock(&target, cx),
        (333., false, panel_names::SSH_CLIENT.to_string())
    );

    persistence::save_state_to(&path, &reset, None, "test-reset").unwrap();
    let written = read_dock_document_from(&path)
        .unwrap()
        .expect("document exists");
    assert_eq!(written.zoomed_panel, None);
    let written_state = written.dock_state::<DockAreaState>().unwrap();
    assert_eq!(written_state.center, reset.center);
    assert_eq!(
        written_state
            .right_dock
            .as_ref()
            .map(|dock| dock.panel().panel_name.as_str()),
        Some(panel_names::SSH_CLIENT),
    );
    assert_eq!(
        written.sftp_table_state.unwrap().column_widths.get("name"),
        Some(&321.0)
    );
}

/// BUG-0054: startup resets the center right after loading it, so `load_layout`
/// must build none of the persisted center's panels — every built `terminal`
/// panel spawns a local shell that the reset then discards mid-initialisation.
#[gpui::test]
fn load_layout_builds_no_center_panel_and_the_reset_builds_one(cx: &mut TestAppContext) {
    let dir = temp_dir("center-builds");
    let path = dir.0.join("docks.json");

    let (source, cx) = dock_area(cx);
    let saved: DockAreaState = source.update_in(cx, |dock_area, window, cx| {
        let center = DockLayout::tabs()
            .panel_view(NamedPanel::view(panel_names::TERMINAL, cx), cx)
            .panel_view(NamedPanel::view(panel_names::TERMINAL, cx), cx);
        dock_area.set_center(center, window, cx);
        let right =
            DockLayout::tabs().panel_view(NamedPanel::view(panel_names::SSH_CLIENT, cx), cx);
        dock_area.set_dock(DockPlacement::Right, right, window, cx);
        dock_area.dump(cx)
    });
    persistence::save_state_to(&path, &saved, None, "test-two-center-terminals").unwrap();
    let document = read_dock_document_from(&path)
        .unwrap()
        .expect("document exists");

    let builds = count_terminal_builds(cx);
    let (target, cx) = cx.add_window_view(|window, cx| {
        DockArea::new("layout-test-3", Some(MAIN_DOCK_VERSION), window, cx)
    });
    cx.update(|window, cx| persistence::load_layout(&target, &document, window, cx).unwrap());
    assert_eq!(
        builds.get(),
        0,
        "loading the saved layout must build none of the center's terminal panels"
    );

    cx.update(|window, cx| layout::apply_center_reset(target.downgrade(), window, cx))
        .expect("dock area alive");
    assert_eq!(
        builds.get(),
        1,
        "the center reset must build exactly one terminal panel"
    );
}

/// BUG-0054 (independent verification): the persisted centre is a *split* of
/// two tab groups, each holding a terminal, and the document also names a
/// zoomed panel that only lives in that centre. Loading must build none of
/// them and must not panic on the resulting empty centre; the zoom restore
/// must find nothing before the reset and the reset's own terminal after it.
/// The right dock must survive the load untouched, panel name included.
#[gpui::test]
fn load_layout_drops_a_split_centre_and_still_restores_zoom_after_the_reset(
    cx: &mut TestAppContext,
) {
    let dir = temp_dir("split-centre");
    let path = dir.0.join("docks.json");

    let (source, cx) = dock_area(cx);
    let saved: DockAreaState = source.update_in(cx, |dock_area, window, cx| {
        let centre = DockLayout::h_split()
            .child(
                DockLayout::tabs()
                    .panel_view(NamedPanel::view(panel_names::TERMINAL, cx), cx)
                    .panel_view(NamedPanel::view(panel_names::SFTP, cx), cx),
                Some(px(400.)),
            )
            .child(
                DockLayout::tabs().panel_view(NamedPanel::view(panel_names::TERMINAL, cx), cx),
                Some(px(400.)),
            );
        dock_area.set_center(centre, window, cx);
        let right = DockLayout::tabs().panel_view(NamedPanel::view(panel_names::AGENT, cx), cx);
        dock_area.set_dock(DockPlacement::Right, right, window, cx);
        dock_area.set_dock_size(DockPlacement::Right, px(464.), window, cx);
        dock_area.dump(cx)
    });
    // The fixture really is a split of two tab groups, not a single group.
    assert_eq!(saved.center.children.len(), 2);
    persistence::save_state_to(
        &path,
        &saved,
        Some(panel_names::TERMINAL),
        "test-split-centre",
    )
    .unwrap();
    let document = read_dock_document_from(&path)
        .unwrap()
        .expect("document exists");

    let builds = count_terminal_builds(cx);
    let (target, cx) = cx.add_window_view(|window, cx| {
        DockArea::new("layout-test-verify", Some(MAIN_DOCK_VERSION), window, cx)
    });
    cx.update(|window, cx| persistence::load_layout(&target, &document, window, cx).unwrap());

    assert_eq!(
        builds.get(),
        0,
        "a split centre's terminals must not be built by the load"
    );
    let centre_panels = target.read_with(cx, |dock_area, _| {
        dock_area
            .layout(DockPlacement::Center)
            .expect("centre layout")
            .panels()
            .count()
    });
    assert_eq!(centre_panels, 0, "the loaded centre must hold no panel");
    // The empty centre must render without panicking.
    cx.refresh().unwrap();
    cx.run_until_parked();

    // The right dock is untouched by the centre drop.
    assert_eq!(
        right_dock(&target, cx),
        (464., true, panel_names::AGENT.to_string())
    );

    // Nothing to zoom into yet, and asking must not panic.
    let zoomed_before =
        cx.update(|window, cx| restore_zoom_in_dock(&target, panel_names::TERMINAL, window, cx));
    assert!(!zoomed_before);
    assert!(!target.read_with(cx, |dock_area, _| dock_area.is_zoomed()));

    cx.update(|window, cx| layout::apply_center_reset(target.downgrade(), window, cx))
        .expect("dock area alive");
    assert_eq!(builds.get(), 1, "the reset builds exactly one terminal");
    let zoomed_after =
        cx.update(|window, cx| restore_zoom_in_dock(&target, panel_names::TERMINAL, window, cx));
    assert!(
        zoomed_after,
        "the saved zoom name must resolve against the reset centre"
    );
}
