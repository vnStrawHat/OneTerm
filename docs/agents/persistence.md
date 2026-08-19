# Persistence ownership and migration conventions

OneTerm centralizes file lifecycle mechanics in `oneterm_core::persistence` while
each domain crate owns its schema and migrations.

## Shared mechanics

All new user-owned JSON writes must use `atomic_write` or `update_json_file` from
`oneterm-core`. These functions provide same-directory temporary files, per-path
serialization, backups, durable replacement, and cleanup. Invalid documents are
moved with `quarantine_file` before defaults are persisted.

### Load outcomes

Document loaders distinguish three read outcomes. A missing file selects the
documented defaults (and, for `terminal.json` / `ui_config.json`, creates the
file). A file that does not parse or migrate is quarantined with a recovery log
and defaults are used. Any other read failure (permissions, I/O) is returned as
`AppError::ConfigLoad { document, message }` — never a string — and the file is
left untouched: the owner keeps in-memory defaults with a `persist_blocked`
flag (`TerminalSettings`, `UiConfig`, `SshSessionStore`) and refuses to write
them back over the possibly valid document until the next start. `docks.json`
readers get `Ok(None)` for "no layout saved yet" and `ConfigLoad` for anything
else; only `update_dock_document` recovers by quarantining.

### Cross-process guarantee

Persistence transactions are serialized with a sibling `.<document>.lock` file and
an operating-system advisory lock. The lock is held across the complete operation:
read, mutation, serialization, backup, flush, and replacement. Lock files are
persistent coordination artifacts and may remain after the process exits; the
operating system releases the actual lock when the file handle closes or the
process terminates.

`update_json_file` provides inter-process-safe read-modify-write semantics and must
be used for shared documents. Whole-document `atomic_write` calls are also
serialized, but use explicit last-completed-writer-wins semantics; they do not
merge independent snapshots. Domain owners that need field-level merging must use
a transaction instead of writing a stale snapshot.

Filesystem operations are blocking. UI handlers may only create an owned snapshot
and schedule persistence on GPUI's background executor. They must not call
`atomic_write`, `update_json_file`, configuration `save`, or quarantine operations
directly on the UI thread.

## Schema owners

| Document | Owner | Notes |
|---|---|---|
| `terminal.json` | `oneterm-settings` | Terminal configuration schema and defaults, including the global local/SSH output-logging policy, destination, and write mode. |
| `ui_config.json` | `oneterm-settings` | UI theme/font/key-binding schema. `UiConfig::observe_theme` is the only writer of `theme_name`/`ui_font_size`; it coalesces `Theme` notifications that leave both unchanged. |
| SSH session store (`ssh_session.json`) | `oneterm-session-ui` | Saved host/session schema, including the default-compatible terminal-logging tri-state (`inherit` / `on` / `off`). Schema v2 gives every session a stable `id` and records `next_session_id`; v0/v1 files are migrated in memory and re-saved on load. Whole-document writes are coalesced through a single-flight queue so the newest snapshot always wins. |
| `update_config.json` | `oneterm-update` | Schema owner. Two field-level writers through `update_json_file`: preferences (`oneterm-settings-ui` persist queue via `UpdateConfig::save_preferences`) and check cache (`UpdateManager` via `UpdateCheckCache::save`). See `docs/auto-update.md`. |
| `docks.json` document model | `oneterm-state` | `DockDocument` is the typed top-level schema and the only read/update API. |
| `docks.json` dock fields | `oneterm-workspace` | Dock layout and the shell-owned `zoomed_panel` field. The exit write runs synchronously from the workspace's `on_app_quit` / `on_release` hooks. |
| `docks.json.sftp_table_state` | `oneterm-sftp-ui` | SFTP table field only, represented by `oneterm_core::SftpTableState`. |

A crate may mutate only fields it owns. Callers of the shared dock document must
use `oneterm_state::dock_persistence`; other shared documents must use
`update_json_file`. Read-modify-write sequences outside those transactions are
not allowed. Only the document owner quarantines: `update_dock_document` moves an
invalid `docks.json` aside, applies the update to a default document, and reports
`DockUpdateOutcome::RecoveredFromInvalidData` so the caller can log the recovery
(`oneterm-state` has no logger of its own); feature crates never quarantine it.

## Migration rules

- Add an explicit top-level schema version when the first incompatible schema
  change is introduced. Absence of the version denotes the original schema.
- Migrations are sequential and idempotent: `N -> N+1`, never an unbounded direct
  jump with undocumented assumptions.
- Migrate a parsed value in memory, validate it as the destination domain type,
  then persist through the shared atomic-write path.
- Preserve the pre-migration `.bak`; quarantine input that cannot be parsed or
  migrated without data loss.
- Never let a feature crate migrate another owner's fields in a shared document.

## Fixture convention

Migration fixtures live under the owning crate's `tests/fixtures/persistence/` and
use `<document>-v<version>.json` plus `<document>-invalid-<case>.json`. Tests must
use a temporary directory and cover: successful migration, idempotent current
schema loading, invalid-file quarantine, backup preservation, and concurrent
updates for shared documents. Tests must never write to the developer's real
configuration directory.
