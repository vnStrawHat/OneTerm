# Persistence ownership and migration conventions

OneTerm centralizes file lifecycle mechanics in `oneterm_core::persistence` while
each domain crate owns its schema and migrations.

## Shared mechanics

All new user-owned JSON writes must use `atomic_write` or `update_json_file` from
`oneterm-core`. These functions provide same-directory temporary files, per-path
serialization, backups, durable replacement, and cleanup. Invalid documents are
moved with `quarantine_file` before defaults are persisted.

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
| `terminal.json` | `oneterm-settings` | Terminal configuration schema and defaults. |
| `ui_config.json` | `oneterm-settings` | UI theme/font/key-binding schema. |
| SSH session store | `oneterm-session-ui` | Saved host/session schema. |
| `update_config.json` | `oneterm-update` | Auto-update channel, cache, proxy/TLS preferences, and skip/version state. |
| `docks.json` document model | `oneterm-state` | `DockDocument` is the typed top-level schema and the only read/update API. |
| `docks.json` dock fields | `oneterm-workspace` | Dock layout and shell-owned display fields. |
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
