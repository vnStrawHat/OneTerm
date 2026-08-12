# High-Level Design: Duplicate sessions into empty Spaces

Intake: IN-0006
Lane: normal
Date: 2026-08-12

## Idea

Make **Duplicate Session** a destination-aware submenu while preserving the existing duplicate semantics and keybinding. The visible number is the Space's existing monotonic, non-reused `SpaceId`; it helps the user match **Into Space #N** with the **Space #N** placeholder and is also the action identity.

Duplication produces a terminal through the existing local factory or fresh-authenticated SSH flow. The result is then routed to one of four destination modes: a new tab, an existing empty Space, a new split to the right, or a new split below. Before placing the result, the originating panel revalidates the destination so stale menu/dialog state can never replace existing content or silently choose another Space.

## Diagram

```text
Right-clicked terminal Space
└── Duplicate Session submenu
    ├── In New Tab ───────────────────────────────┐
    ├── Into Space #1 ── SpaceId(A) ──────────────┤
    ├── Into Space #2 ── SpaceId(B) ──────────────┤
    ├── Split Right ──────────────────────────────┤
    └── Split Down ───────────────────────────────┤
                                                   v
                                  DuplicateDestination
                                                   |
                     +-----------------------------+--------------------+
                     |                                                  |
              Local session factory                         Fresh SSH auth dialog
                     |                                                  |
                     +---------------- duplicated terminal -------------+
                                                   |
                                      revalidate originating panel
                                                   |
                     +-----------------------------+--------------------+
                     |                             |                    |
                  new tab                  fill empty Space       create/fill split

Empty Space rendering/menu:
SpaceTree ordered empty leaves: [(SpaceId(A), #1), (SpaceId(B), #2)]
fill SpaceId(A):              [(SpaceId(B), #2)]  (no compaction)
```

## Data Flow

1. A tab's initial terminal owns `SpaceId(0)`. Empty destinations are created by split with monotonic IDs starting at 1; the spawn-failure recovery placeholder also starts at `SpaceId(1)`.
2. When an occupied Space opens its context menu, `TerminalPanel` walks the current tab's leaves in visual tree order and returns only their empty `SpaceId` values; no derived-number tuple is stored or allocated.
3. The menu always adds **In New Tab**, adds one **Into Space #N** item using each ID's raw value, then adds a separator followed by **Split Right** and **Split Down**. Empty placeholder rendering derives the same raw ID directly from its leaf, without lookup or fallback.
4. Selecting a menu item combines the right-clicked source `SpaceId` with a destination enum. Keyboard dispatch of the existing `DuplicateSession` action uses the active source Space and `InNewTab`.
5. Local duplication creates a session with the existing non-secret launch metadata/cwd rules. SSH duplication sends the same non-secret source metadata plus destination metadata through the existing command registry, prompts again for credentials, and returns the connected session to the originating destination route.
6. Immediately before placement, the panel verifies that the source tab/panel still exists. For an existing-Space destination it also verifies that the captured `SpaceId` still exists and is empty. Split destinations verify that the source Space is still valid before changing the tree.
7. A valid result is placed at the requested destination and focused. An invalid/stale destination causes no layout/content replacement and reports that the destination is no longer available.
8. Filling an existing empty Space does not restructure the tree. Split Right/Down creates a new adjacent leaf from the source Space and fills it with the duplicate. In New Tab retains the current behavior.

## Key Invariants

- Visible `#N` is the raw value of the same stable `SpaceId` used by commands; there is no second display identity.
- `SpaceId(0)` is reserved for the initial terminal. Every user-visible empty placeholder has `SpaceId >= 1`.
- IDs are monotonic and non-reused, so surviving labels are never compacted during the tab lifetime.
- Menu labels, placeholder labels, and Agent Panel card labels read the same raw `SpaceId`, preventing mismatches. Agent Panel card ordering remains a separate depth-first `space_order` value.
- Duplicate placement exhaustively handles every `DuplicateDestination` variant; no panic branch represents a normal enum variant.
- The source session is never moved, closed, or modified.
- Occupied Spaces are never replaced by duplicate placement.
- SSH credentials are never embedded in destination metadata or retained for later duplication.
- No persisted layout/schema is introduced.
- Crate dependency rules remain unchanged; SSH UI coordination uses the existing command-registry seam.

## Detail Design

- [x] Detail design: not needed
- Reason: the normal-lane change extends the existing Space tree, context-menu, duplicate-session, and command-registry seams. The stable identity, ownership, security behavior, destination variants, data flow, and stale-target rule are sufficiently bounded here; implementation-specific GPUI submenu calls should be confirmed against the vendored reference in the work packet.
