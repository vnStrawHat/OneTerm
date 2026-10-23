# Work: vt-public-api.py and vt-docs read rustdoc from CARGO_TARGET_DIR

ID: BUG-0090
Intake: IN-0056
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

- Change type: bug
- Risk lane: normal
- Spec Intake: IN-0056

## Outcome

`python scripts/vt-public-api.py --check --no-doc` finds the rustdoc output in a worktree: the
doc directory is `$CARGO_TARGET_DIR/doc` when that variable is set, else `<repo>/target/doc`.

## Scope

- [x] In scope: `scripts/vt-public-api.py` (`DOC_ROOT`); `scripts/vt-docs.sh` and
  `scripts/vt-docs.ps1` (the printed guide path).
- [x] Out of scope: the other scripts. Audit: `build-release.sh` / `build-release.ps1` already
  honour `CARGO_TARGET_DIR`; `benchmark-scale.py` writes a default `--output` under
  `<repo>/target` (an output file, not a read of cargo output; unchanged);
  `ci-local.sh` / `ci-local.ps1` only mention `target/doc` in comments and call the python
  script, so they need no change.

## Acceptance

- [ ] With `CARGO_TARGET_DIR` set to a target holding `doc/oneterm_vt`, `--check --no-doc` passes from a worktree.
- [ ] With it unset, the old `<repo>/target/doc` path is used (clear "no rustdoc output" failure when absent).
- [ ] `pwsh scripts/ci-local.ps1` green in the worktree.

## Documentation

### Owning Docs Reviewed

- `AGENTS.md` § 3.1b says the hook exports `CARGO_TARGET_DIR` for every Bash call; accurate,
  no change. Gate prose in § 4 says `--no-doc` "reuses the existing target/doc": still true.

### Documentation Action

No contract change.

## Context

Found by coder-lite during BUG-0089 (see its Gaps): the shared root target made the step fail in a worktree.

## Plan

- [ ] `DOC_ROOT` from `os.environ.get("CARGO_TARGET_DIR")`; same in the two `vt-docs` scripts.
- [ ] python checks, then the full gate when the coordinator frees the shared target.

## Decisions

None.

## Verification Plan

Python checks and the two `vt-public-api.py` runs below; `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### PROOF

(filled after the fix)

### Gaps

None yet.

## Handoff

None.
