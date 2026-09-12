//! What one frame's render state costs, recorded and never gated.
//!
//! Benchmark tier 3 of
//! `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md`,
//! measured against the engine being replaced: a full 200x50 viewport copy per
//! painted frame costs **29.9 us**
//! (`docs/spec-intakes/IN-0029-vt-engine/research/perf-baseline.md` § 4).
//!
//! Only the hand-off is timed — the grid mutation that makes a row dirty sits
//! outside the clock, so these numbers answer the same question the baseline
//! did: what does the renderer pay per frame?

use std::time::{Duration, Instant};

use crate::render::tests::Engine;
use crate::render::{Palette, RenderState};

/// `docs/spec-intakes/IN-0029-vt-engine/research/perf-baseline.md` § 4, scenario h.
const OLD_SNAPSHOT_US: f64 = 29.9;
const FRAMES: u32 = 2_000;

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "timings are only meaningful in a release build"
)]
fn render_state_build_cost_per_frame() {
    let mut engine = Engine::new(50, 200);
    let mut state = RenderState::new();
    let palette = Palette::new();
    let line = "the quick brown fox jumps over the lazy dog, repeatedly and at length.";

    // Prime: one full viewport of real content, then one Full update.
    engine.batch();
    for index in 0..50 {
        engine.write(index, line);
    }
    engine.update(&mut state);
    state.map_colors(&palette);

    let idle = measure(&mut engine, &mut state, &palette, 0, line);
    let one_row = measure(&mut engine, &mut state, &palette, 1, line);
    let all_rows = measure(&mut engine, &mut state, &palette, 50, line);

    eprintln!("render-state build cost per frame, 200x50, {FRAMES} frames each:");
    eprintln!(
        "  unchanged frame : {idle:>8.3} us  ({:.0}x the old snapshot)",
        OLD_SNAPSHOT_US / idle.max(f64::MIN_POSITIVE)
    );
    eprintln!(
        "  1 row changed   : {one_row:>8.3} us  ({:.1}x)",
        OLD_SNAPSHOT_US / one_row.max(f64::MIN_POSITIVE)
    );
    eprintln!(
        "  50 rows changed : {all_rows:>8.3} us  ({:.1}x)",
        OLD_SNAPSHOT_US / all_rows.max(f64::MIN_POSITIVE)
    );
    eprintln!("  old full snapshot (alacritty_terminal, measured): {OLD_SNAPSHOT_US:.1} us");

    // Recorded, not gated — but the shape of the claim is asserted: the cost
    // scales with change, not with viewport area.
    assert!(idle < one_row, "an unchanged frame cost more than one row");
    assert!(
        one_row < all_rows,
        "one row cost more than the whole viewport"
    );
}

/// Time `FRAMES` frames, each dirtying `rows` rows before the clock starts.
fn measure(
    engine: &mut Engine,
    state: &mut RenderState,
    palette: &Palette,
    rows: u16,
    line: &str,
) -> f64 {
    let mut total = Duration::ZERO;
    for frame in 0..FRAMES {
        if rows > 0 {
            engine.batch();
            for index in 0..rows {
                engine.write((index + frame as u16) % 50, line);
            }
        }
        let started = Instant::now();
        engine.update(state);
        state.map_colors(palette);
        total += started.elapsed();
    }
    total.as_secs_f64() * 1e6 / f64::from(FRAMES)
}
