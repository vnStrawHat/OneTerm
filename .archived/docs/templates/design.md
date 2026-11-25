# High-Level Design: {{title}}

Intake: {{intake}}
Lane: {{lane}}
Date: {{date}}

## Idea

State the core idea in a few sentences: what this builds and why.

## Diagram

Sketch the main components and how they connect (ASCII, Mermaid, or a linked image).

```text

```

## UI Wireframe

Required when this intake touches any user-facing surface (screen, page, form, component, CLI layout). Sketch each affected view as an ASCII wireframe showing layout, key regions, and primary controls. Mark `N/A — no UI surface` when the intake has no user-facing change.

```text
+--------------------------------------------------+
| Title bar                                 [x][?] |
+--------------------------------------------------+
| [ Nav ]  | Main region                           |
|          |  ( key content / controls )           |
|          |                                       |
+--------------------------------------------------+
| Status / actions:  [ Primary ]  [ Secondary ]    |
+--------------------------------------------------+
```

## Data Flow

How data moves through the system, step by step, from input to result.

1.

## Detail Design

Detail design is **required for the high-risk lane** and optional otherwise. When present, add one file per concern under `low-level-design/` so each stays reviewable.

- [ ] Detail design: required (high-risk) | added (optional) | not needed
- Reason:
