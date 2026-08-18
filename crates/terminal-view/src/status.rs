//! Status-bar metric extractors for the active terminal panel.
//!
//! Registered into the low `oneterm-state` crate at init so the shell's
//! breadcrumb / network-speed widgets can read the active terminal's metrics
//! without depending on `TerminalPanel`. This keeps the shell feature-agnostic.

use std::sync::Arc;

use gpui::{App, Entity, Window};
use gpui_component::dock::{DockArea, PanelView};
use oneterm_core::ShellKind;
use oneterm_state::AppServicesBuilder;
use oneterm_state::active_terminal::ActiveTerminalMetricsProvider;
use oneterm_state::dock_util::collect_tab_panels;
use oneterm_state::panel_names;
use oneterm_terminal::NetStats;

use crate::panel::{PanelSpec, TerminalPanel};

/// The terminal panels that are the active tab of a tab panel, in dock order.
fn active_terminal_panels(dock_area: &Entity<DockArea>, cx: &App) -> Vec<Entity<TerminalPanel>> {
    collect_tab_panels(dock_area.read(cx), cx)
        .into_iter()
        .filter_map(|tp| tp.read(cx).active_panel(cx))
        .filter(|panel| panel.panel_name(cx) == panel_names::TERMINAL)
        .filter_map(|panel| panel.view().downcast::<TerminalPanel>().ok())
        .collect()
}

/// Breadcrumb label (cwd + foreground process) of the active terminal panel.
fn active_breadcrumb(dock_area: &Entity<DockArea>, cx: &App) -> Option<String> {
    let panel = active_terminal_panels(dock_area, cx).into_iter().next()?;
    panel.read(cx).breadcrumb_label(cx)
}

/// Network stats (rx/tx bytes) of the active terminal panel.
fn active_net_stats(dock_area: &Entity<DockArea>, cx: &App) -> Option<NetStats> {
    active_terminal_panels(dock_area, cx)
        .into_iter()
        .find_map(|panel| panel.read(cx).network_stats(cx))
}

/// Contribute the active-terminal metric extractors to `AppServices`.
pub(crate) fn register_status_metrics(cx: &mut App) {
    AppServicesBuilder::pending(cx)
        .and_then(|builder| {
            builder.active_terminal_metrics(ActiveTerminalMetricsProvider {
                breadcrumb: active_breadcrumb,
                net_stats: active_net_stats,
            })
        })
        .expect("terminal feature must contribute its status metrics once during init");
}

/// Construct a terminal panel bound to a specific shell kind.
///
/// Registered as the `new_terminal_with_shell` workspace command so the shell
/// can honor `AddPanelWithShell` without depending on `TerminalPanel`.
pub fn new_terminal_with_shell_cmd(
    shell: ShellKind,
    window: &mut Window,
    cx: &mut App,
) -> Arc<dyn PanelView> {
    Arc::new(TerminalPanel::open(PanelSpec::Shell(shell), window, cx))
}

/// Toggle the in-terminal search bar on the active terminal panel.
///
/// Registered as the `find_in_active_terminal` workspace command (the shell's
/// `Find` handler delegates here without depending on `TerminalPanel`).
pub fn find_in_active_terminal(dock_area: &Entity<DockArea>, window: &mut Window, cx: &mut App) {
    let Some(panel) = active_terminal_panels(dock_area, cx).into_iter().next() else {
        return;
    };
    panel.update(cx, |tp, cx| {
        if let Some(view) = tp.active_view() {
            view.update(cx, |v, cx| v.toggle_search(window, cx));
        }
    });
}
