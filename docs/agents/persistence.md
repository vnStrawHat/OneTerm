# Persistence ownership and migration conventions

OneTerm centralizes file lifecycle mechanics in `oneterm_core::persistence` while
each domain crate owns its schema and migrations.

## Shared mechanics

All new user-owned JSON writes must use `atomic_write` or `update_json_file` from
`oneterm-core` (`update_secret_json_file` for a document that holds secret material, see
"Secrets at rest" below). These functions provide same-directory temporary files, per-path
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

**Exception: `vault.json` is never quarantined and never replaced by a default**
(`IN-0050`, `DEC-0021`). A default would be "no vault", and the next set-up would orphan
every sealed value. A file that does not parse loads as `VaultStatus::Damaged`, one this
binary does not support (newer schema, other algorithm, parameters out of bounds) as
`VaultStatus::Unsupported`; both are left byte for byte where they are, and only Reset
removes the file.

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
| `terminal.json` | `oneterm-settings` | Terminal configuration schema and defaults, including the global local/SSH output-logging policy, destination, and write mode; the `ssh` keepalive policy (`enabled = true`, interval `30` seconds by default, validated to `5..=3600`, unanswered-request limit `3` by default, validated to `1..=20`, captured by newly opened sessions); and the `sftp` group (SFTP browser Edit workflow: editor mode/program/args and the max edit file size). |
| `ui_config.json` | `oneterm-settings` | UI theme/font/key-binding schema. `UiConfig::observe_theme` is the only writer of `theme_name`/`ui_font_size`; it coalesces `Theme` notifications that leave both unchanged. `show_fps` (optional, omitted while `false`, which is also what an absent field means) switches the FPS HUD on (`US-0151`); `UiConfig::set_show_fps` is its only writer, shared by the toggle action and the Settings switch. `lock_after_idle_minutes` (optional, omitted while unset; `US-0159`) is the idle lock's timeout: absent means 15, every read clamps to 1..=480 (no "Never", `IN-0050` Q2) without rewriting the file, and a value that is not a non-negative integer loads as absent with a warning instead of quarantining the document; `UiConfig::set_lock_after_idle_minutes` (Settings > Security) is its only writer. `right_dock_mode` is `ssh_client` or `none`; the withdrawn `agent` (`DEC-0022`) reads as `none` and is written back as `none`, and the withdrawn `agent_stale_threshold_ms` is ignored on read and dropped on the next write (no schema bump; test `a_config_from_the_agent_panel_era_loads_and_heals`). |
| SSH session store (`ssh_session.json`) | `oneterm-session-ui` | Saved host/session schema, including the default-compatible terminal-logging tri-state (`inherit` / `on` / `off`) and the authentication preference (`password` default, `private_key` with `key_path`, or `agent`; never a secret). `jump_host` (optional) is the `id` of the saved session to connect through first; the chain is resolved at use, so a dangling id is reported, never repaired silently. `port_forwards` (optional, omitted when empty) lists `PortForward` specs (`kind` = `local` / `remote` / `dynamic`; bind addresses default to loopback). `agent_forwarding` (optional, omitted when false) forwards the local SSH agent to the remote host. `sealed_password` (optional, omitted when absent; `US-0158`, `DEC-0021`) is an `otv1:` envelope of the login password bound to the session's host, port and username: no schema bump (an older binary ignores it and drops it on its next write, sessions kept), parsed leniently (a non-string value loads as absent, never quarantines), kept byte for byte when it is from a newer OneTerm, valid only with the `password` method. Writes go through `atomic_write_sealed` (owner-only on Unix on every write; `.bak` scrubbed only when the write drops or replaces a sealed value); a vault reset wipes every `sealed_password`. Schema v2 gives every session a stable `id` and records `next_session_id`; v0/v1 files are migrated in memory and re-saved on load. Whole-document writes are coalesced through a single-flight queue so the newest snapshot always wins. |
| `update_config.json` | `oneterm-update` | Schema owner. Two field-level writers through `update_json_file`: preferences (`oneterm-settings-ui` persist queue via `UpdateConfig::save_preferences`) and check cache (`UpdateManager` via `UpdateCheckCache::save`). See `docs/auto-update.md`. |
| `vault.json` | `oneterm-vault` | Master-password vault header (`IN-0050`, `DEC-0021`), schema 1: `vault_id` (32 lowercase hex), `kdf` (`argon2id`, version 19, `memory_kib` / `iterations` / `parallelism`, base64 `salt`) and `wrapped_key` (an `otv1:` envelope of the data key). Never holds a password or a key that opens anything without it. Never quarantined or defaulted; only Reset removes it (with `vault.bak`). Every write goes through `oneterm_vault`'s one write function. Set-up is create-only: any file already there, whatever it holds (even `{}` or not JSON), is refused with `AlreadyInitialised` and left untouched. A password change or rehash runs the same parse `load` uses, inside the lock, and writes only when the file is `Ready` and still exactly the header the process unlocked from (same id, salt and wrapped key); otherwise it refuses with `VaultReplaced` (missing, another vault, or `Damaged` / `Unsupported`) or `ChangedElsewhere` (same vault, rewrapped by another process). It touches only `kdf`'s known fields and `wrapped_key` (unknown fields are preserved). Reset is refused for an `Unsupported` file. Set-up and rewrap use `update_secret_json_file`, so `vault.bak` always equals the new document and the files are `0600` on Unix. Fixtures: `crates/vault/tests/fixtures/persistence/`. |
| `docks.json` document model | `oneterm-state` | `DockDocument` is the typed top-level schema and the only read/update API. `schema_version` is **2**: v1 -> v2 migrates the SFTP table's column layout (drops the stored widths, keeps only the columns the user hid by hand), in `migrate_sftp_table_state_to_v2` — the field's owner may not migrate a shared document, so the document's owner does it. Fixture: `crates/state/tests/fixtures/persistence/docks-v1.json`. |
| `docks.json` dock fields | `oneterm-workspace` | Dock layout and the shell-owned `zoomed_panel` field. A right dock saved as the withdrawn `agent_panel` (`DEC-0022`) is replaced by the SSH Client panel at start-up and is gone from the next write (test `a_saved_agent_panel_right_dock_is_replaced_by_the_ssh_client_panel`). The exit write runs synchronously from the workspace's `on_app_quit` / `on_release` hooks. |
| `docks.json.sftp_table_state` | `oneterm-sftp-ui` | SFTP browser field only, represented by `oneterm_core::SftpTableState`: remote column widths/visibility plus the dual-pane mode (`expanded`, default `false`) and the Local pane's last directory (`local_dir`, omitted when unknown; the home directory is used instead). It carries **no version of its own** — the document's `schema_version` covers it, and the v1 -> v2 step above is what reconciles a pre-US-0124 layout (US-0124; design: `docs/spec-intakes/IN-0042-ux-polish-round-1/low-level-design/sftp-table-state-migration.md`). The Name column's width is derived from the panel width and is never stored. The column keys the migration filters by are `oneterm_core::SFTP_TABLE_COLUMNS`, kept in step with the browser's own `SortColumn` by a test. |

A crate may mutate only fields it owns. Callers of the shared dock document must
use `oneterm_state::dock_persistence`; other shared documents must use
`update_json_file`. Read-modify-write sequences outside those transactions are
not allowed. Only the document owner quarantines: `update_dock_document` moves an
invalid `docks.json` aside, applies the update to a default document, and reports
`DockUpdateOutcome::RecoveredFromInvalidData` so the caller can log the recovery
(`oneterm-state` has no logger of its own); feature crates never quarantine it.

## Secrets at rest

A secret is persisted only sealed (`DEC-0021`): as an envelope string
`otv1:` + base64( kid[8] | nonce[12] | ciphertext | tag[16] ), produced by
`oneterm_vault::UnlockedVault::seal` and stored in the document of the data it belongs to
(the first one is `ssh_session.json`'s `sealed_password`, `US-0158`). The owning
document keeps the value as an opaque `oneterm_vault::SealedSecret`: any string loads, and it
is written back byte for byte, so an envelope from a newer OneTerm (`otv2:`) or a hand-edited
value survives every load and save. Whether it opens is decided only when it is opened; a
value that is not a v1 envelope is never used as a secret. A read-modify-write that retires a
secret uses `update_secret_json_file` (backup scrubbed, owner-only on Unix; `vault.json`). A
whole-document writer whose document holds sealed values uses `atomic_write_sealed(path,
bytes, retires_secret)`: every file it creates is owner-only on Unix, and `.bak` is replaced by
the new document only when `retires_secret`, shown the document being replaced under the lock,
says this write drops or replaces a sealed value (`ssh_session.json`; ordinary session edits
keep the previous generation). `remove_document` removes a document and its backup under the
document's lock, and refuses in an elevated process.

## Migration rules

- Add an explicit top-level schema version when the first incompatible schema
  change is introduced. Absence of the version denotes the original schema.
- Migrations are sequential and idempotent: `N -> N+1`, never an unbounded direct
  jump with undocumented assumptions.
- Migrate a parsed value in memory, validate it as the destination domain type,
  then persist through the shared atomic-write path.
- Preserve the pre-migration `.bak`; quarantine input that cannot be parsed or
  migrated without data loss.
- Never let a feature crate migrate another owner's fields in a shared document — and a
  feature crate may not migrate **its own** field in a shared document either: the document's
  version is what says which schema the fields are in, so the migration belongs to the document's
  owner. `docks.json`'s v1 -> v2 step (the SFTP table's column layout) is the worked example.
- A migration that cannot keep every value must say which values it drops and why they are not
  worth preserving, in the owning packet or a detail design. The pre-migration document survives
  one generation as the shared write path's `.bak`.

## Fixture convention

Migration fixtures live under the owning crate's `tests/fixtures/persistence/` and
use `<document>-v<version>.json` plus `<document>-invalid-<case>.json`. Tests must
use a temporary directory and cover: successful migration, idempotent current
schema loading, invalid-file quarantine, backup preservation, and concurrent
updates for shared documents. Tests must never write to the developer's real
configuration directory.

## Elevation: a process that reads and never writes

An **elevated** OneTerm process (`IN-0043`, `DEC-0019` M4) persists nothing. Before adding a
write, a document, or a recovery step, check it against this rule — it is a property of the
process, not of any one document, so a new writer inherits it only if it is put in the right
place.

| Document | Elevated |
|---|---|
| `ui_config.json`, `terminal.json` | read; every write refused by `write_refusal(elevated)`, the one function both shared write entry points ask |
| `docks.json` | **not read either.** `load_layout` restores side docks *by name*, so reading it would build the SSH Client panel in a window that must not have one. Writes refused in `save_state_to` |
| `ssh_session.json`, `update_config.json` | read; writes refused in each document's own shared write function |
| `vault.json` | read; set-up, password change and reset refused with `VaultError::ReadOnly` in `oneterm-vault`'s one write function (`document::write`) |
| terminal logs | never opened. An arbitrary path from `terminal.json` plus `write_mode: overwrite` is a create-or-truncate primitive under an administrator token |
| crash reports | own store, `crashes/elevated/` |

Three rules that are easy to miss, and were all missed once:

1. **A first-run default write is a write.** `UiConfig::load_from` and
   `TerminalConfig::load_from` create the file when it is absent — *during the load*, so a
   flag set after loading does not stop it. Under over-the-shoulder elevation that directory
   belongs to another account.
2. **A quarantine is a write.** `quarantine_file` renames the user's document, and the guards
   on the write entry points do not cover it. One guard lives inside `quarantine_file` itself
   so every caller is covered at once; an elevated process meeting a corrupt document starts
   on the defaults and leaves the file for the ordinary one.
3. **Put the guard in the shared function, never at the call sites.** Every guard above sits
   at the deepest point its document's writes pass through, so a caller added later is
   covered by construction. The one place that was gated at a *builder* instead of at the
   *restore* is the one that shipped a hole.

The switch is `oneterm_core::elevation::is_restricted()` — true for an elevated token **and**
for a token query that failed. There is deliberately no `is_elevated()` beside it.
