# OneTerm Repository-Wide Engineering Review

**Review date:** 2026-07-22
**Scope:** The complete repository, with primary implementation evidence from `crates/`, workspace manifests, build/release automation, and the project design documents.
**Method:** Whole-system review of crate boundaries, startup wiring, terminal and SFTP data flow, persistence, async lifecycles, tests, security controls, and operational automation. This is not a file-by-file approval review; conclusions consider interactions between modules.

**Status:** This report is the pre-remediation baseline. Reliability, SSH progress UX, maintainability, scoped readability, simplicity, consistency, performance, scalability, testability, and the scoped architecture migration have since been implemented; see [`remediation-plan.md`](remediation-plan.md) for current acceptance status. The original findings remain below for historical rationale.

## Executive summary

OneTerm has a thoughtful direction: the application is being moved from a UI god-crate toward a layered workspace, protocol implementations are hidden behind `TerminalSession`/`SftpBackend`/`SessionFactory`, terminal-controlled strings have a centralized sanitization policy, and the terminal hot path has real damage tracking, event coalescing, bounded queues, and diagnostics. The codebase is also unusually well documented for an active refactor.

The current production-readiness risk is nevertheless high. The SSH handler explicitly accepts every server host key, which makes a man-in-the-middle attack possible. SFTP operations can block the GPUI thread, transfers can write partial destination files, and remote directory names are used to construct local paths without containment checks. Terminal input/events are intentionally dropped when bounded queues fill, while the public session API exposes no error to the caller. Several resource maps and one runtime per SSH session grow with session history, and the release workflow builds artifacts but does not run the project's required test, clippy, or format gates.

The most important architectural observation is that the documented refactor is ahead of the actual repository. The current graph is largely acyclic and correctly keeps backends behind `app`, but `oneterm-ui` is now a vendored dock fork used by nearly every UI/shared crate and is not represented as a first-class layer in the authoritative architecture documents. The docs still contain obsolete `crates/ui` paths and former backend-boundary descriptions. This makes future changes harder to reason about and can hide dependency regressions.

## Scores

Scores are engineering assessments of the current repository, not claims about an intended future state.

| Category | Score / 10 | Assessment |
|---|---:|---|
| Readability | 6.0 | Strong comments and naming, but large files, duplicated facades, and stale documentation increase cognitive load. |
| Simplicity | 5.0 | The layering work is valuable, but there is speculative API surface, duplicated state/closure plumbing, and several indirect bridges. |
| Maintainability | 5.5 | Good module decomposition and explicit design docs are offset by graph/document drift, persistence duplication, and backend duplication. |
| Testability | 4.5 | Pure terminal/parser tests and a fake session exist, but important UI, SFTP, persistence, security, and lifecycle paths have little or no automated coverage. |
| Reliability | 4.0 | Lifecycle intent is explicit, but dropped queues, blocking operations, unbounded waits, partial files, and non-atomic writes create user-visible failure modes. |
| Performance | 5.5 | The terminal hot path has meaningful optimizations, while SFTP, SSH runtime allocation, snapshots, and some UI refresh loops remain expensive. |
| Security | 3.5 | Good terminal sanitization and clipboard defaults, but unconditional host-key acceptance is a release-blocking vulnerability. |
| Scalability | 4.5 | Multi-session composition is possible, but per-session runtimes, retained maps, linear registries, and full copies do not scale cleanly. |
| Consistency | 4.0 | Conventions are clear in guidance, but code/docs/manifests disagree and error/cancellation patterns vary materially. |
| Evolvability | 6.0 | The target layering and migration plans are strong; stale boundaries and process globals make future replacement/testing more difficult. |
| Architecture | 6.0 | The app composition boundary and DAG are good, but the actual dependency graph has an undocumented shared UI fork and an unfinished parallel contract design. |

## Top 10 issues, ordered by engineering impact

1. **Release-blocking SSH host-key bypass — Critical.** `crates/ssh/src/handler.rs:7-18` returns `Ok(true)` for every server key. There is no known-hosts store, fingerprint confirmation, or changed-key rejection. Any network attacker can impersonate a server and receive credentials or terminal input.
2. **Remote SFTP download path escape — High.** `crates/ssh/src/sftp_task.rs:741-769` joins untrusted remote entry names to the user-selected local directory. Only exact `.` and `..` are filtered. A malicious or compromised server can supply names such as `../outside` or names containing separators and cause writes outside the selected destination. Symlink and containment policy is also absent.
3. **Blocking SFTP calls on the GPUI thread — High.** `crates/sftp-ui/src/actions.rs:85-107,210-239,283-311,383-397` calls synchronous `SftpBackend` methods directly from button/dialog handlers. The backend uses `blocking_recv()` in `crates/ssh/src/sftp.rs:142-197`. Network latency, recursive deletion, or a stalled connection can freeze the entire UI.
4. **Input and event loss under load — High.** `crates/ssh/src/listener.rs:132-179` and `crates/local-shell/src/listener.rs:140-147` use bounded non-blocking sends. Writes and ordinary events are dropped on a full queue. The public `TerminalSession` methods in `crates/terminal/src/session.rs:240-312` return no error, despite the unused typed contract documenting `QueueFull` in `crates/terminal/src/contracts.rs:19-31`.
5. **SSH connection has no explicit timeout or cancellation — High.** `crates/session-ui/src/connect_dialog.rs:266-305` detaches a connection task, while `crates/ssh/src/session.rs:92-127` creates a runtime and awaits `client::connect` without a deadline. Closing the dialog does not cancel the attempt, and a network black hole can retain a runtime/thread indefinitely.
6. **Transfers leave partial destination files — High.** `crates/ssh/src/sftp_task.rs:685-725,812-849` creates the final local file before completion and returns on cancellation/error. A failed or cancelled transfer leaves a truncated file at the user's requested path. Uploads similarly create/truncate the remote target before completion (`487-524`).
7. **Persistence is non-atomic and duplicated — High.** `crates/settings/src/terminal_config/mod.rs:137-144`, `crates/settings/src/ui_config.rs:94-101`, `crates/session-ui/src/session_state.rs:135-145`, `crates/workspace/src/layout/workspace/persistence.rs:64-113`, and `crates/sftp-ui/src/persistence.rs:23-33` overwrite files directly. Crash or concurrent writers can corrupt JSON or lose fields; `docks.json` has read-modify-write races between workspace and SFTP state writers.
8. **Unbounded retained per-backend SFTP state — High.** `crates/sftp-ui/src/browser_state.rs:35-125` keys state by `Arc::as_ptr` and never removes old entries. Entries, transfers, errors, and transfer IDs remain for every backend ever seen; pointer reuse can also associate stale state with a later connection.
9. **One Tokio runtime/thread per SSH session — Medium/High.** `crates/ssh/src/session.rs:92-97,257-267` owns a one-worker multi-thread runtime in every session. Many tabs therefore create many runtimes/threads and make shutdown and resource accounting harder. A shared application runtime or bounded runtime pool is more scalable.
10. **Required quality gates are absent from release CI — High process risk.** `.github/workflows/release.yml:95-147` builds release artifacts but does not run `cargo fmt --check`, clippy, tests, or `cargo test --workspace`. A broken or insecure change can be published if the local gate is skipped.

## Top 10 strengths

1. `crates/app/src/session_factory.rs:1-39` is a clear composition boundary: only `app` knows both backend crates.
2. The crate graph is currently acyclic and UI crates do not directly depend on `oneterm-ssh` or `oneterm-local-shell`.
3. `crates/terminal/src/security_policy.rs:32-169` centralizes title, notification, clipboard, cwd, control-character, and BiDi handling.
4. Remote clipboard operations default to disabled (`security_policy.rs:53-65`), and OSC 8 URL handling denies credentials/custom schemes or withholds confirmation targets (`crates/terminal-view/src/handlers/mouse.rs:42-60`).
5. `crates/terminal/src/test_support.rs` provides a deterministic fake session and probes, enabling meaningful terminal-view tests.
6. Terminal output coalescing and lifecycle-task ownership are explicit in `crates/terminal-view/src/view/mod.rs:163-203,260-331`.
7. SFTP transfer cancellation uses `CancellationToken` and checks it during chunk loops (`crates/ssh/src/sftp_task.rs:134-255,459-699`).
8. The app uses bounded channels and lifecycle-specific blocking delivery for `Exited`/`Closed`, avoiding silent loss of the most important terminal events (`crates/ssh/src/listener.rs:172-193`).
9. Design documents identify intended boundaries, performance measurements, and migration phases instead of hiding technical debt.
10. Pure components such as shell resolution, URL policy, OSC parsing, syntax highlighting, space-tree operations, and terminal encoding have substantial unit coverage.

## Biggest architectural risks

- **The real and documented crate graphs have diverged.** `oneterm-ui` is a local dock fork used by `actions`, `state`, `workspace`, all feature crates, and `app`, but the authoritative structure docs do not list it as a layer. The migration docs still describe an old 21k-line UI monolith and old paths.
- **The capability boundary is not enforced by the API.** `TerminalSession` remains a large aggregate with void-returning writes, while `contracts.rs` defines narrower typed traits that are not used. The code therefore documents a safer design without obtaining its guarantees.
- **Synchronous protocol traits force implementation details into UI scheduling.** `SftpBackend` is sync and the SSH implementation hides async work behind `blocking_recv`, making it easy for UI code to accidentally block.
- **Process-global wiring complicates tests and multiple app instances.** `OnceLock<SessionFactory>`, GPUI globals, and function-pointer registries cannot be reset or replaced after initialization.
- **Shared JSON files have multiple writers without a transaction/locking abstraction.** This is especially risky for `docks.json`, which is updated by different crates using read-modify-write.

## Highest-priority improvement sequence

1. Disable SSH connections until host-key verification and changed-key handling are implemented, or make an explicit unsafe-development mode impossible in release builds.
2. Introduce async SFTP operations (or a dedicated service task) and move every network/file operation out of UI callbacks.
3. Make terminal writes return a typed result and implement reliable FIFO/backpressure or explicit coalescing by operation type; never silently drop keystrokes.
4. Add deadlines and cancellation propagation to connect, SFTP, and panel teardown.
5. Secure transfers with canonicalized/contained destination paths, symlink policy, temporary files, atomic rename, and cleanup on cancellation.
6. Centralize atomic JSON persistence with per-file serialization/mutexes and versioned migrations.
7. Add release CI test/lint/format gates and dependency/action pinning.
8. Reconcile `AGENTS.md`, `docs/agents/structure.md`, `docs/agents/dependencies.md`, refactor plans, and all source paths in one architecture update.

## Quick wins

- Gate `SshClientHandler::check_server_key` behind a hard failure rather than `Ok(true)` until verification exists.
- Add `tokio::time::timeout` around connection/auth/channel setup and retain a cancellation handle in the connect task.
- Change notification queuing to a bounded `VecDeque` using the already-declared `max_queued_notifications`; apply the declared rate limit.
- Remove completed transfer cancellation tokens and purge SFTP browser state when a backend closes.
- Use `NamedTempFile`/same-directory temporary files plus `rename` for downloads and JSON saves.
- Move `do_properties`, rename, mkdir, and delete onto the same background path already used by `load_dir`.
- Add `cargo test --workspace`, clippy, and format checks to `.github/workflows/release.yml` before artifact staging.
- Add `crates/highlight` explicitly to root workspace members and document `oneterm-ui` as a deliberate shared UI layer.

## Long-term improvements

- Replace per-session hidden runtimes with one app-owned async runtime/service boundary.
- Split terminal capabilities into implemented traits (`Renderer`, `Input`, `Lifecycle`, `Search`, optional `Sftp`) and remove compatibility methods only after consumers migrate.
- Extract a shared local/SSH terminal-session adapter to eliminate the ~83%-similar `session_terminal.rs` facades.
- Replace pointer-identity maps with connection IDs owned by session state and an explicit lifecycle/purge operation.
- Introduce a persistence service with schema versioning, atomic writes, conflict serialization, and test-injected paths.
- Rework Agent and SFTP views around incremental/virtualized models if fleet/file counts become large.

## Remediation plan

- [Remediation plan and category checklists](remediation-plan.md)

## Detailed category reports

- [Readability](readability.md)
- [Simplicity](simplicity.md)
- [Maintainability](maintainability.md)
- [Testability](testability.md)
- [Reliability](reliability.md)
- [Performance](performance.md)
- [Security](security.md)
- [Scalability](scalability.md)
- [Consistency](consistency.md)
- [Evolvability](evolvability.md)
- [Architecture](architecture.md)

## Review limitations and assumptions

- This review is based on source and configuration evidence. It does not claim that every issue has been reproduced at runtime.
- The security findings assume the application is intended for normal production SSH/SFTP use, not only trusted lab networks. Under that assumption, unconditional host-key acceptance is a release blocker.
- The SFTP path finding assumes a server can return malicious filenames or symlink metadata. A normal compliant server may not exploit it, but the client still writes data received from an untrusted peer and should enforce a local containment invariant.
- Performance findings identify likely scaling boundaries from code structure. They should be validated with representative profiles before invasive optimization.
