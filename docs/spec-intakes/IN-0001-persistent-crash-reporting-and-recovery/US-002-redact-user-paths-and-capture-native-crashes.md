# Work: Redact user paths and capture native crashes

ID: US-002
Intake: IN-0001
Created: 2026-08-11

## Classification

- Change type: existing-contract change and new capability
- Risk lane: high-risk
- Spec Intake: IN-0001 follow-up intake #2

## Outcome

Crash reports never expose the current user's home-directory prefix, and OneTerm records a pending recovery report for supported platform-native fatal exceptions/signals through `crash-handler` 0.8.0.

## Scope

- In scope: home-directory redaction before panic report persistence and again on load; cleanup of legacy unredacted backups; native crash capture on Windows, Linux, and macOS targets supported by `crash-handler`; compromised-context-safe staging writes; promotion into the existing pending-report lifecycle on restart.
- Out of scope: full native stack unwinding/minidumps, crashes that prevent the native callback or kernel write from running, forced OS termination/power loss, automatic upload, and paths outside the current home directory.

## Acceptance

- Panic payloads, locations, and backtrace text replace the current home-directory prefix with `<USER_HOME>` before persistence.
- Redaction recognizes native and alternate slash separators; on Windows matching is ASCII case-insensitive.
- Loading an older report sanitizes its persisted content and removes an unredacted backup.
- `crash-handler = 0.8.0` is an exact workspace dependency used only by `oneterm-app`.
- Native capture performs no heap allocation, locking, logging, formatting allocation, or ordinary persistence transaction inside the compromised callback.
- The callback writes a pre-opened staging file with app/platform metadata and bounded platform crash context, then returns `Handled(false)` so normal crash termination/other handlers continue.
- On the next launch, a non-empty native staging report is promoted into the existing pending recovery report before the new handler is installed.
- Failure to install the native handler is logged and does not prevent startup or Rust panic capture.
- Focused tests cover redaction, legacy sanitization, staging promotion, and simulated native callback capture where supported.

## Documentation

### Owning Docs Reviewed

- `docs/crash-reporting.md` — current capture, recovery, retention, and privacy contract.
- `docs/agents/dependencies.md` — new dependency intake and reference rules.
- `docs/agents/error-policy.md` — initialization failure and observability behavior.
- `docs/agents/persistence.md` — report persistence and blocking I/O boundary.
- `docs/agents/code-style.md` — unsafe-block documentation and tests.
- `crash-handler` 0.8.0 docs/source — compromised callback safety, `CrashHandler::attach`, platform coverage, `CrashContext`, and `CrashEventResult` semantics.

### Documentation Action

Update required: revise `docs/crash-reporting.md` for home-path privacy, native capture coverage, staging lifecycle, and limitations; add `crash-handler` to `docs/agents/dependencies.md` as the approved native crash dependency.

Reason: privacy and supported crash classes are user-visible support contracts future work must preserve.

### Reconciliation

Updated `docs/crash-reporting.md` with the redaction guarantee, native platform coverage, compromised-context staging design, recovery promotion, and best-effort limitations. Updated `docs/agents/dependencies.md` with the approved exact native crash dependency. Updated the original intake with the accepted follow-up scope.

## Context

`crash-handler` explicitly states that callbacks run in a compromised context; Linux permits only async-signal-safe operations and allocation can be undefined behavior. Therefore the native callback may only build bounded text in a stack buffer and invoke a direct write syscall on a file opened before handler installation. Native reports are staged separately so an open descriptor does not interfere with Dismiss cleanup or erase a report from the previous run.

## Plan

1. Update owning docs and add the exact dependency after recording this intake/work packet as the required dependency issue.
2. Add centralized home-prefix redaction and legacy on-load sanitization with focused tests.
3. Add native staging promotion, pre-opened direct writer, platform context formatting, handler installation, and simulated-callback tests.
4. Review unsafe invariants, dependency graph, and changed source; run all focused and workspace gates.

## Decisions

No separate decision record: callback safety and staging ownership are captured in the owning crash-report contract.

## Verification Plan

- `cargo test -p oneterm-app -p oneterm-settings-ui`
- Platform simulation test through `CrashHandler::simulate_exception` / equivalent where supported.
- `python scripts/verify-dependency-graph.py`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --workspace`
- `cargo test --workspace`
- Manual native crash/restart where safely available; report gaps explicitly.

## Evidence and Gaps

- `cargo test -p oneterm-app crash_report::tests` — passed: 7 focused redaction, legacy rewrite, staging promotion, and lifecycle tests.
- `cargo test -p oneterm-app native_crash::tests` — passed: 2 tests, including a Windows `CrashHandler::simulate_exception` callback that wrote and validated native exception code `0xCCA11ED`.
- `cargo test -p oneterm-app -p oneterm-settings-ui` — passed: 14 tests across 5 suites.
- `cargo tree -p oneterm-app -e normal` — confirms exact `crash-handler v0.8.0` and `crash-context v0.8.0` resolution.
- `python scripts/verify-dependency-graph.py` — passed for 18 workspace packages and explicit members.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed with no issues.
- `cargo build --workspace` — passed.
- `cargo test --workspace` — passed: 560 passed, 2 ignored, 64 filtered out across 41 suites.
- `srcwalk review` and `git diff --check` — completed without whitespace errors; unsafe direct-write blocks document handle, buffer, and syscall invariants.
- Native callback simulation was run on Windows. Only the Windows Rust target is installed in this environment, so Linux/macOS code paths were source-reviewed against `crash-handler`/`crash-context` 0.8.0 but not cross-compiled or executed here.
- A destructive real access violation was not triggered; the crate-provided simulation exercises the same registered callback without terminating the test process.

## Handoff

Single-session implementation; no blocker.

## Harness Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Harness Proof

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->
