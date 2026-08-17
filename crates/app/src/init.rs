//! Application initialization: OneTerm global state + feature registration.
//!
//! This replaces the former `gpui_component::init` aggregator. The app crate is the
//! only place that knows about every feature crate, so it wires them together:
//! it initializes the shared globals, calls each feature's `init` (which
//! registers its dock panels + feature globals), and assembles the workspace
//! command registry that the feature-agnostic shell (`oneterm-workspace`) uses
//! to drive the features without depending on them.

use gpui::App;

use oneterm_settings::{TerminalSettings, UiConfig};
use oneterm_state::commands::WorkspaceCommands;
use oneterm_state::{AppServices, AppState, GlobalCompletionHistory};

/// Initialize OneTerm's UI layer: globals + feature registration + commands.
///
/// `gpui_component::init(cx)` (called in [`crate::run`]) already initializes the
/// theme, dock, root, and `PanelRegistry`. This runs afterwards.
pub(crate) fn init(cx: &mut App) {
    // Shared globals (each `init` is idempotent: it installs once and later
    // calls are no-ops). `UiConfig` first so the saved theme/font apply in
    // `theme::init`; the `ui_config.json` theme observer is registered after
    // the theme init so startup mutations do not trigger a write.
    UiConfig::init(cx);
    oneterm_theme::theme::init(cx);
    UiConfig::observe_theme(cx);
    AppState::init(cx);
    TerminalSettings::init(cx);
    // Process-global, cross-tab, non-persistent command history (memory source).
    GlobalCompletionHistory::init(cx);

    // Feature inits — each registers its dock panel(s) + feature globals.
    // Terminal: "terminal" + "terminal-settings" panels + status-bar metrics.
    oneterm_terminal_view::init(cx);
    // SFTP: "sftp" panel.
    oneterm_sftp_ui::init(cx);
    // Session: SSH session store global + "session" panel.
    oneterm_session_ui::init(cx);
    // Agent: the global AgentRegistry (folded OSC 9;7 model behind the Agent
    // Panel). Ensures the registry exists so terminals can fold into it even
    // before the Agent panel is first opened.
    oneterm_agent_ui::init(cx);
    // Settings/About update controls and release-build startup update checks.
    oneterm_settings_ui::init(cx);

    // SSH Client right-dock panel (Session + SFTP) — registered here because it
    // composes two feature crates, which only the omniscient `app` crate may
    // depend on together (R9).
    crate::ssh_client_panel::init(cx);

    // Agent Mode right-dock panel (placeholder for now) — same reason as the
    // SSH Client panel: it may later compose feature crates, so it lives in the
    // omniscient `app` crate (R9).
    crate::agent_panel::init(cx);

    // Seal all cross-feature services into one composition-root bundle. The
    // feature inits above contributed their hooks (active-terminal metrics,
    // agent focuser) to the pending `AppServicesBuilder`; each command callback
    // belongs to its feature, while the shell consumes only this state API.
    AppServices::install(
        cx,
        crate::session_factory::build(),
        WorkspaceCommands {
            new_terminal_with_shell: oneterm_terminal_view::new_terminal_with_shell_cmd,
            open_new_session_dialog: oneterm_session_ui::open_quick_connect_dialog,
            open_duplicate_ssh_dialog: oneterm_session_ui::open_duplicate_ssh_dialog,
            open_settings: oneterm_settings_ui::open_settings,
            open_about: oneterm_settings_ui::open_about_dialog,
            find_in_active_terminal: oneterm_terminal_view::find_in_active_terminal,
            setup_key_bindings: oneterm_settings_ui::setup_key_bindings,
        },
    )
    .expect("application services must be registered exactly once with every feature contribution");
    AppServices::validate(cx).expect("application services must be available");
}
