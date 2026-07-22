//! Status-bar metric extractors for the active terminal panel.
//!
//! Registered into the low `oneterm-state` crate at init so the shell's
//! breadcrumb / network-speed widgets can read the active terminal's metrics
//! without depending on `TerminalPanel`. This keeps the shell feature-agnostic.

use std::sync::Arc;

use gpui::{App, Entity, Window};
use gpui_component::dock::{DockArea, PanelView};
use oneterm_core::ShellKind;
use oneterm_state::active_terminal::ActiveTerminalMetricsProvider;
use oneterm_state::dock_util::collect_tab_panels;
use oneterm_terminal::NetStats;

use crate::panel::TerminalPanel;

/// Breadcrumb label (cwd + foreground process) of the active terminal panel.
fn active_breadcrumb(dock_area: &Entity<DockArea>, cx: &App) -> Option<String> {
    for tp in collect_tab_panels(dock_area.read(cx), cx) {
        if let Some(panel) = tp.read(cx).active_panel(cx) {
            if panel.panel_name(cx) == "terminal" {
                if let Ok(entity) = panel.view().downcast::<TerminalPanel>() {
                    return entity.read(cx).breadcrumb_label(cx);
                }
            }
        }
    }
    None
}

/// Network stats (rx/tx bytes) of the active terminal panel.
fn active_net_stats(dock_area: &Entity<DockArea>, cx: &App) -> Option<NetStats> {
    for tp in collect_tab_panels(dock_area.read(cx), cx) {
        if let Some(panel) = tp.read(cx).active_panel(cx) {
            if panel.panel_name(cx) == "terminal" {
                if let Ok(entity) = panel.view().downcast::<TerminalPanel>() {
                    if let Some(stats) = entity.read(cx).network_stats(cx) {
                        return Some(stats);
                    }
                }
            }
        }
    }
    None
}

/// Register the active-terminal metric extractors with `oneterm-state`.
pub fn register_status_metrics(cx: &mut App) {
    oneterm_state::active_terminal::set_provider(
        cx,
        ActiveTerminalMetricsProvider {
            breadcrumb: active_breadcrumb,
            net_stats: active_net_stats,
        },
    );
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
    Arc::new(TerminalPanel::new_with_shell_entity(shell, window, cx))
}

/// Toggle the in-terminal search bar on the active terminal panel.
///
/// Registered as the `find_in_active_terminal` workspace command (the shell's
/// `Find` handler delegates here without depending on `TerminalPanel`).
pub fn find_in_active_terminal(dock_area: &Entity<DockArea>, window: &mut Window, cx: &mut App) {
    for tp in collect_tab_panels(dock_area.read(cx), cx) {
        if let Some(panel) = tp.read(cx).active_panel(cx) {
            if panel.panel_name(cx) == "terminal" {
                if let Ok(entity) = panel.view().downcast::<TerminalPanel>() {
                    entity.update(cx, |tp, cx| {
                        if let Some(view) = tp.active_view() {
                            view.update(cx, |v, cx| {
                                if v.search_active {
                                    v.close_search(cx);
                                } else {
                                    v.open_search(window, cx);
                                }
                            });
                        }
                    });
                    return;
                }
            }
        }
    }
}
