//! CPU and memory usage of the OneTerm process, shown in the StatusBar.
//!
//! Refreshes every 2 seconds — `sysinfo` needs at least ~1s between refreshes
//! for `cpu_usage()` to produce a meaningful delta. The `System` is seeded with
//! an initial refresh so the first tick gets a real delta instead of 0%.
//!
//! ## CPU normalisation
//!
//! `sysinfo`'s `Process::cpu_usage()` returns a **per-core** percentage
//! (100% = one full core, max = nb_cpus × 100%). This is because internally
//! `sysinfo` multiplies by `nb_cpus`:
//!
//! ```text
//! cpu_usage = 100 × (process_cpu_delta / system_cpu_delta) × nb_cpus
//! ```
//!
//! Task Manager shows the percentage relative to **total system CPU**
//! (100% = all cores). To match Task Manager, we divide by `nb_cpus`:
//!
//! ```text
//! display = cpu_usage() / nb_cpus
//! ```
//!
//! ## Memory
//!
//! `sysinfo`'s `Process::memory()` returns the **full Working Set**
//! (`WorkingSetSize` = private + shared pages like DLLs). Task Manager's
//! default "Memory" column shows the **private working set** (shared excluded).
//! We use `virtual_memory()` (= `PrivateUsage` = private committed bytes) which
//! is closer to what Task Manager shows, though not identical (includes paged-out
//! memory that private working set excludes).
//!
//! Format: `CPU 12.3%  MEM 45.2 MB`

use std::time::Duration;

use gpui::{App, Entity, Window};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use super::status_text::StatusText;

/// Only the two fields the indicator shows — the default kind also walks
/// disk usage, the exe path, and (on Windows) every process thread (PERF-28).
fn refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing().with_cpu().with_memory()
}

/// Indicator showing the CPU and memory usage of the OneTerm process.
pub fn resource(window: &mut Window, cx: &mut App) -> Entity<StatusText> {
    // Resolve the current PID — fails only on unsupported platforms. Degrade
    // to an idle indicator rather than panicking if it is unavailable.
    let pid: Option<Pid> = match sysinfo::get_current_pid() {
        Ok(pid) => Some(pid),
        Err(error) => {
            log::warn!("sysinfo: failed to resolve current PID: {error}");
            None
        }
    };

    // Seed the System with an initial refresh so the first timer tick gets a
    // real CPU delta instead of 0%. This also lazily initialises the CPU list
    // (needed for nb_cpus normalisation).
    let mut sys = System::new();
    if let Some(pid) = pid {
        sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, refresh_kind());
    }

    StatusText::new_entity(
        "resource-indicator",
        Duration::from_secs(2),
        false,
        Box::new(move |_| {
            let pid = pid?;
            sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, refresh_kind());
            let process = sys.process(pid)?;
            // sysinfo returns per-core CPU (100% = 1 core). Divide by nb_cpus
            // to get the total-system percentage that Task Manager shows.
            let nb_cpus = sys.cpus().len().max(1) as f32;
            // Use virtual_memory (PrivateUsage on Windows = private committed
            // bytes) instead of memory() (full working set including shared
            // DLLs) — closer to Task Manager's default "Memory" column.
            Some(format!(
                "CPU {:.1}%  MEM {}",
                process.cpu_usage() / nb_cpus,
                format_memory(process.virtual_memory())
            ))
        }),
        window,
        cx,
    )
}

/// Auto-scale bytes to a human-readable string.
///
/// < 1 KB → `B` (integer) · < 1 MB → `KB` (1 decimal) ·
/// < 1 GB → `MB` (1 decimal) · ≥ 1 GB → `GB` (2 decimals).
fn format_memory(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::format_memory;

    #[test]
    fn format_memory_scales_units_at_binary_thresholds() {
        assert_eq!(format_memory(0), "0 B");
        assert_eq!(format_memory(1023), "1023 B");
        assert_eq!(format_memory(1024), "1.0 KB");
        assert_eq!(format_memory(1536), "1.5 KB");
        assert_eq!(format_memory(1024 * 1024), "1.0 MB");
        assert_eq!(format_memory(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_memory(3 * 1024 * 1024 * 1024 / 2), "1.50 GB");
    }
}
