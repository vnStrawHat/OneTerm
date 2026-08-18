# OneTerm architecture index

This is the current architecture source of truth for contributors. Historical design
records may contain old paths; they are labeled and should be read for rationale only.
Use this page and `docs/agents/structure.md` when locating current implementation code.

## Crate map

| Layer | Crate | Current responsibility | Entry points |
|---|---|---|---|
| Domain | `oneterm-core` | Errors, SSH/local configuration, SFTP contracts | `crates/core/src/lib.rs`, `crates/core/src/sftp.rs` |
| Terminal engine | `oneterm-terminal` | Terminal model, session contract, encoding, OSC, search | `crates/terminal/src/lib.rs`, `crates/terminal/src/model.rs`, `crates/terminal/src/contracts.rs` |
| Completion engine | `oneterm-completion` | Auto-completion engine (gpui-free): embedded command catalogs, line parsing + subcommand resolution, matching/ranking, in-session history, secret redaction | `crates/completion/src/lib.rs`, `crates/completion/src/engine.rs`, `crates/completion/src/catalog.rs`, `crates/completion/src/history.rs`, `crates/completion/src/redact.rs` |
| Shared services | `oneterm-settings` | Persistent terminal and UI settings | `crates/settings/src/lib.rs` |
| Shared services | `oneterm-state` | App-scoped services, workspace state, typed dock persistence, registered dock panel names, Agent folded model, process-global completion history | `crates/state/src/lib.rs`, `crates/state/src/services.rs`, `crates/state/src/dock_persistence.rs`, `crates/state/src/panel_names.rs`, `crates/state/src/agent_registry.rs`, `crates/state/src/completion_history.rs` |
| Shared services | `oneterm-update` | GitHub Releases auto-update service, release selection, download, verification, staging, and install orchestration | `crates/update/src/lib.rs`, `crates/update/src/config.rs`, `crates/update/src/github.rs`, `crates/update/src/archive.rs`, `crates/update/src/install.rs` |
| Vendor patch | `gpui-component` | Pinned upstream UI crate with the reviewed source and standalone-manifest patches | `vendor/README.md`, `vendor/patches/gpui-component/` |
| Shell | `oneterm-workspace` | Feature-agnostic window, layout, dock persistence, status bar | `crates/workspace/src/lib.rs`, `crates/workspace/src/layout/` |
| Backend | `oneterm-local-shell` | Local PTY session implementation | `crates/local-shell/src/lib.rs`, `crates/local-shell/src/session_terminal.rs` |
| Backend | `oneterm-ssh` | SSH shell and SFTP implementations | `crates/ssh/src/lib.rs`, `crates/ssh/src/session_terminal.rs`, `crates/ssh/src/sftp_task.rs`, `crates/ssh/src/sftp_task/`, `crates/ssh/src/sftp_task/transfer/` |
| Feature | `oneterm-terminal-view` | Terminal panel, rendering, input, split spaces, auto-completion overlay | `crates/terminal-view/src/lib.rs`, `crates/terminal-view/src/panel/`, `crates/terminal-view/src/completion/` |
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

`oneterm_state::AppServices` is the single application-scoped service bundle and
the only injection registry. The composition root (`crates/app/src/init.rs`)
constructs the backend-neutral `SessionFactory` and the `WorkspaceCommands`
callbacks, collects the cross-feature hooks each feature exposes
(`oneterm_terminal_view::status_metrics` — the `ActiveTerminalMetricsProvider`
read by the status-bar widgets — and `oneterm_terminal_view::agent_focuser`, the
`AgentFocuser` used by the Agent Panel), and installs them all with
`AppServices::install`, which rejects duplicate registration.

Consumers read handles through `AppServices::global(cx)` (or the typed accessors
`session_factory` / `workspace_commands` / `active_terminal::breadcrumb` /
`agent_focus::focus_terminal`). Presence is a startup invariant: a missing bundle
panics with a precise message per the error policy, so shell handlers carry no
`Option` fallbacks. Feature crates do not own registries or backend construction.
Workspace active terminal and SFTP state remains keyed by DockArea (`AppState`),
while durable settings and persistence policy remain process-wide where documented.

Shared globals follow one init contract: `AppState::init`, `UiConfig::init`,
`TerminalSettings::init`, `AgentRegistry::init` and `GlobalCompletionHistory::init`
are idempotent — the first call installs, later calls are no-ops. Only the
composition root calls them; the shell assumes they exist.

The exit-time `docks.json` write is owned by the shell: `OneTermWorkspace`
registers an entity-bound `on_app_quit` (quit while the window is open) and an
`on_release` hook (window closed by the user, which drops the root view before
`cx.quit()`); whichever runs first writes the layout synchronously so the write
cannot be lost to process exit (CORR-04). `AppState` carries no mirrors for it.

Dock panels are registered with the gpui-component `PanelRegistry` by their owning
feature's `init()` (R12) and built by the shell *by name* (R4). The registered
names are string constants in `crates/state/src/panel_names.rs`
(`oneterm_state::panel_names::{TERMINAL, TERMINAL_SETTINGS, SFTP, SESSION,
SSH_CLIENT, AGENT}`, plus `ALL`); the mapping from `oneterm_core::RightDockMode`
to a panel name is `panel_names::right_dock_panel_name` so `core` stays panel-agnostic.
Saved layouts deserialize by these names, so the string values are a persisted
contract. `oneterm_workspace`'s `build_named_panel` logs an error (with the known
name list) whenever a requested name is not registered instead of silently rendering
the placeholder panel.

## Ownership shortcuts

- Terminal behavior and shared terminal capability changes: `crates/terminal/`.
- Backend transport and lifecycle: `crates/local-shell/` or `crates/ssh/`.
- Terminal UI and rendering: `crates/terminal-view/`.
- SFTP UI actions and transfer presentation: `crates/sftp-ui/`.
- SSH saved sessions and connection UX: `crates/session-ui/`.
- Cross-feature runtime state: `crates/state/`.
- Window shell and dock layout: `crates/workspace/`.
- Persistent file lifecycle mechanics: `crates/core/src/persistence.rs`; the typed dock document is owned by `crates/state/src/dock_persistence.rs`; schema ownership is documented in [`docs/agents/persistence.md`](agents/persistence.md).
- Auto-update service and GitHub Releases release flow: `crates/update/`.
- Auto-update design and GitHub Releases packaging requirements: [`docs/auto-update.md`](auto-update.md).

## Navigation and validation

- Read [`docs/agents/structure.md`](agents/structure.md) for the complete tree and
  crate rules.
- Read [`docs/agents/code-style.md`](agents/code-style.md) before changing GPUI code.
- Read [`docs/agents/error-policy.md`](agents/error-policy.md) before changing runtime
  error handling.
- Run `python scripts/check-doc-paths.py` after changing this page.
- Historical documents are retained for design rationale and are labeled at their
  entry point; update this index instead of copying old paths into new documentation.
