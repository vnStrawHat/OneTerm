# US-0145 independent verification

- Date: 2026-09-28
- Commit under test: `d6cd1afb` (branch `perf/idle-render`, one commit on main `eed33058`)
- Packet: [`US-0145`](../US-0145-idle-render.md); intake [`IN-0046`](../IN-0046.md); owning
  docs `docs/gui-layout.md` § Frames and re-rendering, `docs/terminal-backend.md` § 6.4,
  research [`hotpath-evaluation.md`](../research/hotpath-evaluation.md) § 8
- Host: Windows 11 Enterprise 10.0.26200, toolchain 1.96.0, the worktree's own `target/`,
  `CARGO_BUILD_JOBS=3`, another agent building in parallel. Every app instance was launched
  by this verification with a private `USERPROFILE` and driven only through its own pid.

## Verdict: PASS (findings F1-F7; F1 must be fixed in the records before merge)

The behaviour is correct. A terminal in a tab its group is not showing no longer notifies,
every other channel a hidden tab feeds still works (title, Agent Panel, bell badge, content
on show, PTY drain), Spaces and side-by-side tab groups still notify, and the committed test
fails when the guard is removed. The timer grid is sound under clock steps. Idle frames go
down as claimed (3.47 -> 2.04 focused, 1.76 -> 1.23 unfocused). The one claim that does not
hold as written is the TUI CPU saving: on this machine the window drew ~32 frames a second in
both builds, so the frames the fix freed went to the shown terminal (twice as many terminal
renders) and UI-thread cycles went **up** 39 %. The mechanism is confirmed (frames that draw
no terminal: 50 % -> 0.2 %); the CPU direction depends on whether the frame rate is capped,
and the packet, research § 8.4, `IN-0046.md` and the commit message state it as a fixed -51 %.

## Evidence

### 1. Hidden-tab notify suppression

What a hidden `TerminalView`'s `cx.notify()` used to drive, and what drives it now
(`crates/terminal-view/src/terminal_view/view.rs` `handle_event`, `in_hidden_tab`;
`crates/terminal-view/src/panel/terminal_panel.rs` `is_hidden_tab`, `set_active`):

| Consumer | Depends on the view's notify? | Result |
| --- | --- | --- |
| Tab label, OSC 0/2 | No: `SessionEvent::Title` emits `TitleChanged`, the panel's `_title_subs` notify the panel. The emit runs before the guard. | GUI: hidden tab's label read `HIDDEN-TITLE` ~1.1 s after the script wrote it, while still hidden. Throwaway test `v_title_in_hidden_tab_still_refreshes_the_tab_strip`: view not notified, panel notified. |
| Bell | `has_bell` is drawn as a badge inside the view (`render.rs` `bell_badge`); the tab strip has no bell, activity or unread indicator. | GUI: badge present on the first frame after showing the tab. |
| Child exit / close | No "Closed" state on the tab strip. `Exited`/`Closed` update the Agent registry and the SSH-closed flags; the toast drains in `render`. | Unchanged: a hidden view was never rendered before either (the kit renders only the active panel, `tab_panel.rs:782`). |
| Agent Panel (OSC 20308) | No: `push_agent_status` updates `AgentRegistry`, which the Agent view observes. | GUI: a `working` card from the hidden tab appeared in the shown Agent panel within 1 s. |
| Broadcast channels (IN-0022) | No: input fans out through `InputChannelRegistry` to sessions; tab chips read the registry. | Code read; no path through the view's notify. |
| Search | `search.mark_dirty()`; refreshed in `render`. | Unchanged (hidden view is not rendered). |
| OSC 9 toasts, SSH-closed toast | Queued, drained in `render`. | Unchanged; delivered when shown. |
| Status bar (breadcrumb, git, net, CPU/MEM) | No: timer-sampled from the active session / process. A hidden tab is not the active session. | Unchanged. |
| SFTP follows CWD (OSC 7) | No: `SftpPanel` polls the active session's cwd every 500 ms. | Unchanged; a hidden tab is not the one it follows. |
| Recording dot | Read from the session's logging state when the strip renders. | A self-stop of a hidden tab's log shows at the next frame (<= 1 s, the clock), instead of the next output. Negligible. |
| PTY drain / snapshot | The events task never waits for a render; `TerminalContent::refill` (`snapshot_update`) runs in `render`, so it is deferred until the tab is shown, as before. | GUI: 200,000-line flood (`print` of 90 chars + index) in cmd.exe: shown 5.72 / 5.63 s, hidden 4.94 / 6.56 s (a release build ran concurrently); on show the last line `199999` and the prompt were on screen. No stall. |
| Showing the tab | The switch frame renders the newly displayed panel fresh (it was not in the previous frame, so the kit's `.cached()` has nothing to reuse); `set_active(true)` arrives on the next tick and notifies the panel. | GUI: screenshots 100-120 ms after the tab click showed the complete output, the `HIDDEN-DONE` line and the bell badge. No stale frame. |

Throwaway `#[gpui::test]`s (in `crates/terminal-view/src/panel/verify_tmp.rs`, run and then
deleted; not committed), all passing on `d6cd1afb`:

- `v_two_groups_both_displayed_both_notify`: `h_split` of two tab groups, one panel each: both
  are shown, both views notify on output.
- `v_switching_tabs_moves_the_guard`: `select_tab` on the hidden one; after the kit's deferred
  sync the old tab is hidden, the new one notifies. Before the sync tick the newly shown panel
  still counts as hidden (printed `true`): the one-tick window of F6.
- `v_split_spaces_follow_their_tab`: each tab split right and filled through `place_view`
  (which sets `split_ctx`): both Spaces of the shown tab notify, neither of the hidden tab's.
- `v_title_in_hidden_tab_still_refreshes_the_tab_strip`: see the table.

The kit's contract (`gpui-base` 0.7 `dock/active.rs`, `tab_group.rs`) delivers exactly one
`set_active` per edge on the next tick, seeds a panel moved between groups with its last
state, and tells a collapsed group's panels `false`; no path leaves a displayed panel with
`is_active == false` beyond that tick. A panel outside a group (`tab_panel == None`) and a
view whose panel is gone both count as shown, so they still notify (safe default).

Mutation: `if true || !self.in_hidden_tab(cx)` -> `output_in_a_hidden_tab_does_not_notify_its_view`
fails (`left: [hidden, shown]`, `right: [shown]`), as do three of the throwaways. Restored.

### 2. Timer grid (`oneterm_state::until_next_tick`)

`wait = interval - now % interval`, plus one interval when that is under a quarter, so every
wait is in `[interval/4, 5*interval/4)` and never zero. The sleep itself is monotonic; the wall
clock only picks the phase. A throwaway simulation of the exact arithmetic through a repeating
timer (`tick_sim.py`, scratch) with 0-16 ms late and 3 ms early wake-ups, NTP steps of -10 ms,
-499 ms, -3 s, +490 ms, +1 h, and a -7 ms step every third lap: the gap between consecutive
ticks stayed within 484-522 ms in every case (no double toggle, no stall). A clock before the
epoch gives a full interval. 500 ms, 1 s and 2 s ticks always coincide on the 500 ms grid.
Suspend/resume: the timer fires late once, the next wait is recomputed, so one long gap
and then the grid again.

"Everything jumps together": the blink, the clock and the CPU/MEM text now change in the same
frame, and the clock changes exactly on the wall-clock second instead of up to a second late.
Judged from the code and the walk screenshots (not a watched side-by-side), this is not an
artifact the owner is likely to notice: the three sit far apart on screen and the clock is
now simply punctual. Not verified by eye over a long session (gap).

### 3. Attribution and gains (reproduction)

`hotpath-measure.ps1` as committed (`-IdleSeconds 60`, `-Activate` / `-Inactive`, `-Mode Tui
-LoadSeconds 120`), back-to-back pairs, release + `oneterm-app/hotpath-profiling`. "before" is
`d6cd1afb` with only the three behaviour lines reverted (the notify guard, both
`until_next_tick` calls), so both builds carry the same sites. Frames = `OneTermWorkspace::render`
calls over the whole run (startup included, ~5 s of 66-161 s).

| Load | before | after |
| --- | --- | --- |
| Idle focused | 3.47 frames/s, 34.95 Mcycles/s, tick CPU 1.92 %, hotpath thread 2.2 % | 2.04 frames/s, 33.73 Mcycles/s, 1.22 %, 1.4 % |
| Idle unfocused | 1.76, 29.85, 1.48 %, 1.6 % | 1.23, 18.51, 0.39 %, 0.7 % |
| Idle, Agent panel shown (terminal lost focus to the toggle click) | 2.60, 38.89 | 2.12, 26.68 |
| TUI, 2 tabs, 120 s | 32.0 frames/s, 2,594 terminal renders (50.4 % of frames), 289.6 Mcycles/s, 13.5 % | 32.3 frames/s, 5,174 terminal renders (99.8 %), 403.4 Mcycles/s, 18.6 % |

The committed raw reports, read the same way, give the packet's figures (3.18 -> 2.11, 1.65 ->
1.25, TUI 44.4 -> 31.3 frames/s with 7,071 / 7,136 terminal renders): the claims match their
data. Idle frames: confirmed in direction and size. TUI: see F1.

Sampler and graph: `research/raw/us0145-sampler/` is C#, PowerShell and Python under `docs/`,
not a Cargo package. `cargo metadata --no-deps` lists 20 packages, none from it; `Cargo.toml`
is unchanged; `Cargo.lock` gains only `hotpath` in the three new leaf crates' dependency lists.
`verify-dependency-graph.py`, `check-english.py` and `check-doc-paths.py` pass in the gate. The
sampler only opens the pid that the measure script launched.

### 4. Features and binary

- `cargo clippy --workspace --all-targets --features oneterm-app/hotpath-profiling -- -D warnings`
  and the same with `hotpath-profiling-alloc`: clean. No feature: clean (gate).
- `cargo tree -p oneterm-app -e normal -i hotpath`: not in the graph without the feature; with
  it, reached from app, highlight, local-shell, session-ui, sftp-ui, terminal-view, and the rest.
- Release exe without the feature: main `eed33058` 41,487,872 bytes, fix 41,488,896 (+1,024,
  one alignment page: the fix adds real code, so byte equality is not expected). The string
  `hotpath` occurs 0 times in both (36 in the feature build).

### 5. Tests

`cargo test -p oneterm-state -p oneterm-workspace -p oneterm-terminal-view`: 42 + 394 (3
ignored; 390 committed + the 4 throwaways) + 42 (3 ignored) passed. Mutation: see section 1.

### 6. Records

Packet dated 2026-09-28, owning docs reviewed and reconciled, evidence and gaps present. The
docs describe the change correctly apart from F1, F3 and F4. Proposed `story` row below.

### 7. Gate

`CARGO_BUILD_JOBS=3 pwsh scripts/ci-local.ps1` (no `target/release`), final line:
`ci-local: all checks passed.` (141 `test result: ok` lines, none failed)

## Findings

- **F1 (Medium, records; fix before merge). The TUI CPU saving is conditional.** Here the
  window drew ~32 frames a second in both builds; the fix turned the 2,555 frames that drew no
  terminal into terminal frames, so the shown tab refreshed twice as often (16 -> 32 per second)
  and UI-thread cycles rose 290 -> 403 Mcycles/s (+39 %; hotpath thread 13.5 -> 18.6 %). In the
  implementer's run the frame rate was not capped (44 -> 31 per second, terminal renders equal)
  and cycles halved. Both runs confirm the mechanism: no frame draws nothing. The packet's
  Evidence, research § 8.4 (table and text), `IN-0046.md` ("TUI -30 % frames and -51 % UI
  cycles") and the commit message state the -51 % as a result; they should say it holds when
  frames are not capped, and that a capped window instead gives the shown terminal the freed
  frames at a higher cost per second.
- **F2 (Low). Idle focused cycles fell less here.** -3.5 % (34.95 -> 33.73 Mcycles/s) against
  the claimed -14 %, with the frame cut as claimed (-41 %) and tick-charged CPU -36 %. One pair
  each; worth quoting as a range.
- **F3 (Low). One repainting timer is not on the grid.** `AgentListView`'s refresh task
  (`crates/agent-ui/src/view.rs`) counts 120 ms ticks and notifies every ~1.08 s even with no
  cards. With the Agent panel shown and the terminal unfocused the fixed build draws 2.12
  frames/s against 1.23 with the SSH Client panel. The rustdoc of `until_next_tick` and
  `docs/gui-layout.md` say "every timer that repaints the window" sleeps on the grid. Either
  qualify the sentence or add the Agent panel's 1 s refresh to the grid (a follow-up candidate
  for § 8.5).
- **F4 (Low, docs).** `docs/gui-layout.md`: "an idle, focused window draws 2 frames a second, an
  unfocused one 1". Measured 2.04-2.11 and 1.23-1.25 (and more with the Agent panel, F3);
  "about 2 and about 1.25" would match.
- **F5 (Low, records).** The packet has no `## Handoff` section although the template has one
  and the work crosses actors; `Platform proof` is ticked while the other IN-0046 stories keep
  it at 0 until CI runs; one `high-level-design.md` line is not wrapped.
- **F6 (Info).** For one tick after a tab switch the newly shown panel still counts as hidden
  (kit delivers `set_active` on the next tick), so output in that tick does not notify. Benign:
  the switch frame renders the panel fresh and `set_active(true)` notifies it.
- **F7 (Info).** `until_next_tick(Duration::ZERO)` returns 1 ns, a spin if a caller ever passed
  zero. All callers pass 500 ms, 1 s or 2 s constants.

## Gaps

- Split panes and side-by-side tab groups were proven with `#[gpui::test]`s, not walked in the
  GUI (the split shortcut needs a real modifier state that posted messages do not set).
- One back-to-back pair per load; TUI frame rate depends on the window's desktop visibility.
- The Agent-panel runs rely on the title-bar click at (1076, 16); its effect was confirmed by a
  screenshot in a separate walk, not in the measured runs.
- No macOS/Linux run; SSH sessions not exercised (the guard is transport-independent).
- GUI walks used the release `hotpath-profiling` build of `d6cd1afb`, cmd.exe tabs, OSC tab
  titles turned on in the private `terminal.json` (`layout.tab_title = "osc"`; the default
  mode shows static labels).

## Proposed harness row (`story`, 17 columns; not written)

```text
id: US-0145
title: Attribute and cut the UI thread's work outside the terminal element
created_at: 2026-09-28T00:00:00Z
risk_lane: normal
contract_doc: docs/gui-layout.md
packet_doc: docs/spec-intakes/IN-0046-hotpath-profiling/US-0145-idle-render.md
status: implemented
unit_proof: 1
integration_proof: 1
e2e_proof: 0
platform_proof: 0
evidence: docs/spec-intakes/IN-0046-hotpath-profiling/evidence/US-0145-verify.md
verify_command: cargo test -p oneterm-state -p oneterm-workspace -p oneterm-terminal-view; pwsh scripts/ci-local.ps1
last_verified_at: 2026-09-28T11:41:04Z
last_verified_result: pass
notes: Impl d6cd1afb on perf/idle-render. Hidden tabs skip cx.notify (TerminalView::handle_event), blink + status timers on until_next_tick. Verify PASS: hidden-tab title/agent/bell/flood/show walked in GUI, 4 throwaway gpui tests (split, two groups, switch, title), mutation kills the test; idle 3.47->2.04 fps. F1: TUI CPU -51% is conditional (capped window here: +39% cycles, shown-tab renders 2x) - fix records before merge. F3 Agent panel timer off-grid. platform_proof pending CI.
intake_id: 51
```
