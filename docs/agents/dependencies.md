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

There is **no terminal-engine dependency**. OneTerm's VT engine is `oneterm-vt`
(`crates/vt`, `IN-0029`), first-party code with no third-party engine behind it; the
vendored `alacritty_terminal` / `vte` fork it replaced was deleted at `US-0087`, and with
it the `[patch]` section and the CI job that proved the vendored trees were pristine
upstream plus patches. Do not re-add either.

Rules:

1. `gpui-pre` and `gpui-pre-platform` move together.
2. `gpui-base`, `gpui-component`, and `gpui-kit-assets` move together.
3. Do not adopt the `gpui-kit` facade or add GPUI from git without a new decision record. OneTerm intentionally keeps `use gpui::…` and `use gpui_component::…` imports.
4. Do not add a `[patch]` for the UI layer. Fix compatibility application-side or upstream it.
5. Cargo profile overrides use package names (`gpui-pre`, `gpui-pre-platform`), not workspace aliases.
6. There is no `[patch]` section at all any more. A capability the terminal engine lacks is added to `crates/vt` under ordinary review, not to a forked dependency ([`DEC-0014`](../decisions/DEC-0014-oneterm-owns-its-vt-engine.md)).

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
| SSH and SFTP | `russh 0.63.x` (`default-features = false`, features `ring`, `flate2`, `rsa`), `russh-sftp 3.0.x`. They move together as one family (`IN-0036`): `russh-cryptovec` and `russh-util` follow transitively and are never declared. `russh 0.63` is what takes the crypto stack off RustCrypto release candidates (16 `-rc` crates down to 3); `ssh-key`, `rsa` and `pkcs1` are the three that stay on one, because `russh 0.63.3` pins all three with `=` requirements — they move only when russh does. The SFTP client is constructed with `session::sftp_config()`, not russh-sftp's defaults: 3.0 cut the in-flight write budget 4x and added a read-ahead OneTerm's striped download discards. |
| SSH runtime | `tokio`, `tokio-util`, `rand` |
| Local shell PTY | `oneterm-pty` (OneTerm's own crate) over `polling` + `windows-sys` / `libc` (do not use `portable-pty`) |
| Terminal parser / grid | `oneterm-vt` (OneTerm's own crate) — see the row below; there is no third-party terminal engine |
| OneTerm's own VT engine (`oneterm-vt`, IN-0029) | `memchr 2.x` (the parser's ground-state scan for the next escape), `bitflags 2.x` (cell and row attribute flags), `rustc-hash 2.x` (`FxHashMap` for the interners), `unicode-width 0.2.x` (scalar width), `unicode-segmentation 1.x` (grapheme clusters); dev-only: `proptest 1.x`. The parser's `vte 0.15` differential oracle retired with the fork at `US-0087`; what pins the state machine now is `parser::props::arbitrary_bytes_never_panic_and_chunking_is_invariant`, the parser unit suite, and the 46 frozen corpus recordings. All already resolved in `Cargo.lock`, so the graph does not grow; `bitflags` and `rustc-hash` each resolve to two versions, and the engine pins the 2.x line. |
| Event channel | `async-channel` |
| Terminal helpers | `base64`, `aho-corasick`, `regex` |
| Serialization | `serde`, `serde_json` |
| Errors and logs | `anyhow`, `thiserror`, `log`, `env_logger` |
| Native crash capture | `crash-handler = 0.8.0` (app only) |
| Secrets | `zeroize` |
| Auto-update | `reqwest`, `semver`, `sha2`, `zip`, `tar`, `flate2` |
| UI helpers | `chrono`, `sysinfo`, `rust-embed` |
| Terminal graphics | `image` (default features off: only the pixel-buffer types `gpui::RenderImage` takes; same major as GPUI's own `image`) |
| Windows FFI | `windows-sys 0.59` with a workspace-wide feature union (`Win32_System_Pipes` + `Win32_Security` are `oneterm-pty`'s `CreatePipe`) |
| Build / development | `embed-resource`; diagnostics also use `libc`, `polling`, and `oneterm-pty`; `futures` (dev-only) feeds russh's in-process SSH agent server in `oneterm-ssh` tests |

Do not re-add without a design decision: `tracing` / `tracing-subscriber`, `directories`, `toml`, `russh-cryptovec`, `ssh-key`, `smol`, or `rust-i18n`.

### `oneterm-pty`'s direct dependencies (`US-0071`)

| Crate | Why | Note |
|---|---|---|
| `polling` | the caller's poll loop | **Public**: `Poller`, `Event` and `PollMode` appear in `EventedReadWrite`'s signatures, so a `polling` bump is a breaking change to `oneterm-pty`'s own API. |
| `windows-sys` | ConPTY, `CreatePipe`, the child-exit wait callback | Windows only, at the workspace pin. |
| `libc` | `openpty`, `TIOCSCTTY`/`TIOCSWINSZ`, the signal mask | Unix only. |
| `log` | one `info` line naming the resolved ConPTY host | |

The crate deliberately reproduces `miow` (anonymous pipes), `piper` (the reader/writer ring)
and `signal-hook` + `rustix-openpty` (Unix child exit) with the platform APIs and `std` instead
of depending on them: a transport crate must not install a process-global `SIGCHLD` handler, and
the ring is a `Mutex<VecDeque<u8>>` because a pseudo-console is nowhere near fast enough for the
lock to matter.

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
