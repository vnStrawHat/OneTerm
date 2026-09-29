# High-Level Design: FPS HUD

Intake: IN-0048
Lane: normal
Date: 2026-09-29

## Idea

Wrap GPUI Kit's own HUD rather than write one. `gpui-fps` 0.7 (same release family as
`gpui-component` 0.7, pinned `gpui-pre =0.3.7` like the rest of the graph) already measures
what the owner asked for from gpui's frame trace and the platform's per-process GPU counter,
and already stops everything when it is not rendered. OneTerm adds three things it does not
have: a persisted on/off switch, a placement that clears OneTerm's chrome, and a small strip
under the HUD with the GPU's name and the graphics API.

## Diagram

```text
 ui_config.json  show_fps (default false)
      |  UiConfig::set_show_fps  <-- ToggleFpsMonitor action (app menu, key-binding row)
      |                          <-- Settings > General > Interface switch
      v
 OneTermWorkspace::render
      |-- show_fps == false: drop the monitor entity (if any), render nothing more
      '-- show_fps == true:
            gpui_fps::FpsMonitor entity (created on the first shown frame)
               |-- frame trace (gpui::profiler, window-filtered): FRAME, P95, DROP, INV, MAX FPS
               '-- clock task, 500 ms: republish + ResourceProbe on the background executor
                      (sysinfo CPU, private memory, GPU Engine PDH counters on Windows)
            device/API strip (OneTerm): window.gpu_specs() once, cached; API name constant
```

## UI Wireframe

Top-right of the window, under the title bar and the tab strip, so neither the caption
buttons, the right-dock mode toggles, nor the tab strip's `+` / `...` are covered. The kit
default (12 px from the window's top-right) would sit on the caption buttons.

```text
+--------------------------------------------------------------------------+
| [>_] OneTerm                     [SSH Client|Agent|None]   [_] [ ] [X]   |  title bar
+--------------------------------------------------------------------------+
| [PowerShell x] [+]                                                 [...] |  tab strip
|                                                   +------------------+   |
|  PS C:\>                                          | MAX  60 FPS      |   |  kit HUD
|                                                   | INTERVAL 500.0 ms|   |  (fixed kit
|                                                   | FRAME      4.1 ms|   |   palette)
|                                                   | P95        6.3 ms|   |
|                                                   | DROP 0.0% INV 1.0|   |
|                                                   | GPU        0.4%  |   |
|                                                   | CPU 1.2%  MEM 51 |   |
|                                                   +------------------+   |
|                                                   | DEVICE NVIDIA .. |   |  OneTerm strip
|                                                   | API  Direct3D 11 |   |  (theme tokens)
|                                                   +------------------+   |
+--------------------------------------------------------------------------+
| status bar                                                               |
+--------------------------------------------------------------------------+
```

The kit HUD keeps its own fixed dark palette: the kit makes it non-configurable because its
0.92-alpha backdrop is what keeps every reading at 4.5:1 over any window background
(`style.rs`). The OneTerm strip uses theme tokens only: `popover` fill, `border`,
`muted.foreground` labels, `popover.foreground` values, a pairing the contrast gate already
measures on the popover surface. Click on the kit HUD collapses it to a tag; right-click
switches the headline between `MAX FPS` and the observed `FPS` (kit behaviour, unchanged).

## Data Flow

1. The switch writes `UiConfig.show_fps`, persists `ui_config.json` off the UI thread (refused
   in an elevated window, like every other write) and refreshes the windows.
2. `OneTermWorkspace::render` reads the flag. Off: it drops its `Entity<FpsMonitor>`, which
   drops the kit's clock task and its frame-trace guard at once (the kit would otherwise stop
   them itself after two unrendered 500 ms ticks). On: it creates the monitor on the first
   shown frame and renders it with the strip below it.
3. The kit samples gpui's frame trace for this window and republishes every 500 ms; its
   clock also runs the resource probe on the background executor.
4. The strip asks `Window::gpu_specs()` once per workspace and keeps the answer.

### Fields and sources

| Field | Source | Meaning |
| --- | --- | --- |
| `MAX FPS` | kit: `1 / mean draw`, capped by the display refresh rate (`EnumDisplaySettingsW` on Windows) | The rate a full redraw could sustain. Derived, not counted: counting it would mean redrawing back to back, a full layout and paint per frame charged to OneTerm. Right-click shows the observed `FPS` instead. |
| `INTERVAL` | kit: mean time between presents | How often the window actually drew; an idle OneTerm reads ~500 ms. |
| `FRAME` | kit: mean `Window::draw` time of the retained frames (gpui frame trace) | What a frame costs. This is the owner's "frame". |
| `P95` | kit: 95th percentile of the same frames | What the slow tail costs. |
| `DROP` | kit: share of retained frames whose draw exceeded the budget, one refresh period of the window's display | Dropped frames, as a percentage. |
| `INV` | kit: invalidations per frame | Kept (kit row). |
| `GPU` % | kit, Windows: PDH `\GPU Engine(*)\Utilization Percentage`, this pid's instances, busiest engine type (Task Manager's GPU column); Linux: `drm-engine-*` in `/proc/self/fdinfo`; macOS: IO registry `accumulatedGPUTime`; row absent where no counter exists | This process' GPU share, 3 s mean. |
| `CPU`, `MEM` | kit: sysinfo, single-core scale; private commit (`PrivateUsage`) on Windows | Kept (kit row). Not the status bar's figure: the status bar shows the private working set. |
| `DEVICE` | OneTerm: `Window::gpu_specs().device_name` (Windows: DXGI adapter `GetDesc1` of the adapter gpui renders with; Linux: the wgpu adapter name); `(software)` appended when gpui reports an emulated device; `n/a` where gpui returns none (macOS in 0.3.7) | The GPU's name. |
| `API` | OneTerm: compile-time constant per platform | See the matrix. |

### Platform matrix

| | Windows | Linux | macOS |
| --- | --- | --- | --- |
| GPU name | DXGI adapter description (runtime) | wgpu adapter name (runtime) | `n/a` (`gpu_specs` is `None` in gpui-pre-macos 0.3.7) |
| GPU % | PDH GPU Engine, per pid | DRM fdinfo, where the driver publishes it | IO registry, Apple silicon only |
| API | `Direct3D 11` (compile-time: gpui-pre-windows 0.3.7 only creates a D3D11 device; the feature level it got is logged, not exposed) | `Vulkan/GL (wgpu)` (compile-time: gpui-pre-wgpu asks for Vulkan or GL; the backend it picked is not in `GpuSpecs`) | `Metal` (compile-time: the only macOS renderer) |

### Cost

- **Off:** no monitor entity, no timer, no frame trace, no probe; `render` reads one bool. The
  one thing that is not zero is linked, not run by the HUD: `gpui-fps` enables `gpui-pre`'s
  `profiler` feature for the whole graph, and that feature keeps a foreground journal
  (every task poll, draw and present is noted in a ring buffer) whether or not any HUD
  exists. Measured in `US-0151` against main.
- **On:** one extra whole-window frame every 500 ms (the kit's readout clock; it tells the
  sampler it caused that frame and leaves it out of the readings), the frame trace recording
  every frame, and one resource probe every 500 ms on the background executor (a sysinfo
  refresh of this pid and one PDH collection). `gpu_specs()` runs once, on the first shown
  frame, on the UI thread. Measured in `US-0151`.

## Detail Design

- [x] Detail design: not needed
- Reason: normal lane; one overlay over an existing crate, the tables above are the design.
