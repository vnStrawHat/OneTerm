//! [`ResourceIndicator`] — displays CPU and memory usage of the OneTerm process.
//!
//! Like `DateTimeClock` / `NetSpeedIndicator`: `Entity` + `Render` + `Focusable`,
//! updated every 2s via a timer. The timer spawns on the window context
//! (`cx.spawn_in`) to fire reliably.
//!
//! Uses the `sysinfo` crate to read the current process's CPU usage (%) and
//! memory (bytes). CPU usage is computed as a delta between refreshes, so the
//! first tick always reads 0% — the `System` is initialised in `new()` with an
//! initial refresh to seed the baseline.
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

use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, ParentElement, Render, Styled, Task, Window, div,
};
use gpui_component::ActiveTheme as _;
use sysinfo::{Pid, ProcessesToUpdate, System};

/// Indicator showing the CPU and memory usage of the OneTerm process in the
/// StatusBar.
///
/// Refreshes every 2 seconds — `sysinfo` needs at least ~1s between refreshes
/// for `cpu_usage()` to produce a meaningful delta.
pub struct ResourceIndicator {
    focus_handle: FocusHandle,
    /// `sysinfo` system handle — kept across ticks so CPU deltas are meaningful.
    sys: System,
    /// PID of the current process (OneTerm). `None` on platforms where `sysinfo`
    /// cannot resolve the current PID — the indicator then reports 0% / 0 bytes.
    pid: Option<Pid>,
    /// Latest CPU usage (%) normalised to total system (100% = all cores) —
    /// matches Task Manager. 0.0 on the first tick.
    cpu_usage: f32,
    /// Latest private memory usage (bytes) — closer to Task Manager's "Memory".
    memory: u64,
    _timer: Task<()>,
}

impl ResourceIndicator {
    /// Create a new indicator, seed the `sysinfo` baseline, and start the 2s timer.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        // Resolve the current PID — fails only on unsupported platforms. Degrade
        // to an idle indicator rather than panicking if it is unavailable.
        let pid = match sysinfo::get_current_pid() {
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
            sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        }

        let timer = cx.spawn_in(window, async move |this, window| {
            loop {
                window
                    .background_executor()
                    .timer(Duration::from_secs(2))
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
            sys,
            pid,
            cpu_usage: 0.0,
            memory: 0,
            _timer: timer,
        }
    }

    /// Helper to create an `Entity<Self>`.
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    /// Refresh process stats from `sysinfo` and read CPU/memory.
    fn tick(&mut self, cx: &mut Context<Self>) {
        let Some(pid) = self.pid else {
            return;
        };

        self.sys
            .refresh_processes(ProcessesToUpdate::Some(&[pid]), true);

        if let Some(process) = self.sys.process(pid) {
            // sysinfo returns per-core CPU (100% = 1 core). Divide by nb_cpus
            // to get the total-system percentage that Task Manager shows.
            let nb_cpus = self.sys.cpus().len().max(1) as f32;
            self.cpu_usage = process.cpu_usage() / nb_cpus;

            // Use virtual_memory (PrivateUsage on Windows = private committed
            // bytes) instead of memory() (full working set including shared
            // DLLs) — closer to Task Manager's default "Memory" column.
            self.memory = process.virtual_memory();
        }

        cx.notify();
    }

    /// Format the display: `CPU 12.3%  MEM 45.2 MB`.
    fn formatted(&self) -> String {
        format!(
            "CPU {:.1}%  MEM {}",
            self.cpu_usage,
            format_memory(self.memory)
        )
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

impl Focusable for ResourceIndicator {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ResourceIndicator {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("resource-indicator")
            .track_focus(&self.focus_handle)
            .child(self.formatted())
            .text_color(cx.theme().muted_foreground)
    }
}
