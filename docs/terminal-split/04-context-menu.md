# 04 — Context menu changes

> Part of [Terminal Split design](../terminal-split.md). How the right-click menu
> gains Split R/L/U/D and Close Space.

## 1. Today

The terminal context menu is built in
`views/terminal/handlers/menu.rs::attach_context_menu`, attached to the terminal
view in `render/`. Current layout:

```
New Terminal
Duplicate Session
──────────
Copy
Paste
Select All
Clear
──────────
Close Terminal Tab
```

## 2. Target layout (terminal Space)

Right-clicking a Space that **contains a terminal**:

```
New Terminal
Duplicate Session
──────────
Split Right
Split Left
Split Up
Split Down
──────────
Copy
Paste
Select All
Clear
──────────
Close Terminal Tab
Close Space            ← only shown when the tab has > 1 Space
```

- **Duplicate Session** is directly below **New Terminal** and targets the terminal in the right-clicked Space. The same action is configurable under the **Terminal Context Menu** Key Bindings group and has no built-in default keystroke; keyboard invocation targets the active Space. A local source creates a new sibling tab immediately with the same complete shell launch configuration and the live cwd. An SSH source opens a prefilled authentication dialog, initially focuses the applicable password/passphrase field, requires credentials again, reconnects to the same endpoint/default remote shell, then requests the live cwd when known. If the source has not reported a cwd, the duplicate uses its shell/backend default directory: local launch metadata clears `cwd`, and SSH sends no `cd` command. The source tab and process remain unchanged.
- Duplicate metadata is non-secret. SSH password and private-key passphrase fields are never retained or prefilled. The duplicate authentication dialog does not offer **Save to SSH Sessions**; saving connection metadata remains exclusive to the normal SSH Quick Connect flow. See [`../decisions/0002-ssh-duplicate-auth.md`](../decisions/0002-ssh-duplicate-auth.md).
- **Split R/L/U/D** call `TerminalPanel::split_active(dir, window, cx)` →
  `SpaceTree::split(active, dir, …)` ([02](02-split-and-close.md) §1). The target is
  the Space that was right-clicked (which also becomes active on right-click).
- **Close Space** is placed **directly below Close Terminal Tab** (requirement R6).
  It calls `TerminalPanel::close_space(space_id, window, cx)`
  ([02](02-split-and-close.md) §2). It is **hidden when the tab has only one Space**
  (`tree.leaf_count() == 1`), because in that state "Close Space" == "Close Terminal
  Tab" and would be redundant.
- **Close Terminal Tab** keeps its current behavior (closes the whole tab, all
  Spaces). Unchanged.

## 3. Target layout (empty Space / placeholder)

Right-clicking an **empty** Space shows a reduced menu (no Copy/Paste/Clear — there
is no terminal):

```
New Terminal Here
Split Right
Split Left
Split Up
Split Down
──────────
Close Terminal Tab
Close Space
```

- **New Terminal Here** (confirmed for the MVP): spawns a `LocalSession` into the
  empty Space (same construction path as `TerminalPanel::new`) and replaces the
  placeholder with a `Terminal` leaf via `tree.fill_empty`. Lets a Space be filled
  without dragging a tab in.
- The empty-Space menu is a separate small builder attached to the placeholder
  element ([05](05-rendering-theme.md)), since the placeholder has no
  `TerminalSession` to bind Copy/Paste/etc. to.

## 4. Menu wiring

`attach_context_menu` gains parameters so items can target the right Space and know
whether Close Space should appear:

```rust
pub(crate) fn attach_context_menu<E>(
    div: E,
    session: Entity<Box<dyn TerminalSession>>,
    focus: FocusHandle,
    panel: WeakEntity<TerminalPanel>,   // NEW: dispatch split/close-space
    space_id: SpaceId,                  // NEW: which Space this menu targets
    can_close_space: bool,              // NEW: tree.leaf_count() > 1
) -> ContextMenu<E> { … }
```

Each Split / Close-Space item, on click:

```rust
move |_, window, cx| {
    if let Some(panel) = panel.upgrade() {
        panel.update(cx, |p, cx| p.split_active_at(space_id, SplitDir::Right, window, cx));
    }
    window.focus(&f, cx);
}
```

Reuse the existing `PopupMenuItem` + `.separator()` pattern already in
`menu.rs`. Split items may carry the direction icons from the `AppIcon` set (add
`arrow-right/left/up/down.svg` if desired — build.rs auto-generates the variants; see
`docs/agents/structure.md` §2).

## 5. Keyboard (optional, recommended)

Splitting benefits from shortcuts. These can be added as workspace actions
([06](06-integration.md)) and are **out of scope for the MVP menu work** but noted
so the action names are reserved:

| Action | Suggested binding |
|---|---|
| `SplitRight` | `ctrl-shift-d` (or `ctrl-alt-right`) |
| `SplitDown` | `ctrl-shift-e` |
| `CloseSpace` | `ctrl-shift-w` |

Bindings must go through the configurable key-binding system
(`views/settings/key_bindings.rs`), not hardcoded — consistent with the project's
existing approach.
