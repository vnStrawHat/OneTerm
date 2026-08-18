# Dependencies & gpui-component — OneTerm

> File split from `AGENTS.md` (sections 2, 7, 11). Contains information about rev lock, allowed dependencies, how to integrate upstream, and the **reference-first research** rule.

---

## 1. Locked revs (fixed — only change on an intentional upgrade)

The workspace is pinned to the exact rev set verified as compatible by `reference/gpui-component` (upstream):

| Crate | Source | Rev | Resolved version |
|---|---|---|---|
| `gpui` | `https://github.com/zed-industries/zed` | `1d217ee39d381ac101b7cf49d3d22451ac1093fe` | `0.2.2` |
| `gpui_platform` | `https://github.com/zed-industries/zed` | `1d217ee39d381ac101b7cf49d3d22451ac1093fe` | (same rev — monorepo) |
| `gpui-component` | `https://github.com/longbridge/gpui-component` | `ea6b194db04cc7c0474851f07c7d5b7a9df6a98b` | `0.5.2` (not tagged yet, currently at HEAD between `v0.5.1` → `v0.5.2`) |
| `gpui-component-assets` | `https://github.com/longbridge/gpui-component` | `ea6b194db04cc7c0474851f07c7d5b7a9df6a98b` | (same repo/rev as `gpui-component` — built-in icon/font assets) |
| `alacritty_terminal` | `https://github.com/zed-industries/alacritty` (Zed fork) | `fcf32feacb367b75ec84dd40f041e4fd411d3cc1` | `0.26.1-dev` — **vendored** at `vendor/alacritty_terminal`, see [`vendor/README.md`](../../vendor/README.md) |
| `vte` | crates.io | `0.15.0` (VCS `3b3da71c34cc1256c7e20981cf03f8eb95e08ffc`) | `0.15.0` — **vendored** at `vendor/vte`, see [`vendor/README.md`](../../vendor/README.md) |

Three of these (`gpui-component`, `alacritty_terminal`, `vte`) are **vendored** under
`vendor/<crate>/` and reached through the root `Cargo.toml` `[patch]` section: the
upstream sources stay declared so `[patch]` can redirect every edge, and — deliberately —
so the vendored trees remain outside the scope of workspace-wide tools (`cargo fmt --all`
formats *path* dependencies but not patched sources; see the comment above
`alacritty_terminal` in `Cargo.toml`). **Policy:** the rev-lock above is the *pristine base rev*; every
OneTerm delta lives **exclusively** in `vendor/patches/<crate>/*.patch` (never hand-edit
`vendor/<crate>/`). `bash vendor/refresh.sh --check` (CI) proves
`vendor/<crate> == pristine @ rev + patches`; `python scripts/check-ui-fork.py`
additionally hash-pins the gpui-component package.

> 📌 **Inviolable rules**:
>
> 1. `gpui` and `gpui_platform` **must share the same rev** (same `zed-industries/zed` monorepo).
> 2. `gpui-component` and `gpui-component-assets` **must share the same rev** (same `longbridge/gpui-component` repo).
> 3. Do not add `gpui` from crates.io or any other git source. If you need a feature beyond the 4 crates above → patch upstream or fork locally; do not swap dependencies on a whim.
> 4. When upstream tags `gpui-component` `v0.5.2`, consider switching the rev → tag for long-term stability.
> 5. Bumping a vendored crate's rev is a three-place change (root `Cargo.toml`, `vendor/README.md` §1, this table) plus a patch rebase — follow `vendor/README.md` §5.

## 2. Declaration in the workspace `Cargo.toml`

```toml
[workspace.dependencies]
# GPUI core — same rev, same monorepo
gpui = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe" }
gpui_platform = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe", features = ["font-kit", "x11", "wayland", "runtime_shaders"] }
gpui-component = { git = "https://github.com/longbridge/gpui-component", rev = "ea6b194db04cc7c0474851f07c7d5b7a9df6a98b" }
gpui-component-assets = { git = "https://github.com/longbridge/gpui-component", rev = "ea6b194db04cc7c0474851f07c7d5b7a9df6a98b" }
```

In each sub-crate that renders UI (e.g. `crates/terminal-view/Cargo.toml`):

```toml
[dependencies]
gpui.workspace = true
gpui-component.workspace = true
```

Only the binary crate (`crates/app/Cargo.toml`) additionally depends on `gpui_platform`
(the platform back-end that owns the event loop / window creation) and on
`gpui-component-assets` (built-in icons + fonts, merged into `CustomAssets`). Library
crates never need either — they build against `gpui` + `gpui-component` only.

> ⚠️ **Exact crate name**: in Cargo, the crate name is `gpui_platform` (with an underscore) — not `gpui-platform`. The `use gpui_platform::...` declaration also uses the underscore. See `reference/gpui-component/examples/hello_world/Cargo.toml` for reference (`gpui_platform = { workspace = true }`).

## 3. Allowed auxiliary crates

Every third-party dependency is declared **once** in the root `Cargo.toml`
`[workspace.dependencies]` and pulled into a crate with `name.workspace = true`
(no inline versions in `crates/*/Cargo.toml`). The set currently in use:

| Purpose | Crate(s) |
|---|---|
| SSH protocol + SFTP subsystem | `russh` (features `ring`, `flate2`, `rsa`), `russh-sftp` |
| SSH runtime (hidden inside `ssh`) | `tokio` (`rt`, `rt-multi-thread`, `sync`, `io-util`, `net`, `macros`, `fs`), `tokio-util`, `rand` |
| Local shell PTY | `alacritty_terminal::tty` + `polling` (do NOT use `portable-pty` — design decision, see [`docs/terminal-backend.md`](../terminal-backend.md)) |
| Terminal parser / grid | `alacritty_terminal` (vendored Zed fork, §1) — which pulls the vendored `vte` |
| Event channel (`SessionEvent`) | `async-channel` (keeps Tokio out of the public API) |
| Terminal helpers | `base64` (OSC 52), `aho-corasick` + `regex` (highlight engine), `itertools` (terminal-view) |
| Serialization | `serde` (`derive`), `serde_json` |
| Error | `anyhow` (binary / UI glue), `thiserror` (library error types) |
| Logging | `log` (also used by alacritty_terminal), `env_logger` (app binary) |
| Native crash capture | `crash-handler = 0.8.0` (app binary only; callback must remain compromised-context-safe) |
| Secrets | `zeroize` (`derive`) |
| Auto-update | `reqwest` (blocking, rustls, system proxy), `semver`, `sha2`, `zip`, `tar`, `flate2` |
| UI helpers | `chrono` (clock widget / timestamps), `sysinfo` (CPU/memory widget), `rust-embed` (theme + icon assets) |
| Windows FFI | `windows-sys 0.59` — one workspace entry, feature union of every first-party use |
| Build / dev only | `embed-resource` (app `.rc`) |
| Diagnostics (`crates/tools`, never shipped) | `libc` (DOOM-fire terminal size on Unix), `windows-sys` (console mode), `alacritty_terminal` + `polling` (raw PTY throughput probe) |

Not used (do not re-add without a design decision): `tracing`/`tracing-subscriber`
(the workspace logs through `log`), `directories` (`oneterm_core::config_dir` owns
paths), `toml`, `russh-cryptovec`, `ssh-key`, `smol` (gpui's executor is reached
through `cx.background_executor()`), `rust-i18n`.

Before adding a new crate, ask: "is this crate already in `reference/gpui-component/Cargo.toml`?" If yes → use the locked rev. If not and it is a new crate → open an issue before adding it, then add it to `[workspace.dependencies]` **and** this table.

---

## 4. Integrating with gpui-component upstream

This project uses Cargo's `[patch]` mechanism to replace the upstream
`gpui-component` package with `vendor/gpui-component`. The vendor snapshot is
created from a clean clone at the exact pinned revision because the dock needs
`pub(crate)` access across several upstream sibling modules. Follow the complete
base-revision, delta-review, and baseline-update procedure in
[`ui-fork-maintenance.md`](ui-fork-maintenance.md). Verify the vendor delta with:

```bash
python scripts/check-ui-fork.py
```

For changes to the upstream dependency itself:

1. Read the release note / PR diff in `reference/gpui-component/`.
2. Clone the upstream repository and check out the exact revision pinned in
   `Cargo.toml` before comparing source.
3. Update the corresponding code in the UI crates or the vendor patch.
4. If the change is breaking, update `CHANGELOG.md` (if any) and the current architecture docs.
5. Run `python scripts/check-ui-fork.py --update` only after reviewing every changed vendor file.

### Quick reference for important gpui-component entry points

- `vendor/gpui-component/src/dock/` — `DockArea`, `Panel`, `StackPanel`, `TabPanel`.
- `vendor/gpui-component/src/input/` — `InputState`, `Input`.
- `vendor/gpui-component/src/dialog/` — `Dialog` overlay.
- `vendor/gpui-component/src/notification.rs` — toast/notification.
- `vendor/gpui-component/src/sheet.rs` — side panel.
- `vendor/gpui-component/src/theme/` — `Theme`, `ActiveTheme`, `ThemeColor`.

---

## 5. Reference-first research (IMPORTANT)

> 🚨 **HARD CONSTRAINT**: When you need information related to `gpui` / `gpui-component` (API, patterns, code examples, docs, themes, icons, skills, changelogs), **the agent MUST read from `.\reference\gpui-component\` first**. **Do not** use `web_search` / `fetch_content` / `code_search` to look up gpui-component information unless you have already read the reference and it is still missing what you need.

### 5.1. Why

1. **Version match**: `reference/gpui-component` is pinned exactly at rev `ea6b194d...` (matches the project's `Cargo.lock`). Web search may return docs/code of an older / newer version → hard-to-debug compile errors.
2. **Complete resources**: the reference contains `CLAUDE.md` (agent guide), `crates/ui/src/` (full source), `examples/` (11 runnable examples), `skills/` (knowledge base), `docs/` (en + zh-CN), `.theme-schema.json`, icons, …
3. **Faster**: reading a local file needs no network, no HTML parsing, and you can `grep` precisely.
4. **Avoid hallucination**: web search returns snippets that may have wrong method names / signatures; reading the real source is absolutely accurate.

### 5.2. Lookup tools inside the reference

```bash
# Find files / modules related to something
find reference/gpui-component -name "*.rs" | xargs grep -l "DockArea"
find reference/gpui-component -name "*.rs" -path "*dock/*"

# Find a struct / trait / method
grep -rn "pub trait Panel" reference/gpui-component/crates/ui/src/
grep -rn "fn on_click" reference/gpui-component/crates/ui/src/button/

# Read the source file
read reference/gpui-component/crates/ui/src/dock/dock.rs
read reference/gpui-component/CLAUDE.md

# Look at the story example for a specific component
ls reference/gpui-component/crates/story/src/
grep -rn "Button::new" reference/gpui-component/examples/
```

**Tip**: use `read` + `grep` + `find` with **relative paths from project directory** (e.g. `reference/gpui-component/...`).

### 5.3. Quick lookup table inside the reference

| What you need to know | Specific file in the reference |
|---|---|
| API overview, init pattern | `reference/gpui-component/CLAUDE.md` |
| Component list & API | `reference/gpui-component/crates/ui/src/` (split by file: `button.rs`, `input/`, `dialog/`, `dock/`, …) |
| Icon names | `reference/gpui-component/crates/ui/src/icon.rs` |
| Theme schema & color tokens | `reference/gpui-component/.theme-schema.json` + `crates/ui/src/theme.rs` |
| Dock / Panel / Tab system | `reference/gpui-component/crates/ui/src/dock/` |
| Input / TextField | `reference/gpui-component/crates/ui/src/input/` |
| Form | `reference/gpui-component/crates/ui/src/form/` |
| Chart | `reference/gpui-component/crates/ui/src/chart/` |
| WebView | `reference/gpui-component/crates/webview/` + `examples/webview/` |
| Hello world example | `reference/gpui-component/examples/hello_world/src/main.rs` |
| DockArea example | `reference/gpui-component/examples/sidebar/src/main.rs` |
| Agent skills (gpui, gpui-component) | `reference/gpui-component/skills/` |
| Documentation (en) | `reference/gpui-component/docs/docs/` |
| Documentation (zh-CN) | `reference/gpui-component/docs/zh-CN/docs/` |
| Story gallery source | `reference/gpui-component/crates/story/src/` |

### 5.4. When you ARE allowed to use web search

Only use `web_search` / `fetch_content` / `code_search` for gpui-component when:

- **Looking up a specific GitHub issue / PR** (e.g. you know #2484 → search to read the full thread).
- **Looking up docs for a different Rust crate** (e.g. `russh`, `alacritty_terminal`, `tokio`) — not part of gpui-component.
- **The reference is missing information** (rare, since the reference is a complete mirror).

When you use web search for gpui-component, **always state clearly in your response** why you could not look it up in the reference.

### 5.5. Updating the reference

If you need a newer reference (e.g. upstream released a new tag):

```bash
# Inside .\reference\gpui-component\
git fetch origin
git checkout <tag-or-rev>
```

Then update the rev in the workspace `Cargo.toml` (section 1) to match, and re-run `cargo build` to refresh `Cargo.lock`.

> ⚠️ The `git fetch` / `git checkout` commands on the reference only run inside the `.\reference\gpui-component\` directory — still inside the workspace, so this does not violate the "do not cd outside the project" constraint.
