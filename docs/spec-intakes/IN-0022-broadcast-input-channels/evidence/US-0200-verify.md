# Advisor code review (US-0200)

Date: 2026-10-22. Reviewed head: `1158fa3b` (three commits on `main` @ `3acc6db8`): `e38a0999`
(IN-0055 owner rulings, contrast script wording), `da9fdef6` (US-0200 packet), `1158fa3b`
(the code). Verify branch `verify/us-0200` in the coder's worktree.

## Verdict

**PASS.** No product defect found; no pinning test added. One open owner question (finding 1,
with a fix shape) and three low record notes.

## Gate, re-run one crate set per command

| Command | Result |
| --- | --- |
| `cargo test -p oneterm-terminal-view -p oneterm-state -p oneterm-terminal` | state 78, terminal 183, terminal-view 425 (3 ignored); all pass |
| `cargo clippy -p oneterm-terminal-view -p oneterm-state -p oneterm-terminal --all-targets -- -D warnings` | clean (59.7 s) |
| `python scripts/check-theme-contrast.py --self-test` | self-test passed |
| `python scripts/check-english.py` | 1146 files pass |
| `python scripts/check-doc-paths.py` | 219 paths pass |

`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`, the worktree's own `target`.

## Tests fail on the old code (reproduced)

The two product lines (`view.rs`, the `Exited` and `Closed` arms) were reverted locally with
`git apply -R` (not committed) and the two new tests run alone:

- `terminal_view::view_tests::a_session_that_ends_leaves_its_channel` FAILED at
  `view_tests.rs:673`: `left: Some(A), right: None` (the member stayed after `Exited`).
- `panel::tests::an_exited_space_leaves_the_tab_chips` FAILED at `tests.rs:909`:
  `left: [A, B], right: [B]` (the chip stayed).

File restored; `git status` clean afterwards. With the product lines back, both pass.

## The brief's attack list

1. **Does anything rely on an exited member staying in the registry?** No. Every production
   reader of `InputChannelRegistry` is in `crates/terminal-view`: `panel/channel_menu.rs:85-88`
   (the active Space's channel for the menu rows), `panel/terminal_panel.rs:546-555`
   (`tab_channels`, the chips), `space/render.rs:128` (the badge and frame), and the four
   `fan_out` hooks (`input.rs:131,146`, `ime.rs:72`, `edit.rs:50`). The status bar, recording,
   focus and the tab-wide rows (`terminal_panel.rs:510-521`) read nothing from it; `join_tab`
   and `leave_tab` iterate the tree, not the registry. The US-0199 disabled-row rule
   (`channel_entries`, `channel_menu.rs:95-101`) keys only on the absence of a terminal view
   and is untouched: an exited Space still gets the live submenu, now showing it as a
   non-member (Join rows only), which is the same shape a fresh split Space shows.
   Behaviour change worth knowing: before, typing into an exited member's banner view still
   fanned out to its peers; now it does not (the view is a non-member). That follows from the
   owner's rule and is correct.

2. **Re-entrancy of `leave_channel` inside `handle_event`.** Safe. Both callers of
   `handle_event` wrap it in `this.update(cx, ...)` (`view.rs:239` the events pump,
   `view.rs:351` the coalescing drain), so `leave_channel`'s `registry.update` is a nested
   update of a different entity, exactly as `shutdown` (`view.rs:490`) already does.
   `registry.leave` calls `cx.notify()` (`input_channel_registry.rs:103-109`), which queues an
   effect; the panel's observer (`terminal_panel.rs:288-291`) only calls `cx.notify()` and
   runs after the outermost update returns. No synchronous read of the view while it is
   borrowed. The panel test drives the event through the fake session's event channel, so the
   real pump path (`view.rs:239`) is covered, not only the direct call.

3. **SSH: can `Closed` fire for a still-usable session?** No. `publish_closed` is called once,
   in the single teardown block after the `loop` breaks (`crates/ssh/src/task.rs:139-144`); the
   task then exits and nothing restarts it. `None => "disconnected"` (`:104`) is the jump-hop
   or carrier loss case and is also final. `Exited` comes only from `ExitStatus` /
   `ExitSignal` (`:93-100`), after which the shell channel is done. A session-level
   `set_alive(false)` outside the pump (`crates/ssh/src/session.rs:956`) sends no event. No
   reconnect or re-open exists anywhere in `crates/ssh`. `Closed` on `Close Tab` reaches a view
   that already left in `shutdown`; `registry.leave` returns `false` and does not notify.

4. **Manual join of an exited Space.** See finding 1 below (owner question, recommendation
   given).

5. **Tests fail on the old code.** Reproduced above.

6. **Contrast script edit.** Wording only: one comment block and one `print` f-string
   (`scripts/check-theme-contrast.py:310-312,814`); `KNOWN_BELOW`, `split_known` and the
   stale-entry failure are untouched. `--self-test` passes. The only other place that quoted
   "awaiting the owner" is the US-0198 packet, which now records the new string.

7. **Records.** `IN-0022.md` (packet row, lifecycle sentence), HLD data flow 6, LLD hook
   paragraph and the exited-peer edge case, `docs/gui-layout.md`, `docs/terminal-split.md`
   all state the same rule (leave on `Exited` or `Closed`, no rejoin, manual join not blocked)
   and match the code. The IN-0055 rulings (US-0196: no Zoom Out menu item; US-0198: PowerShell
   icon kept, `KNOWN_BELOW` accepted) are recorded in the packets' own ruling paragraphs.
   `DEC-0009` correctly unchanged. `evidence/US-0199-verify.md:237-238,266-267` describe the
   pre-US-0200 behaviour as history; that is what an evidence file does and needs no edit.

## Findings

1. **Open for the owner (not a defect): a manual join of an already-exited Space is allowed
   and then holds until the tab closes.** `crates/terminal-view/src/terminal_view/view.rs:517-524`
   (`join_channel` has no liveness check), `panel/terminal_panel.rs:510-514`
   (`join_tab_to_channel` joins every terminal view, dead ones included),
   `panel/channel_menu.rs:95-101` (the submenu is live for an exited Space). The packet
   records this as a deliberate non-choice (Plan, "join of a dead Space") and the test pins the
   current behaviour (`view_tests.rs:692-698`).
   **Recommendation: block it, for consistency with the auto-leave.** The rule the owner gave
   is "a dead session is no broadcast target"; letting the user put one back in by hand (and
   `Join All Spaces In Tab` doing it silently for every exited Space of the tab) re-creates
   the exact state US-0200 removes, with the same logged-warning writes. Fix shape, small:
   `join_channel` returns early when `!self.session.read(cx).alive()` (`alive()` is already
   false after `Exited` via `record_exit`, `crates/terminal/src/backend/state.rs:110-113`,
   and after `Closed` via `set_alive(false)`, `pump.rs:227,234`); `channel_entries` returns
   `None` when the active view's session is not alive, so the tab row shows the existing
   disabled "Input Channel" row (the US-0199 shape for a Space that cannot join), and
   `join_tab_to_channel` skips dead views for free through the guard. Flip the last four lines
   of `a_session_that_ends_leaves_its_channel` to assert `None` after the by-hand join and add
   one `channel_menu` test for the disabled row. A new US packet (or a reopen of US-0200 if the
   owner rules before merge); the US-0199 packet line 78 ("nothing in `IN-0022` bars a terminal
   whose process exited") and `channel_menu.rs:136-137` would then change too. If the owner
   keeps it unblocked, nothing to do: the records already say so.

2. **Low, record.** `crates/state/src/input_channel_registry.rs:156-158` (`fan_out` rustdoc):
   "A peer whose session rejects the write (an exited session behind the 'session closed'
   banner)" now only happens after a by-hand rejoin of a dead Space; the LLD edge case was
   updated to say this, the rustdoc was not. Fold into finding 1's packet or leave.

3. **Low, record.** `US-0200-leave-channel-on-exit.md`: the status block is still
   "In progress", the PROOF block and the "Full `ci-local`" acceptance box are unchecked, and
   "Gate: see the report of the dispatch" points at nothing durable. The coordinator's merge
   step should run `harness story` to mark it implemented and paste the gate line here.

4. **Low, test shape.** `a_session_that_ends_leaves_its_channel` loops over two endings and
   re-runs `test_support::init` and `InputChannelRegistry::init` inside the loop; `init` is
   guarded by `try_global` so the second pass reuses the same registry, which still works
   because every id from the first pass has left. A leak from pass one would mask as a pass
   two failure with a confusing message. Acceptable; noted so nobody adds a third ending that
   does not end with every view out.

## What the owner must still check by hand

- The on-screen half: a chip disappearing from the tab, and the Space frame and badge going,
  when a shell exits (the `ui-verifier` dispatch the packet names). Not launched here.
- A real SSH drop (pull the network, or `kill -9` the remote shell) on a joined Space: the
  chip should go on `Closed` alone; only the fake session was used here.
- The ruling on finding 1.

# Advisor re-review (join of an exited Space blocked)

Date: 2026-10-22. Reviewed head: `371de3c5` (one commit on `a19829bf`), `verify/us-0200`
fast-forwarded to it. The owner ruled on finding 1 above: block the join. The coder applied
the fix shape as written.

## Verdict

**PASS.** No defect; one low record note (finding R1). No pinning test added.

## Gate

| Command | Result |
| --- | --- |
| `cargo test -p oneterm-terminal-view -p oneterm-state -p oneterm-terminal` | state 78, terminal 183, terminal-view 426 (3 ignored); all pass, incl. `joining_the_tab_skips_an_exited_space`, `only_a_tab_without_a_running_terminal_loses_the_channel_submenu`, flipped `a_session_that_ends_leaves_its_channel` |
| `cargo clippy` same three crates `--all-targets -- -D warnings` | clean (re-checked `oneterm-state`, `oneterm-terminal-view`) |
| `python scripts/check-english.py`, `check-doc-paths.py` | 1147 files / 219 paths pass |

## The attack list

1. **`alive` timing against the events.** Safe in both backends. Every `Exited` and `Closed`
   comes from `TerminalPump`, and both variants flip the flag before they send:
   `publish_exit*` calls `record_exit` (sets `exit_code`, then `set_alive(false)`,
   `crates/terminal/src/backend/state.rs:111-114`) and `publish_closed*` calls
   `set_alive(false)` first (`crates/terminal/src/backend/pump.rs:210-236`); the event is sent
   after the flush. The local shell goes through `publish_child_exit`
   (`crates/local-shell/src/event_loop.rs:678-685`) and SSH through `publish_exit` /
   `publish_closed` (`crates/ssh/src/task.rs:97,100,144`); nothing else sets `alive` on a
   session (`crates/ssh/src/session.rs:956` is a test). So at the moment the view sees the
   event, `alive()` is already false: a join attempted before the flip succeeds and is undone
   by the auto-leave on delivery; one attempted after the flip is refused. No ordering lets a
   dead Space end up a member. The one known hole is older and symmetric: the Linux lost-hang-up
   case documented at `event_loop.rs:636-642` keeps `alive()` true and sends no event, so the
   Space neither leaves nor is refused; that is the BUG-0074 lane, not this packet.

2. **Other paths into `registry.join`.** Only `TerminalView::join_channel`
   (`view.rs:533`) outside tests. The key-bound / palette `JoinInputChannel` action
   (`terminal_panel.rs:627-630`), the menu row (`channel_menu.rs:111`) and
   `join_tab_to_channel` (`terminal_panel.rs:511-514`) all go through it. Duplicate Session
   and split create a fresh non-member view (HLD flow 6); nothing restores membership
   (DEC-0009, no persistence). Info: a key-bound `JoinInputChannel` on a dead Space is
   refused silently, which matches the empty-Space case and the disabled menu row.

3. **Disabled row when the active Space is dead but a sibling lives.** The whole
   "Input Channel" row is disabled, so "Leave With All Spaces In Tab" and "Close Channel"
   are unreachable from that tab's menu until the user focuses a live Space. This is the
   same shape US-0199 already fixed for an empty active Space with live siblings (owner
   rule, ui-verifier D1), so it is consistent, and the key-bound Leave / Close still work
   from the live Space. If the owner wants the tab-wide rows reachable from a dead active
   Space, the shape is: keep the submenu and omit only the five Join rows when
   `!session_alive` (`channel_menu_items` gains a `can_join` flag). Not recommended now;
   the ruling asked for the disabled row.

4. **Records.** `gui-layout.md`, HLD flow 6, LLD edge case, US-0199 acceptance and Plan,
   `fan_out` rustdoc, US-0200 Scope / Acceptance / Plan / status / proof blocks all say
   "refused" and match the code. `docs/terminal-split.md:52-54` says only "no automatic
   rejoin"; still true, no edit needed. See R1 for the stale lines.

## Findings

- **R1, Low, record.** `US-0200-leave-channel-on-exit.md:97-98` ("Evidence and Gaps"): the
  first two bullets still describe the first version ("Code: two lines in `handle_event`";
  the view test "... a join by hand holds until `shutdown`"), which the rework bullet below
  then contradicts. Rewrite the two bullets to the current code (`join_channel` guard,
  `session_alive`, `channel_entries`) and the current assertion (the join is refused, the
  running member can still move). Text only; can land in the merge commit.

## Still by hand

Unchanged from the first review (on-screen chip / badge / frame, a real SSH drop), plus:
the disabled "Input Channel" row on a tab whose shell has exited, and "Join All Spaces In
Tab" on a split tab with one exited Space (the exited one gets no badge).

## UI verification

Verifier: ui-verifier, 2026-10-22. Binary built from 371de3c5 (later commits docs only); head f7d5a40b. Private profile (scratch USERPROFILE and cwd), pids spawned by the driver only. Screenshots: `C:\Users\trunglt\AppData\Local\Temp\claude\D--TrungKFC-Research-Rust-myTerm2\fa2d0135-9c28-4a69-a255-ba7479e604cb\scratchpad` prefix `uiv200-` (full-window shots, include the ~8 px frame; driver clicks are client coordinates, x = shot x - 8, y = shot y). Desktop was unlocked (shots not black).

Environment limit: in this sandbox every child shell (Command Prompt, PowerShell, PowerShell 7) is already dead at start (blank terminal, no prompt), so every tab under test is an exited Space. Git Bash is not offered in the + menu.

| Item | Result | Shot |
|---|---|---|
| 1a Vertical row, exited tab: "Input Channel" row present, greyed, no submenu, between the two separators (Rename/Duplicate, sep, Input Channel, sep, Close/Close Others/Close Tabs Below); checked on 4 tabs, clicking the row does nothing | PASS | uiv200-v02-menu, uiv200-v05-menu-tab153/217/281, uiv200-v08-click-disabled |
| 1b Collapsed cell | PASS | uiv200-v07-collapsed-menu |
| 1c Horizontal strip (2 tabs) | PASS | uiv200-h02-menu, uiv200-h06-ps7-menu |
| 2 No chip/frame on any exited tab (tab menu path: row disabled, so no join possible) | PASS | uiv200-v04-tabs, uiv200-h03-after-keyjoin |
| 2 Key-bound join: bound `join_input_channel_a` to ctrl-alt-1 through ui_config.json (no default shortcut exists), pressed it on exited tabs: no chip, no frame | PASS (see note 1) | uiv200-h03-after-keyjoin, uiv200-h06-ps7-menu |
| 3 Live shell joined then exited; live sibling keeps chip; Join All Spaces In Tab on a split with one dead and one live Space | NOT WALKED (no shell survives here) | |
| 4 Tab menu order unchanged; terminal-area menu has no Input Channel | PASS | uiv200-h07-area-menu |
| 4 Light (Zed One Light) and dark (Zed One Dark) glance: disabled row legible but clearly dimmer than Close | PASS | uiv200-v02-menu, uiv200-d01-menu |

Note 1: the key press was posted with the driver chord; the binding loading is not proven by a positive control (no live shell to show it firing), so this row proves only "no chip appeared", not that the action was dispatched and refused.

### Defects

None found in the walked scope. Observation (Info, not US-0200): on a dead terminal the area menu still offers Split and Duplicate Session enabled; not part of this packet.

### Owner-only checks

- A live shell joined to a channel, then xit: chip, badge and frame disappear at once; a live sibling keeps its chip.
- Split tab with one dead and one live Space: "Join All Spaces In Tab" joins only the live Space.
- Key-bound join (Settings > Key Bindings > Input Channel) on a dead Space with a live shell elsewhere.
- SSH drop (Closed without Exited) leaving the channel.
