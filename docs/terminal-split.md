# Terminal Split — Design (index)

> **Status (2026-08):** implemented — `crates/terminal-view/src/space/` (`SpaceTree`,
> `render.rs`, `drag.rs`, `placeholder.rs`) and `crates/terminal-view/src/panel/`
> (`TerminalPanel` + `PanelSpec`, `ops.rs`, `actions.rs`). The sub-documents track
> the shipped behaviour; the decisions below are annotated where reality moved on.
>
> Design for **Terminal Split**: splitting a single Terminal Tab into multiple
> resizable **Spaces** (Right / Left / Up / Down), nested recursively, *inside
> the current tab* — no new Dock, no new Tab.
>
> This design is split into focused documents (kept small on purpose). Read them
> in order; each builds on the previous.

## Documents

| # | File | Topic |
|---|---|---|
| 0 | [`terminal-split/00-overview.md`](terminal-split/00-overview.md) | Goals, scope, requirements, non-goals, glossary |
| 1 | [`terminal-split/01-architecture.md`](terminal-split/01-architecture.md) | The Space pane-tree model, data structures, where it lives |
| 2 | [`terminal-split/02-split-and-close.md`](terminal-split/02-split-and-close.md) | Split operations, Close Space, tree collapse, active tracking |
| 3 | [`terminal-split/03-drag-drop.md`](terminal-split/03-drag-drop.md) | Dragging a Terminal Tab into a Space + the `DragPanel` constraint |
| 4 | [`terminal-split/04-context-menu.md`](terminal-split/04-context-menu.md) | Context-menu changes (Split R/L/U/D, Close Space) |
| 5 | [`terminal-split/05-rendering-theme.md`](terminal-split/05-rendering-theme.md) | 1px outer border + 1px inner gutter, active-Space highlight, empty placeholder |
| 6 | [`terminal-split/06-integration.md`](terminal-split/06-integration.md) | Touch points: `TerminalPanel`, `set_active`, statusbar/SFTP, focus |
| 7 | [`terminal-split/07-roadmap-risks.md`](terminal-split/07-roadmap-risks.md) | File layout, implementation order, risks, open questions |

## TL;DR

Before this feature a `TerminalPanel` wrapped exactly one `LocalTerminalView`. It now
holds a **`SpaceTree`**: a binary pane tree whose leaves are either a terminal view or
an empty placeholder. Splitting a leaf turns it into a `Split` node with two children
(the existing terminal + a new empty placeholder). The tree is rendered with nested
`h_resizable`/`v_resizable` groups (gpui-component resize handles); every Space has a
neutral 1px outer border plus a 1px inner gutter that takes the accent colour on the
active leaf, and a leaf can be filled by dragging a Terminal Tab onto it. Closing the
last remaining leaf reverts the tab to a plain single terminal (no borders).

## Confirmed decisions (from clarification)

1. **Drag scope**: only **Terminal Tabs** can be dropped into a Space (not SFTP /
   Session panels). Drop semantics are **move** (the source tab's content is moved
   into the Space; the emptied source tab closes).
2. **Nesting**: **recursive** — any Space can be split again in any direction
   (binary pane tree, resizable), like tmux/Zed panes.
3. **Collapse**: when Spaces are closed down to one, the Tab shows a **plain single
   terminal** again (no Space borders).
4. **Persistence**: **not persisted** for the MVP — on restart a tab is a single
   terminal (matches the fact that terminal sessions don't persist anyway).
5. **Language**: docs are written in **English** (AGENTS.md core principle #6).
6. **Drop target**: only an **empty** Space accepts a dropped tab. A Space that
   already holds a terminal is not droppable (no edge-aware split-on-drop).
7. **Active after split**: the **new empty** Space becomes active.
8. **Border**: originally a uniform 4px frame; shipped as a neutral **1px outer border +
    1px inner gutter** per Space (`space/render.rs`), see [05](terminal-split/05-rendering-theme.md).
9. **New Terminal Here**: the empty-Space menu can spawn a local shell in place
   (in MVP scope).
10. **Keyboard shortcuts** for Split / Close Space: deferred for the MVP; the
    `SplitRight/Left/Up/Down` and `CloseSpace` actions are now rebindable in the
    Settings key-binding UI (`crates/settings-ui/src/key_bindings/`).
