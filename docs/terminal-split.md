# Terminal Split — Design (index)

> **Status: Historical — superseded by IN-0018.** Spaces still work exactly as described here
> (split R/L/U/D, drag a tab into an empty Space, close/collapse, the placeholder and its menu), so
> this document remains the readable narrative of the feature. Its mechanism sections are stale: the
> Space tree and the panel were rewritten and now live in `crates/terminal-view/src/space/`
> (`tree.rs`, `render.rs`) and `crates/terminal-view/src/panel/` (`terminal_panel.rs`, `spaces.rs`,
> `duplicate.rs`, `tab_title.rs`) — the module names, file layout and call paths below no longer
> match the code. The current owning design is
> [`docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md`](spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md)
> (behavior parity for Spaces is its §2.2 checklist).
>
> Design for **Terminal Split**: splitting a single Terminal Tab into multiple
> resizable **Spaces** (Right / Left / Up / Down), nested recursively, *inside
> the current tab* — no new Dock, no new Tab.
>
> This design is split into focused documents (kept small on purpose). Read them
> in order; each builds on the previous.

## Documents

| # | File | Topic |
|---|---|---|
| 0 | [`terminal-split/00-overview.md`](terminal-split/00-overview.md) | Goals, scope, requirements, non-goals, glossary |
| 1 | [`terminal-split/01-architecture.md`](terminal-split/01-architecture.md) | The Space pane-tree model, data structures, where it lives |
| 2 | [`terminal-split/02-split-and-close.md`](terminal-split/02-split-and-close.md) | Split operations, Close Space, tree collapse, active tracking |
| 3 | [`terminal-split/03-drag-drop.md`](terminal-split/03-drag-drop.md) | Dragging a Terminal Tab into a Space with a terminal-specific payload |
| 4 | [`terminal-split/04-context-menu.md`](terminal-split/04-context-menu.md) | Context-menu changes (Split R/L/U/D, Close Space) |
| 5 | [`terminal-split/05-rendering-theme.md`](terminal-split/05-rendering-theme.md) | 1px outer border + 1px inner gutter, active-Space highlight, empty placeholder |
| 6 | [`terminal-split/06-integration.md`](terminal-split/06-integration.md) | Touch points: `TerminalPanel`, `set_active`, statusbar/SFTP, focus |
| 7 | [`terminal-split/07-roadmap-risks.md`](terminal-split/07-roadmap-risks.md) | File layout, implementation order, risks, open questions |

## TL;DR

Before this feature a `TerminalPanel` wrapped exactly one `LocalTerminalView`. It now
holds a **`SpaceTree`**: a binary pane tree whose leaves are either a terminal view or
an empty placeholder. Splitting a leaf turns it into a `Split` node with two children
(the existing terminal + a new empty placeholder). The tree is rendered with nested
`h_resizable`/`v_resizable` groups (gpui-component resize handles); every Space has a
neutral 1px outer border plus a 1px inner gutter that takes the accent colour on the
active leaf, and a leaf can be filled by dragging a Terminal Tab onto it. Closing the
last remaining leaf reverts the tab to a plain single terminal (no borders).

## Broadcast input channels (IN-0022)

A Space can join one of the five broadcast input channels A..E from its context menu
("Input Channel"), so that one keystroke, typed text, paste or Ctrl+C reaches every other
member of that channel. What it means for Spaces (DEC-0009):

- Membership belongs to the **Space** — to the `TerminalView` entity, not to the tab or the
  Space id. A tab dragged into an empty Space keeps its channel because the same view entity
  moves; Close Space and Close Tab leave the channel with the session.
- A Space created by Split or Duplicate Session is a **non-member**, even when its siblings
  are members. "Input Channel > Join All Spaces In Tab To X" is the one-click way to put the
  whole tab in one channel.
- Every member Space shows a badge with its channel letter in the top-right corner — the
  same 16 px square chip the tab strip uses (bold letter on `chart_1..chart_5`, its glyph
  centred by the text node) — 5 px from the top and right edges. The badge is what tells the
  Spaces of one tab apart when only some of them joined.
- While a member Space is the selected one, its active-Space highlight takes the channel
  colour instead of the theme's active colour; unselected Spaces keep the plain border,
  member or not. A Space in no channel keeps the active/inactive rule of
  [05](terminal-split/05-rendering-theme.md). The single Space of an unsplit tab stays
  borderless in either case — it only gains the badge, so an unsplit tab keeps its layout.
- Membership is in-memory: nothing about channels is persisted, like the split layout itself.
- Broadcast is **not** suppressed on the alternate screen. A member Space running a
  full-screen program (`vim`, `htop`) receives the peers' input like any other member, so
  leave the channel before starting one there.

The owning design is
[`docs/spec-intakes/IN-0022-broadcast-input-channels/high-level-design.md`](spec-intakes/IN-0022-broadcast-input-channels/high-level-design.md).

## Confirmed decisions (from clarification)

1. **Drag scope**: only **Terminal Tabs** can be dropped into a Space (not SFTP /
   Session panels). Drop semantics are **move** (the source tab's content is moved
   into the Space; the emptied source tab closes).
2. **Nesting**: **recursive** — any Space can be split again in any direction
   (binary pane tree, resizable), like tmux/Zed panes.
3. **Collapse**: when Spaces are closed down to one, the Tab shows a **plain single
   terminal** again (no Space borders).
4. **Persistence**: **not persisted** for the MVP — on restart a tab is a single
   terminal (matches the fact that terminal sessions don't persist anyway).
5. **Language**: docs are written in **English** (AGENTS.md core principle #6).
6. **Drop target**: only an **empty** Space accepts a dropped tab. A Space that
   already holds a terminal is not droppable (no edge-aware split-on-drop).
7. **Active after split**: the **new empty** Space becomes active.
8. **Border**: originally a uniform 4px frame; shipped as a neutral **1px outer border +
    1px inner gutter** per Space (`space/render.rs`), see [05](terminal-split/05-rendering-theme.md).
    **Amended by `US-0117`, not reversed.** The frame is still 1px + 1px for every Space, and
    a lone Space is still borderless — what changed is the *active* cue, which one pixel
    answered too quietly for "where does my typing go" (`F26`). The active Space now also
    carries a **2px ring in the cue colour** (`active_cue_ring`), painted as an
    absolutely-positioned overlay. An absolute child is laid out against the **padding box**,
    so `inset_0` starts *inside* the 1px outer border: the ring never overpaints that border,
    which stays the neutral separator this decision fixed, and covers the 1px gutter plus the
    outermost pixel of the terminal's own content area. An overlay rather than a wider border
    or a wider padding because geometry that changed with focus would move the terminal's
    content box by a pixel every time focus changed Spaces, which can cost the grid a column;
    the overlay has no id and no mouse handler, so it creates no hitbox and clicks, hover and
    scroll still reach the terminal. The ring's colour is the same one the gutter uses — the
    channel colour for a member Space, `table_active_border` otherwise — so nothing new is
    hardcoded and the channel rule is unchanged.

    Each **inactive** Space in a split also carries a small **number chip** — `#N` and nothing
    else, with the channel badge's own 16px footprint, in the channel-badge slot and left of
    the badge. What the Space holds is on the chip's **tooltip**, not on its face. The first
    shipped attempt put `#N` *plus the live session title* on the face, in a chip up to 160px
    wide with a translucent backdrop, on **every** Space including the active one; independent
    verification found it permanently washing out the first prompt line of every running
    shell, because the top-right of a terminal is not chrome — it is wherever the output
    currently is. The chip is therefore `#N` only, and only where the ring is not: the active
    Space is already answered by its cue, so the chip carries the one thing the cue cannot,
    which is which Space this is. `#N` is the Space's stable `SpaceId`, allocated
    monotonically and never reused — not a positional index, so a split whose Spaces have been
    closed and re-split can read `#0` and `#5`. An empty Space keeps only its placeholder
    (which already prints `Space #N`) and a lone Space stays unmarked.
9. **New Terminal Here**: the empty-Space menu can spawn a local shell in place
   (in MVP scope). It spawns the **default** shell, with no shell picker — the
   empty Space is a placement action, and a user who wants a specific shell
   opens it from the tab bar's `+` menu and drags the tab in. Amended by
   `US-0115`: the placeholder names that action first and in the menu row's own
   words, so the copy promises exactly what the menu does. It reads, under the
   `Space #N` line, `Right-click → New Terminal Here` and then
   `or split, or drag a terminal tab here`; the drag and the split keep their
   mention, but the likeliest action is no longer the hidden one
   (`crates/terminal-view/src/space/render.rs`).
10. **Keyboard shortcuts** for Split / Close Space: deferred for the MVP; the
    `SplitRight/Left/Up/Down` and `CloseSpace` actions are now rebindable in the
    Settings key-binding UI (`crates/settings-ui/src/key_bindings/`).
