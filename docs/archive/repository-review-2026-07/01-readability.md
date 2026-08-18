# Readability review — 7.0/10

## What is working

- The crate map and ownership rules are unusually explicit in [`docs/architecture.md`](../../architecture.md) and [`docs/agents/structure.md`](../../agents/structure.md).
- Modules generally use descriptive names (`sftp_transfer`, `security_policy`, `dock_persistence`, `session_factory`) and explain non-obvious invariants in Rust doc comments.
- The terminal engine exposes neutral types instead of leaking GPUI into the engine (`crates/terminal/src/session.rs:1-9`), which makes the code's layer boundaries easy to infer.
- Security-sensitive code states its assumptions near the implementation. For example, SFTP traversal limits and symlink policy are documented directly in `crates/ssh/src/sftp_transfer.rs:139-146` and `crates/ssh/src/sftp_task.rs:378-443`.

## Findings

### READ-01 — Medium: several modules exceed the repository's own size convention

**Files/modules:** `crates/ssh/src/sftp_task.rs` (912 lines), `crates/ssh/src/sftp_transfer.rs` (743), `crates/terminal-view/src/view/mod.rs` (621), `crates/ssh/src/session.rs` (553), `crates/sftp-ui/src/panel.rs` (550), `crates/sftp-ui/src/actions.rs` (539), `crates/ssh/src/listener.rs` (520), `crates/local-shell/src/listener.rs` (578), and 17 other Rust files over 400 lines.

**Explanation:** The project convention says each Rust file should be approximately 400 lines or less (`docs/agents/structure.md:141-145`), but the repository contains 25 files over that threshold. The largest files combine orchestration, transport lifecycle, error conversion, persistence, and tests. `sftp_task.rs` alone contains the command loop, cancellation bookkeeping, path validation, recursive deletion, metadata mapping, and tests.

**Why it matters:** File size is not itself a defect, but these modules force readers to hold multiple state machines in mind. It increases merge conflicts, makes review less local, and makes it harder to identify the correct ownership of a change.

**Recommended solution:** Split by state-machine responsibility, not arbitrary line count. For SFTP, use `command_loop.rs`, `transfer_registry.rs`, `path_policy.rs`, `recursive_delete.rs`, and `metadata.rs`; keep the public `sftp_task` orchestration thin. For the terminal view, separate event subscription/lifecycle, clipboard/agent integration, and view state from rendering.

**Example shape:**

```rust
// sftp_task.rs
pub(crate) async fn run_sftp_task(ctx: SftpTaskContext) {
    while let Some(command) = ctx.next_command().await {
        dispatch_command(&ctx, command).await;
    }
}

// transfer_registry.rs
pub(crate) struct ActiveTransfers { /* cancellation + lifecycle */ }

// path_policy.rs
pub(crate) fn validate_remote_entry_name(name: &str) -> Result<()> { /* ... */ }
```

### READ-02 — Low: comments explain history more often than the current contract

**Files/modules:** `crates/local-shell/src/session.rs:1-6`, `crates/terminal/src/paste.rs:1-8`, `crates/terminal/src/security_policy.rs:1-12`, `crates/ssh/src/session.rs:1-17`.

**Explanation:** Historical “before Phase 1” and numbered architecture notes are useful, but they are mixed with operational documentation in source files. A reader must distinguish current invariants from migration history and design rationale.

**Why it matters:** Historical context becomes stale faster than the code and can obscure the small set of rules that must remain true.

**Recommended solution:** Keep a short current contract in the module comment, move detailed history to the relevant design record, and link to it. Use structured sections such as “Inputs”, “Ownership”, “Failure behavior”, and “Cancellation”.

### READ-03 — Low: repeated state mirrors require cross-file reasoning

**Files/modules:** `crates/sftp-ui/src/panel.rs:54-92`, `crates/sftp-ui/src/browser_state.rs:39-63`, `crates/state/src/app_state.rs:24-36`, `crates/terminal-view/src/view/mod.rs:288-324`.

**Explanation:** The SFTP panel mirrors store state, the active terminal is mirrored in `AppState`, and terminal views maintain a large collection of cached render/search/agent state. The comments explain the arrangement, but correctness depends on remembering which copy is authoritative at each point.

**Why it matters:** State synchronization bugs are difficult to diagnose because each copy can be locally plausible while stale globally.

**Recommended solution:** Make authority explicit in types and methods (`ActiveBackendSnapshot`, `PanelProjection`), or eliminate mirrors where the render cost is acceptable. At minimum, add assertions/instrumentation for key transitions and document mutation ownership in one place.
