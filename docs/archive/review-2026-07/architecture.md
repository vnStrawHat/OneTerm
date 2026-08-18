# Architecture Review

**Score: 6.0 / 10**

## Remediation status (2026-07-22)

The original findings below are retained as review history. The scoped architecture remediation is complete:

- **ARCH-01:** Resolved. `oneterm-ui` and `crates/ui` were removed; `gpui-component` is supplied through a root Cargo `[patch]` to `vendor/gpui-component`, which is excluded from workspace membership. The vendor snapshot records the upstream commit and approved delta surface.
- **ARCH-02:** Resolved. `DockPlacement` is a domain enum in `oneterm-core`; `oneterm-workspace` maps it to the vendored UI dock type.
- **ARCH-03:** Resolved. The unused parallel capability contracts were removed; `TerminalSession` is the authoritative implemented boundary.
- **ARCH-04:** Resolved. SFTP operations are asynchronous and object-safe, and UI paths use background orchestration.
- **ARCH-05:** Resolved for the current design. `SessionFactory` and `WorkspaceCommands` remain intentionally distinct, but registration rejects duplicates and missing services are handled explicitly.
- **ARCH-06:** Resolved. Workspace membership, dependency allow-lists, architecture paths, vendor baseline, and contributor-facing language are checked by scripts in CI.

## ARCH-01 — The documented crate graph omits the current `oneterm-ui` layer

- **Files:** `Cargo.toml:1-52`; `crates/ui/Cargo.toml:1-19`; `docs/agents/structure.md:147-168`; `docs/agents/crate-dependency-rules.md:12-45`
- **Modules:** Workspace architecture and dependency governance
- **Severity:** **High**
- **Explanation:** The root workspace includes `crates/ui`, described as a vendored fork of the gpui-component dock module. `actions`, `state`, `workspace`, all feature crates, and `app` depend on it. The authoritative layer table and R1-R12 rules do not assign it a layer or allowed edges.
- **Why it matters:** Reviewers cannot determine whether actual dependencies obey R2/R5. A central shared UI fork can become a hidden upward/same-layer dependency and a new god-crate if its responsibility is not constrained.
- **Recommended solution:** Decide and document one of two states: (1) `oneterm-ui` is a deliberate low shared-UI infrastructure crate with a strict API/fork-delta policy, or (2) it is temporary and must be removed on a dated migration. Update every graph, verification command, and crate responsibility table.

## ARCH-02 — `actions` depends on the local dock implementation

- **Files:** `crates/actions/Cargo.toml:10-15`; `crates/actions/src/lib.rs:3-17`; `crates/ui/Cargo.toml`
- **Modules:** Shared actions and dock infrastructure
- **Severity:** **Medium/High**
- **Explanation:** The low “leaf-ui” actions crate imports `oneterm_ui::dock::DockPlacement` for `AddPanel`. This couples action serialization/API to a large local UI implementation.
- **Why it matters:** Changing/replacing the dock fork now changes the shared action layer and every consumer. It undermines the rule that shared types live in the lowest suitable domain crate.
- **Recommended solution:** Define an application-owned small placement enum in `core` or `actions`, then map it to dock placement in `workspace`. Alternatively, remove the generic `AddPanel(DockPlacement)` action if only fixed actions are used.

## ARCH-03 — `TerminalSession` is an oversized capability aggregate while a second API sits unused

- **Files:** `crates/terminal/src/session.rs:155-407`; `crates/terminal/src/contracts.rs:1-151`; `crates/terminal/src/lib.rs:6-60`
- **Modules:** Terminal engine contracts
- **Severity:** **High**
- **Explanation:** Production consumers use the monolithic trait spanning rendering, input, mouse, selection, search, IME, lifecycle, network stats, SFTP, cwd, and agent integration. `contracts.rs` exports narrower traits and typed errors but explicitly says consumers do not use them.
- **Why it matters:** The code pays for two conceptual APIs, while the production one cannot express input failure. Implementations and fakes must satisfy a broad interface, increasing coupling and migration cost.
- **Recommended solution:** Stop adding speculative methods to `contracts.rs`. Migrate one vertical slice first—typed write/resize/close—then renderer/lifecycle/capabilities. Delete each old method when all callers move. If migration is not scheduled, remove the unused public traits and track the design in docs only.

## ARCH-04 — SFTP's synchronous domain trait leaks scheduling responsibility to UI code

- **Files:** `crates/core/src/sftp.rs:53-107`; `crates/ssh/src/sftp.rs:135-243`; `crates/sftp-ui/src/actions.rs`; `crates/sftp-ui/src/panel_ops.rs:15-93`
- **Modules:** Core protocol abstraction and SFTP feature
- **Severity:** **High**
- **Explanation:** A UI-independent trait is good, but its blocking methods are implemented with channel send + `blocking_recv`. Some call sites remember to use a background executor (`load_dir`), while others do not (`actions`).
- **Why it matters:** Correct scheduling becomes a convention instead of a type/API invariant.
- **Recommended solution:** Make operations return futures or operation handles and model transfer/cancellation as typed results. Keep protocol types out of UI while making it impossible to block accidentally.

## ARCH-05 — Multiple global registries provide inversion but hide dependencies

- **Files:** `crates/terminal/src/factory.rs:48-58`; `crates/state/src/commands.rs:15-40`; `crates/app/src/init.rs:20-61`; `crates/state/src/app_state.rs`
- **Modules:** App composition
- **Severity:** **Medium**
- **Explanation:** `SessionFactory`, `WorkspaceCommands`, and many entities are discovered through process/GPUI globals. This successfully prevents forbidden crate edges, but initialization order and availability are runtime concerns.
- **Why it matters:** Missing initialization is detected late; `OnceLock::set` errors are ignored; tests cannot reset the factory; dependencies are not visible in constructors.
- **Recommended solution:** Keep `app` as composition root, but package services into one app-owned service registry/entity. Validate registration once, fail clearly on duplicate/missing services, and provide test-scoped construction.

## ARCH-06 — Workspace membership and architectural guidance disagree

- **Files:** `Cargo.toml:1-23,37-52`; `docs/agents/structure.md`; `docs/refactor/ui-crate-restructure.md:505-536`
- **Modules:** Workspace governance
- **Severity:** **Medium**
- **Explanation:** `oneterm-highlight` is a path dependency but is not explicitly listed in `members`, contrary to R11. The refactor “done when” condition says `crates/ui` is removed, while current manifests deliberately reintroduce it as a dock fork.
- **Why it matters:** Automated graph checks and contributors' mental models are unreliable.
- **Recommended solution:** Explicitly list every crate, update the refactor status, and add a script/test that compares actual internal dependencies with the documented allow-list.

## Architecture strengths

- `app` is a clear composition root and the only crate combining shell, features, and backends.
- UI crates use `SessionFactory` rather than importing backend implementations.
- `workspace` drives feature functionality through an inversion registry and panel names, preserving a feature-agnostic shell.
- Domain/engine separation keeps `core` GPUI/alacritty-free and `terminal` GPUI-free.
- Feature self-registration is localized in `init` functions.
- The repository has explicit architectural rules and verification commands; the priority is to reconcile them with the current graph.
