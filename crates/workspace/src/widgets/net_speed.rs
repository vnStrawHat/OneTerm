//! [`NetSpeedIndicator`] — displays the network speed of the active SSH session.
//!
//! Like `DateTimeClock`: `Entity` + `Render` + `Focusable`, updated every 1s
//! via a timer. The timer spawns on the window context (`cx.spawn_in`) to fire reliably.
//!
//! Each tick:
//! 1. Find the active terminal panel in the DockArea (via `collect_tab_panels`).
//! 2. Downcast `AnyView` → `Entity<TerminalPanel>`.
//! 3. Read `network_stats()` (rx/tx bytes) from the session.
//! 4. Compute the delta against the previous tick → speed in bps (bits/s).
//!
//! Download (↓ rx) and upload (↑ tx) are shown separately, with auto-scaled units:
//! bps → Kbps → Mbps → Gbps.
//!
//! Only shown for SSH sessions (local returns `None` → indicator hidden).

use std::time::Duration;

use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, ParentElement, Render, Styled, Task, WeakEntity, Window, div,
};
use gpui_component::ActiveTheme as _;
use oneterm_terminal::NetStats;
use oneterm_ui::dock::DockArea;

/// Indicator showing the network speed (bps) of the active SSH session in the StatusBar.
pub struct NetSpeedIndicator {
    focus_handle: FocusHandle,
    dock_area: WeakEntity<DockArea>,
    /// Stats from the previous sample — used to compute the delta.
    last_stats: Option<NetStats>,
    /// Current download (rx) speed — bits/s.
    rx_bps: f64,
    /// Current upload (tx) speed — bits/s.
    tx_bps: f64,
    /// Whether the indicator is shown (active panel is an SSH terminal).
    visible: bool,
    _timer: Task<()>,
}

impl NetSpeedIndicator {
    /// Create a new indicator and start the 1s timer.
    pub fn new(
        dock_area: WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let timer = cx.spawn_in(window, async move |this, window| {
            loop {
                window
                    .background_executor()
                    .timer(Duration::from_secs(1))
                    .await;
                if let Some(this) = this.upgrade() {
                    let _ = this.update_in(window, |this, _window, cx| {
                        this.tick(cx);
                    });
                }
            }
        });
        Self {
            focus_handle,
            dock_area,
            last_stats: None,
            rx_bps: 0.0,
            tx_bps: 0.0,
            visible: false,
            _timer: timer,
        }
    }

    /// Helper to create an `Entity<Self>`.
    pub fn new_entity(
        dock_area: WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new(dock_area, window, cx))
    }

    /// Sample network stats from the active terminal panel and compute the speed.
    fn tick(&mut self, cx: &mut Context<Self>) {
        let dock_area = match self.dock_area.upgrade() {
            Some(da) => da,
            None => {
                if self.visible {
                    self.visible = false;
                    cx.notify();
                }
                return;
            }
        };

        let stats = oneterm_state::active_terminal::net_stats(&dock_area, cx);

        match (stats, self.last_stats) {
            (Some(curr), Some(prev)) => {
                // Compute delta — if the counter dropped (session changed) → saturating_sub = 0.
                let drx = curr.rx_bytes.saturating_sub(prev.rx_bytes);
                let dtx = curr.tx_bytes.saturating_sub(prev.tx_bytes);
                // bytes/s → bits/s: × 8.
                self.rx_bps = drx as f64 * 8.0;
                self.tx_bps = dtx as f64 * 8.0;
                self.last_stats = Some(curr);
                if !self.visible {
                    self.visible = true;
                }
            }
            (Some(curr), None) => {
                // First sample — no delta yet, just store it for the next tick.
                self.last_stats = Some(curr);
                self.rx_bps = 0.0;
                self.tx_bps = 0.0;
                if !self.visible {
                    self.visible = true;
                }
            }
            (None, _) => {
                // No active SSH terminal → hide.
                self.last_stats = None;
                self.rx_bps = 0.0;
                self.tx_bps = 0.0;
                if self.visible {
                    self.visible = false;
                }
            }
        }

        cx.notify();
    }

    /// Format the speed: `↓ 1.2 Kbps  ↑ 300 bps`.
    ///
    /// Download (↓) and upload (↑) have separate, auto-scaled units.
    fn formatted(&self) -> String {
        format!(
            "↓ {}  ↑ {}",
            format_speed(self.rx_bps),
            format_speed(self.tx_bps)
        )
    }
}

/// Auto-scale a bits/s speed to a suitable unit.
///
/// < 1,000 → bps (integer) · < 1,000,000 → Kbps (1 decimal) ·
/// < 1,000,000,000 → Mbps (1 decimal) · ≥ 1G → Gbps (2 decimals).
fn format_speed(bps: f64) -> String {
    if bps < 1000.0 {
        format!("{} bps", bps.round() as u64)
    } else if bps < 1_000_000.0 {
        format!("{:.1} Kbps", bps / 1000.0)
    } else if bps < 1_000_000_000.0 {
        format!("{:.1} Mbps", bps / 1_000_000.0)
    } else {
        format!("{:.2} Gbps", bps / 1_000_000_000.0)
    }
}

impl Focusable for NetSpeedIndicator {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for NetSpeedIndicator {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible {
            // Hidden completely when no SSH session is active.
            return div().id("net-speed");
        }

        div()
            .id("net-speed")
            .track_focus(&self.focus_handle)
            .child(self.formatted())
            .text_color(cx.theme().muted_foreground)
    }
}
