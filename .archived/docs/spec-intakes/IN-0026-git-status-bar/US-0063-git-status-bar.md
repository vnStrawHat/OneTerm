# Work: Git status indicator in the status bar

ID: US-0063
Intake: IN-0026
Created: 2026-09-11

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [x] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: normal
- Spec Intake, when required: IN-0026

## Outcome

The status bar shows, after the breadcrumb, the git branch of the active local terminal's
cwd with a `*` dirty marker and ahead/behind counts, refreshed in the background; it hides
for SSH sessions, non-repositories, and when git is unavailable.

## Scope

- [x] In scope: `ActiveTerminalMetricsProvider.local_cwd` + `active_terminal::local_cwd`;
  `TerminalPanel::local_cwd`; `widgets/git_status.rs` (poll, parse, label); status bar
  wiring; parser unit tests; docs.
- [x] Out of scope: git status for SSH sessions (would need a remote command); a setting to
  hide the indicator; file-system watching instead of polling; click actions on the label.

## Acceptance

- [x] A local shell inside a repository shows `branch`, `branch*` when dirty, `↑n ↓m` when
  the upstream differs, and the short oid when detached (parser unit test; GUI:
  `evidence/US-0063-repo-dirty-dark.png` shows `feat/git-status-bar*`).
- [x] The indicator hides when the active terminal is an SSH session, has no cwd, or the cwd
  is outside a repository (parser returns `None` on non-zero exit; SSH filtered by kind; GUI:
  `evidence/US-0063-outside-repo-dark.png`).
- [x] The UI thread never blocks on git: the command runs on the background executor, at
  most one run in flight, and at most one run per 2 s per unchanged cwd (by construction in
  `widgets/git_status.rs`; not measured).
- [x] `pwsh scripts/ci-local.ps1` green.
- [x] Acceptance rework (owner trial, 2026-09-11): each status-bar indicator shows a leading
  icon (clock, folder, git branch, network, CPU), hidden together with its label (GUI:
  `evidence/US-0063-icons-left-dark.png`, `evidence/US-0063-icons-right-dark.png`).
- [x] Acceptance rework 2 (owner trial, 2026-09-11): the git label uses the foreground colour
  instead of muted, and a dirty tree appends `(+added -removed)` line counts against `HEAD`
  from `git diff --numstat`, `+` in the success colour and `-` in the danger colour
  (unit: `build_label` segment tones; GUI: `evidence/US-0063-diffstat-dark.png`).
- [x] Acceptance rework 3 (owner trial, 2026-09-11): every status-bar indicator's text uses the
  foreground colour instead of muted (`Label::from(String)` maps to `Tone::Foreground`; GUI:
  `evidence/US-0063-foreground-dark.png`).

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0026-git-status-bar/high-level-design.md` — flow and seams.
- `docs/gui-layout.md` § Status bar — widget list; must gain the git indicator.
- `docs/agents/structure.md` — widget module list and the `active_terminal.rs` comment.
- `docs/architecture.md` § services — provider description is generic; no change.
- `README.md` § UI — feature bullet.
- `docs/agents/crate-dependency-rules.md` — the shell may not depend on terminal-view; the
  provider seam keeps that true.

### Documentation Action

Update required: `docs/gui-layout.md`, `docs/agents/structure.md`, `README.md`.

Reason: the status bar widget list and widget module list are enumerated in those docs.

### Reconciliation

Changed: `docs/gui-layout.md` § Status bar (git indicator, polling, hidden cases),
`docs/agents/structure.md` (widget list, `active_terminal.rs` comment), `README.md` § UI
bullet. `docs/architecture.md` left unchanged: it names the provider generically.

## Context

- `StatusText` samplers run on the UI thread each tick and return `Option<String>`; the
  git run therefore goes through `cx.background_executor().spawn(..).detach()` with the
  result parked in an `Arc<Mutex<..>>` the sampler reads.
- `crates/update/src/install.rs` already uses `CREATE_NO_WINDOW` for a hidden child on
  Windows; the same flag is used here.
- `GIT_OPTIONAL_LOCKS=0` keeps `git status` from writing the index, so it cannot collide
  with a git command the user runs in the same terminal.

## Plan

- [x] Provider field + panel accessor + terminal-view extractor.
- [x] Widget with parser and tests.
- [x] Status bar wiring.
- [x] Docs, CI, GUI check.

## Decisions

None.

## Verification Plan

- `cargo test -p oneterm-workspace git_status` — parser cases.
- `cargo test -p oneterm-state -p oneterm-terminal-view` — provider fixture and status tests.
- `pwsh scripts/ci-local.ps1`.
- GUI: launch the fast-dev build in this repository, open a local shell, screenshot the bar.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `cargo test -p oneterm-workspace -p oneterm-state -p oneterm-terminal-view`: 10 / 37 / 282
  passed (includes `widgets::git_status::tests::parse_porcelain_formats_branch_dirty_and_ahead_behind`).
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `pwsh scripts/ci-local.ps1` (2026-09-11): green after removing a non-English quote from the
  intake source line (first run failed only at `check-english.py`).
- GUI: `evidence/US-0063-gui-walk.md` with two status-bar crops.
- Gaps: no SSH session, detached HEAD, or ahead/behind walked in the GUI (unit-tested only);
  the separator before the indicator stays visible while it is hidden, matching the existing
  breadcrumb separator behaviour; polling cost on very large repositories not measured.
- Rework (icons): `StatusText` takes an optional `Icon`; `git-branch.svg` and `clock.svg`
  added to `crates/theme/assets/icons` (`AppIcon::GitBranch`, `AppIcon::Clock`), folder /
  network / CPU come from the kit `IconName` set. Clippy clean; ci-local rerun green.
- Rework 2 (diffstat): `StatusText` labels are now `Label(Vec<Segment>)` with a `Tone`
  (muted / foreground / success / danger) per segment; plain widgets convert their `String`
  into one muted segment. The git widget runs `git diff --numstat HEAD` after `git status`
  in the same background batch. Tests: `build_label_*` and `parse_numstat_*` (workspace 11).
- Not committed; the branch `feat/git-status-bar` holds the working tree.

## Handoff

None.
