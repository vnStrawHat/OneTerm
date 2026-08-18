# 06 — Integration points

> Part of [Terminal Split design](../terminal-split.md). Everywhere the existing
> code assumes "one `TerminalPanel` == one terminal view" and must now go through
> the **active Space**.

## 1. `TerminalPanel` field change

`views/terminal/panel.rs`:

```rust
pub struct TerminalPanel {
    tree: SpaceTree,                 // was: view: Entity<LocalTerminalView>
    tab_panel: Option<WeakEntity<TabPanel>>,
    is_active: bool,
    tab_title: String,
    _title_sub: Subscription,        // now re-subscribed to the active leaf's view
    _settings_sub: Subscription,
}
```

Constructors:

- `TerminalPanel::open(PanelSpec::DefaultShell { .. } | PanelSpec::Shell(kind), …)` — build a
  `SpaceTree` with a single `Terminal` leaf wrapping the spawned local session view
  (a one-leaf tree; the tree starts empty when the spawn fails).
- `TerminalPanel::open(PanelSpec::Session { session, title, duplicate_config }, …)` — same,
  seeded with the provided `Box<dyn TerminalSession>` (SSH connect, duplicate).

## 2. Methods that must retarget to the active Space

These currently read `self.view`; they must read `self.tree.active_terminal(cx)`
(the active leaf's view), returning gracefully when the active Space is `Empty`:

| Method | Today | After |
|---|---|---|
| `view()` (Edit ▸ Find) | `&self.view` | active leaf's view (`Option`) |
| `network_stats(cx)` | `self.view…network_stats()` | active terminal's, or `None` if empty |
| `breadcrumb_label(cx)` | `self.view…session.cwd()` as display text | active terminal's, or `None` |
| `title()` live OSC title | `self.view…title()` | active terminal's title |
| `_title_sub` | subscribes to the one view | re-subscribe to the active leaf's view when active changes |

Because the active Space can change (focus/split/close/drop), the title subscription
must be **re-established** when `tree.active` moves to a different terminal leaf.
Simplest: subscribe to a panel-level `SpaceActivated` event and re-`cx.subscribe` the
new active view (dropping the old `Subscription`).

## 3. `Panel::set_active` (the SFTP / cwd / right-dock hook)

`set_active` currently reads `self.view.read(cx).session` to publish
`AppState.active_sftp / active_cwd_source / active_is_local`. After the change
it must read the **active leaf's** session:

```rust
fn set_active(&mut self, active: bool, window, cx) {
    // …mirror is_active as today…
    if active {
        let session = self.tree.active_terminal(cx).map(|v| v.read(cx).session.clone());
        let (sftp, cwd_source, is_local) = match session {
            Some(s) => { let s = s.read(cx); (s.sftp(), s.cwd_source(), s.kind().is_local()) }
            None => (None, None, true),   // empty active Space → treat as local/no-sftp
        };
        AppState::global(cx).update(cx, |st, cx| { /* set fields */ cx.notify(); });
    }
}
```

Additionally, when the **active Space changes within an already-active tab** (user
clicks another pane), the panel should re-publish these `AppState` fields so the
SFTP browser / status bar follow the focused Space — call the same update path from
the focus→active handler ([05](05-rendering-theme.md) §4), not only from
`set_active`.

## 4. Closing the last Space closes the tab

`close_space` maps `CloseOutcome::LastSpaceClosed` to the existing tab-close path:

```rust
if outcome == CloseOutcome::LastSpaceClosed {
    if let Some(tp) = self.tab_panel.as_ref().and_then(|w| w.upgrade()) {
        let panel: Arc<dyn PanelView> = Arc::new(cx.entity());
        tp.update(cx, |tp, cx| tp.remove_panel(panel, window, cx));
    }
}
```

This reuses the same `remove_panel` call the tab's × button and middle-click already
use (`panel.rs`).

## 5. Focus handling

`Focusable::focus_handle` currently delegates to the single view. After the change it
delegates to the **active leaf**:

```rust
fn focus_handle(&self, cx: &App) -> FocusHandle {
    self.tree.active_focus_handle(cx)   // active terminal view's, or placeholder's
}
```

So when the dock focuses the panel, focus lands on the active Space.

## 6. Workspace actions (optional shortcuts)

If keyboard shortcuts are added ([04](04-context-menu.md) §5), register actions on
the workspace (`layout/workspace/`) mirroring the existing `AddPanel` / `Find`
pattern in `workspace/actions.rs`, dispatching to the focused `TerminalPanel`. The
active `TerminalPanel` is resolved via the focused panel in the `DockArea` (same way
`Find` finds its target today).

## 7. What does NOT change

- `TabPanel`, `DockArea`, `StackPanel`, `docks.json` layout — untouched (no new
  dock/tab). The Space tree is invisible to the dock layer.
- The terminal view internals (`element/`, `cell/`, `theme/`, IME, mouse) — a leaf
  hosts the exact same `LocalTerminalView`.
- Zoom, persistence of dock layout, statusbar structure — unchanged (the statusbar
  just reads the active Space instead of the single view, §2).
