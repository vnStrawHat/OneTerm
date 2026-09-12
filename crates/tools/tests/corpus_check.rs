//! The parity drift gate.
//!
//! Replays all 45 vendored alacritty reference recordings through the engine
//! being replaced and compares them, cell for cell, against the expectations
//! frozen at `US-0072`. It runs inside `cargo test --workspace`, so an
//! expectation that drifts — or a declared expected difference that has gone
//! stale — fails the build rather than waiting for someone to run a tool.
//!
//! Until `oneterm-vt` exists this is old-engine-against-old-engine, which is
//! exactly the self-test of the comparator that `US-0072` owes. From `US-0076`
//! the same comparison runs against the new engine and the expectations do not
//! move.

use oneterm_tools::corpus;

#[test]
fn the_alacritty_reference_corpus_matches_its_frozen_expectations() {
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
