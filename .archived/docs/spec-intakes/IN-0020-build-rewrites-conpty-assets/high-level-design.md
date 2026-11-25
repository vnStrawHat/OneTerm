# High-Level Design: Atomic, change-only runtime asset copy

Intake: IN-0020
Lane: normal
Date: 2026-09-09

## Idea

`copy_runtime_asset` in `crates/app/build.rs` skips assets whose destination already has the
same size and a modification time at least as new as the source, and otherwise copies to a
`.tmp` sibling and renames it over the destination so the file is whole at every instant.

## Diagram

```text
assets/conpty.dll ──(differs?)──▶ target/<profile>/conpty.dll.tmp ──rename──▶ conpty.dll
assets/x64/OpenConsole.exe ──(same size, not older)──▶ skip
```

## UI Wireframe

N/A — no UI surface.

## Data Flow

1. Cargo runs the build script; for each asset it compares metadata.
2. Unchanged: return. Changed or missing: copy to `.tmp`, rename, remove `.tmp` on failure.
3. A launch that races the build sees either the old or the new file, never a partial one.

## Detail Design

- [ ] Detail design: not needed
- Reason: two small functions in the build script.
