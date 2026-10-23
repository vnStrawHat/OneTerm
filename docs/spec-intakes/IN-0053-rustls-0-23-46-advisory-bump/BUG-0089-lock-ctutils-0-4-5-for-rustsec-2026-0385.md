# Work: Lock ctutils at the patched 0.4.5 (RUSTSEC-2026-0385)

ID: BUG-0089
Intake: IN-0053
Created: 2026-10-23

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: maintenance
- Risk lane: normal
- Spec Intake: IN-0053

## Outcome

`cargo deny check advisories` passes: `Cargo.lock` resolves `ctutils` to `0.4.4` or newer
(`0.4.5`), so RUSTSEC-2026-0385 ("Non-constant-time mask generation and u128 comparisons")
no longer matches the graph.

## Scope

- [x] In scope: `Cargo.lock` (`ctutils` only), `THIRD-PARTY-NOTICES.md` if the check requires it.
- [x] Out of scope: `Cargo.toml` requirements, `deny.toml` (no ignore entry), any other
  crate update, and any other advisory that shows up (listed in the report, not fixed).

## Acceptance

- [ ] `cargo deny check advisories` passes.
- [ ] The vault (`oneterm-vault`) and SSH (`oneterm-ssh`) tests pass.
- [ ] `python scripts/third-party-notices.py --check` passes.
- [ ] `pwsh scripts/ci-local.ps1 --full` green.

## Documentation

### Owning Docs Reviewed

- `docs/agents/dependencies.md` — records requirements and families, not resolved patch
  versions. No change.
- `deny.toml` `[advisories]` — the advisory has an upstream fix inside the existing
  requirement, so it must not be deferred. No change.

### Documentation Action

No contract change: only the resolved patch version of a transitive crate moves.

## Context

`ctutils` is transitive via `aes-gcm 0.11` (`oneterm-vault`) and `crypto-bigint` / `russh`.
`cargo update -p ctutils --dry-run` shows `0.4.2 -> 0.4.5`, lockfile-only and semver-compatible,
the same ladder step (a) as `BUG-0056` / `BUG-0085` / `BUG-0087`.

## Plan

- [ ] `cargo update -p ctutils`, then `cargo deny check advisories`.
- [ ] Regenerate `THIRD-PARTY-NOTICES.md` only if `--check` fails.
- [ ] Full gate.

## Decisions

None.

## Verification Plan

- `cargo deny check advisories`; `python scripts/third-party-notices.py --check`;
  `cargo test -p oneterm-vault -p oneterm-ssh`; `pwsh scripts/ci-local.ps1 --full`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Verify command: `pwsh scripts/ci-local.ps1 --full`.

### PROOF

(filled after the run)

### Gaps

(filled after the run)

## Handoff

None.
