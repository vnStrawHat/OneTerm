# Work: Duplicate sessions into empty Spaces

ID: US-0011
Intake: IN-0006
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

- Change type: new capability
- Risk lane: normal
- Spec Intake: `IN-0006`

## Outcome

A user can choose where a terminal session is duplicated: the existing new-tab destination, any currently empty Space in the same tab, or a newly created split to the right or below. Every empty Space displays the same derived `Space #N` label used by the destination submenu, while the implementation targets stable Space identities and preserves existing local/SSH duplication and fresh-authentication behavior.

## Scope

- [x] In scope: dynamic Duplicate Session submenu; ordered empty-Space discovery and labels; destination-aware local and SSH duplicate routing; right/down split destinations; focus and stale-target handling; focused tests and owning-contract updates.
- [x] Out of scope: split left/up destinations, hover-preview highlighting, drag/drop changes, persisted Space numbers/layout, remembering a preferred destination, or changing SSH credential lifetime.

## Acceptance

- [x] Right-clicking an occupied Space shows **Duplicate Session** as a submenu ordered as **In New Tab**, one **Into Space #N** item for each currently empty Space, separator, **Split Right**, **Split Down**.
- [x] No **Into Space** item is rendered when the current tab has no empty Spaces; one item is rendered for each empty Space otherwise.
- [x] Empty Spaces alone are numbered in deterministic visual tree order, starting at 1, and each placeholder displays `Space #N` matching its submenu item.
- [x] Selecting **Into Space #N** duplicates into that existing empty Space without changing the split tree; the source remains unchanged and the duplicate becomes active/focused.
- [x] **Split Right** and **Split Down** split the source Space in the requested direction and place/focus the duplicate in the new Space.
- [x] **In New Tab** and the existing Duplicate Session action/keybinding preserve current new-tab behavior.
- [x] Local duplicates preserve the accepted launch metadata/cwd behavior; SSH duplicates still prompt for credentials and retain the selected destination through authentication without retaining a secret.
- [x] A destination that disappears or becomes occupied before placement is not replaced or redirected; the operation leaves layout/content unchanged and reports that the destination is unavailable.
- [x] Focused tests cover empty-Space enumeration/numbering and destination mutation/validation, and relevant crate/workspace quality gates pass.

## Documentation

### Owning Docs Reviewed

- `docs/terminal-split/01-architecture.md` — Space tree identity, ordered traversal, and fill-empty seam.
- `docs/terminal-split/04-context-menu.md` — current occupied/empty Space menus and duplicate behavior.
- `docs/terminal-split/05-rendering-theme.md` — empty placeholder presentation and theme rules.
- `docs/spec-intakes/IN-0003-duplicate-terminal-sessions/IN-0003.md` — accepted local/SSH duplicate and cwd semantics.
- `docs/decisions/0002-ssh-duplicate-auth.md` — mandatory fresh SSH authentication and secret lifetime.
- `docs/agents/crate-dependency-rules.md` — feature and command-registry dependency constraints.
- `docs/agents/error-policy.md` — notification behavior for unavailable user-action destinations.

### Documentation Action

Update required:

- `docs/terminal-split/01-architecture.md` for ordered empty-leaf discovery and destination identity.
- `docs/terminal-split/04-context-menu.md` for the dynamic submenu and destination behaviors.
- `docs/terminal-split/05-rendering-theme.md` for `Space #N` placeholder labels.

Reason: the currently accepted contracts describe Duplicate Session as a direct new-tab action and placeholders without destination numbers, so they will be stale after implementation.

### Reconciliation

Updated `docs/terminal-split/01-architecture.md`, `04-context-menu.md`, and `05-rendering-theme.md` to describe ordered empty-Space identities/numbers, the dynamic destination submenu, stale-target handling, and numbered placeholders. The implementation matches the intake with no contract deviation.

## Context

- `SpaceId` is stable for a leaf lifetime and must be the destination identity; visible numbers are derived labels only.
- `SpaceTree` children are ordered by rendered axis, so ordered depth-first leaf traversal represents top-to-bottom / left-to-right visual order for numbering.
- `TerminalPanel::duplicate_session` currently owns local new-tab creation and delegates SSH authentication through `WorkspaceCommands::open_duplicate_ssh_dialog`.
- The vendored/reference gpui-component menu API must be checked before choosing submenu construction calls.
- No new crate or persisted schema is expected.

## Plan

- [x] Confirm dynamic nested-menu API in the pinned gpui-component reference and map the SSH duplicate callback path end to end.
- [x] Add a small destination model and ordered empty-Space query/mutation APIs with pure focused tests.
- [x] Render matching numbered placeholders and build the dynamic destination submenu while preserving action/keybinding semantics.
- [x] Carry destination metadata through local and fresh-authenticated SSH creation, revalidate it, place/focus the duplicate, and notify on stale targets.
- [x] Update owning contracts and run focused tests plus mandatory workspace quality gates.

## Decisions

- `docs/decisions/0002-ssh-duplicate-auth.md` — SSH duplicates always authenticate again and never retain password/passphrase material.
- `docs/spec-intakes/IN-0006-duplicate-sessions-into-empty-spaces/high-level-design.md` — destination variants, derived numbering, stable identity, and stale-target rule.

## Verification Plan

- Focused unit tests: `SpaceTree` ordered empty-leaf enumeration; empty destination fill and rejection; split destination shape/order; any extractable menu destination model.
- Feature tests: `cargo test -p oneterm-terminal-view`; focused `oneterm-session-ui`/`oneterm-state` tests if their contracts change.
- Integration/build proof: compile the app command wiring and workspace.
- Manual/E2E where GUI is available: inspect zero/one/many destination menus and labels; exercise local/SSH placement and stale-target behavior. If unavailable, record explicitly.
- Platform quality gate: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo build --workspace`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `cargo test -p oneterm-terminal-view`: 101 passed.
- `cargo test -p oneterm-session-ui -p oneterm-state`: 30 passed, 1 ignored by the existing suite.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed with no issues.
- `cargo build --workspace`: passed.
- `git diff --check`: passed.
- `srcwalk review`: inspected all changed source and documentation seams.
- Manual GUI/E2E was not run in this non-interactive coding session. Dynamic menu rendering and real SSH authentication/host-key confirmation remain compile-, contract-, and unit-verified rather than manually exercised.

## Handoff

Implementation and automated verification are complete. Recommended follow-up is a manual GUI smoke test for menu placement and a real SSH duplicate into an empty Space when an SSH endpoint is available.
