# 07 — File layout, roadmap, risks, open questions

> Part of [Terminal Split design](../terminal-split.md). The plan to build it and
> the things to watch.

## 1. Proposed file layout (crate `ui`)

Follows `docs/agents/structure.md` conventions (feature folder under
`views/terminal/`, each file ≤ ~400 lines, one responsibility per file):

```
crates/ui/src/views/terminal/
├── panel.rs                 # CHANGED: holds SpaceTree instead of a single view;
│                            #          retargets methods to the active Space (06)
├── space/                   # NEW — the split "Space" pane tree
│   ├── mod.rs               # SpaceTree + SpaceId + public API (split/close/active/fill)
│   ├── node.rs              # SpaceNode / SpaceLeaf / SpaceSplit + tree ops (find, collapse)
│   ├── ops.rs               # split() / close() / active-selection logic (02)
│   ├── render.rs            # recursive render → nested h/v_resizable + borders/highlight (05)
│   ├── drag.rs              # DragTerminalTab payload + handle_tab_drop (03)
│   └── placeholder.rs       # empty-Space placeholder + its context menu (04 §3, 05 §5)
└── handlers/
    └── menu.rs              # CHANGED: add Split R/L/U/D + Close Space (04)
```

`views/terminal/mod.rs` gains `mod space;` and re-exports what `panel.rs` needs.

No changes to `core`, `local`, or `ssh` — this is a pure `ui` feature (it only
re-parents existing views and calls existing `TerminalSession` methods).

## 2. Implementation order (roadmap)

1. **Tree model, no UI wiring.** Add `space/{mod,node,ops}.rs` with `SpaceTree`,
   `split`, `close`, collapse, active-selection. Unit-test the tree ops (pure logic:
   split shapes, collapse invariants, active-after-close) — no GPUI needed.
2. **Render single-leaf tree.** Make `TerminalPanel` hold a one-leaf `SpaceTree` and
   render it via the fast path. App must look/behave exactly as today. (Refactor-safe
   checkpoint.)
3. **Split + borders + highlight.** Implement `space/render.rs` (nested resizables,
   neutral outer border, layout-reserved inner frame, active highlight) and hook Split R/L/U/D into the context menu
   (04). New Space = empty placeholder (05 §5); activate the new empty Space.
4. **Active tracking.** Focus→active, highlight moves, `set_active`/statusbar/SFTP
   read the active Space (06 §2–§3).
5. **Close Space + New Terminal Here.** Close Space menu item + collapse +
   last-Space-closes-tab (02, 06 §4); "New Terminal Here" in the empty-Space menu
   spawns a local shell in place (04 §3).
6. **Drag a tab into an empty Space.** `space/drag.rs`: `DragTerminalTab` on the tab
   title, **empty**-Space drop targets only, move-on-drop (03). **Prototype the drag
   precedence first** (see risk R1).
7. **Polish (post-MVP).** Keyboard shortcuts (04 §5), full-subtree graft on drop
   (03 §5), persistence (§6).

Run the quality gate after each step: `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`
(AGENTS.md §5).

## 3. Risks

| # | Risk | Mitigation |
|---|---|---|
| R1 | **Drag precedence**: our `on_drag` on the tab title may not beat gpui-component's `Tab` wrapper `on_drag(DragPanel)`. | Prototype step 6 in isolation first. `cx.stop_propagation()` in our drag builder should win when the gesture starts on the title. Fallback: a dedicated drag "grip" affordance in the tab, or patch the fork to expose `DragPanel` (03 §6). |
| R2 | **`DragPanel` is `pub(crate)`** → cannot reuse the dock's native tab DnD. | Own payload `DragTerminalTab` (03). |
| R3 | **Space frame vs resize handle overlap**: the handle shares the pane edge and may obscure the outer border. | Keep the outer border neutral and reserve a 1px inner gutter in layout for selection. The handle may cover the outer pixel but cannot erase the active gutter; do not add a content overlay. `resizable/panel.rs` warns against `overflow_hidden` on panels — do not set it. |
| R4 | **Re-subscription churn**: title/stat subscriptions must follow the active leaf. | Central `SpaceActivated` event; drop+recreate the `Subscription` on activation (06 §2). |
| R5 | **`ResizableState` identity**: recreating states on re-render loses sizes. | One `Entity<ResizableState>` per `Split` node, created once, reused via `with_state` (05 §1). |
| R6 | **Grafting a split source on drop** (source tab itself split). | MVP moves only the source's active leaf (03 §5); full-subtree graft deferred. |
| R7 | **Zoom + split interaction**. | Zoom is `TabPanel`-level; the whole tree zooms as one — no special handling, but test it. |

## 4. Resolved decisions (previously open)

All five were confirmed with the requester:

1. **Active after split** → activate the **new empty Space** (02 §1.6).
2. **Border style** → a neutral one-pixel outer border plus a layout-reserved one-pixel
   inner gutter; active selection changes the gutter color without adding a content
   overlay (05 §2–§3).
3. **"New Terminal Here"** → **included in the MVP** (empty-Space menu spawns a local
   shell in place) (04 §3).
4. **Keyboard shortcuts** for Split / Close Space → **deferred** (post-MVP); MVP uses
   the context menu only (04 §5).
5. **Drop onto a non-empty Space** → **not allowed**. Only **empty** Spaces are drop
   targets; there is no edge-aware split-on-drop (03 §3–§4).

## 5. Testing notes

- **Pure tree ops** (`space/ops.rs`) are unit-testable without GPUI: assert tree
  shape after `split`, collapse invariants after `close`, and active-leaf selection.
  This is where most correctness lives — cover it well (mirrors how `core`/`local`
  are tested today).
- **Render/DnD** need manual verification in `cargo run -p oneterm-app`: split in all
  four directions, nested splits, resize handles, active highlight follows focus,
  drag a second tab into an empty Space (source tab closes), close Spaces back down
  to a single terminal.

## 6. Future: persistence (deferred)

Not in the MVP (confirmed). If added later: serialize the tree shape (axis, child
order, sizes) into `docks.json` alongside the dock layout, and on restore rebuild the
tree with **empty** leaves (sessions can't persist), or re-spawn local shells for
`Terminal` leaves. The active-Space id would be restored too. This mirrors the
existing dock persistence in `layout/workspace/persistence.rs`.
