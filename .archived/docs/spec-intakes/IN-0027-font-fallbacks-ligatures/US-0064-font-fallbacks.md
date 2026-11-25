# Work: Fallback font list for the terminal

ID: US-0064
Intake: IN-0027
Created: 2026-09-11

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

- Change type: new capability
- Risk lane: normal
- Spec Intake, when required: IN-0027

## Outcome

Glyphs the terminal font lacks are drawn from a configurable, ordered list of fallback
families (`terminal.json` `font.fallbacks`, Settings UI "Fallback Fonts"), defaulting to the
Nerd Font symbol fonts, so prompt icons render when such a font is installed.

## Scope

- [x] In scope: `FontConfig.fallbacks`; `TerminalSettings.font_fallbacks` + mutator +
  persistence round-trip; `terminal_font()` sets `Font::fallbacks`; `CachedFont` and
  `RenderInputs::style_key` cover the list; Settings UI input; docs.
- [x] Out of scope: bundling a Nerd Font; per-font-family fallback presets; Linux fallback
  support (GPUI's Linux text system ignores `Font::fallbacks`).

## Acceptance

- [x] `terminal.json` without `fallbacks` loads with the default list; a saved config round-trips
  the list (`persist_tests`: `assert_settings_eq` + `non_default_settings`).
- [x] `terminal_font()` returns `fallbacks == None` for an empty list and the configured
  families otherwise (`font_carries_the_configured_fallbacks`).
- [x] Changing the list rebuilds the font (`cached_font_matches_only_its_own_inputs`) and the
  plan-cache `StyleKey` hashes the list (by construction in `RenderInputs::style_key`; the
  existing `font_change_clears_the_shaped_run_cache` covers the glyph cache).
- [x] GUI (Windows): no Nerd Font is installed on the test machine, so the walk used
  `Segoe Fluent Icons` (same private-use range): with it listed the icons render, with the
  list empty the same cells are tofu (`evidence/US-0064-fallback-segoe-fluent-icons.png`,
  `evidence/US-0064-US-0065-fallbacks-empty-ligatures-off.png`, `evidence/gui-walk.md`).
- [x] `pwsh scripts/ci-local.ps1` green (2026-09-11).

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0027-font-fallbacks-ligatures/high-level-design.md` — plumbing and
  cache keys.
- `docs/spec-intakes/IN-0018-terminal-view-rewrite/high-level-design.md` § Frame Pipeline —
  font resolution and `RenderInputs` change detection; no change for this packet.
- `docs/agents/persistence.md` — `terminal.json` is owned by `oneterm-settings`, schema lives
  in `FontConfig`; no change.
- `README.md` § Features — feature bullet list.
- `docs/agents/structure.md` — `terminal_config/` comment lists the groups; no change.

### Documentation Action

Update required: `README.md` (feature bullet).

Reason: the config schema is documented by the `FontConfig` doc comments; the README lists
user-visible terminal features.

### Reconciliation

Changed: `README.md` § Terminal emulator (fallbacks bullet). `FontConfig` doc comments
describe the new key. No other reviewed doc enumerates font settings.

## Context

- `gpui-pre-windows` `direct_write.rs::generate_font_fallbacks` skips families that are not
  installed and always appends the system fallback, so a default list is safe on a machine
  without Nerd Fonts.
- `RenderState::ensure_fonts` compares the whole `Font` value, so the `GlyphCache` already
  clears when fallbacks change; the plan cache is keyed by `StyleKey`, which must gain the
  list hash.

## Plan

- [x] Settings: config field, `TerminalSettings` field, apply/persist, test (no mutator: the
  UI writes fields through `set()` like the other font fields).
- [x] View: `terminal_font`, `CachedFont`, `StyleKey`, tests.
- [x] Settings UI: "Fallback Fonts" input (comma-separated).
- [x] README, GUI evidence.
- [x] CI.

## Decisions

None.

## Verification Plan

- `cargo test -p oneterm-settings` — round-trip.
- `cargo test -p oneterm-terminal-view` — font build, style key.
- `pwsh scripts/ci-local.ps1`.
- GUI: fast-dev build, local shell with a starship prompt, screenshot.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `cargo test -p oneterm-settings -p oneterm-terminal-view -p oneterm-settings-ui`:
  45 / 26 / 284 passed (2026-09-11).
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- GUI: `evidence/gui-walk.md` with crops.
- Gaps: no real Nerd Font or starship prompt on the test machine (mechanism proven with a
  system private-use font); the Settings UI input was not driven live (restart-based check);
  Linux ignores `Font::fallbacks` (GPUI upstream TODO), macOS not checked.

## Handoff

None.
