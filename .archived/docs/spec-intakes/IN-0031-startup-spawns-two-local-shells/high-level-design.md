# High-Level Design: Startup spawns two local shells

Intake: IN-0031
Lane: normal
Date: 2026-09-14

## Idea

Startup already intends to throw the persisted center away: `OneTermWorkspace::new` loads
`docks.json` and then calls `reset_center_only`, whose whole purpose is to put one fresh terminal
tab in the center. But `DockArea::load` builds every panel in the document first, and building the
`terminal` panel is what spawns a local shell. So the center's saved terminal spawns a shell, the
reset spawns a second one, and `set_center` prunes the first — killing a `cmd.exe` that may still
be initialising.

The fix is to stop building what is about to be discarded: hand `DockArea::load` a state whose
center is an empty stack, then let `reset_center_only` fill it. The right dock, its size and open
state, `sftp_table_state`, and `zoomed_panel` all keep loading exactly as they do now.

Nothing is written differently, so no persisted format moves. This is a startup-order fix inside
`crates/workspace`, not a change to how panels or sessions behave.

## Diagram

```text
TODAY — two shells, one of them killed in its cradle

 OneTermWorkspace::new
   │
   ├─ read_dock_document()  ──────────────▶ DockDocument { center, right_dock, zoomed_panel, … }
   │
   ├─ persistence::load_layout
   │     └─ DockArea::load(state)
   │           └─ RegistryPanelBuilder::build("terminal")          ← the bug
   │                 └─ TerminalPanel::open(DefaultShell)
   │                       └─ LocalSession::spawn
   │                             ├─ CreatePseudoConsole → OpenConsole.exe #1
   │                             └─ CreateProcessW      → cmd.exe #1  ┐ still in DllMain
   │                                                                  │
   └─ layout::reset_center_only                                       │
         └─ build_named_panel("terminal") → LocalSession::spawn       │
         │        └─ OpenConsole.exe #2 + cmd.exe #2                  │
         └─ DockArea::set_center(new tree)                            │
               └─ reconcile → panel #1 not in live_panels             │
                     └─ drop(LocalSession #1) → pty_close             │
                           └─ ClosePseudoConsole ──── kills host #1 ──┘
                                                          │
                                       cmd.exe #1 loses its console server mid-init
                                                          ▼
                                    csrss.exe shows  #32770  "cmd.exe - Application Error
                                                              … (0xc0000142)"

AFTER — one shell, nothing to prune

 OneTermWorkspace::new
   ├─ read_dock_document()
   ├─ persistence::load_layout
   │     └─ DockArea::load(state with center := empty stack)
   │           └─ builds right_dock panels only          ← no terminal, no spawn
   └─ layout::reset_center_only
         └─ build_named_panel("terminal") → LocalSession::spawn
                └─ OpenConsole.exe + cmd.exe            ← the only pair, never discarded
```

Why `load_layout` is the seam, not the alternatives:

| Candidate seam | Rejected because |
| --- | --- |
| `DockArea::load` skipping panel builds | `gpui-base` is an external dependency; and a caller that genuinely wants the saved center must still get it. |
| `TerminalPanel` deferring its spawn | Moves the cost, not the count: the panel would still be built and would still need tearing down, and every other caller of `TerminalPanel::open` pays for a lazily-spawned session. |
| `reset_center_only` reusing the loaded panel | Reintroduces the old-center-survives behaviour the reset exists to prevent, and the loaded tree may hold any number of tabs and splits. |
| `load_layout` dropping the center subtree | **Selected.** One caller, one call site, and it makes the code say what `OneTermWorkspace::new`'s comment and `docs/gui-layout.md` already claim. |

## UI Wireframe

`N/A — no UI surface`. The rendered result is what the current code already intends: one terminal
tab in the center, the right dock as it was saved.

The only visible difference is the absence of the OS dialog that should never have appeared:

```text
+--------------------------------------------------+
|  cmd.exe - Application Error                 [x] |   <- must not appear
+--------------------------------------------------+
|  (x)  The application was unable to start        |
|       correctly (0xc0000142). Click OK to        |
|       close the application.                     |
|                                        [  OK  ]  |
+--------------------------------------------------+
```

## Data Flow

1. `OneTermWorkspace::new` reads `docks.json` once through
   `persistence::read_dock_document()`; `zoomed_panel` is taken from that document before any
   reset rewrites the file.
2. `persistence::load_layout` deserialises `DockAreaState` from the document, then **replaces
   `state.center` with an empty stack node** before handing it to `DockArea::load`. The
   version-mismatch prompt, the right dock, and `set_dock_collapsible` are untouched.
3. `DockArea::load` walks the state through `RegistryPanelBuilder`. With an empty center it builds
   only the side-dock panels, so no `terminal` panel is constructed and no `LocalSession::spawn`
   runs.
4. `layout::reset_center_only` builds one `terminal` panel and one `ssh_client_panel` through
   `build_named_panel`, installs them with `set_center` / `set_dock`, preserves the loaded right
   dock width, and persists the result off the UI thread.
5. `reconcile` finds no departed terminal panel, so no `LocalSession` is dropped and no
   `ClosePseudoConsole` runs during startup. The one shell that was spawned is the one the user
   sees.
6. Zoom restoration runs afterwards against the panel names now in the tree, unchanged.

The persisted document is byte-identical to what today's code writes for the same window state:
the center that gets saved is the reset center in both cases.

## Detail Design

- [x] Detail design: not needed
- Reason: one function in `persistence::load_layout` changes, the deleted work is a subtree of an
  already-deserialised state, and the acceptance is a count assertion in an existing focused test.
  `BUG-0055` is scoped to a diagnosis whose mechanism is not yet known; if it turns out to need a
  lifecycle change in `crates/local-shell` or `crates/pty`, that packet adds its own
  `low-level-design/` file rather than expanding this one.
