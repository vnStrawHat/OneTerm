# Detail design: migrating `docks.json` for the SFTP table's column layout

Intake: IN-0042 · Owning packet: US-0124 · Concern: persisted state · Lane: high-risk

## Why this is high-risk

`docs/HARNESS.md` lists *migrations* and *data loss* among the high-risk triggers, and this
change is both: it bumps a persisted document's schema version and **drops** part of what that
document stores. What is lost is presentation state only — column widths the browser re-derives
on its next frame — and no user data, credential or layout of the workspace is touched. That is
an argument for the change being safe, not for skipping the lane: the document is shared with the
shell, it is read at every start, and getting the migration wrong resets a user's dock.

## What changes

`docks.json` is owned by `oneterm-state` (`crates/state/src/dock_persistence.rs`) and already
carried a top-level `schema_version` with a migration hook. Before this rework, US-0124 instead
put a second, private version inside the feature-owned `sftp_table_state` field and dropped the
old layout at read time inside the UI delegate — which bypassed every rule in
`docs/agents/persistence.md` § Migration rules. That field is gone.

| | Before this rework | After |
|---|---|---|
| Where the version lives | `sftp_table_state.version` (feature-owned field) | `docks.json`'s own `schema_version`, `1` → **`2`** |
| Where the old layout is dropped | `SftpTableDelegate::apply_persisted_state` (a feature crate, at read time) | `dock_persistence::migrate_sftp_table_state_to_v2`, in the document owner, inside `migrate_json_value` |
| What happens to the file | nothing until the next debounced save | the migrated document is written by the next shared update, with the pre-migration file kept as `docks.bak` |
| Invalid document | (unchanged) | (unchanged) `update_dock_document` quarantines it and reports `RecoveredFromInvalidData` |

## The v1 → v2 step

```text
sftp_table_state.column_widths      -> removed entirely            (lossy, see below)
sftp_table_state.version            -> removed                     (the first cut's field)
sftp_table_state.column_visibility  -> only entries that are `false`
                                       and name a column that still exists
sftp_table_state.expanded           -> untouched
sftp_table_state.local_dir          -> untouched
every other field of the document   -> untouched
```

**Why dropping the widths is not a loss worth preserving.** Every v1 width was measured against a
table whose Name column was a fixed 320 px. Name is now derived from the panel width and is never
stored; the other five columns' v1 widths were the v1 defaults, so re-defaulting them discards a
default, not a choice. A user who had dragged one column wider loses that drag — the one real
loss, and the reason this is documented rather than silent.

**Why visibility is migrated rather than dropped.** Under the v1 defaults *every* column was
visible, so a `true` entry records nothing the user did; a `false` entry is a column they hid by
hand. Keeping only the `false` entries preserves every decision a user could actually have made
while letting the v2 defaults decide the rest — which is what keeps a narrow dock free of a
horizontal scrollbar for an existing user too, the point of `F29`. An entry naming a column that
no longer exists is dropped with the widths.

The step is idempotent (a v2 document has no `column_widths` to remove and no `true` entries to
filter) and runs inside the sequential `migrate_json_value` loop, so a version-less document walks
`0 → 1 → 2` and gets it as well.

## Failure and recovery

- **Lossy but recoverable:** the shared write path keeps the pre-migration document as
  `docks.bak`, so the widths survive one generation on disk. A `read` alone does not rewrite the
  file; the migrated document reaches disk on the next shared update.
- **Unparseable document:** unchanged behaviour — `update_dock_document` quarantines the file,
  applies the update to a default document and reports `RecoveredFromInvalidData`, which the shell
  logs. Only the document owner quarantines; the SFTP feature never does.
- **Future version:** `migrate_json_value` refuses a `schema_version` above the current one, so a
  document written by a newer build is reported as `ConfigLoad` rather than read with fields this
  build does not understand.
- **Not an object / not a dock document:** the migration leaves a non-object `sftp_table_state`
  alone; the typed parse then rejects the document and the recovery path above takes over.

## Proof

Fixture: `crates/state/tests/fixtures/persistence/docks-v1.json` — a complete v1 document with
dock fields, `zoomed_panel`, all six column widths, four visible and two hand-hidden columns,
`expanded: true` and a `local_dir`.

| Test (`crates/state/src/dock_persistence.rs`) | Covers |
|---|---|
| `v1_document_migrates_the_sftp_table_layout` | the fixture: widths gone, only the two hides kept, `expanded`/`local_dir`/`zoomed_panel` intact, version now 2 |
| `version_less_document_migrates_and_round_trips` | a document with no `schema_version` migrates too; an unknown column key is dropped; writing it back stores v2, keeps the pre-migration bytes as `docks.bak`, removes the first cut's `version` field, and a second write changes nothing |
| `future_schema_version_is_refused` | a `schema_version` above the current one is a `ConfigLoad`, not a silent read |
| `pre_migration_dock_fixture_preserves_document_fields` | the shipped 0.5.2 document still deserializes field for field (no migration step) |
| `a_migrated_pre_version_state_gives_the_new_defaults` (`oneterm-sftp-ui`) | what the browser makes of a migrated state |
| `the_persisted_column_keys_match_the_columns` (`oneterm-sftp-ui`) | `SFTP_TABLE_COLUMNS` (what the migration filters by) and `SortColumn` name the same columns |

End-to-end: a real pre-US-0124 `docks.json` seeded into the walk's configuration directory comes
up with the v2 defaults — `evidence/US-0124-44b-seeded-old-state-after-connect.png`.
