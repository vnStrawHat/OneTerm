# Work: The list-style override reaches the tokens the kit paints selection from

ID: BUG-0088
Intake: IN-0042
Created: 2026-10-22

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug (found by the advisor review of `IN-0055` rework 7; latent since `US-0111`'s override, not caused by the rail)
- Risk lane: normal (presentation only; no persistence, auth or external effect)
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md` (`US-0111` records the override as the contract: selected list and table items look like hover, no border)

## Outcome

`apply_list_style_override` (`crates/theme/src/theme.rs`) makes the kit paint a selected list or table item the way OneTerm intends: as hover, with no selection overlay and no selection border. Today it only changes the theme's *colours*; GPUI Kit 0.7 paints the data table's selected row and cell from `Theme::tokens.table_active` and a list's from `tokens.list_active`, which it syncs from the colours only inside `Theme::update`. After every theme application the tokens therefore kept the kit's own derivation (`clamp_alpha(.., 0.2)`), and the SFTP tables (`row_selectable(true)`, `crates/sftp-ui/src/panel.rs:152`, `local_pane.rs:500`) drew the kit's 0.2-alpha overlay on top of their own `render_tr` highlight, which `table_delegate.rs:339-341` believes is disabled.

## Scope

- [x] In scope: the override's writes go through `Theme::update` (so the kit's reconcile copies them to `tokens`), with `record_selection_accent` still run first so `selection_accent` keeps the theme's own `table.active.border`; a check of every reader of the overridden fields.
- [x] Out of scope: the `warning` legibility correction (`apply_warning_legibility`) writes `Theme::warning` the same way; the kit's `Warning` button variant reads `tokens.warning`, but OneTerm draws no `Warning` button, so nothing on screen depends on it today; it is untouched and not part of this bug (pinned as current behaviour in `the_override_inside_theme_update_keeps_the_other_overrides`). Any change to what SFTP rows or the session tree look like beyond removing the overlay.

## Acceptance

- [x] After `apply_config` and the override, `tokens.table_active` has zero alpha and `tokens.list_active` equals `list_hover` (the advisor's pin `the_list_style_override_reaches_the_tokens_the_kit_reads`, which failed before the fix with `tokens.table_active.color.a` = 0.2).
- [x] `the_selection_accent_survives_the_list_style_override` and `list_active_is_list_hover_after_every_app_theme_path` still pass (the accent is recorded before the blank; `list_active` is `list_hover` after every theme path).
- [x] Nothing else relied on the overlay: the only kit readers of `tokens.table_active` are the data table's selected row and cell (`table/state.rs`), used by the two SFTP tables, whose `render_tr` draws the selected row in `table_hover` itself; `tokens.list_active` has no kit reader outside the theme, and the session tree reads `list_hover` and the colour `list_active`.
- [ ] On screen, by `ui-verifier`: a selected SFTP row (remote and local pane) is `table_hover` only, with no extra tint, in a dark and a light theme.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0042-ux-polish-round-1/US-0111-secondary-text-contrast-floor.md` and its verify evidence — record the override as the reason `list.active.background` and `table.active.background` are never painted; that statement is what this fix makes true for the tokens as well.
- `docs/gui-layout.md` § Text contrast floor and hierarchy — no mention of the token sync.

### Documentation Action

- No contract change: the contract was already "selected looks like hover, no border"; the code did not meet it for the kit's tokens. The doc comment on `apply_list_style_override` says why the write goes through `Theme::update`.

Reason: behaviour the docs already promise.

### Reconciliation

Docs unchanged apart from this packet and the `IN-0042` list entry.

## Context

- `Theme::update` (kit `theme/mod.rs`) runs the edit, then `tokens.reconcile(colors, colors_before, tokens_before)`, then refreshes windows. `Theme::global_mut` is the raw write path the kit documents as leaving `tokens` alone.
- Every theme path the app has (`init`, `SwitchTheme`, `SwitchThemeMode`, the Appearance page) calls the override right after the change, so the fix applies to all of them.

## Plan

- [x] Run the override's four writes inside `Theme::update`, `record_selection_accent` first.
- [x] Pin: the advisor's token test, now passing.

## Decisions

None.

## Verification Plan

- `cargo test -p oneterm-theme --lib`; `cargo clippy -p oneterm-theme --all-targets -- -D warnings`.
- The coordinator runs the full gate after merge; the SFTP walk is for `ui-verifier`.

## Evidence and Gaps

- `cargo test -p oneterm-theme --lib`: 9 passed, including the pin that failed on the advisor's head.
- Gap: the on-screen SFTP check above.

## Handoff

Implemented; `ui-verifier` for the SFTP selected row.
