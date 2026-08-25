# High-Level Design: Preserve terminal creation entry point

Intake: IN-0016
Lane: normal
Date: 2026-08-25

## Idea

Keep the generic dock unchanged. Route every terminal-tab close request through `TerminalPanel`: remove the panel normally when sibling tabs exist, but when it is the final terminal tab, shut down its sessions and replace its Space tree with one existing empty placeholder. The containing `TabPanel` therefore remains mounted and continues rendering its tab bar and `+` menu.

## UI Wireframe

```text
Before closing final tab:
┌──────────────────────────────────────────────┐
│ [ Terminal × ]                         [ + ] │
├──────────────────────────────────────────────┤
│ terminal content                             │
└──────────────────────────────────────────────┘

After closing final tab:
┌──────────────────────────────────────────────┐
│ [ Terminal ]                           [ + ] │
├──────────────────────────────────────────────┤
│             Empty Space                      │
│          [ New Terminal Here ]               │
└──────────────────────────────────────────────┘
```

## Data Flow

1. Close-button, middle-click, context-menu, or keybinding dispatch reaches `TerminalPanel`.
2. The panel inspects its containing `TabPanel` snapshot to determine whether a sibling tab exists.
3. With siblings, the existing `TabPanel::remove_panel` path runs unchanged.
4. Without siblings, the panel shuts down every current terminal view and replaces its tree with `SpaceTree::new_empty`.
5. Existing empty-Space and tab-bar actions create a local terminal or add a newly connected SSH panel.

## Boundaries and Invariants

- `TerminalPanel` owns terminal-specific final-tab behavior; the gpui-component fork exposes only a read-only `TabPanel::panel_count` accessor needed to distinguish the final tab.
- Closing a final tab must close each owned terminal session exactly once.
- The retained panel has one active empty leaf and remains focusable.
- Closing one of multiple tabs must still remove that tab.
- No dock persistence schema or session backend contract changes.

## Detail Design

- [x] Detail design: not needed
- Reason: the change is localized to existing panel close routing and Space-tree construction, with no new interface, schema, or high-risk behavior.
