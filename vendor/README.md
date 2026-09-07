# `vendor/` — OneTerm terminal-engine forks

This directory contains lightly patched forks of `vte` and `alacritty_terminal`. Root Cargo `[patch]` entries redirect all matching dependency edges to these paths, which remain outside the workspace.

Each vendored tree must equal pristine upstream at its pinned revision plus the ordered, reviewable patches in `vendor/patches/<crate>/`. Never hand-edit a vendored source tree.

```text
vendor/
├── README.md
├── refresh.sh
├── patches/
│   ├── vte/0001-*.patch
│   └── alacritty_terminal/{0001,0002}-*.patch
├── vte/
└── alacritty_terminal/
```

## 1. Provenance

| Vendored crate | Upstream | Pinned base | Cargo redirection |
|---|---|---|---|
| `vte` | crates.io | `0.15.0` (VCS `3b3da71c34cc1256c7e20981cf03f8eb95e08ffc`) | `[patch.crates-io]` |
| `alacritty_terminal` | `github.com/zed-industries/alacritty` | `fcf32feacb367b75ec84dd40f041e4fd411d3cc1` | `[patch."https://github.com/zed-industries/alacritty"]` |

Keep these values synchronized with root `Cargo.toml`, `vendor/refresh.sh`, and [`docs/agents/dependencies.md`](../docs/agents/dependencies.md).

## 2. OneTerm deltas

The patches implement the single-pass OSC/clear hook described in [`docs/terminal-fullscreen-perf/09-patch-alacritty-fork.md`](../docs/terminal-fullscreen-perf/09-patch-alacritty-fork.md):

- `patches/vte/0001` adds `Handler::report_osc(params, bell_terminated)` and forwards otherwise-unhandled OSC sequences to the embedder.
- `patches/alacritty_terminal/0001` makes the crate manifest standalone outside the upstream workspace.
- `patches/alacritty_terminal/0002` adds `Event::Osc` and `Event::ClearScreen`, forwarding parser and terminal events without a second parsing pass.

Patch files are the only OneTerm-owned source delta and should read as the fork changelog.

## 3. Regenerate or verify

`refresh.sh` materializes pristine sources (preferring Cargo caches), applies patches in filename order, prunes documented dead weight, and either replaces or compares the vendor trees.

```bash
bash vendor/refresh.sh            # regenerate both trees
bash vendor/refresh.sh --check    # compare only; CI uses this
```

`--check` must verify both crates. A mismatch means the tree was hand-edited, a patch no longer applies, or a pinned input drifted.

The script prunes registry metadata from `vte` and upstream test recordings from `alacritty_terminal`; these paths are unused by Cargo path dependencies and would add substantial dead weight.

## 4. Editing and rebasing patches

Create changes in a temporary git repository initialized from the pristine pinned source, not in `vendor/<crate>`:

1. Materialize the pristine crate and commit it as the baseline with `core.autocrlf=false`.
2. Apply the existing patch series.
3. Make and commit the OneTerm change.
4. Export every commit after the pristine baseline with `git format-patch --zero-commit --no-signature` into `vendor/patches/<crate>/`.
5. Run `bash vendor/refresh.sh --check`, then the full workspace quality gate.

When bumping an upstream base, update root `Cargo.toml`, this provenance table, `docs/agents/dependencies.md`, and the constants in `refresh.sh`; rebase the patch series and review the complete resulting diff.

## 5. Cargo wiring

```toml
[workspace]
exclude = ["vendor/vte", "vendor/alacritty_terminal"]

[patch."https://github.com/zed-industries/alacritty"]
alacritty_terminal = { path = "vendor/alacritty_terminal" }

[patch.crates-io]
vte = { path = "vendor/vte" }
```

The UI stack is not vendored. OneTerm consumes the published GPUI Kit release family directly, as documented in `docs/agents/dependencies.md` and DEC-0006.
