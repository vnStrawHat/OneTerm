# High-Level Design: Right dock follows the tab kind

Intake: IN-0054
Lane: normal
Date: 2026-10-15

## Idea

The right dock now holds one thing, the SSH Client panel (Session list + SFTP browser), and it
is mostly useful while working over SSH. The title bar's `SSH Client | None` control is gone
(owner ruling). Instead the dock **follows the active Terminal Tab's kind**: it opens on an SSH
tab and closes on a local-shell tab, and whatever the user chooses with a dock button, or with
the new `Ctrl+Shift+B`, is remembered **per kind** and persisted.

Owner rulings (2026-10-15):

| Question | Ruling |
| --- | --- |
| Mechanism | Follow the tab kind, remember the user's choice per kind. |
| Defaults | Local tab: closed. SSH tab: open. |
| Shortcut | Yes: a bindable "Toggle Right Dock", default `ctrl-shift-b`. |
| Saved `right_dock_mode` | Ignored; everyone starts from the new defaults. |

## Diagram

```text
TerminalPanel::publish_active_session --> AppState.active_workspaces[dock].active_is_local
                                                   |
                                    (observe; act only when the kind changes)
                                                   v
OneTermWorkspace.apply_right_dock_for_kind  -->  set_right_dock_open(UiConfig.right_dock[kind])

status-bar dock button ┐
tab-bar dock button    ├─> DockArea open state changes
ToggleRightDock action ┘            |
                        (observe dock area; open != remembered[current kind])
                                    v
                UiConfig.right_dock[current kind] = open; persist ui_config.json
```

## UI Wireframe

```text
before:  >_ OneTerm                               [ SSH Client | None ]  _ □ x
after:   >_ OneTerm                                                      _ □ x

local tab selected                       SSH tab selected
+---------------------------------+      +----------------------+----------+
| [pwsh] [ssh:web]                |      | [pwsh] [ssh:web]     | Session  |
|                                 |      |                      |----------|
|  (terminal, full width)         |      |  (terminal)          | SFTP     |
|                                 |      |                      |          |
+---------------------------------+      +----------------------+----------+
| ...                       [▣]   |      | ...                   [▣]       |
  dock button still toggles; the choice is remembered for this tab kind
```

## Data Flow

1. A tab becomes active (selection, new tab, Space change): the panel publishes
   `active_is_local` to `AppState` (existing).
2. The workspace observes `AppState`. When its own workspace's kind differs from the last kind it
   applied, it records the new kind and opens or closes the dock to that kind's remembered
   choice. Same kind: nothing happens, so a manual choice made on this tab stays.
3. Any open/close of the dock (status-bar button, tab-bar dock button, `Ctrl+Shift+B`) reaches the
   existing dock-area observer. If the dock's open state differs from the remembered choice for the
   current kind, the user changed it: store it in `UiConfig.right_dock` and persist. An automatic
   change made in step 2 already equals the remembered choice, so it is never recorded.
4. Start-up: after the layout is built, the dock is set for the kind of the active tab (a local
   shell, since tabs are not restored), overriding the open state `docks.json` carried. The width
   still comes from `docks.json`.
5. An elevated window has no right dock; every step above is a no-op there.

### Persistence

`ui_config.json` gains `right_dock: { "local": bool, "ssh": bool }` (default `false` / `true`,
omitted while it equals the default). `right_dock_mode` is removed: an old file keeps loading (the
field is unknown and ignored) and loses it on the next write. No schema bump.

### Removed

The title bar's segmented control, `SetRightDockMode`, `oneterm_core::RightDockMode`,
`panel_names::right_dock_panel_name`, and the mode-switch logic in the workspace
(`switch_right_dock_mode`, `apply_right_dock_mode`, `reopen_or_rebuild`, `right_dock_mode_for`,
`sync_right_dock_mode`). The SSH Client panel is the right dock's only content and is always built.

## Detail Design

- [x] Detail design: not needed
- Reason: one observer pair in the workspace, one settings field, one action; the data flow above
  is the design.
