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

> 📌 **Inviolable rules**:
>
> 1. `gpui` and `gpui_platform` **must share the same rev** (same `zed-industries/zed` monorepo).
> 2. Do not add `gpui` from crates.io or any other git source. If you need a feature beyond the 3 crates above → patch upstream or fork locally; do not swap dependencies on a whim.
> 3. When upstream tags `gpui-component` `v0.5.2`, consider switching the rev → tag for long-term stability.

## 2. Declaration in the workspace `Cargo.toml`

```toml
[workspace.dependencies]
# GPUI core — same rev, same monorepo
gpui = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe" }
gpui_platform = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe", features = ["font-kit"] }
gpui-component = { git = "https://github.com/longbridge/gpui-component", rev = "ea6b194db04cc7c0474851f07c7d5b7a9df6a98b" }
```

In each sub-crate (e.g. `crates/ui/Cargo.toml`):

```toml
[dependencies]
gpui.workspace = true
gpui_platform.workspace = true
gpui-component.workspace = true
```

> ⚠️ **Exact crate name**: in Cargo, the crate name is `gpui_platform` (with an underscore) — not `gpui-platform`. The `use gpui_platform::...` declaration also uses the underscore. See `reference/gpui-component/examples/hello_world/Cargo.toml` for reference (`gpui_platform = { workspace = true }`).

## 3. Allowed auxiliary crates

| Purpose | Recommended crate |
|---|---|
| SSH protocol | `russh` + `russh-sftp` |
| Local shell PTY | `alacritty_terminal::tty` (do NOT use `portable-pty` — design decision, see [`docs/terminal-backend.md`](../terminal-backend.md)) |
| Terminal parser / grid | `alacritty_terminal` (fork `zed-industries/alacritty` @ rev `fcf32feacb367b75ec84dd40f041e4fd411d3cc1` — patched build with `TerminalContent`/`display_iter`, see the workspace `Cargo.toml`) |
| Async runtime (re-export) | `smol` / `futures` (already available in gpui) |
| Serialization | `serde`, `serde_json`, `toml` |
| Storage (host list, settings) | `directories` (XDG / AppData) |
| Logging | `tracing` + `tracing-subscriber` |
| Error | `anyhow` (binary), `thiserror` (library) |
| Extra crypto | `russh-cryptovec`, `ssh-key` |
| i18n | `rust-i18n` (matches gpui-component) |

Before adding a new crate, ask: "is this crate already in `reference/gpui-component/Cargo.toml`?" If yes → use the locked rev. If not and it is a new crate → open an issue before adding it.

---

## 4. Integrating with gpui-component upstream

This project uses gpui-component directly from git. When upstream changes its API:

1. Read the release note / PR diff in `reference/gpui-component/`.
2. Update the corresponding code in `crates/ui/`.
3. If the change is breaking → update `CHANGELOG.md` (if any) + `docs/architecture.md`.

### Quick reference for important gpui-component entry points

- `crates/ui/src/dock/` — `DockArea`, `Panel`, `StackPanel`, `TabPanel`.
- `crates/ui/src/input/` — `InputState`, `Input`.
- `crates/ui/src/dialog/` — `Dialog` overlay.
- `crates/ui/src/notification/` — toast/notification.
- `crates/ui/src/sheet/` — side panel.
- `crates/ui/src/theme.rs` — `Theme`, `ActiveTheme`, `ThemeColor`.

---

## 5. Reference-first research (IMPORTANT)

> 🚨 **HARD CONSTRAINT**: When you need information related to `gpui` / `gpui-component` (API, patterns, code examples, docs, themes, icons, skills, changelogs), **the agent MUST read from `D:\TrungKFC-Research\Rust\myTerm2\reference\gpui-component\` first**. **Do not** use `web_search` / `fetch_content` / `code_search` to look up gpui-component information unless you have already read the reference and it is still missing what you need.

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

**Tip**: use `read` + `grep` + `find` with **relative paths from `D:\TrungKFC-Research\Rust\myTerm2`** (e.g. `reference/gpui-component/...`).

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
# Inside D:\TrungKFC-Research\Rust\myTerm2\reference\gpui-component\
git fetch origin
git checkout <tag-or-rev>
```

Then update the rev in the workspace `Cargo.toml` (section 1) to match, and re-run `cargo build` to refresh `Cargo.lock`.

> ⚠️ The `git fetch` / `git checkout` commands on the reference only run inside the `D:\TrungKFC-Research\Rust\myTerm2\reference\gpui-component\` directory — still inside the workspace, so this does not violate the "do not cd outside the project" constraint.