# Architecture review — 7.5/10

## System-level assessment

The repository follows a recognizable layered architecture:

- pure domain and terminal engine at the bottom;
- settings/state/theme shared services;
- a feature-agnostic workspace shell;
- feature crates and backend crates at the same high layer;
- one app crate as composition root.

This is not merely aspirational: `scripts/verify-dependency-graph.py` validates the manifest graph, and the check passed for all 16 packages. The design successfully prevents protocol code from leaking into GPUI features and prevents the shell from becoming an omniscient UI monolith.

## Architectural strengths

### ARCH-S1 — Layer boundaries are explicit and executable

**Files/modules:** `Cargo.toml:1-55`, `scripts/dependency-graph-policy.json`, `scripts/verify-dependency-graph.py:32-124`, crate manifests.

The graph rules encode app-only backend composition, feature independence, shell independence, and low-level purity. This materially reduces accidental coupling.

### ARCH-S2 — App is a clear composition root

**Files/modules:** `crates/app/src/init.rs:20-62`, `crates/app/src/session_factory.rs`, `crates/app/src/window.rs`.

The app installs the backend factory, initializes globals/features, composes cross-feature panels, registers commands, and opens the window. Cross-feature composition is not hidden in arbitrary feature crates.

### ARCH-S3 — Domain/engine types are UI-neutral

**Files/modules:** `crates/core`, `crates/terminal/src/session.rs:1-9`, `crates/terminal/src/factory.rs:1-8`.

Mouse buttons, cursor bounds, channels, and session contracts avoid GPUI types. Backends are replaceable behind traits and can be tested without launching the GUI.

### ARCH-S4 — Persistence ownership is explicitly modeled

**Files/modules:** `crates/core/src/persistence.rs`, `crates/state/src/dock_persistence.rs`, `docs/agents/persistence.md`.

Mechanics and schemas have separate owners. The typed `DockDocument` prevents independent features from destructively rewriting each other's fields.

## Findings

### ARCH-01 — High: hidden runtime dependencies weaken otherwise strong compile-time architecture

**Files/modules:** `crates/terminal/src/factory.rs:48-110`, `crates/state/src/commands.rs:15-45`, `crates/app/src/init.rs:50-62`.

**Explanation:** The graph is compile-time safe, but key dependencies are resolved through a process global and a GPUI global. Consumers can compile without a registered factory/command set. The app panics on duplicate registration, while missing registration is handled variably by consumers.

**Why it matters:** Initialization ordering, alternate app contexts, integration tests, and future plugin sets are runtime concerns that the type system cannot validate.

**Recommended solution:** Introduce typed service bundles passed at feature initialization/window construction. Add one explicit startup validation before opening the window. Preserve app-only backend knowledge by constructing trait objects in app.

### ARCH-02 — High: global active-terminal state is not window-scoped

**Files/modules:** `crates/state/src/app_state.rs:13-37`, `crates/sftp-ui/src/panel.rs:34-52`, terminal panel activation flow.

**Explanation:** Active SFTP/CWD/local flags live in a process-global app entity. The SFTP feature assumes one panel for the whole app.

**Why it matters:** Multi-window/multi-workspace behavior will be ambiguous. Background tasks and panel swaps already require mirrored per-backend state to compensate.

**Recommended solution:** Create `WorkspaceState` per window containing dock area, active terminal capabilities, zoom, and SFTP context. Keep only durable settings/theme and genuinely process-wide registries global.

### ARCH-03 — Medium: `session-ui → terminal-view` makes a feature a composition root

**Files/modules:** `docs/agents/crate-dependency-rules.md:38`, `crates/session-ui/Cargo.toml`.

**Explanation:** The exception exists because SSH connection UX creates a terminal panel. This is the only allowed feature cross-dependency.

**Why it matters:** It blurs the otherwise clean rule that app owns feature composition and makes new session/presentation types harder to add.

**Recommended solution:** Route a typed “open connected terminal” request through an app-installed service. Session UI should return a session/request, not construct another feature's view.

### ARCH-04 — Medium: backpressure is not part of the architecture contract

**Files/modules:** `crates/terminal/src/session.rs`, `crates/ssh/src/session.rs:153-157`, listeners/event queues, SFTP channels.

**Explanation:** The architecture defines type and dependency boundaries but not capacity/ordering guarantees. Some queues are bounded and lossy, some bounded and awaited, and terminal command queues are unbounded.

**Why it matters:** Reliability under load depends on incidental channel choices. Changes can preserve types while changing user-visible loss or memory behavior.

**Recommended solution:** Document an end-to-end event/command contract: ordering, coalescing, capacity, byte budget, priority, cancellation, and close semantics. Encode it in shared transport adapters and overload tests.

### ARCH-05 — Medium: persistence architecture promises concurrency more broadly than implementation provides

**Files/modules:** `crates/core/src/persistence.rs:1-5`, `:16-27`, `docs/agents/persistence.md:6-11`.

**Explanation:** Documentation says concurrent writers cannot interleave, but locking is process-local. The distinction is mentioned only in the `atomic_write` function comment (`crates/core/src/persistence.rs:45-48`).

**Why it matters:** Architectural guarantees should match the actual concurrency domain.

**Recommended solution:** State “in-process” explicitly everywhere until an OS lock/single-instance mechanism exists, then test the chosen guarantee with subprocesses.
