# 03 — Drag a Terminal Tab into a Space

> Part of [Terminal Split design](../terminal-split.md). The trickiest part of the
> feature. Read the constraint (§1) first — it drives the whole approach.

## 1. Why terminal Spaces use a custom drag payload

GPUI Kit 0.6 exposes the dock's native `DragPanel`, but that payload represents a dock-panel move. A terminal Space drop moves one terminal view between `SpaceTree` leaves inside a `TerminalPanel`, which is a different ownership operation. OneTerm therefore uses a terminal-specific payload containing the source panel and drag-preview title.

Using the dock's own drop machinery is intentionally rejected: it operates at the DockArea/tab-group level and would create or rearrange dock tabs rather than fill an empty terminal Space.

## 2. Chosen approach: custom drag payload on the tab title we render

`TerminalPanel::title()` (in `crates/terminal-view/src/panel/terminal_panel.rs`) renders the tab's title
element **ourselves**. We attach our own `on_drag` there with a public payload:

```rust
/// Drag payload for moving a Terminal Tab into a Space.
#[derive(Clone)]
pub struct DragTerminalTab {
    /// The source terminal panel being dragged.
    pub panel: WeakEntity<TerminalPanel>,
    /// The label rendered in the drag preview.
    pub title: SharedString,
}
impl Render for DragTerminalTab { /* small drag preview: the tab label */ }
```

Wire-up on the title element (inside the `h_flex().id("tab-title")` we already build):

```rust
.on_drag(
    DragTerminalTab {
        panel: cx.entity().downgrade(),
        title: tab_label.into(),
    },
    |drag, _pos, _win, cx| { cx.stop_propagation(); cx.new(|_| drag.clone()) },
)
```

`cx.stop_propagation()` keeps this Space-tree gesture separate from the surrounding dock tab gesture.

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
        this.bg(cx.theme().tokens.drop_target)      // same token the dock skin uses
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

## 6. Why not use the dock's `DragPanel`?

GPUI Kit 0.6 exposes `DragPanel`, but it represents a dock-level panel move. Accepting
it would couple Space ownership to the dock's tab/group editing rules and could move a
whole `TerminalPanel` instead of one active terminal leaf. `DragTerminalTab` is a
stable OneTerm payload with exactly the source and preview data this operation needs.

## 7. Reference APIs used

| Need | API (public) | Source |
|---|---|---|
| Emit our drag payload | `InteractiveElement::on_drag` | gpui |
| Hover affordance | `.drag_over::<T>()` / `.group_drag_over::<T>()` | gpui-component styled ext |
| Handle drop | `.on_drop(cx.listener(|_, drag: &T, …| …))` | gpui |
| Remove an emptied source tab | `DockArea::remove_panel` | `gpui_base::dock::DockArea` |
| Inspect sibling tabs | `TabGroup::panels` | `gpui_component::dock::TabGroup` |
| Drop-target color token | `cx.theme().tokens.drop_target` | GPUI Kit theme |
