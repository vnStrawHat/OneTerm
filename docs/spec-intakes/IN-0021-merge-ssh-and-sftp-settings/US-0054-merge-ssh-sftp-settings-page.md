# Work: Merge the SFTP settings into the SSH page

ID: US-0054
Intake: IN-0021
Created: 2026-09-09

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change (UI composition only)
- Risk lane: normal
- Spec Intake: IN-0021

## Outcome

The Settings window has one "SSH" page with the groups `Connection`, `SFTP Editor` and
`SFTP Edit Limit` in that order, and no separate "SFTP" page. Every setting keeps its label,
value range, default and persistence path.

## Scope

- [x] In scope: `crates/settings-ui/src/ssh.rs`, `crates/settings-ui/src/sftp.rs` (deleted),
      `crates/settings-ui/src/panel.rs`, `crates/settings-ui/src/lib.rs`, module docs.
- [x] Out of scope: `crates/settings`, `terminal.json` schema, the SFTP browser itself, and
      the setting labels/ranges.

## Acceptance

- [x] `ssh::page` yields the three groups in the order Connection, SFTP Editor, SFTP Edit Limit.
- [x] No "SFTP" page is registered in `SettingsPanel::pages` (the module no longer exists).
- [x] The page keeps `IconName::Network` and `resettable(true)`, so "Reset All" covers all
      three groups (each field still declares `default_value`).
- [x] The persisted `terminal.json` `ssh` / `sftp` groups are unchanged (no schema edit).
- [x] `cargo test -p oneterm-settings-ui`, `cargo test --workspace`, `cargo fmt --all`,
      `cargo clippy --workspace --all-targets -- -D warnings`, `python scripts/check-english.py`,
      `python scripts/check-doc-paths.py` pass.

## Documentation

### Owning Docs Reviewed

- `docs/agents/persistence.md` — `terminal.json` is owned by `crates/settings`; this change
  touches no schema, so no update is required.
- `docs/gui-layout.md` § "Settings window" — describes the widget/variant and the `Reset All`
  contract, not the page list; still accurate.
- `docs/README.md` — index of current designs; adds the IN-0021 row only if the index lists
  intakes (it lists design docs by area, so no row is required).
- `docs/architecture.md`, `docs/agents/structure.md` — describe `oneterm-settings-ui` as "the
  General Settings window"; no page list, unchanged.
- `docs/sftp-browser-design.md` § 4.14 — points readers at the "SFTP" settings page for the
  editor config; must now point at the SSH page.
- `docs/spec-intakes/IN-0011-.../US-0021-*.md` — historical record of the original SFTP page;
  left as accepted history.

### Documentation Action

Update required: `docs/sftp-browser-design.md` § 4.14 (the pointer to the settings page).
No contract change elsewhere: the persisted schema, the crate boundaries and the Settings
widget contract are all unchanged.

Reason: only the page composition inside `crates/settings-ui` moves.

### Reconciliation

Confirmed after implementation. Docs changed: `docs/sftp-browser-design.md` (§ 4.14 pointer), this intake and packet.
`docs/agents/persistence.md` unchanged — verified no schema change.

## Context

`SettingPage` / `SettingGroup` keep `title` and `groups` private in gpui-component 0.6, so the
page order is asserted through the crate's own `groups(cx)` builder, which pairs each group
with the title `page` applies.

## Plan

- [x] Move the two SFTP group builders into `ssh.rs`, rename their titles.
- [x] Delete `sftp.rs`, its `mod` and its `pages()` entry.
- [x] Add the group-order test; refresh the module docs in `lib.rs` / `panel.rs`.
- [x] Update `docs/sftp-browser-design.md` § 4.14.

## Decisions

None — the merge introduces no rule future work must inherit.

## Verification Plan

- `cargo test -p oneterm-settings-ui` — group order + existing panel tests.
- `cargo test --workspace` — regression.
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`.
- `python scripts/check-english.py`, `python scripts/check-doc-paths.py`.
- Visual: run `target/fast-dev/oneterm.exe`, open Settings, capture the SSH page to
  `evidence/US-0054-ssh-page.png`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Commands (2026-09-09):

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — `Finished dev profile`, no warnings.
- `cargo test -p oneterm-settings-ui` — 25 passed, including the new
  `ssh::tests::ssh_page_lists_connection_then_the_two_sftp_groups`.
- `cargo test --workspace` — 1055 passed, 5 ignored (44 suites).
- `python scripts/check-english.py` — passed for 538 files.
- `python scripts/check-doc-paths.py` — passed for 146 paths in 10 documents.

Visual (Windows, workstation locked, so the app was driven with PostMessage mouse events and
captured with `PrintWindow(h, hdc, 2)`):

- `evidence/US-0054-ssh-page.png` — Settings window on the SSH page: sidebar entries
  General / Key Bindings / Terminal / SSH / Appearance / About (no SFTP page), the SSH page
  expanded into Connection, SFTP Editor, SFTP Edit Limit.
- `evidence/US-0054-ssh-page-edit-limit.png` — scrolled to the "SFTP Edit Limit" group
  ("Max Edit File Size (MB)" = 1), with the Custom Program / Custom Arguments rows disabled in
  OS-default mode.

Gaps:

- The app was launched from `target/debug/oneterm.exe`; `--profile fast-dev` could not relink
  because another session was already running `cargo run --profile fast-dev` and held
  `target/fast-dev/oneterm.exe` open. The debug binary is the same code.
- `pwsh scripts/ci-local.ps1` not run (excluded by the task); no commit made.

## Handoff

None.

## Acceptance Rework (2026-09-09): input alignment

Reported on the built work: in Settings > Terminal > Logging the "File Name Format" text input
sat further right than "Log Folder" and "Content Format" and overflowed the card; the
"Custom Program" and "Custom Arguments" inputs of Settings > SSH > SFTP Editor did the same.

### Cause

The horizontal `SettingItem` row in `gpui-component-0.6.0`
(`src/setting/item.rs`, `render_item`) is an `h_flex().justify_between().gap_3()` with two
children: a label column styled `flex_1().max_w_3_5()` and an unstyled `div().id("field")`
that wraps the rendered field. `StringField` gives the input a fixed `w_64()` (256 px).

At the Settings window's default size the row is 634 px wide, so the label column's
`max_w_3_5` cap is 0.6 x 634 = 380 px. A description whose unwrapped text is wider than that
cap pins the label column at 380 px instead of letting it grow to the 366 px that leaves room
for the field. The row then needs 380 + 12 (gap) + 256 = 648 px in a 634 px row, and the
14 px shortfall pushes the input right, past the card padding. Rows whose description fits
under the cap are unaffected, and rows with a narrow control (switch, dropdown, number field)
are unaffected too, because `justify_between` still has free space to right-align them.

An earlier attempt added `flex_shrink_0()` to every `SettingField::input(..)`. It had no
effect and was reverted: the input is not the flex item of the row, the `div().id("field")`
wrapper is, and that wrapper's automatic minimum size is already the input's fixed 256 px, so
it never shrank in the first place. The box was overflowing, not being squashed.

The label column cannot be restyled from our side: `SettingItem::description` takes
`impl Into<Text>` (a plain string or a `TextView`, not an element), and neither `SettingGroup`
nor `SettingsPage` exposes a label-width option.

### Fix

Bring the three over-long descriptions under the 60 % label cap, keeping the 256 px input
width that every other row uses:

- `crates/settings-ui/src/terminal/logging.rs` — "File Name Format":
  "Fixed for this release. %n = process or SSH endpoint."
- `crates/settings-ui/src/ssh.rs` — "Custom Program":
  "Executable (e.g. code, notepad). Custom mode only."
- `crates/settings-ui/src/ssh.rs` — "Custom Arguments":
  "Space-separated, before the file path. Custom mode only."

Known ceiling: the cap scales with the row, so a description longer than roughly 60 characters,
or a Settings window narrower than about 900 px, reproduces the overflow on any 256 px input
row. That is upstream row-layout behaviour shared by the whole settings UI; escaping it would
mean re-rendering these rows with `SettingItem::render`, which loses the built-in search
matching and reset handling.

### Verification

- `cargo fmt --all -- --check` — clean (exit 0).
- `cargo clippy --workspace --all-targets -- -D warnings` — `Finished dev profile`, no warnings.
- `cargo test -p oneterm-settings-ui` — `test result: ok. 25 passed; 0 failed; 0 ignored`.

Visual (Windows, `target/fast-dev/oneterm.exe`, driven with PostMessage mouse events and
captured with `PrintWindow(h, hdc, 2)`; edges measured from the PNGs):

- `evidence/US-0054-rework-logging.png` — Terminal > Logging. Log Folder, File Name Format and
  Content Format inputs all span x = 669..924 (256 px); the "Existing File" dropdown and both
  switches end at x = 924. Nothing overflows the card.
- `evidence/US-0054-rework-sftp-editor.png` — SSH > SFTP Editor (Editor left at "OS default
  application", so the two inputs are shown disabled; the row geometry is identical in Custom
  mode). Custom Program and Custom Arguments inputs both span x = 669..924 (256 px); the Editor
  dropdown and the "Max Edit File Size (MB)" number field end at x = 924.

Before the fix the same measurement put "File Name Format" at x = 683..938 against x = 669..924
for its neighbours.

Additionally the default Settings window width is 1000 px (was 950): at that width a 60 %
label column plus a 256 px input always fits a setting row, so the ceiling above only applies
when the user shrinks the window.

