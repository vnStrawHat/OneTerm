# OneTerm repository-wide engineering review

> **Status (2026-08): historical — archived.** Superseded by [`docs/review-refresh-2026-08/`](../../review-refresh-2026-08/README.md), the live review checklist. Kept for rationale only; paths and line numbers refer to revision `4dc9bfd` (2026-07-23).

**Review date:** 2026-07-23  
**Reviewed revision:** `4dc9bfd` (`test(testability): add isolated backend and persistence coverage`)  
**Scope:** all tracked OneTerm workspace source under `crates/`, root/workspace manifests, `scripts/`, and `.github/workflows/`. The review used architecture documents for intended behavior but did not treat them as proof that the implementation is correct. Per request, `docs/review/` was excluded. Ignored research sources under `reference/` and vendored upstream implementation under `vendor/` were not scored as OneTerm-owned product code; the vendor verification mechanism was reviewed.

## Method and validation

The review combined repository-wide searches, crate/dependency inspection, targeted source reads across every layer, file-size/test-distribution analysis, and execution of project gates.

Validated on this revision:

- `python scripts/verify-dependency-graph.py` — passed for 16 packages.
- `python scripts/check-ui-fork.py` — covered by CI design inspection; the tracked hash baseline mechanism was reviewed.
- `python scripts/check-doc-paths.py` — passed.
- `python scripts/check-english.py` — passed, but this report identifies a scope defect in that checker.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed with no issues.
- `cargo build --workspace` — passed.
- `cargo test --workspace` — **426 passed, 2 ignored, 0 failed** across 33 suites.

Passing gates are evidence of baseline discipline, not proof of runtime correctness or complete coverage.

## Scorecard

| Category | Score | Summary |
|---|---:|---|
| [Readability](01-readability.md) | **7.0/10** | Strong module naming and documentation, offset by 25 Rust files over the project's ~400-line limit and several state-heavy UI modules. |
| [Simplicity](02-simplicity.md) | **6.0/10** | Useful abstractions exist, but the broad terminal façade, dual service-locator mechanisms, mirrored UI state, and custom parsers create avoidable indirection. |
| [Maintainability](03-maintainability.md) | **6.5/10** | Crate ownership and policy automation are strong; hidden runtime dependencies, synchronous persistence, and oversized modules increase change cost. |
| [Testability](04-testability.md) | **6.5/10** | Engine/backend tests are extensive, but CI omits required full-workspace gates and several UI/wiring crates have almost no direct tests. |
| [Reliability](05-reliability.md) | **6.0/10** | Atomic replacement, cancellation, and lifecycle handling are thoughtful; process-local persistence locking, stringly errors, lossy event delivery, and an avoidable runtime `unwrap` remain. |
| [Performance](06-performance.md) | **6.5/10** | Hot paths are intentionally profiled and damage-aware; unbounded queues, periodic full SFTP-state cloning, and lock-heavy full-grid snapshots are scaling risks. |
| [Security](07-security.md) | **7.5/10** | Host-key verification, secret zeroization, URL/OSC/SFTP defenses, and bounded traversal are major strengths; security policy wiring and release supply-chain hardening need work. |
| [Scalability](08-scalability.md) | **5.5/10** | Suitable for a desktop client with modest session counts; process globals, one active SFTP panel, two SSH runtime workers, and one thread per local session constrain larger workloads/multi-window evolution. |
| [Consistency](09-consistency.md) | **5.5/10** | Naming and crate rules are consistent, but contributor-language enforcement misses release scripts and documented SSH-agent support contradicts the implementation. |
| [Evolvability](10-evolvability.md) | **6.5/10** | Layering and typed persistence ownership help; global registries, a cross-feature exception, broad contracts, and unversioned schemas increase future migration cost. |
| [Architecture](11-architecture.md) | **7.5/10** | The layered DAG and app-only composition are sound. Runtime service location and active-session global state are the principal architectural debt. |

**Overall engineering assessment: 6.5/10.** The repository is substantially better structured and more security-conscious than a typical early-stage desktop terminal client. The largest gap is not basic code quality; it is the mismatch between the strong written architecture and a few runtime mechanisms—persistence, event delivery, global state, and CI—that do not yet enforce the same guarantees.

## Top 10 issues, ordered by engineering impact

1. **CI does not run the repository's mandatory full-workspace format, clippy, build, and test gates.** `.github/workflows/ci.yml:32-44` tests only four backend/engine crates. UI, app wiring, settings UI, workspace shell, SFTP UI, and feature combinations can regress on a pull request. See [TEST-01](04-testability.md#test-01-high-ci-does-not-enforce-the-repositorys-own-quality-gate).
2. **Persistence blocks the UI thread and is only serialized within one process.** UI mutation methods call `save()` directly, while `atomic_write` performs directory creation, writes, `sync_all`, copies, and rename under process-local mutexes. This risks visible stalls and lost updates when two OneTerm processes run. See [REL-01](05-reliability.md#rel-01-high-persistence-guarantees-stop-at-the-process-boundary-and-several-ui-paths-block-on-disk-io).
3. **Terminal/control queues are unbounded.** SSH uses `async_channel::unbounded::<Cmd>()`; local shell uses `mpsc::channel()` and an unbounded `VecDeque`. A stalled transport plus a large paste or terminal-generated response stream can grow memory without limit. See [PERF-01](06-performance.md#perf-01-high-terminal-command-queues-have-no-memory-bound).
4. **The event queue intentionally drops more than repaint notifications.** `forward` uses `try_send` for title, clipboard, CWD, progress, bell, notification, and agent events as well as coalescible `Output`. State caches mitigate some losses, but clipboard/progress/agent transitions can disappear. See [REL-03](05-reliability.md#rel-03-medium-the-bounded-session-event-queue-drops-non-coalescible-events).
5. **Cancellation is represented as the string `"cancelled"`.** The backend constructs `AppError::Other("cancelled")` and the UI branches on `e.to_string() == "cancelled"`. Message changes break behavior silently. See [REL-02](05-reliability.md#rel-02-high-transfer-cancellation-is-a-string-protocol).
6. **The OSC 52 security setting has split-brain policy.** Settings/UI can enable clipboard reads, but the SSH listener owns a separate default policy that always rejects remote reads. This is safe-by-default but makes the setting misleading and creates two policy authorities. See [SEC-02](07-security.md#sec-02-medium-clipboard-security-policy-is-split-between-the-backend-and-ui).
7. **Documented SSH-agent support is not implemented.** README and project roadmap claim agent authentication, while `SshAuthMethod::Agent` returns a roadmap error. See [CONS-02](09-consistency.md#cons-02-high-ssh-agent-authentication-is-advertised-but-explicitly-unimplemented).
8. **SFTP panel state is cloned every 500 ms.** The follow timer clones the entire entry list and transfer list even when nothing changed. Large remote directories create persistent allocation and UI-thread work. See [PERF-02](06-performance.md#perf-02-medium-the-sftp-panel-clones-its-complete-browser-state-twice-per-second).
9. **A stale SFTP selection can panic.** `do_download` obtains a selected entry, then calls `self.sftp.clone().unwrap()` despite the backend being optional and asynchronously replaced. See [REL-04](05-reliability.md#rel-04-medium-sftp-download-contains-an-avoidable-production-unwrap).
10. **Repository consistency automation misses tracked release scripts.** `check-english.py` excludes `.sh` and `.ps1`, while both release scripts contain non-English contributor text in direct conflict with the zero-exception rule. See [CONS-01](09-consistency.md#cons-01-medium-the-english-only-checker-does-not-scan-the-release-scripts).

## Top 10 strengths

1. **Machine-verified dependency boundaries.** `scripts/verify-dependency-graph.py` validates membership, exact internal dependencies, app-only backend ownership, shell independence, and cross-feature rules.
2. **Clear layered architecture.** Domain/engine/shared/shell/feature/backend/wiring responsibilities are explicit, and only `oneterm-app` composes all layers.
3. **Fail-closed SSH host-key handling.** Unknown keys require explicit fingerprint confirmation; changed keys are always rejected; loopback tests validate persistence and strict reconnect behavior.
4. **Credential hygiene.** `SecretString` zeroizes on drop, masks `Debug`, avoids serialization, and authentication material is removed from the long-lived config after use.
5. **Strong SFTP path defenses.** Remote names reject separators, traversal, reserved Windows names, symlinks, excessive depth, and excessive entry counts; downloads use temporary files and finalization.
6. **Central terminal security policy.** OSC-controlled titles, notifications, clipboard data, and CWD values are bounded/sanitized; remote clipboard operations default off.
7. **Safe paste and URL handling.** Bracketed-paste markers are stripped and payload size is capped; URL opening is scheme-restricted and credential-bearing authorities are rejected.
8. **Thoughtful terminal hot-path design.** Rendering consumes damage exactly once, auxiliary queries avoid consuming damage, and diagnostics are feature-gated.
9. **Persistence recovery mechanics.** Atomic same-directory replacement, backups, quarantine, typed dock ownership, and focused concurrent-update tests materially reduce corruption risk.
10. **Large automated test baseline.** 426 passing tests cover terminal encoding/parsing, highlighting, transport lifecycle, host-key behavior, persistence recovery, path traversal, and backend contracts.

## Biggest architectural risks

- **Runtime service location hides required dependencies.** `SessionFactory` is a process `OnceLock`; workspace commands are a GPUI global of function pointers. Compile-time DAG enforcement is excellent, but missing registration and multi-context behavior remain runtime concerns.
- **Global active-session state assumes one workspace focus.** `AppState.active_sftp`, `active_cwd_source`, and a singleton SFTP panel do not naturally support multiple windows or side-by-side SFTP contexts.
- **Transport and UI backpressure are not modeled end-to-end.** Unbounded command queues and lossy event queues make overload behavior implicit rather than a deliberate service-level contract.
- **Persistence correctness assumes one process.** The fixed `.bak` file and process-local lock map are insufficient for concurrent application instances.
- **The terminal façade is a high-churn integration point.** `TerminalSession` mixes rendering, input, mouse, search, lifecycle, clipboard, metrics, and SFTP access, so unrelated features share one large contract.

## Highest-priority improvements

1. Add full-workspace `fmt`, `clippy`, `build`, and `test` jobs to PR CI; keep the existing cross-platform backend matrix.
2. Introduce a typed `AppError::Cancelled`/`TransferError` and remove string comparisons.
3. Bound terminal command queues by bytes/messages and define backpressure behavior for keystrokes, paste, generated responses, resize, and close.
4. Split coalescible repaint notifications from reliable control/lifecycle events.
5. Move settings/session/layout persistence off interactive handlers and add an inter-process lock or single-instance policy.
6. Make clipboard policy a session construction input, with one authoritative read/write decision per session type.
7. Either implement SSH-agent auth with tests or remove it from public claims and UI/domain options until ready.

## Quick wins

- Replace `self.sftp.clone().unwrap()` with an early-return plus user notification.
- Add `.sh` and `.ps1` to `scripts/check-english.py`, then translate the two release scripts.
- Add `fmt`, `clippy`, and full `cargo test --workspace` steps to `ci.yml`.
- Add `Cancelled` to `AppError` and update the two UI comparisons.
- Replace the SFTP upload discovery 1 ms polling loop with a blocking channel send or a cancellation-aware condition.
- Save SFTP browser snapshots only on actual mutations/tab switches rather than every follow tick.
- Pin third-party GitHub Actions to immutable commit SHAs.

## Long-term improvements

- Introduce a window/workspace-scoped service container rather than process-global registries for UI-facing services.
- Split `TerminalSession` into stable capability traits (`TerminalRender`, `TerminalInput`, `TerminalLifecycle`, optional `SftpProvider`) behind a small façade.
- Implement backpressure and overload tests that simulate stalled transports and sustained terminal output.
- Add a persistence coordinator with inter-process locking, monotonic revisions, and schema-version migrations.
- Build a UI integration-test harness around fake sessions/SFTP backends for connection, tab switching, transfer cancellation, settings persistence, and layout restoration.
- Decide and document a target scale (for example, 20 SSH sessions, 10 local PTYs, 100k-entry SFTP trees), then benchmark against it before changing runtime topology.

## Report index

- [Readability](01-readability.md)
- [Simplicity](02-simplicity.md)
- [Maintainability](03-maintainability.md)
- [Testability](04-testability.md)
- [Reliability](05-reliability.md)
- [Performance](06-performance.md)
- [Security](07-security.md)
- [Scalability](08-scalability.md)
- [Consistency](09-consistency.md)
- [Evolvability](10-evolvability.md)
- [Architecture](11-architecture.md)
