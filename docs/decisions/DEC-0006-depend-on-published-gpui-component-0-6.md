# DEC-0006 Depend on published gpui-component 0.6.0 with aliased gpui-pre GPUI crates instead of the gpui-kit facade or a vendored fork

Date: 2026-09-07

## Status

Accepted

## Context

OneTerm currently builds against two git dependencies and one local fork:

- `gpui` / `gpui_platform` from `zed-industries/zed` at rev `1d217ee3` (resolved `gpui` 0.2.2),
- `gpui-component` and its former companion assets package from `longbridge/gpui-component`
  at rev `ea6b194d` (resolved 0.5.2), redirected to a local four-patch source snapshot.

Upstream has since renamed the project to **GPUI Kit** (`longbridge/gpui-kit`) and published
v0.6.0 to crates.io. Three facts force a choice rather than a version bump:

1. GPUI itself is no longer reachable as a versioned crates.io release from Zed. The gpui-kit
   maintainer republished a snapshot of Zed's GPUI crates as the `gpui-pre-*` family
   (`gpui-pre` 0.3.x = "snapshot of zed@5b055fa"), and gpui-component 0.6.0 depends on that
   family, not on GPUI 0.2.x. Taking 0.6.0 means taking `gpui-pre`.
2. Upstream now offers two supported consumption shapes: the `gpui-kit` facade (one dependency,
   re-exports GPUI and every layer, own `application()` / `init()` / `actions!`), or depending on
   `gpui-component` directly as before.
3. The original justification for the vendored fork — "the dock needs `pub(crate)` access across
   several upstream sibling modules" (`docs/agents/dependencies.md` §4) — no longer holds, because
   v0.6.0 makes `TabGroup::panels()`, `active_ix()` and `select_tab()` public.

## Decision

Depend on the individual published crates, with the GPUI packages aliased back to their familiar
names, and retire the vendored fork:

```toml
[workspace.dependencies]
gpui = { package = "gpui-pre", version = "0.3" }
gpui_platform = { package = "gpui-pre-platform", version = "0.3", features = [
    "font-kit", "x11", "wayland", "runtime_shaders",
] }
gpui-base = "0.6"
gpui-component = "0.6"
gpui-kit-assets = "0.6"
```

Rules future work inherits:

1. **Do not adopt the `gpui-kit` facade.** Keep the import paths `use gpui::…` and
   `use gpui_component::…`.
2. **`gpui-base`, `gpui-component` and `gpui-kit-assets` move as one version.** Upstream releases
   them together; bumping one alone is not supported.
3. **`gpui-pre` and `gpui-pre-platform` move as one version.**
4. **Do not add `gpui` from git.** This inverts the previous rule, which forbade crates.io.
5. **No `[patch]` on the UI layer.** A defect that needs an upstream change is fixed application-
   side or sent upstream. `alacritty_terminal` and `vte` stay vendored; that is unaffected.
6. **Cargo profile overrides key on package names, not aliases** — `[profile.*.package]` entries
   must say `gpui-pre` / `gpui-pre-platform`, or GPUI silently drops to `opt-level = 0`.

## Alternatives

- [x] Selected: individual crates with package aliasing, no vendor fork.
- [ ] **`gpui-kit` facade.** Upstream's recommendation for new applications, and it removes the
      need to name the `gpui-pre` family at all. Rejected: it would rewrite imports across all 107
      files that use `gpui` or `gpui_component`, and require switching to gpui-kit's own `actions!`
      macro, for no functional gain. Aliasing achieves the same version coupling at zero churn.
      Revisit only if upstream stops publishing `gpui-component` standalone.
- [ ] **Keep the vendored fork, rebase patches onto 0.6.0.** Rejected: three of the four patches
      become obsolete under 0.6.0's public `TabGroup` API, and the fourth (settings-section scroll)
      is fixable application-side. Retaining the fork would keep UI-fork refresh, baseline, and
      maintenance machinery alive to guard a single behavioral workaround.
- [ ] **Stay on 0.5.2 / the pinned Zed rev.** Rejected: the pinned rev is not a tagged release and
      upstream development has moved to the gpui-kit repo, so the current position receives no
      fixes and drifts further from every future upgrade path.

## Consequences

- [x] Benefit confirmed: the former UI source snapshot, its four patches, baseline tooling, and
      one CI step are removed; `vendor/refresh.sh` no longer clones the UI repository.
- [x] Benefit confirmed: dependency upgrades become ordinary version bumps with no patch rebase.
- [ ] Tradeoff: GPUI now arrives through `gpui-pre`, a republish maintained by the gpui-kit author
      rather than by Zed. If Zed resumes publishing GPUI to crates.io, or gpui-kit switches to it,
      re-evaluate. Rollback path: restore the git dependency on a Zed rev and pin gpui-component to
      the last version compatible with it.
- [ ] Tradeoff: `THIRD-PARTY-NOTICES.md`, `deny.toml` and
      `scripts/verify-dependency-graph.py` must absorb a materially different crate set.
- [x] Follow-up completed: the stale 0.5.2 reference was replaced by `reference/gpui-kit` at
      `v0.6.0`, matching the mandatory reference-first source in `AGENTS.md` §3.2.
- [x] Follow-up completed: the settings-section scroll fix is OneTerm-side code plus a regression
      test, since the upstream index-offset bug still exists in 0.6.0.
