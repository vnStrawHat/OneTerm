//! Injectable "active terminal metrics" provider.
//!
//! The status-bar widgets (breadcrumb, network speed) live in the shell but must
//! not depend on the terminal feature crate. Instead, the terminal feature
//! registers extractor functions here at init; the widgets call them each tick.
//! This preserves the exact polling behavior while removing the shell → feature
//! type dependency.

use gpui::{App, Entity, Global};
use gpui_component::dock::DockArea;
use oneterm_terminal::NetStats;

/// Extractor functions provided by the terminal feature crate.
#[derive(Clone, Copy)]
pub struct ActiveTerminalMetricsProvider {
    /// Breadcrumb label (cwd + foreground process) of the active terminal panel.
    pub breadcrumb: fn(&Entity<DockArea>, &App) -> Option<String>,
    /// Network stats (rx/tx bytes) of the active terminal panel.
    pub net_stats: fn(&Entity<DockArea>, &App) -> Option<NetStats>,
}

impl Global for ActiveTerminalMetricsProvider {}

/// Register the extractor functions (called from the terminal feature's `init`).
pub fn set_provider(cx: &mut App, provider: ActiveTerminalMetricsProvider) {
    cx.set_global(provider);
}

/// Breadcrumb label of the active terminal panel, or `None` if unavailable
/// (no provider registered yet, or no active terminal with a breadcrumb).
pub fn breadcrumb(dock_area: &Entity<DockArea>, cx: &App) -> Option<String> {
    cx.try_global::<ActiveTerminalMetricsProvider>()
        .and_then(|p| (p.breadcrumb)(dock_area, cx))
}

/// Network stats of the active terminal panel, or `None` if unavailable.
pub fn net_stats(dock_area: &Entity<DockArea>, cx: &App) -> Option<NetStats> {
    cx.try_global::<ActiveTerminalMetricsProvider>()
        .and_then(|p| (p.net_stats)(dock_area, cx))
}
