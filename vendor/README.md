# `vendor/` — OneTerm forks of `vte` + `alacritty_terminal`

This directory holds **lightly-patched forks** of two upstream crates that the terminal
engine depends on. They are consumed by the root `Cargo.toml` via `[patch]` (and excluded
from the workspace), so Cargo builds *these* sources instead of the upstream ones.

The whole point of this directory's layout is to make the fork delta **trivial to review
and reproduce**: each vendored crate is `pristine upstream @ a pinned rev` **plus** a
small, human-readable patch set under [`patches/`](patches). You can read exactly what
OneTerm changed by reading the `.patch` files — nothing else in the tree is ours.

```
vendor/
├── README.md                     ← this file
├── refresh.sh                    ← regenerate <crate>/ from pristine + patches (bash)
├── patches/
│   ├── vte/
│   │   └── 0001-*.patch          ← OneTerm delta over pristine vte 0.15.0
│   └── alacritty_terminal/
│       ├── 0001-*.patch          ← standalone Cargo manifest
│       └── 0002-*.patch          ← Event::Osc / Event::ClearScreen single-pass hook
├── vte/                          ← pristine vte 0.15.0 + patches/vte/*   (built by Cargo)
└── alacritty_terminal/           ← pristine alacritty @ fcf32fe + patches/alacritty_terminal/*
```

---

## 1. Provenance — the pinned upstream revisions

| Vendored crate | Upstream source | Pinned rev | Notes |
|---|---|---|---|
| `vte` | crates.io, `vte 0.15.0` | published `0.15.0` (VCS sha1 `3b3da71c34cc1256c7e20981cf03f8eb95e08ffc`) | consumed via `[patch.crates-io]` |
| `alacritty_terminal` | `github.com/zed-industries/alacritty` (Zed's fork) | `fcf32feacb367b75ec84dd40f041e4fd411d3cc1` | Zed's patched build with `TerminalContent`/`display_iter`; consumed via `[patch."https://github.com/zed-industries/alacritty"]` |

These revs are also declared in the root `Cargo.toml` (`[workspace.dependencies]` +
`[patch]`) and in [`docs/agents/dependencies.md`](../docs/agents/dependencies.md) §1/§3.
**Keep all three in sync.** Bumping a rev is an intentional dependency decision — see §5.

> ⚠️ Vendoring the alacritty fork **intentionally breaks the upstream alacritty rev-lock**
> (we no longer track `zed-industries/alacritty` directly). This is orthogonal to the
> `gpui`/`gpui-component` rev-lock — `gpui` does not depend on `alacritty_terminal`.
> Rationale + design: [`docs/terminal-fullscreen-perf/09-patch-alacritty-fork.md`](../docs/terminal-fullscreen-perf/09-patch-alacritty-fork.md).

---

## 2. What the patches do (the fork delta)

All patches implement **R1 — the single-pass OSC/clear hook** (see doc 09): they let
OneTerm capture the OSCs and screen-clears alacritty's `Term` doesn't surface, *during the
one `Processor::advance` parse*, so the PTY pump no longer runs a **second** `vte::Parser`.

**`patches/vte/`**
- `0001` — adds `Handler::report_osc(params, bell_terminated)` (default no-op) and calls it
  from the `osc_dispatch` fallthrough, forwarding every OSC `vte` doesn't itself dispatch
  (OSC 7/9/133/…) to the embedder. *(touches only `src/ansi.rs`)*

**`patches/alacritty_terminal/`**
- `0001` — standalone `Cargo.toml`: inlines `edition`/`rust-version` from the upstream
  alacritty workspace (which isn't vendored) so the crate builds on its own under
  `[patch]`, and drops the workspace-relative `readme` path.
- `0002` — adds `Event::Osc { params, bell_terminated }` + `Event::ClearScreen`; `Term`
  forwards `report_osc` → `Event::Osc`, and emits `Event::ClearScreen` from `clear_screen`
  (`CSI 2J`/`3J`) and `reset_state` (RIS). *(touches `src/event.rs`, `src/term/mod.rs`)*

The patches are **git `format-patch`** files: each carries the commit message describing
the change, so `patches/` reads like a changelog of the fork.

---

## 3. Reviewing the delta

To see everything OneTerm changed relative to the original revs, just read the patch
files, or diff the vendored tree against a pristine checkout:

```bash
# quickest: read the patches
git -C .. diff --no-index -- /dev/null vendor/patches/vte/0001-*.patch   # or just open them

# or diff vendored vs pristine (needs the pristine in the Cargo cache; refresh.sh finds it)
diff -ru "$(ls -d ~/.cargo/registry/src/*/vte-0.15.0)"                vendor/vte
diff -ru ~/.cargo/git/checkouts/alacritty-*/fcf32fe*/alacritty_terminal vendor/alacritty_terminal
```

---

## 4. Regenerating the vendored source (`pristine → apply patch → vendored`)

[`refresh.sh`](refresh.sh) rebuilds each `vendor/<crate>/` from its pristine upstream rev
plus `patches/<crate>/*.patch`. It prefers the local Cargo cache (byte-exact) and falls
back to the network.

```bash
bash vendor/refresh.sh            # rebuild vendor/vte + vendor/alacritty_terminal
bash vendor/refresh.sh --check    # verify current vendor == pristine + patches (CI-friendly, no writes)
```

`--check` is the guard against drift: if someone hand-edits a vendored file without
updating a patch, `--check` fails and prints the diff.

---

## 5. Editing the patches (`create-from-rev → commit → patch → commit`)

To change the fork (add/modify a patch), reproduce the pristine crate in a throwaway git
repo, make the change as a commit, and export it back into `patches/`:

```bash
CRATE=vte                              # or alacritty_terminal
STAGE=$(mktemp -d)

# 1. materialize the pristine upstream rev, commit it as the baseline
cp -a "$(ls -d ~/.cargo/registry/src/*/vte-0.15.0)/." "$STAGE/"   # (alacritty: ~/.cargo/git/checkouts/alacritty-*/fcf32fe*/alacritty_terminal)
cd "$STAGE"; chmod -R u+w .
git init -q && git add -A && git -c core.autocrlf=false commit -qm "pristine: $CRATE @ <rev>"

# 2. (optional) re-apply the current patches so you edit on top of them
git -c core.autocrlf=false am /path/to/repo/vendor/patches/$CRATE/*.patch   # or: for p in ...; do git apply "$p"; done && commit

# 3. make your change, commit with a descriptive message
$EDITOR src/ansi.rs
git add -A && git -c core.autocrlf=false commit -qm "OneTerm fork: <what & why>"

# 4. export ALL OneTerm commits (everything after the pristine baseline) back into patches/
rm -f /path/to/repo/vendor/patches/$CRATE/*.patch
git -c core.autocrlf=false format-patch --zero-commit --no-signature <pristine-sha> \
    -o /path/to/repo/vendor/patches/$CRATE

# 5. materialize + verify
bash /path/to/repo/vendor/refresh.sh --check
```

> Use `core.autocrlf=false` when generating patches on Windows so the hunks stay LF-clean
> and match the pristine (LF) upstream sources. `refresh.sh` applies with `patch -p1`,
> which is what these patches are tested against (`git apply` is skipped by the parent
> repo's worktree, so prefer `patch`).

### Bumping an upstream rev

1. Update the rev in **three** places: this file (§1), the root `Cargo.toml`
   (`[workspace.dependencies]` + `[patch]`), and `docs/agents/dependencies.md` §1/§3.
2. Update the pinned values at the top of `refresh.sh` (`VTE_VERSION` / `ALA_REV`).
3. Re-do §5 against the new pristine rev (the patches may need a rebase if upstream moved
   the surrounding code), then `refresh.sh --check` + `cargo build --workspace`.
4. Record the bump per the dependency-change process in `docs/agents/dependencies.md` §3.

---

## 6. How Cargo wires these in

Root `Cargo.toml`:

```toml
[workspace]
exclude = ["vendor/vte", "vendor/alacritty_terminal"]   # not workspace members

[patch."https://github.com/zed-industries/alacritty"]
alacritty_terminal = { path = "vendor/alacritty_terminal" }

[patch.crates-io]
vte = { path = "vendor/vte" }
```

So any dependency edge that resolves to upstream `vte`/`alacritty_terminal` is redirected
to these vendored, patched paths.
