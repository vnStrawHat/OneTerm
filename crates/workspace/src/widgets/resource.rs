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
//! 10 22H2 or Windows 11 22H2 with the September 2023 cumulative update, and
//! later), read with `GetProcessMemoryInfo` because `sysinfo` does not expose
//! it. Where it is unavailable the full working set (`sysinfo` `memory()`,
//! shared pages included) stands in, so an older Windows reads higher: about
//! 136 MB idle instead of 51. On Linux and macOS `memory()` is the
//! resident set size. Commit (`virtual_memory()`, `PrivateUsage`) is not the
//! item's figure: it counts pages that were never touched (`US-0137`).
//!
//! Format: `CPU 12.3%  MEM 45.2 MB`
//!
//! ## Hover table
//!
//! The same sample also fills a table the item shows on hover (`US-0148`): every
//! memory figure the OS gives (see [`details`] for the per-platform names), the
//! CPU figure with its core count, CPU time, thread count and uptime. Nothing is
//! read when the tooltip opens; it shows the latest sample.

use std::time::Duration;

use gpui::{App, Entity, Window};
use gpui_component::{Icon, IconName};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use super::status_text::{Label, Presentation, Section, Shorten, StatusText};

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
            let cores = sys.cpus().len().max(1);
            let os = os_memory_counters();
            let sample = Sample {
                cpu_percent: process.cpu_usage() / cores as f32,
                cores,
                cpu_time: Duration::from_millis(process.accumulated_cpu_time()),
                uptime: Duration::from_secs(process.run_time()),
                threads: process
                    .tasks()
                    .map(|tasks| tasks.len())
                    .or_else(thread_count),
                private_working_set: os.map(|c| c.private_working_set).filter(|&b| b > 0),
                resident: process.memory(),
                virtual_or_commit: process.virtual_memory(),
                peak_working_set: os.map(|c| c.peak_working_set).filter(|&b| b > 0),
            };
            let mut label = Label::from(format!(
                "CPU {:.1}%  MEM {}",
                sample.cpu_percent,
                format_memory(displayed_memory(
                    sample.private_working_set,
                    sample.resident
                ))
            ));
            label.details = details(&sample);
            Some(label)
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

/// One 2 s reading of this process, everything the item and its table show.
struct Sample {
    /// Of all logical cores, as Task Manager shows it.
    cpu_percent: f32,
    cores: usize,
    /// User plus kernel.
    cpu_time: Duration,
    uptime: Duration,
    threads: Option<usize>,
    /// Windows only (`PrivateWorkingSetSize`).
    private_working_set: Option<u64>,
    /// `sysinfo` `memory()`: the working set on Windows, RSS elsewhere.
    resident: u64,
    /// `sysinfo` `virtual_memory()`: commit (`PrivateUsage`) on Windows, the
    /// virtual size elsewhere.
    virtual_or_commit: u64,
    /// Windows only (`PeakWorkingSetSize`).
    peak_working_set: Option<u64>,
}

/// What `sysinfo` `memory()` and `virtual_memory()` are called on this OS.
const RESIDENT_NAME: &str = if cfg!(windows) {
    "Working set"
} else {
    "Resident (RSS)"
};
const VIRTUAL_NAME: &str = if cfg!(windows) {
    "Commit (private bytes)"
} else {
    "Virtual size"
};

/// The hover table, in a fixed order. A figure the OS does not give is left out,
/// except the thread count, whose absence is shown as `n/a`.
fn details(sample: &Sample) -> Vec<Section> {
    let mut memory = Vec::new();
    if let Some(bytes) = sample.private_working_set {
        memory.push(("Private working set", format_memory(bytes)));
    }
    memory.push((RESIDENT_NAME, format_memory(sample.resident)));
    memory.push((VIRTUAL_NAME, format_memory(sample.virtual_or_commit)));
    if let Some(bytes) = sample.peak_working_set {
        memory.push(("Peak working set", format_memory(bytes)));
    }
    let cpu = vec![
        (
            "Usage",
            format!(
                "{:.1}% of {} logical cores",
                sample.cpu_percent, sample.cores
            ),
        ),
        ("CPU time (user + kernel)", format_duration(sample.cpu_time)),
        (
            "Threads",
            sample
                .threads
                .map_or_else(|| "n/a".to_string(), |n| n.to_string()),
        ),
        ("Uptime", format_duration(sample.uptime)),
    ];
    vec![
        Section {
            title: "Memory",
            rows: memory,
        },
        Section {
            title: "CPU",
            rows: cpu,
        },
    ]
}

/// `12.4 s` under a minute, `4m 05s` under an hour, `3h 07m` from there.
fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{:.1} s", duration.as_secs_f64())
    } else if secs < 3600 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h {:02}m", secs / 3600, secs % 3600 / 60)
    }
}

/// The two `PROCESS_MEMORY_COUNTERS_EX2` fields `sysinfo` does not give.
#[derive(Clone, Copy)]
struct OsMemoryCounters {
    private_working_set: u64,
    peak_working_set: u64,
}

/// This process's private and peak working set, or `None` where the OS does not
/// give them.
fn os_memory_counters() -> Option<OsMemoryCounters> {
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
        // Windows without the EX2 struct (before 10 22H2 / 11 22H2 with the
        // September 2023 update) either rejects the larger `cb` (`ok == 0`, so
        // `None`) or fills only the older prefix and leaves
        // `PrivateWorkingSetSize` at 0; `displayed_memory` treats both as
        // unavailable and shows the working set.
        let ok = unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                (&raw mut counters).cast::<PROCESS_MEMORY_COUNTERS>(),
                size,
            )
        };
        (ok != 0).then_some(OsMemoryCounters {
            private_working_set: counters.PrivateWorkingSetSize as u64,
            peak_working_set: counters.PeakWorkingSetSize as u64,
        })
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// This process's thread count where `sysinfo` has none (it has one on Linux
/// only): the own entry of a process snapshot on Windows, `None` elsewhere.
///
/// ponytail: walks every process like `sysinfo`'s own refresh does, once per
/// 2 s sample; `NtQueryInformationProcess` if that ever shows up in a profile.
fn thread_count() -> Option<usize> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        };

        let own = std::process::id();
        // SAFETY: the snapshot handle is closed below and not used after.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        // SAFETY: all-zero is a valid value of this plain-data struct.
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut threads = None;
        // SAFETY: `snapshot` is valid and `entry` has its `dwSize` set.
        let mut has_entry = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while has_entry {
            if entry.th32ProcessID == own {
                threads = Some(entry.cntThreads as usize);
                break;
            }
            // SAFETY: same valid snapshot and entry as above.
            has_entry = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        // SAFETY: `snapshot` came from `CreateToolhelp32Snapshot` and is not used again.
        unsafe {
            CloseHandle(snapshot);
        }
        threads
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
    use std::time::Duration;

    use super::{
        RESIDENT_NAME, Sample, VIRTUAL_NAME, details, displayed_memory, format_duration,
        format_memory,
    };

    fn sample() -> Sample {
        Sample {
            cpu_percent: 1.25,
            cores: 16,
            cpu_time: Duration::from_millis(12_400),
            uptime: Duration::from_secs(245),
            threads: Some(31),
            private_working_set: Some(49 << 20),
            resident: 94 << 20,
            virtual_or_commit: 182 << 20,
            peak_working_set: Some(101 << 20),
        }
    }

    /// `(section, name, value)` for every row, in order.
    fn rows(sample: &Sample) -> Vec<(&'static str, &'static str, String)> {
        details(sample)
            .into_iter()
            .flat_map(|section| {
                section
                    .rows
                    .into_iter()
                    .map(move |(name, value)| (section.title, name, value))
            })
            .collect()
    }

    #[test]
    fn the_table_lists_every_field_in_a_fixed_order() {
        assert_eq!(
            rows(&sample()),
            vec![
                ("Memory", "Private working set", "49.0 MB".to_string()),
                ("Memory", RESIDENT_NAME, "94.0 MB".to_string()),
                ("Memory", VIRTUAL_NAME, "182.0 MB".to_string()),
                ("Memory", "Peak working set", "101.0 MB".to_string()),
                ("CPU", "Usage", "1.2% of 16 logical cores".to_string()),
                ("CPU", "CPU time (user + kernel)", "12.4 s".to_string()),
                ("CPU", "Threads", "31".to_string()),
                ("CPU", "Uptime", "4m 05s".to_string()),
            ]
        );
    }

    #[test]
    fn figures_the_os_does_not_give_are_left_out_or_marked() {
        // Linux and macOS: no private or peak working set; macOS: no threads.
        let sample = Sample {
            private_working_set: None,
            peak_working_set: None,
            threads: None,
            ..sample()
        };
        let names: Vec<_> = rows(&sample).into_iter().map(|(_, n, v)| (n, v)).collect();
        assert_eq!(names[0].0, RESIDENT_NAME);
        assert_eq!(names[1].0, VIRTUAL_NAME);
        assert_eq!(names[2].0, "Usage");
        assert_eq!(names[4], ("Threads", "n/a".to_string()));
    }

    #[test]
    fn durations_scale_to_their_size() {
        assert_eq!(format_duration(Duration::from_millis(0)), "0.0 s");
        assert_eq!(format_duration(Duration::from_millis(59_940)), "59.9 s");
        assert_eq!(format_duration(Duration::from_secs(60)), "1m 00s");
        assert_eq!(format_duration(Duration::from_secs(3599)), "59m 59s");
        assert_eq!(
            format_duration(Duration::from_secs(3 * 3600 + 7 * 60 + 9)),
            "3h 07m"
        );
    }

    #[test]
    fn displayed_memory_prefers_the_private_working_set() {
        assert_eq!(displayed_memory(Some(49 << 20), 94 << 20), 49 << 20);
    }

    #[test]
    fn displayed_memory_falls_back_to_resident_when_unavailable() {
        // Not Windows, or the call failed.
        assert_eq!(displayed_memory(None, 94 << 20), 94 << 20);
        // A Windows without the EX2 struct (before 10 22H2 / 11 22H2 with the
        // September 2023 update) may leave the field at 0.
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
