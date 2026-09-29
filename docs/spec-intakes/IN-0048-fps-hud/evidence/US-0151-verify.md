# US-0151 verification: FPS HUD, off by default

Verifier: Claude Fable 5.1 (adversarial pass), 2026-09-29.
Target: `feat/fps-hud` @ `09812f0d` (one commit on main @ `e0db223f`).
Machine: Windows 11, Intel UHD Graphics 770 (the only adapter), 60 Hz panel.
Builds: `fast-dev` of `09812f0d` and of `e0db223f`, each in this worktree's own target dir,
`CARGO_BUILD_JOBS=2`. Every GUI run used a private `USERPROFILE` and a private working
directory (fast-dev reads `ui_config.json` from `<cwd>\target`), and touched only the pid it
launched.

## Interim, superseded by the owner's scope change (2026-09-29)

The owner narrowed the HUD to fps, GPU name, graphics API and GPU % and dropped the
`gpui-fps` dependency (so gpui's `profiler` feature, `hdrhistogram` and the kit rows go). The
verdict below is for `09812f0d` as built and is superseded. Facts that survive the change:

1. **Per-frame cost of `profiler` with the HUD off.** Per draw: none measurable (release
   micro-bench, min 54-58 us both ways; 258 allocations / 174,432 bytes per draw both ways).
   Per task poll, every thread: +55-65 ns (hook 54-69 ns without, 115-137 ns with). Memory:
   +4.1 MB commit, +3.8 MB private working set (the 4 MiB journal ring). Process-level idle and
   flood time: no difference. Dropping the dependency removes all of it (F1).
2. **The ~30 MB after toggling off** is gpui-pre-windows' per-window path textures (4x MSAA +
   resolve, 20 B/px, committed at window creation, first touched by the kit chart's
   `paint_path`); it scales with window area and is released on the next resize. Not PDH, not a
   leak; 20 toggles do not ratchet (F2). A replacement HUD that draws no path avoids it.
3. **`Window::gpu_specs()` on a dual-GPU box** names the adapter gpui renders with, by
   construction: `gpu_specs` reads `GetDesc1` of the same `IDXGIAdapter1` the D3D11 device was
   created on (first `EnumAdapters` entry that creates a device). Argued from source; this
   machine has one adapter (F6).
4. **D3D feature level:** not reachable through public API in gpui-pre 0.3.7. It is only
   logged (`directx_devices.rs`); `GpuSpecs` has no such field (F6).

## Verdict

**PASS**, with one owner decision before merge (F1) and four record corrections (F2-F5).

Every functional claim holds: default off and not written while off, the three toggles plus a
user-bound key, the check mark after a restart, the Settings switch and the Key Bindings row
(both walked in the GUI this time), the placement at 800 px, GPU % against PDH, the
compile-time API. The "off" cost is **not** literally zero (F1): gpui's `profiler` feature runs
whether or not the HUD exists. Per frame it is below measurement (same draw time, same
allocations); per task poll it is +55-65 ns in isolation; in memory it is +4 MB. Under the
owner's invariant that is option (c) and needs the owner's sign-off. The "~30 MB that stays
after toggling off" is not a leak and not PDH: it is gpui's window-sized path MSAA textures,
first touched by the HUD's frame-time chart (F2). Twenty toggles do not ratchet.

## Findings

| # | Severity | Finding |
| --- | --- | --- |
| F1 | Medium (owner decision) | The off-cost is not zero; recommend option (c), owner sign-off |
| F2 | Low (records) | The ~30 MB left after "off" is gpui's path MSAA textures, not PDH or allocator pages; no ratchet |
| F3 | Low (records) | "On" memory is not a constant +30 MB: gpui's trace buffers grow with activity while on |
| F4 | Low (UX, records) | A left-click on the HUD also reaches the terminal under it |
| F5 | Info (records) | Field definitions: `DROP` and `MAX FPS` are CPU-draw based; a frame counter is cheap |
| F6 | Info | `DEVICE` / `API` checked against gpui-pre 0.3.7; the feature level is not reachable |

### F1 — the off-cost is not zero (Medium, owner decision)

`gpui-fps 0.7.0` depends on `gpui-pre =0.3.7` with `features = ["profiler"]`
(`gpui-fps-0.7.0/Cargo.toml`), so every OneTerm build compiles gpui with it. What that feature
runs **with no monitor at all** (read in `gpui-pre-0.3.7/src`):

- `App::new_app` installs the foreground journal (`app.rs:857`,
  `profiler/journal.rs::install_foreground_journal`): a ring of
  `MAX_JOURNAL_ENTRIES = 4 MiB / size_of::<JournalSlot>()` slots, all written at creation, so
  resident at once.
- Every runnable on every thread (`gpui-pre-windows` `dispatcher.rs::execute_runnable`) calls
  `profiler::update_running_task` / `save_task_timing`. With the feature these take a second
  `Instant::now()`, update the per-thread "longest polls" statistics and, on the main thread,
  `journal::begin_foreground_turn` / `record_task_poll` (polls under 100 us are folded into a
  summary, longer ones published into the ring). Without the feature the pair is one
  timestamp and a lock.
- Every `Window::draw` (`window.rs:3260-3302`): `take_frame_dirty`, `WindowProfiler::begin_draw`
  / `end_draw` (two timestamps, a pending-frame journal entry, a `draw_duration` histogram
  record, `journal::record_draw`), and `debug_frame_overlay.record_frame` (a 1000-sample
  `VecDeque`). Every `present`: a foreground turn, two timestamps, up to two histogram records,
  `journal::record_present`. Every input and action handler: a journal entry and timestamps.
- `record_frame_event` and the per-thread task-timing ring are gated on `trace_enabled()`, so
  those two only run while a monitor holds the trace (F3).

Measured, HUD off:

| Measure | main @e0db223f | this branch, HUD off |
| --- | --- | --- |
| Draw of a 20-row view, min / median of 9 x 20k draws (release micro-bench, below) | 54.2-57.9 / 65.7-77.7 us | 53.6-56.5 / 71.1-82.3 us |
| Allocations per draw (same bench, counting allocator, 10k steady-state draws) | 258.00 allocs, 174,432 B | 258.00 allocs, 174,432 B |
| Task-poll hook (`update_running_task` + `save_task_timing`, main thread, 9 x 2M) min / median | 53.7-68.6 / 62.7-84.6 ns | 114.9-136.5 / 127.2-150.5 ns |
| Idle 60 s focused, UI thread (`QueryThreadCycleTime`), 3 runs | 30.4, 32.0, 33.7 Mcycles/s | 29.0, 29.3, 38.7 Mcycles/s |
| Idle, whole process (`QueryProcessCycleTime`), 3 runs | 61.9, 71.2, 68.3 Mcycles/s | 62.3, 63.2, 78.9 Mcycles/s |
| Private commit after ~68 s, 3 runs | 111.7, 110.6, 110.1 MB | 114.9, 114.8, 115.0 MB (+4.1) |
| Private working set after ~68 s, 3 runs | 52.6, 51.4, 50.9 MB | 55.4, 55.5, 55.4 MB (+3.8) |
| 100k-line flood (`for /l ... echo`), wall time, 3 runs | 15.2, 14.6, 14.6 s | 14.9, 14.2, 14.7 s |
| Same flood, process Mcycles total | 27,029, 25,287, 25,045 | 22,531, 21,650, 25,484 |

The micro-bench is a scratch crate (not committed) on `gpui-pre =0.3.7` + `test-support`,
built `--release` with and without `gpui/profiler`; it opens a `TestAppContext` window and
times `window.refresh(); window.draw(cx).clear(cx)`, and times the two public
`gpui::profiler` hooks the Windows dispatcher calls. Runs alternated ON/OFF three times.
`present` is private to gpui and not covered by it.

Reading: per frame the added bookkeeping is below what a draw measures (under ~1 us against
a 54 us floor) and adds **no allocation**; per task poll it is **+55-65 ns** (about 2x the
hook) on every thread; at process level nothing separates the builds in time; the memory is
**+4 MB** (commit and private working set). The commit message's "measured no UI-thread cost
when off" is right at process level; the intake's "While off it costs nothing at run time"
is not literally true.

Options for the owner's zero-cost-when-off requirement:

- (a) A OneTerm cargo feature for `gpui-fps`, **default on**: rejected. It keeps exactly this
  cost in every default build, so it is only worth doing if the cost were provably nil, and it
  is not (the task-poll hook doubles; 4 MiB is always resident).
- (b) The same feature **default off**, with the menu item, the Settings switch and the Key
  Bindings row shown disabled ("not in this build") when it is missing: the only way to make
  "off" truly zero. Roughly 40-60 lines across `oneterm-app`, `oneterm-workspace`,
  `oneterm-settings-ui` plus a CI clippy leg for the feature, and the owner's everyday
  (release) build would then have **no** HUD, which defeats "show FPS when needed".
- (c) **Recommended:** accept, with the owner's explicit sign-off, +4 MB resident and
  +55-65 ns per task poll (no per-frame allocation, no process-level time difference
  measured), and correct the intake wording. Revisit if a later gpui-pre makes the journal
  lazy or splits it from the trace.

### F2 — the ~30 MB left after "off" is gpui's path MSAA textures (Low, records)

The packet attributes it to "the PDH/perf-counter DLLs and allocator pages". Measured instead:

- The jump is in the **private working set** only: showing the HUD once at 1280 x 800 took it
  from 54.9 to 80.5 MB while **commit** moved 114.3 -> 119.8 MB, and after "off" commit fell
  back to 118.7 MB (+4.4 MB one-time) while the working set stayed at ~80 MB. No module was
  loaded (78 before and after; `pdh.dll` is already loaded at startup) and no thread stayed.
- It scales with the **window area** and is released by the next **resize**:

  | Window | Private WS before / HUD on / off / after a resize | Released by the resize | 20 B/px of window |
  | --- | --- | --- | --- |
  | 800 x 600 | 54.1 / 70.1 / 69.1 / 58.5 MB | 10.6 MB | 9.6 MB |
  | 1600 x 1000 | 55.9 / 93.9 / 93.1 / 59.8 MB | 33.3 MB | 32.0 MB |

- Cause: gpui-pre-windows 0.3.7 creates, per window and at window size, a path intermediate
  texture and a 4x MSAA one (`directx_renderer.rs`, `PATH_MULTISAMPLE_COUNT = 4`,
  `create_path_intermediate_*`): 4 + 16 = 20 bytes a pixel, committed at creation (commit is
  24 MB higher at 1600 x 1000 than at 800 x 600) but never touched until a path is drawn.
  OneTerm draws no path (`paint_path` / `PathBuilder` appear nowhere under `crates/`); the kit
  HUD's frame-time chart does (`gpui-fps` `monitor.rs::render_chart`, `PathBuilder::stroke`,
  `window.paint_path`). On this integrated GPU the textures live in shared system memory, so
  touching them shows up as private working set; on a discrete GPU they would be VRAM. They
  stay resident after "off" until gpui recreates them on the next resize.
- No ratchet, 20 toggles (F9 bound to the action, 4 s on / 4 s off, 1280 x 800):
  private WS 85.4-86.8 MB in every "off" state (first 85.8, last 85.6, 30 s later 85.6);
  commit 124.2-125.7 MB (one +0.9 MB step at cycle 13, flat for the remaining 7 cycles and
  30 s); handles 514 in every "off" state and 517-521 while on (the PDH query is closed each
  time, `gpu/windows.rs` `Drop` -> `PdhCloseQuery`); threads 16-17 throughout (the GPU probe
  runs on gpui's background executor inside the clock task, there is no dedicated thread);
  modules 78 throughout. The kit monitor entity is dropped (`fps_hud: None`), which drops its
  clock task, the `ResourceProbe` (sysinfo + PDH) and its `FrameTraceGuard`.

Fix: correct the packet's "Evidence and Gaps" and `docs/gui-layout.md` § FPS HUD (the path
textures, window-size dependent, released on resize, iGPU shared memory).

### F3 — "on" memory grows with activity while shown (Low, records)

While the HUD holds the trace, gpui keeps **every task poll of every thread** in a per-thread
ring (`MAX_TASK_TIMINGS = 16 MiB / size_of::<TaskTiming>()` each) and every draw/present in
`FRAME_TIMINGS` (16 MiB); the kit reads only frames. Measured: HUD on vs off side by side for
8 min with `ping -t` output, commit grew +6.2 MB on vs +2.3 MB off (~+0.5 MB/min); a 300k-line
flood with the HUD on took commit 120.1 -> 160.2 MB (about +10 MB with it off) and switching
the HUD off gave 31 MB back within 6 s (`clear_trace_buffers` + `shrink_to_fit`). Bounded and
released, so not a leak, but the packet's "+30 MB" is a floor: about 20 MB of it is F2's
textures at 1280 x 800, and the rest grows with activity up to the per-thread cap.
Idle on-cost measured here: UI thread 52.0 / 53.9 Mcycles/s (off: ~32), process 127 / 129
Mcycles/s (off: ~68), `INTERVAL` ~250 ms (about 4 frames/s), consistent with the packet.

### F4 — a left-click on the HUD reaches the terminal under it (Low, UX)

With the right dock set to None the HUD lies over the terminal. A drag started inside the HUD
selected terminal text (`US-0151-verify-drag-from-hud-selects.png`); a left-click on the HUD
collapsed it **and** cleared that selection (`US-0151-verify-click-hud-clears-selection.png`):
the kit's `on_click` does not stop propagation, so the click also reaches the terminal (with a
mouse-reporting TUI it would be a click sent to the application at that cell). A right-click on
the HUD switches the headline (`MAX` <-> observed `FPS`) and is consumed (the kit calls
`stop_propagation`; no terminal context menu). The DEVICE/API strip has no handlers, so it is
fully click-through (a right-click on it opened the terminal's context menu, drawn above the
HUD). Over the SSH Client dock the HUD covers the right end of "Search sessions..." and the
empty-state text; a click there both collapses the HUD and focuses the input.
Not documented. Either state it in `docs/gui-layout.md`, or stop left mouse-downs at OneTerm's
wrapper div so the HUD is not click-through (one `on_mouse_down(MouseButton::Left, ...)` with
`cx.stop_propagation()` on the wrapper in `fps_hud.rs`; the kit's own handlers run first).
Owner's call.

### F5 — field definitions (Info, records)

- `MAX FPS` = `1 / mean Window::draw time` of the retained frames, capped by the panel's refresh
  rate (`sustainable_rate`, `monitor.rs:167`). The headline carries a `MAX` marker and
  right-click switches to the observed rate, so it is honest; the docs could say "CPU draw
  time" (present and GPU time are not in it).
- `FRAME` = mean draw cost in ms, not a count. The owner wrote "frame"; the kit has no running
  frame counter (its `frames_rendered` is private). A count is cheap: one `u64` increment in
  `OneTermWorkspace::render`, which runs on every frame of the main window, shown in the strip.
  For the owner to decide.
- `P95` = 95th percentile of the same retained frames (default capacity 120).
- `DROP` = share of the retained frames whose **CPU draw** exceeded one refresh period
  (`over_budget_ratio`, `sampler.rs:191`); the budget is the panel's period from
  `EnumDisplaySettingsW` (60 Hz here, 16.7 ms). It is not missed-vsync or GPU drops. The HUD's
  own 500 ms clock frames are excluded (`expect_own_frame`).
- `GPU` %: HUD 1.9 % and 1.9 % vs an independent `Get-Counter "\GPU Engine(pid_<pid>_*)\
  Utilization Percentage"` (busiest engine type, `3d`, summed within the type, 3 x 1 s) of
  1.96 % and 1.79 %: within 0.2 pt (`gpucmp` run, idle HUD-on window). Task Manager itself was
  not read: its window is outside the launched pid.
- `MEM` (kit, private commit, 126 MB) and the status bar `MEM` (private working set, 85.8 MB)
  disagree on screen at the same moment; `docs/gui-layout.md` already says why.

### F6 — `DEVICE` and `API` (Info)

- `DEVICE`: `gpu_specs()` reads `GetDesc1` of `devices.adapter`
  (`gpui-pre-windows` `directx_renderer.rs:823`), the same `IDXGIAdapter1` the D3D11 device was
  created on in `get_adapter` (`directx_devices.rs:106`: the first `EnumAdapters` entry that
  creates a D3D11 device at 11.1/11.0/10.1). So on an iGPU + dGPU machine it names the adapter
  that renders, by construction; DXGI enumerates the adapter picked by the Windows per-app GPU
  preference first. Not testable here (one adapter); the HUD shows `Intel(R) UHD Graphics 770`,
  matching `Win32_VideoController` and gpui's own `Using GPU:` log line.
- `API`: `Direct3D 11` is right for gpui-pre 0.3.7 on Windows (`gpui-pre-platform` depends on
  `gpui-pre-windows` only on Windows; its renderer is `directx_renderer.rs`, no wgpu). The
  feature level is only logged (`directx_devices.rs:54-61`); `GpuSpecs` has four public fields
  (`is_software_emulated`, `device_name`, `driver_name`, `driver_info`) and no feature level,
  so it is not reachable through public API. `driver_info` (the driver version) and
  `driver_name` (the vendor) are reachable and unused: a one-line `DRIVER` row if the owner
  wants it (not requested).

## Checks that passed

- **Default off, not written while off.** `cargo test -p oneterm-workspace -p oneterm-settings
  -p oneterm-actions -p oneterm-settings-ui`: 4 + 49 + 54 (1 ignored) + 44 (3 ignored) passed.
  Mutations on `crates/settings/src/ui_config.rs` (script restores the file; `git status` clean
  after): M1 `#[serde(default = "show_fps_on")]` returning `true` ->
  `ui_config::tests::legacy_partial_schema_uses_current_defaults` FAILED; M2 dropping
  `skip_serializing_if` (so "off" is written) -> the same test FAILED. Both killed.
- **Toggles in the GUI** (1280 x 800 unless stated):
  - A user binding in `ui_config.json` (`"toggle_fps": "f9"`) toggles it and rewrites
    `show_fps`; the app menu shows the binding next to the item.
  - Restart with `"show_fps": true`: the HUD is up on the first frame and the app menu's
    **Show FPS Monitor** is checked (`US-0151-verify-menu-checked-after-restart.png`).
  - **Settings > General > Interface > Show FPS Monitor** reflects the state; clicking it off
    removed the HUD and `show_fps` from the file, clicking it on restored both; toggling from
    the main window with the key moved the open Settings switch
    (`US-0151-verify-settings-switch-synced.png`).
  - **Settings > Key Bindings > App Menu > Toggle FPS Monitor** shows `Default: (unbound)`;
    Reset removed the user binding from the file; Edit + F8 recorded `"toggle_fps": "f8"`
    (`US-0151-verify-key-binding-recorded.png`), after which F8 toggled the HUD and F9 did not.
- **Placement.** Top of the HUD at y = 78 (34 title bar + 32 tab strip + 12). At 800 x 600 with
  the SSH Client dock it covers no caption button, dock toggle, tab-strip `+` / `...` or panel
  header (`US-0151-verify-800x600.png`); the tab height is the kit's fixed 32 px
  (`gpui-component` `tab.rs`), independent of the UI font size. Popups (the terminal context
  menu) draw above it.
- **Light theme.** Ayu Light: the kit HUD keeps its dark, near-opaque panel and stays legible;
  the strip follows the theme (`US-0151-verify-ayu-light.png`).
- **Records.** Intake, HLD and packet follow `docs/templates/` (dated 2026-09-29; owning docs,
  documentation action, reconciliation, evidence, gaps, handoff present).
  `docs/agents/dependencies.md` § 1 row and rule 2 name `gpui-fps` and its `profiler` side
  effect. `THIRD-PARTY-NOTICES.md` adds `gpui-fps` 0.7.0 and `hdrhistogram` 7.6.0 (905 -> 907);
  `Cargo.lock` adds only those two (`windows 0.58`, `sysinfo 0.37` were already locked).
  `cargo deny check licenses bans advisories`: `advisories ok, bans ok, licenses ok`.
- **Gate.** `pwsh scripts/ci-local.ps1` with `CARGO_BUILD_JOBS=2`, this worktree's target dir,
  after deleting `target/release` and `target/debug/incremental`, exit 0, final line
  `ci-local: all checks passed.`

## Record corrections to make (not made here)

1. Intake § Requested Outcome and HLD § Cost: "While off it costs nothing at run time" ->
   state F1 (4 MiB journal resident; +55-65 ns per task poll; no per-frame allocation) and the
   owner's sign-off once given.
2. Packet § Evidence and Gaps and `docs/gui-layout.md` § FPS HUD: replace the "PDH DLLs and
   allocator pages" explanation with F2; describe "on" memory as F3.
3. `docs/gui-layout.md` § FPS HUD: state F4 (or fix it) and the `DROP` / `MAX FPS` definitions
   of F5.
4. Harness rows below (`harness.db` was read only).

## Proposed harness rows

`intake` (12 columns; `id` = rowid 53, `document_number` 48):

```text
id=53, created_at='2026-09-29T00:00:00', input_type='new_spec',
summary='FPS HUD: kit gpui-fps HUD plus DEVICE/API strip, off by default (show_fps), toggled from the app menu, Settings > General and an unbound action (owner 2026-09-29)',
risk_lane='normal',
risk_flags='new dependency gpui-fps enables gpui-pre profiler graph-wide (4 MiB journal, per-poll bookkeeping even when off); one optional persisted field',
affected_docs='docs/gui-layout.md; docs/agents/persistence.md; docs/agents/dependencies.md',
story_id=NULL, doc_path='docs/spec-intakes/IN-0048-fps-hud/IN-0048.md',
notes='US-0151 impl 09812f0d; verify PASS (F1 off-cost needs owner sign-off, option c).',
document_number=48, design_doc='docs/spec-intakes/IN-0048-fps-hud/high-level-design.md'
```

`story` (17 columns; `intake_id` 53):

```text
id='US-0151', title='FPS HUD, off by default', created_at='2026-09-29T00:00:00Z',
risk_lane='normal', contract_doc='docs/gui-layout.md',
packet_doc='docs/spec-intakes/IN-0048-fps-hud/US-0151-fps-hud.md', status='implemented',
unit_proof=1, integration_proof=0, e2e_proof=1, platform_proof=0,
evidence='docs/spec-intakes/IN-0048-fps-hud/evidence/US-0151-verify.md',
verify_command='cargo test -p oneterm-settings -p oneterm-actions -p oneterm-workspace -p oneterm-settings-ui; pwsh scripts/ci-local.ps1',
last_verified_at='<commit time of the verify commit>', last_verified_result='pass',
notes='Impl 09812f0d. Verify PASS: toggles/menu/settings/key row walked; 20 toggles no ratchet; ~30 MB after off = gpui path MSAA textures (released on resize); off-cost +4 MB and +55-65 ns/task poll, 0 per-frame allocs (owner sign-off, option c); click on HUD reaches the terminal; records corrections F2-F5 pending.',
intake_id=53
```

Numbering note: `US-0150` exists neither in `harness.db` nor under `docs/`; the implementer
skipped it.

## Gaps

- Windows only; one adapter (the iGPU + dGPU case is argued from gpui's source, not run).
- `present` and input dispatch are not in the micro-bench (gpui-private); their bookkeeping is
  the same kind as `draw`'s.
- hotpath builds were not used; frames/s off was not re-counted (the profiler feature changes no
  scheduling; the implementer's 2.04 / 2.20 stands), UI time came from `QueryThreadCycleTime`.
- One flood length (100k lines) and three pairs; fast-dev leaves OneTerm crates and `gpui-fps`
  at opt-level 0, so absolute on-costs are higher than in a release build.
- Task Manager's GPU column was not read (window outside the launched pid).

## Second pass: acceptance rework `2a8a7e65` (2026-09-29)

Verifier: Claude Fable 5.1. Target: `feat/fps-hud` @ `2a8a7e65` (own four-row HUD, no
`gpui-fps`). Same machine and method as above. Builds: `fast-dev`, plain and with
`hotpath-profiling`, of `2a8a7e65` in this worktree's own target dir, `CARGO_BUILD_JOBS=3`.
Main is the `e0db223f` fast-dev from the first pass. Private `USERPROFILE`, own pids only.

### Verdict

**PASS.** The zero-cost-when-off invariant now holds, both by construction and by measurement.
The four rows are honest. The HUD adds no frame in steady state. The PDH query is released
every time, and 20 toggles do not ratchet. Two Low findings (R1, R2) should be fixed or
reworded before or right after the merge; the rest are Info.

### Findings

| # | Severity | Finding |
| --- | --- | --- |
| R1 | Low | The first PDH open runs on the UI thread: about 165 ms, once per process |
| R2 | Low (records, or about 3 lines) | `GPU usage` sums engines per *type*; Task Manager shows the busiest single *engine* |
| R3 | Info | The one-time ~4 MB left after the first show is PDH's own initialisation; bounded, no ratchet |
| R4 | Info (cosmetic) | The GPU name wraps at 220 px |
| R5 | Info | The HUD blocks clicks on whatever it covers (by design), e.g. the right part of the SSH search box |
| R6 | Info (records) | Text from the first cut is still in the packet and reads as current |

**R1: the first PDH open runs on the UI thread (Low).** `FpsHud::new` runs inside
`OneTermWorkspace::render` and calls `gpu::Probe::open()` there: `PdhOpenQueryW`,
`PdhAddEnglishCounterW` and a baseline `PdhCollectQueryData` over `\GPU Engine(*)`. Only the
periodic samples run on the background executor, so the packet's "never on the UI thread" is
not exact. Measured:

- In a separate process (`pdhprobe.ps1`, the same three calls), the first open takes
  **165 ms**; later opens take 0.6-1.0 ms (579 instances).
- In OneTerm, the UI thread spent **368 Mcycles in the second after the first show**. That
  compares with 25-70 Mcycles in the second after each of the 19 later shows, and 18 with no
  toggle.

The result is a one-time hitch of roughly 100 ms per process when the HUD is first shown. It
also happens at startup when `show_fps` is saved as on.

Fix (about 5 lines): open the probe in the ticker's first background spawn, as `gpui-fps` did
(`probe = executor.spawn(async { gpu::Probe::open() })`). The row already has a `Pending` state
for the wait.

**R2: busiest engine type vs Task Manager's busiest engine (Low).** Microsoft defines the
Processes tab's GPU column as "the utilization of the busiest engine, across all GPUs"
([GPUs in the task manager](https://devblogs.microsoft.com/directx/gpus-in-the-task-manager/)).

- `busiest_engine` does something different: it sums this pid's instances per engine *type*
  (and across adapters), then takes the busiest type.
- The two differ when two engines of the same type are busy at once. This iGPU exposes two
  `VideoProcessing` and two `VideoDecode` engines per process (instances `eng_1`/`eng_4` and
  `eng_3`/`eng_11`), so hardware video decode could read up to 2x Task Manager.
- On this machine the results are identical in practice, because only `3D` is busy.

Two ways to resolve it:

- Take the max over this pid's instances. The code gets simpler (no per-type table); the test's
  expected 1.75 changes, and M5 still kills a plain sum.
- Or reword `docs/gui-layout.md`, the HLD and the packet to say "summed per engine type"
  instead of "what Task Manager shows".

**R3: the ~4 MB left after the first show (Info).** 20 toggles (F9, 5 s on / 5 s off,
1280 x 800):

| Point | Commit | Private WS | Handles |
| --- | --- | --- | --- |
| Baseline, HUD off | 109.9 MB | 50.6 MB | 498 |
| After the first on/off | 114.0 MB | 54.8 MB | 512 |
| Every later off state | 113.8-114.7 MB | 54.5-55.6 MB | 505-509 |
| End of the run, and 30 s later | 113.9 MB | 54.7 MB | 505 |

- The first show costs a one-time +4.1 MB and +7 handles, which stay flat for the next 19
  cycles: no ratchet.
- Each show opens the query and each hide closes it (+2 handles on, -2 off).
- Threads went from 19 to 16 (the thread pool trimming).

The standalone probe shows the same shape: the first open takes 165 ms and about 3 MB, and
later opens stay flat. That is PDH's per-process counter-name and provider initialisation,
which Windows keeps once loaded. It is bounded, and not OneTerm's to release.

**R4: the GPU name wraps (Info, cosmetic).** At `HUD_WIDTH` 220 px in `text_xs` mono, the value
column holds about 23 characters. `Intel(R) UHD Graphics 770` (25) wraps to two lines in every
screenshot, and `NVIDIA GeForce RTX 4060 Laptop GPU` (34) would too. Two possible fixes:

- Keep the name on one line with `.whitespace_nowrap().overflow_hidden().text_ellipsis()` on the
  value, and show the full name in a tooltip.
- Or replace the fixed width with `.min_w(HUD_WIDTH)`, so the box grows to fit the name (it only
  extends to the left).

**R5: the HUD blocks what it covers (Info).** `block_mouse_except_scroll` works as claimed (see
Mouse below). The flip side: the part of the panel under the HUD cannot be clicked. With the SSH
Client dock, that is the right ~40 % of "Search sessions..." at 1280 px and most of it at
800 px. This is by design, and worth one line in `docs/gui-layout.md`.

**R6: first-cut text still in the packet (Info, records).**

- The packet still has the first cut's Acceptance list (MAX FPS, FRAME, P95, DROP, the
  DEVICE/API strip, "no monitor entity"). Its Scope still says "out of scope ... the kit HUD's
  palette". Its Context still says "gpui-fps depends on ...". Its Documentation Action still
  lists "`gpui-fps` in the 0.7 family". All of these should be marked as superseded by
  § Acceptance rework.
- The cost lines in the HLD and `docs/gui-layout.md` should add R1 and R3: a one-time ~165 ms
  and ~4 MB on the first show in each process.
- `docs/gui-layout.md` is missing a blank line before `## Text contrast floor and hierarchy`.

### Measurements

**(1) HUD off = main.**

- **Dependencies:** `git diff e0db223f 2a8a7e65` shows no change to `Cargo.lock` or
  `THIRD-PARTY-NOTICES.md`. The only new manifest item is the `windows-sys` feature
  `Win32_System_Performance`.
- **gpui features:** `cargo tree -e features -i gpui-pre` shows no `profiler`. The features are
  `default`, `font-kit`, `wayland`, `x11` and `windows-manifest`, plus the dev-only
  `test-support`.
- **So gpui is compiled exactly as on main,** and the first pass's "profiler OFF"
  micro-benchmark numbers apply to this build: draw min 54-58 µs, 258 allocations /
  174,432 bytes per draw, and 54-69 ns per task-run hook.
- **Modules:** main already loads `pdh.dll` at startup (via `sysinfo`); both builds load 81
  modules.
- **Render path:** with the HUD off, `OneTermWorkspace::render` does one `UiConfig` global read
  and one `Option` check, with no allocation (`children(None)`).

Process runs (idle 60 s focused, then a 100k-line flood; two pairs). A concurrent build made the
flood row noisy.

| | main | rework, HUD off |
| --- | --- | --- |
| Idle UI thread, Mcycles/s | 30.3, 26.9 | 26.7, 28.3 |
| Idle process, Mcycles/s | 66.2, 57.6 | 54.2, 61.6 |
| Commit / private WS after ~68 s | 110.3 / 51.2, 110.6 / 51.4 MB | 112.2 / 53.1, 110.1 / 50.9 MB |
| Flood wall time | 19.4, 17.3 s | 16.3, 21.6 s |

**(2) Frames, counted independently by hotpath** (`OneTermWorkspace::render` calls). The
steady-state rate is (count at 180 s - count at 60 s) / 120 s, which removes startup.

| Window | HUD off | HUD on |
| --- | --- | --- |
| Inactive (no blink; clock ticks 1/s; WM_ACTIVATE re-posted every 10 s) | 60 s: 76, 73; 180 s: 215 -> **1.17/s** | 60 s: 78, 84; 180 s: 211 -> **1.08/s** |
| Active (cursor blink 2/s) | 60 s: 121; 180 s: 379 -> **2.15/s** | 60 s: 139; 180 s: 389 -> **2.08/s** |

- If the HUD's 1 s refresh were not aligned with the clock, the inactive rate would have
  doubled. It stays at about 1.1/s, so the HUD's tick lands in the clock's frame: one frame,
  not two.
- Startup with the HUD on draws a few more frames (+2 to +18 in the first minute: creation and
  the first ticks), then no more.

**(2) FPS honesty.** HUD on, 300k-line flood lasting 40.7 s:

- hotpath counted 2,555 renders in 59.1 s, about 2,490 of them during the flood, so **about
  61 frames/s**.
- 26 PrintWindow captures taken during the flood read 58, 59, 60 or 61 every time.
- At idle, the HUD reads 2 (window active) or 1 (inactive), matching the table above.

**(2) GPU usage.** Compared against an independent `Get-Counter "\GPU Engine(pid_<pid>_*)\
Utilization Percentage"` reading (busiest type, 3 x 1 s):

- Idle: HUD 0.4 %, Get-Counter 0.38 / 0.38 / 0.39 %.
- Under `ping -t` output: HUD 1.2 %, Get-Counter 1.16 / 1.18 / 1.57 %.

They agree within 0.1 point when the load is steady. Task Manager's own window was not read.

**(3) Mouse and layout.** With the right dock set to None, the HUD lies over the terminal:

- The wheel over the HUD scrolls the terminal back (rows 176-194 shown).
- A drag that starts on the HUD selects nothing.
- A terminal selection survives a left click on the HUD.
- A right-click on the HUD opens no context menu.

At 800 x 600 with the SSH Client dock, the HUD starts at y = 78, clear of the tab strip `+` and
`...`, the dock toggles and the caption buttons. In Ayu Light and Zed One Dark the HUD follows
the theme (popover fill, border, muted labels, `popover.foreground` values) and is legible in
both. The contrast script passes in the gate.

**(4) Tests and mutations.** `cargo test` runs in the gate for all crates. Each mutation was
restored afterwards, and all five were killed:

| Mutation | Test that fails |
| --- | --- |
| M1: `show_fps` serde default `true` | `ui_config::tests::legacy_partial_schema_uses_current_defaults` |
| M2: owner prefix `pid_{pid}` without the trailing `_`, so `pid_421_` leaks into `pid_42` | `gpu_usage_is_this_pids_busiest_engine_type` |
| M3: no zero-elapsed guard in `per_second` | `n_renders_in_a_second_read_n_fps` |
| M4: no 100 % clamp | the GPU test |
| M5: plain sum over all engine types | the GPU test |

**(5) Records.** The intake, HLD and packet carry the four-row field list and a dated
§ Acceptance rework. `docs/agents/dependencies.md` § 1 has no `gpui-fps` row (rule 2 and § 2 are
back to main's), and § 3 names `Win32_System_Performance` and why `gpui-fps` is not used. R6 lists
what is still out of date.

**(6) Gate.** `pwsh scripts/ci-local.ps1`, `CARGO_BUILD_JOBS=3`, own target dir, after deleting
`target/release`, `target/debug/incremental` and `target/fast-dev`. It exited 0 with the final
line `ci-local: all checks passed.`

### Proposed harness rows (final)

`intake` (12 columns): as in the first pass (`id` 53, `document_number` 48, `new_spec`,
`normal`, `doc_path` `docs/spec-intakes/IN-0048-fps-hud/IN-0048.md`, `design_doc`
`docs/spec-intakes/IN-0048-fps-hud/high-level-design.md`), with
`risk_flags='one optional persisted field; Windows PDH FFI (cfg(windows)); no new dependency'`,
`affected_docs='docs/gui-layout.md; docs/agents/persistence.md; docs/agents/dependencies.md'`,
`notes='Owner ruling 2026-09-29: four rows (FPS, GPU, API, GPU usage), no gpui-fps. US-0151 first cut 09812f0d (verify 66ae88b9), rework 2a8a7e65, verify #2 PASS.'`.

`story` (17 columns): `id='US-0151'`, `title='FPS HUD, off by default'`,
`created_at='2026-09-29T00:00:00Z'`, `risk_lane='normal'`, `contract_doc='docs/gui-layout.md'`,
`packet_doc='docs/spec-intakes/IN-0048-fps-hud/US-0151-fps-hud.md'`, `status='implemented'`,
`unit_proof=1`, `integration_proof=0`, `e2e_proof=1`, `platform_proof=0`,
`evidence='docs/spec-intakes/IN-0048-fps-hud/evidence/US-0151-verify.md'`,
`verify_command='cargo test -p oneterm-settings -p oneterm-actions -p oneterm-workspace -p oneterm-settings-ui; pwsh scripts/ci-local.ps1'`,
`last_verified_at='<verify #2 commit time>'`, `last_verified_result='pass'`,
`notes='Rework 2a8a7e65 (own HUD, no gpui-fps). Verify #2 PASS: off = main (no profiler, lock/notices identical); steady frames on = off (1.08 vs 1.17/s inactive); FPS 58-61 vs hotpath ~61 in a flood; GPU usage within 0.1 pt of Get-Counter; 20 toggles no ratchet (one-time +4 MB PDH). R1 first PDH open on UI thread ~165 ms; R2 busiest type vs Task Manager busiest engine; R4 name wraps.'`,
`intake_id=53`.

### Gaps (second pass)

- Windows only, with one adapter. The Linux and macOS rows are `n/a` or constants by design,
  and were not run.
- The first-show hitch (R1) is inferred from UI-thread cycles and a standalone timing, not from a
  frame-time trace.
- The R2 difference is argued from the instance list and Microsoft's definition; it was not
  reproduced with hardware video decode.
- Task Manager's GPU column was not read: its window is outside the launched pid.
