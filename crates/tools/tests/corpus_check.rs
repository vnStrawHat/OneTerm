//! The parity gate.
//!
//! Replays all 45 vendored alacritty reference recordings — plus OneTerm's own
//! recordings under `oneterm/`, which cover behaviour the vendored set never
//! reaches — and compares them, cell for cell, against the expectations frozen
//! by the engine being replaced. It runs inside
//! `cargo test --workspace`, so an expectation that drifts — or a declared
//! expected difference that has gone stale — fails the build rather than
//! waiting for someone to run a tool.
//!
//! This was `US-0076`'s exit criterion and it is the standing one. `oneterm-vt`
//! may legitimately differ from the fork that blessed these files — but only
//! inside a declared `expected-diffs.json` window naming a correction. Since
//! `US-0087` there is no second engine to fall back on: the frozen files are the
//! whole reference, and nothing can re-bless them.

use oneterm_tools::corpus;

fn run() {
    let dir = corpus::alacritty_ref_dir();
    let recordings = corpus::load_all(&dir, None).expect("the corpus is vendored");
    assert_eq!(
        recordings.len(),
        45,
        "expected the 45 vendored alacritty reference recordings under {}",
        dir.display()
    );
    check(&recordings);
}

/// OneTerm's own recordings, for behaviour the vendored set does not reach —
/// Sixel, today. Same rules: blessed once by the engine being replaced, then
/// frozen.
fn run_oneterm() {
    let dir = corpus::oneterm_dir();
    let recordings = corpus::load_all(&dir, None).expect("OneTerm's own recordings");
    assert!(
        !recordings.is_empty(),
        "expected OneTerm's own recordings under {}",
        dir.display()
    );
    check(&recordings);
}

fn check(recordings: &[corpus::Recording]) {
    let mut failures = Vec::new();
    for recording in recordings {
        let report = corpus::check_recording(recording)
            .unwrap_or_else(|error| panic!("{}: {error:#}", recording.name));
        if report.passed() {
            continue;
        }
        let mut lines = vec![format!("{}:", recording.name)];
        lines.extend(report.stale.iter().map(|stale| format!("  stale: {stale}")));
        lines.extend(
            report
                .undeclared
                .iter()
                .take(5)
                .map(|difference| format!("  {difference}")),
        );
        if report.undeclared.len() > 5 {
            lines.push(format!("  ... and {} more", report.undeclared.len() - 5));
        }
        failures.push(lines.join("\n"));
    }

    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn the_engine_matches_the_frozen_alacritty_expectations() {
    run();
}

#[test]
fn the_engine_matches_the_frozen_oneterm_expectations() {
    run_oneterm();
}
