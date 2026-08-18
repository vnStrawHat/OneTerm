# Reliability Review

**Score: 4.0 / 10**

## REL-01 — Terminal writes and ordinary events can be silently lost

- **Files:** `crates/ssh/src/listener.rs:132-179`; `crates/local-shell/src/listener.rs:107-147`; `crates/terminal/src/session.rs:240-312`; `crates/terminal/src/contracts.rs:19-31,80-101`
- **Modules:** SSH/local transport bridges, terminal session contract
- **Severity:** **High**
- **Explanation:** SSH writes/resizes use `try_send` into a 64-item queue and only log on `Full`/`Closed`. Ordinary events use `try_send` into a 4096-item queue and can also be dropped. `TerminalSession::write`/`resize` return `()`, so callers cannot retry or notify the user. A safer `TerminalInput` API exists but is not implemented by production backends.
- **Why it matters:** Dropped keystrokes alter commands. Dropped title/clipboard/notification/agent events can desynchronize the UI. Logging is not recovery.
- **Recommended solution:** Use reliable FIFO/backpressure for writes, a latest-value coalescing slot for resize, a dedicated atomic/flag wakeup for output, and typed errors for closed/saturated transports. Migrate callers to handle those errors visibly. Preserve the existing lifecycle-specific guaranteed path.

## REL-02 — SSH connect attempts have no deadline or cancellation ownership

- **Files:** `crates/session-ui/src/connect_dialog.rs:266-305`; `crates/session-ui/src/quick_connect_dialog.rs:152-189`; `crates/ssh/src/session.rs:92-251`
- **Modules:** Session dialogs and SSH connection lifecycle
- **Severity:** **High**
- **Explanation:** Connection work is detached and creates a runtime before awaiting TCP/auth/channel/SFTP setup. No explicit timeout or cancellation token is passed. Dialog closure is unrelated to task cancellation.
- **Why it matters:** Stalled DNS/network/auth can retain tasks and threads, produce late notifications, or connect after the initiating UI context has disappeared.
- **Recommended solution:** Introduce a connection operation entity with states (`Connecting`, `Connected`, `Failed`, `Cancelled`), an overall deadline plus phase deadlines, and a cancellation token owned by the dialog/panel. Ensure cancellation tears down partially opened SSH/SFTP channels.

## REL-03 — SFTP operations freeze the UI and can wait indefinitely

- **Files:** `crates/sftp-ui/src/actions.rs:85-107,210-239,283-311,383-397`; `crates/ssh/src/sftp.rs:141-197`; `crates/ssh/src/sftp_task.rs:137-184`
- **Modules:** SFTP UI actions and sync-to-async bridge
- **Severity:** **High**
- **Explanation:** Rename, delete, mkdir, and stat execute from GPUI callbacks. Their backend methods send a command and call `blocking_recv()`. Recursive directory deletion is spawned in the Tokio runtime, but the GPUI caller still blocks until completion.
- **Why it matters:** Any network delay makes the application appear hung and can prevent cancellation or window processing.
- **Recommended solution:** Make `SftpBackend` async, or return operation handles/futures. Route every SFTP call through a background executor with cancellation and update the UI through `cx.update`. Disable duplicate actions while an operation is in flight.

## REL-04 — Failed/cancelled transfers leave truncated final files

- **Files:** `crates/ssh/src/sftp_task.rs:487-524,610-640,685-725,812-849`
- **Modules:** SFTP upload/download
- **Severity:** **High**
- **Explanation:** The final destination is created/truncated before transfer completion. Cancellation and I/O errors return without deleting the partial file. There is no temporary name or final atomic promotion.
- **Why it matters:** Existing files can be destroyed and replaced with incomplete content. Users may mistake a partial file for a successful download/upload after restart.
- **Recommended solution:** Transfer to a same-directory temporary file, flush/sync as appropriate, then atomically rename on success. On failure/cancel, close and remove the temporary file. Define overwrite/backup semantics explicitly.

## REL-05 — Configuration and layout writes are non-atomic and race-prone

- **Files:** `crates/settings/src/terminal_config/mod.rs:108-145`; `crates/settings/src/ui_config.rs:73-101`; `crates/session-ui/src/session_state.rs:115-145`; `crates/workspace/src/layout/workspace/persistence.rs:64-113`; `crates/sftp-ui/src/persistence.rs:23-33`
- **Modules:** Settings/session/layout persistence
- **Severity:** **High**
- **Explanation:** Writers call `std::fs::write` directly. `docks.json` has independent read-modify-write code in two crates, with no lock or generation check. Parse failures often fall back to defaults/empty state without preserving the corrupt file.
- **Why it matters:** Process interruption or overlapping writes can truncate JSON or lose another writer's fields. A later save can permanently replace recoverable user data with defaults.
- **Recommended solution:** Centralize persistence in one service. Serialize writes per path, write to a temporary sibling, flush, and rename. Keep a backup, version schemas, and quarantine invalid JSON rather than silently treating it as absent.

## REL-06 — Recursive transfer planning has unsafe failure modes

- **Files:** `crates/ssh/src/sftp_task.rs:537-592,741-778`
- **Modules:** Recursive SFTP transfer walkers
- **Severity:** **Medium/High**
- **Explanation:** Upload walks use recursive synchronous filesystem APIs inside an async task and can follow directory symlinks via `metadata`/`is_dir`; cycles/deep trees can cause unbounded recursion or stack exhaustion. Both upload and download materialize the entire file list before transfer. `collect_dirs` silently ignores read errors, unlike `collect_files`.
- **Why it matters:** Large, deep, cyclic, or permission-changing trees can hang, overflow, consume excessive memory, or yield inconsistent partial results.
- **Recommended solution:** Use an iterative bounded walker, `symlink_metadata`, explicit symlink policy, cancellation during discovery, and streaming enumeration. Treat discovery errors consistently and surface skipped paths.

## REL-07 — Local PTY thread ownership is incomplete

- **Files:** `crates/local-shell/src/session.rs:104-123`; `crates/local-shell/src/event_loop.rs:107-115`; `crates/local-shell/src/session_terminal.rs:221-230`
- **Modules:** Local shell lifecycle
- **Severity:** **Medium**
- **Explanation:** `ShellEventLoop::spawn` returns a join handle, but `LocalSession::spawn` immediately discards it. Shutdown sends a message but cannot wait for thread termination or report a panic.
- **Why it matters:** Teardown races, delayed resource release, and reader-thread panics cannot be observed or tested deterministically.
- **Recommended solution:** Store the join handle in an owned lifecycle object. Send shutdown once, wake the poller, and join from a non-UI teardown context with a bounded wait. Record abnormal thread exits.

## Reliability strengths

- `Exited`/`Closed` use a guaranteed blocking-send path instead of ordinary `try_send` (`crates/ssh/src/listener.rs:182-193`, local equivalent).
- SSH close has an atomic fallback flag so a full command queue cannot lose shutdown (`crates/ssh/src/listener.rs:142-169`).
- Terminal views retain task handles and explicitly cancel/close sessions on panel removal (`crates/terminal-view/src/view/mod.rs:130-145,321-342`; `panel/mod.rs:376-407`).
- SFTP cancellation is checked within transfer loops, not only between files.
