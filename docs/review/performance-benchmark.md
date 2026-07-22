# Performance Remediation Measurement Report

**Date:** 2026-07-22  
**Scope:** PERF-04 through PERF-07  
**Status:** Instrumentation and deterministic before/after checks are committed; interactive terminal and PTY runs remain workload-dependent.

## Measurement protocol

The repository now exposes the measurements required before changing the terminal hot path:

1. Build the app with `terminal-diagnostics`:

   ```text
   cargo run -p oneterm-app --features terminal-diagnostics
   ```

2. Exercise a representative large grid (45×160 or larger), a high-output TUI, and an idle terminal.
3. Collect at least 30 seconds of diagnostics. While frames are being painted, the terminal renderer reports every five seconds from a rolling window of up to 512 samples. The first painted frame reports immediately. Each record includes:
   - `snapshot p95` and `snapshot p99`: terminal lock plus owned-cell snapshot duration;
   - `frame p95` and `frame p99`: prepaint plus paint duration;
   - `dirty`, `row_layouts`, and `samples`: damage-tracking evidence.
4. For local PTY parsing, enable DEBUG logging and collect the `[PTY pump]` record. Every two-second window reports `lock p95`, `lock p99`, and the sample count. The lock sample starts after `Term` acquisition and ends immediately after the parser releases it, so reply I/O is not included.

The diagnostics are feature- and log-level-gated. A normal build does not allocate the latency sample windows, call the PTY timing clocks, or format throughput records.

## Deterministic before/after evidence

| Area | Before | After | Evidence |
|---|---|---|---|
| Inactive Agent refresh | 120 ms notification cadence (8.33 refreshes/s) | Relative-time refresh every 1 s (1 refresh/s) | `ACTIVE_CARD_TICK` and `RELATIVE_TIME_TICK` in `crates/agent-ui/src/lib.rs` |
| Unchanged Agent model work | Clone, summary scan, grouping, and sorting on every render | Clone, summary, grouping, and sorting only in the registry observer; animation renders cached cards and stable indices | `AgentDisplayGroup`, `cards`, and `counts` in `crates/agent-ui/src/lib.rs` |
| PTY throughput logging | INFO record approximately every two seconds per local session | No production record; DEBUG plus `terminal-diagnostics` only | `crates/local-shell/src/event_loop.rs` |
| SFTP upload memory | Whole-file read | Fixed reusable buffer and streaming read/write | `crates/ssh/src/sftp_task.rs` |
| Render damage | Damage reset once per render snapshot | Unchanged; existing damage tests remain in `crates/terminal/src/content.rs` | `TerminalContent::from` and row-cache diagnostics |

## Runtime result fields

Numeric p95/p99 values are deliberately not checked into this document: they depend on the operating system, GPU, terminal size, font, PTY implementation, and workload. A benchmark run is complete when the captured records contain all of the following fields:

```text
[TerminalElement] ... dirty=... snapshot_us=... prepaint_us=... paint_us=... | snapshot p95=...us p99=...us | frame p95=...us p99=...us samples=...
[PTY pump] ... | lock p95=...us p99=...us samples=...
```

The implementation retains full snapshots and the existing unbounded parser batch because the required measurements are now available and no invasive optimization should be made without a workload-specific regression baseline. Damage tracking is covered by the existing terminal tests and the renderer diagnostics include dirty-row counts for manual comparison.
