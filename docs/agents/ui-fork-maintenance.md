# `oneterm-ui` fork maintenance

`crates/ui` is a deliberately small local fork of the `gpui-component` UI crate. It
exists because OneTerm's dock implementation needs `pub(crate)` access across the
`dock`, `resizable`, `tab`, and `history` modules; those items cannot be reached
when the modules are imported from a separate crate.

## Upstream base

The authoritative upstream base is the `gpui-component` revision in
`Cargo.toml` (`ea6b194db04cc7c0474851f07c7d5b7a9df6a98b`). The local
`reference/gpui-component` checkout is a research mirror and must be checked out
at that revision before making a comparison. Do not silently compare against a
moving branch or a different release.

## Delta surface

Keep the fork limited to these upstream modules:

- `crates/ui/src/dock/`
- `crates/ui/src/resizable/`
- `crates/ui/src/tab/`
- `crates/ui/src/history.rs`

`crates/ui/src/lib.rs` is the OneTerm crate wrapper. It registers the local
translation catalog and exposes the four module groups. New UI components belong
in upstream `gpui-component` or in a feature crate, not in this fork.

## Synchronization procedure

1. Check the pinned `gpui-component` revision in `Cargo.toml`.
2. In the local reference checkout, run `git fetch` and `git checkout` for that
   exact revision.
3. Run `python scripts/check-ui-fork.py` from the workspace root.
4. Review upstream changes in the four delta directories and copy only the
   required changes into `crates/ui/src/`.
5. Reapply and document any OneTerm-specific patch in the commit body and in the
   fork's module-level comments.
6. Run `cargo fmt --all`, the workspace clippy/build/test gates, and the dock
   integration tests before updating the pinned revision.

The check script exits successfully when the ignored reference checkout is not
present, so normal CI does not require the large research mirror. When the
reference is available, it fails on missing or differing fork module files and
prints the revision being compared.
