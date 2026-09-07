# Work: Prove docks.json loads unchanged across the dock redesign

ID: US-0042
Intake: IN-0017
Created: 2026-09-07

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [ ] Implemented
- [x] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change
- Risk lane: high-risk (persisted user data; a wrong answer silently discards saved workspaces)
- Spec Intake: IN-0017

## Outcome

A `docks.json` written by the pre-migration build loads into the migrated build and produces the
same workspace — same dock placements, open flags, sizes, panel order, active tabs, zoom and SFTP
column state — proven by a fixture test rather than assumed from upstream's compatibility claim.
`CURRENT_SCHEMA_VERSION` and `MAIN_DOCK_VERSION` are unchanged, or the deviation is an explicit,
recorded decision.

## Scope

- [x] In scope: commit a pre-migration layout fixture; round-trip it through `DockDocument` and
      `DockArea::load` → `dump`; exercise zoom/SFTP owner fields; retain the corrupted-file
      quarantine regression.
- [x] Out of scope: no persisted-schema change was needed.

## Acceptance

- [x] A genuine pre-migration layout body is committed. It came from the surviving 2026-08-17
      `.docks.json.tmp-9756-5` artifact (two terminal tabs, active index 1, 443 px open right dock).
      No surviving pre-migration file contained zoom/SFTP fields; those owner fields were added to
      the fixture as explicit compatibility vectors rather than represented as captured bytes.
- [x] `DockDocument` deserializes `schema_version`, `zoomed_panel`, `sftp_table_state`, and the
      flattened `DockAreaState` fields intact.
- [x] `DockArea::load(fixture)` → `dump()` preserves panel names/order, active index, dock
      placement, open state, and size (split pixel measurements are normalized before comparison).
- [x] The recorded terminal group is actually zoomed through `DockArea::set_zoomed_in`, and the
      test asserts `DockArea::is_zoomed()`.
- [x] The right dock's `PanelInfo::panel(Null)` leaf round-trips through the compatibility writer.
- [x] All five persisted panel names resolve through `PanelRegistry` with a `PanelHandle` and title.
- [x] Corrupted `docks.json` quarantine remains covered by `dock_persistence` tests.
- [x] `CURRENT_SCHEMA_VERSION == 1` and `MAIN_DOCK_VERSION == 3` remain unchanged.

## Documentation

### Owning Docs Reviewed

- `docs/agents/persistence.md` — the owning contract for persisted schemas and storage mechanics,
  including the schema-version and quarantine rules.
- `docs/gui-layout.md` — what the persisted layout represents.
- `docs/sftp-browser-design.md` — `sftp_table_state`, the one feature-owned field inside
  `docks.json`.
- `docs/agents/error-policy.md` — recovery rules for unreadable persisted state.

### Documentation Action

No contract change expected: the schema is asserted unchanged, so `docs/agents/persistence.md`
already describes the correct behavior. Record the reviewed paths and this reason unless the
round-trip forces a schema change, in which case `persistence.md` and the migration table must be
updated in the follow-up packet.

Reason: upstream states the v0.5.0 JSON shape is retained, and the v0.6.0 `DockAreaState` /
`PanelState` / `PanelInfo` / `DockPlacement` serde tags are frozen and pinned by upstream tests —
so the expected outcome is a proof, not an edit.

### Reconciliation

Before completion, confirm the recorded no-change reason still holds against the actual test
results, or link the follow-up packet.

## Context

- `DockDocument` owns `schema_version` (1), `zoomed_panel` and `sftp_table_state`, and
  `#[serde(flatten)]`s the gpui-component-owned `version` / `center` / `left_dock` / `right_dock` /
  `bottom_dock`. `persistence.rs` discards any state whose `version != Some(MAIN_DOCK_VERSION)`.
- v0.6.0 keeps the persisted container names `"StackPanel"` and `"TabPanel"` even though the Rust
  types are now `PaneNode::Split` and `TabGroup`, so container naming is not a concern.
- `DockState`'s fields became private with accessors; OneTerm does not construct it, but any code
  pattern-matching its fields must move to `panel()` / `placement()` / `size()` / `open()`.
- The OneTerm-specific unknown is the right dock: `SshClientPanel::dump` deliberately writes
  `PanelInfo::panel(Null)`, and the 0.5.2 load path wrapped that leaf back into a tabs container.
  0.6.0 has no `Panel` node variant at all.
- Detail design: `low-level-design/03-persistence-compatibility.md`.

## Plan

- [x] Recover a genuine pre-migration layout artifact and commit its layout body as
      `crates/state/src/fixtures/docks-0.5.2.json`; enrich only the absent zoom/SFTP owner fields.
- [x] Add the `crates/state` document-level field-preservation test.
- [x] Add the `crates/workspace` load/dump, zoom, right-leaf, and owner-field round-trip test.
- [x] Retain and run corrupted-file quarantine coverage.
- [ ] Manual: launch against a copied real profile and compare screenshots. No P0 screenshots were
      captured, so this remains an explicit visual-E2E gap.

## Decisions

- `docs/decisions/DEC-0006-depend-on-published-gpui-component-0-6.md`
- A new decision is required if the fallback is taken: bumping `MAIN_DOCK_VERSION` discards the
  user's saved layout and must not be a quiet commit.

## Verification Plan

Unit: fixture → `DockDocument` field-preservation test; corrupted-file quarantine test.
Integration: fixture → `DockArea::load` → `dump` structural equality, including zoom restore.
E2E (manual): a copied real user profile launches with a visually unchanged workspace — dock width,
tab order, active tab, zoom state, SFTP column widths.
Platform: Windows, where the release path resolves `~/.OneTerm/docks.json`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

The historical source is `%USERPROFILE%/.OneTerm/.docks.json.tmp-9756-5`, timestamped
2026-08-17, before the dependency migration. Its layout body contains two terminal children,
active index 1, and an open 443 px `ssh_client_panel` right dock. The committed fixture preserves
that body and adds `zoomed_panel = "terminal"` plus representative SFTP width/visibility maps
because none of the surviving historical files carried those optional fields.

`cargo test -p oneterm-state dock_persistence` passed 9 tests. `cargo test -p
oneterm-workspace` passed 9 tests, including actual zoom restoration, all-five-name registration,
and load/dump/save preservation. Harness `story verify US-0042` passes. The serial full local CI
gate also passed. The unrun gap is visual comparison against a real copied profile/screenshots;
the enriched owner fields prove serialization and restoration code, not that an untouched 0.5.2
profile with those exact optional values was recovered.

## Handoff

The fixture capture must happen before US-0039. The tests depend on US-0040.
