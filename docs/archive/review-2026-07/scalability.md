# Scalability Review

**Score: 4.5 / 10**

## SCALE-01 — Per-session runtimes scale threads linearly

- **Files:** `crates/ssh/src/session.rs:58-73,92-97,253-268`
- **Modules:** SSH session ownership
- **Severity:** **High at many sessions**
- **Explanation:** Each SSH session owns a dedicated Tokio runtime with one worker.
- **Why it matters:** Dozens/hundreds of tabs create dozens/hundreds of scheduler threads and associated memory. Shared network I/O is a natural fit for one bounded runtime.
- **Recommended solution:** Move runtime ownership to `app`; sessions retain only task handles and cancellation scopes. Add a multi-session soak test that opens/closes many fake or loopback connections and checks thread/resource return.

## SCALE-02 — SFTP browser state is retained forever and keyed by an address

- **Files:** `crates/sftp-ui/src/browser_state.rs:35-40,42-125`; `crates/sftp-ui/src/panel.rs:205-252`
- **Modules:** Global SFTP browser state
- **Severity:** **High**
- **Explanation:** `BackendKey` is the erased `Arc` pointer converted to `usize`. The global `HashMap` has save/get/update but no removal. Each entry retains directory contents and transfer history after the connection closes.
- **Why it matters:** Memory grows with connection history, not active connections. Allocator pointer reuse can make a new backend inherit stale state and transfer IDs.
- **Recommended solution:** Assign a monotonically unique `SessionId`/`SftpBackendId` at connection creation. Purge state on backend close/panel lifecycle, with optional bounded recent-history retention if desired.

## SCALE-03 — Transfer cancellation entries never leave the map

- **Files:** `crates/ssh/src/sftp_task.rs:134-135,185-255`
- **Modules:** SFTP task lifecycle
- **Severity:** **Medium**
- **Explanation:** Cancellation tokens are inserted for every transfer. The source comments acknowledge that spawned tasks cannot remove them, but no completion channel or shared map removal exists. The claim that the map contains “only running transfers” is incorrect.
- **Why it matters:** Long-lived sessions accumulate tokens, and reused transfer IDs can target stale/completed operations.
- **Recommended solution:** Track `JoinSet`/completion messages in the main SFTP loop and remove entries on completion. Reject duplicate IDs. On close, cancel all tokens and await/abort all transfer tasks.

## SCALE-04 — Recursive transfers materialize complete trees before work starts

- **Files:** `crates/ssh/src/sftp_task.rs:537-644,741-852`
- **Modules:** SFTP directory upload/download
- **Severity:** **Medium/High**
- **Explanation:** Both paths build vectors containing every file/path/size before transferring.
- **Why it matters:** Very large trees delay first-byte progress and consume memory proportional to file count. Cancellation during discovery is limited or absent.
- **Recommended solution:** Stream discovery through a bounded channel, maintain progress as discovered/processed bytes or use indeterminate progress until enumeration completes, and cap traversal concurrency.

## SCALE-05 — Agent registry and view use linear scans/copies

- **Files:** `crates/state/src/agent_registry.rs:484-617`; `crates/agent-ui/src/view.rs:192-260`
- **Modules:** Agent state and fleet view
- **Severity:** **Medium**
- **Explanation:** Event apply searches a `Vec` linearly by `(terminal_key, agent_id)`. Each render clones all cards, linearly finds groups, and sorts them.
- **Why it matters:** The design calls this a fleet view, so card counts can become materially larger than a normal tab list.
- **Recommended solution:** Store cards in a map keyed by a stable composite ID plus a stable-order vector/index. Maintain summary/group indices incrementally and virtualize long lists.

## SCALE-06 — Global process registries assume one app instance and one initialization order

- **Files:** `crates/terminal/src/factory.rs:48-58`; `crates/state/src/commands.rs:15-40`; GPUI global wrappers across `state`/`settings`
- **Modules:** Composition and global state
- **Severity:** **Medium**
- **Explanation:** A process-wide `OnceLock` factory cannot be replaced, and feature wiring depends on globals initialized in a specific sequence.
- **Why it matters:** Tests, multiple windows/app contexts, headless tools, and future plugin/runtime isolation are harder to support.
- **Recommended solution:** Prefer app/context-owned service entities passed through explicit composition. Where a global is required by GPUI, make initialization idempotent and injectable for tests, and expose validation errors rather than silently ignoring replacements.

## Scalability strengths

- Backend/UI separation permits different implementations without feature-to-backend dependencies.
- Terminal tabs and split spaces have explicit teardown and task ownership.
- Bounded queues constrain transient memory growth.
- Per-backend SFTP state preserves active background transfer state across tab switches; lifecycle
  cleanup now uses stable IDs and weak backend tracking.

## Current remediation status (2026-07-22)

The highest-risk growth paths are remediated while two measured UI-scale follow-ups remain:

- SCALE-01: SSH sessions now share one lazily initialized two-worker Tokio runtime. A concurrent
  stress test verifies that all callers reuse the same runtime instead of allocating one runtime
  and worker thread per session.
- SCALE-02: The existing stable `SftpSessionId` is now paired with weak backend tracking.
  `SftpBrowserStore` purges closed or dropped backends during panel synchronization, does not
  retain backend references, and does not recreate state after purge.
- SCALE-03: Active duplicate transfer IDs are rejected to prevent token replacement. Transfer
  tasks are tracked, token removal is enforced by a drop guard on completion, panic, or abort, and
  close waits for cooperative cancellation before aborting shutdown stragglers.
- SCALE-04: Recursive upload discovery runs in `spawn_blocking` and sends entries through a
  capacity-128 cancellation-aware channel. Recursive download processes each discovered file
  immediately with a bounded DFS stack instead of retaining the complete file plan. Existing
  traversal limits, symlink rejection, destination containment, cancellation, and atomic
  finalization remain enforced.
- SCALE-05: `AgentRegistry` now indexes cards by `(EntityId, agent_id)` and rebuilds the index
  after removals. Agent view grouping uses a hash index, eliminating repeated group-position
  scans. Full Agent-list virtualization remains an explicit follow-up; the SFTP table is already
  virtualized by `DataTable`.
- SCALE-06: The two intentionally distinct global registries already reject duplicate
  initialization and document their ownership. Moving them into app-context-owned services
  remains an evolvability follow-up rather than part of this runtime-growth remediation.

Focused coverage now verifies shared runtime reuse, SFTP state cleanup, composite Agent indexing,
bounded streaming discovery, cancellation under channel backpressure, traversal limits, tracked
transfer shutdown, and token cleanup. A measured multi-session UI/network soak harness and
Agent-list virtualization remain open in `remediation-plan.md`.
