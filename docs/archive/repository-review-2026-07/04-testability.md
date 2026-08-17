# Testability review — 6.5/10

## Evidence summary

`cargo test --workspace` passed **426 tests with 2 ignored**. Test density is strongest in `terminal` (~183 test attributes), `terminal-view` (~80), `highlight` (~67), `local-shell` (~25), and `ssh` (~24). It is weakest in `app` (0), `settings-ui` (0), `theme` (0), `actions` (0), `workspace` (~1), and `sftp-ui` (~1). Counts are approximate source-level attributes; the cargo result is authoritative for executed tests.

## What is working

- Pure engine functions have broad focused tests: encoding, OSC parsing, security policy, URL policy, search, and highlighting.
- Backend lifecycle and security tests include real loopback SSH host-key handshakes (`crates/ssh/src/handler.rs:306-370`), PTY lifecycle tests, SFTP path traversal checks, and cancellation-map cleanup.
- Persistence APIs provide explicit-path variants so tests never touch real user configuration (`crates/settings/src/ui_config.rs:80-81`, `crates/state/src/dock_persistence.rs:60-66`).
- `FakeTerminalSession` and `SessionFactorySlot` are useful seams for deterministic feature tests (`crates/terminal/src/test_support.rs`, `crates/terminal/src/factory.rs:48-83`).

## Findings

### TEST-01 — High: CI does not enforce the repository's own quality gate

**Files/modules:** `.github/workflows/ci.yml:13-44`, `AGENTS.md:138-148`.

**Explanation:** The documented mandatory gate is format check, full-workspace clippy, and full-workspace build. CI's cross-platform job runs only:

```text
cargo test -p oneterm-core -p oneterm-terminal -p oneterm-local-shell -p oneterm-ssh
```

It does not run `cargo fmt --check`, clippy, full workspace build, or tests for app/UI/workspace/settings/state/theme/highlight crates.

**Why it matters:** A pull request can merge with UI compile failures, lint regressions, broken feature wiring, or failing non-backend tests even though local policy says those failures are blockers.

**Recommended solution:** Add a Linux full-workspace quality job and retain the portable backend matrix. If Linux GPUI dependencies make the job expensive, cache Cargo and split `fmt`, `clippy/build`, and tests while preserving required coverage.

```yaml
workspace-quality:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@<pinned-sha>
    - uses: dtolnay/rust-toolchain@<pinned-sha>
      with: { components: rustfmt, clippy }
    - run: cargo fmt --all -- --check
    - run: cargo clippy --workspace --all-targets -- -D warnings
    - run: cargo build --workspace
    - run: cargo test --workspace
```

### TEST-02 — High: critical UI orchestration has little direct test coverage

**Files/modules:** `crates/app/src/init.rs`, `crates/session-ui/src/common.rs`, `crates/sftp-ui/src/transfer.rs`, `crates/sftp-ui/src/panel.rs`, `crates/workspace/src/layout/workspace`, `crates/settings-ui/src`.

**Explanation:** Core engines are well tested, but the interaction-heavy code that maps events into UI state has few tests. Examples include unknown-host-key confirmation and retry, SFTP transfer status transitions, active backend switching, settings persistence from UI actions, dock save/recovery, and app initialization order.

**Why it matters:** Most product failures in a GUI client occur between individually correct modules—task cancellation, stale entities, state mirrors, notification behavior, and shutdown ordering.

**Recommended solution:** Use `gpui::TestAppContext`, `FakeTerminalSession`, and a fake `SftpBackend` to test state transitions without pixel assertions. Prioritize:

1. connect success/failure/cancel/unknown-key retry;
2. SFTP start/progress/cancel/error/completion while switching tabs;
3. terminal close cancels event/blink tasks and removes agent state;
4. settings mutation persists or exposes write failure;
5. dock corruption quarantine and default recovery.

### TEST-03 — Medium: overload and backpressure behavior is only partially exercised

**Files/modules:** `crates/ssh/src/listener.rs` queue tests, `crates/local-shell/src/listener.rs`, `crates/ssh/src/session.rs:153-157`, `crates/local-shell/src/event_loop.rs:156-249`.

**Explanation:** Listener tests verify bounded test transports and counters, but production SSH/local command queues are unbounded. There are no end-to-end tests for stalled writes plus large paste, sustained OSC responses, event queue saturation, or graceful shutdown while queues are full.

**Why it matters:** Unit tests around test-only bounded transports do not validate production overload semantics.

**Recommended solution:** Make queue capacity injectable, use the same bounded implementation in tests and production, then test FIFO input, resize coalescing, close priority, paste rejection/backpressure, and lifecycle event delivery under saturation.

### TEST-04 — Medium: persistence concurrency tests are thread-local only

**Files/modules:** `crates/core/src/persistence.rs:204-229`.

**Explanation:** The concurrent update test uses eight threads in one process, which all share `FILE_LOCKS`. It does not test two processes or a crash between backup and replace.

**Why it matters:** The implementation's documented limitation is exactly the untested boundary.

**Recommended solution:** Add a subprocess integration test after introducing an OS-level lock. Add fault-injection seams around temp write, backup, replacement, and parent sync to verify recovery paths.
