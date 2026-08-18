# 03 — Drag a Terminal Tab into a Space

> Part of [Terminal Split design](../terminal-split.md). The trickiest part of the
> feature. Read the constraint (§1) first — it drives the whole approach.

## 1. The constraint: `DragPanel` is `pub(crate)`

gpui-component's `TabPanel` already implements tab drag-and-drop. When a tab is
dragged, the payload it emits is:

```rust
// reference/gpui-component/crates/ui/src/dock/tab_panel.rs
#[derive(Clone)]
pub(crate) struct DragPanel {          // ← pub(crate): NOT visible outside the crate
    pub(crate) panel: Arc<dyn PanelView>,
    pub(crate) tab_panel: Entity<TabPanel>,
}
```

Even though `dock/mod.rs` does `pub use tab_panel::*`, a `pub(crate)` item stays
crate-private. **The `ui` crate cannot name `DragPanel`**, so it cannot write
`.drag_over::<DragPanel>()` / `.on_drop(|drag: &DragPanel| …)` to intercept the
dock's built-in tab drag. We must therefore provide our **own** drag payload.

Dropping onto gpui-component's own machinery (`add_panel_at` + `will_split_placement`)
is also rejected: that splits at the **DockArea/StackPanel** level (new tab-panel
with its own tab strip) — the "new dock/tab" the requirements forbid.

## 2. Chosen approach: custom drag payload on the tab title we render

`TerminalPanel::title()` (in `crates/terminal-view/src/panel/terminal_panel.rs`) renders the tab's title
element **ourselves**. We attach our own `on_drag` there with a public payload:

```rust
/// Public drag payload for moving a Terminal Tab into a Space.
/// Defined in ui (e.g. views/terminal/space/drag.rs) so both the drag source
/// (tab title) and the drop target (a Space) can name it.
#[derive(Clone)]
pub struct DragTerminalTab {
    /// The source terminal panel being dragged.
    pub panel: WeakEntity<TerminalPanel>,
    /// The TabPanel the source lives in (to remove it on a successful move).
    pub tab_panel: WeakEntity<TabPanel>,
}
impl Render for DragTerminalTab { /* small drag preview: the tab label */ }
```

Wire-up on the title element (inside the `h_flex().id("tab-title")` we already build):

```rust
.on_drag(
    DragTerminalTab { panel: cx.entity().downgrade(), tab_panel: /* weak */ },
    |drag, _pos, _win, cx| { cx.stop_propagation(); cx.new(|_| drag.clone()) },
)
```

`cx.stop_propagation()` in the drag handler prevents the event from bubbling to
gpui-component's `Tab` wrapper, so **our** drag wins when the gesture starts on the
title. (This is the load-bearing assumption — verify it early; see §6 and
[07](07-roadmap-risks.md).)

## 3. Drop target: **only empty Spaces**

**Decision (confirmed):** only an **`Empty`** Space is a drop target. A Space that
already holds a terminal is **not** droppable — this keeps the interaction
unambiguous (no edge-aware split-on-drop) and matches the mental model "split to make
an empty slot, then fill it".

Only the empty placeholder wrapper registers the drop hooks:

```rust
// Empty-Space placeholder only (Terminal leaves do NOT get these):
placeholder_wrapper
    .id(("space", leaf.id.0))
    .drag_over::<DragTerminalTab>(|this, _drag, _win, cx| {
        // Visual affordance while hovering a valid drag over this empty Space.
        this.bg(cx.theme().tokens.drop_target)      // same token TabPanel uses
    })
    .on_drop(cx.listener(move |panel, drag: &DragTerminalTab, window, cx| {
        panel.handle_tab_drop(leaf_id, drag, window, cx);
    }))
```

A terminal leaf renders no `on_drop`/`drag_over`, so dragging a tab over it shows no
drop affordance and releasing there does nothing.

## 4. Drop handling = **move** (into an empty Space)

`TerminalPanel::handle_tab_drop(target: SpaceId, drag, window, cx)` — `target` is
always an `Empty` leaf (§3):

1. `let Some(src) = drag.panel.upgrade() else { return };`
2. **No-op guard**: if `src == cx.entity()` and the tab has no split (single Space),
   dropping onto itself does nothing.
3. **Extract the source content.** Read the source panel's active terminal view (or,
   if the source itself is split, its whole subtree root — see §5). For the MVP,
   move the source's **active terminal leaf's view** (`Entity<LocalTerminalView>`).
4. **Fill the empty target:** `tree.fill_empty(target, view, cx)` replaces the
   placeholder with the moved terminal. (Because only empty Spaces are droppable,
   there is no split-on-drop / edge-detection to handle.)
5. **Remove the emptied source.** After moving the view out:
   - If the source panel now has no terminal leaves left, remove the source tab:
     `src_tab_panel.update(cx, |tp, cx| tp.remove_panel(Arc::new(src), window, cx))`.
   - If the source panel still has other Spaces (it was split), just collapse the
     emptied leaf there ([02](02-split-and-close.md) §3).
6. Set the target Space active + focus its terminal. `cx.notify()`.

Moving the **view entity** (not the session) means no reconnect and no session
churn — the running shell keeps going, only its parent changes.

## 5. Dragging a split tab (source has multiple Spaces)

The dragged source is a whole `TerminalPanel`, which may itself contain a Space
tree. Two options:

- **MVP**: move only the **source's active terminal leaf** (§4.3). Simple, matches
  "drag a tab" mental model when the source is a plain single terminal (the common
  case). If the source is split, its active pane is moved and the source keeps the
  rest.
- **Later**: graft the source's **entire subtree** into the target position (true
  "move the whole tab"). Requires re-parenting `ResizableState` nodes; deferred.

The MVP behavior is well-defined for the primary use case (source = single-terminal
tab): the source becomes empty → the source tab closes → its terminal now lives in
the target Space.

## 6. Why not just patch gpui-component?

Making `DragPanel` public in the vendored fork would let a Space accept the dock's
native tab drag directly. Rejected for the MVP because:

- The project is **rev-locked** to a specific gpui-component revision (see
  `docs/agents/dependencies.md`); local patches fight upgrades.
- Our own payload is self-contained and avoids coupling to gpui-component internals.

It remains a documented fallback if the `on_drag`-on-title approach proves unable to
beat the `Tab` wrapper's own drag in practice (§2, [07](07-roadmap-risks.md)).

## 7. Reference APIs used

| Need | API (public) | Source |
|---|---|---|
| Emit our drag payload | `InteractiveElement::on_drag` | gpui |
| Hover affordance | `.drag_over::<T>()` / `.group_drag_over::<T>()` | gpui-component styled ext |
| Handle drop | `.on_drop(cx.listener(|_, drag: &T, …| …))` | gpui |
| Remove source tab | `TabPanel::remove_panel` | `dock/tab_panel.rs` (pub) |
| Read source active panel | `TabPanel::active_panel` | `dock/tab_panel.rs` (pub) |
| Drop-target color token | `cx.theme().tokens.drop_target` | theme (used by TabPanel) |
