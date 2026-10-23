# 03 — Drag a Terminal Tab into a Space

> Part of [Terminal Split design](../terminal-split.md). The trickiest part of the
> feature. Read the constraint (§1) first — it drives the whole approach.

## 1. The drag is the dock's own: `DragPanel`

A Terminal Tab is dragged by the dock's native `DragPanel`, wherever it is drawn: the horizontal
strip's tab (its title included), an expanded rail row and a collapsed rail cell all start the
kit's drag, and that one gesture both **reorders** the tab (dropped on another tab, a row, a cell
or the space below the last one) and **moves its terminal into an empty Space** (dropped on one).

*History.* Until `US-0202` (2026-10-23) the strip's tab title started a second, OneTerm-owned
payload, `DragTerminalTab` (the source panel and a preview title), and an empty Space accepted that
payload only. The title therefore moved the tab into a Space but reordered nothing, and only the
tab's padding started the reorder: a drag worked or not by where the press landed (the same split
made the collapsed rail "sometimes reorder, sometimes not", `US-0198` rework 8). The payload was
deleted once nothing started it; the empty Space resolves the dock's drag instead (§3, §6).

## 2. What starts it

Nothing in OneTerm starts a drag of its own on a tab. `TerminalPanel::title()` renders the tab's
row (`render_tab_strip`) with **no** `on_drag`; the kit's `Tab` (the strip) or the rail's row (the
vertical layout) is the element that starts `DragPanel`, so a press anywhere on the row, the
label, the chips and the status icon included, is the reorder drag. Only the `x` close button
stops the press.

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
    .drag_over::<DragPanel>(|this, drag, _win, cx| {
        // Visual affordance while a *terminal tab's* drag hovers this empty Space.
        if TerminalPanel::dragged_by(drag, cx).is_some() {
            this.bg(cx.theme().tokens.drop_target)      // same token the dock skin uses
        } else {
            this
        }
    })
    .on_drop(move |drag: &DragPanel, window, cx| {
        panel.update(cx, |p, cx| p.handle_panel_drop(leaf_id, drag, window, cx));
    })
```

A terminal leaf renders no `on_drop`/`drag_over`, so dragging a tab over it shows no
drop affordance and releasing there does nothing.

## 4. Drop handling = **move** (into an empty Space)

`TerminalPanel::handle_panel_drop(target: SpaceId, drag, window, cx)` — `target` is
always an `Empty` leaf (§3):

1. `let Some(src) = TerminalPanel::dragged_by(drag, cx) else { return };` (the panel behind the
   drag's id, found through the workspace dock area; only a `TerminalPanel` qualifies).
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

## 6. The dock's `DragPanel`, and the rule that terminal content never splits the dock

GPUI Kit exposes `DragPanel`, which represents a dock-level panel move. Accepting it blindly in a
Space would couple Space ownership to the dock's tab/group editing rules and could move a whole
`TerminalPanel` instead of one active terminal leaf. The risk is handled, not waived (history:
this section once kept a private payload, `DragTerminalTab`, to avoid it; `US-0196` rework 9
and `US-0202` replaced it):
`TerminalPanel::dragged_by` resolves the drag's panel id through the workspace dock area and
accepts it only if it is a `TerminalPanel` (an SFTP or a session panel is neither highlighted nor
moved), and `handle_panel_drop` runs the very move of §4 on the resolved tab, so only its active
terminal leaf moves. Still only an `Empty` Space is a target.

**Amended 2026-10-23** (`US-0196` owner rework 9): a vertical-rail row is dragged whole, as the
dock's `DragPanel` (that is how a row reorders). Because an empty Space's drop is innermost, it also bypasses the dock's own handling of
that drag (merge or split of the centre). **Amended again 2026-10-23 (owner ruling, "no split by
drop"): terminal content never splits or merges the dock by a drop.** The `TerminalPanel` root
consumes every `DragPanel` released on its content with a handler that does nothing (gpui
hands a drop to the consumer closest to the pointer and takes the drag before asking anything,
so a `can_drop` predicate cannot decline and the dock's handler never runs), and an empty Space
is the only inside consumer that acts. So: a terminal tab's drag (an expanded row, a collapsed
cell, or the strip's tab, all `DragPanel`) dropped on an empty Space moves the terminal
in, on anything else of a terminal tab's content does nothing; another panel kind's drag (SFTP,
session) dropped on a terminal tab's content, empty Space or not, does nothing. This is the
rule, not a defect: a swallowed drop beats an accidental second group with its own rail.
OneTerm's tab-group skin also draws no split placeholder while the group displays a terminal
(it would promise a split the drop does not do), and the dock's own filter by an active drag
leaves none behind after a drop.

## 7. Reference APIs used

| Need | API (public) | Source |
|---|---|---|
| Emit our drag payload | `InteractiveElement::on_drag` | gpui |
| Hover affordance | `.drag_over::<T>()` / `.group_drag_over::<T>()` | gpui-component styled ext |
| Handle drop | `.on_drop(cx.listener(|_, drag: &T, …| …))` | gpui |
| Remove an emptied source tab | `DockArea::remove_panel` | `gpui_base::dock::DockArea` |
| Inspect sibling tabs | `TabGroup::panels` | `gpui_component::dock::TabGroup` |
| Drop-target color token | `cx.theme().tokens.drop_target` | GPUI Kit theme |
