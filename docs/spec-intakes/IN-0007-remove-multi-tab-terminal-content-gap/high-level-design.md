# High-Level Design: Remove multi-tab terminal content gap

Intake: IN-0007
Lane: tiny
Date: 2026-08-12

## Idea

Use the existing gpui-component `Panel::inner_padding` contract to mark terminal content as full bleed. `TabPanel` will then skip its multi-tab-only `pt_2()` wrapper padding for `TerminalPanel`, making one-tab and multi-tab terminal content start at the same vertical position without changing other panel types or the vendored dependency.

## Diagram

```text
TabPanel
├── tab bar
└── active panel wrapper
    ├── default panel: inner_padding = true  -> pt_2
    └── TerminalPanel: inner_padding = false -> no gap
```

## Data Flow

1. `TabPanel` asks the active panel for `inner_padding()` when more than one tab exists.
2. `TerminalPanel` returns `false` because terminal content owns its own full-size layout.
3. `TabPanel` skips the conditional top padding and places terminal content directly below the tab bar.

## Detail Design

- [x] Detail design: not needed
- Reason: the fix is a one-method override of an existing UI extension point with no new state, interface, or dependency.
