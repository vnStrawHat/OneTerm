# Reliability review — 6.0/10

## What is working

- SSH connection phases and the whole connect operation have deadlines and cooperative cancellation (`crates/ssh/src/session.rs:76-133`, `:380-383`).
- Local lifecycle events use blocking delivery so `Exited`/`Closed` are not silently dropped (`crates/local-shell/src/listener.rs:166-175`).
- SFTP transfer finalization uses temporary files, backups, cancellation cleanup, flush/sync for local files, and a bounded shutdown drain (`crates/ssh/src/sftp_transfer.rs:97-137`, `crates/ssh/src/sftp_task.rs:336-361`).
- Invalid settings/session/layout JSON is quarantined rather than overwritten blindly.

## Findings

### REL-01 — High: persistence guarantees stop at the process boundary and several UI paths block on disk I/O

**Files/modules:** `crates/core/src/persistence.rs:16-27`, `:43-80`, `crates/session-ui/src/session_state.rs:69-115`, `crates/settings/src/ui_config.rs:148-155`, `crates/workspace/src/layout/workspace/persistence.rs:81-104`.

**Explanation:** `FILE_LOCKS` is an in-memory `HashMap<PathBuf, Mutex<()>>`, so only threads in one process are serialized. Two running OneTerm processes can race read-modify-write operations and the fixed `.bak` file. Separately, UI mutation paths synchronously call an implementation that writes, `sync_all`s, copies the previous file, and renames.

**Why it matters:** Multi-instance use can lose changes or produce misleading backups. Slow/antivirus-scanned/network-backed home directories can freeze interactive handlers. Memory is mutated before save, so a failed write leaves the UI and persisted state divergent.

**Recommended solution:** Adopt one of two explicit policies:

1. enforce a single application instance and route open requests to it; or
2. use an OS-level advisory lock plus revision-aware updates.

In either case, write immutable snapshots on a background executor, retain a dirty/retry state, and notify users when durable save fails.

### REL-02 — High: transfer cancellation is a string protocol

**Files/modules:** `crates/core/src/error.rs:40-48`, `crates/ssh/src/sftp_transfer.rs:190-194`, `:541-545`, `crates/sftp-ui/src/transfer.rs:150-154`, `:432-436`.

**Explanation:** The backend returns `AppError::msg("cancelled")`; the UI checks `e.to_string() == "cancelled"`.

**Why it matters:** Adding context such as `"upload cancelled"`, localization, or a source chain changes control flow. Cancellation may then be shown as an error or leave status stuck.

**Recommended solution:** Add a typed variant and match it.

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("operation cancelled")]
    Cancelled,
    // ...
}

match result {
    Err(AppError::Cancelled) => mark_cancelled(),
    Err(error) => mark_failed(error),
    Ok(()) => mark_complete(),
}
```

### REL-03 — Medium: the bounded session event queue drops non-coalescible events

**Files/modules:** `crates/local-shell/src/listener.rs:156-163`, `:211-281`, `:285-335`; equivalent code in `crates/ssh/src/listener.rs`; queue creation in `crates/local-shell/src/session.rs:87` and `crates/ssh/src/session.rs:157`.

**Explanation:** `forward` uses non-blocking `try_send` and can drop any normal event. The comment justifies this because `Output` is debounced, but the same method transports CWD, title, clipboard, progress, bell, notification, shell integration, and agent state. Only lifecycle has a reliable path.

**Why it matters:** Under heavy output, a terminal can miss a clipboard write, progress completion, CWD update, or agent transition. Cached state repairs some reads, but not one-shot side effects.

**Recommended solution:** Separate channels: a coalescing repaint flag/watch channel for output, a bounded reliable control channel with explicit overflow policy, and a lifecycle channel/flag with priority. Coalesce replaceable values (latest title/CWD/progress) rather than dropping arbitrary arrivals.

### REL-04 — Medium: SFTP download contains an avoidable production `unwrap`

**Files/modules:** `crates/sftp-ui/src/transfer.rs:282-306`.

**Explanation:** `do_download` validates that an entry is selected, then executes `self.sftp.clone().unwrap()`. The backend is optional and can change when the active terminal changes. A stale selection plus a backend transition can violate the assumption.

**Why it matters:** A user action can panic the GUI instead of producing a recoverable message.

**Recommended solution:** Capture the entry and backend in one guarded snapshot; clear selection when backend changes; show a warning if no active SFTP connection exists.

```rust
let (entry, sftp) = match (self.selected_entry(cx).cloned(), self.sftp.clone()) {
    (Some(entry), Some(sftp)) => (entry, sftp),
    _ => {
        notify_user("No active SFTP connection", window, cx);
        return;
    }
};
```

### REL-05 — Medium: local shutdown can block indefinitely while joining the PTY owner thread

**Files/modules:** `crates/local-shell/src/session.rs:149-172`, `crates/local-shell/src/event_loop.rs:187-249`.

**Explanation:** `shutdown_owner` sends shutdown and immediately joins without a deadline. `Drop` also joins. If the poller/PTY implementation or parser thread does not return, close/drop blocks the caller.

**Why it matters:** Closing a tab or quitting can hang the UI indefinitely on platform-specific PTY behavior.

**Recommended solution:** Make close asynchronous, wait with a deadline through a completion channel, and detach/report a stuck owner thread rather than blocking the UI. Preserve the direct join path in tests where deterministic cleanup is required.
