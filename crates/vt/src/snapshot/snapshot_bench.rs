// What one frame's snapshot state costs, recorded and never gated.
//
// Benchmark tier 3 of
// `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md`,
// measured against the engine being replaced: a full 200x50 viewport copy per
// painted frame costs 29.9 us
// (`docs/spec-intakes/IN-0029-vt-engine/research/perf-baseline.md` section 4).
//
// Only the hand-off is timed - the grid mutation that makes a row dirty sits
// outside the clock, so these numbers answer the same question the baseline
// did: what does the renderer pay per frame?

use std::time::{Duration, Instant};

use crate::snapshot::tests::Engine;
use crate::snapshot::{Palette, SnapshotState};
use crate::{Config, EventBatch, Size, Terminal};

// `docs/spec-intakes/IN-0029-vt-engine/research/perf-baseline.md` section 4,
// scenario h.
const OLD_SNAPSHOT_US: f64 = 29.9;
const FRAMES: u32 = 2_000;

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "timings are only meaningful in a release build"
)]
fn render_state_build_cost_per_frame() {
    let mut engine = Engine::new(50, 200);
    let mut state = SnapshotState::new();
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

    eprintln!("snapshot-state build cost per frame, 200x50, {FRAMES} frames each:");
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

// What the debug integrity walk costs per `feed` and per `snapshot_update` over a
// full 100 000-row history (R-28, the `US-0075` / `US-0079` rework).
//
// The numbers are recorded, not a benchmark gate. What is asserted is R-28
// itself - "bounded to what the operation touched, so the history depth must not
// appear in the cost" - and that is a shape, not a duration: the same probe loop
// runs twice in this process, over a full history and over almost none, and the
// two must cost the same. A wall-clock ceiling on its own cannot tell an
// O(history) walk from a descheduled thread, which is what it did on a loaded CI
// runner (`BUG-0075`); a ratio between two loops on the same machine in the same
// second can, because the machine cancels out of it.
/// Run it both ways: `--features vt-paranoid` is the "before" number.
#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "the integrity walk is compiled out of release builds"
)]
fn integrity_walk_cost_per_feed_and_snapshot_update() {
    const HISTORY: usize = 100_000;

    let (full_feed, full_render) = integrity_walk_probe(HISTORY);
    let (empty_feed, empty_render) = integrity_walk_probe(0);
    let feed_ratio = full_feed / empty_feed.max(f64::MIN_POSITIVE);
    let render_ratio = full_render / empty_render.max(f64::MIN_POSITIVE);

    eprintln!(
        "integrity walk over {HISTORY} history rows, 160x45, vt-paranoid = {}:",
        cfg!(feature = "vt-paranoid")
    );
    eprintln!(
        "  feed(one line)  : {full_feed:>9.1} us  ({empty_feed:>7.1} us near-empty, {feed_ratio:.2}x)"
    );
    eprintln!(
        "  snapshot_update() : {full_render:>9.1} us  ({empty_render:>7.1} us near-empty, {render_ratio:.2}x)"
    );

    #[cfg(not(feature = "vt-paranoid"))]
    {
        // Both probes walk the same two 45-row screens; only the scrollback
        // behind them differs. So the bounded walk puts the ratio at about 1,
        // and an O(history) walk puts it at about 1700 (`US-0075`'s measured
        // before/after). 20 sits between the two with more than an order of
        // magnitude of margin on each side, and it is not a timing threshold -
        // it does not move when the machine does.
        const DEPTH_RATIO: f64 = 20.0;
        // ...plus a backstop for a regression that slows both probes and so
        // cannot show in their ratio. Loose enough that a debug build on a
        // loaded 2-vCPU runner never reaches it - the honest cost is ~150 us -
        // and tight enough that the 250 ms whole-history walk does.
        const CEILING_US: f64 = 20_000.0;

        assert!(
            feed_ratio < DEPTH_RATIO,
            "feed cost {full_feed:.1} us over {HISTORY} history rows \
             against {empty_feed:.1} us over almost none, {feed_ratio:.1}x: O(history)"
        );
        assert!(
            render_ratio < DEPTH_RATIO,
            "snapshot_update cost {full_render:.1} us over {HISTORY} history rows \
             against {empty_render:.1} us over almost none, {render_ratio:.1}x: O(history)"
        );
        assert!(
            full_feed < CEILING_US,
            "feed cost {full_feed:.1} us, over the {CEILING_US:.0} us ceiling"
        );
        assert!(
            full_render < CEILING_US,
            "snapshot_update cost {full_render:.1} us, over the {CEILING_US:.0} us ceiling"
        );
    }
}

// Fill `history` rows of scrollback under a full screen, then time `CALLS`
// `feed` + `snapshot_update` pairs over it. Returns the *cheapest* call of
// each, in microseconds.
//
// The cheapest, not the mean: the walk is what every call pays, a descheduling
// is what one call pays, and it is only ever added. Over ten samples the
// minimum is the estimator a loaded runner cannot inflate - and cannot deflate
// an O(history) walk either, since that one costs a quarter of a second in
// every sample (`BUG-0075`).
//
// The scrollback limit is the same in both calls, so the two probes differ in
// the one variable R-28 is about: how many rows are live behind the screen.
//
// Plain `//`, not `///`: the crate's published rustdoc must read for an
// embedder who does not have this repository, so `ci-local` and the CI
// `vt-package` job forbid a work-packet citation in `///` text anywhere under
// `crates/vt/src`. A private test helper owes no rustdoc, so the citation stays
// and the doc comment goes.
fn integrity_walk_probe(history: usize) -> (f64, f64) {
    // Ten is enough for a minimum to find a clean sample, and keeps the
    // `vt-paranoid` CI run - where one call is a quarter of a second - down to a
    // few seconds.
    const CALLS: u32 = 10;
    const SCROLLBACK: u32 = 100_000;

    let mut term = Terminal::new(
        Size {
            rows: 45,
            cols: 160,
        },
        Config {
            scrollback_limit: SCROLLBACK,
            ..Config::default()
        },
    );
    let mut batch = EventBatch::default();
    let mut state = SnapshotState::new();
    let palette = Palette::new();

    // One feed, so filling the history costs one walk rather than 100 000.
    // A screen's worth on top, so the history is full and not one short, and so
    // both probes walk an equally populated screen.
    let mut fill = String::with_capacity((history + 64) * 3);
    for row in 0..history + 64 {
        fill.push_str("row ");
        fill.push_str(&row.to_string());
        fill.push_str("\r\n");
    }
    term.feed(fill.as_bytes(), &mut batch, Instant::now());
    term.snapshot_update(&mut state, Instant::now());
    state.map_colors(&palette);
    // Both ends, not just the lower one. The single shape that could silently
    // disarm the caller's ratio is a control whose own history is deep - the
    // quotient would sit at 1.0 for ever, whatever the walk did - so the depth
    // is pinned to the fill in both probes, as it was before the rework.
    let filled = term.grid().primary().history_len() as usize;
    assert!(
        (history..history + 64).contains(&filled),
        "history filled to {filled} rows, not {history}"
    );

    let mut fed = Duration::MAX;
    let mut rendered = Duration::MAX;
    for call in 0..CALLS {
        let line = format!("probe {call}\r\n");
        let started = Instant::now();
        term.feed(line.as_bytes(), &mut batch, Instant::now());
        fed = fed.min(started.elapsed());
        let started = Instant::now();
        term.snapshot_update(&mut state, Instant::now());
        rendered = rendered.min(started.elapsed());
    }
    (fed.as_secs_f64() * 1e6, rendered.as_secs_f64() * 1e6)
}

/// Time `FRAMES` frames, each dirtying `rows` rows before the clock starts.
fn measure(
    engine: &mut Engine,
    state: &mut SnapshotState,
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
