# Simplicity review — 6.0/10

## What is working

- The dependency rules avoid a common failure mode: feature crates do not import protocol implementations. `SessionFactory` in `crates/terminal/src/factory.rs:25-45` is a focused abstraction with an isolated test slot.
- Shared terminal-model operations were deliberately extracted into `TerminalModel<EP>` (`crates/terminal/src/model.rs:1-8`), avoiding two copies of selection, scrolling, search, and snapshot logic.
- SFTP traversal uses bounded work structures rather than materializing unbounded directory plans (`crates/ssh/src/sftp_transfer.rs:587-609`).

## Findings

### SIMP-01 — Medium: `TerminalSession` is a broad capability interface

**Files/modules:** `crates/terminal/src/session.rs:151-360`, `crates/terminal/src/model.rs`, `crates/local-shell/src/session_terminal.rs`, `crates/ssh/src/session_terminal.rs`.

**Explanation:** One trait covers full-grid rendering, compact queries, input, resize, mouse, selection, search, IME, clipboard, shell integration, SFTP access, lifecycle, network statistics, and agent status. Backend implementations delegate part of this to `TerminalModel`, but the trait remains the single dependency for every feature.

**Why it matters:** A change to an optional capability can affect both backends, the fake session, and every UI consumer. It also makes mocks large and encourages `None`/default behavior for unsupported features.

**Recommended solution:** Keep a small stable session identity/lifecycle façade and split optional capabilities into traits such as `TerminalRender`, `TerminalInput`, `TerminalSearch`, `CwdSource`, `SftpProvider`, and `NetworkMetrics`. Expose them through `Arc<dyn ...>` or a typed session capability object. Do not split merely for aesthetics; split when ownership or test setup differs.

### SIMP-02 — Medium: two service-locator mechanisms trade compile-time clarity for runtime indirection

**Files/modules:** `crates/terminal/src/factory.rs:48-110`, `crates/state/src/commands.rs:15-45`, `crates/app/src/init.rs:50-62`.

**Explanation:** Backend creation is a process-global `OnceLock`, while UI commands are a GPUI global containing function pointers. This preserves the crate DAG, but consumers silently depend on initialization order and runtime registration. `app::init` must register the exact set of callbacks before any feature invokes them.

**Why it matters:** The design is easy to use in production but harder to understand and test in isolation. Missing registration is a runtime `None`/error path rather than a compiler error; multiple application contexts cannot independently install the process-global factory.

**Recommended solution:** Retain the app-only wiring boundary, but pass an `AppServices`/`WorkspaceServices` entity to features during initialization. Keep a process-level fallback only for backend code that truly lacks an app context. Add an explicit startup validation that reports all required services and feature registrations together.

### SIMP-03 — Medium: custom URL parsing is more code and less precise than a well-specified parser

**Files/modules:** `crates/terminal/src/url_policy.rs:147-220`.

**Explanation:** `ExternalTargetPolicy` implements a minimal parser to extract scheme, credentials, and port instead of using a URL parser. The allowlist is valuable, but the parser intentionally does not validate the complete URL structure and has edge cases around authority/IPv6 parsing.

**Why it matters:** Security policy code benefits from a parser with well-defined URL grammar. A minimal parser can misclassify a target, cause unnecessary confirmation, or make future scheme/authority rules unsafe to extend.

**Recommended solution:** Use the already accepted dependency policy to determine whether a URL crate is permissible; otherwise document a deliberately narrow grammar and add tests for bracketed IPv6, userinfo, malformed authorities, empty hosts, encoded delimiters, and non-default ports. Do not broaden accepted schemes without threat-model review.

### SIMP-04 — Low: duplicated UI persistence entry points obscure one policy

**Files/modules:** `crates/settings/src/terminal_config/mod.rs:117-166`, `crates/settings/src/ui_config.rs:80-123`, `crates/session-ui/src/session_state.rs:117-162`, `crates/state/src/dock_persistence.rs:69-91`.

**Explanation:** Schema ownership is correctly separated, but each owner repeats load/default/quarantine/save logging patterns. The shared primitives are used for writes, while read and recovery behavior varies subtly.

**Why it matters:** Future changes to backup retention, schema versions, telemetry, or malformed-file recovery can be applied inconsistently.

**Recommended solution:** Introduce a small generic `JsonDocumentStore<T>` in `core` for common lifecycle behavior while keeping schema migration closures in the owning crate. Avoid an abstraction that hides ownership; centralize mechanics only.
