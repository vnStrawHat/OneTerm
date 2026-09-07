# High-Level Design: Migrate to gpui-component 0.6.0 (GPUI Kit) from crates.io

Intake: IN-0017
Lane: high_risk
Date: 2026-09-07

## Idea

Replace the vendored `gpui-component` 0.5.2 fork (git rev `ea6b194d` + four local patches) and
the `zed-industries/zed` git dependency on `gpui` 0.2.2 with published crates.io releases:
`gpui-component` 0.6.0, `gpui-base` 0.6.0, `gpui-kit-assets` 0.6.0, and the `gpui-pre` 0.3.x
GPUI family. Upstream renamed the project to **GPUI Kit** (`longbridge/gpui-kit`,
[gpui-kit.com](https://gpui-kit.com)); `gpui-component` is still the styled layer inside it and
is still published under the same crate name, so OneTerm keeps its `use gpui_component::…`
import surface instead of adopting the `gpui-kit` facade.

Two things force this to be one coherent migration rather than a version bump:

1. **The dock was redesigned.** v0.6.0 split the dock into `gpui_base::dock` (pure-data layout
   tree, behavior) and `gpui_component::dock` (a *skin* over it). `DockItem` is gone,
   `TabPanel`/`StackPanel` are gone, the `Panel` trait is split in two, and a `DockArea` now
   needs a `DockSkin` renderer to draw any chrome at all. OneTerm's workspace shell is built
   directly on all of those.
2. **GPUI moved with it.** The component layer now targets `gpui-pre` 0.3.x (a crates.io
   republish of Zed's GPUI, snapshot `zed@5b055fa`) rather than GPUI 0.2.x, so OneTerm inherits
   whatever upstream GPUI drift sits between the pinned rev `1d217ee3` and that snapshot.

The upside is that the *reason* the fork exists disappears. Three of the four vendor patches
exist only because `TabPanel`'s tab-selection internals were private; v0.6.0 makes
`TabGroup::panels()`, `active_ix()` and `select_tab()` public, so those patches have no purpose
after the migration and the whole `vendor/gpui-component` tree, its patch series, and the
`check-ui-fork.py` CI gate can be retired.

## Diagram

```text
BEFORE                                          AFTER
──────                                          ─────
zed-industries/zed @1d217ee3 (git)              crates.io
  ├── gpui 0.2.2                                  ├── gpui-pre 0.3.x        (alias: gpui)
  └── gpui_platform                               ├── gpui-pre-platform     (alias: gpui_platform)
                                                  ├── gpui-base 0.6.0
longbridge/gpui-component @ea6b194d (git)         ├── gpui-component 0.6.0
  └── [patch] -> vendor/gpui-component 0.5.2      └── gpui-kit-assets 0.6.0
        + 0001 TabPanel::set_active_panel
        + 0002 standalone manifest              (no vendor tree, no patch series,
        + 0003 settings scroll fix               no check-ui-fork gate)
        + 0004 TabPanel::panel_count

DOCK ARCHITECTURE
─────────────────
0.5.2                                  0.6.0
DockArea                               DockArea            (gpui_base::dock — behavior + PaneTree)
 ├── DockItem::Split{items,sizes}       ├── PaneTree/PaneNode: Split | Tabs | Tiles  (pure data)
 ├── DockItem::Tabs{view: TabPanel}     ├── TabGroup        (entity, projected from a Tabs node)
 └── DockItem::Panel(PanelView)         └── DockLayout      (entity-free layout builder)
                                              ▲
trait Panel  (title + behavior)         DockSkin (renderer)  (gpui_component::dock — appearance)
                                        trait gpui_base::dock::Panel      -> behavior
                                        trait gpui_component::dock::Panel -> title/toolbar/menu
                                        PanelHandle / panel_handle()      -> carries presentation
```

## UI Wireframe

N/A — no intended user-facing change. The migration must be visually and behaviorally
invisible: same title bar, same tab bar, same right dock, same settings pages, same terminal
rendering. Any visual difference is a defect of this migration, not a feature of it. The
`DockSkin` seam is the one place where "invisible" is not free — the skin is what draws the tab
bar and dock chrome OneTerm currently gets implicitly — so the acceptance work includes a
side-by-side screenshot comparison of the workspace before and after.

## Data Flow

Persisted dock layout is the only user data crossing this migration, and it is the part that
can silently corrupt a user's workspace.

1. On startup `OneTermWorkspace::new` builds a `DockArea` with id `main-dock` and version
   `MAIN_DOCK_VERSION = 3`, then loads `docks.json` through `oneterm_state::dock_persistence`.
2. `dock_persistence::DockDocument` owns the OneTerm fields (`schema_version`, `zoomed_panel`,
   `sftp_table_state`) and `#[serde(flatten)]`s the gpui-component-owned fields
   (`version`, `center`, `left_dock`, `right_dock`, `bottom_dock`).
3. `DockArea::load` rebuilds each leaf panel through `PanelRegistry` keyed by `panel_name`
   (`Terminal`, `Session`, `SFTP`, `Agent`, `SshClient`), and `DockArea::dump` writes it back.
4. Upstream states that v0.6.0 **keeps the v0.5.0 JSON shape**, and the v0.6.0 `DockAreaState`
   / `DockState` / `PanelState` / `PanelInfo` / `DockPlacement` serde tags are frozen and
   test-pinned upstream (`the_serde_tags_are_frozen`, `the_shipped_fixture_still_deserializes`).
   So the plan is **no schema migration and no `MAIN_DOCK_VERSION` bump** — but that has to be
   *proven* against a real pre-migration `docks.json`, not assumed. If a round-trip test shows
   any drift, the fallback is a `MAIN_DOCK_VERSION` bump, which discards the user's saved
   layout and is therefore a decision, not an implementation detail.

## Measured Blast Radius

Counts are from the current tree, not estimates.

| Surface | Sites | Notes |
| --- | --- | --- |
| Crates depending on `gpui-component` | 11 | actions, agent-ui, app, session-ui, settings, settings-ui, sftp-ui, state, terminal-view, theme, workspace |
| Files importing `gpui_component` | 107 | import paths stay identical under package aliasing |
| `gpui_component::dock` references | 46 | the bulk of the real work |
| `Panel` trait impls | 5 | agent-ui/view.rs, app/ssh_client_panel.rs, session-ui/panel.rs, sftp-ui/panel.rs, terminal-view/panel/terminal_panel.rs |
| `register_panel` call sites | 5 | agent-ui, session-ui, sftp-ui, terminal-view, app/ssh_client_panel |
| `DockItem::*` construction | 4 files | workspace/layout.rs, workspace/actions.rs, workspace/layout_tests.rs, state/dock_util.rs |
| `TabPanel` references | 10 files | terminal-view (panel, space/drag, agent), workspace, state/dock_util |
| `setting::RenderOptions` field reads | 4 files | about.rs, separators.rs, terminal/font.rs, terminal/logging.rs — fields are now private |
| `InputState::multi_line(true)` | 1 site | app/crash_report_dialog.rs → `TextareaState`/`Textarea` |
| `gpui-component-assets` | 1 site + manifests | app/assets.rs → `gpui_kit_assets` |
| Not affected | — | `divider`→`separator` (unused), `Table`→`DataTable` (already `DataTable`), `is_eof`→`has_more` (unused), `row_selector` (unused), `webview` (unused), `History` (unused), chart/editor/markdown (unused) |

`gpui_component::resizable` (16 sites) survives: 0.6.0 keeps it as a backwards-compatible
re-export module.

## Owning Boundaries

- **Dependency declaration** — root `Cargo.toml` `[workspace.dependencies]` only. Package
  aliasing (`gpui = { package = "gpui-pre" }`) keeps rule R "declare once, `name.workspace =
  true` everywhere" intact and keeps all 107 import sites untouched.
- **Dock shell** — `crates/workspace` owns the `DockArea`, the skin, layout construction and
  persistence; `crates/state/src/dock_util.rs` owns tree traversal. Feature crates own only
  their own panel impls.
- **Panel presentation** — after the trait split, each panel crate implements
  `gpui_base::dock::Panel` (name, closable, zoomable, set_active, dump) *and*
  `gpui_component::dock::Panel` (title, toolbar, zoom_control).
- **Persisted schema** — `oneterm_state::dock_persistence` stays the single read/update API for
  `docks.json`; nothing else patches that file.

## Risk Register

| Risk | Impact | Mitigation |
| --- | --- | --- |
| `gpui-pre` is a third-party republish of Zed's GPUI, not a Zed release | Supply chain / licence | DEC record with rationale, publisher identity, and the rollback path back to a Zed git rev; regenerate `THIRD-PARTY-NOTICES.md`; run `cargo deny` |
| Unmeasured GPUI 0.2.2 → 0.3.x API drift | Unknown compile-error volume across every UI crate | Phase P1 is a pure dependency swap whose only deliverable is the full error inventory, recorded before any dock work starts |
| Dock chrome regressions from the `DockSkin` seam | User-visible layout/tab-bar breakage | Screenshot diff of the workspace, plus the existing `layout_tests.rs` / `panel/tests.rs` suites |
| `docks.json` incompatibility | Silent loss of the user's saved workspace | Round-trip test against a captured pre-migration `docks.json` before touching `MAIN_DOCK_VERSION` |
| Losing vendor patch 0003 (settings scroll fix) | Settings section navigation regression | Verify upstream behavior first; if still broken, fix from OneTerm side or keep a narrowed vendor tree — decided at P3, not assumed now |
| Transitive dependency churn (`rust-i18n`, `tree-sitter`, `ropey`, `windows`) vs OneTerm's pins | Build/licence surprises | `cargo tree -d` for duplicates and `cargo deny check` in the phase gate |

## Detail Design

- [x] Detail design: required (high-risk) — added
- Reason: the migration touches a persisted schema, a public trait surface consumed by five
  panel crates, and a third-party supply-chain change. Each is detailed separately under
  `low-level-design/`.

| File | Concern |
| --- | --- |
| `low-level-design/01-dependency-aliasing.md` | Workspace dependency shape, package aliasing, feature selection |
| `low-level-design/02-dock-migration.md` | `DockItem`→`DockLayout`, `TabPanel`→`TabGroup`, Panel trait split, `DockSkin`, registry |
| `low-level-design/03-persistence-compatibility.md` | `docks.json` round-trip proof and the `MAIN_DOCK_VERSION` fallback |
| `low-level-design/04-component-api-drift.md` | Non-dock breakage: `RenderOptions`, `Textarea`, assets rename, init |
| `low-level-design/05-vendor-retirement.md` | Retiring the fork, patches, CI gate, and refreshing `reference/` |

## Phasing

Executed on one long-lived branch (`feat/gpui-kit-0.6`), one commit per phase, merged only when
`scripts/ci-local.sh --full` is green and manual UAT is done. The set cannot be split into
independently mergeable slices: the dependency swap and the dock rewrite must land together or
the workspace does not compile.

| Phase | Outcome | Gate |
| --- | --- | --- |
| P0 | Capture baseline: pre-migration `docks.json`, workspace screenshots, `cargo tree` snapshot | Artifacts stored in the packet |
| P1 | Dependency swap + GPUI drift inventory | Full error list recorded; non-dock crates compile |
| P2 | Dock migration | `cargo clippy --workspace --all-targets -D warnings` clean |
| P3 | Non-dock component API drift | Same, plus settings-scroll behavior verified |
| P4 | Persistence compatibility proof | Round-trip test against the P0 `docks.json` |
| P5 | Vendor retirement + CI gates + docs/reference refresh | `ci-local.sh --full` green |
| P6 | Manual UAT (dock drag/drop, split, zoom, SFTP table, settings, SSH) | Screenshot diff vs P0 |
