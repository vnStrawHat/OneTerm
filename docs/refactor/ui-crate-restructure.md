# OneTerm workspace crate restructure — layer every crate the way Zed does

> Status: **proposal** (not yet implemented). Audience: OneTerm maintainers / AI agents.
> This is the single authoritative document. It supersedes and replaces the three
> earlier drafts (`docs/ui-crate-refactor-plan.md`,
> `docs/refactor/ui-crate-restructure-proposal.md`,
> `docs/refactor/ui-crate-restructure-plan.md`), resolving the contradictions
> between them.
>
> Goal: study how Zed organizes its crates (UI **and** non-UI), review every
> OneTerm crate, and propose a concrete, phased refactor that turns today's
> workspace — where `crates/ui` is a 21k-line monolith that depends *backwards* on
> `ssh`/`local`, and `crates/core` conflates a pure leaf with the alacritty
> terminal engine — into a cleanly layered set of crates with **no dependency
> cycle** and **no UI→backend edge**.

## Table of contents

1. [Scope: patterns only, not Zed code](#1-scope-patterns-only-not-zed-code)
2. [How Zed organizes its crates](#2-how-zed-organizes-its-crates)
3. [Current OneTerm workspace — review + coupling evidence](#3-current-oneterm-workspace--review--coupling-evidence)
4. [Target architecture](#4-target-architecture)
5. [Breaking the workspace ↔ feature cycle](#5-breaking-the-workspace--feature-cycle)
6. [File → target-crate mapping](#6-file--target-crate-mapping)
7. [`Cargo.toml` changes](#7-cargotoml-changes)
8. [`init()` ordering](#8-init-ordering)
9. [Phased execution plan (with gates)](#9-phased-execution-plan-with-gates)
10. [Risks & mitigations](#10-risks--mitigations)
11. [Definition of Done + effort](#11-definition-of-done--effort)
12. [Decisions log (resolved contradictions)](#12-decisions-log-resolved-contradictions)

---

## 1. Scope: patterns only, not Zed code

We borrow only Zed's **organizational patterns**: crate boundaries, where
components live, in-crate directory shape, and the *direction* of dependencies
between crates. We do **not** copy Zed types, functions, or implementations, and
no design choice here is justified by a Zed `file:line` citation. Where a rule is
grounded in OneTerm's own code, the evidence is cited against OneTerm files.

Two orthogonal axes that this plan keeps separate:

- **Zed splits by crate** (and tolerates very large files).
- **OneTerm splits by small files** (≤ ~400 lines) and puts every domain in one
  crate.

These are independent. The refactor keeps OneTerm's small-file discipline **and**
adds Zed's crate-level boundaries.

---

## 2. How Zed organizes its crates

### 2.1 UI layers (bottom → top)

```
gpui  (framework)
  → theme, component, icons, menu, ui_macros, ui_input, ui_prompt  (leaf UI)
    → ui                     (pure design system: Button/Label/Modal/… + prelude/styles/traits/utils)
      → workspace            (feature-agnostic app shell: Dock/Pane/Panel trait, status bar, persistence)
        → terminal_view, project_panel, outline_panel, settings_ui, git_ui, …  (one crate per dock panel)
          → zed              (binary: wires panels into the shell)
```

Key facts:
- `ui` is a **pure design system**; it never depends on a feature crate or a
  backend.
- `workspace` (the shell) is **feature-agnostic**: its `Cargo.toml` does not
  depend on `project_panel` / `terminal_view` / etc. Panels register into it.
- The **binary** depends on every panel crate and wires them in.
- Panels depend on `workspace` only to obtain the `Panel` trait. (OneTerm's `Panel`
  comes from `gpui-component::dock` instead — see §4, this removes the feature→shell
  edge entirely.)
- Zed allows exactly one feature→feature edge (`project_panel → git_ui`); otherwise
  panels are independent.

### 2.2 Non-UI layers

The same "split by crate, layer strictly downward" discipline governs Zed's non-UI
code:

- **Leaf/util crates** — tiny, dependency-light, no domain knowledge: `collections`,
  `util`, `paths`, `fuzzy`, `sum_tree`, `text`, `clock`. (Zed's `paths` depends on
  just `const_format`, `dirs`, `ignore`, `util`.)
- **Engine crates** — own one heavyweight subsystem and its third-party deps:
  `terminal` (owns `alacritty_terminal` + PTY + key/mouse mappings + terminal
  settings), `language`, `git`, `fs`. The engine crate is the *only* place its
  heavy dependency is visible.
- **Domain/model crates** — `project`, `worktree`, `buffer_diff`.
- **Protocol/transport crates** — `rpc`, `remote`, `client`, `http_client`.

The load-bearing rule: **a heavy or specialized dependency is confined to the one
crate that owns that subsystem; every other crate reaches it only through that
crate's types.** The clearest instance is `terminal` (engine, owns
`alacritty_terminal`) vs. `terminal_view` (UI) — the template for OneTerm's non-UI
split (§4.5).

---

## 3. Current OneTerm workspace — review + coupling evidence

### 3.1 `crates/ui` is a "god" crate

`crates/ui/src` = ~21,300 lines / 138 files in one crate, mixing four layers:

```
views/            17,094 lines / 101 files
  views/terminal   9,985 / 66   ← terminal_view + part of the terminal engine
  views/sftp       3,143 / 12   ← its own feature panel
  views/settings   2,026 / 11   ← settings_ui analog
  views/session_tabs 1,929 / 11 ← its own feature panel
state/            1,615 / 18     ← app state + settings infra mixed
layout/           1,471 / 9      ← workspace shell + title_bar + status_bar
components/          612 / 5     ← app-specific widgets (clock/net/resource/breadcrumb)
theme.rs             198 / 1
actions.rs           106 / 1
notif_ext.rs          89 / 1
icon.rs               89 / 1
```

It also **depends backwards** on `oneterm-local`, `oneterm-ssh`, `oneterm-core`,
`oneterm-highlight`, `alacritty_terminal` — violating `docs/agents/structure.md` §3
("UI must not import ssh/local") and `code-style.md` §4.

### 3.2 Coupling evidence (verified against code)

| Edge | Evidence (OneTerm code) | Handling |
|---|---|---|
| `views/session_tabs → views/terminal` | `connect_dialog.rs`, `quick_connect_dialog.rs` build a `TerminalPanel` for the SSH tab | keep one-way `session-ui → terminal-view` (Zed allows one such edge) |
| shell `layout/workspace/actions.rs → views::*` | verified: `use crate::views::{SessionPanel, SftpPanel, TerminalPanel}`; `on_action_add_panel` calls `TerminalPanel::new_entity` | **invert**: feature crates register their own action handlers (§5) |
| `AppState` is a cross-cutting bus | verified `state/app_state.rs`: `dock_area: Option<WeakEntity<DockArea>>`, `active_sftp: Option<Arc<dyn SftpBackend>>`, `active_cwd_source: Option<Arc<dyn CwdSource>>`, `active_is_local` — depends only on `oneterm_core` + `gpui-component` | put `AppState` in a **low** crate `oneterm-state` that both shell and features depend on (§4.3) |
| shared `actions.rs` | terminal/sftp/session/settings/shell all `use crate::actions::…` | extract leaf `oneterm-actions` |
| `theme.rs → state::UiConfig` + `crate::actions` | verified `theme.rs`: `use crate::actions::{SwitchTheme, SwitchThemeMode}`; reads `UiConfig` for font/theme | `oneterm-theme` depends on `oneterm-settings` (UiConfig) + `oneterm-actions` |
| `oneterm-highlight` used only by `views/terminal` | `highlight/bridge.rs`, render pipeline | move the `highlight` dep to `oneterm-terminal-view` |

### 3.3 The other (non-UI) crates

| Crate | Current deps | Verdict |
|---|---|---|
| `oneterm-core` | **`alacritty_terminal`**, async-channel, linkify, base64, thiserror, serde | **Conflates two layers.** `error`+`config`(shell/paths)+`sftp` are a pure leaf; all of `terminal/` is an alacritty-coupled engine — verified `core/src/terminal/session.rs` imports `alacritty_terminal::{selection, term::TermMode, vte::ansi::Rgb}`. → split (§4.5). |
| `oneterm-ssh` | core, alacritty, russh, tokio, russh-sftp | clean backend; repoint the trait import to the new engine crate |
| `oneterm-local` | core, alacritty, polling | clean backend; same note |
| `oneterm-highlight` | aho-corasick, regex | textbook leaf; keep as-is |
| `oneterm-app` | ui, core, gpui, gpui-component | thin binary; becomes the wiring point (implements `SessionFactory`, registers panels) |

The vendored `vendor/{alacritty_terminal,vte}` forks stay `[patch]` leaves; only the
engine + backends + terminal render crate depend on them.


---

## 4. Target architecture

### 4.1 Full crate graph (whole workspace)

Dependencies point **down only**. `NEW` marks crates this plan adds; `oneterm-ui`
is replaced by `oneterm-workspace`.

```
oneterm-app  (binary)
  • the ONLY crate that imports oneterm-ssh + oneterm-local
  • implements SessionFactory (wires backends behind the engine trait)
  • depends on the shell + all four feature crates; registers panels + action handlers

── GPUI feature layer (one crate per dock panel) ───────────────────────
  oneterm-terminal-view  NEW   ← views/terminal/
  oneterm-sftp-ui        NEW   ← views/sftp/
  oneterm-session-ui     NEW   ← views/session_tabs/ (+ SshSessionStore)
  oneterm-settings-ui    NEW   ← views/settings/
        depend on → gpui-component (Panel/register_panel), oneterm-state,
                    oneterm-settings, oneterm-actions, oneterm-theme, oneterm-core
        terminal-view also → oneterm-terminal (engine) + oneterm-highlight + alacritty_terminal
        session-ui also → oneterm-terminal-view  (the one allowed feature→feature edge)
        NONE of them depend on the shell, on ssh, or on local

── GPUI shell layer ────────────────────────────────────────────────────
  oneterm-workspace  NEW (replaces oneterm-ui)
        DockArea shell + title bar + app menus + status bar + app widgets
        (clock/net/resource/breadcrumb) + persistence + zoom
        depend on → gpui-component, oneterm-state, oneterm-settings,
                    oneterm-actions, oneterm-theme, oneterm-core
        does NOT depend on any feature crate, ssh, or local

── GPUI low layer (shared by shell AND features → breaks the cycle) ─────
  oneterm-actions  NEW   action structs            → gpui, gpui-component, oneterm-core, serde
  oneterm-theme    NEW   theme registry + AppIcon  → gpui, gpui-component, rust-embed, oneterm-settings
  oneterm-settings NEW   TerminalConfig/TerminalSettings/UiConfig → gpui, serde, schemars, oneterm-core
  oneterm-state    NEW   AppState + KeyBindingsSnapshot + notif helpers → gpui, gpui-component, oneterm-core

── backend layer (no GPUI) ─────────────────────────────────────────────
  oneterm-ssh   oneterm-local   implement oneterm-terminal::TerminalSession (+ SftpBackend)
        depend on → oneterm-terminal + oneterm-core

── engine layer (no GPUI) ──────────────────────────────────────────────
  oneterm-terminal  NEW   TerminalSession trait + content/palette/key+mouse encode
                          + osc/url/search + SessionFactory trait; OWNS alacritty_terminal
        depend on → oneterm-core (+ alacritty_terminal)

── leaf layer (no GPUI, minimal deps) ──────────────────────────────────
  oneterm-core       error, Result, config(shell/paths), SftpBackend, config data — NO alacritty
  oneterm-highlight  aho-corasick + regex only

── external ────────────────────────────────────────────────────────────
  alacritty_terminal + vte (vendored fork, [patch])      gpui + gpui-component
```

No cycle: the shell and the feature crates **both** depend on the low crates
(`actions`/`theme`/`settings`/`state`) and on `core`/`gpui-component`; neither
depends on the other; the binary wires them together. `ssh`/`local` are invisible
to every UI-layer crate.

### 4.2 Per-crate responsibility + dependency table

| Crate | Layer | Responsibility | Depends on |
|---|---|---|---|
| `oneterm-core` | leaf | error, `config`(shell/`config_dir`/`home_dir`), `SftpBackend`/`FileEntry`, config **data** types, connect-param structs | (minimal: serde, thiserror, …) |
| `oneterm-highlight` | leaf | plain-text semantic highlighter | aho-corasick, regex |
| `oneterm-terminal` | engine | `TerminalSession` trait + content/palette/encode/osc/url/search + `SessionFactory` trait + factory global | `oneterm-core`, `alacritty_terminal` |
| `oneterm-ssh` | backend | russh SSH + SFTP; impl `TerminalSession`+`SftpBackend` | `oneterm-terminal`, `oneterm-core`, russh, tokio |
| `oneterm-local` | backend | PTY local shell; impl `TerminalSession` | `oneterm-terminal`, `oneterm-core`, polling |
| `oneterm-actions` | UI-low | action structs (`AddPanel`, `AddSession`, `SwitchTheme`, …) | gpui, gpui-component, `oneterm-core`, serde |
| `oneterm-settings` | UI-low | `TerminalConfig`, `TerminalSettings`, `UiConfig` (schema + live globals) | gpui, serde, schemars, `oneterm-core` |
| `oneterm-state` | UI-low | `AppState`, `KeyBindingsSnapshotGlobal`, notif helpers | gpui, gpui-component, `oneterm-core` |
| `oneterm-theme` | UI-low | theme registry, `AppIcon`, icon assets, `build.rs` | gpui, gpui-component, rust-embed, serde_json, `oneterm-settings` |
| `oneterm-workspace` | shell | DockArea shell, title bar, menus, status bar, app widgets, persistence, zoom | gpui, gpui-component, `oneterm-state`, `oneterm-settings`, `oneterm-actions`, `oneterm-theme`, `oneterm-core` |
| `oneterm-terminal-view` | feature | terminal panel/element/render | gpui, gpui-component, `oneterm-terminal`, `oneterm-highlight`, alacritty_terminal, `oneterm-settings`, `oneterm-actions`, `oneterm-state`, `oneterm-core` |
| `oneterm-sftp-ui` | feature | SFTP browser panel | gpui, gpui-component, `oneterm-settings`, `oneterm-actions`, `oneterm-state`, `oneterm-core` (uses `Arc<dyn SftpBackend>`) |
| `oneterm-session-ui` | feature | session tabs + dialogs + `SshSessionStore` | gpui, gpui-component, `oneterm-terminal-view`, `oneterm-settings`, `oneterm-actions`, `oneterm-state`, `oneterm-core` |
| `oneterm-settings-ui` | feature | settings window (`pages/` + `components/`) | gpui, gpui-component, `oneterm-theme`, `oneterm-settings`, `oneterm-actions`, `oneterm-state` |
| `oneterm-app` | binary | wiring; implements `SessionFactory`; registers panels/actions | everything above + `oneterm-ssh` + `oneterm-local` |

### 4.3 Where cross-cutting state lives (decisions)

| State | Crate | Why |
|---|---|---|
| `AppState` (dock_area, zoomed_panel, toggle_button_visible, active_sftp/cwd/is_local) | **`oneterm-state`** (low) | it is the cross-cutting bus every feature reads/writes; placing it *below* both shell and features is what removes the cycle. Verified it needs only `oneterm-core` + `gpui-component`. |
| `KeyBindingsSnapshotGlobal` | **`oneterm-state`** | shell snapshots it; `settings-ui` consumes it — both are low-crate consumers, so no shell↔feature edge |
| `TerminalConfig` + `TerminalSettings` | **`oneterm-settings`** | shell (menus), terminal-view, settings-ui all use it |
| `UiConfig` (ui_font_size, theme_name, key_bindings) | **`oneterm-settings`** | theme + settings-ui + shell need it; keep it at the low layer |
| `SshSessionStore` + `SshSession` | **`oneterm-session-ui`** (feature-local) | only `views/session_tabs/*` uses it — panel-local state, mirroring Zed's `ProjectPanelSettings`-in-`project_panel` pattern |

> Config **data** types that are pure `serde` (no gpui) may live in `oneterm-core`
> instead of `oneterm-settings`, mirroring Zed's `settings_content` (data) vs.
> `settings` (live store). Optional refinement; not required for the cycle fix.

### 4.4 Backend injection via `SessionFactory` (no UI→backend edge)

Instead of UI crates calling `oneterm_ssh::connect` / `oneterm_local::LocalSession`
directly (today's illegal coupling), define:

- `trait SessionFactory` in **`oneterm-terminal`** (it returns `Box<dyn TerminalSession>`,
  an engine type), plus a global slot for the active factory.
- Connect-param structs (host/port/auth, shell kind) are plain data in
  **`oneterm-core`**.
- **`oneterm-app` implements `SessionFactory`**, wiring `oneterm-ssh` +
  `oneterm-local`, and installs it as the global during `init`.
- UI crates call the factory via the global: `terminal-view` for local shells,
  `session-ui` for SSH. They never import `ssh`/`local`.

This makes `oneterm-app` the only crate that sees the backends — exactly Zed's
"binary wires concrete implementations" pattern.

### 4.5 Non-UI split: `oneterm-core` → leaf + `oneterm-terminal` engine

`oneterm-core` pulls `alacritty_terminal` only because of its `terminal/` module.
Mirroring Zed's `terminal` (engine) vs. leaf-crate separation:

- **`oneterm-core` stays a pure leaf** and **drops `alacritty_terminal`**: keeps
  `error`, `config`, `sftp`, config data, connect-param structs.
- **`oneterm-terminal` (NEW engine)** receives all of `core::terminal/` (the
  `TerminalSession` trait + `content`/`palette`/`colors_util`/`key_encode`/
  `mouse_encode`/`osc`/`osc_color`/`url`/`url_policy`/`search`/`paste`/
  `security_policy`/`contracts`/`model`) plus the `SessionFactory` trait. It is the
  only non-UI crate that sees `alacritty_terminal`.
- `oneterm-ssh`/`oneterm-local` implement `oneterm-terminal::TerminalSession`;
  `oneterm-terminal-view` depends on `oneterm-terminal`. This reproduces Zed's
  `terminal` ↔ `terminal_view` pairing and makes `core` cheap for everything to
  depend on.


---

## 5. Breaking the workspace ↔ feature cycle

Today two edges would form a cycle once split:

```
shell  ──(actions.rs builds TerminalPanel/SessionPanel/SftpPanel)──▶  features
features ──(read/write AppState, dock_area)───────────────────────▶  shell
```

Both edges are removed by the target design — **no registry indirection is
needed**:

1. **feature → shell edge disappears.** Everything features need from the shell
   (`AppState`, `dock_area`, key-binding snapshot, notifications) now lives in the
   low crates `oneterm-state` / `oneterm-settings`. The `Panel` trait comes from
   `gpui-component::dock`, not the shell. So no feature crate depends on
   `oneterm-workspace`.
2. **shell → feature edge disappears.** The shell no longer constructs feature
   panels. Each feature crate registers its own panel factory and action handlers
   in its `init(cx)`; the binary calls each `init`.

### 5.1 Feature `init` registers its own action handlers

```rust
// crates/terminal-view/src/lib.rs (illustrative — not Zed code)
pub fn init(cx: &mut App) {
    register_panel(cx, "terminal", |_, _, _, window, cx| {
        Box::new(TerminalPanel::new_entity(window, cx))
    });
    register_panel(cx, "terminal-settings", |_, _, _, window, cx| {
        Box::new(TerminalSettingsPanel::new_entity(window, cx))
    });

    cx.on_action::<oneterm_actions::AddPanel>(|action, cx| {
        // IMPORTANT: read the dock_area from the AppState global AT CALL TIME.
        // Do NOT capture it at init — `AppState.dock_area` is set later, in
        // `OneTermWorkspace::new`, so a value captured during init is still `None`.
        let Some(weak) = oneterm_state::AppState::global(cx).read(cx).dock_area.clone() else { return };
        let Some(dock) = weak.upgrade() else { return };
        let placement = action.0;
        cx.update(|cx| {
            let panel: Arc<dyn PanelView> = Arc::new(TerminalPanel::new_entity(/* window, */ cx));
            dock.update(cx, |d, cx| d.add_panel(panel, placement, None, /* window, */ cx));
        });
    });
}
```

> **Bug fixed from the earlier draft:** the previous plan captured
> `dock_area_global(cx)` at registration time. Because `AppState.dock_area` is set
> only in `OneTermWorkspace::new` (after the window opens — see
> `state/app_state.rs`), a captured weak ref is `None`. The handler must resolve
> the global **inside** the closure, at action-dispatch time.

Handlers that move out of the shell into features/binary:
`AddPanel`, `AddPanelWithShell` → `terminal-view`; `AddSession`, `NewSession` →
`session-ui`; `AddSftpBrowser` → `sftp-ui`; `OpenSettings` → `settings-ui`.
The shell keeps only feature-agnostic handlers: `Quit`, `About`,
`ToggleDockToggleButton`, `ToggleAutoHideRightDock`, and `Find` (delegated).

### 5.2 Key-binding cycle (Vòng 2) also gone

`OneTermWorkspace::bind_keys` only **snapshots** current bindings into
`oneterm_state::KeyBindingsSnapshotGlobal`. `oneterm-settings-ui::init` reads that
snapshot and applies rebinds. Both touch the low crate, not each other; the `init`
order (§8) guarantees the snapshot exists first.

---

## 6. File → target-crate mapping

**Non-UI:**

| Current path | Target crate | Notes |
|---|---|---|
| `core/src/terminal/**` + `SessionFactory` | `oneterm-terminal` (NEW) | alacritty-coupled engine |
| `core/src/{error,config,sftp}.rs` + config data | `oneterm-core` (leaf) | drops `alacritty_terminal` |
| `ssh/**`, `local/**` | unchanged crates | repoint trait import to `oneterm-terminal` |
| `highlight/**` | unchanged | pure leaf |

**UI (`crates/ui/src/…`):**

| Current path | Target crate | Notes |
|---|---|---|
| `actions.rs` | `oneterm-actions` | rewrite `use crate::actions::…` → `use oneterm_actions::…` (~15 files) |
| `theme.rs`, `themes/`, `assets/icons/`, `icon.rs`, `build.rs` | `oneterm-theme` | move `ONETERM_UI_ICONS_DIR` build path with it; verify icon macro |
| `state/terminal_config/`, `state/terminal_settings/`, `state/ui_config.rs` | `oneterm-settings` | schema + live globals |
| `state/app_state.rs`, `notif_ext.rs` | `oneterm-state` | + `KeyBindingsSnapshotGlobal` |
| `layout/**` (mod, app_menus, statusbar, title_bar, workspace/*) | `oneterm-workspace` | shell |
| `components/**` (clock/net/resource/breadcrumb) | `oneterm-workspace/widgets/` | shell-only widgets |
| `views/terminal/**` | `oneterm-terminal-view` | keep sub-module structure |
| `views/sftp/**` | `oneterm-sftp-ui` | |
| `views/session_tabs/**` + `state/session_state.rs` | `oneterm-session-ui` | `SshSessionStore` is panel-local |
| `views/settings/**` | `oneterm-settings-ui` | `pages/` + `components/` sub-folders |

---

## 7. `Cargo.toml` changes (workspace root)

### 7.1 `members`

```toml
members = [
    "crates/app", "crates/core", "crates/highlight", "crates/local", "crates/ssh",
    "crates/terminal",        # NEW engine
    "crates/actions", "crates/theme", "crates/settings", "crates/state",
    "crates/workspace",       # replaces crates/ui
    "crates/terminal-view", "crates/sftp-ui", "crates/session-ui", "crates/settings-ui",
]
```

### 7.2 `[workspace.dependencies]` — add the new crates

```toml
oneterm-terminal      = { path = "crates/terminal" }
oneterm-actions       = { path = "crates/actions" }
oneterm-theme         = { path = "crates/theme" }
oneterm-settings      = { path = "crates/settings" }
oneterm-state         = { path = "crates/state" }
oneterm-workspace     = { path = "crates/workspace" }
oneterm-terminal-view = { path = "crates/terminal-view" }
oneterm-sftp-ui       = { path = "crates/sftp-ui" }
oneterm-session-ui    = { path = "crates/session-ui" }
oneterm-settings-ui   = { path = "crates/settings-ui" }
# remove oneterm-ui at the final phase
```

### 7.3 `[profile.dev.package]` — keep the terminal hot path optimized

The current block force-optimizes `oneterm-ui`, `oneterm-core`, `oneterm-local`,
`oneterm-ssh`, `alacritty_terminal`. The hot path now spans the new crates:

```toml
oneterm-terminal      = { opt-level = 3 }   # per-frame content clone
oneterm-terminal-view = { opt-level = 3 }   # render / box-drawing paint loop
oneterm-settings      = { opt-level = 3 }   # TerminalSettings::from clones
# keep oneterm-core / oneterm-local / oneterm-ssh / alacritty_terminal as-is
# drop the oneterm-ui entry when crates/ui is removed
```

Otherwise DOOM-fire / fast scroll will regress to choppy in debug.

---

## 8. `init()` ordering (in `oneterm-app::run`)

```
gpui_component::init(cx);
// low layer
oneterm_settings::init(cx);        // TerminalSettings + UiConfig globals
oneterm_state::init(cx);           // AppState + KeyBindingsSnapshot
oneterm_theme::init(cx);           // theme registry + apply UiConfig
// install the backend factory (only place that sees ssh/local)
oneterm_app::install_session_factory(cx);
// shell
oneterm_workspace::init(cx);       // shell globals; bind_keys snapshots into oneterm-state
// features (each registers its panels + action handlers)
oneterm_terminal_view::init(cx);
oneterm_sftp_ui::init(cx);
oneterm_session_ui::init(cx);      // needs terminal-view (session→terminal edge)
oneterm_settings_ui::init(cx);     // applies key bindings from the snapshot
```

Ordering constraints: `settings` → `state` → `theme` → factory → `workspace` →
`terminal-view` → `session-ui` (needs terminal-view) → `settings-ui` (needs the
snapshot).


---

## 9. Phased execution plan (with gates)

Every phase must end green:
`cargo fmt --all -- --check` · `cargo clippy --workspace --all-targets -- -D warnings` ·
`cargo build --workspace` · `cargo test --workspace` (per `AGENTS.md` §5).
Keep `crates/ui` as a shrinking scaffold until the final phase. Recommend one
crate per PR.

### Track T — Extract the `oneterm-terminal` engine (non-UI; can run first/in parallel)
1. Create `crates/terminal` (`oneterm-terminal`); add to `members` + workspace deps.
2. Move `core/src/terminal/**` + `SessionFactory` into it verbatim; deps =
   `oneterm-core` + `alacritty_terminal`.
3. Remove `alacritty_terminal` from `oneterm-core`.
4. Repoint `ssh`/`local` at `oneterm-terminal` for the trait; migrate `use` paths
   (leave `pub use` shims in `core` only if needed, then delete).
5. Gate + `cargo tree`: confirm `oneterm-core` no longer pulls `alacritty_terminal`.

### Phase 1 — Low leaf crates (lowest risk)
- **1.1 `oneterm-actions`** ← `actions.rs`; rewrite `use crate::actions::` across ui.
- **1.2 `oneterm-settings`** ← `state/{terminal_config,terminal_settings,ui_config}`;
  merged `init`; repoint `theme.rs` + views to `oneterm_settings::…`.
- **1.3 `oneterm-state`** ← `state/app_state.rs` + `notif_ext.rs` +
  `KeyBindingsSnapshotGlobal`.
- Gate after each: build + test `oneterm-ui`.

### Phase 2 — `oneterm-theme`
- Move `theme.rs`, `icon.rs`, `themes/`, `assets/icons/`, `build.rs`. Deps: gpui,
  gpui-component, rust-embed, serde_json, `oneterm-settings`. Verify icon macro +
  theme load at runtime.

### Phase 3 — `oneterm-workspace` shell (feature-agnostic)
- Move `layout/**` + `components/**` (→ `widgets/`). Split `workspace/actions.rs`:
  keep feature-agnostic handlers; the feature handlers (`Add*`, `NewSession`,
  `OpenSettings`) are **removed** here and re-added in the feature crates (Phase 4).
- `bind_keys` snapshots into `oneterm-state` only.
- Deps: gpui, gpui-component, `oneterm-state`, `oneterm-settings`,
  `oneterm-actions`, `oneterm-theme`, `oneterm-core`. **No feature/backend deps.**

### Phase 4 — Feature panel crates (terminal-view first; session-ui depends on it)
- **4.1 `oneterm-terminal-view`** ← `views/terminal/`. `init`: register panels +
  `AddPanel`/`AddPanelWithShell` (§5.1). Replace the direct `oneterm_local::LocalSession`
  call with `SessionFactory`. Drop the `local` dep.
- **4.2 `oneterm-sftp-ui`** ← `views/sftp/`. `init`: register panel + `AddSftpBrowser`.
  Uses `Arc<dyn SftpBackend>` from `AppState`; no `ssh` dep.
- **4.3 `oneterm-session-ui`** ← `views/session_tabs/` + `session_state.rs`. `init`:
  `SshSessionStore::init` + register panel + `AddSession`/`NewSession`. Replace the
  direct `oneterm_ssh::connect` calls with `SessionFactory`. Drop the `ssh` dep;
  keep the `terminal-view` dep.
- **4.4 `oneterm-settings-ui`** ← `views/settings/` (reshape into `pages/` +
  `components/`). `init`: `OpenSettings` + apply key bindings from the snapshot.

### Phase 5 — Cleanup
- Delete `crates/ui`; remove from `members` / workspace deps / profile.
- Point `oneterm-app` at the new crates; move backend deps + `SessionFactory` impl
  into `app`.
- Update `docs/agents/structure.md` + `dependencies.md`, `AGENTS.md` FAQ crate
  count, and the README doc list.
- Verify dev render (DOOM-fire, fast scroll) is smooth (opt-level check).

---

## 10. Risks & mitigations

| # | Risk | Mitigation |
|---|---|---|
| R1 | Moving `TerminalSession` out of `core` (Track T) breaks downstream imports | `pub use` shims in `core` during the move; migrate crate-by-crate; build gate each step |
| R2 | Re-introducing a shell↔feature cycle | Enforced by design: `AppState`/snapshot in low crates; shell has zero feature/backend deps; verify `cargo tree -e features` has no cycle |
| R3 | Capturing `dock_area` at init returns `None` | Resolve the `AppState` global **inside** the action closure (§5.1) |
| R4 | `build.rs` icon path breaks after moving assets | Move `build.rs` + assets together into `oneterm-theme`; runtime icon test |
| R5 | opt-level regression → choppy terminal | Add opt-level=3 for `oneterm-terminal` + `oneterm-terminal-view` + `oneterm-settings` (§7.3); verify in Phase 5 |
| R6 | UI crate still pulls `ssh`/`local` | `SessionFactory` injection (§4.4); confirm via `cargo tree` that only `oneterm-app` depends on them |
| R7 | `register_panel` called before `gpui_component::init` | `init` order (§8) runs all feature `init` after `gpui_component::init` |
| R8 | session→terminal-view feature edge | Accepted (one-way; Zed allows one such edge). To fully decouple later, inject a panel factory trait |

---

## 11. Definition of Done + effort

**Done when:**
- [ ] `cargo build --workspace` + `cargo test --workspace` green (dev + release).
- [ ] `crates/ui` removed; no `oneterm-ui` in members/deps/profile.
- [ ] New crates present: `terminal`, `actions`, `theme`, `settings`, `state`,
      `workspace`, `terminal-view`, `sftp-ui`, `session-ui`, `settings-ui`.
- [ ] `cargo tree` shows **only `oneterm-app`** depends on `oneterm-ssh`/`oneterm-local`.
- [ ] `cargo tree` shows `oneterm-core` no longer depends on `alacritty_terminal`.
- [ ] `cargo tree -e features` shows **no cycle**; no feature crate depends on
      `oneterm-workspace`.
- [ ] Docs updated (`structure.md`, `dependencies.md`, `AGENTS.md`, `README.md`).
- [ ] Smoke test: open terminal, split, SSH connect → tab, SFTP browser + sync-cwd,
      settings window, key rebind — all work; dev render is smooth.

**Effort (relative):**

| Phase | Crate(s) | Files moved | Risk | Effort |
|---|---|---|---|---|
| T | terminal (engine) | ~18 | medium | medium |
| 1 | actions / settings / state | ~1 / ~18 / ~2 | low | small–medium |
| 2 | theme | ~3 + assets | medium (build.rs) | medium |
| 3 | workspace | ~14 | **high** (shell split) | large |
| 4.1 | terminal-view | ~66 | medium | large |
| 4.2–4.4 | sftp-ui / session-ui / settings-ui | ~12 / ~13 / ~11 | medium | medium each |
| 5 | cleanup | — | low | small |

---

## 12. Decisions log (resolved contradictions)

The earlier drafts disagreed with each other; resolved here:

1. **Does the shell depend on `oneterm-core`?** The proposal said no; the plan said
   yes (via `AppState`). → **Neither.** `AppState` moves to `oneterm-state`; the
   shell depends on `oneterm-state` (which depends on `core`), not on the raw bus.
2. **Where is `AppState`?** Draft put it in the shell → forced feature→shell and a
   registry hack. → **`oneterm-state`** (low crate); the hack is deleted.
3. **Do UI crates depend on `ssh`/`local`?** Drafts kept `terminal-view→local`,
   `session-ui/sftp-ui→ssh`. → **No.** `SessionFactory` injected by the binary;
   only `oneterm-app` sees the backends (fixes `structure.md` §3 violation).
4. **Settings crate name / `UiConfig` home.** → single **`oneterm-settings`** holds
   `TerminalConfig`/`TerminalSettings`/`UiConfig`; pure data types may drop to
   `core` (optional).
5. **App widgets (clock/net/resource) home.** → `oneterm-workspace` (shell-only
   consumers), not a separate crate.
6. **`SshSessionStore` home.** → `oneterm-session-ui` (panel-local; only session UI
   uses it).
7. **Terminal engine.** → dedicated **`oneterm-terminal`** crate; `oneterm-core`
   becomes alacritty-free (was ambiguous in the drafts, which left the engine in
   `core`).
8. **Shell name.** → **`oneterm-workspace`** (Zed's shell name), replacing
   `oneterm-ui`.
