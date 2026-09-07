# Low-Level Design: Workspace dependency shape and package aliasing

Intake: IN-0017
HLD: ../high-level-design.md
Topic: dependency-aliasing
Date: 2026-09-07

## Concern

How the four current GPUI/UI dependencies are re-declared against crates.io so that all 107
files importing `gpui::` / `gpui_component::` compile unchanged, and which features are selected.

## Design

### Target declaration

Upstream publishes GPUI under the `gpui-pre-*` names (a crates.io republish of Zed's GPUI by the
gpui-kit maintainer) and the UI layers under `gpui-base` / `gpui-component` / `gpui-kit-assets`.
GPUI Kit's own workspace aliases them back to the familiar names, and OneTerm does the same, so
no import statement changes:

```toml
[workspace.dependencies]
# GPUI core — published as the `gpui-pre-*` family (see docs/agents/dependencies.md §1)
gpui = { package = "gpui-pre", version = "0.3" }
gpui_platform = { package = "gpui-pre-platform", version = "0.3", features = [
    "font-kit", "x11", "wayland", "runtime_shaders",
] }
# GPUI Kit — the styled layer plus the unstyled foundation the dock traits live in
gpui-base = "0.6"
gpui-component = "0.6"
gpui-kit-assets = "0.6"
```

`gpui-component-assets` is renamed to `gpui-kit-assets` upstream (Rust path `gpui_kit_assets`),
so `crates/app/src/assets.rs` and `crates/app/Cargo.toml` change; that is the only assets site.

### Why aliasing rather than the `gpui-kit` facade

`gpui-kit` 0.6.0 is the recommended single dependency for new applications: it re-exports GPUI
plus every layer and provides its own `application()`, `init()` and `actions!`. Adopting it would
mean rewriting every import to `gpui_kit::*` / `gpui_kit::component::*` and switching the
`actions!` macro across all 107 files, for no functional gain. Aliasing gives the same version
coupling with zero import churn. Recorded as a decision so future work does not re-litigate it.

### Why `gpui-base` is a direct dependency

The dock trait split puts behavior in `gpui_base::dock::Panel` and presentation in
`gpui_component::dock::Panel`. `gpui_component::dock` re-exports base's trait as `BasePanel`
(because its own `Panel` is the presentation half that extends it), so a panel crate *can* write
`impl gpui_component::dock::BasePanel for X`. Both spellings compile; the plan uses the explicit
`gpui-base` dependency in the five panel crates plus `workspace` and `state`, because
`impl gpui_base::dock::Panel` matches upstream's own examples and reads unambiguously next to
the presentation impl. Crates that never touch the dock (`actions`, `settings`, `theme`,
`settings-ui`) do not gain the dependency.

### Rev-lock rules that change

`docs/agents/dependencies.md` §1 currently pins four git revs and states two inviolable
same-rev rules. Those rules are replaced, not deleted:

| Old rule | New rule |
| --- | --- |
| `gpui` and `gpui_platform` must share the same zed rev | `gpui-pre` and `gpui-pre-platform` must share the same version |
| `gpui-component` and `gpui-component-assets` must share the same rev | `gpui-base`, `gpui-component`, `gpui-kit-assets` must share the same version (upstream releases them together) |
| Do not add `gpui` from crates.io | Do not add `gpui` from git; the crates.io `gpui-pre` family is now the only source |

`alacritty_terminal` and `vte` stay vendored and untouched by this migration — the
`[patch."https://github.com/zed-industries/alacritty"]` and `[patch.crates-io] vte` entries are
unchanged. Only the `[patch."https://github.com/longbridge/gpui-component"]` entry is removed
(see `05-vendor-retirement.md`).

### Features

Current vendored manifest builds with default features and no `tree-sitter-*` grammars, because
OneTerm does not use the markdown renderer or code editor. v0.6.0 adds a `tree-sitter` feature
gate above the per-language features, so the default set is smaller than before, not larger.
Keep defaults; add nothing. Verify with `cargo tree -p gpui-component -e features` that no
grammar crate is pulled in.

### Profile overrides

`[profile.dev.package]` / `[profile.fast-dev.package]` / `[profile.release.package]` name `gpui`
and `gpui_platform`. Cargo profile overrides key on the **package name**, not the dependency
alias, so these must become `gpui-pre` and `gpui-pre-platform` or they silently stop applying —
which would leave GPUI at `opt-level = 0` in dev and make full-screen TUIs unusable. This is a
real, easy-to-miss regression; it is verified by checking that a `fast-dev` build still runs
DOOM-fire at an acceptable frame rate.

## Interfaces

```toml
# crates/{agent-ui,app,session-ui,sftp-ui,terminal-view,workspace,state}/Cargo.toml
gpui.workspace = true
gpui-base.workspace = true        # new — dock trait behavior half
gpui-component.workspace = true

# crates/app/Cargo.toml only
gpui_platform.workspace = true
gpui-kit-assets.workspace = true  # was gpui-component-assets
```

```rust
// crates/app/src/assets.rs
- use gpui_component_assets::Assets;
+ use gpui_kit_assets::Assets;
```

## Edge Cases and Failure Modes

- [ ] Profile overrides silently stop matching after the package rename → dev/fast-dev builds
      lose GPUI optimization. Proof: `cargo build --profile fast-dev` timing plus a DOOM-fire run.
- [ ] A transitive dependency appears twice (e.g. `windows`, `tree-sitter`, `chrono`) because
      gpui-kit pins different versions than OneTerm's workspace entries. Proof: `cargo tree -d`.
- [ ] `deny.toml` bans/licence lists still name the old crate names or git sources.
      Proof: `cargo deny check licenses bans advisories`.
- [ ] `scripts/verify-dependency-graph.py` encodes the old dependency names and fails.
- [ ] `scripts/third-party-notices.py --check` fails until `THIRD-PARTY-NOTICES.md` is regenerated.

## Verification

- [ ] `cargo tree -d` shows no new duplicate major versions attributable to this change.
- [ ] `cargo tree -p gpui-component -e features` shows no `tree-sitter-*` grammar crates.
- [ ] `cargo deny check licenses bans advisories` passes.
- [ ] `python scripts/verify-dependency-graph.py` passes.
- [ ] `python scripts/third-party-notices.py --check` passes after regeneration.
- [ ] A `fast-dev` build still renders DOOM-fire without the window going unresponsive.
