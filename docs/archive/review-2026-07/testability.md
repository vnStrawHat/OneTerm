# Testability Review

**Score: 4.5 / 10**

## TEST-01 — Important SFTP behavior has no automated coverage

- **Files:** `crates/sftp-ui/src/` (no test functions found); `crates/ssh/src/sftp.rs`; `crates/ssh/src/sftp_task.rs`
- **Modules:** SFTP backend, transfer engine, file browser
- **Severity:** **High**
- **Explanation:** The repository has unit tests in terminal, parser, shell-resolution, and space-tree areas, but no SFTP UI tests and no SFTP transfer/path tests. The highest-risk code—recursive walking, cancellation, partial files, path joining, command saturation—has no fake backend or loopback coverage.
- **Why it matters:** The critical path/security findings cannot be regression-tested today.
- **Recommended solution:** Extract pure path/plan functions and test them with malicious names, symlinks, Windows separators, empty/huge trees, and cancellation. Add a fake `SftpBackend` and an in-memory/loopback transfer harness. Add GPUI tests for async action state transitions.

## TEST-02 — Host-key security has no tests because the feature does not exist

- **Files:** `crates/ssh/src/handler.rs:10-19`; `crates/ssh/src/session.rs:120-169`
- **Modules:** SSH authentication/verification
- **Severity:** **Critical**
- **Explanation:** `check_server_key` has no known-hosts behavior to test and no test asserts that unknown/mismatched keys fail closed.
- **Why it matters:** Security behavior will be easy to regress or accidentally ship as accept-all.
- **Recommended solution:** Before wiring real connections, add unit tests for matching key, unknown key approval/rejection, changed key, malformed known-hosts data, host+port canonicalization, and permissions. Add an integration test with a local SSH server or fixture handler.

## TEST-03 — Persistence is not injected and is effectively untested

- **Files:** `crates/core/src/config/shell.rs:77-103`; `crates/settings/src/{terminal_config/mod.rs,ui_config.rs}`; `crates/session-ui/src/session_state.rs`; `crates/workspace/src/layout/workspace/persistence.rs`
- **Modules:** File persistence
- **Severity:** **High**
- **Explanation:** Paths are resolved from process environment/debug mode and writers call real filesystem APIs. Existing tests mostly deserialize values in memory; they do not exercise atomic writes, corrupt-file recovery, concurrent writers, permissions, or temp directories.
- **Why it matters:** Persistence failures are user-data failures, yet the code has no deterministic test seam.
- **Recommended solution:** Introduce a `ConfigStore`/path provider or pass a directory into persistence functions. Use `tempfile` in tests for missing, corrupt, interrupted, and concurrent-save scenarios.

## TEST-04 — Process/global state makes isolation difficult

- **Files:** `crates/terminal/src/factory.rs:48-58`; `crates/state/src/commands.rs:30-40`; GPUI global wrappers across `state`, `settings`, and feature crates
- **Modules:** Application composition
- **Severity:** **Medium**
- **Explanation:** `OnceLock` accepts only the first factory, and global initialization is expected once per process. Tests cannot replace the factory or run two independently configured app contexts without special process isolation.
- **Why it matters:** Hidden global state creates order-dependent tests and prevents reusable headless application harnesses.
- **Recommended solution:** Keep a production adapter around an app-owned service registry, but allow test-scoped construction/reset. Make duplicate registration return a diagnostic error in test builds instead of silently ignoring it.

## TEST-05 — Local shell tests rely on real platform shells and sleeps

- **Files:** `crates/local-shell/src/session_tests.rs:1-220`
- **Modules:** Local PTY integration tests
- **Severity:** **Medium**
- **Explanation:** Tests spawn the host shell, write commands, and use `thread::sleep`/time-bounded polling. They are platform-dependent and can be flaky under CI load or unusual shell environments.
- **Why it matters:** A green or red result may reflect the runner rather than OneTerm behavior, and the suite does not cover the equivalent Windows/Unix paths uniformly.
- **Recommended solution:** Separate deterministic listener/parser tests from a small opt-in PTY integration suite. Use readiness/event predicates rather than fixed sleeps, inject the shell command, and run platform-specific tests explicitly in CI.

## TEST-06 — UI feature coverage is uneven

- **Files:** `crates/settings-ui/src/`, `crates/sftp-ui/src/`, `crates/app/src/`, `crates/theme/src/`, plus test inventory
- **Modules:** Settings, SFTP, app startup, themes
- **Severity:** **Medium**
- **Explanation:** A repository-wide count found roughly 395 test functions, concentrated in `terminal` (180), `terminal-view` (78), `highlight` (67), and `local-shell` (25). `sftp-ui`, `settings-ui`, `theme`, and `app` have no test functions in the source inventory.
- **Why it matters:** The most integration-heavy composition and persistence code has the least coverage.
- **Recommended solution:** Add contract tests at feature boundaries, not only snapshot/render tests: startup order, panel registration, command registry availability, settings round trips, and SFTP state swap/purge.

## Testability strengths

- `crates/terminal/src/test_support.rs` provides a fake session, probes, bounded transports, and observable writes/close state.
- Pure tests cover OSC parsing, URL policy, paste sanitization, encoding, highlighting, shell resolution, and split-tree invariants.
- `terminal-view` has GPUI tests for panel shutdown and view behavior.
- Backend listeners have tests for queue saturation and lifecycle guarantees.

---

## Remediation status (2026-07-22)

All scoped Testability work items and acceptance criteria in
[`remediation-plan.md`](remediation-plan.md) are complete. The findings above are retained as the
pre-remediation baseline.

### TEST-01 — Resolved

- SFTP traversal, path containment, dangerous components, symlink rejection, cancellation,
  bounded discovery, and atomic finalization have direct unit tests in
  `crates/ssh/src/sftp_transfer.rs` and `crates/ssh/src/sftp_task.rs`.
- `crates/sftp-ui/src/browser_state.rs` provides a fake `SftpBackend` and verifies state creation,
  backend swapping, closed-backend purge, and prevention of stale state recreation.
- These tests do not require a remote SSH or SFTP server.

### TEST-02 — Resolved

- Host-key unit tests cover matching, unknown, approved, changed, malformed, and wrong-fingerprint
  cases in `crates/ssh/src/handler.rs`.
- A Tokio loopback russh server exercises the real client handshake: strict mode rejects the first
  unknown key, explicit fingerprint approval persists it, and a subsequent strict connection succeeds.

### TEST-03 — Resolved

- `TerminalConfig`, `UiConfig`, `SshSessionStore`, and `DockDocument` now offer explicit-path
  load/save/update seams while their production methods retain the standard configuration paths.
- Isolated temporary-directory tests cover round trips, corrupt-file quarantine, partial-schema
  migration defaults, atomic replacement, and typed malformed-document errors.
- Core persistence tests cover same-directory atomic replacement and concurrent JSON updates.

### TEST-04 — Resolved

- `SessionFactorySlot` is independently constructible. Production uses one static slot, while tests
  use fresh slots and verify isolation plus duplicate-registration rejection without changing the
  process-global factory.
- Existing command-registry tests retain duplicate-registration diagnostics at the GPUI boundary.

### TEST-05 — Resolved

- Local PTY integration tests wait for lifecycle or terminal snapshot predicates with deadlines
  instead of using fixed setup/output sleeps.
- Shell resolution remains covered by pure platform-specific tests, while listener and terminal-model
  tests remain deterministic and avoid spawning a shell.

### TEST-06 — Resolved

- Boundary tests cover startup service registration, panel shutdown, settings persistence/schema
  round trips, and SFTP backend state swap/purge behavior.
- `.github/workflows/ci.yml` runs portable core, terminal, local-shell, and SSH tests on Linux, macOS,
  and Windows, including the real PTY integration cases.

### Verification

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace
python scripts/check-doc-paths.py
python scripts/check-english.py
python scripts/verify-dependency-graph.py
```
