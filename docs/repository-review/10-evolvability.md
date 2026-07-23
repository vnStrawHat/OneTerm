# Evolvability review — 6.5/10

## What is working

- Protocol backends implement neutral contracts, so a future backend can be added without importing it into feature crates.
- Feature registration and app-only composition make new panels discoverable without adding shell-to-feature dependencies.
- Persisted fields have explicit schema owners and unknown dock fields are preserved through `#[serde(flatten)]` (`crates/state/src/dock_persistence.rs:16-31`).
- Settings use serde defaults, allowing additive fields to remain backward compatible.

## Findings

### EVOL-01 — High: process-global registration limits alternate application topologies

**Files/modules:** `crates/terminal/src/factory.rs:48-110`, `crates/state/src/commands.rs:15-45`, `crates/app/src/init.rs:20-62`.

**Explanation:** The session factory can be installed only once per process. Workspace commands are one global callback set. This fits one app/runtime, but alternate workspaces, embedded tests, plugin sets, or different backend policies cannot coexist.

**Why it matters:** Future multi-window support, safe mode, enterprise policy, or plugin-driven backends would need to replace globals or add more global switches.

**Recommended solution:** Move services into workspace/app-scoped entities and pass handles to features. Keep process globals only for immutable process services that cannot be scoped. Provide a typed startup manifest listing required panels/services.

### EVOL-02 — Medium: persisted schemas have no explicit version yet

**Files/modules:** `crates/settings/src/terminal_config`, `crates/settings/src/ui_config.rs:27-61`, `crates/session-ui/src/session_state.rs:21-41`, `crates/state/src/dock_persistence.rs:16-31`, `docs/agents/persistence.md:29-39`.

**Explanation:** Current additive changes rely on defaults and optional fields. The policy says to add a top-level version at the first incompatible change, but no schema currently has a migration harness/fixtures matching the documented convention.

**Why it matters:** The first incompatible change is the worst time to invent migration infrastructure. Session host data and layout are user-owned and difficult to recreate.

**Recommended solution:** Before the first incompatible change, implement reusable migration runners and fixture conventions. It is reasonable to keep version absence as v0, but add tests that load representative legacy documents and validate idempotent current output.

### EVOL-03 — Medium: the feature boundary exception will complicate new session types

**Files/modules:** `docs/agents/crate-dependency-rules.md:38`, `crates/session-ui` → `crates/terminal-view` dependency.

**Explanation:** Session UI directly creates terminal-view panels. If future host/session types produce different panels, or terminal presentation is replaced, session management becomes the composition point despite app being the declared composition root.

**Why it matters:** New RDP/container/serial session types could create additional feature-to-feature edges.

**Recommended solution:** Define a domain request such as `OpenTerminalRequest { session, label, placement }` and let an app-installed presentation service create the panel.

### EVOL-04 — Medium: error strings are not stable extension points

**Files/modules:** `crates/core/src/error.rs:40-48`, `crates/sftp-ui/src/transfer.rs:150-154`, `:432-436`, many `AppError::msg` conversions in `crates/ssh`.

**Explanation:** Callers cannot evolve retry, cancellation, telemetry, and user messaging independently because categories are embedded in text.

**Why it matters:** Adding localization or richer context becomes a behavior change.

**Recommended solution:** Introduce typed, domain-owned errors now while the public surface is small. Preserve source errors and machine-readable categories.

### EVOL-05 — Low: the vendored UI patch is controlled but increases upgrade cost

**Files/modules:** `Cargo.toml:168-179`, `scripts/check-ui-fork.py`, `docs/agents/ui-fork-maintenance.md`.

**Explanation:** Hash baselines and a one-file patch are excellent controls, but every upstream upgrade requires full snapshot review and patch regeneration.

**Why it matters:** The longer the fork remains, the more expensive security/compatibility upgrades become.

**Recommended solution:** Continue the current baseline process and upstream `TabPanel::set_active_panel` if possible. Track the patch's upstream status and delete the fork once released support is available.
