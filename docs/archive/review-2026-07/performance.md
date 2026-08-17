# Performance Review

**Score: 5.5 / 10**

**Status:** Remediated on 2026-07-22. PERF-01 through PERF-03 were completed by the Reliability/Scalability work. PERF-04 and PERF-05 now have opt-in rolling p95/p99 diagnostics, with the existing full snapshot and unbounded parser batch intentionally retained until measurements justify a more invasive change. PERF-06 and PERF-07 are implemented. See [`performance-benchmark.md`](performance-benchmark.md) for the measurement protocol and deterministic before/after evidence.

The original findings remain below as the pre-remediation baseline.

## PERF-01 — SFTP uploads load whole files into memory

- **Files:** `crates/ssh/src/sftp_task.rs:479-524,609-635`
- **Modules:** SFTP upload
- **Severity:** **High for large files**
- **Explanation:** `tokio::fs::read` allocates the full file, then the code sends 32 KiB slices. Directory upload repeats this one file at a time.
- **Why it matters:** A multi-gigabyte file requires equivalent process memory and can cause paging or allocation failure. Chunking after a full read does not bound memory.
- **Recommended solution:** Open `tokio::fs::File`, reuse a fixed-size buffer, and stream read/write. Preserve cancellation and progress after each chunk.

## PERF-02 — Synchronous filesystem walks run on a Tokio worker

- **Files:** `crates/ssh/src/sftp_task.rs:537-592`
- **Modules:** Recursive upload discovery
- **Severity:** **Medium/High**
- **Explanation:** `std::fs::read_dir`, metadata calls, and recursion run inside a transfer future on the session's only Tokio worker thread.
- **Why it matters:** A slow/network filesystem can block the SSH runtime that also drives shell and SFTP I/O for that session.
- **Recommended solution:** Use Tokio filesystem APIs carefully or `spawn_blocking` for traversal. Better, use a bounded producer/consumer walker that streams entries and observes cancellation.

## PERF-03 — One runtime and worker thread are created per SSH session

- **Files:** `crates/ssh/src/session.rs:58-73,92-97,253-268`
- **Modules:** SSH session runtime
- **Severity:** **Medium/High**
- **Explanation:** Every SSH tab owns a new one-worker multi-thread runtime.
- **Why it matters:** Thread stacks, schedulers, timers, and shutdown machinery scale linearly with sessions. This is unnecessary isolation for a desktop client expected to host many tabs.
- **Recommended solution:** Create one app-owned Tokio runtime or a bounded runtime service. Sessions own task/cancellation handles, not runtimes. Validate russh task isolation and shutdown on the shared runtime.

## PERF-04 — Terminal snapshots clone every visible cell per frame

- **Files:** `crates/terminal/src/content.rs:122-190,193-236`; `crates/terminal/src/model.rs:73-105`
- **Modules:** Terminal snapshot/render boundary
- **Severity:** **Medium; profile before changing**
- **Explanation:** Damage identifies dirty rows, but `TerminalContent::from` still clones the entire visible `display_iter` into `Vec<IndexedCell>`. Auxiliary `from_query` also clones all visible cells. The project has optimized narrower query methods, but full render snapshots remain O(rows × cols).
- **Why it matters:** Large grids, high-output TUIs, and multiple visible split terminals multiply clone/allocation cost while holding the terminal lock.
- **Recommended solution:** Keep the current implementation until profiling confirms it is dominant. If it is, snapshot only damaged rows plus required cursor/selection metadata, retain immutable row buffers, or use generation-stamped row caches. Preserve the critical rule that render damage is consumed exactly once.

## PERF-05 — Local PTY parsing can hold the terminal lock until the pipe is empty

- **Files:** `crates/local-shell/src/event_loop.rs:238-294`
- **Modules:** Local PTY pump
- **Severity:** **Medium**
- **Explanation:** Once the parser gets the terminal guard, it reads/parses until `WouldBlock`; the code explicitly removed a maximum locked-read budget. Under a producer that continuously outruns parsing, the pipe may not become empty promptly. The code uses `lock_unfair` in this path.
- **Why it matters:** The GPUI renderer also needs the terminal lock; long critical sections can increase frame latency and make the window appear unresponsive.
- **Recommended solution:** Measure lock-hold p95/p99 under sustained output. If it is material, use a byte/time budget, release the lock, yield/wake the UI, and continue. Do not reintroduce arbitrary sleeps.

## PERF-06 — Agent view rebuilds and clones the entire model about 8 times per second

- **Files:** `crates/agent-ui/src/lib.rs:25-29,70-95`; `crates/agent-ui/src/view.rs:192-260`; `crates/state/src/agent_registry.rs:545-617`
- **Modules:** Agent fleet view
- **Severity:** **Medium**
- **Explanation:** A 120 ms timer always calls `cx.notify`. Every render clones all `AgentCard`s, recomputes summary, performs linear grouping, sorts every group, and rebuilds all child elements.
- **Why it matters:** The cost grows with agent count even when no agent is active. It conflicts with the feature's “fleet view” goal.
- **Recommended solution:** Animate only active cards; refresh relative-time labels at 1 second or longer; cache grouping/summary in the registry; use stable keyed/virtualized rows for large fleets. Optimize only after measuring representative agent counts.

## PERF-07 — Production INFO logging is unnecessarily frequent

- **Files:** `crates/local-shell/src/event_loop.rs:347-374`
- **Modules:** PTY diagnostics
- **Severity:** **Low/Medium**
- **Explanation:** Every local session emits a detailed throughput line at INFO roughly every two seconds, including idle sessions.
- **Why it matters:** Multiple tabs can generate large log volume and recurring formatting/I/O overhead.
- **Recommended solution:** Put throughput reports behind `terminal-diagnostics` or DEBUG/TRACE, and avoid formatting when disabled.

## Performance strengths

- Output events are coalesced without fixed sleep latency (`crates/terminal-view/src/view/mod.rs:178-203`).
- Terminal rendering separates owned snapshots from paint and tracks damage/row caches.
- Bounded channels prevent unlimited queue growth.
- SFTP transfer I/O uses fixed 32 KiB remote read/write chunks after discovery (download path is already streaming).
- The workspace documents and applies targeted debug optimization to measured terminal hot-path crates instead of globally optimizing everything.
