# Work: Remove multi-tab terminal content gap

ID: BUG-0008
Intake: IN-0007
Created: 2026-08-12

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [ ] Implemented
- [x] Changed
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug
- Risk lane: tiny
- Spec Intake: `IN-0007`

## Outcome

Terminal content begins directly below the tab bar with either one or multiple terminal tabs; the multi-tab-only empty band shown in the supplied screenshot is removed.

## Scope

- [x] In scope: `TerminalPanel` inner-padding contract and focused regression proof.
- [x] Out of scope: tab height/title styling, other panel types, vendor-wide TabPanel behavior, terminal Space borders, or layout persistence.

## Acceptance

- [x] `TerminalPanel` opts out of gpui-component TabPanel inner padding.
- [x] The one-tab terminal layout remains unchanged.
- [x] Multi-tab terminal content no longer receives the conditional `pt_2` top inset.
- [x] Existing terminal title and terminal-view tests pass.
- [x] Mandatory fmt, clippy, and workspace build gates pass.

## Documentation

### Owning Docs Reviewed

- `docs/terminal-split/06-integration.md` — TerminalPanel remains inside the existing TabPanel.
- `docs/agents/ui-fork-maintenance.md` — avoid unnecessary vendor changes.
- `vendor/gpui-component/src/dock/panel.rs` — `Panel::inner_padding` is the intended opt-out seam.
- `vendor/gpui-component/src/dock/tab_panel.rs` — applies `pt_2` only for multiple tabs when `inner_padding` is true.
- `Screenshot 2026-08-12 140419.png` — visual bug evidence.

### Documentation Action

No product-contract change. The accepted terminal layout is already full bleed; this packet and intake retain the root cause and verification. No owning product design needs a new behavior clause for removing an unintended dependency default.

Reason: this restores consistent existing layout rather than introducing a new terminal behavior.

### Reconciliation

Confirmed: no owning product-contract update is needed. The change restores the existing full-bleed terminal layout through the dependency's intended panel-specific opt-out.

## Context

`Panel::inner_padding` defaults to true. `TabPanel::render_active_panel` applies `pt_2()` only when `panels.len() > 1 && active_panel.inner_padding(cx)`, exactly matching the reported one-tab/two-tab difference. A TerminalPanel-specific override avoids expanding the project's vendor patch surface.

## Plan

- [x] Override `Panel::inner_padding` for `TerminalPanel` to return false.
- [x] Add a focused test for the contract.
- [x] Run focused and mandatory quality gates, then record the manual visual gap if the GUI is unavailable.

## Decisions

No new decision record is needed; the change uses an existing dependency extension point and creates no inherited architecture choice.

## Verification Plan

- Focused: unit test the TerminalPanel `Panel::inner_padding` result.
- Regression: `cargo test -p oneterm-terminal-view`.
- Platform: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo build --workspace`.
- Manual/E2E: visually compare one terminal tab and two terminal tabs if GUI execution is available.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Focused `terminal_panel_disables_multi_tab_inner_padding`: 1 passed.
- `cargo test -p oneterm-terminal-view`: 102 passed.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed with no issues.
- `cargo build --workspace`: passed.
- `git diff --check`: passed.
- Manual GUI comparison was not run in this non-interactive session. The exact conditional responsible for the screenshot gap is disabled by the tested panel contract.

## Handoff

Implementation and automated verification are complete. Recommended follow-up: reopen the supplied two-tab layout and visually confirm the terminal begins directly under the tab bar.
