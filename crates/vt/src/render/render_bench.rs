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
use crate::{Config, EventBatch, Size, Terminal};

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
    eprintln!("  old full snapshot (the fork, measured): {OLD_SNAPSHOT_US:.1} us");

    // Recorded, not gated — but the shape of the claim is asserted: the cost
    // scales with change, not with viewport area.
    assert!(idle < one_row, "an unchanged frame cost more than one row");
    assert!(
        one_row < all_rows,
        "one row cost more than the whole viewport"
    );
}

/// What the debug integrity walk costs per `feed` and per `render_update` over
/// a full 100 000-row history (R-28, the `US-0075` / `US-0079` rework).
///
/// Recorded, not a benchmark gate — but the R-28 budget itself is asserted
/// without the feature, with a margin of two orders of magnitude, so a walk
/// that goes back to O(history) fails here instead of in a user's session.
/// Run it both ways: `--features vt-paranoid` is the "before" number.
#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "the integrity walk is compiled out of release builds"
)]
fn integrity_walk_cost_per_feed_and_render_update() {
    const HISTORY: usize = 100_000;
    // Ten is enough for a number whose two states differ by three orders of
    // magnitude, and keeps the `vt-paranoid` CI run down to a few seconds.
    const CALLS: u32 = 10;

    let mut term = Terminal::new(
        Size {
            rows: 45,
            cols: 160,
        },
        Config {
            scrollback_limit: HISTORY as u32,
            ..Config::default()
        },
    );
    let mut batch = EventBatch::default();
    let mut state = RenderState::new();
    let palette = Palette::new();

    // One feed, so filling the history costs one walk rather than 100 000.
    // A screen's worth over the limit, so the history is full, not one short.
    let mut fill = String::with_capacity(HISTORY * 3);
    for row in 0..HISTORY + 64 {
        fill.push_str("row ");
        fill.push_str(&row.to_string());
        fill.push_str("\r\n");
    }
    term.feed(fill.as_bytes(), &mut batch, Instant::now());
    term.render_update(&mut state, Instant::now());
    state.map_colors(&palette);
    assert_eq!(term.grid().primary().history_len() as usize, HISTORY);

    let mut fed = Duration::ZERO;
    let mut rendered = Duration::ZERO;
    for call in 0..CALLS {
        let line = format!("probe {call}\r\n");
        let started = Instant::now();
        term.feed(line.as_bytes(), &mut batch, Instant::now());
        fed += started.elapsed();
        let started = Instant::now();
        term.render_update(&mut state, Instant::now());
        rendered += started.elapsed();
    }
    let per_feed = fed.as_secs_f64() * 1e6 / f64::from(CALLS);
    let per_render = rendered.as_secs_f64() * 1e6 / f64::from(CALLS);

    eprintln!(
        "integrity walk over {HISTORY} history rows, 160x45, vt-paranoid = {}:",
        cfg!(feature = "vt-paranoid")
    );
    eprintln!("  feed(one line)  : {per_feed:>9.1} us");
    eprintln!("  render_update() : {per_render:>9.1} us");

    #[cfg(not(feature = "vt-paranoid"))]
    {
        // R-28: bounded to what the operation touched, so the history depth
        // must not show up in either number. Both are a couple of hundred
        // microseconds when the bound holds — the two 45-row screens and their
        // cells, nothing more — and hundreds of milliseconds when it does not.
        assert!(
            per_feed < 1000.0,
            "feed cost {per_feed:.1} us is O(history)"
        );
        assert!(
            per_render < 1000.0,
            "render_update cost {per_render:.1} us is O(history)"
        );
    }
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
