# High-Level Design: Selected Space highlight content overlap

Intake: IN-0005
Lane: tiny
Date: 2026-08-12

## Idea

A split Space already has a one-pixel frame outside its content. Color that frame with the active theme token for the selected Space and the neutral border token otherwise. Remove the second, absolutely positioned inner ring that is painted after and over terminal content.

## Diagram

```text
Space wrapper frame (selected or neutral)
└── Terminal / empty placeholder content
```

## Data Flow

1. `SpaceTree.active` supplies the active `SpaceId` during rendering.
2. `render_leaf` compares the leaf id with the active id.
3. The Space wrapper chooses `table_active_border` or `border` for its existing frame.
4. Content paints inside the wrapper without a later border overlay.

## Detail Design

- [x] Detail design: not needed
- Reason: this tiny maintenance change is confined to one existing rendering seam and introduces no new interface or state.
