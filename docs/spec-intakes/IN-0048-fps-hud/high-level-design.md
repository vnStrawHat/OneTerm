# High-Level Design: FPS HUD

Intake: IN-0048
Lane: normal
Date: 2026-09-29 (revised the same day on the owner's acceptance ruling, see `US-0151`
§ Acceptance rework)

## Idea

A small overlay of OneTerm's own, off by default, showing four things: the frames the window
draws per second, the GPU's name, the graphics API and this process' GPU usage. It reuses
what OneTerm already has — the persisted `UiConfig`, the shared `until_next_tick` repaint grid,
the background executor the status bar's resource sampler uses, and `Window::gpu_specs` — and
adds one Windows FFI surface (PDH). The first cut wrapped GPUI Kit's `gpui-fps`; the owner
dropped it because it turns on gpui's `profiler` feature for every build, which costs memory
and per-task time even with the HUD off (`evidence/US-0151-verify.md` F1).

## Diagram

```text
 ui_config.json  show_fps (default false)
      |  UiConfig::set_show_fps  <-- ToggleFpsMonitor action (app menu, key-binding row)
      |                          <-- Settings > General > Interface switch
      v
 OneTermWorkspace::render
      |-- show_fps == false: drop Option<Entity<FpsHud>>  (ticker ends, PDH query closed)
      '-- show_fps == true: create it once, render it as a child
            FpsHud::render            frames += 1 (every window frame renders it)
            ticker (spawn_in, until_next_tick 1 s)
               |-- every tick: fps = frames / elapsed; frames = 0; notify (same tick as the clock)
               '-- every 2nd tick: PDH collect on background_executor -> GPU usage (shown next tick)
            gpu_specs() once -> GPU row; API row = compile-time constant
```

## UI Wireframe

Top-right of the window, under the title bar and the tab strip, so neither the caption
buttons, the right-dock mode toggles, nor the tab strip's `+` / `...` are covered.

```text
+--------------------------------------------------------------------------+
| [>_] OneTerm                     [SSH Client|Agent|None]   [_] [ ] [X]   |  title bar
+--------------------------------------------------------------------------+
| [Command Prompt x]                                     [+] [] [...] [|]  |  tab strip
|                                            +------------------------+    |
|  C:\>                                      | FPS                  2 |    |
|                                            | GPU  Intel(R) UHD Gr.. |    |
|                                            | API        Direct3D 11 |    |
|                                            | GPU usage         1.8% |    |
|                                            +------------------------+    |
+--------------------------------------------------------------------------+
| status bar                                                               |
+--------------------------------------------------------------------------+
```

Theme tokens only: `popover` fill, `border`, `muted.foreground` labels, `popover.foreground`
values — pairs the contrast gate already measures on the popover surface. The overlay blocks
clicks and drags from reaching what is under it (`block_mouse_except_scroll`); the wheel still
scrolls the terminal.

## Data Flow

1. The switch writes `UiConfig.show_fps`, persists `ui_config.json` off the UI thread (refused
   in an elevated window, like every other write) and refreshes the windows.
2. `OneTermWorkspace::render` reads the flag. Off: it drops its `Entity<FpsHud>`. On: it
   creates the entity on the first shown frame (reading `gpu_specs()` once and opening the PDH
   query) and renders it.
3. Each window frame renders the HUD view, which adds one to its frame count.
4. The ticker wakes on the shared one-second grid, publishes `frames / elapsed` and notifies;
   every second tick it also runs one PDH collection on the background executor and stores the
   result for the next tick to show.

### Fields and sources

| Row | Source | Meaning |
| --- | --- | --- |
| `FPS` | Renders of the HUD view per second (whole number) | Frames actually drawn. Idle reads about 2 (the cursor blink and the clock share the grid), output reads the real rate. The HUD forces no frames beyond its once-a-second refresh, which coincides with the clock's tick. Not a refresh rate or a "max"; that would be a follow-up. |
| `GPU` | `Window::gpu_specs().device_name` (Windows: DXGI `GetDesc1` of the adapter gpui's D3D11 device runs on; Linux: the wgpu adapter name); `(software)` for an emulated device; `n/a` where gpui answers `None` (macOS in 0.3.7) | The GPU's name. |
| `API` | Compile-time constant per platform | See the matrix. |
| `GPU usage` | Windows: PDH `\GPU Engine(*)\Utilization Percentage`, instances named `pid_<this pid>_…`, summed per engine type (`engtype_3D`, `engtype_Copy`, …), busiest type shown, clamped to 100 | This process' GPU share, as Task Manager's GPU column shows it. 2 s cadence. |

### Platform matrix

| | Windows | Linux | macOS |
| --- | --- | --- | --- |
| FPS | render count | render count | render count |
| GPU | DXGI adapter description (runtime) | wgpu adapter name (runtime) | `n/a` (`gpu_specs` is `None` in gpui-pre-macos 0.3.7) |
| API | `Direct3D 11` (gpui-pre-windows 0.3.7 only creates a D3D11 device; the feature level it got is logged, not exposed) | `Vulkan/GL (wgpu)` (gpui-pre-wgpu asks for Vulkan or GL; the backend it picked is not in `GpuSpecs`) | `Metal` |
| GPU usage | PDH, per pid | `n/a` | `n/a` |

### Cost

- **Off:** no entity, so no ticker, no PDH query, no count, no allocation. `render` reads one
  bool from the `UiConfig` global. No new dependency and no gpui feature.
- **On:** one `u64` add per frame; one notify a second, on the grid the clock already repaints
  on; one `gpu_specs()` call when shown; one PDH collection (`PdhCollectQueryData` +
  `PdhGetFormattedCounterArrayW`, every engine of every process) every 2 s on the background
  executor. Measured in `US-0151`.

## Detail Design

- [x] Detail design: not needed
- Reason: normal lane; one overlay, the tables above are the design.
