# 05 — Rendering, borders, and the active-Space highlight

> Part of [Terminal Split design](../terminal-split.md). How the Space tree is
> painted: nested resizables, one-pixel frames, active highlight, placeholder.

## 1. Rendering the tree

`TerminalPanel::render` delegates the body to a recursive render of `SpaceTree`
(proposed `views/terminal/space/render.rs`):

```rust
fn render_node(node: &SpaceNode, active: SpaceId, panel: WeakEntity<TerminalPanel>,
               window: &mut Window, cx: &mut App) -> AnyElement {
    match node {
        SpaceNode::Leaf(leaf) => render_leaf(leaf, active, panel, window, cx),
        SpaceNode::Split(split) => {
            let group = if split.axis.is_horizontal() {
                h_resizable(("space-split", /*stable id*/))
            } else {
                v_resizable(("space-split", /*stable id*/))
            }
            .with_state(&split.state);        // reuse the node's ResizableState

            group.children(
                split.children.iter().map(|child|
                    resizable_panel().child(render_node(child, active, panel.clone(), window, cx))
                )
            ).into_any_element()
        }
    }
}
```

- **Single-terminal fast path.** When `root` is a bare `Leaf` (no split), render the
  terminal view directly with **no** Space chrome — the tab looks exactly like today
  (requirement: revert to plain single terminal).
- **Stable ids.** Give each split group and panel a stable `ElementId` derived from
  `SpaceId`s so `ResizableState` and hitboxes survive re-renders.

## 2. The Space frame

Each leaf has a neutral **one-pixel outer border** and a **one-pixel layout-reserved
inner gutter**. At a shared edge, the resizable component may paint its own one-pixel
resize handle over the outer border. The inner gutter remains visible, so the selected
frame is complete without overlaying terminal content.

```rust
// Each Space leaf wrapper:
div()
    .id(("space", leaf.id.0))
    .size_full()
    .border_1()
    .border_color(cx.theme().border)
    .p(px(1.))
    .bg(if leaf.id == active {
        cx.theme().table_active_border
    } else {
        cx.theme().border
    })
    .child(render_content(leaf, …))
```

Do not hardcode colors. `border` and `table_active_border` are existing theme tokens;
`table_active_border` is also used for the active-tab highlight and keeps the selected
state visually consistent.

## 3. Active-Space highlight

Requirement R8: the active Space's border is distinct/prominent.

- Active leaf → `border_color(cx.theme().table_active_border)` (accent).
- Inactive leaves → `border_color(cx.theme().border)` (muted).
- The highlight moves whenever `SpaceTree.active` changes (split / close / focus /
  drop — [02](02-split-and-close.md) §6). `active` is captured at render time, so a
  single `cx.notify()` repaints all leaves with the correct color.
- The layout-reserved inner gutter is the active highlight. Its padding reduces the
  child's bounds, so the highlight never paints over terminal glyphs.
- Keep the outer border neutral: the resize handle can paint over a shared outer edge.
  The inner gutter provides the uninterrupted four-sided selection frame.
- Do not append an absolute inner ring after the content: later siblings paint above
  earlier siblings and such an overlay can cover terminal glyphs at the pane edges.

## 4. Focus → active

Clicking anywhere in a Space makes it active:

- **Terminal leaf**: the `LocalTerminalView` already takes focus on click; the panel
  observes focus (or the view emits an event) and sets `tree.active = leaf.id`. A
  lightweight approach: wrap each leaf in a div with `.on_mouse_down(MouseButton::Left,
  …)` (capture phase) that sets active before the terminal consumes the event.
- **Empty leaf**: the placeholder wrapper holds a `FocusHandle`; clicking focuses it
  and sets active.

## 5. Empty placeholder

Requirement R4: a newly split Space is empty and shows placeholder text.

`render_content` for `SpaceContent::Empty`:

```rust
v_flex()
    .size_full()
    .items_center()
    .justify_center()
    .gap_2()
    .bg(cx.theme().background)
    .text_color(cx.theme().muted_foreground)
    .child(Icon::new(IconName::SquareTerminal)) // or AppIcon::Terminal
    .child("Drag a terminal tab here")
    .child("or right-click to split")
    // context menu for empty Space (see 04 §3)
```

The placeholder is the **only** drop target for `DragTerminalTab` — terminal leaves
are not droppable ([03](03-drag-drop.md) §3). It also carries the empty-Space context
menu, including **New Terminal Here** ([04](04-context-menu.md) §3).

## 6. Resize behavior

- Dragging the resize handle resizes adjacent Spaces via `ResizableState`
  (`resize_panel_at_handle` — already implemented in the primitive). No custom resize
  math needed.
- `ResizableState` re-distributes proportionally when the tab/window resizes
  (`adjust_to_container_size`), so Spaces keep their ratios.
- Each terminal view already resizes its PTY on layout change (existing
  resize-on-layout in the terminal element) — splitting just gives it a smaller
  bounds; no extra wiring.

## 7. Theme tokens used

| Purpose | Token |
|---|---|
| Inactive Space border | `cx.theme().border` |
| Active Space border | `cx.theme().table_active_border` |
| Placeholder text | `cx.theme().muted_foreground` |
| Space / placeholder background | `cx.theme().background` |
| Drop hover affordance | `cx.theme().tokens.drop_target` |
