# OneTerm gpui-component vendor snapshot

This directory is a source snapshot of the upstream `gpui-component` UI crate.
It is not an independent OneTerm workspace crate.

## Upstream provenance

- Repository: <https://github.com/longbridge/gpui-component>
- Commit: `ea6b194db04cc7c0474851f07c7d5b7a9df6a98b`
- Source path: `crates/ui`

The snapshot is produced from a clean clone checked out at the exact commit above.
Do not regenerate it from OneTerm's ignored `reference/` checkout.
The reviewable source patch is tracked at
`vendor/patches/gpui-component/0001-OneTerm-add-TabPanel-set_active_panel.patch`; it applies from the upstream clone
root after checking out the pinned revision.

## OneTerm patch surface

OneTerm currently maintains one reviewed source change:

- `src/dock/tab_panel.rs`: expose `TabPanel::set_active_panel` so Agent navigation can activate an existing panel.

The remaining source files must match the upstream commit. The vendored manifest is
made self-contained so Cargo can use this directory as a path patch without adding it
to the OneTerm workspace. The patch file contains only the source delta; the
self-contained manifest adjustment is a packaging step for the excluded vendor copy.

See `docs/agents/ui-fork-maintenance.md` and run
`python scripts/check-ui-fork.py` before committing changes to this directory.
