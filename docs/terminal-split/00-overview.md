# 00 — Overview

> Part of [Terminal Split design](../terminal-split.md). Goals, scope,
> requirements, non-goals, and shared vocabulary.

## 1. Motivation

Today each Terminal Tab (`TerminalPanel`) hosts exactly one terminal. Power users
want to see several terminals side by side (e.g. run a build in one pane, tail logs
in another) without juggling separate tabs or docks. The **Terminal Split** feature
lets a single tab be divided into multiple resizable **Spaces**, arranged by
splitting Right / Left / Up / Down, nested to any depth.

## 2. Scope (what we build)

- Split the current Terminal Tab into multiple Spaces via **Split Right / Left /
  Up / Down**.
- Add **Split Right / Left / Up / Down** items to the terminal context menu.
- Splitting happens **inside the current Terminal Tab** — never a new dock panel,
  never a new tab.
- A newly created Space starts **empty**, showing only placeholder text.
- A **Terminal Tab** (from the tab strip) can be **dragged into** an empty Space to
  fill it. Dropping **moves** the tab's content into the Space and closes the now
  empty source tab.
- Right-clicking a Space shows a context menu that includes **Close Space**, placed
  directly **below** the existing **Close Terminal Tab** item.
- The border between Spaces is **4px** thick.
- The **active** Space has a distinct/prominent border color; inactive Spaces use
  the normal border color.
- Closing Spaces down to one reverts the tab to a **plain single terminal**.

## 3. Non-goals (explicitly out of scope for the MVP)

- **Persisting** split layouts across app restarts (see [07](07-roadmap-risks.md)
  for how it could be added later).
- Dragging **non-terminal** panels (SFTP / Session) into a Space.
- Dragging a tab into a Space that **already holds a terminal** — only **empty**
  Spaces are drop targets (confirmed). No edge-aware split-on-drop.
- Dragging a Space **out** of a tab to become its own tab/dock (only tab → Space is
  in scope, not Space → tab).
- Cross-tab Space drag choreography beyond the single move-on-drop described here.
- Splitting SFTP / Session / Settings panels — this feature is terminal-only.

## 4. Requirements → design map

| # | Requirement | Where it is designed |
|---|---|---|
| R1 | Split a tab into Spaces (R/L/U/D) | [01](01-architecture.md), [02](02-split-and-close.md) |
| R2 | Context-menu items Split R/L/U/D | [04](04-context-menu.md) |
| R3 | Split inside current tab, no new dock/tab | [01](01-architecture.md) |
| R4 | New Space is empty / placeholder | [02](02-split-and-close.md), [05](05-rendering-theme.md) |
| R5 | Drag a Terminal Tab into a Space | [03](03-drag-drop.md) |
| R6 | Context menu on a Space with **Close Space** below **Close Terminal Tab** | [04](04-context-menu.md) |
| R7 | 4px border between Spaces | [05](05-rendering-theme.md) |
| R8 | Active Space has a distinct border color | [05](05-rendering-theme.md) |

## 5. Glossary

- **Space** — one pane inside a Terminal Tab. A Space is a *leaf* of the tab's
  pane tree. It holds either a live terminal or an empty placeholder.
- **Space tree / pane tree** — the binary tree that describes how a Terminal Tab is
  divided. Internal nodes are *splits* (horizontal or vertical); leaves are Spaces.
- **Active Space** — the Space that currently has focus. It receives keyboard input
  and is the target of tab-level actions (Split, Close Space) and status-bar
  reporting (breadcrumb, SFTP, network stats).
- **Terminal Tab** — a `TerminalPanel` in the center dock's `TabPanel`. After this
  feature, a Terminal Tab contains a Space tree instead of a single terminal view.
- **Placeholder** — the empty state of a Space: centered hint text (e.g. *"Drag a
  terminal tab here, or right-click to split"*).
