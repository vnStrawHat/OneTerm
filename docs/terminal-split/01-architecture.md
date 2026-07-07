# 01 — Architecture: the Space pane tree

> Part of [Terminal Split design](../terminal-split.md). Defines the data model
> that lives inside a `TerminalPanel` and how it maps to gpui-component primitives.

## 1. Where it lives

The split lives **entirely inside a single `TerminalPanel`** (crate `ui`,
`views/terminal/panel.rs`). Nothing at the `DockArea` / `TabPanel` /
`StackPanel` level changes — that is what "no new dock, no new tab" means.

Before:

```
TerminalPanel
└── view: Entity<LocalTerminalView>          // exactly one terminal
```

After:

```
TerminalPanel
├── tree: SpaceTree                           // the pane tree (was: a single view)
└── active_leaf: SpaceId                      // which Space is focused/active
```

`TerminalPanel` keeps all its existing `Panel` responsibilities (tab title, close
button, `set_active`, zoom). Only the **body** it renders changes: instead of one
`LocalTerminalView`, it renders the Space tree (see [05](05-rendering-theme.md)).

## 2. The tree model

A binary pane tree. Internal nodes split along an axis; leaves are Spaces.

```rust
/// Stable identity for a Space leaf (for active tracking, focus, drop targeting).
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct SpaceId(u64);

/// One node of the Space tree.
pub enum SpaceNode {
    /// A leaf Space — holds a terminal or is empty.
    Leaf(SpaceLeaf),
    /// A split of two-or-more children along one axis.
    Split(SpaceSplit),
}

pub struct SpaceLeaf {
    pub id: SpaceId,
    pub content: SpaceContent,
    pub focus: FocusHandle,           // focus target for the placeholder / wrapper
}

pub enum SpaceContent {
    /// A live terminal view (local or SSH — both are `LocalTerminalView`).
    Terminal(Entity<LocalTerminalView>),
    /// Empty Space: renders placeholder text (see 05).
    Empty,
}

pub struct SpaceSplit {
    /// Horizontal split = children laid out left→right (Split Right/Left).
    /// Vertical split   = children laid out top→bottom (Split Up/Down).
    pub axis: Axis,                   // gpui::Axis
    pub children: Vec<SpaceNode>,
    /// Sizes/handles for this split level, one entity per split node.
    pub state: Entity<ResizableState>,
}
```

Notes:

- **Binary at creation, N-ary in storage.** A split is created with two children,
  but we store `children: Vec<_>` so that splitting a leaf *in the same direction as
  its parent* can be flattened into the parent (a common tmux-like nicety and a good
  fit for `ResizablePanelGroup`, which already handles N panels). Splitting in the
  perpendicular direction creates a nested split node. This keeps the tree shallow.
- **One `ResizableState` per `Split` node.** `h_resizable`/`v_resizable` own a
  `ResizableState` (sizes + handles). We create/keep one per split so resize state
  is stable across re-renders. See [05](05-rendering-theme.md).
- **`SpaceId` is stable** for the lifetime of a leaf. It is used to mark the active
  Space, route focus, and identify drop targets.

### 2.1 `SpaceTree` wrapper

```rust
pub struct SpaceTree {
    root: SpaceNode,
    next_id: u64,           // SpaceId allocator
    active: SpaceId,        // active leaf
}
```

`SpaceTree` exposes the operations the panel needs (see [02](02-split-and-close.md)):

- `split(target: SpaceId, dir: SplitDir, window, cx)` — split a leaf.
- `close(target: SpaceId, window, cx) -> CloseOutcome` — close a leaf + collapse.
- `set_active(id: SpaceId)` / `active() -> SpaceId`.
- `active_terminal(cx) -> Option<Entity<LocalTerminalView>>` — the active leaf's
  terminal, if any (used by `set_active` integration, [06](06-integration.md)).
- `leaf_count() -> usize` — used to decide "single terminal vs split" rendering and
  whether **Close Space** appears in the menu ([04](04-context-menu.md)).
- `fill_empty(target: SpaceId, view: Entity<LocalTerminalView>, cx)` — used by drop
  ([03](03-drag-drop.md)).

## 3. Mapping direction → axis + child order

| Menu item | Axis | New empty child goes | Existing terminal stays |
|---|---|---|---|
| Split Right | Horizontal | to the **right** (after) | left |
| Split Left  | Horizontal | to the **left** (before) | right |
| Split Down  | Vertical   | **below** (after) | top |
| Split Up    | Vertical   | **above** (before) | bottom |

```rust
pub enum SplitDir { Right, Left, Up, Down }

impl SplitDir {
    fn axis(self) -> Axis { /* Right/Left → Horizontal, Up/Down → Vertical */ }
    /// Whether the new (empty) child is inserted after the existing one.
    fn new_after(self) -> bool { matches!(self, SplitDir::Right | SplitDir::Down) }
}
```

## 4. Why a custom tree instead of reusing dock split / Tiles

We deliberately do **not** reuse gpui-component's dock-level split machinery:

- `DockItem::h_split` / `v_split` / `StackPanel` create split **panels within the
  DockArea**, each carrying its own tab strip — that is exactly the "new dock/tab"
  the requirements forbid.
- `Tiles` is a free-floating, draggable canvas of panels — heavier than needed and
  again tab/panel-oriented.

Instead we compose the lightweight, public **`resizable`** primitives
(`h_resizable` / `v_resizable` / `resizable_panel` + `ResizableState`) directly in
the panel body. They give us resizable panes with a single shared handle style,
with no tabs, no docks, and full control over borders and the placeholder — exactly
the "Space" concept. See [05](05-rendering-theme.md) for the render mapping.

## 5. Session lifecycle

- Splitting **does not** spawn a new session — the new Space is `Empty`. A session
  only exists once a terminal is dropped into (or, later, created in) a Space.
- Each `Terminal` leaf owns its `LocalTerminalView` + `Box<dyn TerminalSession>`
  exactly as today. The tree just changes *where* the view is parented.
- Closing a `Terminal` Space closes that session (`TerminalSession::close`), same as
  closing a tab does today ([02](02-split-and-close.md)).
- Dropping a tab into a Space **moves** the existing view entity (no new session,
  no reconnect) — see [03](03-drag-drop.md).
