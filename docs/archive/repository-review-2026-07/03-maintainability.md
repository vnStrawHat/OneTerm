# Maintainability review — 6.5/10

## What is working

- Crate responsibilities are documented and machine-enforced. `scripts/verify-dependency-graph.py:60-105` checks exact internal edges, backend dependants, shell independence, and feature cross-dependencies.
- Vendor drift is controlled by pinned revisions, a narrow patch surface, and per-file SHA-256 baselines (`scripts/check-ui-fork.py:107-152`).
- Persistence ownership is explicit: shared mechanics live in core; each schema has an owner; the typed dock document prevents unrelated field patching (`crates/state/src/dock_persistence.rs:1-5`).
- Workspace lints deny `todo!` and `dbg!`, and the full clippy gate passes.

## Findings

### MAINT-01 — High: persistence side effects are embedded in mutable UI models

**Files/modules:** `crates/session-ui/src/session_state.rs:69-115`, `crates/settings/src/ui_config.rs:134-155`, `crates/settings-ui/src/terminal.rs:59-63`, `crates/workspace/src/layout/workspace/persistence.rs:81-104`.

**Explanation:** Session-store mutation methods update memory, notify observers, and synchronously save. Settings globals similarly own both live state and disk persistence. This couples domain mutation, GPUI notification, serialization, error reporting, and filesystem durability.

**Why it matters:** Callers cannot choose transactional behavior, test failed persistence independently, batch edits, or move I/O off the UI thread without changing model APIs. A failed save leaves memory updated but disk stale, with only logging as feedback.

**Recommended solution:** Separate mutation from persistence scheduling. Return a change/result, then let a persistence coordinator debounce and write immutable snapshots in background work. For user-visible operations, surface save failure through a notification and keep a dirty flag for retry.

### MAINT-02 — Medium: architecture policy has one explicit feature-to-feature exception

**Files/modules:** `crates/session-ui/Cargo.toml`, `docs/agents/crate-dependency-rules.md:38`, `crates/session-ui` connection flow, `crates/terminal-view` panel construction.

**Explanation:** `session-ui → terminal-view` is documented as the only same-layer cross-feature edge. It is understandable—connecting creates a terminal panel—but it means session UI changes can compile against terminal-view internals while all other feature composition is routed through app/state contracts.

**Why it matters:** Exceptions tend to become precedents. Future connection types or alternate terminal views may require additional cross-feature imports.

**Recommended solution:** Move “open a connected terminal session” behind an app-installed command/service contract, similar to the workspace command registry but typed around a session/panel request. Then session-ui owns connection UX and app/terminal-view owns terminal presentation.

### MAINT-03 — Medium: broad `AppError::Other(String)` erases operational categories

**Files/modules:** `crates/core/src/error.rs:3-48`, `crates/ssh/src/sftp_task.rs:373-375`, `crates/ssh/src/sftp_transfer.rs`, `crates/ssh/src/session.rs:402-408`.

**Explanation:** Host-key failures are typed, but most transport, cancellation, traversal, queue, and SFTP failures become arbitrary strings. The UI already compensates by comparing messages.

**Why it matters:** Error handling, telemetry, retry policy, and user messaging cannot reliably distinguish cancellation, closure, validation, permission, timeout, and transport failure.

**Recommended solution:** Add stable variants or domain-specific error types with `From` conversions. Keep human-readable context as fields/source chains, not as the discriminator.

### MAINT-04 — Medium: file-size policy is not automated

**Files/modules:** `docs/agents/structure.md:141-145`, 25 Rust files above 400 lines, CI scripts.

**Explanation:** The project treats ~400 lines as an immediate split threshold, but no check reports violations. The repository has multiple 500–900-line production modules.

**Why it matters:** A mandatory convention that is neither followed nor enforced becomes misleading governance and creates inconsistent review expectations.

**Recommended solution:** Either change the rule to a review guideline based on responsibility or add a lightweight script with an allowlist and trend limit. Prefer responsibility-based checks over an inflexible global line cap.
