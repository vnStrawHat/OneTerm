# Vendored `gpui-component` maintenance

`vendor/gpui-component` is a source snapshot of the upstream `gpui-component`
`crates/ui` package. It is used through Cargo's `[patch]` mechanism and is not a
OneTerm workspace member or an `oneterm-ui` crate. The snapshot exists because
OneTerm's dock implementation needs `pub(crate)` access across the `dock`,
`resizable`, `tab`, and `history` modules; those items cannot be reached when the
modules are imported from a separate crate.

## Upstream base

The authoritative upstream base is the `gpui-component` revision in `Cargo.toml`:
`ea6b194db04cc7c0474851f07c7d5b7a9df6a98b`. The vendor snapshot is regenerated from
a clean clone of `https://github.com/longbridge/gpui-component` checked out at that
exact revision. The ignored `reference/gpui-component` checkout is for API research
only and is not the source used to generate the vendor snapshot.

`vendor/README.md` records the provenance, pinned revisions, and patch workflow.

## Delta surface

The current OneTerm patch is intentionally limited to one source file:

- `vendor/gpui-component/src/dock/tab_panel.rs` — exposes
  `TabPanel::set_active_panel` for Agent navigation.

All other files under `vendor/gpui-component/src/` must match the pinned upstream
revision. New UI components belong in upstream `gpui-component` or in a feature
crate, not in the vendor patch.

## Synchronization procedure

1. Check the pinned `gpui-component` revision in `Cargo.toml`.
2. Clone `https://github.com/longbridge/gpui-component` into a temporary directory,
   check out that exact revision, and verify its `HEAD` before copying `crates/ui`.
3. Apply both patches under `vendor/patches/gpui-component/` from the clone root.
   Verify that `0001` changes only `crates/ui/src/dock/tab_panel.rs` and that `0002`
   only makes `crates/ui/Cargo.toml` standalone.
4. Copy the patched `crates/ui` package into `vendor/gpui-component`.
5. Keep the vendor package excluded from the OneTerm workspace.
6. Run `python scripts/check-ui-fork.py --update` to review the complete source
   snapshot and refresh `scripts/ui-fork-baseline.json`.
7. Run `python scripts/check-ui-fork.py`, then the workspace format, clippy, build,
   and test gates before committing.

The check script does not access the ignored research mirror during normal CI. Its
`--update` mode performs a clean clone, verifies the pinned commit, compares the
complete source file set, and rejects deltas outside `dock/tab_panel.rs`.
