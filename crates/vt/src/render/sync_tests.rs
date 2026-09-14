//! Mode 2026, driven by an injected clock so nothing here is time-dependent.
//!
//! Named for the verification list in
//! `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md`.

use crate::render::sync::{SYNC_REFRESH, SYNC_WATCHDOG};
use crate::render::tests::Engine;
use crate::render::{RenderState, RenderUpdate};

#[test]
fn mode_2026_suppresses_frames_until_close() {
    let mut engine = Engine::new(6, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);

    let opened = engine.now;
    engine.sync.begin(opened);
    engine.batch();
    engine.write(2, "inside the update");

    assert_eq!(
        engine.update_at(&mut state, opened + SYNC_REFRESH / 2),
        RenderUpdate::Unchanged
    );
    assert!(state.changed().is_empty());

    engine.sync.end();

    assert_eq!(
        engine.update_at(&mut state, opened + SYNC_REFRESH / 2),
        RenderUpdate::Partial { scrolled: 0 }
    );
    assert_eq!(state.changed(), &[2]);
}

#[test]
fn mode_2026_refresh_deadline_resumes_frames() {
    let mut engine = Engine::new(6, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);

    let opened = engine.now;
    engine.sync.begin(opened);
    engine.batch();
    engine.write(1, "held back");

    assert_eq!(
        engine.update_at(&mut state, opened + SYNC_REFRESH / 2),
        RenderUpdate::Unchanged
    );
    // The program stopped refreshing the update; the renderer stops waiting.
    assert_eq!(
        engine.update_at(&mut state, opened + SYNC_REFRESH),
        RenderUpdate::Partial { scrolled: 0 }
    );
    assert_eq!(state.changed(), &[1]);
}

#[test]
fn mode_2026_watchdog_forces_the_mode_off() {
    let mut engine = Engine::new(6, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);

    let opened = engine.now;
    engine.sync.begin(opened);
    engine.batch();
    engine.write(3, "never closed");

    // A program that keeps the update open by refreshing it every frame.
    let mut elapsed = SYNC_REFRESH / 2;
    while elapsed < SYNC_WATCHDOG {
        engine.sync.begin(opened + elapsed);
        assert_eq!(
            engine.update_at(&mut state, opened + elapsed),
            RenderUpdate::Unchanged
        );
        elapsed += SYNC_REFRESH / 2;
    }

    engine.sync.begin(opened + SYNC_WATCHDOG);
    let update = engine.update_at(&mut state, opened + SYNC_WATCHDOG);

    assert_eq!(update, RenderUpdate::Partial { scrolled: 0 });
    assert_eq!(state.changed(), &[3]);
    assert!(!engine.sync.is_set(), "the watchdog left the mode set");
}

#[test]
fn decrqm_reports_2026_as_set_while_open() {
    let mut engine = Engine::new(6, 20);
    assert!(!engine.sync.is_set());

    engine.sync.begin(engine.now);
    assert!(engine.sync.is_set());

    engine.sync.end();
    assert!(!engine.sync.is_set());
}

#[test]
fn a_first_update_inside_a_sync_block_still_builds_the_state() {
    let mut engine = Engine::new(6, 20);
    let mut state = RenderState::new();
    engine.sync.begin(engine.now);
    engine.batch();
    engine.write(0, "already on screen");

    // R-15: rows() holds the full viewport for every result, so a state that has
    // never been built is built before suppression can start.
    let update = engine.update_at(&mut state, engine.now);

    assert_eq!(update, RenderUpdate::Full);
    assert_eq!(state.rows().len(), 6);
    // From here on the block does what it says.
    engine.batch();
    engine.write(1, "inside the update");
    assert_eq!(
        engine.update_at(&mut state, engine.now),
        RenderUpdate::Unchanged
    );
}

#[test]
fn a_mode_change_inside_a_sync_block_reaches_the_next_frame() {
    let mut engine = Engine::new(6, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);

    let opened = engine.now;
    engine.sync.begin(opened);
    // DECTCEM off, and not one row touched.
    engine.modes.show_cursor = false;
    assert_eq!(
        engine.update_at(&mut state, opened + SYNC_REFRESH / 2),
        RenderUpdate::Unchanged
    );

    engine.sync.end();

    // The snapshot was refreshed on the skipped frame, so the next frame sees
    // no difference: only the sticky flag makes the view repaint the cursor.
    assert_eq!(
        engine.update_at(&mut state, opened + SYNC_REFRESH / 2),
        RenderUpdate::Partial { scrolled: 0 }
    );
    assert!(!state.modes().show_cursor);
    assert!(state.changed().is_empty());
    // And it is reported once, not forever.
    assert_eq!(
        engine.update_at(&mut state, opened + SYNC_REFRESH),
        RenderUpdate::Unchanged
    );
}

#[test]
fn a_cursor_move_inside_a_sync_block_reaches_the_next_frame() {
    let mut engine = Engine::new(6, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);

    let opened = engine.now;
    engine.sync.begin(opened);
    engine.batch();
    engine.grid.screen_mut().goto(4, 9);
    assert_eq!(
        engine.update_at(&mut state, opened + SYNC_REFRESH / 2),
        RenderUpdate::Unchanged
    );

    engine.sync.end();

    assert_eq!(
        engine.update_at(&mut state, opened + SYNC_REFRESH / 2),
        RenderUpdate::Partial { scrolled: 0 }
    );
    assert_eq!(state.cursor().row, Some(4));
    assert_eq!(state.cursor().col, 9);
}

#[test]
fn mode_snapshot_is_refreshed_even_when_the_frame_is_skipped() {
    let mut engine = Engine::new(6, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);

    let opened = engine.now;
    engine.sync.begin(opened);
    engine.modes.bracketed_paste = true;
    engine.modes.app_cursor = true;

    assert_eq!(
        engine.update_at(&mut state, opened + SYNC_REFRESH / 2),
        RenderUpdate::Unchanged
    );
    assert!(state.modes().bracketed_paste);
    assert!(state.modes().app_cursor);
}
