# Readability Review

**Score: 6.0 / 10**

## READ-01 — The repository contains many files beyond its own size guideline

- **Files:** `crates/ui/src/dock/{mod.rs,tab_panel.rs,tiles.rs,dock.rs}`, `crates/state/src/agent_registry.rs`, `crates/ssh/src/sftp_task.rs`, `crates/terminal-view/src/{panel/mod.rs,view/mod.rs,layout/cache.rs}`, plus the inventory of `crates/**/*.rs`
- **Modules:** Dock, state, SFTP, terminal view, backend transfer code
- **Severity:** **Medium**
- **Explanation:** The guidance in `docs/agents/structure.md:135-145` says each Rust file should be about 400 lines or less. The current source inventory found 243 Rust files and 47,479 lines, including 1,236-line `tab_panel.rs`, 1,231-line `tiles.rs`, 1,175-line `dock/mod.rs`, 877-line `agent_registry.rs`, and 853-line `sftp_task.rs`.
- **Why it matters:** Large files mix policy, state transitions, rendering, and tests. A reviewer must hold more context, and changes are more likely to create accidental coupling.
- **Recommended solution:** Split by stable responsibility, not arbitrary line count. For example, split SFTP transfer orchestration, traversal, upload/download streaming, and cancellation; split Agent data models/folding/registry/tests; split dock interaction, persistence, and rendering. Update module-level docs after each split.

## READ-02 — Local and SSH session facades repeat the same conceptual API

- **Files:** `crates/local-shell/src/session_terminal.rs` and `crates/ssh/src/session_terminal.rs`
- **Modules:** Backend implementations of `TerminalSession`
- **Severity:** **Medium**
- **Explanation:** A line-level comparison shows about 83% similarity between the two files (252 vs. 276 lines). Both adapt `TerminalModel`, manage cell metrics/IME, expose selection/mouse/search/lifecycle, and read shared state.
- **Why it matters:** Bug fixes and contract changes must be applied twice. The two implementations can drift in subtle ways, especially around close, query behavior, and new terminal capabilities.
- **Recommended solution:** Extract a shared backend-neutral session adapter or capability helper around `TerminalModel`, leaving only transport-specific input/lifecycle/SFTP methods in each backend. Keep type-specific listeners as parameters rather than duplicating the facade.

## READ-03 — The active architecture is difficult to infer because source paths in docs are stale

- **Files:** `docs/agents/structure.md:106-168`; `docs/refactor/ui-crate-restructure.md`; `docs/sftp-browser-design.md`; `docs/ssh-client-connect.md`; `docs/terminal-fullscreen-perf/*`; current `crates/terminal-view`, `crates/sftp-ui`, `crates/session-ui`, `crates/ui`
- **Modules:** Repository documentation and navigation
- **Severity:** **High for contributor comprehension**
- **Explanation:** Many design documents still refer to `crates/ui/src/views/...`, `crates/local`, and an old monolith, while implementation is split across feature crates and `oneterm-ui` is a vendored dock fork.
- **Why it matters:** Readers may implement changes in nonexistent modules or misunderstand which boundaries are enforced today.
- **Recommended solution:** Mark historical docs explicitly, add current-path mapping at the top, and periodically validate referenced paths in CI. Keep one current architecture document as the source of truth.

## READ-04 — Long closure capture chains obscure UI behavior

- **Files:** `crates/session-ui/src/{connect_dialog.rs,quick_connect_dialog.rs,session_dialog.rs,rename_group.rs}`, `crates/sftp-ui/src/actions.rs`, `crates/terminal-view/src/panel/title.rs`
- **Modules:** GPUI dialog actions
- **Severity:** **Medium**
- **Explanation:** Dialogs construct `Rc<dyn Fn(...)>` callbacks, clone many `Entity`/`Rc` values, then reuse the callback for click and keyboard paths. This is valid GPUI plumbing but makes validation, persistence, async launch, and dialog-close behavior hard to follow.
- **Why it matters:** It is easy for the click and keyboard paths to diverge or capture stale state. Error handling is spread across nested closures.
- **Recommended solution:** Extract named command functions/operation structs that receive a small context and return a typed action result (`KeepOpen`, `Close`, `Started`). Keep the closure as a thin adapter.

## READ-05 — “State” files contain both domain folding and UI lifecycle responsibilities

- **Files:** `crates/state/src/agent_registry.rs:1-700`; `crates/state/src/app_state.rs:1-55`
- **Modules:** Shared state and Agent feature model
- **Severity:** **Medium**
- **Explanation:** `agent_registry.rs` contains protocol display caps, sanitization, all folded data models, lifecycle logic, ordering, global initialization, and tests. `AppState` also carries active terminal/SFTP handles and dock/window coordination fields.
- **Why it matters:** The crate is intentionally shared, but its responsibilities are broad enough that a change to agent protocol folding can affect global state initialization and feature assumptions.
- **Recommended solution:** Keep `state` as a runtime boundary, but split the Agent fold into a focused model module and keep global/entity registration in a small registry module. Document which fields are ownership, mirrors, or cross-feature capabilities.

## Readability strengths

- Most public types and functions have useful English doc comments.
- File/module names generally describe their main responsibility.
- Security and lifecycle comments explain non-obvious invariants, especially around event coalescing and close flags.
- The design documents provide intent and rationale, even where their paths need updating.

## Current remediation status (2026-07-22)

The scoped Readability remediation is complete. It split the Agent model and SFTP transfer
responsibilities, established the current architecture index, and aligned historical documentation
and ownership comments with the implementation. The remaining dock/terminal-view file splits and
deep GPUI callback extraction are optional follow-up work and are not included in the completed
scope.

Evidence: commit `819d589` (`refactor(readability): clarify module ownership`). Workspace
formatting, policy checks, Clippy, build, and tests passed with `413 passed, 3 ignored` across
35 suites.
