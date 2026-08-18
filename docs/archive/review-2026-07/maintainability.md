# Maintainability Review

**Score: 5.5 / 10**

## Remediation status

- **MAINT-01:** Resolved for current manifests. The crate table now includes `oneterm-ui` and `oneterm-highlight`; `scripts/verify-dependency-graph.py` checks the machine-readable internal dependency allow-list in CI.
- **MAINT-02:** Core terminal-model duplication is resolved by `TerminalModel`; transport-specific session/state code remains intentionally separate. Further state consolidation is deferred until the session contract is split.
- **MAINT-03:** Shared persistence mechanics are centralized in `oneterm-core::persistence`; feature crates retain only domain serialization and field ownership.
- **MAINT-04:** The runtime error taxonomy is documented in [`docs/agents/error-policy.md`](../../agents/error-policy.md). Remaining ignored results should be migrated incrementally with focused behavior changes rather than broad mechanical edits.
- **MAINT-05:** The local UI fork now has a pinned-revision procedure and reviewed hash baseline via `scripts/check-ui-fork.py`; the reference checkout remains optional in CI.

## MAINT-01 — Architecture/document drift makes dependency maintenance unsafe

- **Files:** `Cargo.toml`; `crates/*/Cargo.toml`; `docs/agents/structure.md`; `docs/agents/crate-dependency-rules.md`; `docs/refactor/ui-crate-restructure.md`
- **Modules:** Workspace dependency governance
- **Severity:** **High**
- **Explanation:** The actual graph includes `oneterm-ui`, while the documented layer table omits it. `oneterm-highlight` is used but not explicitly listed in root members. Several design documents refer to obsolete `crates/ui`/`crates/local` structures.
- **Why it matters:** A future dependency change can appear compliant against the docs while violating the actual intended architecture.
- **Recommended solution:** Generate a machine-readable allow-list from the architecture decision and verify manifests in CI. Update all current paths and label historical plans.

## MAINT-02 — Backend duplication multiplies contract-change cost

- **Files:** `crates/local-shell/src/session_terminal.rs`; `crates/ssh/src/session_terminal.rs`; related `state.rs`/`listener.rs`
- **Modules:** Local and SSH terminal adapters
- **Severity:** **Medium**
- **Explanation:** The two session facade files are approximately 83% similar, and their state files are approximately 64% similar by line-sequence comparison.
- **Why it matters:** New terminal features must be implemented and tested twice, and behavioral divergence is likely.
- **Recommended solution:** Extract shared model/capability adapters while leaving transport-specific listeners and optional SFTP methods in backend modules.

## MAINT-03 — Configuration persistence is scattered across feature crates

- **Files:** `crates/settings/src/{terminal_config/mod.rs,ui_config.rs}`; `crates/session-ui/src/session_state.rs`; `crates/workspace/src/layout/workspace/persistence.rs`; `crates/sftp-ui/src/persistence.rs`
- **Modules:** Persistent configuration and layout
- **Severity:** **High**
- **Explanation:** Each module owns its own path resolution, serialization, error fallback, and direct write behavior. `docks.json` is shared by at least two writers.
- **Why it matters:** Schema migration, backup, atomicity, locking, and recovery must be fixed in multiple places.
- **Recommended solution:** Provide a common persistence utility with injected path, schema version, atomic write, backup/quarantine, and serialized per-file updates. Keep domain serializers in their owning modules.

## MAINT-04 — Error policy varies between modules

- **Files:** Examples include `crates/sftp-ui/src/actions.rs:48,184,257,383`; `crates/app/build.rs:29,43,72,118`; `crates/sftp-ui/src/browser_state.rs:103-124`; `crates/ssh/src/sftp.rs:208-238`
- **Modules:** UI operations, build scripts, global state, transport bridge
- **Severity:** **Medium**
- **Explanation:** Some failures are returned, some logged, some converted to empty/default state, some ignored (`let _ =`), and some panic with `unwrap`/`expect`. This is appropriate in some build/test cases but not consistently documented in runtime code.
- **Why it matters:** Users cannot predict which failures are recoverable, visible, or destructive.
- **Recommended solution:** Define an error taxonomy: user-action errors become notifications; transport closure/cancellation become typed state; invariant violations panic only at initialization; persistence preserves data and reports recovery. Enforce it in review/clippy policy.

## MAINT-05 — The local vendored UI fork needs an explicit delta-maintenance process

- **Files:** `crates/ui/Cargo.toml`; `crates/ui/src/dock/*`; `docs/agents/dependencies.md:67-83`; `Cargo.toml:29-34`
- **Modules:** gpui-component integration
- **Severity:** **Medium**
- **Explanation:** `oneterm-ui` is a local fork of the dock module, while the project also pins upstream gpui-component. The docs explain reference lookup but do not state how the fork is synchronized, what patches exist, or how upstream security/behavior changes are reviewed.
- **Why it matters:** Forks accumulate divergence and can silently miss upstream fixes.
- **Recommended solution:** Record the upstream commit, fork delta, and rebase procedure. Add a diff/check script and a small integration test suite around patched behavior.

## Maintainability strengths

- The workspace uses narrow domain crates and feature crates rather than one current UI monolith.
- Public API comments and design docs explain intended invariants.
- Workspace lints and format/clippy commands establish a quality baseline.
- `TerminalModel` already centralizes many shared operations, providing a good seam for removing adapter duplication.
