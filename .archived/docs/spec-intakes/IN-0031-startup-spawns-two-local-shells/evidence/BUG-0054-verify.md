# Independent verification: BUG-0054

Packet: `BUG-0054` — Startup spawns exactly one local shell
Intake: `IN-0031`
Verified range: `git diff 2f47366..f6a7065` (6 files), worktree reset to `f6a7065`
Worktree: `D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-a9123be2c274e0d70`
Date: 2026-09-14
Verifier: independent agent (not the implementer)

## Verdict

**PASS-WITH-NOTES.**

The fix is correct, minimal, and does what the packet claims. The counting test really fails
2 / 3 before the fix, the empty-stack centre is a first-class `DockArea` state rather than a
tolerated one, the side docks and `sftp_table_state` survive the load, and three real launches
give one shell and no dialog. Four notes below; none is a behavioural defect, and none blocks
acceptance. The one that matters is N1: an Acceptance box cites a test that does not assert what
the box claims.

The reference source used for `DockArea::load` / `PanelState` / `PanelInfo` is the **cargo
registry copy** that the crate actually resolves to —
`C:\Users\trunglt\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\gpui-base-0.6.0\src\dock\` —
because `reference/gpui-kit/` does not exist inside this worktree (it is not tracked here).

## 1. The fix

### (a) An empty-children stack centre is a valid state on every path `DockArea::load` takes

Confirmed by reading, not inferred:

- `dock_area.rs:167` — `DockArea::new` initialises `center: PaneTree::new(RootKind::Split)`, i.e.
  an empty split root. The state the fix hands to `load` is the state every `DockArea` already
  starts in.
- `state_convert.rs:166` `build_node`, `PanelInfo::Stack` arm — `sizes` is rebuilt from
  `(0..children.len())`, so `children.len() == sizes.len()` holds by construction. The
  `PanelInfo::stack(Vec::new(), …)` the fix passes is discarded; there is no
  `sizes.len() != children.len()` path to hit.
- `state_convert.rs:137` `from_state` with `RootKind::Split` — the built node *is* a `Split`, so
  the single-child wrapping arm is skipped and the root is used as is.
- `normalize.rs:96-110` `is_normalized` — the invariant is written with an explicit carve-out:
  *"The root may legitimately be an empty or single-child split."* `collapse_root`
  (`normalize.rs:255`) returns immediately for `RootKind::Split`, so the empty root is never
  collapsed away. No assertion fires.
- `dock_area.rs:992` `reconcile` — an empty split produces one `ContainerPlan::Split` with
  `children: []`; no `TabGroup` is created, so no `ActiveTracker` runs and nothing is focused.
  `views_of`'s `debug_assert!` is never reached because there are no panels.
- `dock_area.rs:1630` `render` → `render_node(self.center.root(), …)` on a split whose entity
  `reconcile` has just created. Exercised live: my new test calls `cx.refresh()` +
  `run_until_parked()` on the loaded, empty centre and does not panic.

### (b) The version-mismatch prompt path

Unchanged. `persistence.rs:56` reads `state.version`, which the replacement at `persistence.rs:50`
does not touch. On "Yes" the spawned task runs `reset_default_layout`, which builds its own centre
and right dock; on "No" the caller's `reset_center_only` has already replaced the centre anyway.
Verified by reading only — **there is no test for this path**, before or after the fix (pre-existing
gap, see N4).

### (c) `zoomed_panel` restore when the zoomed panel was in the centre

Behaviour is correct, and was **not covered by any test** before this verification (see N1).

`restore_zoom_in_dock` (`mod.rs:282`) searches the *live* tree by panel name, and
`OneTermWorkspace::new` calls it at `mod.rs:214` — after `reset_center_only`. So it has always
searched the post-reset centre; the loaded centre's panels were pruned before it ran either way.
The fix changes nothing here. I wrote the missing test (section 3): it asserts the restore finds
nothing against the loaded empty centre (and does not panic), and finds the reset's terminal
afterwards.

### (d) Side docks and `sftp_table_state` untouched

- `load_reset_center_and_save_round_trip` passes unchanged (right dock `(333., false)` after
  load, `ssh_client_panel` after reset, `sftp_table_state` `name: 321.0` preserved through the
  save).
- `pre_migration_fixture_loads_and_saves_without_semantic_drift` passes unchanged and untouched.
- Independently confirmed **in the real binary**: after the three E2E launches the fixture's right
  dock (`ssh_client_panel`, `size 443.0`, `open true`) and its `sftp_table_state`
  (`name 321.0`, `size 128.0`, `permissions true`, `owner false`) are all still in
  `target/docks.json`, with only the centre replaced by one `terminal`.

## 2. Tamper

Reverted **only** the four `state.center = …` lines in `persistence.rs` (imports neutralised with
`#[allow]` so nothing else moved), rebuilt, ran, then restored the file byte-for-byte from a
pre-tamper copy and confirmed `git status` shows `persistence.rs` unmodified.

```
test layout::workspace::layout_tests::load_layout_builds_no_center_panel_and_the_reset_builds_one ... FAILED
assertion `left == right` failed: loading the saved layout must build none of the center's terminal panels
  left: 2
 right: 0
```

Then, with the first assertion temporarily relaxed to `2`:

```
assertion `left == right` failed: the center reset must build exactly one terminal panel
  left: 3
 right: 1
```

**2 / 3 confirmed, exactly as the packet records.** Both numbers are real failures, not restated
from the packet.

### Test-order independence

The counting builder cannot leak: `register_panel` writes into the per-`App` `PanelRegistry`, and
every `#[gpui::test]` gets a fresh `TestAppContext`; `dock_area()` also re-registers the test
panels per test. Confirmed empirically — the suite passes both in parallel (default) and
single-threaded, where alphabetical order puts `load_layout_builds_no_center_panel…` and
`load_layout_drops_a_split_centre…` *before* `load_reset_center_and_save_round_trip`,
`pre_migration_fixture…` and `switch_right_dock_mode…`:

```
cargo test -p oneterm-workspace -- --test-threads=1
running 13 tests
... all 13 ok
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

(`libtest` has no stable `--shuffle`, so the two orders above are the substitute the task allows.)

## 3. Test written by this verification

`crates/workspace/src/layout/workspace/layout_tests.rs::load_layout_drops_a_split_centre_and_still_restores_zoom_after_the_reset`
(**uncommitted, left in the worktree**).

Fixture: a centre that is an `h_split` of **two** tab groups (first holds `terminal` + `sftp`,
second holds `terminal`), a right dock of `agent_panel` at 464 px, and `zoomed_panel = "terminal"`.
Asserts, in order:

1. the saved state really is a split of two children (guards the fixture itself);
2. `load_layout` builds **0** terminal panels;
3. the loaded centre holds **0** panels;
4. `cx.refresh()` + `run_until_parked()` — the empty centre renders without panicking;
5. the right dock is still `(464., true, agent_panel)` — the only assertion anywhere that
   `load_layout` preserves the right dock's *panel name*;
6. `restore_zoom_in_dock("terminal")` returns `false` against the empty centre and leaves the dock
   un-zoomed (no panic on a missing tab);
7. `apply_center_reset` builds exactly **1** terminal;
8. `restore_zoom_in_dock("terminal")` now returns `true`.

Result with the fix: **passes**. Result under the same tamper as section 2:

```
test layout::workspace::layout_tests::load_layout_drops_a_split_centre_and_still_restores_zoom_after_the_reset ... FAILED
assertion `left == right` failed: a split centre's terminals must not be built by the load
  left: 2
 right: 0
```

## 4. Other callers

`load_layout` has exactly **one** production call site —
`crates/workspace/src/layout/workspace/mod.rs:131` — plus two test call sites
(`layout_tests.rs:284`, `:354`, and now mine). The packet's claim holds.

`DockArea::load` is called directly from one place in the repo, `layout_tests.rs:184`
(`pre_migration_fixture_loads_and_saves_without_semantic_drift`), which deliberately bypasses
`load_layout` and is unaffected.

Everything that reads the centre (`crates/state/src/dock_util.rs`,
`crates/workspace/src/layout/workspace/actions.rs:58`, `crates/terminal-view/src/panel/spaces.rs`,
`crates/session-ui/src/common.rs:242`) runs from user interaction, long after
`OneTermWorkspace::new` has reset the centre synchronously. Nothing observes the window between
the load and the reset.

## 5. Cross-platform

`crates/workspace/src/` contains **no** `cfg(windows)`, `cfg(unix)` or `cfg(target_os = …)` at all,
and the fix adds none. Nothing macOS- or Linux-specific reads the centre after the load. The change
is platform-neutral and correctly guarded by nothing. The packet's gap entry ("Non-Windows
platforms take the same code path and waste the same shell, but only Windows was measured") is the
honest statement of what was proved.

## 6. Documentation

- `docs/gui-layout.md` §Persistence — the added sentence matches the code. See N2 for one inexact
  clause.
- `load_layout` doc comment (`persistence.rs:32-40`) — accurate: names the single caller, the
  reason, `BUG-0054`, and the `ClosePseudoConsole` consequence.
- `IN-0031.md` — `BUG-0054` ticked, `BUG-0055` left open. Correct.
- Packet status `Planned/In progress/Implemented` and all five PROOF boxes ticked; every Acceptance
  box carries evidence in the packet except as noted in N1.
- The HEAD-baseline table is **internally consistent**: 10 rows, all with 2 distinct `cmd.exe` /
  2 `OpenConsole.exe` (→ "10/10 two shells"); the dialog column is filled in rows 2-10 (→ "9/10
  dialogs"); the survivor column is `2` in rows 1-9 and `0` in row 10 (→ the Acceptance line's
  "2 orphans in 9 runs"). The prose in Acceptance, Evidence and the `f6a7065` commit message all
  quote the same numbers.
- The pre-fix dialog PNG exists at
  `evidence/BUG-0054-prefix-application-error-dialog.png` (18 414 bytes, new in `f6a7065`).

## 7. `pwsh scripts/ci-local.ps1`

Run from this worktree with `CARGO_BUILD_JOBS=4`. Exit code **0**; final line
`ci-local: all checks passed.` All ten steps ran, including
`verify-dependency-graph` (21 packages), `check-doc-paths` (120 paths / 10 documents),
`check-english` (769 files), `completion-catalog validate`, and
`third-party-notices --check`.

```
sections: 60  passed: 1933  failed: 0  ignored: 13
failed-sections: 0
```

The implementer reported **60 / 1932 / 0 / 13**. The single extra pass is my added test
(section 3), which is present in the tree for this run. Totals otherwise reproduce exactly.

## 8. E2E spot-check (run)

Built `cargo build -p oneterm-app --profile fast-dev` in this worktree only (D: had 63.3 GB free
before, 45.4 GB after — above the 25 GB floor). `fast-dev` inherits `dev`, so `debug_assertions`
is on and `config_dir()` resolves to `target/`.

Fixture copied to `target/docks.json` before every launch:
`crates/state/src/fixtures/docks-0.5.2.json` — `version` 3, centre `StackPanel` → `TabPanel` →
**two** `terminal` leaves, right dock `ssh_client_panel` 443 px open, `zoomed_panel: "terminal"`.

Safety, as required: `Get-Process oneterm` was enumerated **before** launching and recorded
(`pid 14804`, the owner's own OneTerm running Claude Code) and never touched. Each launch used
`Start-Process -PassThru`; only that pid's descendant subtree (BFS over `Win32_Process`
`ParentProcessId`) was sampled, at 100 ms for 4 s; shutdown was `CloseMainWindow` then
`Stop-Process -Id <my pid>`; survivors were stopped **by pid**, each one first observed as a
descendant of my own launch. No process was ever matched by name or window title. `#32770`
windows were enumerated for observation only and attributed with `GetWindowThreadProcessId`.

| run | app pid | distinct `cmd.exe` | max live `cmd.exe` | `OpenConsole.exe` | `#32770` | survivors after exit |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 4560 | 1 | 1 | 1 | none | 0 |
| 2 | 9444 | 1 | 1 | 1 | none | 0 |
| 3 | 14124 | 1 | 1 | 1 | none | 0 |

After the runs: owner `oneterm` pid 14804 still alive; my three pids gone; no survivor to clean up.
`target/docks.json` was rewritten with the reset centre and the fixture's right dock and
`sftp_table_state` intact — independent confirmation of section 1(d) in the real binary.

I did **not** re-run the ten-launch HEAD baseline; the implementer's baseline table is accepted on
the strength of the tamper reproduction in section 2, which pins the same cause deterministically
at the unit level.

## 9. Trailers

Both commits carry the required pair.

| commit | `Co-Authored-By` | `Claude-Session` |
| --- | --- | --- |
| `5b42247` | `Claude Fable 5.1 <noreply@anthropic.com>` | `…/session_01NvasBgYuPESbdNxVEiMgas` |
| `f6a7065` | `Claude Fable 5.1 <noreply@anthropic.com>` | `…/session_01Q6xr5jX29B2b6L4MGsoNdW` |

Both sessions are in the accepted set. Both messages carry `Refs: BUG-0054, IN-0031`.

## Notes (numbered, with severity)

### N1 — Low. An Acceptance box cites a test that does not assert half of what it claims

`docs/spec-intakes/IN-0031-startup-spawns-two-local-shells/BUG-0054-startup-spawns-exactly-one-local-shell.md:57-60`

> - [x] The right dock keeps its saved panel name, width and open state across load → reset → save,
>   and `zoomed_panel` still restores by name.
>   `load_reset_center_and_save_round_trip` passes unchanged.

`load_reset_center_and_save_round_trip` (`layout_tests.rs:241-333`) never calls
`restore_zoom_in_dock`. It only reads `document.zoomed_panel` off the file and later asserts the
re-saved document has `zoomed_panel: None`. The only test that exercises the restore is
`pre_migration_fixture_loads_and_saves_without_semantic_drift`, which drives `DockArea::load`
directly, bypasses `load_layout`, and never resets the centre — so it cannot speak for this path
either. The same test also does not assert the right dock's **panel name** after the load (it
checks `(size, open)` only; the name is checked after the reset, which rebuilds it).

Repro: `grep -n restore_zoom crates/workspace/src/layout/workspace/layout_tests.rs` — the only hit
inside `load_reset_center_and_save_round_trip`'s line range is none.

The **behaviour is correct** — verified by reading (section 1(c)) and now by the test in section 3,
which fails under the tamper and passes with the fix. This is an evidence-accuracy defect in the
packet, not a code defect. Fix: cite the new test, or drop the zoom clause from that box.

### N2 — Low. "keeping its container name" is inert, and the doc implies it matters

`crates/workspace/src/layout/workspace/persistence.rs:50-54` and
`docs/gui-layout.md:84` ("drops the document's center subtree (keeping its container name, with no
children)").

```rust
state.center = PanelState {
    panel_name: state.center.panel_name.clone(),   // <- never read again
    children: Vec::new(),
    info: PanelInfo::stack(Vec::new(), Axis::Vertical),
};
```

`PaneTree::build_node` (`state_convert.rs:166`) dispatches purely on `info`. `panel_name` is
consulted in exactly one arm — `PanelInfo::Panel(_)` at `state_convert.rs:219` — which a forced
`PanelInfo::Stack` can never reach. And the next `dump` writes `STACK_PANEL_NAME` unconditionally
(`state_convert.rs:58`), so the preserved label does not even survive to the next save. The clone
is harmless dead weight; the doc sentence reads as if the retained name preserves something.
Fix (optional): `PanelState::new(&state.center.panel_name)` is no better — just say "with no
children" in the doc, or drop the clone.

### N3 — Low / advisory. `load_layout` is now coupled to one caller's assumption

`crates/workspace/src/layout/workspace/persistence.rs:41`. The function is named "load the layout"
and silently discards a third of it. Today that is safe: `pub(crate)`, one production call site,
and the doc comment states the coupling plainly. But a future second caller — a "reload layout"
command, a workspace-switch, a crash-recovery path — would silently lose the centre with no
compile-time signal. Not worth a refactor now (the HLD rejected the alternatives for good
reasons); worth a line in the packet's Gaps so the next person touching this file sees it.

### N4 — Info. The version-mismatch prompt path has no test, before or after

`persistence.rs:56-77`. Nothing in `layout_tests.rs` loads a document whose `version != 3`, so the
prompt, the spawned answer task and the `reset_default_layout` branch are covered by reading only.
`VisualTestContext::simulate_prompt_answer` exists, so a test is cheap if anyone wants one. This is
a **pre-existing** gap the packet neither created nor widened, and the packet's Context section
does state the reasoning ("Version-mismatch prompt: unaffected"). Recorded so it is not mistaken
for coverage.

### N5 — Nit. "config_dir() is target/ next to it" is imprecise

Packet Evidence, E2E preamble. `config_dir()` (`crates/core/src/config/shell.rs:90`) returns the
**relative** path `target` under `debug_assertions`; it resolves against the process working
directory, not against the executable. The conclusion is unaffected — the launcher's working
directory is the worktree root, so it is the worktree's `target/`, and my own run confirms the
fixture there was read and rewritten — but a reader who launches the exe from elsewhere would get a
different file, and the owner's `~/.OneTerm/docks.json` claim rests on the cwd, not on the exe
location.

## What I did not verify

- The implementer's ten-launch tables (both with-fix and HEAD baseline) were not re-run; I ran
  three launches with the fix only. The HEAD baseline is accepted on the tamper reproduction.
- macOS / Linux: no runtime check (no host). Reading shows no platform-specific code on this path.
- The packaged release binary: E2E used `fast-dev`, same as the implementer.
