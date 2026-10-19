# Work: Bump zip to 9 for RUSTSEC-2026-0367..0369

ID: BUG-0086
Intake: IN-0053
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
- Change type: maintenance (security advisory, major dependency bump)
- Risk lane: normal — `zip` is used only by `oneterm-update` to unpack the Windows update archive; the extraction loop is ours and already rejects symlinks and never applies Unix permissions, so the advisories do not reach OneTerm. The bump is for the gate (`cargo deny`) and hygiene.
- Spec Intake, when required: IN-0053

## Outcome

`zip` resolves to >= 9.0.0 and `cargo deny check licenses bans advisories` passes; update-archive extraction behaves as before.

## Scope

- [x] In scope: workspace `zip` requirement, `Cargo.lock`, any API adaptation in `crates/update`, `THIRD-PARTY-NOTICES.md`.
- [x] Out of scope: loosening the extraction rules.

## Acceptance

- [x] `cargo tree -i zip` shows one version, >= 9.0.0.
- [x] `cargo deny check licenses bans advisories` passes.
- [x] `oneterm-update` archive tests (zip traversal, symlink, budget) pass unchanged; full gate passes.

## Documentation

### Owning Docs Reviewed

- `docs/agents/dependencies.md` — Auto-update row lists `zip` without a version; step 5 (deny.toml, notices).
- `crates/update/src/archive.rs` — `extract_zip`: `enclosed_name`, symlink rejection, size budget.

### Documentation Action

Choose one and explain it before implementation:

- No contract change.

Reason: `dependencies.md` names `zip` without a version and the extraction contract is unchanged; only `THIRD-PARTY-NOTICES.md` is regenerated if the graph changes.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

Add only relevant code, decisions, dependencies, and invariants not already clear from the owning docs.

## Plan

- [x] Bump the requirement, `cargo update -p zip`, adapt the API if needed, regenerate notices, gate with `--full`.

## Decisions

Link consequential decisions; do not repeat their rationale here.

## Verification Plan

`cargo test -p oneterm-update`, `cargo deny check licenses bans advisories`, `scripts/ci-local.sh --full`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `Cargo.toml` `zip = "9"`; `cargo update -p zip`: zip 2.4.2 -> 9.0.5, adds `typed-path` 0.12.3 and `zlib-rs` 0.6.8 (Zlib licence, allowed by `deny.toml`), drops `derive_arbitrary`. `cargo tree -i zip`: one version, 9.0.5.
- API: `ZipFile::name()` now returns a `Result`; the two error messages use `name_raw()` (lossy UTF-8).
- Behaviour change caught by `zip_absolute_entry_is_rejected`: zip 9's `enclosed_name` strips a leading root or drive instead of refusing it, so `/etc/evil.txt` extracted as `etc/evil.txt` inside the staging directory (confined, but no longer refused). `extract_zip` now refuses an entry whose raw name starts with `/`, `\` or a drive letter, keeping the old contract. New test `zip_drive_and_backslash_rooted_entries_are_rejected`; the absolute test also asserts nothing was written. `cargo test -p oneterm-update`: 60 passed.
- Not exposed to the advisories either way: `extract_zip` never calls `ZipArchive::extract`, refuses symlinks, and sets no Unix permissions.
- `python scripts/third-party-notices.py` regenerated (863 packages).
- `CARGO_BUILD_JOBS=6 bash scripts/ci-local.sh --full`: all checks passed, `cargo deny`: advisories ok, bans ok, licenses ok; 141 suites / 5040 passed / 0 failed.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
