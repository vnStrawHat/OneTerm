//! Injectable "active terminal metrics" provider.
//!
//! The status-bar widgets (breadcrumb, network speed) live in the shell but must
//! not depend on the terminal feature crate. Instead, the terminal feature
//! contributes extractor functions to [`crate::AppServices`] at init; the
//! widgets call them each tick. This preserves the exact polling behavior while
//! removing the shell → feature type dependency.

use gpui::{App, Entity};
use gpui_component::dock::DockArea;
use oneterm_terminal::NetStats;

use crate::AppServices;

/// Extractor functions provided by the terminal feature crate.
#[derive(Clone, Copy)]
pub struct ActiveTerminalMetricsProvider {
    /// Breadcrumb label (cwd + foreground process) of the active terminal panel.
    pub breadcrumb: fn(&Entity<DockArea>, &App) -> Option<String>,
    /// Network stats (rx/tx bytes) of the active terminal panel.
    pub net_stats: fn(&Entity<DockArea>, &App) -> Option<NetStats>,
}

/// Breadcrumb label of the active terminal panel, or `None` if no active
/// terminal has a breadcrumb.
pub fn breadcrumb(dock_area: &Entity<DockArea>, cx: &App) -> Option<String> {
    (AppServices::active_terminal_metrics(cx).breadcrumb)(dock_area, cx)
}

/// Network stats of the active terminal panel, or `None` if unavailable.
pub fn net_stats(dock_area: &Entity<DockArea>, cx: &App) -> Option<NetStats> {
    (AppServices::active_terminal_metrics(cx).net_stats)(dock_area, cx)
}
