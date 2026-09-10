# Dependencies and GPUI Kit — OneTerm

> Current dependency versions, allowed auxiliary crates, upgrade procedure, and the mandatory reference-first research rule.

---

## 1. Locked dependency families

The workspace consumes the published GPUI Kit 0.6 release family from crates.io. The package aliases in the root manifest preserve the existing Rust import paths.

| Workspace name | Package | Requirement | Resolved version | Policy |
|---|---|---:|---:|---|
| `gpui` | `gpui-pre` | `0.3` | `0.3.3` | Move with `gpui_platform`. |
| `gpui_platform` | `gpui-pre-platform` | `0.3` | `0.3.3` | Move with `gpui`; app crate only. |
| `gpui-base` | `gpui-base` | `0.6` | `0.6.0` | Move with all GPUI Kit 0.6 layers. |
| `gpui-component` | `gpui-component` | `0.6` | `0.6.0` | Move with all GPUI Kit 0.6 layers. |
| `gpui-kit-assets` | `gpui-kit-assets` | `0.6` | `0.6.0` | Move with all GPUI Kit 0.6 layers; app crate only. |
| `alacritty_terminal` | Zed's `alacritty` fork | pinned rev | `0.26.1-dev` | Vendored at `vendor/alacritty_terminal`; see [`vendor/README.md`](../../vendor/README.md). |
| `vte` | crates.io | `0.15.0` | `0.15.0` | Vendored at `vendor/vte`; see [`vendor/README.md`](../../vendor/README.md). |

Rules:

1. `gpui-pre` and `gpui-pre-platform` move together.
2. `gpui-base`, `gpui-component`, and `gpui-kit-assets` move together.
3. Do not adopt the `gpui-kit` facade or add GPUI from git without a new decision record. OneTerm intentionally keeps `use gpui::…` and `use gpui_component::…` imports.
4. Do not add a `[patch]` for the UI layer. Fix compatibility application-side or upstream it.
5. Cargo profile overrides use package names (`gpui-pre`, `gpui-pre-platform`), not workspace aliases.
6. The terminal forks are never hand-edited. Every OneTerm delta lives in `vendor/patches/<crate>/`; `bash vendor/refresh.sh --check` proves each tree equals pristine source plus its patches.

The governing choice is [`DEC-0006`](../decisions/DEC-0006-depend-on-published-gpui-component-0-6.md).

## 2. Declaration in the workspace manifest

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

A UI crate normally declares only what it directly imports:

```toml
[dependencies]
gpui.workspace = true
gpui-component.workspace = true
```

Dock-owning crates additionally use `gpui-base`. Only `oneterm-app` directly uses `gpui_platform` and `gpui-kit-assets`: the platform crate owns window/event-loop integration, while the asset crate supplies GPUI Kit icons and fonts merged into `CustomAssets`.

## 3. Allowed auxiliary crates

Every third-party dependency is declared once in root `[workspace.dependencies]` and pulled into a crate with `name.workspace = true`. The principal groups are:

| Purpose | Crate(s) |
|---|---|
| SSH and SFTP | `russh` (features `ring`, `flate2`, `rsa`), `russh-sftp` |
| SSH runtime | `tokio`, `tokio-util`, `rand` |
| Local shell PTY | `alacritty_terminal::tty` + `polling` (do not use `portable-pty`) |
| Terminal parser / grid | vendored `alacritty_terminal`, which pulls vendored `vte` |
| Event channel | `async-channel` |
| Terminal helpers | `base64`, `aho-corasick`, `regex` |
| Serialization | `serde`, `serde_json` |
| Errors and logs | `anyhow`, `thiserror`, `log`, `env_logger` |
| Native crash capture | `crash-handler = 0.8.0` (app only) |
| Secrets | `zeroize` |
| Auto-update | `reqwest`, `semver`, `sha2`, `zip`, `tar`, `flate2` |
| UI helpers | `chrono`, `sysinfo`, `rust-embed` |
| Windows FFI | `windows-sys 0.59` with a workspace-wide feature union |
| Build / development | `embed-resource`; diagnostics also use `libc`, `polling`, and `alacritty_terminal`; `futures` (dev-only) feeds russh's in-process SSH agent server in `oneterm-ssh` tests |

Do not re-add without a design decision: `tracing` / `tracing-subscriber`, `directories`, `toml`, `russh-cryptovec`, `ssh-key`, `smol`, or `rust-i18n`.

Before adding a dependency, check whether the GPUI Kit release already provides the capability and inspect root `Cargo.toml`. If a new crate is still required, open an issue, add one workspace declaration, and update this table when it introduces a new dependency category.

## 4. Upgrading GPUI Kit

Treat a GPUI upgrade as one reviewed dependency change:

1. Read the target release notes and source in `reference/gpui-kit`.
2. Update both `gpui-pre` requirements together and all three GPUI Kit layer requirements together as applicable.
3. Refresh `Cargo.lock`; confirm a single intended version of each family with `cargo tree`.
4. Adapt OneTerm code without a local UI `[patch]`.
5. Update this version table, `deny.toml`, and generated `THIRD-PARTY-NOTICES.md` when the graph changes.
6. Replace `reference/gpui-kit` with a clean checkout at the exact released tag used for research.
7. Run `scripts/ci-local.sh --full` (or the PowerShell twin).

The vendored terminal engine follows the separate patch/rebase process in [`vendor/README.md`](../../vendor/README.md).

## 5. Reference-first research (mandatory)

> When researching GPUI or GPUI Kit APIs, patterns, themes, icons, skills, or examples, inspect `reference/gpui-kit` first. Use web search only for a known issue/PR or when the pinned reference does not contain the needed information, and state that gap.

The local tree is a clean checkout of `longbridge/gpui-kit` tag `v0.6.0`, matching the GPUI Kit layers resolved by `Cargo.lock`. Use `srcwalk` for code navigation before raw searches.

```bash
srcwalk overview --scope reference/gpui-kit/crates/component/src --symbols
srcwalk discover 'Panel,TabGroup,DockArea' --as symbol --scope reference/gpui-kit/crates
srcwalk show reference/gpui-kit/crates/base/src/dock/tab_group.rs:1-160
```

### Quick lookup table

| Need | Pinned reference path |
|---|---|
| Repository/API overview | `reference/gpui-kit/CLAUDE.md`, `reference/gpui-kit/README.md` |
| Component source | `reference/gpui-kit/crates/component/src/` |
| Dock behavior (`DockArea`, `TabGroup`, persisted state) | `reference/gpui-kit/crates/base/src/dock/` |
| Dock components and panel skin | `reference/gpui-kit/crates/component/src/dock/` |
| Inputs and text areas | `reference/gpui-kit/crates/component/src/input/` |
| Buttons, dialogs, forms, menus | `reference/gpui-kit/crates/component/src/button/`, `reference/gpui-kit/crates/component/src/dialog/`, `reference/gpui-kit/crates/component/src/form/`, `reference/gpui-kit/crates/component/src/menu/` |
| Icon names | `reference/gpui-kit/crates/component/src/icon.rs` |
| Theme schema and implementation | `reference/gpui-kit/.theme-schema.json`, `reference/gpui-kit/crates/component/src/theme/` |
| Hello-world example | `reference/gpui-kit/examples/hello_world/src/main.rs` |
| DockArea example | `reference/gpui-kit/examples/sidebar/src/main.rs`, `reference/gpui-kit/crates/story/examples/dock.rs` |
| WebView | `reference/gpui-kit/crates/webview/`, `reference/gpui-kit/examples/webview/` |
| Agent skills | `reference/gpui-kit/skills/` |
| Architecture and UI guidance | `reference/gpui-kit/docs/` |

To refresh the ignored research checkout, fetch and checkout the intended tag inside `reference/gpui-kit`; do not change the reference independently of the workspace dependency decision.
