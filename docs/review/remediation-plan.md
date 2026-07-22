# OneTerm Remediation Plan and Execution Checklist

This plan converts the repository review into actionable work. It is intentionally ordered by risk and dependency rather than by category score alone.

## How to use this plan

- Treat every unchecked item as **not started** until the implementation, tests, and verification evidence exist.
- Assign an owner and issue number before starting a workstream.
- Keep changes small enough to review independently; do not combine security fixes with broad refactors unless the refactor is required for the fix.
- Update the status, evidence links, and date in this document as work lands.
- All implementation and documentation changes must remain English-only.

## Status legend

- `[ ]` Not started
- `[-]` In progress
- `[x]` Complete
- `[!]` Blocked or requires a design decision

## Priority and dependency map

| Priority | Outcome | Depends on | Suggested milestone |
|---|---|---|---|
| P0 | Prevent credential interception and unsafe file writes | None; can start immediately | Security release blocker |
| P0 | Stop silent terminal input loss and make connection attempts cancellable | Transport API decision | Reliability baseline |
| P1 | Remove blocking SFTP work from UI callbacks | Async SFTP boundary decision | Responsiveness release |
| P1 | Make persistence atomic, serialized, versioned, and testable | Persistence ownership decision | Data-safety release |
| P1 | Reconcile the real and documented crate graph | Architecture decision on `oneterm-ui` | Architecture baseline |
| P2 | Remove retained state and per-session runtime scaling costs | Stable session identity and app runtime boundary | Multi-session scale |
| P2 | Reduce backend duplication and complete terminal capability migration | API migration plan | Refactor milestone |
| P3 | Improve documentation navigation and broad integration coverage | Current architecture baseline | Continuous quality |

## Release gates

The following must be complete before calling the client production-ready for ordinary SSH/SFTP networks:

- [ ] SSH host-key verification is fail-closed, tested, and enabled by default.
- [ ] SFTP local-path containment and symlink policy are enforced and tested.
- [ ] Transfers use temporary files and atomic finalization, with cleanup on failure/cancellation.
- [ ] SSH connection/auth/channel operations have deadlines and cancellation.
- [ ] Terminal input delivery has a typed failure/backpressure policy and cannot silently lose keystrokes.
- [ ] SFTP network/filesystem work cannot block the GPUI thread.
- [ ] Persistent JSON writes are atomic and serialized; corrupt files are preserved/quarantined.
- [ ] Release CI runs formatting, clippy, tests, build, dependency auditing, and artifact checksums.
- [ ] The full workspace quality gate is green on supported platforms.

---

## 1. Security

**Primary report:** [`security.md`](security.md)  
**Priority:** P0

### Work items

- [x] **SEC-01: Implement SSH host-key verification.**
  - Define a known-hosts storage abstraction and inject it into `SshClientHandler`.
  - Support matching keys, unknown keys, changed keys, host/port canonicalization, and malformed entries.
  - Reject mismatches and changed keys by default.
  - Add an explicit first-use approval flow showing algorithm and SHA-256 fingerprint.
  - Ensure any development override is default-off and unavailable in release configuration.
- [x] **SEC-02: Contain SFTP download paths.**
  - Reject path separators, rooted paths, parent components, empty names, and platform-specific prefixes.
  - Define and enforce a symlink policy.
  - Verify normalized/canonical destination containment before creating each file.
  - Add Windows and Unix path fixtures.
- [x] **SEC-03: Enforce terminal notification limits.**
  - Implement the configured rate limit and queue cap.
  - Coalesce excess notifications into a bounded summary event.
  - Add sustained-burst tests.
- [x] **SEC-04: Reduce credential lifetime in memory.**
  - Evidence: implemented in commit `35e783b` across `crates/core`, `crates/ssh`, `crates/session-ui`, `crates/local-shell`, `crates/terminal`, and `crates/terminal-view`.
  - Introduce a zeroizing secret wrapper for passwords and passphrases.
  - Avoid unnecessary credential clones.
  - Clear temporary UI credential state after dispatch.
- [ ] **SEC-05: Harden release supply chain.**
  - Pin GitHub Actions to reviewed commit SHAs.
  - Minimize job permissions.
  - Add dependency audit, SBOM, checksums, and artifact provenance/signing.

### Acceptance criteria

- [ ] A test proves unknown and changed host keys cannot connect without explicit approval.
- [ ] A malicious remote filename fixture cannot write outside the selected download root.
- [ ] Security policy tests prove notification rate and queue limits are active, not merely declared.
- [ ] Release artifacts are traceable to a reviewed workflow and source revision.

### Completed security evidence (2026-07-22)

- SEC-01 now verifies known, unknown, and changed SSH host keys fail-closed, with explicit fingerprint approval for unknown keys.
- SEC-02 validates remote names, canonicalizes the local root, rejects remote/local symlinks, and verifies destination containment.
- SEC-03 applies notification rate limiting and a bounded UI queue with drop accounting.
- SEC-04 stores passwords and passphrases in zeroizing wrappers, masks debug output, and clears authentication material after the auth boundary.
- Verification passed: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, and `cargo test --workspace` (`405 passed, 3 ignored`).
- SEC-05 remains not started and is not included in commit `35e783b`.

### Verification

```text
cargo test -p oneterm-ssh -p oneterm-terminal -p oneterm-sftp-ui
cargo clippy --workspace --all-targets -- -D warnings
``` 

---

## 2. Reliability

**Primary report:** [`reliability.md`](reliability.md)  
**Priority:** P0/P1

### Work items

- [x] **REL-01: Make terminal input delivery explicit.**
  - Change `write`, `resize`, and `close` to return typed results where loss matters.
  - Use reliable FIFO/backpressure for keystrokes.
  - Use latest-value coalescing for resize events.
  - Preserve guaranteed delivery for lifecycle events.
  - Define the UI behavior when a session is closed or saturated.
- [x] **REL-02: Add connection deadlines and cancellation.**
  - Introduce operation state and a cancellation token owned by the initiating dialog/session.
  - Add overall and phase-specific deadlines for DNS/connect/auth/channel/SFTP setup.
  - Ensure dialog closure cancels the operation and suppresses late UI updates.
- [x] **REL-03: Remove blocking SFTP calls from UI callbacks.**
  - Move rename, delete, mkdir, stat, and recursive operations to the background service path.
  - Surface progress, cancellation, failure, and completion as typed operation events.
- [x] **REL-04: Make transfer finalization atomic.**
  - Write to a same-directory temporary path.
  - Flush and rename only after successful completion.
  - Delete temporary files on cancellation/error.
  - Define overwrite, existing-file, and resume semantics.
- [x] **REL-05: Centralize persistence safety.**
  - Serialize writers per file.
  - Write sibling temporary files and atomically rename.
  - Preserve backups and quarantine invalid JSON.
- [x] **REL-06: Make recursive traversal bounded and cancellable.**
  - Replace recursive synchronous walks with iterative/bounded traversal.
  - Surface discovery errors instead of silently skipping them.
- [x] **REL-07: Own and join local PTY threads.**
  - Retain join handles.
  - Wake the owner thread and deterministically join it during shutdown.
  - Record abnormal thread exits.

### Completed reliability evidence (2026-07-22)

- REL-01 returns `TerminalError` from write, resize, and close operations; production SSH input uses FIFO delivery, while full/closed test transports expose failures.
- REL-02 gives each SSH attempt an owned cancellation handle and applies 20-second phase deadlines plus a 60-second overall deadline. Dialog cancellation suppresses stale callbacks and host-key retries use fresh handles.
- REL-03 makes the object-safe `SftpBackend` boundary asynchronous and awaits metadata and mutation operations from GPUI tasks; no `blocking_recv()` remains in the SSH/SFTP UI path.
- REL-04 writes uploads and downloads through unique same-directory temporary files, flushes/syncs before rename, preserves existing regular targets during replacement, and removes temporary files on failure or cancellation.
- REL-05 centralizes serialized same-path writes, sibling temporary files, `sync_all`, backups, JSON read-modify-write transactions, and invalid-file quarantine in `oneterm_core`. Settings, SSH sessions, workspace layout, and SFTP table state use the shared mechanics.
- REL-06 replaces recursive directory walks with iterative traversal limited to 64 levels and 100,000 entries. Local discovery runs through `spawn_blocking`, transfer discovery checks cancellation, uploads stream with a fixed 32 KiB buffer, symlinks are rejected, and completed cancellation handles are removed.
- REL-07 constructs, operates, and drops each local PTY on its named owner thread. `LocalSession` retains the join handle, sends shutdown, joins deterministically, records join panics as transport errors, and repeats cleanup from `Drop`.
- Targeted checks passed during implementation: REL-01 (294 tests), REL-02 (15 tests), REL-03 (20 tests), REL-04 (12 tests), persistence migration tests, REL-06 (13 SSH tests), and REL-07 (25 local-shell tests), with targeted clippy runs warning-free.
- Production remediation is recorded in commit `6cd8c58` (`fix(reliability): harden session and transfer lifecycles`).
- Final verification passed: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, and `cargo test --workspace` (`413 passed, 3 ignored` across 35 suites).

### Acceptance criteria

- [ ] Keystroke delivery tests prove ordering and observable failure on closed/full transports.
- [ ] A network-black-hole test completes with a bounded timeout.
- [ ] UI responsiveness tests show no blocking SFTP call on the GPUI thread.
- [ ] Failed and cancelled transfers leave no final partial file.
- [ ] Persistence interruption tests leave either the previous valid file or the complete new file.

### Verification

```text
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
``` 

---

## 3. Performance

**Primary report:** [`performance.md`](performance.md)  
**Priority:** P1/P2; profile before invasive optimization

### Work items

- [x] Stream SFTP uploads from an async file with a fixed-size reusable buffer.
- [x] Move local filesystem traversal off the SSH runtime worker and use bounded iterative remote traversal.
- [ ] Measure terminal snapshot cloning and lock-hold duration under sustained output.
- [ ] If profiling confirms a bottleneck, snapshot only damaged rows or use generation-stamped row caches.
- [ ] Profile local PTY parser lock duration and introduce a byte/time budget only if frame latency requires it.
- [ ] Reduce Agent UI refresh frequency for inactive data and avoid cloning/rebuilding unchanged cards.
- [ ] Move high-frequency diagnostics from INFO to DEBUG/TRACE or a feature flag.
- [ ] Add representative benchmarks for large terminal grids, high-output sessions, large SFTP files, and large agent lists.

### Acceptance criteria

- [ ] Large-file upload memory usage is bounded independently of file size.
- [ ] A benchmark report records p95/p99 terminal frame latency and parser lock hold time.
- [ ] Performance changes include before/after measurements and do not regress damage tracking.

### Verification

```text
cargo test --workspace
cargo build --workspace --release
# Run repository-specific benchmarks/profilers when available.
``` 

---

## 4. Scalability

**Primary report:** [`scalability.md`](scalability.md)  
**Priority:** P2

### Work items

- [x] Replace one Tokio runtime per SSH session with a process-wide bounded shared runtime.
- [x] Introduce stable session/SFTP IDs instead of `Arc` pointer identity.
- [x] Purge SFTP browser state when a backend closes.
- [x] Remove completed transfer cancellation tokens from the active map and reject duplicate active IDs.
- [x] Stream recursive upload discovery through a bounded producer/consumer channel and process
      recursive downloads incrementally without retaining a complete file plan.
- [x] Replace linear Agent registry lookup with stable composite-key indexing and linear-time grouping.
- [ ] Add Agent-list virtualization/incremental rendering. The SFTP table already uses the
      virtualized `DataTable`; Agent rendering still clones and sorts the filtered snapshot.
- [ ] Add a full multi-session soak test for opening, switching, cancelling, and closing many
      sessions. A focused concurrent runtime-reuse stress test now covers bounded runtime ownership.

### Acceptance criteria

- [x] Session count does not create one runtime worker per session.
- [x] Closed sessions release browser state, transfer tokens, tasks, and backend references.
- [ ] A soak test demonstrates bounded memory and thread growth across repeated session cycles.
- [ ] Large Agent lists remain responsive under representative data volumes.

### Implemented scalability evidence (2026-07-22)

- `crates/ssh/src/session.rs` now reuses one lazily initialized two-worker Tokio runtime; a
  concurrent stress test confirms all callers receive the same runtime instead of creating
  one scheduler per session.
- `SftpBrowserStore` tracks stable `SftpSessionId` values with weak backend references, purges
  closed/dropped backends during panel lifecycle synchronization, and refuses to recreate state
  for an untracked session. Focused tests cover cleanup.
- Recursive upload traversal now runs off-runtime and feeds a capacity-128 channel with
  cancellation-aware backpressure. Recursive download traverses and transfers incrementally.
  Existing depth, entry, symlink, destination-containment, temporary-file, and finalization
  controls remain in place.
- The SFTP task rejects duplicate active transfer IDs, removes tokens through a drop guard, tracks
  transfer and recursive-mutation tasks, and waits before aborting any shutdown stragglers.
- `AgentRegistry` uses a `(EntityId, agent_id)` index, rebuilds it after removals, and the view
  groups cards with a hash index rather than repeated linear position scans.
- Remaining optional scalability work is explicit: Agent-list virtualization and a measured
  multi-session UI/network soak harness.

### Verification

```text
cargo test --workspace
cargo build --workspace --release
# Capture thread count, heap usage, and task counts during the soak test.
``` 

---

## 5. Architecture

**Primary report:** [`architecture.md`](architecture.md)  
**Priority:** P1/P2

### Work items

- [ ] Decide whether `oneterm-ui` is permanent shared UI infrastructure or a temporary fork.
- [ ] Document its allowed dependency layer, upstream revision, patch delta, and update process.
- [ ] Remove `oneterm_ui::dock::DockPlacement` from the low-level `actions` crate; map an app-owned placement type in `workspace`.
- [ ] Migrate terminal consumers incrementally from the monolithic `TerminalSession` to implemented capability contracts, or remove the unused contracts and record the future design separately.
- [ ] Replace blocking synchronous SFTP methods with async operations or typed background operation handles.
- [ ] Consolidate process-global services behind an app-owned service registry/entity with test-scoped construction.
- [ ] Add an automated dependency allow-list check for forbidden feature/backend edges and undocumented crates.

### Acceptance criteria

- [ ] The architecture document and all manifests describe the same crate graph.
- [ ] Feature crates still compile without direct backend dependencies.
- [ ] SFTP API usage makes UI blocking impossible by construction.
- [ ] Tests can create isolated service registries without process-global ordering.

### Verification

```text
cargo tree --workspace --depth 3
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
``` 

---

## 6. Maintainability

**Primary report:** [`maintainability.md`](maintainability.md)  
**Priority:** P1/P2

### Work items

- [x] Centralize persistence mechanics while leaving domain serializers in owning crates (`oneterm_core::persistence`).
- [x] Extract shared local/SSH terminal model operations (`oneterm_terminal::TerminalModel`).
- [ ] Split `sftp_task.rs`, `agent_registry.rs`, and large dock/terminal-view files by stable responsibility.
- [x] Establish an explicit `oneterm-ui` fork maintenance and upstream synchronization process (`ui-fork-maintenance.md`, `check-ui-fork.py`).
- [x] Define a common runtime error taxonomy and review policy for ignored errors, defaults, and panics (`error-policy.md`).
- [x] Add schema migration ownership and fixture conventions (`persistence.md`).
- [x] Add a dependency-graph allow-list verification script to CI (`verify-dependency-graph.py`).

### Acceptance criteria

- [x] New persistence features use one shared atomic-write path.
- [ ] A terminal capability change does not require copy-pasted edits across local and SSH adapters.
- [ ] Large modules have clear responsibility boundaries and focused tests.
- [ ] Ignored runtime errors are justified in code comments or eliminated.

---

## 7. Testability

**Primary report:** [`testability.md`](testability.md)  
**Priority:** P0/P1/P3

### Work items

- [ ] Add a fake SFTP backend and pure path/transfer planning functions.
- [ ] Add host-key verification unit and local-server integration tests.
- [ ] Inject persistence directories and add temporary-directory tests.
- [ ] Add tests for atomic writes, corrupt-file quarantine, concurrent saves, and schema migrations.
- [ ] Add test-scoped service/factory construction without `OnceLock` pollution.
- [ ] Replace fixed sleeps in local PTY tests with event/readiness predicates.
- [ ] Add UI boundary tests for startup registration, panel lifecycle, settings round trips, and SFTP operation states.
- [ ] Add cross-platform test coverage for shell resolution, path handling, and PTY behavior.

### Acceptance criteria

- [ ] Every P0 security/reliability issue has a regression test.
- [ ] SFTP path safety and transfer cleanup are tested without a real remote server.
- [ ] Persistence tests do not write to the developer's real configuration directory.
- [ ] Test suites can run independently and in any order.

---

## 8. Readability

**Primary report:** [`readability.md`](readability.md)  
**Priority:** P2/P3

### Work items

- [x] Update stale documentation paths and clearly label historical design documents.
- [x] Add a current architecture navigation page with crate responsibility and dependency links.
- [x] Split transfer orchestration and Agent state by stable responsibility; the remaining large
      dock and terminal-view modules are explicitly retained as optional follow-up work.
- [ ] Extract named operations from deeply nested GPUI callback chains. This remains a separate
      follow-up because the scoped remediation preserved callback behavior without changing UX.
- [x] Keep lifecycle and security comments aligned with actual guarantees.
- [x] Add module-level documentation for public feature boundaries and ownership rules.

### Acceptance criteria

- [x] Every path linked from the current architecture docs exists.
- [x] A new contributor can locate backend, feature, state, and shell code from one index.
- [x] Comments do not promise controls or cleanup that the implementation does not provide.

### Completed readability evidence (2026-07-22)

- `crates/state/src/agent_model.rs` now owns folded Agent data and event application, with focused
  tests in `agent_model_tests.rs`; `agent_registry.rs` retains registry and GPUI lifecycle duties.
- `crates/ssh/src/sftp_transfer.rs` now owns bounded traversal, cancellation, streaming, temporary
  files, and finalization; `sftp_task.rs` retains command dispatch and task orchestration.
- `docs/architecture.md` is the current architecture source of truth. Historical design records
  are labeled, and `scripts/check-doc-paths.py` validates its current paths.
- Lifecycle/security comments and module ownership documentation were aligned with implementation.
- Verification passed: formatting, architecture/dependency/UI-fork policy checks, workspace
  Clippy/build/tests (`413 passed, 3 ignored` across 35 suites).
- The scoped remediation is recorded in commit `819d589` (`refactor(readability): clarify module ownership`).
- Remaining large dock/terminal-view splits and deep GPUI callback extraction are optional follow-up
  readability work, not part of the completed scoped remediation.

---

## 9. Simplicity

**Primary report:** [`simplicity.md`](simplicity.md)  
**Priority:** P2

### Work items

- [x] Remove the unused terminal contract API rather than maintaining a parallel facade.
- [x] Replace pointer-keyed SFTP state with stable typed session IDs and one authoritative store.
- [x] Replace arbitrary JSON field patching with the typed `DockDocument` persistence model.
- [x] Validate and document the two intentionally distinct service registries: the process-wide
      `SessionFactory` and the GPUI `WorkspaceCommands` registry.
- [x] Combine recursive SFTP discovery into one bounded traversal pipeline and surface
      remote-directory creation failures.
- [x] Avoid adding new defaults/compatibility methods to the monolithic session trait.

### Acceptance criteria

- [x] Each major runtime capability has one authoritative API.
- [x] Session identity and SFTP state ownership are understandable without pointer/lifetime inference.
- [x] New features require fewer special-case bridges and duplicated state updates.

### Completed simplicity evidence (2026-07-22)

- The unused `TerminalRenderer`, `TerminalInput`, and `TerminalLifecycle` traits were removed;
  `TerminalSession` remains the single terminal capability contract.
- `SftpSessionId` now owns stable per-process session identity, and SFTP browser state is keyed
  by that ID instead of an `Arc` address.
- `oneterm_state::dock_persistence::DockDocument` owns the typed `docks.json` envelope, while
  workspace and SFTP UI updates share its serialized atomic update path.
- The two remaining service registries reject duplicate registration and their distinct
  GPUI/process lifecycles are documented in `docs/architecture.md`.
- Recursive SFTP upload discovery produces one bounded plan; cancellation is checked during
  directory creation and failed creates are ignored only after metadata confirms an existing
  directory.
- Verification passed: `cargo fmt --all -- --check`, policy scripts, workspace Clippy/build,
  and `cargo test --workspace` (`412 passed, 3 ignored` across 35 suites).
- Implementation is recorded in commit `737254e` (`refactor(simplicity): reduce redundant service layers`).

---

## 10. Consistency

**Primary report:** [`consistency.md`](consistency.md)  
**Priority:** P1/P3

### Work items

- [x] Update architecture and feature documentation to current crate paths, while labeling
      retained historical/design records.
- [x] Translate remaining contributor-facing non-English repository comments to English and
      add an automated language check.
- [x] Standardize transport error, close, cancellation, and backpressure semantics across
      local, SSH, and SFTP boundaries.
- [x] Standardize persistence recovery behavior: missing files may initialize defaults, invalid
      JSON is quarantined, other read failures are reported, and writes remain atomic.
- [x] Correct comments describing transfer cleanup, security limits, and task ownership.
- [x] Add `crates/highlight` explicitly to root workspace members.
- [x] Add CI checks for documentation links, workspace membership, dependency rules, and
      contributor-facing language.

### Acceptance criteria

- [x] Contributor-facing repository content complies with the English-only rule; user-facing
      locale translations remain supported as data.
- [x] Local and SSH sessions expose equivalent observable lifecycle semantics.
- [x] Missing, corrupt, and failed persistence cases follow the documented policy.
- [x] Root workspace membership includes every path crate required by project rules.

### Completed consistency evidence (2026-07-22)

- Current architecture paths are indexed in `docs/architecture.md`, historical design records
  are labeled, and `scripts/check-doc-paths.py` validates the current path index.
- `scripts/check-english.py` validates contributor-facing comments and documentation; it excludes
  user-facing locale data. CI runs this check together with dependency and UI-fork checks.
- Local and SSH terminal input now uses typed delivery results, bounded event queues, FIFO input,
  explicit close/cancellation behavior, and observable queue failures. SFTP enqueue failures now
  reach the transfer reply channel instead of being silently discarded.
- Settings, SSH session, SFTP table, and dock persistence distinguish missing files from invalid
  or unreadable files, quarantine malformed JSON, report failures, and use atomic writes.
- `oneterm-highlight` is an explicit workspace member, and the dependency/path policy checks run
  in CI. Lifecycle and security comments were aligned with the current implementation.
- Verification passed: `cargo fmt --all`, consistency policy scripts, targeted tests (`42 passed,
  1 ignored`), workspace check, and `python scripts/check-english.py`.

---

## 11. Evolvability

**Primary report:** [`evolvability.md`](evolvability.md)  
**Priority:** P2/P3

### Work items

- [ ] Close the UI crate migration with an explicit architecture decision record.
- [ ] Complete terminal capability migration or remove the unused future API.
- [ ] Move service ownership from process globals toward app/context-owned services.
- [ ] Add versioned persistence envelopes and migrations for settings, sessions, docks, and SFTP state.
- [ ] Document the fork delta and upstream update cadence for `oneterm-ui`.
- [ ] Preserve feature self-registration while adding validation for duplicate/missing panel and command registrations.

### Acceptance criteria

- [ ] A future backend can implement only the capabilities it supports.
- [ ] A future app/window context can be instantiated without process-global state collisions.
- [ ] Older persisted configurations migrate through tested versions without data loss.
- [ ] The UI fork can be updated from upstream through a documented, reviewable process.

---

## Suggested issue breakdown

Create separate issues or pull requests in this order:

1. `security: fail closed on unknown SSH host keys`
2. `security: contain SFTP download destinations`
3. `reliability: add SSH connection timeout and cancellation`
4. `reliability: make terminal input delivery typed and loss-aware`
5. `reliability: move all SFTP actions off the UI thread`
6. `reliability: finalize transfers through temporary files`
7. `persistence: centralize atomic versioned JSON storage`
8. `architecture: reconcile oneterm-ui and workspace dependency graph`
9. `test: add SFTP, host-key, persistence, and lifecycle harnesses`
10. `scalability: introduce stable session IDs and cleanup`
11. `performance: share async runtime and stream large uploads`
12. `refactor: reduce local/SSH session facade duplication`
13. `docs: update architecture links and enforce English-only content`

## Definition of done for each issue

- [ ] Scope and threat/performance model are documented.
- [ ] Implementation is covered by focused unit or integration tests.
- [ ] Cancellation, shutdown, and failure behavior are tested where applicable.
- [ ] No new direct feature-to-backend dependency is introduced.
- [ ] Documentation and comments describe the current implementation.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.
- [ ] `cargo build --workspace` passes.
- [ ] The review report is updated with evidence and the item is checked off here.
