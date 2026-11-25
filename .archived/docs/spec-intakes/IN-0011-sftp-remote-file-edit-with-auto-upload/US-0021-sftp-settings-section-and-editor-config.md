# Work: SFTP settings section and editor config

ID: US-0021
Intake: IN-0011
Created: 2026-08-19

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: high-risk
- Spec Intake, when required: IN-0011
- Ordering: first packet; blocks US-0022 and US-0023. No blockers.

## Outcome

`terminal.json` has a new `sftp` config group holding an editor configuration
(`EditorMode` = OS default | custom, plus `program` and `args`) and a
configurable maximum edit file size (`edit_max_file_size`, default 1 MiB, `0` =
no limit), defaulting to OS default. A new "SFTP" page in the Settings window
reads and writes these, and the values round-trip through save/load. Old config
files without the group load cleanly with the defaults.

## Scope

- [ ] In scope: `SftpConfig` / `EditorConfig` / `EditorMode` schema in
  `crates/settings/src/terminal_config/sftp.rs` (including `edit_max_file_size`);
  wiring into `TerminalConfig` + `lib.rs` re-exports; a new
  `crates/settings-ui/src/sftp.rs` page (Editor group + Edit-size field)
  registered in `SettingsPanel::pages`.
- [ ] Out of scope: the launcher, the edit workflow, the watcher, uploads
  (US-0022 / US-0023). No consumer reads the config yet.

## Acceptance

- [ ] A fresh `terminal.json` (or one missing the `sftp` block) loads with
  `SftpConfig::default()` (mode = OS default, `edit_max_file_size` = 1 MiB).
- [ ] Setting mode = Custom with a program + args and a non-default edit size,
  saving, and reloading yields an identical `SftpConfig` (idempotent serialize).
- [ ] The Settings window shows an "SFTP" page; toggling mode enables/disables
  the custom program/args fields; the edit-size field (in MB, `0` = no limit)
  and mode edits persist across reopening the window.
- [ ] `cargo test -p oneterm-settings` passes; `cargo clippy` clean.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0011-.../low-level-design/editor-config.md` — schema,
  defaults, wiring, and page layout owned by this packet.
- `crates/settings/src/terminal_config/completion.rs` / `logging.rs` — the
  `#[serde(default)]` group pattern to mirror.
- `docs/PROJECT.md` — persistence list (records the new group).

### Documentation Action

- Update required: add the `sftp` group to the `terminal.json` schema mention in
  `docs/PROJECT.md` persistence list once implemented. The LLD already describes
  the intended shape.

Reason: this introduces a new persisted config surface, which PROJECT.md tracks.

### Reconciliation

Before completion, confirm `docs/PROJECT.md` lists the `sftp` group and the LLD
matches the shipped struct field names.

## Context

Follow the per-group pattern in `crates/settings/src/terminal_config/`. Settings
pages are built in `crates/settings-ui/src/*.rs` and registered in
`panel.rs::pages`. Reads use getter closures; writes persist through the same
`terminal.json` path the Terminal page uses.

## Plan

- [ ] Add `sftp.rs` with `SftpConfig` / `EditorConfig` / `EditorMode`.
- [ ] Add `pub sftp: SftpConfig` to `TerminalConfig`; re-export the new types.
- [ ] Add `crates/settings-ui/src/sftp.rs` page + register in `pages()`.
- [ ] Add config round-trip unit tests.

## Decisions

DEC-0004 fixes the `open` crate, `notify` watcher, and `crates/core` launcher
home inherited by US-0022/US-0023. This packet owns no additional decision; the
default edit-size limit (1 MB) is fixed by IN-0011 review.

## Verification Plan

Unit: config default/round-trip/idempotent-serialize tests. Manual: the SFTP
settings page behaves as specified. Command: `cargo test -p oneterm-settings`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Implemented 2026-08-19.

- Added `crates/settings/src/terminal_config/sftp.rs` (`SftpConfig`,
  `EditorConfig`, `EditorMode`, `DEFAULT_EDIT_MAX_FILE_SIZE`) with 4 unit tests
  (default, missing-block, partial-block, explicit round-trip).
- Wired `sftp` into `TerminalConfig`, re-exported from `terminal_config/mod.rs`
  and `lib.rs`; mirrored into `TerminalSettings` (`apply.rs` load, `persist.rs`
  save, `settings.rs` field).
- Added `crates/settings-ui/src/sftp.rs` page (Editor group: mode dropdown +
  program/args inputs disabled outside Custom mode; Edit group: max size MB
  number field, 0 = no limit); registered in `panel.rs::pages` and observed
  `TerminalSettings` so Custom-mode enable/disable updates live.
- Docs: `docs/agents/persistence.md` terminal.json owner note now lists the
  `sftp` group.

Commands:

- `cargo test -p oneterm-settings` → 41 passed.
- `cargo build -p oneterm-settings-ui` → ok.
- `cargo fmt` + `cargo clippy -p oneterm-settings -p oneterm-settings-ui
  --all-targets -- -D warnings` → no issues.

Gaps: manual settings-window interaction (toggle mode enables/disables custom
fields; persistence across reopen) not run in this environment — UI is verified
by compile + the live-observe wiring; needs a manual Windows pass.

## Handoff

US-0022 can now read `TerminalSettings::global(cx).read(cx).sftp` for the editor
choice and `edit_max_file_size`.
