//! The parity gate.
//!
//! Replays all 45 vendored alacritty reference recordings and compares them,
//! cell for cell, against the expectations frozen at `US-0072`. It runs inside
//! `cargo test --workspace`, so an expectation that drifts — or a declared
//! expected difference that has gone stale — fails the build rather than
//! waiting for someone to run a tool.
//!
//! Two engines, two different jobs:
//!
//! * **old against the frozen files** is the comparator's own self-test, which
//!   `US-0072` owes: it must stay at zero differences forever, because the files
//!   were blessed by that engine.
//! * **new against the frozen files** is `US-0076`'s exit criterion, and the one
//!   that can legitimately differ — but only inside a declared
//!   `expected-diffs.json` window naming a correction.

use oneterm_tools::corpus::{self, Engine};

fn run(engine: Engine) {
    let dir = corpus::alacritty_ref_dir();
    let recordings = corpus::load_all(&dir, None).expect("the corpus is vendored");
    assert_eq!(
        recordings.len(),
        45,
        "expected the 45 vendored alacritty reference recordings under {}",
        dir.display()
    );

    let mut failures = Vec::new();
    for recording in &recordings {
        let report = corpus::check_recording(recording, engine)
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
fn the_alacritty_reference_corpus_matches_its_frozen_expectations() {
    run(Engine::Old);
}

#[test]
fn the_new_engine_matches_the_frozen_expectations() {
    run(Engine::New);
}
