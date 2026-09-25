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
//! `MEM` is the number Task Manager's "Memory" column shows: the **private
//! working set** (`PROCESS_MEMORY_COUNTERS_EX2::PrivateWorkingSetSize`, Windows
//! 10 21H1+), read with `GetProcessMemoryInfo` because `sysinfo` does not expose
//! it. Where it is unavailable the full working set (`sysinfo` `memory()`,
//! shared pages included) stands in. On Linux and macOS `memory()` is the
//! resident set size. Commit (`virtual_memory()`, `PrivateUsage`) is not shown:
//! it counts pages that were never touched (`US-0137`).
//!
//! Format: `CPU 12.3%  MEM 45.2 MB`

use std::time::Duration;

use gpui::{App, Entity, Window};
use gpui_component::{Icon, IconName};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use super::status_text::{Label, Presentation, Shorten, StatusText};

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
        Presentation {
            icon: Some(Icon::new(IconName::Cpu)),
            // `MEM 577.0 MB` is a value and its unit: never shortened.
            copyable: false,
            shorten: Shorten::Never,
        },
        Box::new(move |_| {
            let pid = pid?;
            sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, refresh_kind());
            let process = sys.process(pid)?;
            // sysinfo returns per-core CPU (100% = 1 core). Divide by nb_cpus
            // to get the total-system percentage that Task Manager shows.
            let nb_cpus = sys.cpus().len().max(1) as f32;
            Some(Label::from(format!(
                "CPU {:.1}%  MEM {}",
                process.cpu_usage() / nb_cpus,
                format_memory(displayed_memory(private_working_set(), process.memory()))
            )))
        }),
        window,
        cx,
    )
}

/// The figure `MEM` shows: the private working set when the OS gave one,
/// otherwise the resident figure (`sysinfo` `memory()`). A live process never
/// has a private working set of 0, so 0 means the field was not filled.
fn displayed_memory(private_working_set: Option<u64>, resident: u64) -> u64 {
    private_working_set
        .filter(|&bytes| bytes > 0)
        .unwrap_or(resident)
}

/// This process's private working set, or `None` where the OS does not give it.
fn private_working_set() -> Option<u64> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;

        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
        // SAFETY: all-zero is a valid value of this plain-integer struct.
        let mut counters: PROCESS_MEMORY_COUNTERS_EX2 = unsafe { std::mem::zeroed() };
        counters.cb = size;
        // SAFETY: `GetCurrentProcess` returns a pseudo-handle that needs no close;
        // the pointer is to a live, writable struct of exactly `size` bytes. A
        // Windows older than 10 21H1 leaves `PrivateWorkingSetSize` at 0, which
        // `displayed_memory` treats as unavailable.
        let ok = unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                (&raw mut counters).cast::<PROCESS_MEMORY_COUNTERS>(),
                size,
            )
        };
        (ok != 0).then_some(counters.PrivateWorkingSetSize as u64)
    }
    #[cfg(not(windows))]
    {
        None
    }
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
    use super::{displayed_memory, format_memory};

    #[test]
    fn displayed_memory_prefers_the_private_working_set() {
        assert_eq!(displayed_memory(Some(49 << 20), 94 << 20), 49 << 20);
    }

    #[test]
    fn displayed_memory_falls_back_to_resident_when_unavailable() {
        // Not Windows, or the call failed.
        assert_eq!(displayed_memory(None, 94 << 20), 94 << 20);
        // Windows before 10 21H1 leaves the EX2 field at 0.
        assert_eq!(displayed_memory(Some(0), 94 << 20), 94 << 20);
    }

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
