# US-0150 independent verification

- Date: 2026-09-29
- Commit under test: `fee8947e` (branch `perf/render-caching`: `89536cc6` title bar content
  cached, `e4133b7e` status bar cached, `fee8947e` records; on main `e0db223f`)
- Packet: [`US-0150`](../US-0150-render-caching.md); intake [`IN-0046`](../IN-0046.md); owning
  docs `docs/gui-layout.md` § Frames and re-rendering, research
  [`hotpath-evaluation.md`](../research/hotpath-evaluation.md) § 9
- Host: Windows 11 Enterprise 10.0.26200, 96 DPI, the worktree's own `target/`,
  `CARGO_BUILD_JOBS=3`, another agent building in parallel. Two release builds with the
  implementer's flags (thin LTO, line tables, no strip, `oneterm-app/hotpath-profiling`):
  `before` = main's `crates/workspace` (`e0db223f`), `after` = `fee8947e`. Every instance was
  launched by this verification with a private `USERPROFILE` and driven only through its own
  pid (posted messages, `PrintWindow`, UI Automation on its own window). One real-mouse step,
  noted in § 2.

## Verdict: FAIL (F1 blocks; small fix)

Drawing, input and invalidation are correct: every input that changes what the two cached
views draw reaches them, and 42 side-by-side pixel pairs against main (dark, light, a runtime
dark -> light switch, Agent mode, 12 / 16 / 20 / 32 px UI fonts, 1280 / 800 / 640 px,
maximised, a long git branch, the app menu open and walked by keyboard, hover on a toggle
and on close, an Agent click, the status bar's dock button collapsing and reopening the dock)
differ in **0 pixels in the title bar and tab bar rows**, and in the status bar only in the
clock's seconds and the CPU/MEM value. Window controls answer `WM_NCHITTEST` exactly as main.

What fails is **accessibility**: gpui does not replay a reused view's AccessKit nodes, so
the title bar's app menu (`MenuBar`, `Button "OneTerm"`) and the mode toggles (`ToolBar` and
its three `Button`s) disappear from the UI Automation tree from the first reused frame on,
which is nearly every frame (F1). On main they stay. `docs/gui-layout.md` makes every
OneTerm-owned focus target expose a role and ID; a cached view silently takes them away
again. Fix: embed the two views uncached while `window.is_a11y_active()` (gpui refreshes the
window when AccessKit activates), plus a line in `docs/gui-layout.md` and an upstream note.

## Evidence

### 1. Invalidation completeness

gpui-pre 0.3.7 `view.rs` 484-489: a cached view is reused only when its bounds, content mask
and inherited text style are unchanged, it is not in `dirty_views`, and `window.refreshing`
is false. `mark_view_dirty` (`window.rs` 2148) dirties a notified view and every ancestor
view on its dispatch path; hover and group-hover changes notify the painting view
(`div.rs` 2860 / 2882 / 3312); `Window::refresh` (2272) and `App::refresh_windows`
(`app.rs` 1156 -> `apply_refresh_effect` 1918) set `refreshing`; `Window::focus` / `blur`
refresh (2311 / 2326); `bounds_changed` (2683: resize, maximise, DPI) refreshes. A notify of a
plain model the view only *reads* (not a view in its subtree) marks nothing the view owns,
hence the `UiConfig` observer.

| Input | What carries it to the cached view | Evidence | Result |
| --- | --- | --- | --- |
| Theme light/dark at runtime | `SwitchThemeMode` -> `Theme::change` + `cx.refresh_windows()` (`crates/theme/src/theme.rs` 205-211; kit `Theme::edit` also refreshes, `theme/mod.rs` 314) | GUI: OneTerm menu > Appearance > Light by keyboard, both builds; both bars light, 0 px in the title rows, status bar only clock/CPU/MEM | OK |
| Colour theme pick / theme file reload | `SwitchTheme` and the Settings picker call `apply_config` + `refresh_windows` (`theme.rs` 190-203, `settings-ui/src/appearance.rs` 157-166); kit hot reload `registry.rs` 70 | Code; same refresh path as the GUI-tested mode switch | OK |
| UI font family / size | `Theme::global_mut(cx).font_size = ..; cx.refresh_windows()` (`settings-ui/src/general.rs` 43-44); Root re-sets `rem_size` each frame; the status bar's height is recomputed from `window.rem_size()` by the uncached parent, so its bounds change too | GUI: pairs at 12 / 20 / 32 px (startup), 0 px in title rows; runtime change not driven (dialog), code-proven | OK |
| Window resize / maximise | `bounds_changed` -> `refresh`; bounds change anyway | GUI: 1280 -> 800 -> 640 px and `SC_MAXIMIZE` / `SC_RESTORE`, toggles stay right-aligned, 0 px in title rows | OK |
| DPI change | `WM_DPICHANGED` -> size change -> `bounds_changed` -> `refresh` (gpui-pre-windows `events.rs` 878-950) | Code only (no per-monitor override for a private instance) | OK (gap) |
| Locale / text | No locale-dependent text in either bar (fixed clock format, static labels); labels come from the indicators | Code | n/a |
| Mode SSH Client / Agent / None | Only two writers of `right_dock_mode`, both `UiConfig::update` + notify (`actions.rs` 161, `workspace/mod.rs` 363); `UiConfigGlobal` set once (`ui_config.rs` 223); content observes it | GUI: Agent click, and the status bar's dock button collapsing (toggles -> None, BUG-0067 path) and reopening (-> Agent); identical to main at each step (`US-0150-verify-title-states.png`). Test + mutation M2 | OK |
| Workspace title / tabs in the title bar | None: the title bar shows the fixed app title; tab titles live in the dock | Code | n/a |
| Elevation suffix (IN-0043 M5) | Process-wide atomic set once at startup (`core/src/config/elevation.rs` 399-427), read on the content's first render | Code only (no UAC run) | OK (gap) |
| App menu content | `AppMenuBar::reload` / `set_selected_index` notify the menu bar, a child view of the content | GUI: menu opened, stays drawn over 3 s of idle frames, Down / Right walk highlights and opens the submenu, Escape closes; identical to main (`US-0150-verify-menu.png`) | OK |
| Popups anchored in the title bar | `deferred(anchored(..))` in `AppMenu::render`; `reuse_prepaint` replays deferred draws (`window.rs` 3845-3862); the popup's notify dirties its dispatch path through the content | GUI as above, incl. the anchored submenu | OK |
| Hover on toggles / close | Toggle hover notifies the content (`div.rs` 2860); close is the kit's uncached `TitleBar` | GUI: `None` highlighted after a posted move and cleared 2 s after moving away; close red via `WM_MOUSEMOVE` and via `WM_NCMOUSEMOVE HTCLOSE`; identical to main | OK |
| Focus rings | `focus` / `blur` refresh the window; the toggles take no focus | Code | OK |
| Clock / net / breadcrumb / git / CPU-MEM | Each is a `StatusText` child view; `tick` notifies on change (`status_text.rs` 378-384), dirtying the bar; budgets are set in the bar's render from `viewport_size` (resize refreshes) | GUI: clock and CPU/MEM advance in every capture; long branch elided identically at 1280 / 800 / 640. Test + hotpath 0.52 bar renders per idle focused frame | OK |
| Notifications | Kit notifications are a Root overlay, not in either bar | Code | n/a |
| Tooltips (US-0148 live table, dock button) | `reuse_prepaint` replays `tooltip_requests`; `DetailsTooltip` observes the item | GUI: CPU/MEM table hovered 8 s, uptime 6.0 -> 7.0 -> 10.0 -> 12.0 s with the label; "Toggle Right Dock" shows and hides | OK |
| Accessibility tree | **Not replayed** for a reused view (§ F1) | UI Automation, below | **FAIL** |

UI Automation, focused idle, 4 s after the first query (own window, `uiatree.ps1`):

```
before                                   after
MenuBar                                  (missing)
  Button "OneTerm"                       (missing)
ToolBar                                  (missing)
  Button x3 (SSH Client / Agent / None)  (missing)
Pane "OneTerm workspace" ...             Pane "OneTerm workspace" ...   (same subtree)
Button (status bar dock toggle)          Button (status bar dock toggle)
```

Repeated: 4 samples each, focused and unfocused, `after` 10-11 nodes against 16-17 for
`before`; the six title-bar nodes appear only on the one full frame after AccessKit activates.
The status bar's `Button` is present in these samples (the bar re-renders on each clock tick)
and absent by the same mechanism on frames where it is reused.

### 2. Window controls (own pids, posted messages)

`WM_NCHITTEST` after 4 s focused idle, after hovering the toggles, with the app menu open, and
maximised, 1.2 s after each move so the reply comes from reused frames:

| Point | before | after |
| --- | --- | --- |
| empty caption x=500 / x=900, gap left of the toggles, icon | caption | caption |
| min / max / close | min / max / close | min / max / close |
| terminal, status bar | client | client |
| app menu, toggles | caption (one early `before` pass: agent / none = client) | caption |

The menu and toggles answer `HTCAPTION` in **both** builds: pre-existing kit behaviour (their
mouse-down handlers stop the move), not this story. `WM_NCLBUTTONDBLCLK` on the caption
restores and re-maximises in both; `SC_MAXIMIZE` / `SC_RESTORE` in both. Close hover via
`WM_NCMOUSEMOVE HTCLOSE` paints the danger colour in both.

**One real-mouse step**: the `after` window was made topmost without activation and the real
cursor put on its maximise button for 1.9 s, then restored. The Windows 11 snap-layout flyout
was **not** captured, and the maximise button showed no hover in that capture either, so the
cursor most likely did not register (inconclusive, not repeated to stay at one real-mouse
step). The hit test answers `HTMAXBUTTON` there, which is what the flyout keys on; the
implementer's capture (`raw/us0150/pixels/window-controls-before-after.png`) shows it.

### 3. Height and size

- `status_bar_height(rem) = 1.75 rem + 1 px` equals the natural kit layout at rem 12 / 14 / 16
  / 20 (the test), and GUI pairs at 12 / 16 / 20 / 32 px fonts line up to the pixel (the bar's
  rows differ only in the live values).
- Title bar content at 800 and at the 640 px minimum (`crates/app/src/window.rs` 38), at
  16 / 20 / 32 px: 0 differing pixels in rows 0-36. At 32 px / 640 px the toggles overflow into
  the window controls **identically in both builds** (pre-existing, not this story): the cached
  leaf has no min-content, but the kit `bar` never got to use its min-content there either.
- Long branch `feature/a-very-long-branch-name-for-the-status-bar-us0150-verification-run`
  (git repo in the private home): elided the same at 1280 / 800 / 640.

### 4. Measurements (one interleaved pair per load, release + hotpath, own pids)

| Load | before | after | Change |
| --- | --- | --- | --- |
| Idle focused, Mcycles/s (frames/s) | 30.13 (1.99) | 20.42 (2.14) | -32 %; per frame 15.1 -> 9.6 Mc |
| Idle unfocused, Mcycles/s (frames/s) | 17.92 (1.23) | 21.87 (1.19) | **+22 %** (wrong direction; noise) |
| TUI two tabs, Mcycles/s (frames/s) | 315.9 (31.9) | 238.2 (31.3) | per frame 9.90 -> 7.61 Mc, -23 % |
| `TitleBarContent::render` per frame | - | 0.05 idle F / 0.09 idle U / 0.01 TUI | |
| `build_status_bar` per frame | 1.00 | 0.52 idle F / 0.94 idle U / 0.04 TUI | |
| `AppTitleBar` + `TitleBarContent` render, us/frame | 17.6 / 18.4 / 13.9 | 2.0 / 3.4 / 1.4 | |
| `build_status_bar` + `StatusText::render`, us/frame | 188 / 229 / 66 | 86 / 173 / 4.7 | |

Direction confirmed for idle focused and TUI; the magnitudes (-32 %, -23 %) are larger than
the render sites can explain (~0.1-0.2 ms of a 5-10 ms frame), i.e. inside the ±20 % run-to-run
noise the packet reports. Idle unfocused went the other way in this pair; unfocused frames are
driven by the clock, so the bar re-renders on 94 % of them and only the title bar's ~16 us
can be saved: the packet's "-15 % unfocused" is not a real effect of this change (F4).
Render counts match the packet (content 0.04-0.07 -> here 0.05; bar 0.51-0.61 -> 0.52 idle,
0.05-0.09 -> 0.04 TUI).

**Status bar keep or revert (`e4133b7e`)**: **keep.** Its own render work drops 54 % idle
focused and 93 % under TUI (plus the layout and paint the hotpath sites do not count), the
invalidation model is the simplest possible (every input is a child view's notify or a
refresh), 42 pixel pairs show no change, and it reverts cleanly if a future kit change makes
the fixed height awkward. Conditions: the F1 a11y switch covers it too, and the workspace's
embed gets a test (F2) so that the height and the `.cached` stay pinned where they are used.

### 5. Tests and mutations

`cargo test -p oneterm-workspace -p oneterm-app -p oneterm-terminal-view`: 26 + 398 (3
ignored) + 45 (3 ignored) passed, as the packet says. Mutations (each applied to the source,
`cargo test -p oneterm-workspace --lib -- caching`, file restored from git):

| Mutation | Result |
| --- | --- |
| M1 title content embedded uncached (`.into_any_element()`) | killed (title test) |
| M2 `UiConfig` observer does not notify | killed (title test) |
| M3a `status_bar_height` without the 1 px border | killed (height test) |
| M3b `status_bar_height` + 2 px | killed (height test) |
| M4 **workspace** embeds the status bar uncached (every unrelated notify re-renders it) | **survives** |
| M5 **workspace** embeds the status bar at a fixed 28 px | **survives** |
| M6 title content cached at a fixed 100 px width | **survives** |

The status-bar tests build their own `Host` that repeats the `.cached(..)` embed, so the
embed in `OneTermWorkspace::render` (`workspace/mod.rs` 587-596) is not under test (F2).

### 6. Records

- Packet follows `docs/templates/work.md`: dated 2026-09-29, owning docs reviewed, evidence,
  gaps, Handoff, upstream notes with gpui-pre file/line. Missing: the accessibility effect
  (F1) and the unfocused claim (F4).
- `docs/gui-layout.md` § Frames and re-rendering is accurate for drawing and input; it needs
  one rule for F1 ("a cached view's AccessKit nodes are not replayed").
- `IN-0046.md` and `high-level-design.md` lines match the code.

### 7. Gate

`CARGO_BUILD_JOBS=3 pwsh scripts/ci-local.ps1` on `fee8947e` with this file in the tree (no
`target/release`, no incremental dir). Runs 1 and 2 stopped in `cargo test --workspace` with
rustc out of memory (machine commit charge at 29 of 33 GB from parallel builds; environmental);
run 3: 141 `test result: ok` lines, none failed, `check-doc-paths.py` 212 paths,
`check-english.py` passed; final line below.

## Findings

- **F1 (High, blocks) — the title bar's menu and toggles leave the accessibility tree.**
  gpui-pre 0.3.7 builds AccessKit nodes during prepaint (`window/a11y.rs` module docs,
  `begin_frame` clears them every frame) and `reuse_prepaint` (`window.rs` 3814) replays
  hitboxes, tooltips, dispatch nodes and deferred draws but no a11y nodes, and `prepaint_view`
  (`view.rs` 484-489) does not consider `a11y.is_active()`. Evidence: § 1, UI Automation.
  Fix: in both embeds, `if window.is_a11y_active() { view.into_any_element() } else {
  view.cached(style).into_any_element() }` (AccessKit activation already refreshes the window,
  `window.rs` 1649-1660); a rule in `docs/gui-layout.md`; upstream request "replay a11y nodes
  in `reuse_prepaint`". Pre-existing and out of scope: the kit's cached right-dock panel
  (`Pane "SSH client"`) drops out the same way on main.
- **F2 (Medium) — the status bar's production embed is untested.** M4 and M5 survive: the
  `.cached` and the height in `OneTermWorkspace::render` can be removed or changed with every
  test green, and the packet's "height pinned by test" holds only for `status_bar_height`.
  Fix: one helper (e.g. `statusbar::embed(&Entity<StatusBarView>, &Window) -> AnyElement`)
  used by the workspace and by the test host. M6 (title content style) is the same gap, lower
  risk (the style is one line beside the kit row); the F1 helper can cover both.
- **F3 (Low) — snap-layout flyout not re-proven here.** One real hover attempt did not
  register; hit test is `HTMAXBUTTON` as on main. Accept the implementer's capture or repeat by
  hand.
- **F4 (Low, records) — the unfocused and magnitude claims overstate.** Unfocused frames
  re-render the bar on 94 % of frames; this pair measured +22 %, the packet -15 %: both are
  noise. The packet and research § 9.2 should say "idle unfocused: no expected change (the
  clock drives those frames); measured -15 % / +22 % in independent pairs".
- **F5 (Info) — menu and toggles answer `HTCAPTION`** in both builds; pre-existing kit
  behaviour, unchanged.

## Gaps

- DPI change and runtime UI-font change not driven in the GUI (no per-monitor DPI override for
  a private instance; the font setting needs the Settings dialog): both go through
  `refresh`, code-proven.
- Elevated instance not launched (UAC); the suffix is a startup constant.
- One pair per load; the cycle counter moved ±20 % between runs of the same build.
- The status bar's own a11y node was not caught missing in a sample (it re-renders every
  clock tick), only inferred from the mechanism.
- Windows only; macOS / Linux wait for CI.

## Gate line

```
ci-local: all checks passed.
```


---

# Pass 2 (2026-09-29): rework `39563a98`

- Commit under test: `39563a98` (`fix(workspace): cached views fall back to uncached while
  accessibility is active`), on top of pass 1 (`a027a7d6`). Builds as in pass 1: `before`
  (main's `crates/workspace`), `after` (`fee8947e`, pass 1), `rework` (`39563a98`), release
  with thin LTO, line tables, `oneterm-app/hotpath-profiling`; this worktree's own `target/`,
  `CARGO_BUILD_JOBS=3`, the only builder. Every instance launched here with a private
  `USERPROFILE`, driven through its own pid only. No real input except the two activation
  probes in § P4 (a real cursor move and a real Shift tap, both only onto this run's own
  window).

## Verdict: PASS (F6 must be fixed in the records before merge)

F1 is fixed: with AccessKit active the title bar keeps all six nodes on every idle sample,
pixel-identical to main; with it inactive the cached path is taken and the helper costs
nothing. F2 is fixed (production embeds under test, mutations killed). What remains is the
records' account of how often AccessKit is active (F6): on this desktop it was switched on by
an unidentified client in 8 of 11 launches during one hour and in 0 of 33 afterwards, so
the packet's "seen once, rate not measured" understates it.

## Evidence (pass 2)

### P1. Same-script UIA contrast (own windows, AccessKit activated by this reader)

One reader (`uia2.ps1`: `FromHandle` + `FindAll(Descendants)` once, then a raw-view walk
every 3 s, 6 samples, no input in between), both builds, focused and unfocused:

| Build | Focused: nodes, title-bar nodes per sample | Unfocused |
| --- | --- | --- |
| `fee8947e` | 11, **0/6** in 6 of 6 samples | 10, **0/6** in 6 of 6 |
| `39563a98` | 17, **6/6** in 6 of 6 | 16, **6/6** in 6 of 6 |

The status bar's dock `Button` is the last node in every sample of both. `39563a98`'s 17 /
16 nodes are main's count from pass 1. Caveat found on the way: a posted `WM_ACTIVATE`
before each sample makes gpui draw a full frame, and then `fee8947e` also reads 6/6: a
reader must not poke the window between samples (the implementer's `uia.ps1` does not).

### P2. AccessKit active: pixels and window controls against main

Side-by-side pairs, `before` vs `rework`, both activated by a UIA query first (both
`stderr.log`s show `Accessibility activated`):

- Dark, 1280 px: idle, hover on `None` and away, hover close, app menu open / Down x2 /
  Escape, Agent click, status-bar dock button collapse and reopen, maximise, restore, 800 px.
- Zed One Light at a 12 px UI font: runtime switch to light, 800 px.
- Dark at 20 px: menu, 800 px, 640 px. Dark at 32 px: 1280, 800, 640 px.

**0 differing pixels in the title-bar and tab-bar rows in all 27 captures**; in the status
bar only the clock's seconds and the CPU/MEM value (checked by eye at 32 px). `WM_NCHITTEST`
with AccessKit active (idle focused, after toggle hover, menu open, maximised): identical to
main at all 13 points; caption double-click restores and re-maximises in both.

### P3. AccessKit not active: the cached path, and the helper's cost

Interleaved idle runs, 90 s each, every run's `stderr.log` checked: AccessKit **not active in
any of the eight**. The desktop was quiet in this window, so these are the steadiest pairs
of the day:

| Run | Mcycles/s | frames/s | Mcycles/frame | Title content / frame | Status bar / frame | `AppTitleBar` + content, us/frame |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| before idle F | 27.77 | 2.11 | 13.18 | - | 1.00 | 16.1 |
| rework idle F | 26.58 | 2.12 | 12.56 | 0.04 | 0.51 | 1.7 |
| fee8947e idle F | 26.86 | 2.13 | 12.63 | 0.04 | 0.52 | 2.0 |
| fee8947e idle F #2 | 26.83 | 2.12 | 12.68 | 0.04 | 0.51 | 1.9 |
| rework idle F #2 | 26.32 | 2.10 | 12.50 | 0.04 | 0.51 | 1.9 |
| before idle F #2 | 29.62 | 2.08 | 14.22 | - | 1.00 | 18.2 |
| rework idle U | 18.90 | 1.12 | 16.94 | 0.07 | 0.96 | 2.4 |
| before idle U | 20.80 | 1.14 | 18.30 | - | 1.00 | 14.0 |

- The cached path is taken (0.04 / 0.51 per frame, as `fee8947e`), and `rework` equals
  `fee8947e` within 1 % per frame: `cached_unless_a11y` (one flag read) adds nothing.
- Idle focused per frame: 13.18 / 14.22 -> 12.56 / 12.50 (-5 % / -12 %); idle unfocused
  -7 % (one pair). The implementer's AccessKit-active run (`raw/us0150/runs/us0150-rework-idleF`)
  renders both views on every frame (1.00 / 1.00): the fallback works as designed there.

### P4. The spontaneous activation (activator, and the rate on this desktop)

AccessKit on Windows (`accesskit_windows` 0.34 `adapter.rs` 535-560) activates on the first
`WM_GETOBJECT` for `UiaRootObjectId` **or `OBJID_CLIENT`**, i.e. any UI Automation or MSAA
client asking for the window, and it has no deactivation path, so the window stays active
for its life. Every launch this verification made without a UIA query of its own, with
gpui's `Accessibility activated` line looked up in the instance's own `stderr.log`:

| When (UTC) | Launches | Activated | Delay after start |
| --- | ---: | ---: | --- |
| 09:41-10:05 (pass 1: tooltip, pixel-pair, hit-test and measurement runs whose logs survive) | 11 | **8** | +1, +2, +6, +6, +47, +65, +66, +79 s |
| 10:38 (implementer, rework runs) | 3 | 1 | +1 s |
| 10:54-11:22 (this pass) | 33 (22 probe launches, 3 windows idle 300 s, 8 measurement runs of 90 s) | **0** | - |

Probes in the quiet period, each on its own window: plain launch holding the real
foreground for 16-20 s (9 launches), the real cursor moved over the window (3), a real Shift
tap into it while foreground (3), posted `WM_ACTIVATE` every 5 s with no foreground (4), the
measurement script itself (3), three windows side by side for 300 s: **none activated**. So
the activator is not launch, focus, foreground, mouse, keyboard or the measurement
tooling; it is an external UIA/MSAA client active in the session for about an hour and
then gone. Not identified: `WM_GETOBJECT` carries no sender, and a hook inside the process
would need a native hook DLL or a gpui patch. Processes in the session holding
`UIAutomationCore.dll` or `oleacc.dll` (candidates, not proven): explorer, sihost,
SearchHost, ApplicationFrameHost, ms-teams, msedge / webview2, olk, OneDrive, PowerToys,
Zed, notepad++, Lens, OpenConsole; screen readers and Magnifier were not running.

Consequence, stated plainly: the gain is real only for a window no UIA or MSAA client has
asked for; once one has (a screen reader, an IME or tool that uses MSAA, a UI tester, as
here for an hour), both views render on every frame for the rest of the window's life, as on
main. On this desktop that was 8 of 11 launches in one hour and 0 of 33 in the next.

Also a measurement confounder for all of IN-0046: an active AccessKit makes every frame
build its node tree, so an idle pair where one side activates mid-run is skewed. Pass 1's
pairs were (`before` idle F activated at +1 s, `after` at +66 s), which is part of pass 1's
-32 %; the measurement script should report the activation.

### P5. Tests and mutations

`cargo test -p oneterm-workspace -p oneterm-app -p oneterm-terminal-view`: 26 + 398 (3
ignored) + 46 (3 ignored) passed. Mutations (applied, `cargo test -p oneterm-workspace --lib
-- caching`, restored from the original bytes):

| Mutation | Result |
| --- | --- |
| R1 `cached_unless_a11y` always uncached | killed (2 reuse tests) |
| R2 the a11y switch inverted | killed (2 reuse tests) |
| R3 status bar box at a fixed 28 px | killed (height test) |
| R4 title content box at a fixed 100 px | killed (`the_content_fills_the_kit_row_cached_or_not`) |
| R5 uncached box without its style | killed (same) |
| R6 `UiConfig` observer does not notify | killed (title reuse test) |
| R7 height without the 1 px border | killed (height test) |
| R8 status bar box without `w_full` | survives: equivalent (the workspace column stretches its children; so does the test host) |

Not testable, accepted: "the switch removed, always cached" (the test platform's
`a11y_init` is a no-op, gpui-pre 0.3.7 `platform.rs` 1070); P1 covers it.

### P6. Records

- Packet: template followed, dated, Reopened ticked, rework section with F1-F4, upstream
  request 3 with correct lines (`window/a11y.rs` 275 / 429, `window.rs` 1004 / 1649-1661 /
  3814 / 6794, `view.rs` 484-489, kit `dock/tab_panel.rs` 782, all checked), the
  pre-existing kit right-dock note, Handoff. `docs/gui-layout.md` rule accurate.
- Needs (F6): the first gap to carry P4's rate and the plain consequence, and the note
  that AccessKit also activates on MSAA `OBJID_CLIENT`, not only UIA.
- Nit: research § 9.3's window-controls bullet has one unwrapped line (> 100 columns).

## Findings (pass 2)

- F1 (pass 1): **fixed** (P1, P2).
- F2 (pass 1): **fixed** (P5).
- F3, F4 (pass 1): recorded in the packet as asked.
- **F6 (Medium, records, fix before merge)**: the packet's gap says AccessKit activated
  "once here" and the rate "is not measured"; measured here: 8 of 11 launches in one hour,
  0 of 33 in the next, activator external and unidentified, and permanent per window. The
  packet, research § 9.3 and `IN-0046.md` should say that the gain holds only for windows no
  UIA/MSAA client has touched, with these numbers.
- **F7 (Low, method)**: idle measurements should log whether AccessKit activated
  (`hotpath-measure.ps1` can grep the instance's `stderr.log`); pass 1's -32 % pair was
  confounded by it.

## Gaps (pass 2)

- The activator is not identified (no sender on `WM_GETOBJECT` without an in-process hook);
  the rate is from one desktop on one afternoon.
- The AccessKit-active path was driven by this verification's own UIA reader, not by a
  screen reader.

## Gate line (pass 2)

`CARGO_BUILD_JOBS=3 pwsh scripts/ci-local.ps1` on `39563a98` with this section in the tree (no
`target/release`, no incremental dir): 141 `test result: ok` lines, none failed,
`check-doc-paths.py` 212 paths, `check-english.py` 1076 files.

```
ci-local: all checks passed.
```
