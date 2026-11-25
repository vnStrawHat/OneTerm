# Low-Level Design: Retiring the vendored gpui-component fork

Intake: IN-0017
HLD: ../high-level-design.md
Topic: vendor-retirement
Date: 2026-09-07

## Concern

Removing `vendor/gpui-component`, its four-patch series, and the machinery that guards it —
without weakening the guard that still protects the two vendored crates that stay
(`alacritty_terminal`, `vte`).

## Design

### Why the fork can go

`docs/agents/dependencies.md` §4 states the reason it exists: "the dock needs `pub(crate)` access
across several upstream sibling modules". v0.6.0 removes that need.

| Patch | Purpose | Status after 0.6.0 |
| --- | --- | --- |
| `0001-OneTerm-add-TabPanel-set_active_panel` | public wrapper over `TabPanel`'s private `set_active_ix`, so Agent navigation can activate a panel | **Obsolete.** `TabGroup::panels()` + `TabGroup::select_tab(ix, ..)` are public. |
| `0002-OneTerm-make-Cargo-manifest-standalone` | inline workspace-provided edition/dep versions so the vendored `crates/ui` builds outside its workspace | **Obsolete.** Only needed because of vendoring. |
| `0003-OneTerm-fix-settings-section-scroll` | deterministic first-click settings scrolling | **Split.** The `page.rs` half is superseded upstream (`deferred_scroll_group_ix`); the `settings.rs` index-offset half is still needed and moves to an OneTerm-side fix — see `04-component-api-drift.md` §4. |
| `0004-OneTerm-add-TabPanel-panel_count` | read-only panel count for terminal final-tab close policy | **Obsolete.** `TabGroup::panels().len()`. |

So three of four disappear and the fourth becomes application-side code. Nothing left justifies a
fork.

### Removal set

Files and directories deleted:

- `vendor/gpui-component/` (the whole snapshot)
- `vendor/patches/gpui-component/` (all four patches)
- `scripts/check-ui-fork.py`
- `scripts/ui-fork-baseline.json`
- `docs/agents/ui-fork-maintenance.md` — this doc is *entirely* about the gpui-component fork
  (base-revision, delta-review, baseline-update procedure). Delete it and drop the reference from
  `dependencies.md` §4, or archive it under `docs/archive/`. Prefer archiving: the procedure is a
  useful record of why the fork existed.

Root `Cargo.toml`:

```diff
 exclude = [
     "vendor/vte",
     "vendor/alacritty_terminal",
-    "vendor/gpui-component",
 ]

-[patch."https://github.com/longbridge/gpui-component"]
-gpui-component = { path = "vendor/gpui-component" }
```

The `alacritty_terminal` and `vte` patches and excludes stay exactly as they are.

### Machinery that must be updated, not just trimmed

Each of these currently knows about the gpui-component fork; missing one turns into a red CI run
on an unrelated later change:

| File | Change |
| --- | --- |
| `.github/workflows/ci.yml` | drop the `check-ui-fork.py` step |
| `AGENTS.md` §4 | drop `python scripts/check-ui-fork.py` from the listed gate |
| `scripts/ci-local.sh`, `scripts/ci-local.ps1` | drop the same step |
| `vendor/refresh.sh` | remove the gpui-component arm; keep `--check` working for the other two |
| `vendor/README.md` | remove §1 row and any gpui-component-specific procedure |
| `scripts/README.md` | remove the `check-ui-fork.py` entry |
| `scripts/check-doc-paths.py` | it validates architecture-doc paths and references `gpui-component`; re-point or drop those entries |
| `scripts/third-party-notices.py` | regenerate `THIRD-PARTY-NOTICES.md`; the crate set changes substantially (new `gpui-pre-*`, `gpui-base`, `gpui-kit-assets`, plus transitive churn) |
| `deny.toml` | remove any gpui-component git-source allowance; add whatever the new crates need |
| `scripts/verify-dependency-graph.py` | encodes allowed per-crate dependencies; `gpui-base` is new and `gpui-component-assets` is renamed |

### Reference tree refresh

`AGENTS.md` §3.2 and `dependencies.md` §5 make `reference/gpui-component/` the mandatory
first-stop for API research, with a hard constraint against web search. That reference is a clone
pinned at `ea6b194d` — i.e. 0.5.2 — and after this migration it describes the wrong version, in
the wrong repo, with the wrong module layout (`crates/ui/src/` no longer exists; it is
`crates/component/src/` + `crates/base/src/`). Leaving it in place is worse than deleting it:
the rule says trust it over the web, so every future agent session would be pointed at stale
dock APIs.

Required:

1. Re-clone as `reference/gpui-kit` at tag `v0.6.0` (repo `longbridge/gpui-kit`).
2. Update all 56 `reference/gpui-component` path references across `AGENTS.md`, `README.md`, and
   `docs/**`. Note the lookup-table paths change shape, not just the prefix:
   `reference/gpui-component/crates/ui/src/dock/dock.rs` becomes both
   `reference/gpui-kit/crates/base/src/dock/` (behavior) and
   `reference/gpui-kit/crates/component/src/dock/` (skin).
3. `docs/agents/dependencies.md` §1 rev-lock table → version table; §2 declaration block;
   §4 rewritten from "integrating with a vendored fork" to "upgrading a crates.io dependency";
   §5 quick-reference paths.
4. Historical docs under `docs/spec-intakes/**` and `docs/archive/**` that cite
   `reference/gpui-component` are records of past work — leave their prose intact; only fix them
   if `check-doc-paths.py` enforces path validity there, in which case prefer relaxing the check
   over rewriting history.

### Sequencing

Vendor removal is **last** (P5), not first. Keeping the vendored tree available during P1–P4
leaves an escape hatch: if the migration hits an upstream bug that blocks progress, a temporary
patch is still possible. Removing it early would force either an upstream PR or an abandoned
branch. The tree is deleted only once P2–P4 are green without any patch applied.

## Interfaces

No runtime interface. The deliverables are the absence of files and green CI:

```bash
scripts/ci-local.sh --full     # includes vendor/refresh.sh --check and cargo deny
bash vendor/refresh.sh --check # must still prove alacritty_terminal + vte == pristine + patches
```

## Edge Cases and Failure Modes

- [ ] `vendor/refresh.sh --check` breaks for the remaining two crates while the gpui-component arm
      is removed — this is the one script where a careless edit silently stops verifying the
      terminal-engine fork, which is a real correctness guard.
- [ ] The vendor tree is deleted before P4 proves persistence, leaving no fallback.
- [ ] `reference/` is deleted but not replaced, so the reference-first rule points at nothing and
      agents fall back to web search against an unknown version.
- [ ] `THIRD-PARTY-NOTICES.md` is regenerated but the licence set changed (new transitive crates);
      `cargo deny check licenses` must pass, not just the notices check.
- [ ] `docs/README.md` (documentation index) and `docs/architecture.md` still describe a vendored
      UI fork.

## Verification

- [ ] `bash vendor/refresh.sh --check` passes for `alacritty_terminal` and `vte`.
- [ ] `scripts/ci-local.sh --full` green with no `check-ui-fork` step.
- [ ] `python scripts/check-doc-paths.py` passes.
- [ ] `python scripts/check-english.py` passes on all rewritten docs.
- [ ] `rg -n 'gpui-component-assets|vendor/gpui-component|check-ui-fork'` returns only archived
      historical records.
- [ ] `reference/gpui-kit` exists at `v0.6.0` and the `dependencies.md` §5 lookup table resolves.
