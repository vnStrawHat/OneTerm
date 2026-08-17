# Scalability review — 5.5/10

OneTerm is a desktop application, so the relevant scale is concurrent sessions, split panes, large terminal grids, large SFTP trees, background transfers, and potentially multiple windows—not horizontal server scaling.

## What is working

- SSH sessions share one runtime rather than creating a Tokio runtime per tab (`crates/ssh/src/session.rs:1-6`).
- SFTP transfers run as background tasks while the command loop remains responsive to cancellation (`crates/ssh/src/sftp_task.rs:134-176`).
- Recursive SFTP operations are bounded by depth and entry count.
- Terminal snapshots release locks before drawing and damage-aware layout reduces repeated work.
- Per-backend SFTP state prevents tab switching from destroying active transfer state (`crates/sftp-ui/src/browser_state.rs:1-18`).

## Findings

### SCAL-01 — High: application state assumes one active workspace/SFTP context

**Files/modules:** `crates/state/src/app_state.rs:13-37`, `crates/sftp-ui/src/panel.rs:34-52`, `crates/sftp-ui/src/browser_state.rs:89-104`.

**Explanation:** `AppState` stores one active SFTP backend, one CWD source, and one local/remote flag. `SftpPanel` is explicitly one panel for the whole app. Globals are process-wide.

**Why it matters:** Multiple windows, independent workspaces, or two simultaneously visible SFTP browsers cannot have independent “active” state without races/last-writer-wins behavior.

**Recommended solution:** Scope active-session state to a `WorkspaceState` entity owned by each window. Make panels receive that entity. Keep truly process-wide settings/theme state global, but not window focus/session selection.

### SCAL-02 — Medium: SSH runtime capacity is fixed at two workers for every session and transfer

**Files/modules:** `crates/ssh/src/session.rs:76-96`, `:365-375`, `crates/ssh/src/sftp_task.rs:227-295`.

**Explanation:** All SSH shell tasks and SFTP transfer tasks share a runtime with exactly two worker threads.

**Why it matters:** Async network work is normally efficient, but parsing, crypto, compression, filesystem completions, and many concurrent transfers can contend. A fixed value ignores machine capacity and target session count.

**Recommended solution:** First define/benchmark the target scale. If two workers show saturation, use `available_parallelism()` with a conservative cap/configuration, or split transport and heavier work. Avoid increasing thread count without evidence.

### SCAL-03 — Medium: each local shell owns a dedicated OS thread and 1 MiB read buffer

**Files/modules:** `crates/local-shell/src/event_loop.rs:30-31`, `:118-142`, `crates/local-shell/src/session.rs:44-57`.

**Explanation:** Every local terminal spawns a PTY owner thread; the event loop allocates a 1 MiB stack buffer when running.

**Why it matters:** This is simple and robust at normal desktop counts, but dozens of local tabs increase thread scheduling and memory overhead linearly.

**Recommended solution:** Keep the model unless measured target scale requires change. Document a supported session target and benchmark 10/25/50 local sessions for idle memory, shutdown latency, and sustained output. A shared poll loop is a long-term option, not an immediate rewrite.

### SCAL-04 — Medium: state cloning scales with SFTP directory size

**Files/modules:** `crates/sftp-ui/src/panel.rs:128-148`, `:284-306`, `crates/sftp-ui/src/browser_state.rs:39-63`.

**Explanation:** The full active entry and transfer vectors are cloned into a global store every 500 ms.

**Why it matters:** The traversal backend supports up to 100,000 entries, but the UI-state mechanism performs O(n) copying independent of changes.

**Recommended solution:** Store entries once behind an immutable `Arc<[FileEntry]>` or make the store authoritative and mutate via generation-tagged actions. Remove periodic snapshots.

## Capacity assumptions that should be made explicit

The repository does not define service-level targets. Before structural optimization, establish and automate representative targets such as:

- 20 concurrent SSH sessions, including 4 active transfers;
- 10 local PTYs with 4 visible split panes;
- 100k-entry hostile traversal rejection and 20k-entry normal directory display;
- 240×80 and 400×120 terminal grids under sustained output;
- close-all/shutdown deadline under load.
