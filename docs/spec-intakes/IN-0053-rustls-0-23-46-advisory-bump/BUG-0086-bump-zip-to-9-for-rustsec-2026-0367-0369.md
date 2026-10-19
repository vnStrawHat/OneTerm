# Work: Bump zip to 9 for RUSTSEC-2026-0367..0369

ID: BUG-0086
Intake: IN-0053
Created: 2026-10-19

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
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
- [x] Out of scope: changing the extraction rules.

## Acceptance

- [ ] `cargo tree -i zip` shows one version, >= 9.0.0.
- [ ] `cargo deny check licenses bans advisories` passes.
- [ ] `oneterm-update` archive tests (zip traversal, symlink, budget) pass unchanged; full gate passes.

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

- [ ] Bump the requirement, `cargo update -p zip`, adapt the API if needed, regenerate notices, gate with `--full`.

## Decisions

Link consequential decisions; do not repeat their rationale here.

## Verification Plan

`cargo test -p oneterm-update`, `cargo deny check licenses bans advisories`, `scripts/ci-local.sh --full`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
