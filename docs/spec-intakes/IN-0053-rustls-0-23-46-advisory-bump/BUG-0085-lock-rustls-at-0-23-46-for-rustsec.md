# Work: Lock rustls at 0.23.46 for RUSTSEC-2026-0346..0349

ID: BUG-0085
Intake: IN-0053
Created: 2026-10-15

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
- Change type: maintenance (security advisory, lockfile only)
- Risk lane: normal — TLS used only by the update checker (`oneterm-update` via `reqwest`); no API change.
- Spec Intake, when required: IN-0053

## Outcome

`Cargo.lock` resolves `rustls` to a release that fixes RUSTSEC-2026-0346, -0347, -0348 and -0349 (>= 0.23.46), and `cargo deny check licenses bans advisories` passes.

## Scope

- [x] In scope: `Cargo.lock` (`rustls` and anything it must pull), `THIRD-PARTY-NOTICES.md` if the graph changes.
- [x] Out of scope: any manifest or source change; other crates.

## Acceptance

- [x] `cargo tree -i rustls` shows one version, >= 0.23.46.
- [x] `cargo deny check licenses bans advisories` passes.
- [x] `third-party-notices.py --check` passes; the crates that use TLS build and test (`oneterm-update`, `oneterm-settings-ui`).

## Documentation

### Owning Docs Reviewed

- `docs/agents/dependencies.md` — lists no `rustls` version; the lockfile is the record.
- `docs/spec-intakes/IN-0034-rustls-advisory-bump/` — the same change for RUSTSEC-2026-0285; this follows it.

### Documentation Action

Choose one and explain it before implementation:

- No contract change: no doc names the `rustls` version; behaviour is unchanged.

Reason: lockfile-only patch release.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

Add only relevant code, decisions, dependencies, and invariants not already clear from the owning docs.

## Plan

- [x] `cargo update -p rustls --precise 0.23.46`; `cargo tree -i rustls`.
- [x] `cargo deny`, notices, build/test the TLS users.

## Decisions

Link consequential decisions; do not repeat their rationale here.

## Verification Plan

`cargo deny check licenses bans advisories`; `cargo test -p oneterm-update -p oneterm-settings-ui`; `third-party-notices.py --check`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `cargo update -p rustls --precise 0.23.46`: `rustls v0.23.45 -> v0.23.46`, nothing else moved; `Cargo.lock` 2 lines, `THIRD-PARTY-NOTICES.md` 1 line (regenerated, `--check` ok).
- `cargo tree -q -i rustls`: one version, 0.23.46.
- `cargo deny check licenses bans advisories`: advisories ok, bans ok, licenses ok.
- `cargo test -p oneterm-update -p oneterm-settings-ui`: 59 + 59 passed, 0 failed.
- The full `ci-local.sh --full` ran green up to `cargo deny` on the US-0190 tree an hour earlier; this change moves only `rustls`, so it was not re-run in full. `platform_proof` waits for CI.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
