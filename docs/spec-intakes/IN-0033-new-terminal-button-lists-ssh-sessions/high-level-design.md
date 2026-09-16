# High-Level Design: New Terminal button lists SSH sessions

Intake: IN-0033
Lane: normal
Date: 2026-09-14

## Idea

The "+" button in the center tab bar is already a dropdown: clicking it opens a popup menu
offering the platform's local shells and "New SSH Session". This intake appends one section
to that menu listing the sessions saved in `ssh_session.json`, so a saved host is reachable
from the place the user already goes to open a terminal, instead of only from the SSH
Sessions panel in the right dock.

Nothing about how a session connects changes. The menu entry calls the same
`open_connect_dialog` the SSH Sessions panel calls, with the same saved-session id, and the
connected shell lands where that path already puts it — a new center terminal tab.

## Diagram

```text
crates/terminal-view (L3 feature)            crates/state (L2)            crates/session-ui (L3 feature)
+-------------------------------+     +---------------------------+     +-----------------------------+
| TerminalPanel::title_suffix   |     | WorkspaceCommands         |     | SshSessionStore (global)    |
|   Button "+" .dropdown_menu   |     |  saved_ssh_sessions  ---- | --> |   menu_entries(&entries)    |
|                               | --> |  open_saved_ssh_session - | --> |   get(id) + connect dialog  |
|   for (id, name) in entries   |     |  (fn pointers, installed  |     +--------------+--------------+
|     item(name).on_click(...)  |     |   by crates/app at start) |                    |
+-------------------------------+     +---------------------------+                    v
                                                                        open_connect_dialog(session, id)
                                                                                       |
                                                                            (existing, unchanged)
                                                                                       v
                                                                        TerminalPanel::open(PanelSpec::Session)
                                                                        + add_ssh_terminal_to_dock -> new center tab
```

Why the indirection: `oneterm-session-ui` already depends on `oneterm-terminal-view` (the
one same-layer edge `docs/agents/crate-dependency-rules.md` R5 permits, because opening an
SSH session builds a `TerminalPanel`). A direct `terminal-view -> session-ui` edge would be
a dependency cycle (R1) as well as a second cross-feature edge (R5). `WorkspaceCommands` in
`crates/state` is the registry that already exists for exactly this — `crates/terminal-view`
reads it today in `panel/duplicate.rs` to reach the SSH duplicate dialog — so this adds two
fn-pointer fields rather than a new mechanism (R10).

## UI Wireframe

The "+" button lives in the center tab bar's trailing control group, next to the zoom
control (`Panel::title_suffix`). The button itself does not change: it stays a plain
`Button` with an icon and no split half.

```text
+--------------------------------------------------------------------------+
| [ prod-web x ] [ local x ]                                    [ + ] [ ^ ] |
+--------------------------------------------------------------------------+
                                                                  |
                                    +-----------------------------+
                                    v
                            +------------------------------+
                            | Command Prompt               |   <- local shells, platform
                            | PowerShell                   |      specific, unchanged.
                            | PowerShell 7                 |      (Bash / Sh / Zsh on unix)
                            |------ SSH Sessions ---------|   <- labelled separator
                            | [] prod-web                  |   <- ungrouped sessions, store
                            | [] staging                   |      order, colour square + title
                            |- - - - - infra - - - - - - -|   <- dashed separator, group name
                            | [] db-01                     |   <- that group's sessions
                            | [] db-02                     |
                            |- - - - - - lab - - - - - - -|   <- next group, store order
                            | [] sandbox                   |
                            |------------------------------|
                            | New SSH Session              |   <- moved to the end
                            +------------------------------+
```

Empty state — nothing saved in `ssh_session.json` yet:

```text
                            +------------------------------+
                            | Command Prompt               |
                            | PowerShell                   |
                            | PowerShell 7                 |
                            |------ SSH Sessions ---------|
                            | No saved sessions            |   <- disabled hint, not clickable
                            |------------------------------|
                            | New SSH Session              |
                            +------------------------------+
```

Decisions this wireframe fixes:

- **Local shells stay first, and the menu above the new separator is byte-for-byte what it
  is today.** A user with no saved sessions sees one extra label and one disabled hint and
  nothing else changes, so the "open a local shell" path keeps its position and its
  muscle memory.
- **Button, not split button.** `Button::dropdown_menu` already opens the popup on a plain
  click, which is the simplest thing the kit supports and what the button does today. A
  split button would need a default action on the primary half; the menu's first entry
  already is that default, and halving the hit target of an `xsmall` icon button to save one
  click is a bad trade. Keyboard reach is unchanged: `Ctrl-T` (`AddPanel`) still opens a
  local terminal without touching the menu, and the popup itself is arrow-key navigable
  because it is the kit's standard `PopupMenu`.
- **Grouped, storage order, title only** (owner, 2026-09-15 acceptance rework; this
  reverses the intake's "flat" open decision). The ungrouped sessions come first, then one
  section per group in the order the groups first appear in `ssh_session.json` — the file's
  order, not the tree's alphabetical order. Rows carry the session title alone, so two
  sessions sharing a label are indistinguishable here; the owner accepted that, and the
  right dock's tree still shows `user@host:port`. A hand-edited entry with a blank label
  still falls back to `host:port` so no row is ever invisible.
- **Rows carry the session's colour square** (owner, 2026-09-16, `US-0110`). Each saved
  session row is drawn like the right dock's tree leaf: an 8px square in the session's own
  colour, `gap_2`, then the title. A session with no saved colour gets the same
  `SshSession::DEFAULT_COLOR_HEX` default the tree gives it, applied by `menu_entries` so
  the constant is read in one place. A row is a `PopupMenuItem::element` — the kit renders
  it with the same padding, height, hover and selection styling as a plain `Item`, and
  `is_clickable()` / `confirm()` treat it identically, so mouse and keyboard reach are
  unchanged.
- **The headings are separators that carry a centred label.** The kit's `PopupMenu` offers
  a plain `Separator` and a plain `Label` but nothing that is both, so the heading row is
  composed from `PopupMenuItem::element(...).disabled(true)`: a rule, the label text,
  another rule, with `border_dashed()` for a group heading and solid for "SSH Sessions".
- **No scrollbar until the menu needs one.** The kit caps the popup's height only when the
  menu is `scrollable`, and a scrollable menu always shows a scrollbar under this app's
  `ScrollbarMode::Always` theme. `scrollable` is therefore set only when the estimated row
  height exceeds that cap (`min(half the window, 450px)`), so the everyday menu carries no
  bar while a long saved list still gets the cap and the scrolling.
- **"New SSH Session" is last**, behind a plain separator, so the saved list sits directly
  under the shells where the owner looks for it.
- **Section label "SSH Sessions"** matches the panel the owner named, so the two surfaces
  read as the same list.
- **No colour or icon literals in the menu builder.** Headings and text take their colours
  from `cx.theme()` through the kit. A row's square is the session's own saved colour,
  data rather than a literal; where the saved hex will not parse the builder falls back to
  `cx.theme().accent`, and the "no colour saved" default lives with the session type in
  `crates/session-ui`, not in the menu.

## Data Flow

1. The user clicks "+". `Button::dropdown_menu`'s builder closure runs — it is an `Fn`, so
   it runs on **every** open and never caches a stale list.
2. The closure reads `oneterm_state::commands::commands(cx)` and calls
   `saved_ssh_sessions(cx)`.
3. That fn pointer resolves to `oneterm_session_ui::saved_ssh_sessions`, which reads the
   `SshSessionStore` global and maps its entries through `menu_entries` to
   `Vec<(String, Vec<(u64, String, String)>)>` — sections of `(group name, rows)`, the
   ungrouped rows under an empty group name first, each row the stable session id, its
   title, and its hex colour with the default already applied (`US-0110`). Primitives
   only: `crates/state` sits below `crates/session-ui` and must not name its types.
4. The closure appends the labelled "SSH Sessions" separator and then either the sections
   — ungrouped rows first, then a dashed labelled separator and its rows per group — or the
   disabled "No saved sessions" hint, and finally a plain separator and "New SSH Session".
5. Clicking a row calls `open_saved_ssh_session(id, window, cx)` with the **id**, not an
   index: the store's schema v2 gives every session a stable id precisely so a concurrent
   add or delete cannot retarget a pending UI action.
6. `oneterm_session_ui::open_saved_ssh_session` looks the id up in the store (a session
   deleted since the menu opened is simply a no-op) and calls the existing
   `open_connect_dialog(session, id, window, cx)`.
7. From there the flow is the one `docs/ssh-client-connect.md` already documents:
   credentials, jump chain, `SessionFactory::connect_ssh`, then `TerminalPanel::open` with
   `PanelSpec::Session` added to the center dock as a new tab.

## Detail Design

- [x] Detail design: not needed
- Reason: normal lane, one menu section, two fn-pointer fields, and no new state, schema or
  connect path. The wireframe and the data flow above fully determine the change; the
  connect half is already specified in `docs/ssh-client-connect.md`.
