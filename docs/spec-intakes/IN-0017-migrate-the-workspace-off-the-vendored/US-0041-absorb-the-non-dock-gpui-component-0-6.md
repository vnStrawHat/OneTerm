# Work: Absorb the non-dock gpui-component 0.6.0 API drift

ID: US-0041
Intake: IN-0017
Created: 2026-09-07

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

- Change type: existing-contract change
- Risk lane: normal
- Spec Intake: IN-0017

## Outcome

Every v0.6.0 breaking change outside the dock that OneTerm touches is absorbed: private
`setting::RenderOptions` fields read through accessors, the crash-report multiline input moved to
`Textarea`, the assets crate renamed, settings-section scroll behavior preserved from OneTerm's
side now that the vendor patch is going away, and OneTerm's settings groups use the bordered
`Outline` variant demonstrated by the v0.6.0 Settings story without custom divider rows between
items. Editable fields expose reset metadata so the story's conditional `Reset All` control works,
and OneTerm-owned keyboard-focus targets expose accessibility roles.

## Scope

- [x] In scope: `crates/settings-ui/src/terminal/{font.rs, logging.rs}` (10 `RenderOptions` field
      reads); `crates/app/src/crash_report_dialog.rs` (`InputState::multi_line` → `TextareaState`);
      the settings-sidebar group-index fix and its test; applying the v0.6.0 Settings story's
      bordered `Outline` group variant in `crates/settings-ui/src/panel.rs`; removing the custom
      `SettingItem` separator rows from all settings groups; restoring `Reset All` through field
      defaults/reset handlers; assigning accessibility roles to OneTerm-owned focus targets and the
      host-owned dock renderer frames that GPUI Base focuses; confirming `oneterm_state::form_dialog`
      and `gpui_component::resizable` still compile untouched;
      and re-validating the built-in themes against the refreshed `.theme-schema.json`.
- [x] Out of scope: anything dock-related (US-0040); the assets-crate rename itself, which lands
      with the dependency swap in US-0039; adopting new v0.6.0 components.

## Acceptance

- [x] `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- [x] Settings group indices are kept aligned by placing untitled groups after titled groups; the
      About-page ordering is covered by a regression test.
- [x] Every settings group is rendered with `GroupBoxVariant::Outline`, matching the bordered group
      output configured by the pinned v0.6.0 Settings story; a focused regression pins the variant.
- [x] No settings group inserts custom separator `SettingItem` rows between its real items; groups
      rely on GPUI Kit's standard gap and `Outline` border.
- [x] A changed resettable field makes the active page's GPUI Kit `Reset All` control appear and the
      control restores the page's declared defaults. Focused coverage exercises the same
      `AnySettingField::{is_resettable, reset}` contract used by the page renderer.
- [x] The Key Bindings capture target has both an ID and `Role::TextInput`; every OneTerm-owned
      stateful Settings element found by source scan has a role. Docked panels expose a distinct
      dock-navigation focus proxy and forward it to their independently tracked content target, so
      an active panel and its `TabGroup` never claim the same accessibility focus in one frame.
- [ ] Clicking every sidebar group in a live Settings window, including About/Links, was not manually
      exercised.
- [x] Clicking any settings-sidebar group scrolls to that group at the model/order boundary, including on the About page where
      an untitled group precedes the titled `"Links"` group. Covered by a test, not only by hand.
- [x] The crash-report dialog uses `TextareaState`/`Textarea`; compilation and tests cover the
      multiline value path. Manual scrolling/copy interaction was not run.
- [x] All `FormDialog` callers compile and their existing tests pass; manual opening/submission of
      every dialog was not run.
- [x] Built-in theme parsing/schema regressions pass; manual light/dark rendering was not run.
- [x] Custom `AppIcon` SVG resolution compiles and the asset-source ordering is unchanged.
- [x] The "does not apply" list is confirmed by clean all-target compilation: no
      `Divider`, `Table`→`DataTable`, `is_eof`, `row_selector`, `History`, webview or chart
      breakage appears.

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` — settings window structure and the settings-page composition contract.
- `docs/crash-reporting.md` — the crash-report dialog's behavior, including what the user can read
  and copy.
- `docs/sftp-browser-design.md` — the `DataTable` delegate contract; relevant for confirming the
  table renames do not apply.
- `docs/agents/code-style.md` — conventions for the mechanical rewrites.
- `AGENTS.md` §3.4 — theme and icon registration, which depends on the theme schema and the
  asset-source merge order.

### Documentation Action

Update required: `docs/gui-layout.md` — add the missing settings-window contract, including the
OneTerm-side scroll fix, the untitled-group ordering constraint, the v0.6.0 story's bordered
`Outline` group variant, and the absence of custom dividers between items.

No contract change: `docs/crash-reporting.md` — the dialog's user-visible contract is unchanged;
only the widget type behind it changes. Recorded rather than edited.

### Reconciliation

Confirmed: `docs/gui-layout.md` states the bordered `Outline` variant, standard item gap without
custom divider rows, and the untitled-group ordering constraint; no reviewed current-state doc
attributes the behavior to the retired vendor patch.

## Context

- `RenderOptions` fields became private with builders/accessors so upstream can add options
  compatibly: `page_ix()`, `group_ix()`, `item_ix()`, `size()`, `layout()`, `is_disabled()`.
- The settings scroll situation is the only non-mechanical item. Vendor patch 0003 had two hunks:
  the `ListState::measure_all()` hunk is superseded upstream by `deferred_scroll_group_ix`, but the
  `settings.rs` hunk is **not** — v0.6.0 still does `.filter(|g| g.title.is_some()).enumerate()`,
  so sidebar `group_ix` counts only titled groups while the scroll target indexes all groups.
  OneTerm reaches this: `crates/settings-ui/src/about.rs` has an untitled group at index 0 followed
  by titled `"Links"` at index 1.
- Preferred fix order: (1) title every group, (2) order untitled groups last on every page, (3)
  send the one-liner upstream and carry (2) until it lands. Whichever is chosen gets a test so the
  behavior is pinned on OneTerm's side.
- The v0.6.0 `SettingsStory` initializes `GroupBoxVariant::Outline` and forwards it with
  `Settings::with_group_variant`; `Settings` itself defaults to `Normal`, which has no border or
  padding. OneTerm must opt into `Outline` at its `SettingsPanel` composition seam.
- GPUI Kit already spaces `SettingGroup` children with its standard group gap. OneTerm's custom
  separator rows are not part of the v0.6.0 story output and must not be inserted between items.
- `SettingPage::resettable(true)` only permits reset UI; it does not make fields resettable. The
  v0.6.0 page renders `Reset All` only while a child field's `default_value` or `on_reset` reports a
  dirty value. OneTerm fields therefore need reset metadata sourced from their owning model defaults.
- Every focused GPUI element needs both an ID and a role. The Key Bindings capture target is a keyboard
  text-capture surface and uses `Role::TextInput`; panel/container focus targets use `Role::Pane`.
  GPUI Base attaches focus to the frames returned by the host's `DockAreaRenderer`,
  `TabGroupRenderer`, and `TilesRenderer`, so OneTerm assigns roles at those renderer seams rather
  than patching the published dependency. A docked panel must not return its content handle directly:
  `TabGroup` tracks the returned handle on its own frame, so sharing it with the panel content makes
  two accessibility nodes call `set_focus` in one frame. Each dock panel therefore returns a distinct
  dock proxy and forwards proxy focus to its independently tracked content handle after that frame.
- `InputState` is single-line only in 0.6.0; `Textarea` does not carry input adornments
  (`prefix`, `suffix`, mask toggle, clear button) — those compose around the control.
- Detail design: `low-level-design/04-component-api-drift.md`.

## Plan

- [x] Use `RenderOptions` accessors in the two settings files.
- [x] Migrate the crash-report dialog to `TextareaState` / `Textarea`; preserve sizing/value
      behavior that `multi_line` implied.
- [x] Put untitled groups after titled groups and add the About-page ordering regression.
- [x] Configure `SettingsPanel` with `GroupBoxVariant::Outline` and add a focused regression.
- [x] Remove the custom separator module, helper calls, and Key Bindings separator insertion.
- [x] Add default/reset metadata to editable settings fields and regression coverage for dirty/reset
      behavior.
- [x] Give the Key Bindings capture target `Role::TextInput`, expose Settings/panel roots as panes,
      and assign pane roles to the host-owned dock, tab-group, and tiles renderer frames.
- [x] Confirm `form_dialog`, `resizable`, `Root`, `TitleBar`, `WindowExt`, `GlobalState`, theme and
      icon surfaces compile untouched; record anything that did not.
- [x] Re-validate the built-in themes against the refreshed schema through the workspace gate.

## Decisions

- `docs/decisions/DEC-0006-depend-on-published-gpui-component-0-6.md`
- A decision record is needed only if the fix chosen for the settings groups changes visible page
  structure (option 1 adds a heading to the About identity block).

## Verification Plan

Unit: settings-sidebar navigation, group presentation, reset metadata, and capture accessibility
regressions; existing `settings-ui` and `settings` tests.
Integration: `cargo test --workspace`.
E2E (manual): open the settings window, change one field on each page, verify `Reset All` appears and
restores defaults, enter Key Bindings capture mode and confirm no missing-node a11y log, confirm the
v0.6.0 outline and no item dividers, then click every sidebar group including About and Key Bindings; open the crash-report dialog from the hidden ten-click About trigger; open each of the
five form dialogs; switch through every built-in theme in both modes.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

The acceptance rework traced the missing borders to the GPUI Kit defaults: `Settings::new` defaults
to `GroupBoxVariant::Normal`, while the pinned v0.6.0 `SettingsStory` initializes and forwards
`GroupBoxVariant::Outline`. `SettingsPanel` now forwards the same `Outline` variant, and the focused
regression pins that composition choice. The follow-up removed OneTerm's custom separator module,
all 12 `items_with_separators` call sites, and the independent Key Bindings separator insertion.
Settings groups now contain only their real items and rely on GPUI Kit's standard gap inside the
outline. The existing identity-group ordering remains unchanged, so sidebar and scroll indices still
agree without adding a heading.

The latest acceptance rework traced the missing `Reset All` action to GPUI Kit's conditional
contract: `SettingPage::resettable(true)` only permits the action, while the page renders it only when
an item reports dirty state through `default_value` or `on_reset`. All editable OneTerm fields now
supply built-in defaults from their owning models (with custom handlers for font family, line height,
log directory, key bindings, and skipped-update state). The focused regression exercises the same
`AnySettingField::{is_resettable, reset}` path used by the page and proves a changed field becomes
dirty, resets to its default, and becomes clean again.

The first accessibility pass was too narrow: it covered the Key Bindings capture `div`
(`Role::TextInput`) and repository links (`Role::Link`) but only scanned `settings-ui`. Runtime Windows
UAT still produced a missing-node warning. The follow-up audited every production `.track_focus(...)`
in the workspace and the published GPUI Base render path. OneTerm now renders the Settings root and
all content focus targets as named `Role::Pane` nodes. GPUI Base also attaches focus to the exact frames
returned by `DockAreaRenderer`, `TabGroupRenderer`, and `TilesRenderer`; `OneTermDockSkin` assigns pane
roles to those host-owned frames without modifying the published dependency.

That role-only follow-up exposed a second bug under active Windows accessibility: `TabGroup::render`
tracks the active panel's `Focusable` handle on its role-bearing frame, while each panel root tracked the
same handle. Both nodes therefore called GPUI's a11y `set_focus` in one frame and debug builds panicked.
Terminal, Session, SFTP, SSH Client, and Agent panels now return a separate dock-navigation proxy from
`Focusable`; an `on_focus` listener forwards that proxy to the independently tracked content target in
the following frame. The SSH composite does not track its proxy on another outer node. Terminal-focused
regressions prove proxy-to-content forwarding still preserves action dispatch and terminal close behavior.

Post-fix focused tests passed 299 tests across the five changed crates, including the proxy action-routing
regressions; focused all-target clippy passed. The Harness verifier reran 24 `settings-ui` tests and the
serial `scripts/ci-local.sh --full` gate successfully. The existing yanked-dependency warnings for
`chacha20 0.10.1` and `der 0.8.0` remain warnings accepted by that gate.

A Windows `fast-dev` runtime smoke crossed real platform accessibility activation and continued for more
than 30 seconds through startup and additional terminal creation without `set_focus called more than once`,
missing-node warnings, or a process panic. Manual Settings visual comparison and clicking every sidebar
entry remain E2E gaps; the platform panic path itself is now reproduced and verified.

## Handoff

Depends on US-0039 and US-0040 (the workspace must compile to see these errors in isolation).
