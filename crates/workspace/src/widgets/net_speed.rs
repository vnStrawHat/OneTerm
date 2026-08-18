//! Network speed of the active SSH session, shown in the StatusBar.
//!
//! Each 1s tick reads `network_stats()` (rx/tx bytes) from the active terminal
//! panel and computes the delta against the previous tick → speed in bps.
//!
//! Download (↓ rx) and upload (↑ tx) are shown separately, with auto-scaled
//! units: bps → Kbps → Mbps → Gbps. Only shown for SSH sessions (local returns
//! `None` → indicator hidden).

use std::time::Duration;

use gpui::{App, Entity, WeakEntity, Window};
use gpui_component::dock::DockArea;
use oneterm_terminal::NetStats;

use super::status_text::StatusText;

/// Indicator showing the network speed (bps) of the active SSH session.
pub fn net_speed(
    dock_area: WeakEntity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<StatusText> {
    let mut last_stats: Option<NetStats> = None;
    StatusText::new_entity(
        "net-speed",
        Duration::from_secs(1),
        false,
        Box::new(move |cx| {
            let dock_area = dock_area.upgrade();
            let stats =
                dock_area.and_then(|area| oneterm_state::active_terminal::net_stats(&area, cx));
            let (rx_bps, tx_bps) = sample(&mut last_stats, stats)?;
            Some(format!(
                "↓ {}  ↑ {}",
                format_speed(rx_bps),
                format_speed(tx_bps)
            ))
        }),
        window,
        cx,
    )
}

/// Fold one sample into the delta state; `None` means "no active SSH terminal".
fn sample(last_stats: &mut Option<NetStats>, stats: Option<NetStats>) -> Option<(f64, f64)> {
    match (stats, *last_stats) {
        (Some(curr), Some(prev)) => {
            // Delta — if the counter dropped (session changed) → saturating_sub = 0.
            let drx = curr.rx_bytes.saturating_sub(prev.rx_bytes);
            let dtx = curr.tx_bytes.saturating_sub(prev.tx_bytes);
            *last_stats = Some(curr);
            // bytes/s → bits/s: × 8.
            Some((drx as f64 * 8.0, dtx as f64 * 8.0))
        }
        (Some(curr), None) => {
            // First sample — no delta yet, just store it for the next tick.
            *last_stats = Some(curr);
            Some((0.0, 0.0))
        }
        (None, _) => {
            *last_stats = None;
            None
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_speed_scales_units_at_decimal_thresholds() {
        assert_eq!(format_speed(0.0), "0 bps");
        assert_eq!(format_speed(999.4), "999 bps");
        assert_eq!(format_speed(1_000.0), "1.0 Kbps");
        assert_eq!(format_speed(12_345.0), "12.3 Kbps");
        assert_eq!(format_speed(1_000_000.0), "1.0 Mbps");
        assert_eq!(format_speed(999_950_000.0), "1000.0 Mbps");
        assert_eq!(format_speed(1_000_000_000.0), "1.00 Gbps");
        assert_eq!(format_speed(2_500_000_000.0), "2.50 Gbps");
    }

    #[test]
    fn sample_needs_two_readings_and_resets_when_the_session_goes_away() {
        let stats = |rx, tx| {
            Some(NetStats {
                rx_bytes: rx,
                tx_bytes: tx,
            })
        };
        let mut last = None;
        assert_eq!(sample(&mut last, stats(100, 10)), Some((0.0, 0.0)));
        assert_eq!(sample(&mut last, stats(200, 20)), Some((800.0, 80.0)));
        // Counter dropped (session changed) → saturating_sub yields 0.
        assert_eq!(sample(&mut last, stats(50, 5)), Some((0.0, 0.0)));
        assert_eq!(sample(&mut last, None), None);
        assert!(last.is_none());
    }
}
