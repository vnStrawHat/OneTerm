# OneTerm remediation plan

**Source review:** [`README.md`](README.md)  
**Planning model:** checklist grouped by review category  
**Priority:**

- **P0 — Immediate:** correctness, security, release, or data-loss risk.
- **P1 — Near term:** significant reliability, testability, maintainability, or scaling risk.
- **P2 — Planned:** structural improvement with lower immediate risk.

Items are ordered within each category by priority. Cross-category dependencies are called out so implementation can be sequenced without creating temporary inconsistencies.

## Recommended execution order

- [x] **Phase 1 — Protect the delivery path:** add the full-workspace CI gate; fix the production `unwrap`; correct English-check coverage; resolve the SSH-agent documentation mismatch; pin release actions.
  - **Completed:** all Phase 1 implementation and validation tasks are complete; the final workspace gate passes with 427 tests passed and 2 ignored.
- [ ] **Phase 2 — Make failure behavior typed and bounded:** introduce typed cancellation/errors; define event delivery semantics; bound terminal command queues; add overload tests.
- [ ] **Phase 3 — Make persistence and state safe:** move persistence off UI handlers; choose single-instance or inter-process locking; scope active workspace state; add subprocess/fault-injection tests.
- [ ] **Phase 4 — Reduce structural cost:** split oversized modules; remove periodic SFTP snapshot cloning; narrow terminal capabilities; consolidate service registration.
- [ ] **Phase 5 — Measure and evolve:** define supported scale targets, add benchmarks and UI integration coverage, introduce schema migrations, and upstream/remove the UI fork where possible.

---

## 1. Readability

- [ ] **P1 — Split oversized SFTP modules.**
  - **Targets:** `crates/ssh/src/sftp_task.rs`, `crates/ssh/src/sftp_transfer.rs`.
  - Extract command dispatch, transfer registry, path policy, recursive deletion, metadata conversion, and traversal planning into focused modules.
  - Keep the public orchestration function small and preserve existing backend contracts.
  - **Done when:** each extracted module has one primary responsibility, public behavior is unchanged, and targeted SFTP tests remain green.

- [ ] **P1 — Split terminal-view orchestration from rendering.**
  - **Targets:** `crates/terminal-view/src/view/mod.rs`, `crates/terminal-view/src/element/prepaint.rs`.
  - Separate lifecycle/event subscription, view state, clipboard/agent integration, and render preparation.
  - **Done when:** rendering code no longer owns unrelated session lifecycle concerns and all terminal-view tests pass.

- [ ] **P2 — Rewrite module comments around current contracts.**
  - Replace historical phase/migration commentary with concise sections for ownership, inputs, cancellation, failure behavior, and security invariants.
  - Move durable history to architecture/design documentation.
  - **Done when:** comments describe current behavior and links point to maintained design documents.

- [ ] **P2 — Make state authority explicit.**
  - Document whether `AppState`, `SftpBrowserStore`, panel state, or terminal state is authoritative for each value.
  - Introduce named snapshot/projection types where mirrors are unavoidable.
  - **Done when:** every mirrored field has one documented owner and synchronization tests cover transitions.

## 2. Simplicity

- [ ] **P1 — Define a narrower terminal capability model.**
  - **Target:** `crates/terminal/src/session.rs`.
  - Split optional concerns into focused capabilities such as rendering, input, search, lifecycle, CWD, SFTP, and metrics.
  - Retain a compatibility façade during migration if required.
  - **Dependency:** coordinate with architecture/service-registration work before changing feature constructors.
  - **Done when:** feature crates depend only on the capabilities they use and fakes do not implement unrelated methods.

- [ ] **P1 — Replace or formally constrain custom URL parsing.**
  - **Target:** `crates/terminal/src/url_policy.rs`.
  - Prefer a mature URL parser if allowed by dependency policy; otherwise document and enforce a narrow grammar.
  - Add tests for IPv6, userinfo, malformed authorities, empty hosts, encoded delimiters, backslashes, and non-default ports.
  - **Done when:** parsing and policy decisions are independently tested and edge cases have an explicit expected result.

- [ ] **P2 — Consolidate persistence mechanics without moving schema ownership.**
  - **Targets:** `crates/core/src/persistence.rs`, settings/session/layout persistence modules.
  - Create a small generic document-store helper for load/default/quarantine/save mechanics.
  - Keep migrations and schema definitions in their owning crates.
  - **Done when:** recovery behavior is consistent and schema ownership remains visible at call sites.

- [ ] **P2 — Reduce runtime service indirection.**
  - Replace implicit global lookups with explicit service handles during feature/window initialization.
  - Add a single startup validation for required registrations.
  - **Done when:** missing registration produces one actionable startup error and tests can install isolated service sets.

## 3. Maintainability

- [x] **P0 — Add automated enforcement for the documented quality gate.**
  - **Target:** `.github/workflows/ci.yml`.
  - Add PR-required jobs for `cargo fmt --all -- --check`, workspace clippy with `-D warnings`, workspace build, and workspace tests.
  - Retain the cross-platform backend test matrix.
  - **Done when:** a pull request cannot pass without the full documented gate.
  - **Completed:** `.github/workflows/ci.yml` now runs format, full-workspace clippy, build, and test commands on Ubuntu while retaining the three-platform backend matrix.

- [ ] **P1 — Separate model mutation from persistence scheduling.**
  - **Targets:** `crates/session-ui/src/session_state.rs`, `crates/settings/src/ui_config.rs`, workspace layout persistence.
  - Make mutations return changes/results; let a coordinator debounce and persist immutable snapshots asynchronously.
  - Add dirty/retry state and user-visible failure notification.
  - **Dependency:** coordinate with the reliability persistence decision.
  - **Done when:** interactive handlers do not perform blocking disk I/O and failed saves remain observable/retryable.

- [ ] **P1 — Replace broad `AppError::Other(String)` categories.**
  - **Target:** `crates/core/src/error.rs` and backend conversion sites.
  - Add stable variants for cancellation, validation, permission, timeout, connection, authentication, closed session, and persistence failures as appropriate.
  - Preserve source errors and contextual messages.
  - **Done when:** UI recovery logic matches typed variants rather than message text.

- [ ] **P2 — Align the file-size policy with engineering practice.**
  - Either implement a measured size/lint check with an allowlist or rewrite the guideline as a responsibility-based review heuristic.
  - **Done when:** the documented rule is either automatically enforceable or explicitly presented as a non-blocking guideline.

- [ ] **P2 — Remove the feature-layer dependency exception.**
  - **Target:** `session-ui → terminal-view`.
  - Introduce an app-installed typed “open terminal panel” request/service.
  - **Done when:** session-ui no longer imports terminal-view and app remains the composition root.

## 4. Testability

- [ ] **P0 — Add missing UI/app/workspace test coverage to CI.**
  - Cover `app`, `workspace`, `settings-ui`, `sftp-ui`, `theme`, `settings`, and state wiring through workspace tests.
  - Use GPUI test contexts and fakes instead of pixel-level tests for state transitions.
  - **Done when:** all workspace crates are compiled and tested in CI and critical interaction paths have deterministic tests.

- [ ] **P1 — Add connection-flow integration tests.**
  - Cover success, authentication failure, timeout, cancellation, unknown-host-key confirmation, changed-host-key rejection, retry, and close-before-connect-completes.
  - **Targets:** `crates/session-ui`, `crates/app`, `crates/ssh` test support.
  - **Done when:** each outcome asserts session state, notifications, cleanup, and persistence effects.

- [ ] **P1 — Add SFTP UI state-machine tests.**
  - Cover backend switching, stale selections, list refresh, upload/download progress, cancellation, failure, completion, tab switching, and transfer cleanup.
  - Use a fake SFTP backend with controllable delays and failures.
  - **Done when:** no test requires a live SSH server and every transfer terminal state is asserted.

- [ ] **P1 — Add overload/backpressure tests.**
  - Inject queue capacities and simulate stalled SSH/PTY writers, large paste, sustained output, event saturation, and close under pressure.
  - Assert ordering, coalescing, memory bounds, close priority, and error reporting.
  - **Dependency:** define queue semantics first under Reliability/Performance.
  - **Done when:** production and test queues use the same bounded implementation.

- [ ] **P1 — Expand persistence fault tests.**
  - Add subprocess tests for concurrent writers after an inter-process policy is selected.
  - Add fault injection for temp-file write, flush, backup, rename, and parent-directory sync failures.
  - **Done when:** recovery guarantees are tested at the same concurrency boundary that production claims.

- [ ] **P2 — Add lifecycle leak tests.**
  - Assert event/blink/transfer tasks stop after terminal close and agent/session registrations are removed.
  - **Done when:** repeated open/close cycles do not retain entities, channels, or task registrations.

## 5. Reliability

- [x] **P0 — Replace string-based cancellation handling.**
  - Add `AppError::Cancelled` and update `crates/ssh/src/sftp_transfer.rs` and `crates/sftp-ui/src/transfer.rs`.
  - Add regression tests proving contextual error messages do not change cancellation classification.
  - **Done when:** no production code compares `to_string()` for control flow.
  - **Completed:** cancellation producers now return `AppError::Cancelled`; SFTP upload/download UI paths pattern-match the typed variant, and backend regression tests assert the variant directly.

- [x] **P0 — Remove the SFTP download `unwrap`.**
  - **Target:** `crates/sftp-ui/src/transfer.rs`.
  - Atomically capture selected entry and active backend; show a recoverable notification when either is unavailable.
  - Clear selections when the backend changes.
  - **Done when:** stale backend/selection tests prove the action cannot panic.
  - **Completed:** `do_download` now returns a warning notification when no backend is active, and a GPUI regression test covers a stale selection with no backend.

- [ ] **P1 — Decide the multi-process persistence policy.**
  - Choose either single-instance enforcement or OS-level advisory locking with revision-aware writes.
  - Document the guarantee in `docs/agents/persistence.md` and all persistence API comments.
  - **Done when:** concurrent behavior is explicit and covered by integration tests.

- [ ] **P1 — Move persistence off interactive handlers.**
  - Add background snapshot writes, debounce, dirty state, retry, and user-visible errors.
  - **Done when:** settings, sessions, and layout remain responsive during slow filesystem operations.

- [ ] **P1 — Separate reliable and lossy session events.**
  - Make output/repaint notifications coalescible, while clipboard, lifecycle, progress completion, and agent transitions use reliable or latest-value delivery.
  - Add explicit overflow metrics/logging.
  - **Done when:** no non-coalescible event is silently discarded.

- [ ] **P1 — Bound local-shell shutdown.**
  - Replace unconditional UI-thread joins with asynchronous completion and a deadline.
  - Report a stuck owner thread and continue shutdown safely.
  - **Done when:** close/quit has a bounded latency under PTY failure simulation.

## 6. Performance

- [ ] **P0 — Bound terminal command queues by bytes and messages.**
  - Apply to SSH command channels and local-shell write buffers.
  - Coalesce resize, prioritize close, preserve input ordering, and reject/await when paste budget is exhausted.
  - **Done when:** queue memory has a documented upper bound and overload tests pass.

- [ ] **P1 — Remove periodic full SFTP snapshot cloning.**
  - **Targets:** `crates/sftp-ui/src/panel.rs`, `crates/sftp-ui/src/browser_state.rs`.
  - Track dirty generations and persist only after changes/tab transitions; use immutable shared entries if snapshots remain necessary.
  - **Done when:** idle panels perform no O(entry-count) copies.

- [ ] **P1 — Measure terminal snapshot and lock hold latency.**
  - Use existing diagnostics for snapshot time, parse lock hold, frame latency, and throughput.
  - Establish p95/p99 targets for representative grid sizes and concurrent panes.
  - **Done when:** benchmark results are recorded and regressions are detectable.

- [ ] **P2 — Introduce a bounded parse/yield budget only if measurements justify it.**
  - **Target:** `crates/local-shell/src/event_loop.rs`.
  - Preserve throughput while limiting UI starvation under sustained PTY output.
  - **Done when:** latency improves without regressing throughput benchmarks.

- [ ] **P2 — Replace 1 ms SFTP producer polling.**
  - Use a blocking or cancellation-aware channel send instead of repeated `try_send` plus sleep.
  - **Done when:** backpressure has lower wakeup overhead and cancellation remains prompt.

## 7. Security

- [x] **P0 — Pin third-party GitHub Actions to immutable SHAs.**
  - **Targets:** `.github/workflows/ci.yml`, `.github/workflows/release.yml`.
  - Add automated dependency-update support and review action permission scopes.
  - **Done when:** no third-party action uses a mutable tag and release permissions are minimized.
  - **Completed:** all CI/release actions are pinned to verified 40-character SHAs, the release build job no longer has `actions: write`, and Dependabot tracks GitHub Actions updates.

- [x] **P0 — Resolve SSH-agent capability mismatch.**
  - Either implement agent authentication with platform coverage and tests, or remove it from the public enum/UI/docs until implemented.
  - **Done when:** README, roadmap, domain options, UI behavior, and backend behavior agree.
  - **Completed:** the unimplemented enum/backend branch was removed; README and the roadmap now list only supported methods and retain SSH-agent authentication as a future item.

- [ ] **P1 — Make clipboard policy a per-session construction input.**
  - Build one validated `TerminalSecurityPolicy` from settings and pass it into SSH/local listener construction.
  - Keep UI checks as defense in depth, not as a second policy authority.
  - **Done when:** local and remote clipboard read/write behavior is covered for enabled and disabled policy combinations.

- [ ] **P1 — Harden URL target policy tests or adopt a mature parser.**
  - Add adversarial authority/IPv6/encoding tests before changing accepted behavior.
  - **Done when:** every supported target form has an explicit security-policy test.

- [ ] **P2 — Add release integrity metadata.**
  - Publish checksums and artifact provenance/attestations; add platform signing/notarization where practical.
  - **Done when:** users can verify artifact integrity and provenance from release assets.

## 8. Scalability

- [ ] **P1 — Define supported desktop scale targets.**
  - Specify target concurrent SSH sessions, local PTYs, visible panes, transfer count, directory size, grid sizes, and shutdown latency.
  - **Done when:** targets are documented and represented in repeatable benchmarks.

- [ ] **P1 — Scope active session state to a workspace/window.**
  - Replace process-global active SFTP/CWD/local flags with `WorkspaceState` owned by each window.
  - Keep theme and durable settings process-wide only where appropriate.
  - **Dependency:** coordinate with Architecture and Evolvability service-scope work.
  - **Done when:** two windows/workspaces can maintain independent active sessions in tests.

- [ ] **P1 — Remove O(n) SFTP state copying.**
  - Use generation-tagged state, immutable `Arc` collections, or authoritative store state.
  - **Done when:** directory size affects work only when entries actually change.

- [ ] **P2 — Benchmark SSH worker capacity.**
  - Measure two-worker runtime behavior under concurrent sessions/transfers before changing topology.
  - Adjust worker count using available parallelism only when saturation is demonstrated.
  - **Done when:** worker sizing is evidence-based and documented.

- [ ] **P2 — Benchmark local-shell thread scaling.**
  - Measure idle memory, sustained output, and close latency at representative session counts.
  - Consider a shared poller only if the current per-session thread model misses targets.
  - **Done when:** the chosen model has a documented capacity rationale.

## 9. Consistency

- [x] **P0 — Extend English-only enforcement to release scripts.**
  - **Target:** `scripts/check-english.py`.
  - Include `.sh` and `.ps1`, translate existing non-English comments/messages, and add checker fixtures.
  - **Done when:** the checker detects violations in every tracked source/documentation suffix covered by policy.
  - **Completed:** the checker now scans `.sh`, `.ps1`, `README.md`, and all `docs/`; release-script text is English; two regression tests run in CI.

- [x] **P0 — Align SSH-agent documentation and implementation.**
  - Coordinate with Security CONS-02 work.
  - **Done when:** no documentation claims unsupported authentication behavior.
  - **Completed:** public documentation now consistently identifies SSH-agent authentication as unsupported roadmap work.

- [ ] **P1 — Make ignored persistence results observable.**
  - Replace bare `_ = save_state(...)` with logging/notification or a documented best-effort helper.
  - Include operation and path context without leaking secrets.
  - **Done when:** every user-visible persistence failure has an observable recovery path.

- [ ] **P2 — Align the module-size rule with enforcement.**
  - Coordinate with Maintainability MAINT-04.
  - **Done when:** contributors have one consistent interpretation of the rule.

## 10. Evolvability

- [ ] **P1 — Replace process-global service registration with scoped service bundles.**
  - Introduce app/window/workspace service handles for session creation and workspace commands.
  - Keep backend construction in the app composition root.
  - Add startup validation and isolated test installation.
  - **Done when:** two independent test/application contexts can use different service configurations.

- [ ] **P1 — Introduce schema migration infrastructure before the first breaking change.**
  - Define version constants, migration functions, legacy fixtures, current-schema serialization, and idempotence tests.
  - Apply to UI config, terminal config, session store, and dock documents as needed.
  - **Done when:** a legacy fixture can be upgraded deterministically and safely.

- [ ] **P1 — Remove the feature-to-feature composition exception.**
  - Route terminal-panel creation through an app-installed typed request/service.
  - **Done when:** `session-ui` no longer depends directly on `terminal-view`.

- [ ] **P1 — Replace error strings with stable categories.**
  - Coordinate with Maintainability and Reliability error work.
  - **Done when:** adding context/localization cannot change retry/cancel/recovery behavior.

- [ ] **P2 — Track and upstream the vendored UI patch.**
  - Open/maintain an upstream issue or PR for the required tab activation API.
  - Keep the hash baseline check until the upstream fix is available.
  - Remove the fork patch after upgrading to the upstream implementation.
  - **Done when:** the application no longer carries the custom vendor patch.

## 11. Architecture

- [ ] **P1 — Establish an explicit app/workspace service boundary.**
  - Define which dependencies are process-wide, app-wide, window-wide, and workspace-wide.
  - Replace hidden global lookups for UI-facing services with typed handles.
  - **Done when:** the architecture document and constructors agree on scope and initialization order.

- [ ] **P1 — Make active-terminal and SFTP state window-scoped.**
  - Introduce per-workspace state and pass it to panels/views.
  - Add a multi-window or two-context test harness.
  - **Done when:** active state cannot leak between workspaces.

- [ ] **P1 — Publish a transport backpressure contract.**
  - Specify capacity, byte budgets, ordering, coalescing, priorities, cancellation, and close behavior for terminal commands and events.
  - Encode the contract in shared adapters and tests.
  - **Done when:** channel implementation changes cannot silently alter overload semantics.

- [ ] **P1 — Correct persistence architecture guarantees.**
  - State whether persistence is single-instance or inter-process safe, then implement and test that guarantee.
  - Add revision/concurrency semantics if multiple writers are supported.
  - **Done when:** documentation, APIs, and subprocess tests agree.

- [ ] **P2 — Move session-panel composition behind an application service.**
  - Replace the `session-ui → terminal-view` exception with a typed open-panel request.
  - **Done when:** the app remains the only feature composition point.

- [ ] **P2 — Split the broad terminal contract by capability.**
  - Coordinate with Simplicity SIMP-01 and preserve backend-neutrality.
  - **Done when:** new optional capabilities can be added without expanding every session implementation and fake.

## Completion criteria for the plan

- [ ] All P0 items are complete and covered by CI or an explicit security/release check.
- [ ] P1 items have an owner, issue, target milestone, and regression test or benchmark.
- [ ] No production control flow relies on error-message string matching.
- [ ] No production terminal command queue is unbounded without an explicit documented exception.
- [ ] Persistence guarantees are accurate for both single-process and multi-process behavior.
- [ ] UI/app/workspace orchestration has deterministic state-transition coverage.
- [ ] The review scorecard is re-evaluated after P0/P1 completion.
