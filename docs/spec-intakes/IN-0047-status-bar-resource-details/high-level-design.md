# High-Level Design: status bar resource details

Intake: IN-0047
Lane: normal
Date: 2026-09-28

## Idea

The CPU/MEM item already samples its own process every 2 s. That sample now keeps a few
more figures it reads anyway (or reads with one cheap call), and the item shows them in a
two-column table in the kit's tooltip on hover. Nothing is read when the tooltip opens; the
table tracks the item's own 2 s cadence for as long as it stays open (2026-09-28 acceptance
rework — see [`US-0148`](US-0148-resource-tooltip.md) — the owner rejected the first cut,
which froze at the sample the tooltip opened with).

## Diagram

```text
 2 s timer (StatusText, unchanged)
      |
      v
 resource sampler ---- sysinfo refresh of the own pid (cpu, memory, run time; tasks on Linux)
      |           \--- GetProcessMemoryInfo EX2 (Windows, already there): private WS + peak WS
      |            \-- CreateToolhelp32Snapshot (Windows): the own entry's thread count
      v
 Sample { cpu %, cores, cpu time, uptime, threads, memory figures }
      |                          |
      v                          v
 Label "CPU 1.2%  MEM 49.3 MB"   details: Memory section, CPU section
      \______________ StatusText stores both; re-renders only on change
                                 |
                           hover -> kit Tooltip::element mounts a DetailsTooltip
                                    entity, which observes StatusText and
                                    re-renders the table on every tick while open
```

The kit calls a tooltip's builder closure once, when it is first shown (`gpui-base`
`tooltip.rs`: `TooltipOverlay::request_show`), not on every repaint — a `Vec<Section>`
captured in that closure would freeze there. The fix nests a small entity
(`DetailsTooltip`) as the tooltip's content instead of a snapshot: it holds an
`Entity<StatusText>` and calls `cx.observe(&source, |_, _, cx| cx.notify())` in its
constructor, so it re-renders (reading `StatusText::details()` fresh) on every tick the
source entity notifies on — the same `cx.notify()` `StatusText::tick` already calls,
whether or not the tooltip is open. No new timer, no per-frame read: the tooltip only ever
reacts to the existing 2 s sample. This mirrors the codebase's existing pattern for a small
live view nested in static content (`crates/settings-ui/src/about.rs`
`AboutUpdateControls`, `cx.observe` + `.detach()`).

## UI Wireframe

```text
                                        +--------------------------------------+
                                        | Memory                               |
                                        | Private working set          49.3 MB |
                                        | Working set                  93.8 MB |
                                        | Commit (private bytes)      182.2 MB |
                                        | Peak working set            101.0 MB |
                                        | CPU                                  |
                                        | Usage             1.2% of 16 cores   |
                                        | CPU time (user + kernel)      12.4 s |
                                        | Threads                           31 |
                                        | Uptime                        4m 05s |
                                        +--------------------------------------+
 status bar: [clock] ...  [net]  [cpu] CPU 1.2%  MEM 49.3 MB  [dock]
```

Section titles and values in `popover.foreground` (the kit tooltip's text colour), row
names in `muted.foreground`, on the kit tooltip's `popover` fill. Both pairs are already in
the contrast gate's `SURFACES` table, so no surface is added.

## Data Flow

1. Every 2 s the sampler refreshes the own pid in `sysinfo` (CPU and memory, as before).
2. On Windows it reads `PROCESS_MEMORY_COUNTERS_EX2` once (as before) and keeps
   `PeakWorkingSetSize` next to `PrivateWorkingSetSize`, and walks one process snapshot
   for the own entry's `cntThreads`. On Linux `sysinfo` already refreshes the task list,
   whose length is the thread count; macOS has none (`n/a`).
3. Pure functions turn the figures into the label and the table rows; `StatusText` stores
   both and re-renders only when either changed.
4. On hover `StatusText` builds the kit tooltip, mounting a `DetailsTooltip` entity that
   observes `StatusText` and re-reads its stored rows on every tick while the tooltip stays
   open (2026-09-28 acceptance rework).

Platform matrix:

| Row | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Private working set | `PrivateWorkingSetSize` (row omitted where 0 or the call fails) | omitted | omitted |
| Working set / Resident (RSS) | `WorkingSetSize` (`sysinfo` `memory()`) | RSS | RSS |
| Commit (private bytes) / Virtual size | `PrivateUsage` (`sysinfo` `virtual_memory()`) | virtual size | virtual size |
| Peak working set | `PeakWorkingSetSize` (omitted where the call fails) | omitted | omitted |
| Usage | `cpu_usage() / cores`, of N logical cores | same | same |
| CPU time (user + kernel) | `accumulated_cpu_time()` | same | same |
| Threads | snapshot `cntThreads` | `tasks()` length | `n/a` |
| Uptime | `run_time()` | same | same |

## Detail Design

- [x] Detail design: not needed
- Reason: normal lane, one widget; the matrix above is the whole design.
