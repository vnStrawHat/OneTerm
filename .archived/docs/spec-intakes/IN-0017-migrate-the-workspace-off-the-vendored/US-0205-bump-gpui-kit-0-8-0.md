# Work: Bump GPUI Kit to 0.8.0 (gpui-pre stays 0.3.9)

ID: US-0205
Intake: IN-0017
Created: 2026-10-23

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

- Change type: maintenance (dependency upgrade)
- Risk lane: normal — UI layer only; no auth, persistence schema, or public `oneterm-vt` API.
- Spec Intake: [`IN-0017`](IN-0017.md) (the intake that put OneTerm on published GPUI Kit; owner ruling Q1, 2026-10-23: no new intake)

## Outcome

The workspace requires `gpui-base` / `gpui-component` / `gpui-kit-assets` `0.8` and resolves them at
`0.8.0`; `gpui-pre` / `gpui-pre-platform` stay at `0.3.9` (GPUI Kit 0.8.0 pins `gpui-pre =0.3.9`).
The full local CI gate passes and the UI builds with no local `[patch]` (DEC-0006 rules unchanged).
Research: [`research-gpui-kit-0-8-0.md`](research-gpui-kit-0-8-0.md).

## Scope

- [x] In scope: root `Cargo.toml` kit requirements `"0.7"` -> `"0.8"`; `Cargo.lock` (`cargo update -p`
  on the kit layers only); any compile or behaviour fix-up 0.8.0 forces (expected none);
  `reference/gpui-kit` re-vendored at tag `v0.8.0`; tag citations in `docs/agents/dependencies.md`,
  `docs/agents/structure.md` (~line 230) and `docs/terminal-backend.md` (~line 1257);
  `THIRD-PARTY-NOTICES.md` regenerated (only the four kit packages move; `gpui-fast` / `gpui-fast-macros` stay as optional, inactive entries, as at 0.7.3);
  `cargo deny check licenses bans advisories`.
- [x] Out of scope (owner ruling Q2, pure bump): adopting any 0.8.0 feature — `DataTable::fit_col`
  and double-click auto-fit, `DockToggleEvent`, `ScrollableMask::vertical_wheel`, `fixed_cols_border`,
  `select_on_modifier_click`, `focus_ring`, `InputState::display_map()` / `prewrap`,
  `TabVariant::Folder`, the `syntect` feature.
- [x] Out of scope (Q4): the history wording "the 0.7.3 kit" in `docs/gui-layout.md` and the
  `crates/settings-ui` module notes stays; it records when the fix arrived.
- [x] Out of scope (Q5): no upstream request about the kit's unconditional "Zoom In" row and no
  follow-up packet. The `PopupMenu::rebuild` strip in `crates/terminal-view/src/panel/tab_title.rs`
  (`US-0203`) stays; 0.8.0 still appends `menu_with_disabled("Dock.Zoom In", !menu_zoom)`
  unconditionally (`component/src/dock/tab_panel.rs` at `v0.8.0`).

## Acceptance

- [x] `Cargo.toml` kit lines are `"0.8"`; `gpui` / `gpui_platform` stay `"0.3"` and resolve `0.3.9`.
- [x] `cargo tree -i` shows one version each: `gpui-pre` / `gpui-pre-platform` 0.3.9,
  `gpui-base` / `gpui-component` / `gpui-component-macros` / `gpui-kit-assets` 0.8.0.
- [x] Only the four kit packages move in `Cargo.lock`; `gpui-fast` / `gpui-fast-macros` stay as
  optional, inactive entries, as at 0.7.3.
- [x] `reference/gpui-kit` sits at `v0.8.0`; the three doc citations name `v0.8.0`.
- [x] `THIRD-PARTY-NOTICES.md` regenerated; `python scripts/third-party-notices.py --check` passes.
- [x] `cargo deny check licenses bans advisories` clean (or only a failure that also fails on the base commit, recorded).
- [x] `force_width_copy_matches_gpui` passes (dependencies.md § 4 step 4; `gpui-pre` unchanged).
- [x] Full gate (`pwsh scripts/ci-local.ps1 --full`) green.
- [x] The strip "..." menu still has no zoom row: the tests in
  `crates/terminal-view/src/panel/tests/tab_menu_and_card.rs` pass, checked against the 0.8.0 rule
  "nested context menus open only the innermost".
- [ ] ui-verifier Windows walk (Q3): SFTP table Home / End select the first / last row and the wheel
  works in both panes; Session tree context-menu wheel; Settings `Select` dropdowns and a long
  settings page scroll; tab-strip "..." menu (tab rows only, no zoom row); one dialog; one toast;
  rail and strip drag-reorder smoke. Added after the advisor review: (a) Session tree: wheel-scroll a
  long tree; left-click a group toggles it; click a session then Enter / Delete act on it;
  press-and-drag-away leaves selection and expansion alone (the kit `Tree` moved select / toggle from
  mouse-down to click and wrapped the tree in `ScrollBounce`); (b) a dialog with a multi-line text
  input (the startup command dialog, `IN-0051`): the wheel over the textarea scrolls the textarea,
  not the page.

## Documentation

### Owning Docs Reviewed

- `docs/agents/dependencies.md` — version table (§ 1), declaration (§ 2), upgrade procedure (§ 4), reference tag (§ 5).
- `docs/decisions/DEC-0006-depend-on-published-gpui-component-0-6.md` — an ordinary version bump; rules unchanged.
- `docs/agents/structure.md`, `docs/terminal-backend.md` — cite the reference tag.
- `docs/gui-layout.md`, `crates/settings-ui` module notes — "the 0.7.3 kit" history wording; left as is (Q4).

### Documentation Action

- Update required: `docs/agents/dependencies.md` (intro, table, § 2 snippet, § 5 tag),
  `docs/agents/structure.md` (reference tag), `docs/terminal-backend.md` (reference tag).

Reason: they state the pinned kit version and reference tag, which change.

### Reconciliation

Before completion: list docs changed; DEC-0006 rules re-read, expected unchanged.

## Context

- 0.8.0 breaking changes (13) were grep-checked against `crates/` in the research; none names an API
  OneTerm calls. Exposure is behavioural: DataTable Home / End in row mode (both SFTP panes,
  `crates/sftp-ui/src/panel.rs`, `local_pane.rs`), scroll wrapper owning the vertical wheel
  (`PopupMenu`, `Select`, form dialog), and nested context menus opening only the innermost.
- `reference/gpui-kit` is gitignored and lives in the main checkout; a worktree has none. The
  worktree gets its own clone at `v0.8.0` (not committed); the shared checkout needs the same refresh.
- `US-0149` hit `E0786` at 20 build jobs: build with `CARGO_BUILD_JOBS=4`.

## Plan

- [x] Records first (this packet, the `IN-0017` entry).
- [x] Manifest to `"0.8"`; `cargo update -p gpui-base -p gpui-component -p gpui-component-macros -p gpui-kit-assets` (+ nothing else); inspect the lock diff.
- [x] Re-vendor `reference/gpui-kit` at `v0.8.0`; update the three doc citations and the `dependencies.md` table.
- [x] Regenerate notices; `cargo deny`.
- [x] Build, fix any fallout, run the gate, record counts.
- [x] Build the debug binary for ui-verifier.

## Decisions

None new; DEC-0006 covers an ordinary version bump.

## Verification Plan

`cargo tree -i` per family; `pwsh scripts/ci-local.ps1 --full`; `cargo test -p oneterm-terminal-view` strip-menu tests;
ui-verifier walk of the list under Acceptance.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Prepared before any build (file operations and `cargo update` only):

- `cargo update -p gpui-base -p gpui-component -p gpui-component-macros -p gpui-kit-assets`: exactly those four
  packages moved 0.7.3 -> 0.8.0 (version + checksum, 8 lines of `Cargo.lock`); nothing else moved.
  `cargo tree -q -i <pkg> --depth 0`: `gpui-pre` / `gpui-pre-platform` 0.3.9, the four kit packages 0.8.0, one version each.
- `gpui-fast` / `gpui-fast-macros` do **not** leave `Cargo.lock` (a lock file keeps optional dependencies) and do not
  leave `THIRD-PARTY-NOTICES.md` (`cargo metadata`, which the notices script reads, still lists the optional
  edge from `gpui-base` / `gpui-component`). They are, however, out of the real build graph:
  `cargo tree --target all -e all --workspace -i gpui-fast` prints nothing and no `gpui-base` feature
  `gpui-fast` is active. The notices over-report them (conservative); `--check` passes. The script was not changed
  (pure bump). The research claim that they would leave the lock was wrong: they were already optional at 0.7.3.
- `reference/gpui-kit`: shallow clone of `longbridge/gpui-kit` at `v0.8.0` inside this worktree (gitignored;
  tag `v0.8.0`, crate version 0.8.0). The shared checkout needs the same refresh.
- `cargo deny check licenses bans advisories`: advisories ok, bans ok, licenses ok (3 `license-exception-not-encountered`
  warnings, as before).
- `python scripts/third-party-notices.py --check`, `verify-dependency-graph.py`, `check-doc-paths.py`, `check-english.py`: pass.
- Doc citations updated: `docs/agents/dependencies.md` (intro, table, section 2 snippet, section 5), `docs/agents/structure.md`,
  `docs/terminal-backend.md`, `docs/gui-layout.md` line 40 ("GPUI Kit 0.7 `Settings` widget" -> "GPUI Kit `Settings` widget").
  Left per Q4: "the 0.7.3 kit" history wording; the `crates/theme/src/theme.rs` pin comment and `.claude/agents/coder.md` still say 0.7.

Build and gate (after merging main `2dc3e1f0`, which was conflict-free; head `09ab82c5` at gate time):

- `CARGO_BUILD_JOBS=4 pwsh scripts/ci-local.ps1 --full` on Windows, 2026-10-23: every step passed ("ci-local: all checks
  passed", exit 0), including both clippy runs, `cargo test --workspace`, the three `--ignored --test-threads=1` lines,
  the `oneterm-vt` feature runs, `vt-public-api.py` checks, notices `--check`, and `cargo deny check licenses bans
  advisories` (advisories ok, bans ok, licenses ok).
- Totals over the `cargo test --workspace` run: 141 test suites, 5155 passed, 0 failed, 33 ignored (US-0149 had 4819 passed).
- Source fallout: none. No OneTerm source file changed; the 0.8.0 breaking changes reach nothing the code calls.
- `force_width_copy_matches_gpui` ran in the workspace test run and passed (`gpui-pre` unchanged at 0.3.9).
- Nested-context-menu rule (kit 0.8.0: nested context menus open only the innermost) against
  `crates/terminal-view/src/panel/tests/tab_menu_and_card.rs` and the `tab_menu_open` tests: all pass unchanged, including
  `the_session_panel_offers_no_zoom` and `the_sftp_panel_offers_no_zoom`. Those tests assert panel/menu content, so a pass
  shows the strip menu content is unchanged; the interactive nesting is for the ui-verifier walk.
- `cargo build -p oneterm-app` (debug) succeeded; `oneterm.exe`, `conpty.dll` and `x64\OpenConsole.exe` copied to the
  `r12bin` scratchpad folder for the ui-verifier.
- Gaps: the ui-verifier Windows walk (acceptance last line) has not run, so E2E proof stays open; Linux / macOS wait for CI.
