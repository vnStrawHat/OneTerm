# Work: Retire the vendored gpui-component fork and refresh the reference tree

ID: US-0043
Intake: IN-0017
Created: 2026-09-07

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [ ] Implemented
- [x] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: maintenance
- Risk lane: normal
- Spec Intake: IN-0017

## Outcome

`vendor/gpui-component`, its four-patch series, `scripts/check-ui-fork.py`,
`scripts/ui-fork-baseline.json` and the CI step that runs it are gone; `vendor/refresh.sh --check`
still proves `alacritty_terminal` and `vte` against pristine + patches; and
`reference/gpui-component` is replaced by `reference/gpui-kit` at `v0.6.0` so the
reference-first rule points at the version the workspace actually builds against.

## Scope

- [x] In scope: delete the vendor tree, patches, `check-ui-fork.py` and its baseline; remove
      the `[patch]` entry and `exclude` line; the `check-ui-fork` step in `.github/workflows/ci.yml`,
      `scripts/ci-local.sh`, `scripts/ci-local.ps1` and `AGENTS.md` §4; the gpui-component arm of
      `vendor/refresh.sh`; `vendor/README.md`; `scripts/README.md`; `scripts/check-doc-paths.py`;
      `deny.toml`; `THIRD-PARTY-NOTICES.md`; archiving `docs/agents/ui-fork-maintenance.md`;
      re-cloning the reference tree and updating the 56 `reference/gpui-component` path references.
- [x] Out of scope: the `alacritty_terminal` / `vte` vendoring and patches remain functionally
      unchanged; source migration belongs to US-0039 through US-0042.

## Acceptance

- [x] `vendor/gpui-component/`, `vendor/patches/gpui-component/`, `scripts/check-ui-fork.py` and
      `scripts/ui-fork-baseline.json` no longer exist.
- [x] Root `Cargo.toml` has no `[patch."https://github.com/longbridge/gpui-component"]` and no
      `vendor/gpui-component` in `exclude`; the alacritty and vte patch entries are byte-identical
      to before.
- [x] `bash vendor/refresh.sh --check` still passes for `alacritty_terminal` and `vte`.
- [x] `scripts/ci-local.sh --full` is green with no `check-ui-fork` step, and the step is gone from
      the GitHub workflow, the PowerShell script and `AGENTS.md` §4 alike.
- [x] `python scripts/check-doc-paths.py`, `python scripts/verify-dependency-graph.py`,
      `python scripts/third-party-notices.py --check` and `python scripts/check-english.py` pass.
- [x] `cargo deny check licenses bans advisories` passes with the new crate set.
- [x] `reference/gpui-kit` exists at tag `v0.6.0` and every lookup-table path in
      `docs/agents/dependencies.md` §5 resolves to a real file.
- [x] `rg -n 'gpui-component-assets|vendor/gpui-component|check-ui-fork'` matches only archived
      historical records.

## Documentation

### Owning Docs Reviewed

- `docs/agents/ui-fork-maintenance.md` — the entire base-revision / delta-review / baseline-update
  procedure for this fork. Its subject ceases to exist.
- `docs/agents/dependencies.md` §4 (integrating with upstream via `[patch]`) and §5
  (reference-first research, including the hard constraint that the reference outranks web search).
- `vendor/README.md` — the vendored-crate table and refresh procedure.
- `AGENTS.md` §3.2 and §4 — the reference-first rule and the quality gate.
- `scripts/README.md` — script inventory.
- `docs/README.md` — the documentation index, which must not point at an archived doc as current.
- `docs/architecture.md` — describes the vendored UI fork as part of the architecture.

### Documentation Action

Update required:

- Archive `docs/agents/ui-fork-maintenance.md` under `docs/archive/` rather than deleting it: the
  record of why the fork existed is worth keeping, but it must stop being reachable as current
  guidance from `dependencies.md` §4 and `docs/README.md`.
- Rewrite `docs/agents/dependencies.md` §4 from "integrating with a vendored fork" to "upgrading a
  crates.io dependency", and repoint every §5 path. The paths change shape, not just prefix:
  `reference/gpui-component/crates/ui/src/dock/` splits into
  `reference/gpui-kit/crates/base/src/dock/` (behavior) and
  `reference/gpui-kit/crates/component/src/dock/` (skin).
- Update `AGENTS.md` §3.2, §4 and the §5 quick-reference table; `vendor/README.md`;
  `scripts/README.md`; `docs/README.md`; `docs/architecture.md`.
- Leave the prose of historical records under `docs/spec-intakes/**` and `docs/archive/**` intact.
  If `check-doc-paths.py` enforces path validity there, relax the check rather than rewrite
  history.

Reason: `AGENTS.md` §3.2 makes the reference tree the mandatory first source of API truth and
explicitly ranks it above web search. A stale tree pinned at 0.5.2, in a repo that no longer holds
the code, would actively mislead every future session — worse than having no reference at all.

### Reconciliation

Before completion, confirm no current doc describes a vendored gpui-component fork or a
`check-ui-fork` gate, and that the reference-first rule names `reference/gpui-kit`.

## Context

- Why the fork can go: `dependencies.md` §4 states it exists because "the dock needs `pub(crate)`
  access across several upstream sibling modules". v0.6.0 makes `TabGroup::panels()`,
  `active_ix()` and `select_tab()` public, so patches 0001 and 0004 are obsolete; 0002 exists only
  because of vendoring; 0003's `page.rs` hunk is superseded upstream by `deferred_scroll_group_ix`
  and its `settings.rs` hunk moves to OneTerm-side code in US-0041.
- `check-ui-fork.py` hard-codes the three-patch list and the allowed-changed-path set, so it fails
  the moment the tree changes shape — it must be removed, not adjusted.
- Sequencing matters: this packet runs **last**. Keeping the vendored tree on disk through
  US-0039–US-0042 leaves an escape hatch for an upstream bug found mid-migration; deleting it early
  would force either an upstream PR or an abandoned branch.
- `vendor/refresh.sh` is the one high-consequence edit here: a careless change silently stops
  verifying the terminal-engine fork, which is a real correctness guard for
  `Handler::report_osc` / `Event::Osc` / `Event::ClearScreen`.
- Detail design: `low-level-design/05-vendor-retirement.md`.

## Plan

- [x] Confirm US-0040 through US-0042 are green with no UI patch applied.
- [x] Delete the UI vendor tree, patches, verifier, and baseline.
- [x] Remove the UI `[patch]`, workspace exclusion, and CI/script hooks.
- [x] Preserve and verify the vte/alacritty terminal-fork checks.
- [x] Reconcile deny policy, notices, path checks, current guidance, and the archived fork record.
- [x] Verify `reference/gpui-kit` at `v0.6.0` and repoint current references.
- [x] Run `scripts/ci-local.sh --full` serially.

## Decisions

- `docs/decisions/DEC-0006-depend-on-published-gpui-component-0-6.md` — rule 5 ("no `[patch]` on
  the UI layer") is what this packet enforces mechanically.

## Verification Plan

Focused: `bash vendor/refresh.sh --check`; `python scripts/check-doc-paths.py`;
`python scripts/check-english.py`; `python scripts/verify-dependency-graph.py`;
`python scripts/third-party-notices.py --check`; `cargo deny check licenses bans advisories`;
the `rg` sweep for stale references.
Integration: `scripts/ci-local.sh --full` end to end.
Platform: a clean-clone build on Windows, to prove nothing still resolves through the deleted tree
or a warm cargo cache.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

A serial `bash scripts/ci-local.sh --full` passed on Windows, including all 955 workspace tests,
all-target clippy, cargo-deny, dependency policy, notices, docs/English checks, and both remaining
terminal-vendor reconstruction checks. The first bundled run hit a transient Windows rustc stack
overflow while compiling the app test binary; the immediate workspace-test rerun and the later
serial full gate both passed.

The obsolete files and UI CI hook are absent. `reference/gpui-kit` is the upstream repository at
exact tag `v0.6.0`; current path checks pass. Stale-token sweeps are clean outside preserved
spec-intake/archive records (and historical review text explicitly rewritten as superseded).
A separate clean-clone build was not run; this checkout's lockfile/source resolution and deleted
paths were validated, but the clean-clone platform criterion remains the only packet-specific gap.

## Handoff

Last packet in IN-0017. Depends on US-0039, US-0040, US-0041, US-0042.
