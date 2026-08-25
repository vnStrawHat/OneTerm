# 02 — Split, Close Space, and tree collapse

> Part of [Terminal Split design](../terminal-split.md). The operations that
> mutate the Space tree and keep it well-formed.

## 1. Split

`SpaceTree::split(target: SpaceId, dir: SplitDir, window, cx)`

Steps:

1. Find the `Leaf` with `id == target`. (Split only ever targets a leaf — the
   active Space, see [04](04-context-menu.md).)
2. Allocate a new `SpaceId` for a new **`Empty`** leaf.
3. Let `axis = dir.axis()`.
4. **Flatten when possible.** If the target leaf's *parent* is a `Split` whose
   `axis == axis`, insert the new empty leaf as a sibling directly into that parent
   (before/after the target per `dir.new_after()`), reusing the parent's
   `ResizableState`. This keeps splits in the same direction flat (N panes in one
   `ResizablePanelGroup`).
5. **Otherwise nest.** Replace the target leaf in place with a new `Split { axis,
   children, state: cx.new(|_| ResizableState::default()) }`, where `children` is
   `[existing_leaf, new_empty]` or `[new_empty, existing_leaf]` per `dir.new_after()`.
6. Set the new empty leaf as **active** (`active = new_id`) and focus it, so the
   next action (drag a tab in, or split again) targets it. *(Alternative: keep the
   existing terminal active. Chosen: activate the new empty Space, because the user
   just asked to create it and will typically fill it next. This is an easy tweak if
   feedback differs.)*
7. `cx.notify()` on the panel to re-render.

Splitting an **empty** leaf is allowed and behaves the same (an empty leaf becomes a
split of two empty leaves) — harmless, and simpler than special-casing it.

## 2. Close Space

`SpaceTree::close(target: SpaceId, window, cx) -> CloseOutcome`

Steps:

1. Find the target leaf.
2. If its content is `Terminal(view)`, close the session:
   `view.read(cx).session.read(cx).close()` (same call the tab close performs today).
   Drop the view entity so it is released.
3. Remove the leaf from its parent `Split`'s `children`.
4. **Collapse** (keep the tree well-formed) — see §3.
5. Choose a new **active** leaf (§4) and focus it.
6. Return a `CloseOutcome`:

```rust
pub enum CloseOutcome {
    /// A Space was removed; the tab still has ≥ 1 Space.
    Removed,
    /// The closed Space was the last one → reset the final tab or remove a tab with siblings.
    LastSpaceClosed,
}
```

`TerminalPanel` maps `LastSpaceClosed` through its terminal-tab close policy. When
sibling tabs exist, the existing `TabPanel::remove_panel` path removes this tab. When
this is the final terminal tab, its sessions are shut down and its tree is replaced by
a single empty placeholder, preserving the tab bar and `+` New Terminal menu. See
[06](06-integration.md).

## 3. Collapse rules (invariants)

After any removal, the tree must satisfy:

- **No split with fewer than 2 children.** If a `Split` ends up with exactly one
  child after a removal, replace the `Split` node with that single child (splice it
  into the grandparent). Its `ResizableState` is dropped.
- **Recursively.** Splicing may cascade (a split-of-splits collapsing upward).
- **Flatten same-axis nesting (optional cleanup).** If, after splicing, a child
  `Split` has the same `axis` as its new parent `Split`, its children may be merged
  into the parent. Not required for correctness; keeps the tree tidy and matches the
  flatten-on-split rule (§1.4).

Consequence (requirement R "revert to single terminal"): when only one leaf remains,
`root` **is** that `Leaf` (no `Split` wrapper), so the panel renders a plain
terminal with **no Space borders** ([05](05-rendering-theme.md)).

## 4. Active-Space selection after close

When the active Space is the one being closed, pick the next active leaf
deterministically:

1. Prefer the **previous sibling** in the parent split; else the **next sibling**.
2. If the parent collapsed, walk to the nearest remaining leaf in tree order.
3. If a chosen leaf is itself a `Split` after collapse, descend to its first leaf.

The newly active leaf is focused (`focus_handle.focus(window, cx)`); if it is a
`Terminal`, its view receives focus; if `Empty`, the placeholder wrapper does.

## 5. Edge cases

| Case | Behavior |
|---|---|
| Split target is empty | Allowed; empty leaf → split of two empty leaves. |
| Close an empty Space | Removes it, no session to close; collapse as usual. |
| Close the only Space | `LastSpaceClosed` → remove the tab when siblings exist; otherwise retain it as one empty placeholder. |
| Session exits on its own (`Exited`) inside a Space | The Space stays open showing the terminal's exit state, exactly as a single terminal does today; the user closes it via Close Space. *(No auto-close — matches current tab behavior.)* |
| Zoom (tab fullscreen) while split | Unchanged — zoom is a `TabPanel`-level concern; the whole tab (with its tree) zooms as one unit. |

## 6. Active-Space tracking source of truth

`SpaceTree.active` is the single source of truth. It is updated by:

- **Split** — activates the new empty leaf (§1.6).
- **Close** — activates a surviving leaf (§4).
- **Focus / click** — when a leaf's terminal (or placeholder) gains focus, the panel
  sets `active` to that leaf id and re-renders so the highlight border moves
  ([05](05-rendering-theme.md), [06](06-integration.md)).
- **Drop** — filling an empty Space activates it ([03](03-drag-drop.md)).
