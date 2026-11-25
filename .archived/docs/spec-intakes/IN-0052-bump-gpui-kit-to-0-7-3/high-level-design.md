# High-Level Design: Bump GPUI Kit to 0.7.3

Intake: IN-0052
Lane: normal
Date: 2026-10-15

## Idea

An ordinary GPUI Kit patch-level bump within the 0.7 family (DEC-0006). The workspace requirements (`0.3`, `0.7`) already admit the new versions, so only `Cargo.lock` moves; Kit 0.7.3 pins `gpui-pre =0.3.9`, so the gpui-pre family moves with it. No OneTerm architecture changes.

## Diagram

Sketch the main components and how they connect (ASCII, Mermaid, or a linked image).

```text
Cargo.lock: gpui-pre 0.3.7 -> 0.3.9 <- gpui-base / gpui-component / gpui-kit-assets 0.7.0 -> 0.7.3
```

## UI Wireframe

Required when this intake touches any user-facing surface (screen, page, form, component, CLI layout). Sketch each affected view as an ASCII wireframe showing layout, key regions, and primary controls. Mark `N/A — no UI surface` when the intake has no user-facing change.

N/A — no UI surface

## Data Flow

How data moves through the system, step by step, from input to result.

1. N/A — no data flow change.

## Detail Design

Detail design is **required for the high-risk lane** and optional otherwise. When present, add one file per concern under `low-level-design/` so each stays reviewable.

- [x] Detail design: not needed
- Reason: dependency bump; no design change.
