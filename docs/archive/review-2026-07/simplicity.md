# Simplicity Review

**Score: 5.0 / 10**

## SIMPLE-01 — An unused parallel contract API adds conceptual surface

- **Files:** `crates/terminal/src/contracts.rs:1-151`; `crates/terminal/src/lib.rs:6-33`; `crates/terminal/src/session.rs:155-407`
- **Modules:** Terminal engine contracts
- **Severity:** **Medium**
- **Explanation:** `contracts.rs` exports `TerminalRenderer`, `TerminalInput`, `TerminalLifecycle`, and `TerminalModeQuery`, but its own comments state that consumers do not use them and migration is future Phase 5 work. Production still uses the monolithic trait.
- **Why it matters:** Two competing abstractions increase cognitive load and invite partial migrations. The typed errors look authoritative but do not govern actual writes.
- **Recommended solution:** Either schedule and execute one incremental migration with compile-time adoption, or remove the unused public traits and retain a short design RFC. Do not maintain a second API indefinitely.

## SIMPLE-02 — Pointer-keyed per-backend state is an indirect identity mechanism

- **Files:** `crates/sftp-ui/src/browser_state.rs:35-40,42-125`; `crates/sftp-ui/src/panel.rs:44-69,222-252`
- **Modules:** SFTP state restoration
- **Severity:** **Medium**
- **Explanation:** The implementation converts an `Arc` address into a `usize` key and mirrors state both in the panel and global map.
- **Why it matters:** It is difficult to reason about identity, lifetime, cleanup, and pointer reuse. The duplicate active-view/store state creates synchronization code and failure modes.
- **Recommended solution:** Give each session a stable `SessionId`, make the global store authoritative, and derive the active view from that state. Keep only transient input/focus state in the panel.

## SIMPLE-03 — `docks.json` field injection duplicates persistence logic

- **Files:** `crates/workspace/src/layout/workspace/persistence.rs:55-113`; `crates/sftp-ui/src/persistence.rs:23-33`
- **Modules:** Layout and SFTP table persistence
- **Severity:** **Medium**
- **Explanation:** Two modules read the entire JSON document, mutate a field, serialize, and write it. The workspace then has special preservation logic for the SFTP field.
- **Why it matters:** A workaround for shared ownership becomes a distributed protocol with hidden ordering assumptions.
- **Recommended solution:** Define a versioned top-level persisted state owned by one module/service. Pass typed optional SFTP table state into the workspace serializer rather than patching arbitrary JSON values.

## SIMPLE-04 — Multiple process/global callback bridges solve real layering constraints but are over-indirect

- **Files:** `crates/state/src/commands.rs:15-40`; `crates/app/src/init.rs:50-61`; `crates/terminal/src/factory.rs:48-58`
- **Modules:** Feature composition and session creation
- **Severity:** **Medium**
- **Explanation:** Function-pointer registries and a `OnceLock<Arc<dyn SessionFactory>>` are used to avoid shell→feature and UI→backend edges. The pattern is defensible, but missing registration is a runtime condition and function pointers cannot carry scoped dependencies.
- **Why it matters:** Callers cannot see what they require in constructors, and tests need global setup. It also makes future multi-app contexts difficult.
- **Recommended solution:** Keep the composition root, but inject a single app service registry/entity into shell/features where GPUI permits it. Use the global only as a compatibility adapter and validate duplicate/missing registration.

## SIMPLE-05 — Recursive SFTP code performs two separate tree walks

- **Files:** `crates/ssh/src/sftp_task.rs:537-592`
- **Modules:** Directory upload
- **Severity:** **Medium**
- **Explanation:** The code first recursively collects files, then recursively collects directories, then reads each file again. Errors in the second walk are ignored.
- **Why it matters:** It duplicates traversal logic and makes semantics differ between discovery and directory creation.
- **Recommended solution:** Use one traversal yielding directory-create and file-upload operations through a bounded queue, or create directories as the stream discovers them. Centralize remote path joining.

## Simplicity strengths

- `SessionFactory` is a small, understandable interface despite its global installation.
- Terminal security and URL policy centralize repeated checks instead of duplicating them at every UI call site.
- Feature `init()` functions keep registration ownership local.
- Bounded channel and cancellation designs avoid more complex scheduler machinery in the common path.
