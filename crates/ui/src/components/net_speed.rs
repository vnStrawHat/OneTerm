//! [`NetSpeedIndicator`] — hiển thị tốc độ network của SSH session active.
//!
//! Tương tự `DateTimeClock`: `Entity` + `Render` + `Focusable`, cập nhật mỗi 1s
//! qua timer. Timer spawn trên window context (`cx.spawn_in`) để fire ổn định.
//!
//! Mỗi tick:
//! 1. Tìm active terminal panel trong DockArea (qua `collect_tab_panels`).
//! 2. Downcast `AnyView` → `Entity<TerminalPanel>`.
//! 3. Đọc `network_stats()` (rx/tx bytes) từ session.
//! 4. Tính delta so với tick trước → tốc độ bps (bits/s).
//!
//! Download (↓ rx) và upload (↑ tx) hiển thị riêng, đơn vị auto-scale:
//! bps → Kbps → Mbps → Gbps.
//!
//! Chỉ hiển thị cho SSH session (local trả về `None` → ẩn indicator).

use std::time::Duration;

use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, ParentElement, Render, Styled, Task, WeakEntity, Window, div,
};
use gpui_component::{ActiveTheme as _, dock::DockArea};
use oneterm_core::NetStats;

use crate::layout::workspace::zoom::collect_tab_panels;
use crate::views::terminal::panel::TerminalPanel;

/// Indicator hiển thị tốc độ network (bps) của SSH session active trong StatusBar.
pub struct NetSpeedIndicator {
    focus_handle: FocusHandle,
    dock_area: WeakEntity<DockArea>,
    /// Stats lần sample trước — để tính delta.
    last_stats: Option<NetStats>,
    /// Tốc độ download (rx) hiện tại — bits/s.
    rx_bps: f64,
    /// Tốc độ upload (tx) hiện tại — bits/s.
    tx_bps: f64,
    /// Có đang hiển thị không (active panel là SSH terminal).
    visible: bool,
    _timer: Task<()>,
}

impl NetSpeedIndicator {
    /// Tạo indicator mới, bắt đầu timer 1s.
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

    /// Helper tạo `Entity<Self>`.
    pub fn new_entity(
        dock_area: WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new(dock_area, window, cx))
    }

    /// Sample network stats từ active terminal panel, tính tốc độ.
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

        let stats = active_terminal_net_stats(&dock_area, cx);

        match (stats, self.last_stats) {
            (Some(curr), Some(prev)) => {
                // Tính delta — nếu counter giảm (session đổi) → saturating_sub = 0.
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
                // Lần sample đầu — chưa có delta, chỉ lưu để tick sau tính.
                self.last_stats = Some(curr);
                self.rx_bps = 0.0;
                self.tx_bps = 0.0;
                if !self.visible {
                    self.visible = true;
                }
            }
            (None, _) => {
                // Không có active SSH terminal → ẩn.
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

    /// Format tốc độ: `↓ 1.2 Kbps  ↑ 300 bps`.
    ///
    /// Download (↓) và upload (↑) có đơn vị riêng, auto-scale.
    fn formatted(&self) -> String {
        format!(
            "↓ {}  ↑ {}",
            format_speed(self.rx_bps),
            format_speed(self.tx_bps)
        )
    }
}

/// Auto-scale tốc độ bits/s sang đơn vị phù hợp.
///
/// < 1,000 → bps (nguyên) · < 1,000,000 → Kbps (1 số thập phân) ·
/// < 1,000,000,000 → Mbps (1 số thập phân) · ≥ 1G → Gbps (2 số thập phân).
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

/// Tìm active terminal panel trong DockArea, đọc network stats.
///
/// Duyệt tất cả TabPanel → tìm active panel có `panel_name == "terminal"` →
/// downcast `AnyView` → `Entity<TerminalPanel>` → gọi `network_stats()`.
fn active_terminal_net_stats(
    dock_area: &Entity<DockArea>,
    cx: &App,
) -> Option<NetStats> {
    let tab_panels = collect_tab_panels(dock_area.read(cx), cx);
    for tp in tab_panels {
        if let Some(panel) = tp.read(cx).active_panel(cx) {
            if panel.panel_name(cx) == "terminal" {
                // Downcast AnyView → Entity<TerminalPanel>.
                let any_view = panel.view();
                if let Ok(entity) = any_view.downcast::<TerminalPanel>() {
                    if let Some(stats) = entity.read(cx).network_stats(cx) {
                        return Some(stats);
                    }
                }
            }
        }
    }
    None
}

impl Focusable for NetSpeedIndicator {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for NetSpeedIndicator {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible {
            // Ẩn hoàn toàn khi không có SSH session active.
            return div().id("net-speed");
        }

        div()
            .id("net-speed")
            .track_focus(&self.focus_handle)
            .child(self.formatted())
            .text_color(cx.theme().muted_foreground)
    }
}