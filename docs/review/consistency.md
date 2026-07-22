# Consistency Review

**Score: 4.0 / 10**

## CONS-01 — Documentation and source paths use incompatible architectures

- **Files:** `docs/agents/structure.md`, `docs/agents/dependencies.md`, `docs/refactor/ui-crate-restructure.md`, `docs/sftp-browser-design.md`, `docs/ssh-client-connect.md`, `docs/terminal-fullscreen-perf/*`, current `crates/*`
- **Modules:** Repository documentation
- **Severity:** **High**
- **Explanation:** Current code uses `terminal-view`, `sftp-ui`, `session-ui`, `settings-ui`, `agent-ui`, `workspace`, and a vendored `oneterm-ui`; many docs still describe `crates/ui/src/views/...` and old `local` paths.
- **Why it matters:** Contributors receive contradictory placement and dependency instructions.
- **Recommended solution:** Add a `historical/design` label to old documents, update current references in bulk, and keep one checked current graph. Add a CI link/path validator.

## CONS-02 — English-only guidance is violated by root build/config comments

- **Files:** `Cargo.toml:30-34,54,62,145-147,158-169`; `docs/agents/*` states English-only as mandatory
- **Modules:** Workspace manifest comments and governance
- **Severity:** **Medium**
- **Explanation:** The root manifest contains Vietnamese comments while `AGENTS.md` and `docs/agents/code-style.md` explicitly require all repository written content to be English.
- **Why it matters:** This is a direct project convention violation and signals that automated consistency checks are absent.
- **Recommended solution:** Translate comments to English and add a review/lint check for non-English repository content if the rule is truly mandatory.

## CONS-03 — Error and cancellation conventions differ by transport

- **Files:** `crates/ssh/src/listener.rs:132-193`; `crates/local-shell/src/listener.rs:107-159`; `crates/ssh/src/sftp.rs:200-248`; `crates/terminal/src/contracts.rs`
- **Modules:** Backend transport APIs
- **Severity:** **High**
- **Explanation:** SSH uses bounded `try_send` and explicit close flags; local input uses unbounded standard `mpsc::Sender::send`; SFTP upload/download ignore command-send errors and return channels that may simply close; the public session API returns no input errors.
- **Why it matters:** Feature code cannot apply one consistent retry/user-notification policy.
- **Recommended solution:** Define common typed transport semantics: ordered input, coalescible resize, close, cancellation, and event delivery. Implement the same result behavior for local/SSH/SFTP.

## CONS-04 — Comments sometimes describe behavior that the code no longer has

- **Files:** `crates/ssh/src/sftp_task.rs:251-255`; `crates/terminal/src/security_policy.rs:26-30`; `crates/sftp-ui/src/panel.rs:83-87`; `crates/terminal/src/contracts.rs:1-8`
- **Modules:** SFTP cleanup, security policy, task ownership, contracts
- **Severity:** **Medium**
- **Explanation:** The SFTP cleanup comment says the map contains only running transfers although tokens remain. Security policy comments advertise notification rate/queue limits that are unused. Some comments say tasks can be detached while fields actually retain task handles, and contracts describe a future API.
- **Why it matters:** Misleading comments are worse than missing comments in lifecycle/security code.
- **Recommended solution:** Make comments state current guarantees, add tests for each claimed invariant, and annotate future designs as RFCs rather than exporting them as production contracts.

## CONS-05 — Persistence error policy is inconsistent

- **Files:** `crates/settings/src/terminal_config/mod.rs:117-132`; `crates/settings/src/ui_config.rs:78-89`; `crates/session-ui/src/session_state.rs:120-145`; `crates/sftp-ui/src/persistence.rs:14-33`
- **Modules:** JSON persistence
- **Severity:** **Medium**
- **Explanation:** Missing files, invalid JSON, serialization failure, and write failure are handled differently: defaults may be written, sessions become empty, read errors are ignored, and some errors are returned as `anyhow::Result`.
- **Why it matters:** Users cannot predict whether bad data is preserved, replaced, or silently discarded.
- **Recommended solution:** Adopt a shared recovery policy: preserve/quarantine invalid input, report a clear warning, write atomically, and return typed status to callers.

## CONS-06 — Workspace membership does not follow its own R11 rule

- **Files:** `Cargo.toml:1-23`; `crates/highlight/Cargo.toml`; `docs/agents/crate-dependency-rules.md:42-45`
- **Modules:** Workspace membership
- **Severity:** **Low/Medium**
- **Explanation:** `oneterm-highlight` is a path dependency and appears in the architecture table, but it is not explicitly listed in root `members`, despite R11 requiring new path crates to be listed.
- **Why it matters:** Explicit membership is part of discoverability and tooling behavior.
- **Recommended solution:** Add `crates/highlight` to `members` and verify all path crates with a script.

## Consistency strengths

- Naming is generally idiomatic Rust and package names follow `oneterm-*`.
- Feature initialization ownership is consistently implemented through `init` functions.
- Security logging masks password values and uses `tracing`/`log` rather than printing credentials.
- The project has explicit rules; the main gap is keeping them synchronized with current code.

## Current remediation status (2026-07-22)

The findings above were recorded before the reliability, maintainability, readability, and
simplicity remediations. The current implementation addresses the documented consistency gaps:

- CONS-01: Current paths are indexed in `docs/architecture.md`; historical/design records are
  labeled, and `scripts/check-doc-paths.py` validates the current architecture index.
- CONS-02: Contributor-facing comments and documentation are checked by `scripts/check-english.py`;
  user-facing locale translations remain data and are intentionally excluded.
- CONS-03: Local and SSH terminal input use typed results, bounded event queues, FIFO delivery,
  and explicit close/cancellation behavior. SFTP transfer enqueue failures now reach their reply
  channel, while cancellation and close failures are logged with operation context.
- CONS-04: Lifecycle, transfer cleanup, security policy, and terminal contract comments reflect
  current behavior rather than the removed future contracts.
- CONS-05: Missing, invalid, and unreadable persistence files now follow one documented policy;
  malformed JSON is quarantined, read failures are reported, and writes remain atomic.
- CONS-06 and CONS-07: `crates/highlight` is an explicit workspace member, and CI runs the
  dependency, UI-fork, architecture-path, and contributor-language checks.
