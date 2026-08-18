# Vendored `gpui-component` maintenance

`vendor/gpui-component` is a source snapshot of the upstream `gpui-component`
`crates/ui` package. It is used through Cargo's `[patch]` mechanism and is not a
OneTerm workspace member or an `oneterm-ui` crate.

**Why a fork at all.** OneTerm needs two small behaviour changes that cannot be made
from outside the crate: `TabPanel::set_active_panel` (the Agent Panel activates a
terminal tab by panel entity; upstream's `set_active_ix` is private and reachable only
from tab clicks / add-remove — patch `0001`, 45 lines) and a settings-page scroll fix
(group heights are measured before the first click and sidebar child clicks map to the
filtered group index — patch `0003`, 41 lines, private state of the settings page). Both
are candidates for upstreaming; the whole 259-file snapshot exists only to carry those
~90 lines plus the standalone manifest (`0002`). Once both land upstream and the pinned
rev is bumped past them, delete `vendor/gpui-component`, `vendor/patches/gpui-component`,
`scripts/check-ui-fork.py` and `scripts/ui-fork-baseline.json`, drop the `[patch]` entry
from the root `Cargo.toml`, and this document (BUILD-21).

## Upstream base

The authoritative upstream base is the `gpui-component` revision in `Cargo.toml`:
`ea6b194db04cc7c0474851f07c7d5b7a9df6a98b`. The vendor snapshot is regenerated from
a clean clone of `https://github.com/longbridge/gpui-component` checked out at that
exact revision. The ignored `reference/gpui-component` checkout is for API research
only and is not the source used to generate the vendor snapshot.

`vendor/README.md` records the provenance, pinned revisions, and patch workflow.

## Delta surface

The current OneTerm source patch set is intentionally split across three patch
files:

- `vendor/gpui-component/src/dock/tab_panel.rs` — exposed by `0001`, adds
  `TabPanel::set_active_panel` for Agent navigation.
- `vendor/gpui-component/src/setting/page.rs` — exposed by `0003`, measures
  settings groups up front so section navigation has stable item heights on the
  first click.
- `vendor/gpui-component/src/setting/settings.rs` — exposed by `0003`, maps
  sidebar child clicks to the actual filtered group index so untitled groups do
  not offset the scroll target.

All other files under `vendor/gpui-component/src/` must match the pinned upstream
revision. New UI components belong in upstream `gpui-component` or in a feature
crate, not in the vendor patch.

## Synchronization procedure

1. Check the pinned `gpui-component` revision in `Cargo.toml`.
2. Clone `https://github.com/longbridge/gpui-component` into a temporary directory,
   check out that exact revision, and verify its `HEAD` before copying `crates/ui`.
3. Apply all three patches under `vendor/patches/gpui-component/` from the clone root.
   Verify that `0001` changes only `crates/ui/src/dock/tab_panel.rs`, that
   `0002` only makes `crates/ui/Cargo.toml` standalone, and that `0003` changes
   only `crates/ui/src/setting/page.rs` and `crates/ui/src/setting/settings.rs`.
4. Copy the patched `crates/ui` package into `vendor/gpui-component`.
5. Keep the vendor package excluded from the OneTerm workspace.
6. Run `python scripts/check-ui-fork.py --update` to review the complete source
   snapshot and refresh `scripts/ui-fork-baseline.json`.
7. Run `python scripts/check-ui-fork.py`, then the workspace format, clippy, build,
   and test gates before committing.

## Upstreaming a patch (retiring the fork)

The fork should shrink, not grow. To send a patch upstream:

1. Re-create the change against upstream `main` of
   `https://github.com/longbridge/gpui-component` (the vendored base `ea6b194` is
   old; the surrounding code may have moved). Use `git am vendor/patches/gpui-component/000N-*.patch`
   in a clean clone as the starting point, then rebase.
2. Keep the upstream PR self-contained: the API name (`TabPanel::set_active_panel`),
   a story/example exercising it if upstream has one for the component, and no
   OneTerm-specific wording. Reference this file for the motivation.
3. Until the PR is merged **and** OneTerm's pinned rev is bumped past it, keep the
   patch here unchanged; do not "pre-adopt" the upstream shape in the vendor tree.
4. When the pinned rev finally includes the change: delete the patch file, renumber
   the remaining ones (`refresh.sh` applies them in order), run
   `bash vendor/refresh.sh`, `python scripts/check-ui-fork.py --update`, and update
   the patch table in `vendor/README.md` §2 and the module list above. When no
   source patch is left, retire the whole fork as described at the top of this page
   (`0002`, the standalone manifest, only exists to make the snapshot buildable).

The check script does not access the ignored research mirror during normal CI. Its
`--update` mode performs a clean clone, verifies the pinned commit, compares the
complete package file set (`src/**`, `Cargo.toml`, `build.rs`, `locales/**`, licence),
and rejects deltas outside the four patched paths: `src/dock/tab_panel.rs` (`0001`),
`Cargo.toml` (`0002`), `src/setting/page.rs` and `src/setting/settings.rs` (`0003`).
`bash vendor/refresh.sh --check` (also run in CI) is the complementary guard: it
rebuilds every vendored crate from pristine + patches and diffs it against `vendor/`.
