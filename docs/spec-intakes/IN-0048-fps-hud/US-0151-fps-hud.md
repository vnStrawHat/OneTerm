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
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: normal (a view overlay and one optional persisted boolean; no public contract)
- Spec Intake, when required: [`IN-0048`](IN-0048.md)

## Outcome

A user can switch on a performance HUD when they need it and off again; it is off on a fresh
profile and after an upgrade. While on it shows the kit's MAX FPS / INTERVAL / FRAME / P95 /
DROP / INV / GPU % / CPU / MEM readings plus OneTerm's DEVICE (GPU name) and API rows; while
off it runs nothing ([`high-level-design.md`](high-level-design.md)).

## Scope

- [x] In scope:
  - root `Cargo.toml`: `gpui-fps = "0.7"`; `Cargo.lock`; `THIRD-PARTY-NOTICES.md`.
  - `crates/settings/src/ui_config.rs`: `show_fps` (serde default `false`, omitted while
    false) and `UiConfig::set_show_fps` (update, persist, refresh windows).
  - `crates/actions`: `ToggleFpsMonitor`; `elevated_policy.rs` classifies `toggle_fps`
    allowed (window chrome).
  - `crates/workspace`: `widgets/fps_hud.rs` (monitor + strip), the root render, the app-menu
    item and the action handler in `layout/app_menus.rs`.
  - `crates/settings-ui`: the General page switch; the Key Bindings row (unbound).
  - Docs: `docs/gui-layout.md`, `docs/agents/persistence.md`, `docs/agents/dependencies.md`.
- [x] Out of scope: the Direct3D feature level at run time and a frame counter (intake
  follow-ups); changing the kit HUD's palette, rows or cadence (it is a published crate;
  `docs/PROJECT.md` forbids patching dependencies).

## Acceptance

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

- Update required: `docs/gui-layout.md` (new § FPS HUD, General page row),
  `docs/agents/persistence.md` (`show_fps`), `docs/agents/dependencies.md` (`gpui-fps` in the
  0.7 family and its `profiler` feature side effect).

Reason: a new user-visible overlay, a new persisted field and a new dependency.

### Reconciliation

Changed: `docs/gui-layout.md` (§ FPS HUD, the General page row, source map),
`docs/agents/persistence.md` (`show_fps`), `docs/agents/dependencies.md` (§ 1 row and rule 2,
§ 2 declaration), `scripts/check-theme-contrast.py` (comment citations only: the strip draws
`popover.foreground` and `muted.foreground` on `popover.background`, a pairing already in
`SURFACES`), `THIRD-PARTY-NOTICES.md` (regenerated: `gpui-fps`, `hdrhistogram`).

## Context

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

Implemented on `feat/fps-hud`; awaiting owner acceptance and a verifier.
