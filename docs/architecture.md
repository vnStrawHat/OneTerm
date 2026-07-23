# OneTerm architecture index

This is the current architecture source of truth for contributors. Historical design
records may contain old paths; they are labeled and should be read for rationale only.
Use this page and `docs/agents/structure.md` when locating current implementation code.

## Crate map

| Layer | Crate | Current responsibility | Entry points |
|---|---|---|---|
| Domain | `oneterm-core` | Errors, SSH/local configuration, SFTP contracts | `crates/core/src/lib.rs`, `crates/core/src/sftp.rs` |
| Terminal engine | `oneterm-terminal` | Terminal model, session contract, encoding, OSC, search | `crates/terminal/src/lib.rs`, `crates/terminal/src/model.rs`, `crates/terminal/src/contracts.rs` |
| Shared services | `oneterm-settings` | Persistent terminal and UI settings | `crates/settings/src/lib.rs` |
| Shared services | `oneterm-state` | Global state, commands, typed dock persistence, Agent folded model | `crates/state/src/lib.rs`, `crates/state/src/dock_persistence.rs`, `crates/state/src/agent_registry.rs`, `crates/state/src/agent_model.rs` |
| Vendor patch | `gpui-component` | Pinned upstream UI crate with the reviewed source and standalone-manifest patches | `vendor/README.md`, `vendor/patches/gpui-component/` |
| Shell | `oneterm-workspace` | Feature-agnostic window, layout, dock persistence, status bar | `crates/workspace/src/lib.rs`, `crates/workspace/src/layout/` |
| Backend | `oneterm-local-shell` | Local PTY session implementation | `crates/local-shell/src/lib.rs`, `crates/local-shell/src/session_terminal.rs` |
| Backend | `oneterm-ssh` | SSH shell and SFTP implementations | `crates/ssh/src/lib.rs`, `crates/ssh/src/session_terminal.rs`, `crates/ssh/src/sftp_task.rs`, `crates/ssh/src/sftp_transfer.rs` |
| Feature | `oneterm-terminal-view` | Terminal panel, rendering, input, split spaces | `crates/terminal-view/src/lib.rs`, `crates/terminal-view/src/panel/` |
| Feature | `oneterm-sftp-ui` | SFTP browser, transfer queue, persistence UI | `crates/sftp-ui/src/lib.rs`, `crates/sftp-ui/src/panel.rs` |
| Feature | `oneterm-session-ui` | Session tree and SSH connection dialogs | `crates/session-ui/src/lib.rs`, `crates/session-ui/src/connect_dialog.rs` |
| Feature | `oneterm-settings-ui` | General Settings window | `crates/settings-ui/src/lib.rs` |
| Feature | `oneterm-agent-ui` | Agent fleet view and cards | `crates/agent-ui/src/lib.rs` |
| Wiring | `oneterm-app` | Installs backends, initializes features, opens the workspace | `crates/app/src/lib.rs`, `crates/app/src/init.rs` |

## Dependency direction

Dependencies point downward through the layers. `oneterm-app` is the only crate that
knows the backends, feature crates, shell, and shared layers together. Feature crates
create sessions through `oneterm_terminal::SessionFactory`; they do not depend on
`oneterm-ssh` or `oneterm-local-shell`. The workspace shell is feature-agnostic and
uses command/panel registries rather than importing feature implementations.

`gpui-component` remains an external dependency in every crate manifest. The root
Cargo `[patch]` redirects that dependency to `vendor/gpui-component`; the vendor
package is not a OneTerm workspace member and does not create an internal UI layer.
Shared action contracts use `oneterm_core::DockPlacement`, and only the workspace
shell maps that domain value to `gpui_component::dock::DockPlacement`.

The machine-readable dependency policy and verification commands are in
[`docs/agents/crate-dependency-rules.md`](agents/crate-dependency-rules.md), and the
CI entry point is [`scripts/verify-dependency-graph.py`](../scripts/verify-dependency-graph.py).

## Service registration

Two validated registries remain intentionally distinct:

- `oneterm_terminal::SessionFactory` is a process-wide `OnceLock` because terminal
  session creation is used from background work that does not carry a GPUI `App`.
- `oneterm_state::WorkspaceCommands` is a GPUI global because its window-bound
  callbacks require `App`/`Window` access and follow the application lifecycle.

Both reject duplicate registration, and consumers handle a missing registry before
dispatch. The app crate is the only registration site for either service.

## Ownership shortcuts

- Terminal behavior and shared terminal capability changes: `crates/terminal/`.
- Backend transport and lifecycle: `crates/local-shell/` or `crates/ssh/`.
- Terminal UI and rendering: `crates/terminal-view/`.
- SFTP UI actions and transfer presentation: `crates/sftp-ui/`.
- SSH saved sessions and connection UX: `crates/session-ui/`.
- Cross-feature runtime state: `crates/state/`.
- Window shell and dock layout: `crates/workspace/`.
- Persistent file lifecycle mechanics: `crates/core/src/persistence.rs`; the typed dock document is owned by `crates/state/src/dock_persistence.rs`; schema ownership is documented in [`docs/agents/persistence.md`](agents/persistence.md).

## Navigation and validation

- Read [`docs/agents/structure.md`](agents/structure.md) for the complete tree and
  crate rules.
- Read [`docs/agents/code-style.md`](agents/code-style.md) before changing GPUI code.
- Read [`docs/agents/error-policy.md`](agents/error-policy.md) before changing runtime
  error handling.
- Run `python scripts/check-doc-paths.py` after changing this page.
- Historical documents are retained for design rationale and are labeled at their
  entry point; update this index instead of copying old paths into new documentation.
