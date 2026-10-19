# Work: Merge the settings pages the 0.7.3 kit no longer needs split

ID: US-0195
Intake: IN-0052
Created: 2026-10-19

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

- Change type: new capability | existing-contract change | bug | maintenance | acceptance rework
- Change type: UI reorganisation (owner request 2026-10-15)
- Risk lane: normal — Settings page composition only; no persisted data changes.
- Spec Intake, when required: IN-0052 (the 0.7.3 kit is what makes this safe)

## Outcome

Settings shows one **Key Bindings** page (six groups) and one **Terminal** page (Font, Cursor, Layout, Scroll, Bell, Mouse, Security, Logging), and every sidebar sub-item on both still lands on its group.

## Scope

- [x] In scope: `KEY_BINDING_PAGES`, `TERMINAL_PAGES`, their tests and comments, the Logging group's title, `docs/gui-layout.md` § Settings / Sidebar navigation.
- [x] Out of scope: Completion page, other pages, the settings themselves.

## Acceptance

- [x] Sidebar: one Key Bindings row with six sub-items; one Terminal row with eight sub-items; Completion unchanged.
- [x] `Reset All` on Key Bindings resets every binding; on Terminal, every terminal setting on the page.
- [x] Why it is safe now is recorded: gpui-kit 0.7.3 `setting/page.rs` scrolls a sub-item by index (`ListState::scroll_to`), not by measured height, so the US-0122 under-shoot is gone.
- [x] Full `scripts/ci-local.sh` passes.

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` § Settings and § Sidebar navigation — the "pages are short on purpose" rule this reverses.
- `reference/gpui-kit/crates/component/src/setting/page.rs:170-180`, `settings.rs:178-183` — the kit's index scroll and titled-only sub-items.

### Documentation Action

Choose one and explain it before implementation:

- Update required: `docs/gui-layout.md`.

Reason: its page-count rule rested on a kit limitation 0.7.3 removed.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

Add only relevant code, decisions, dependencies, and invariants not already clear from the owning docs.

## Plan

- [x] Merge the two tables; title the Logging group; update tests and docs; gate.

## Decisions

Link consequential decisions; do not repeat their rationale here.

## Verification Plan

`cargo test -p oneterm-settings-ui` (page tables), full gate.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `KEY_BINDING_PAGES` is one page of six groups; `TERMINAL_PAGES` is Terminal (eight groups) + Completion. The Logging group took a title and the old page description. The three-group cap test and the cross-page Reset All test went (one page cannot reach another); the partition and coverage tests stay.
- Walk, debug build, private config (cwd `target/walk-0195`), 1016x708 Settings window, 2026-10-19: sidebar shows one Key Bindings row with 6 sub-items and one Terminal row with 8; clicking Bell (mid-page) and Input Channel (below the fold) puts that group's heading at the top of the page; Logging (last group) scrolls the page to its end with the whole group visible. Screenshots `target/walk-0195/03..08-*.png` (not committed).
- `CARGO_BUILD_JOBS=6 bash scripts/ci-local.sh`: all checks passed, 141 suites / 5039 passed / 0 failed.
- Not walked: `Reset All` on the merged pages (the scope is unit-tested by `each_page_resets_exactly_the_actions_it_shows`).

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
