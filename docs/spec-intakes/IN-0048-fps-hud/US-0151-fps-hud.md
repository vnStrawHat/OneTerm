# Work: FPS HUD, off by default

ID: US-0151
Intake: IN-0048
Created: 2026-09-29

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: normal (a view overlay and one optional persisted boolean; no public contract)
- Spec Intake, when required: [`IN-0048`](IN-0048.md)

## Outcome

A user can switch on a performance HUD when they need it and off again; it is off on a fresh
profile and after an upgrade. While on it shows four rows — FPS, GPU (name), API and GPU
usage — from OneTerm's own overlay; while off it runs nothing
([`high-level-design.md`](high-level-design.md)). (Acceptance rework 2026-09-29: the first
cut's GPUI Kit HUD and its ten readings are gone; see § Acceptance rework.)

## Scope

- [x] In scope:
  - root `Cargo.toml`: the `windows-sys` feature `Win32_System_Performance` (rework; the first
    cut's `gpui-fps` dependency is removed, `Cargo.lock` and `THIRD-PARTY-NOTICES.md` are main's).
  - `crates/settings/src/ui_config.rs`: `show_fps` (serde default `false`, omitted while
    false) and `UiConfig::set_show_fps` (update, persist, refresh windows).
  - `crates/actions`: `ToggleFpsMonitor`; `elevated_policy.rs` classifies `toggle_fps`
    allowed (window chrome).
  - `crates/workspace`: `widgets/fps_hud.rs` (the four-row overlay and its PDH sampler), the root render, the app-menu
    item and the action handler in `layout/app_menus.rs`.
  - `crates/settings-ui`: the General page switch; the Key Bindings row (unbound).
  - Docs: `docs/gui-layout.md`, `docs/agents/persistence.md`, `docs/agents/dependencies.md`.
- [x] Out of scope: the Direct3D feature level at run time, a refresh-rate / "max" FPS row,
  and GPU usage on Linux/macOS (intake follow-ups). (The first cut's "kit HUD palette" line is
  superseded: there is no kit HUD any more.)

## Acceptance

> **Superseded** by § Acceptance rework (first cut `09812f0d`); kept as the record of what the
> owner ruled on.

- [x] `show_fps` is `false` when `ui_config.json` lacks it, and survives a save/load round trip.
- [x] The `ToggleFpsMonitor` action flips it; the app menu shows a checked "Show FPS Monitor"
  item that tracks it; Settings > General > Interface has the same switch; Settings > Key
  Bindings lists "Toggle FPS Monitor" unbound (DEC-0018: no default chord).
- [x] On: the overlay sits top-right under the tab strip and covers no title-bar or tab-strip
  control; it shows MAX FPS, FRAME, P95, DROP, GPU %, DEVICE and API (plus the kit's
  INTERVAL, INV, CPU, MEM).
- [x] The DEVICE/API strip formats a fixed sample in a fixed order (unit test).
- [x] Off: no monitor entity exists, so no clock and no frame trace; idle frames/s stays
  about 2 (US-0145) and the idle UI-thread cycles are measured against main.
- [x] GPU % roughly agrees with Task Manager's GPU column for the same pid.
- [x] Theme tokens only in OneTerm code; `python scripts/check-theme-contrast.py` passes.
- [x] Full `pwsh scripts/ci-local.ps1` green; `cargo deny` if installed.

## Acceptance rework — 2026-09-29

Reopened on the owner's acceptance ruling of 2026-09-29, after the first cut (`09812f0d`) and
its verification (`evidence/US-0151-verify.md`, commits `66ae88b9`, `30e12f20`):

> The HUD shows ONLY fps, GPU name, Graphics API and GPU % usage, and must NOT depend on
> `gpui-fps`.

Why: `gpui-fps` turns on `gpui-pre`'s `profiler` feature for every build, which costs +4 MB
and +55-65 ns per task run with the HUD off (verify F1); its frame-time chart made gpui's
window-sized path textures resident (F2, the "30 MB"); a left click or drag on it reached the
terminal (F4).

### Reworked acceptance

- [x] `gpui-fps` is gone and with it the `profiler` feature: `Cargo.lock` and
  `THIRD-PARTY-NOTICES.md` are byte-for-byte main's again (`git diff e0db223f -- Cargo.lock
  THIRD-PARTY-NOTICES.md` is empty); the only manifest change is the `windows-sys` feature
  `Win32_System_Performance`.
- [x] OneTerm's own overlay (`crates/workspace/src/widgets/fps_hud.rs`), theme tokens only, four
  rows in this order:
  - `FPS` — frames the window actually drew in the last second: the HUD view (not cached, so it
    renders on every window frame) adds one to a `u64` per render; once a second, on the
    `until_next_tick` grid, the count divided by the elapsed time is published and reset. It
    forces no frame beyond that once-a-second refresh, which shares the status bar clock's tick,
    so **it reads about 2 at idle and the real rate under output**; it is not a refresh rate or a
    "max" (a follow-up if wanted).
  - `GPU` — `Window::gpu_specs().device_name`, asked once when shown; `(software)` / `n/a`.
  - `API` — compile-time constant: `Direct3D 11` / `Metal` / `Vulkan/GL (wgpu)`.
  - `GPU usage` — own PDH sampler on `\GPU Engine(*)\Utilization Percentage`: this pid's
    instances, **busiest single engine across all GPUs** (how Task Manager defines its GPU
    column), clamped to 100 (verify #2 R2; the rework first summed per engine type). The query is opened when the HUD is created and closed (`PdhCloseQuery`) when
    it is dropped; the open (verify #2 R1) and one collection every 2 s run on the background
    executor, never on the UI thread;
    the FFI is `cfg(windows)`, `n/a` elsewhere.
- [x] Off costs nothing: no entity, so no ticker, no count, no PDH query, no allocation.
- [x] F4: the overlay is opaque to the mouse except the wheel (`block_mouse_except_scroll`):
  clicks and drags on it do not reach the terminal; the wheel still scrolls it.
- [x] Setting, action, menu item, Settings switch and Key Bindings row unchanged; tests kept,
  plus PDH parsing of a fixed sample and the FPS arithmetic.

### Rework evidence (Windows 11, Intel UHD 770, 60 Hz; fast-dev + `hotpath-profiling`, private `USERPROFILE`, own pid only)

Idle, focused, one cmd.exe tab, 1280 x 800, 60 s measured after a 5 s settle. Frames/s =
`OneTermWorkspace::render` calls / whole run (~72.6 s, startup included); UI cycles =
`QueryThreadCycleTime` of the UI thread over the 60 s; memory read at the end of the run.
Main is `e0db223f` built the same way.

| Build, HUD | Frames/s | UI Mcycles/s | Private commit | Private working set |
| --- | ---: | ---: | ---: | ---: |
| main, run 1 / run 2 | 2.10 / 2.19 | 40.95 / 32.01 | 117.1 / 117.2 MB | 58.8 / 58.9 MB |
| rework, off, run 1 / run 2 | 2.15 / 2.28 | 40.99 / 37.06 | 117.3 / 117.3 MB | 58.8 / 59.0 MB |
| rework, on, run 1 / run 2 | 2.33 / 2.26 | 42.53 / 38.75 | 122.3 / 121.0 MB | 64.1 / 62.8 MB |
| rework, on, then switched off from the menu (60 s later) | - | 36.45 | - | 62.6 MB |

Allocations (`hotpath-profiling-alloc`, count metric, same idle run): `OneTermWorkspace::render`
allocates **13 per call** in main (150 calls), in the rework with the HUD off (161 calls) and on
(158 calls) — the off path and the on path add no allocation to the root render.

Reading:

- **Off = main.** Frames/s, UI cycles and memory are within run-to-run spread of main; the
  +4 MB of the first cut is gone (117.3 vs 117.2 MB commit, 59.0 vs 58.9 MB working set). What
  the off path runs is one `UiConfig` global read and an `Option` check in the root render.
- **On:** no extra frames (2.3 vs 2.2 frames/s: the HUD's refresh lands on the clock's tick),
  UI cycles within the spread, **+4-5 MB** commit and private working set (PDH's counter data
  for every engine of every process). About 3.7 MB of it stays after the HUD is switched off
  (62.6 vs 58.9 MB, the query itself is closed); not investigated further. No path textures: the
  overlay draws no path, so the first cut's window-sized ~20 B/px jump is gone.
- **FPS row:** reads 2-3 at idle (screenshots), 10-14 while the F4 drags below repainted.
- **GPU usage:** the HUD read 0.5-0.6 % idle and 2.6-3.1 % during the drags; an independent
  `Get-Counter "\GPU Engine(pid_<pid>_*)\Utilization Percentage"` (same pid, busiest engine
  type) read 0.66-2.77 % in the same run at 10 s intervals. Same source, different sampling
  instants; they agree to within the counter's own churn. Task Manager's window was not read.
- **F4 in the GUI** (right dock None, so the HUD lies over the terminal): a drag that starts on
  the HUD and ends over terminal text selects nothing
  (`evidence/US-0151-rework-drag-from-hud-no-selection.png`); a selection made in the terminal
  survives a left click on the HUD (`evidence/US-0151-rework-click-on-hud-keeps-selection.png`).
- **Toggle:** OneTerm > Show FPS Monitor removed the HUD
  (`evidence/US-0151-rework-toggled-off.png`).

Screenshots: `evidence/US-0151-rework-hud-dark.png`, `evidence/US-0151-rework-hud-light.png`
(Ayu Light). In these the adapter name still wrapped at 220 px; since verify #2 R4 it stays on
one line and widens the HUD (`evidence/US-0151-rework2-one-line-name.png`).

Tests (`cargo test -p oneterm-settings -p oneterm-actions -p oneterm-workspace
-p oneterm-settings-ui`, all green): `widgets::fps_hud::tests::the_rows_are_fps_gpu_api_and_usage_in_that_order`,
`...::n_renders_in_a_second_read_n_fps`, `...::gpu_usage_is_this_pids_busiest_single_engine`
(PDH instance names -> percent, including a `pid_421_` neighbour of `pid_42_` and the clamp),
plus the kept `the_toggle_action_flips_show_fps` and the `ui_config` default/round-trip tests.

Rework gates: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`,
also with `--features oneterm-app/hotpath-profiling`; the four crates' tests; contrast (1482
pairings >= 4.5:1); dependency graph; notices; doc paths; English; `cargo deny check licenses
bans advisories` (`advisories ok, bans ok, licenses ok`); full `pwsh scripts/ci-local.ps1`
(`CARGO_BUILD_JOBS=3`): `ci-local: all checks passed.`

Rework gaps:

- Linux/macOS not run; their `GPU usage` is `n/a` by design, `GPU`/`API` as before.
- The ~3.7 MB left after switching the HUD off is attributed to PDH's process-wide counter
  data from reasoning, not measured apart.
- `FpsHud::render` itself is not a hotpath site, so its own allocations while on (the row
  strings, about a dozen small ones per frame) are not in the alloc table above.
### Verify #2 fixes — 2026-09-29

The rework's verification (`evidence/US-0151-verify.md` § Second pass, `25f85759`) passed with
two Low findings and records notes, fixed here:

- **R1:** the PDH query is opened in the ticker's first step on the background executor, not in
  `OneTermWorkspace::render`. The `GPU usage` row reads `-` until the first sample (or `n/a` if
  the open fails). UI-thread cycles in the second after the **first** show of the HUD in a
  process (fast-dev + `hotpath-profiling`, own pid, HUD toggled on from the app menu; idle
  seconds before it for scale), three runs each:

  | Build | Idle seconds before | Menu open | First-show second | Next second |
  | --- | --- | --- | --- | --- |
  | `2a8a7e65` (open on the UI thread) | 31-69 | 22.8-23.3 | **365.2, 325.3, 381.4** | 21-49 |
  | this fix | 34-59 | 28.6-32.6 | **48.1, 46.1, 35.5** | 39-48 |

  Mcycles. The first-show second is now inside the idle spread: no hitch. The ~165 ms and the
  one-time ~4 MB of PDH's first initialisation (R3) now land on a background thread.
- **R2:** `GPU usage` is this pid's busiest single engine across all GPUs (max over instances),
  which is how Task Manager defines its GPU column; the per-type sum could read up to 2x on
  this iGPU's duplicate VideoDecode/VideoProcessing engines. Test
  `gpu_usage_is_this_pids_busiest_single_engine` (1.25 from a sample whose type sum is 1.75,
  two busy VideoDecode engines read 40 not 70, clamp at 100).
- **R4:** the GPU name stays on one line: the HUD's width is a 220 px minimum and the value does
  not wrap, so a longer name widens the box leftwards
  (`evidence/US-0151-rework2-one-line-name.png`: `Intel(R) UHD Graphics 770` on one line).
- **R5/R6:** `docs/gui-layout.md` says the HUD blocks clicks on what it covers; the first cut's
  Acceptance, Scope line, Context and Documentation Action are marked superseded; R1/R3 are in
  the HLD and `docs/gui-layout.md` cost lines; the missing blank line before
  `## Text contrast floor and hierarchy` is back.
## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` — frame, status bar, settings pages, "Frames and re-rendering" (why a
  notify is a whole-window frame; the 500 ms timer grid).
- `docs/agents/persistence.md` — `ui_config.json` row, elevated write refusal.
- `docs/agents/dependencies.md` — § 1 family table and rule 2, § 3, § 5 reference-first.
- `docs/agents/crate-dependency-rules.md` — R2/R4 (the shell may depend on `settings`,
  `actions`; no feature crate).
- `docs/decisions/DEC-0018-app-shortcuts-leave-single-ctrl-keys-to-the-terminal.md` — no
  default chord for a view toggle.
- `docs/decisions/DEC-0019-*` (via `elevated_policy.rs`) — every bindable id is classified.
- `reference/gpui-kit/crates/fps/` (README, Cargo.toml, `lib.rs`, `monitor.rs`,
  `overlay.rs`, `style.rs`, `gpu*.rs`, `refresh.rs`) and the story's use of it
  (`reference/gpui-kit/crates/story/src/lib.rs`, `title_bar.rs`, `themes.rs`).

### Documentation Action

> **Superseded** by § Acceptance rework (first cut `09812f0d`); kept as the record of what the
> owner ruled on.

- Update required: `docs/gui-layout.md` (new § FPS HUD, General page row),
  `docs/agents/persistence.md` (`show_fps`), `docs/agents/dependencies.md` (`gpui-fps` in the
  0.7 family and its `profiler` feature side effect).

Reason: a new user-visible overlay, a new persisted field and a new dependency.

### Reconciliation

Changed: `docs/gui-layout.md` (§ FPS HUD, the General page row, source map),
`docs/agents/persistence.md` (`show_fps`), `docs/agents/dependencies.md` (§ 1 row and rule 2,
§ 2 declaration), `scripts/check-theme-contrast.py` (comment citations only: the strip draws
`popover.foreground` and `muted.foreground` on `popover.background`, a pairing already in
`SURFACES`). Rework: `docs/agents/dependencies.md` § 1 row, rule 2 and § 2 back to main, § 3
Windows FFI row names `Win32_System_Performance` and why `gpui-fps` is not used;
`docs/gui-layout.md` § FPS HUD rewritten (four rows, sources, mouse behaviour, cost);
intake and HLD field lists; `THIRD-PARTY-NOTICES.md` regenerated back to main's.

## Context

> **Superseded** by § Acceptance rework (first cut `09812f0d`); kept as the record of what the
> owner ruled on.

- `gpui-fps` depends on `gpui-pre =0.3.7` with `features = ["profiler"]`; Cargo unifies that
  feature into OneTerm's `gpui-pre`, which is why the "off" cost is measured against main and
  not assumed.
- The kit HUD is a view whose readout clock notifies it every 500 ms; a notify is a
  whole-window frame in OneTerm (`docs/gui-layout.md` § Frames), so "on" adds up to two frames
  a second on an idle window.

## Plan

- [x] Records (this packet, intake, HLD).
- [x] Dependency + notices; setting + action + menu + settings page + key-binding row; overlay.
- [x] Tests; gates; GUI walk with measurements; docs; commit on `feat/fps-hud`.

## Decisions

None new. Placement is argued in the HLD; it is a view choice, not one future work inherits.

## Verification Plan

- Unit: `oneterm-settings` (default + round trip), `oneterm-workspace` (action flips the flag;
  strip formatting), `oneterm-settings-ui` (bindable-action tables, elevated classification).
- Gates: fmt, clippy `-D warnings`, contrast, dependency graph, notices, `cargo deny`, doc
  paths, English, then the full `pwsh scripts/ci-local.ps1`.
- GUI (fast-dev + `hotpath-profiling`, private `USERPROFILE`, own pid only): baseline main
  and this branch, idle 60 s focused, HUD off: `OneTermWorkspace::render` count / s and
  UI-thread Mcycles/s; this branch with the HUD on: the same, plus a PrintWindow screenshot
  and a Task Manager GPU comparison.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

> First cut (`09812f0d`), superseded by § Acceptance rework above; kept as the record of
> what the owner ruled on.

### What `gpui-fps` provides and what OneTerm adds

- Provided by the kit, used unchanged: `MAX FPS` (derived: `1 / mean draw`, capped by the
  display refresh; right-click shows the observed `FPS`), `INTERVAL`, `FRAME` (mean draw
  cost; the owner's "frame"), `P95`, `DROP` (% of frames over one refresh period), `INV`,
  `GPU` % (Windows: PDH `\GPU Engine(*)\Utilization Percentage` filtered to this pid, busiest
  engine type), `CPU`, `MEM`, the trace, the click-to-collapse tag, and the stop-when-hidden
  logic. No GPU % sampler was written: the kit already has one on the background executor.
- Added by OneTerm: the persisted switch and its three entry points, the placement (below
  the tab strip; the kit's default corner sits on the caption buttons), dropping the monitor
  entity while off, and the `DEVICE` (`Window::gpu_specs().device_name`, runtime) / `API`
  (compile-time constant) strip.
- Graphics API: **compile-time**, not read at run time. gpui-pre-windows 0.3.7 only creates a
  Direct3D 11 device and does not expose it; the feature level it got is in the log only.
  This machine logs `Created device with Direct3D 11.1 feature level.` and
  `Using GPU: Intel(R) UHD Graphics 770`; the HUD shows `Direct3D 11` and
  `Intel(R) UHD Graphics 770`.

### Unit (Windows)

`cargo test -p oneterm-settings -p oneterm-actions -p oneterm-workspace -p oneterm-settings-ui`:
all green, including `ui_config::tests::legacy_partial_schema_uses_current_defaults` (absent
`show_fps` = false, and false is not written), the round trip in
`explicit_path_roundtrip_and_corruption_quarantine_are_isolated` (`show_fps: true` survives),
`layout::app_menus::tests::the_toggle_action_flips_show_fps` (dispatching `ToggleFpsMonitor`
flips it both ways), `widgets::fps_hud::tests::the_strip_names_the_device_then_the_api`
(fixed sample, fixed order, `(software)` suffix, `n/a`), and the settings-ui tables that
require every bindable id to be classified for an elevated window.

### GUI (Windows 11, Intel UHD 770, 60 Hz; fast-dev + `hotpath-profiling`, private `USERPROFILE`, own pid only)

Idle, focused, one cmd.exe tab, 1280 x 800, 60 s measured after a 5 s settle. Frames/s =
`OneTermWorkspace::render` calls / whole run (startup included, ~72 s). Cycles = the UI
thread's `QueryThreadCycleTime` over the measured 60 s. fast-dev leaves OneTerm's own crates
at opt-level 0, so absolute cycles are higher than `US-0145`'s release numbers; compare rows.

| Build, HUD | Frames/s | UI Mcycles/s | Process CPU (% of a core) | Status bar MEM |
| --- | ---: | ---: | ---: | ---: |
| main @e0db223f, run 1 | 2.13 | 35.8 | 16.5 | 58.4 MB |
| main @e0db223f, run 2 | 2.12 | 38.4 | 15.3 | 58.6 MB |
| this branch, off, run 1 | 2.04 | 30.4 | 15.3 | - |
| this branch, off, run 2 | 2.20 | 32.5 | 14.8 | 63.1 MB |
| this branch, on | 3.99 | 65.3 | 20.7 | 92.7 MB |
| this branch, on then toggled off from the menu (60 s after) | ~2 (est.) | 38.2 | - | 89.3 MB |

- **Off costs nothing measurable in time.** Frames/s stays at ~2 (US-0145's figure) and the
  UI thread is within the run-to-run spread of main. The one real "off" cost is memory:
  `gpui-pre`'s `profiler` feature, which `gpui-fps` turns on for the whole graph, allocates a
  4 MiB foreground-journal ring (`MAX_JOURNAL_ENTRIES` in `profiler/journal.rs`), which
  matches the +4.5 MB private working set.
- **On:** +2 frames/s (the kit's 500 ms readout clock; it is not on OneTerm's
  `until_next_tick` grid), about +33 Mcycles/s on the UI thread in this unoptimized build,
  about +5 % of a core for the process (frames, frame trace, sysinfo + PDH probe every
  500 ms), and about +30 MB private working set, which **stays after the HUD is switched off**
  (89 MB 60 s later; the PDH/perf-counter DLLs and allocator pages are not returned). The
  kit's own `MEM` row reads 127-132 MB because it shows private commit, not the private
  working set.
- **GPU %:** the kit read 1.8-2.0 % while an independent `Get-Counter
  "\GPU Engine(pid_<pid>_*)\Utilization Percentage"` (Task Manager's GPU-column source,
  same pid, busiest engine type) read 1.7-2.2 %. Task Manager's window itself was not
  screenshotted.
- **Toggle:** clicking OneTerm > Show FPS Monitor (checked while on) removed the HUD in the
  next frame and rewrote `ui_config.json` without `show_fps`.

Screenshots (PrintWindow): `evidence/US-0151-hud-on-dark.png`, `evidence/US-0151-hud-on-light.png`
(Ayu Light: the kit HUD keeps its dark palette, the strip follows the theme),
`evidence/US-0151-app-menu-checked.png`, `evidence/US-0151-toggled-off.png`.

### Gates

`cargo fmt --all -- --check`, `python scripts/check-theme-contrast.py` (1482 pairings, all
>= 4.5:1), `python scripts/verify-dependency-graph.py`, `python scripts/third-party-notices.py
--check`, `cargo deny check licenses bans advisories` (advisories ok, bans ok, licenses ok),
`python scripts/check-doc-paths.py`, `python scripts/check-english.py`: pass. Full
`pwsh scripts/ci-local.ps1` (`CARGO_BUILD_JOBS=2`, own target dir): `ci-local: all checks passed.`

### Gaps

- The Settings > General switch and the Key Bindings row were not GUI-walked (both go
  through the same `UiConfig::set_show_fps` / action as the walked menu item; the tables are
  unit-tested).
- Linux and macOS not run: their `DEVICE` source and `API` constant are unproven there
  (`gpu_specs` is `None` on macOS 0.3.7, so it shows `n/a`).
- The ~30 MB that showing the HUD once leaves behind is not investigated further (kit /
  Windows PDH side).
- The Direct3D feature level and a frame counter are intake follow-ups.

## Handoff

Acceptance rework implemented on `feat/fps-hud` (on `30e12f20`); awaiting owner acceptance
and a re-verification.
