//! Reporting for operations that are intentionally best effort.
//!
//! `docs/agents/error-policy.md` forbids a bare `let _ =` on a runtime
//! operation: when a failure is deliberately tolerated, the operation name and
//! the error must still reach the log so the failure can be diagnosed without
//! reproducing the action. Every crate that discards a result on purpose
//! routes it through [`report_best_effort`].

use std::fmt::Display;

/// Log (at `warn`) and discard the failure of a best-effort `operation`.
///
/// Use this instead of `let _ = fallible()` when continuing is correct even if
/// the operation failed (cleanup of temporaries, closing an already-closed
/// channel, notifying a consumer that may have gone away). The message names
/// the operation so the log line is actionable on its own.
pub fn report_best_effort<T, E: Display>(operation: &str, result: Result<T, E>) {
    if let Err(error) = result {
        log::warn!("{operation}: best-effort operation failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_any_result_shape() {
        report_best_effort("unit test ok", Ok::<u8, String>(1));
        report_best_effort("unit test err", Err::<(), _>("boom"));
    }
}
