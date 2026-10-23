# Research: Bump GPUI Kit to 0.8.0

Intake: IN-0017 (the intake that put OneTerm on published GPUI Kit; `US-0149` bumped 0.6 -> 0.7.0 here, `IN-0052` / `US-0190` bumped 0.7.0 -> 0.7.3)
Date: 2026-10-23
Author: advisor (research only; no cargo, no code changes)
Request: owner, 2026-10-23 — "bump gpui to 0.8.0"

## 1. Executive summary

1. **0.8.0 is published.** `gpui-base`, `gpui-component`, `gpui-component-macros` and `gpui-kit-assets` 0.8.0 reached crates.io on 2026-10-19; tag `v0.8.0` is the latest GitHub release. None is yanked.
2. **The gpui snapshot does not move.** GPUI Kit 0.8.0 pins `gpui-pre =0.3.9`, the version `Cargo.lock` already resolves (`Cargo.lock` `gpui-pre` 0.3.9). `gpui-pre` 0.3.9 is still the newest snapshot on crates.io. So `apply_force_width` (dependencies.md § 4 step 4) and every other gpui-level citation stay valid.
3. **No Rust toolchain change.** Neither crate declares `rust-version`; the workspace is edition 2024, which OneTerm already uses (`Cargo.toml:32`, toolchain 1.96.0).
4. **Exposure is small.** Of the 13 breaking changes in the 0.8.0 notes, none names an API OneTerm calls (section 3 below, grep-verified). The dock traits OneTerm implements (`DockAreaRenderer`, `TabGroupRenderer`, base and component `Panel` / `PanelView`) have identical function sets in both tags; `DockSkin` gains two public getters; `TableDelegate` gains `fit_column` with a default body; `Root` has an unchanged public surface; the theme schema has the same 185 keys.
5. **The "Zoom In" workaround stays.** 0.8.0 still builds the `...` menu as panel rows, separator, `menu_with_disabled("Dock.Zoom In", !menu_zoom)`, Close (`tab_panel.rs:352-362` at `v0.8.0`), identical to 0.7.3. `PopupMenu::rebuild` still exists. `US-0203`'s `observe_new` + `rebuild` strip in `crates/terminal-view/src/panel/tab_title.rs:983-1010` cannot be deleted.
6. **Behaviour to re-check, not compile breaks:** DataTable Home / End now select first / last row in row mode (both SFTP panes set `row_selectable(true)`); scroll-bounce wrapper now owns the vertical wheel (bounce itself is macOS-default only; OneTerm uses the kit's `scrollable` only on menus and `Select`, and the terminal grid is OneTerm's own element); `set_line_number` / `ListState::set_searchable` no longer notify on no-op.
7. **Graph shrinks.** `gpui-fast` 0.1.10 and `gpui-fast-macros` become optional in 0.8.0 (features `gpui-fast` on base / component / assets) and drop from `Cargo.lock`; `THIRD-PARTY-NOTICES.md` must be regenerated. No new crate enters: `syntect`, `two-face`, `tree-sitter`, `gpui-webview` are all optional and off.
8. **Effort:** one packet, same shape as `US-0190`: `cargo update` of the five packages, build, probably zero source changes, notices regeneration, four doc citations, `reference/gpui-kit` at `v0.8.0`, full gate. Estimate half a day including the Windows test build at `CARGO_BUILD_JOBS` 4.
9. **Recommendation: go**, under this intake (`IN-0017`) as `US-0205`, no new intake and no decision record (DEC-0006 calls this an ordinary version bump). The `IN-0052` precedent opened a dedicated intake for 0.7.3; that is the owner's call (Q1).

## 2. What changed 0.7.3 -> 0.8.0

Source: `release-notes.md` at tag `v0.8.0` and the GitHub release page (URLs in section 7). The local `reference/gpui-kit/release-notes.md` (tag `v0.7.3`) still has 0.7.0 as "Pending Updates" and knows nothing of 0.8.0.

### 2.1 Versions and requirements

| Package | 0.7.3 (today) | 0.8.0 | Note |
|---|---:|---:|---|
| `gpui-base`, `gpui-component`, `gpui-component-macros`, `gpui-kit-assets` | 0.7.3 | 0.8.0 | crates.io 2026-10-19, not yanked |
| `gpui-pre`, `gpui-pre-platform`, `gpui-pre-macros`, `gpui-pre-sum-tree` | =0.3.9 | =0.3.9 | unchanged; 0.3.9 is still the newest snapshot |
| `gpui-fast` | 0.1.10, mandatory | 0.1.10, **optional** | leaves OneTerm's lock |
| `rust-version` | none | none | edition 2024 both sides |
| new optional deps | — | `syntect ^5.3`, `two-face ^0.4.5` (feature `syntect`), `gpui-webview 0.8.0` (separate crate) | not pulled without the feature |

Workspace requirements `gpui-base = "0.7"` etc. (`Cargo.toml:43-45`) do **not** admit 0.8.0 — unlike `US-0190`, the manifest changes this time (`"0.8"`), together with `cargo update -p` on the three Kit layers. `gpui = { package = "gpui-pre", version = "0.3" }` (`Cargo.toml:40-41`) stays.

### 2.2 Breaking changes listed by the release (13) and whether OneTerm is touched

| # | Change | OneTerm |
|---|---|---|
| 1 | `gpui-wry` renamed `gpui-webview`; Linux WebView must start on X11 | no WebView in `crates/` (grep `WebView|webview`: none) |
| 2 | GPUI Fast optional again; `gpui-fast` feature must be on both `gpui-kit` and `gpui-webview` | OneTerm never enabled it; lock drops `gpui-fast` |
| 3 | `SelectionFormat` gains `SourceAcrossStyles` (not `#[non_exhaustive]`) | no `SelectionFormat` match in `crates/` |
| 4 | `ResizeHandle::on_drag` takes `&T`, not `Rc<T>` (component `resizable`) | not called; the dock skin only forwards `&ResizeHandleContext` (`crates/workspace/src/layout/workspace/dock_skin.rs:80-86`); base `resize_handle.rs` public surface unchanged |
| 5 | Scroll bounce: on by default on macOS; wrapper owns the vertical wheel unless `.scroll_wheel(false)`; desktop motion retuned; nested areas no longer scroll together | OneTerm uses the kit `scrollable` on `PopupMenu` (`crates/session-ui/src/tree_render.rs:119`, `crates/sftp-ui/src/render.rs:302`), on `Select` (`crates/settings-ui/src/appearance.rs:137`, `crates/settings-ui/src/terminal/font.rs`) and the form dialog scrollbar (`crates/state/src/form_dialog.rs:278`). The terminal grid is OneTerm's own element and handles the wheel itself. Re-check by hand on Windows: settings pages, Session tree, SFTP tables, long menus. |
| 6 | Textarea / editor keep the wheel gesture at the end of their text | settings-ui text inputs; cosmetic |
| 7 | `TabVariant` gains `Folder` | no `TabVariant` match in `crates/` |
| 8 | Out-of-range cubic-bezier easing overshoots | no `Easing` / cubic use in `crates/` |
| 9 | DataTable: Home / End select first / last row in row mode; `TableState::set_selected_row` no longer stops propagation | **both SFTP panes are row mode** (`crates/sftp-ui/src/panel.rs:151`, `crates/sftp-ui/src/local_pane.rs:500`); OneTerm does not bind Home / End itself and does not call `set_selected_row`. New behaviour, should be welcome; walk it. |
| 10 | `set_line_number`, `ListState::set_searchable` notify only on change | neither is called in `crates/` |
| 11 | Shell CLI (`gpui-shell`) | not used |
| 12 | Shell runtime data / env | not used |
| 13 | UI testing: pointer helpers keep modifiers; `TestWindowExt` requires `find_all` and `click_with_options`; nested context menus open only the innermost | OneTerm does not implement `TestWindowExt`; its UI tests drive gpui's `TestAppContext` directly. The "innermost menu" rule may matter for `crates/terminal-view/src/panel/tests/tab_menu_and_card.rs` (strip-menu tests) — run the gate and see. |

### 2.3 Trait and struct surface OneTerm implements or wraps (diffed `v0.7.3` vs `v0.8.0`)

| Item | File | Result |
|---|---|---|
| `gpui_base::dock::DockAreaRenderer` | `base/src/dock/dock_area.rs` | 7 fns, identical signatures |
| `gpui_base::dock::TabGroupRenderer` | `base/src/dock/tab_group.rs` | 6 fns, identical |
| `TabGroup` / `TabGroupContext` / `DockArea` public fns | same files | no added, removed or renamed `pub fn` |
| `gpui_base::dock::Panel`, `PanelView` | `base/src/dock/panel.rs` | 9 / 13 fns, identical |
| `gpui_component::dock::Panel`, `PanelView` | `component/src/dock/panel.rs` | 9 / 9 fns, identical (`zoom_control`, `dropdown_menu`, `title_bar`, `inner_padding` unchanged) |
| `gpui_component::dock::DockSkin` | `component/src/dock/mod.rs` | adds `pub fn is_open(&self) -> bool` and `pub fn placement(&self) -> DockPlacement`; nothing removed |
| `gpui_component::Root` | `component/src/root.rs` | identical public fns (`open_window` not mentioned by the notes either) |
| `TableDelegate` | `component/src/table/delegate.rs` | adds `fit_column(...) -> Option<Pixels>` with a default `None`; the two SFTP delegates compile unchanged |
| `PopupMenu::rebuild`, `menu_with_disabled` | `component/src/menu/popup_menu.rs` | both present |
| `.theme-schema.json` | repo root | same 185 property keys — no token added, removed or renamed; `scripts/check-theme-contrast.py`'s `SURFACES` table needs no change |

### 2.4 Additions OneTerm could adopt later (out of scope for the bump)

`DockToggleEvent` (user-driven vs programmatic dock toggles), `DataTable::fit_col` / double-click auto-fit, `fixed_cols_border`, `select_on_modifier_click`, `focus_ring`, `ScrollableMask::vertical_wheel` (a wheel on a horizontal strip — the `US-0190` note on `longbridge/gpui-kit` #3518 "vertical wheel on tab strips" is the same theme; whether #3518 itself landed in 0.8.0 was not verified), `InputState::display_map()` / `prewrap`, `TabVariant::Folder`, the `syntect` feature.

## 3. Which OneTerm code is exposed

Grep of `crates/` (`*.rs`, `*.toml`) for every identifier the release names: `SelectionFormat`, `TabVariant`, `ResizeHandle`, `resize_handle`, `scroll_bounce`, `ScrollableMask`, `ScrollBounce`, `set_line_number`, `set_searchable`, `TestWindowExt`, `DockToggleEvent`, `set_selected_row`, `gpui_fast`, `syntect`, `Easing`, `TextView`, `MarkdownView`, `WebView` — the only hit is `ResizeHandleContext` in `crates/workspace/src/layout/workspace/dock_skin.rs:18,82`, which is the base dock type, not the component `ResizeHandle` whose `on_drag` changed.

Items the brief asked about specifically:

- **Dock skin layer** (`crates/workspace/src/layout/workspace/dock_skin.rs`, `OneTermDockSkin: DockAreaRenderer`, `OneTermTabGroupSkin: TabGroupRenderer`): traits unchanged, wraps `DockSkin::new` / `with_renderer` which are unchanged. Expected to compile as is.
- **Panel implementations** (`TerminalPanel` `crates/terminal-view/src/panel/terminal_panel.rs:704`, `SshClientPanel` `crates/app/src/ssh_client_panel.rs:215`, `SessionPanel` `crates/session-ui/src/panel.rs:293`, `SftpPanel` `crates/sftp-ui/src/panel.rs:761`, plus the test probes): trait unchanged.
- **`PopupMenu::rebuild` + `observe_new`** (`crates/terminal-view/src/panel/tab_title.rs:983-1010`): 0.8.0 does **not** let a panel omit the zoom row; the kit still appends `menu_with_disabled(Dock.Zoom In, !menu_zoom)` unconditionally (`component/src/dock/tab_panel.rs:352-358` at `v0.8.0`). Keep the workaround. An upstream request ("skip the zoom row when `zoom_control()` is `None`") is the right fix and is a one-line `when` in the kit; not filed.
- **Theme tokens / `check-theme-contrast`**: schema identical; no work.
- **`Root` / `open_window`**: unchanged; the `US-0149` rule (call `WindowExt` outside a `Root` update, `window.defer`) still applies.
- **`Input` / `InputState`**: additive only (`display_map`, `prewrap`, software-keyboard hints). The eight `InputState` users in `crates/session-ui`, `crates/settings-ui`, `crates/state`, `crates/terminal-view` are not named by any breaking change.
- **`Table`**: see 2.2 #9 and 2.3 (`fit_column` default).
- **WebView**: not used.

## 4. Re-vendoring `reference/gpui-kit`

Yes. `docs/agents/dependencies.md` § 4 step 6 and § 5 require the ignored checkout to sit at the exact tag the lock resolves ("do not change the reference independently of the workspace dependency decision"). Procedure as recorded and as `US-0149` / `US-0190` did it: inside `reference/gpui-kit`, fetch and check out `v0.8.0`, leaving a clean tree; then update the tag citations in `docs/agents/dependencies.md` (§ 1 table, § 5 sentence), `docs/agents/structure.md:230`, `docs/terminal-backend.md:1257`. `docs/gui-layout.md:59,73` and the three `crates/settings-ui` module notes cite "the 0.7.3 kit" as the version that fixed the settings scroll; re-check `component/src/setting/page.rs` still scrolls by index in 0.8.0 and either leave the history as is or reword to "0.7.3 and later".

## 5. Risks

| Risk | Assessment |
|---|---|
| Windows build | `gpui-pre` unchanged, so the DirectWrite / DXGI surface OneTerm touches is unchanged. The component crate's Windows-only deps (`windows ^0.58`, `resvg`, `raw-window-handle`) are the same requirements as 0.7.3. Low. |
| Test build memory | unchanged from `US-0190`: keep `CARGO_BUILD_JOBS` at the hook's cap (4); `US-0149` hit `E0786` at 20 jobs. |
| Ignored elevation tests (`oneterm-workspace`, `oneterm-session-ui`, `oneterm-settings-ui`, `--ignored --test-threads=1`) | they drive `Root` / dialogs through the kit; `Root` surface unchanged. Run the three gate lines; no specific exposure found. |
| `fast-dev` and other profile overrides | keyed on package names `gpui-pre`, `gpui-pre-platform` (`Cargo.toml:209-210,242-243,297-298`); unchanged. If `gpui-fast` ever gets enabled it would need its own override; not now. |
| `THIRD-PARTY-NOTICES.md` | **will be stale**: `gpui-fast` and `gpui-fast-macros` leave the graph (lock shows them pulled only by `gpui-base` / `gpui-component` 0.7.3). Regenerate with `python scripts/third-party-notices.py` and verify with `--check`. |
| `cargo deny` | no new crate; `deny.toml` licence allow-list unaffected. `US-0190` recorded an unrelated `advisories` failure on `rustls`; expect the same until that is handled separately. |
| Behaviour drift (not compile) | DataTable Home / End in SFTP; wheel ownership on `scrollable` menus / `Select` / the form dialog; nested-context-menu rule in the strip-menu tests. Covered by the gate plus a manual walk (Q3). |
| gpui-kit `release-notes.md` says 0.8.0 "changes since 0.7.3" but 0.7.1-0.7.3 behaviour changes (`TooltipRequest::placement` deprecated, paragraph gap) are already absorbed by `US-0190`. | none |

## 6. Candidate packets and owner questions

Reserved ids checked by scoped grep of `docs/`: `US-0200..0204` used, `BUG-0089` named as taken by the brief, `IN-0056` used, `DEC-0023` / `DEC-0024` named as optional candidates in `IN-0051` / `IN-0055`. Proposals only:

- **`US-0205` — Bump GPUI Kit to 0.8.0 (gpui-pre stays 0.3.9).** Scope mirrors `US-0190`: `Cargo.toml:43-45` to `"0.8"`, `cargo update -p gpui-base -p gpui-component -p gpui-component-macros -p gpui-kit-assets`, `cargo tree -i` per family, build and any fallout (expected none), notices regeneration, `reference/gpui-kit` at `v0.8.0`, the doc citations in section 4, `scripts/ci-local.sh --full`, fast-dev launch, ui-verifier walk of the behaviour-drift items. Risk lane normal. Home: this intake, or `IN-0057` per Q1.
- No `BUG` and no `DEC`: DEC-0006 already covers an ordinary version bump; nothing persisted changes.
- Optional, later, separate packets (not part of the bump): adopt `DataTable::fit_col` in the SFTP panes; upstream a kit change to drop the zoom row when `zoom_control()` is `None`, then delete the `US-0203` strip.

Owner questions:

1. **Home for the packet** — `IN-0017` (where `US-0149` lives, as the brief suggests) or a new `IN-0057` (the `IN-0052` precedent for 0.7.3)? Recommendation: `IN-0017`; a one-packet intake with empty sections adds records, not information.
2. **Adopt any 0.8.0 feature in the same packet?** Recommendation: no; keep the bump pure like `US-0149` / `US-0190`, list candidates in the packet's "out of scope".
3. **Manual walk scope after the bump** — Recommendation: Windows only, ui-verifier: SFTP table Home / End and wheel, Session tree menu wheel, Settings `Select` dropdowns and a long settings page, a tab-strip `...` menu (no zoom row), one dialog and one toast. Linux / macOS wait for CI as before.
4. **The settings-ui "0.7.3 kit" wording** (`docs/gui-layout.md:59,73`, three `crates/settings-ui` module notes) — rewrite to "0.7.3 and later" or leave as history? Recommendation: leave; they describe when the fix arrived, which stays true.
5. **File the upstream request for an omit-able zoom row?** Recommendation: yes, but as a follow-up packet, not inside the bump; until it lands the `tab_title.rs` strip stays.

## 7. Queries and URLs relied on

- crates.io API `gpui-component` versions — 0.8.0 published 2026-10-19T13:06Z, 0.7.3 2026-10-12, none yanked, no `rust_version`: <https://crates.io/api/v1/crates/gpui-component/versions>
- crates.io API `gpui-base` versions — 0.8.0 published 2026-10-19T13:05Z: <https://crates.io/api/v1/crates/gpui-base/versions>
- crates.io API `gpui-pre` versions — 0.3.9 (2026-10-12) is the newest, none yanked: <https://crates.io/api/v1/crates/gpui-pre/versions>
- crates.io API `gpui-component` 0.8.0 dependencies — `gpui-pre =0.3.9`, `gpui-base ^0.8.0`, `gpui-fast ^0.1.10` optional, `syntect` / `two-face` optional: <https://crates.io/api/v1/crates/gpui-component/0.8.0/dependencies>
- GitHub releases index (v0.8.0 latest, v0.7.3, v0.7.1, v0.7.0 summaries): <https://github.com/longbridge/gpui-kit/releases>
- GitHub release v0.8.0 (the 13 breaking changes): <https://github.com/longbridge/gpui-kit/releases/tag/v0.8.0>
- `release-notes.md` at `v0.8.0` (same content, with signatures): <https://raw.githubusercontent.com/longbridge/gpui-kit/v0.8.0/release-notes.md>
- Workspace `Cargo.toml` at `v0.8.0` (`gpui-pre =0.3.9`, edition 2024, no `rust-version`, `gpui-webview` path crate): <https://raw.githubusercontent.com/longbridge/gpui-kit/v0.8.0/Cargo.toml>
- Source files at `v0.8.0` downloaded and diffed against the local `v0.7.3` reference (same path under `crates/`): `component/Cargo.toml`, `base/Cargo.toml`, `base/src/dock/{panel,tab_group,dock_area,mod,renderer}.rs`, `base/src/resizable/resize_handle.rs`, `component/src/dock/{panel,tab_panel,mod}.rs`, `component/src/menu/popup_menu.rs`, `component/src/root.rs`, `component/src/table/{table,delegate}.rs`, `.theme-schema.json` — all under <https://raw.githubusercontent.com/longbridge/gpui-kit/v0.8.0/>.

Everything fetched was treated as data. No `cargo` command was run for this research.
