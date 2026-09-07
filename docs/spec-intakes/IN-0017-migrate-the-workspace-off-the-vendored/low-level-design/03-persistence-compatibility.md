# Low-Level Design: docks.json compatibility across the 0.6.0 dock redesign

Intake: IN-0017
HLD: ../high-level-design.md
Topic: persistence-compatibility
Date: 2026-09-07

## Concern

Whether an existing user's `docks.json` still loads after the dock redesign, and what happens if
it does not. This is the only place in the migration where a mistake destroys user data (their
saved workspace layout), so it gets its own proof rather than riding along with the dock work.

## Design

### What is persisted, and who owns each part

`oneterm_state::dock_persistence::DockDocument` is the single read/update API for the file
(`target/docks.json` in debug, `~/.OneTerm/docks.json` in release):

```rust
pub struct DockDocument {
    pub schema_version: u32,          // OneTerm-owned, CURRENT_SCHEMA_VERSION = 1
    #[serde(flatten)]
    dock_fields: BTreeMap<String, Value>,   // gpui-component-owned: version, center, *_dock
    pub zoomed_panel: Option<String>, // OneTerm-owned
    pub sftp_table_state: Option<SftpTableState>, // OneTerm-owned (SFTP feature)
}
```

The flattened `dock_fields` are whatever `DockAreaState` serializes. Independently,
`OneTermWorkspace` carries `MAIN_DOCK_VERSION = 3` and `persistence.rs` discards a loaded state
whose `state.version != Some(MAIN_DOCK_VERSION)`.

### Evidence that the shape is unchanged

Three independent signals, all pointing the same way:

1. The v0.6.0 release notes state explicitly: "Dock persistence keeps the v0.5.0 JSON shape, but
   the Rust construction and extension APIs changed."
2. The v0.6.0 `gpui_base::dock::state` module carries the same `DockAreaState` (`version`,
   `center`, `left_dock`, `right_dock`, `bottom_dock`), `DockState` (`panel`, `placement`, `size`,
   `open`), `PanelState` (`panel_name`, `children`, `info`) and `PanelInfo`
   (`stack` / `tabs` / `panel` / `tiles`) with a doc comment saying the fields stay `pub` and the
   serde tags are frozen because it "mirrors a persisted, on-disk schema shipped to end users".
3. Upstream pins that with its own tests: `the_serde_tags_are_frozen` asserts the exact JSON for
   `PanelInfo::stack`, `PanelInfo::tabs` and `DockPlacement::Bottom`, and
   `the_shipped_fixture_still_deserializes` reads a full real-user fixture including
   `"StackPanel"` / `"TabPanel"` container names.

Note the third point carefully: the persisted **container names are still the 0.5.x strings**
`"StackPanel"` and `"TabPanel"`, even though the Rust types are now `PaneNode::Split` and
`TabGroup`. So container naming is not a migration concern.

### Plan: no schema change

- `CURRENT_SCHEMA_VERSION` stays `1`.
- `MAIN_DOCK_VERSION` stays `3`.
- `DockDocument` is unchanged.

This is a **claim to be proven, not an assumption**. The proof is a round-trip against a real
pre-migration file, because upstream's fixture is upstream's layout, not OneTerm's: OneTerm's
right dock persists a deliberate `PanelInfo::panel(Null)` leaf for `SshClientPanel`, and the
0.5.2 load path for `PanelInfo::Panel` wrapped that leaf back into a `DockItem::tabs`. How
v0.6.0's `from_state` treats a `panel` leaf — and whether `SshClientPanel`'s reworked layout
(see `02-dock-migration.md`) still dumps the same leaf — is OneTerm-specific and unverified.

### Proof procedure

1. **P0, before any code change:** run the current build, arrange a representative layout
   (multiple terminal tabs, a split Space, right dock in SSH-client mode at a non-default width,
   a zoomed panel, customized SFTP columns), quit cleanly, and copy `docks.json` to a fixture at
   `crates/state/src/fixtures/docks-0.5.2.json`. Record the file in the packet.
2. **P4:** add a focused test in `crates/state` that deserializes the fixture through
   `DockDocument`, asserts the OneTerm-owned fields survive, and asserts the flattened dock
   fields still parse as a `DockAreaState`.
3. **P4:** add a workspace-level test that loads the fixture into a `DockArea` via
   `DockArea::load`, dumps it again, and asserts the dump is structurally equal to the input for
   the parts OneTerm controls (dock placements, open flags, sizes, panel names, active indices).
4. **P6, manual:** launch the migrated build against a copy of a real user profile and confirm
   the layout appears as it did before — same dock width, same tab order, same active tab, same
   zoom, same SFTP columns.

### Fallback if the round-trip fails

Two options, in order of preference:

1. **Adapt in `dock_persistence`** — if the drift is narrow and mechanical (a renamed field, a
   moved flag), migrate the JSON in `migrate_json_value` and bump `CURRENT_SCHEMA_VERSION` to 2.
   `quarantine_file` already exists for unparseable input, so a bad file is preserved rather than
   overwritten.
2. **Bump `MAIN_DOCK_VERSION` to 4** — the blunt instrument. `persistence.rs` then discards the
   saved state and the user gets the default layout on first launch after upgrade. This loses
   user data, so it requires an explicit decision record and a release note, not a quiet commit.

## Interfaces

```rust
// unchanged, asserted rather than modified
DockDocument { schema_version: u32, /* flatten */ dock_fields, zoomed_panel, sftp_table_state }
const CURRENT_SCHEMA_VERSION: u32 = 1;
pub const MAIN_DOCK_VERSION: usize = 3;

// gpui_base::dock, v0.6.0 — same serde shape as 0.5.2
DockAreaState { version: Option<usize>, center: PanelState,
                left_dock/right_dock/bottom_dock: Option<DockState> }
DockState::{panel, placement, size, open}   // now accessors, fields private
PanelState { panel_name: String, children: Vec<PanelState>, info: PanelInfo }
PanelInfo::{Stack{sizes,axis}, Tabs{active_index}, Panel(Value), Tiles{metas}}
```

`DockState`'s fields became private with `new()` + accessors. OneTerm does not construct
`DockState` directly today, so this is a read-only note — but any code that pattern-matches its
fields must switch to the accessors.

## Edge Cases and Failure Modes

- [ ] `SshClientPanel`'s `PanelInfo::panel(Null)` leaf loads differently in 0.6.0 (wrapped, or
      rejected as an invalid child), changing the right dock on first launch after upgrade.
- [ ] `zoomed_panel` restore fails silently because zoom is keyed differently (`PanelId` vs. the
      old "find the TabPanel whose active panel matches this name") — the file parses, the layout
      appears, but the panel is not zoomed.
- [ ] A panel name that is not registered becomes an upstream placeholder that *carries the
      original `PanelState` forward*, so a mis-registered panel does not corrupt the file but does
      render as a blank slot; make sure all five `register_panel` names still match.
- [ ] `sftp_table_state` and `schema_version` are stripped because a rewrite path writes
      `DockAreaState` directly rather than through `DockDocument`.
- [ ] Round-trip is asserted on a freshly generated file rather than a genuine 0.5.2 one, which
      would prove nothing about the upgrade path. The P0 capture must happen before P1.

## Verification

- [ ] `crates/state` focused test: 0.5.2 fixture → `DockDocument` → OneTerm fields intact.
- [ ] `crates/workspace` focused test: fixture → `DockArea::load` → `dump` → structurally equal.
- [ ] Manual: real profile copy launches with an unchanged-looking workspace, including zoom,
      dock width, tab order, and SFTP column widths.
- [ ] Negative case: a deliberately corrupted `docks.json` is still quarantined, not overwritten.
