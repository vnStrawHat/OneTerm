# Performance review — 6.5/10

The review recommends optimization only where the current design has an identifiable cost or unbounded behavior. The repository already contains useful performance diagnostics and damage-aware rendering; no broad rewrite is justified without measurements.

## What is working

- The terminal snapshot is owned, so the engine lock is not held during drawing (`crates/terminal/src/content.rs:1-9`).
- Damage is consumed once per render and auxiliary queries avoid consuming it (`crates/terminal/src/content.rs:122-203`).
- Compact query APIs avoid full-grid cloning for common mode/cursor checks (`crates/terminal/src/model.rs:84-99`).
- SFTP streams 32 KiB chunks, bounds recursive traversal to 64 levels/100,000 entries, and uses incremental plans.
- Diagnostics are feature-gated to avoid normal-build instrumentation overhead.

## Findings

### PERF-01 — High: terminal command queues have no memory bound

**Files/modules:** `crates/ssh/src/session.rs:153-157`, `crates/ssh/src/listener.rs:135-174`, `crates/local-shell/src/event_loop.rs:99-103`, `:156-160`, `:207-249`.

**Explanation:** SSH uses `async_channel::unbounded::<Cmd>()`. Local shell uses `std::sync::mpsc::channel()` and an unbounded `VecDeque` of write buffers. Both preserve FIFO input, but neither limits queued messages or bytes while a transport is stalled.

**Why it matters:** Large paste operations, generated OSC/terminal responses, or a blocked network/PTY writer can cause sustained memory growth. Message count alone is insufficient because one message may contain a large paste.

**Recommended solution:** Bound by total queued bytes and prioritize control messages. Suggested behavior:

- keystrokes: bounded FIFO, report `QueueFull` rather than drop;
- resize: keep only the latest pending size;
- close: out-of-band atomic flag or priority channel;
- paste: enforce existing 1 MiB cap and reject/await when byte budget is exhausted;
- generated responses: bounded with operation-specific diagnostics.

Add saturation benchmarks before selecting capacities.

### PERF-02 — Medium: the SFTP panel clones its complete browser state twice per second

**Files/modules:** `crates/sftp-ui/src/panel.rs:128-148`, `:284-306`, `crates/sftp-ui/src/browser_state.rs:39-63`.

**Explanation:** The 500 ms follow timer always calls `save_state_for_key`, which clones `delegate.entries`, transfers, errors, CWD, and other state, even when no CWD or UI state changed.

**Why it matters:** Large directories can contain tens of thousands of `FileEntry` values. Repeated cloning allocates and copies on the UI update path, even while the SFTP panel is idle.

**Recommended solution:** Make the store authoritative or track a dirty generation. Save only after actual mutation and on tab/panel transition. The follow timer should compare CWD and do nothing when unchanged.

### PERF-03 — Medium: full-grid snapshots remain proportional to viewport size every paint

**Files/modules:** `crates/terminal-view/src/element/prepaint.rs:139-143`, `crates/terminal/src/content.rs:159-190`, `crates/terminal/src/model.rs:72-82`.

**Explanation:** Damage limits layout recomputation, but `TerminalContent::from` still clones every displayed cell into a `Vec<IndexedCell>` on each snapshot. This is a deliberate lock-release tradeoff and is documented in the dev profile comments.

**Why it matters:** Multiple visible split terminals or large grids multiply clone cost. The cost can dominate even when only one row is damaged.

**Recommended solution:** Do not replace this without profiling. First benchmark snapshot time at target grid/session counts using existing diagnostics. If it is material, snapshot only damaged rows plus stable row generations, or share immutable row buffers. Preserve the invariant that GPUI drawing never holds the terminal lock.

### PERF-04 — Medium: sustained PTY output may hold the terminal lock until the pipe drains

**Files/modules:** `crates/local-shell/src/event_loop.rs:276-356`.

**Explanation:** Once acquired, the event loop reads and parses until the pipe is empty; the comments explicitly reject a maximum locked-read limit. A 1 MiB buffer and sustained producer can create long lock holds despite `FairMutex` fairness.

**Why it matters:** The UI snapshot waits on the same lock. Full-screen/high-throughput workloads can trade throughput for interaction latency.

**Recommended solution:** Use the existing diagnostics to measure p95/p99 lock hold and frame latency. If interactive latency exceeds the target, introduce a byte/time parse budget per lock acquisition and yield between batches. Keep throughput benchmarks to avoid regressing the DOOM-fire workload.

### PERF-05 — Low: SFTP upload discovery uses a 1 ms polling loop

**Files/modules:** `crates/ssh/src/sftp_transfer.rs:242-262`.

**Explanation:** A blocking traversal thread repeatedly calls `try_send` and sleeps for 1 ms when the bounded channel is full.

**Why it matters:** It introduces wakeups and latency under backpressure and is more complex than a blocking send.

**Recommended solution:** Use `send_blocking` if available, or a cancellation-aware blocking primitive. Ensure cancellation closes the channel or wakes the producer promptly.
