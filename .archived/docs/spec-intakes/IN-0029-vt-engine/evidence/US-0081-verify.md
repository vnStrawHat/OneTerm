# US-0081 — independent verification

Verifier: separate agent, no authorship of the packet. Branch
`worktree-agent-a53e46421076a2d27` @ `c349dd5` (merge of `feat/vt-engine` @ `3538047`),
worktree `.claude/worktrees/agent-a53e46421076a2d27`, built only into that worktree's
`target/`. Nothing committed; the one file added is the untracked scratch differential
`crates/terminal/tests/us0081_parity.rs` (delete or adopt — see Finding 9).

Environment: Windows 11 Enterprise 10.0.26200, D: had 21.8 GiB free at start (above the
20 GB floor) and 13.4 GiB at the end. Owner's `oneterm.exe` pid **27376** recorded before
anything was launched and never touched; the one instance this verification started
(pid 26448) was stopped by pid.

## Verdict

**Merge after fixes** — one major finding (an extra `SessionEvent::Output` per chunk, a
one-line fix that contradicts the packet's own "same event sequence, same order"
acceptance) and eight minor ones, all documentation or scheduling. No blocker. The seam
itself is in better shape than the packet claims: an 81-stream cell-by-cell differential
against the old engine shows **zero** differences in cells, cursor, selection,
display_offset, bounds and graphics, and the new damage is **sound** on every stream.

| # | Check | Result |
| --- | --- | --- |
| 1 | Scope vs the may-touch table, trailers | **PASS** (1 nit: merge commit carries no trailers) |
| 2 | `ci-local.ps1` green, test dispositions | **PASS** |
| 3 | Snapshot behaviour parity (the differential) | **PASS with 5 declared-elsewhere/undeclared deltas, none user-visible** |
| 4 | Events reach the same consumer with the same payload | **FAIL (minor→major)** — payloads identical, but `Output` is emitted twice per chunk |
| 5 | Locking / threading, no deadlock | **PASS** |
| 6 | Performance reproduced and attributed | **PASS** — attribution confirmed by direct isolation |
| 7 | GUI walks | **NOT REPRODUCIBLE** — RDP session `Disc`; implementer's evidence spot-checked and genuine |
| 8 | Code quality | **PASS** |
| 9 | Packet completeness, DB row | **PASS** (3 inaccuracies, listed) |

---

## 1. Scope

`git diff 3538047...HEAD --stat` — 51 files. Outside `crates/terminal`:

| File | What changed | In the may-touch table? |
| --- | --- | --- |
| `crates/local-shell/src/{event_loop,session}.rs`, `crates/ssh/src/{session,task}.rs` | shared-terminal type + construction call only | yes |
| `crates/{local-shell,ssh}/src/session_terminal.rs` | the `impl_pty_terminal_session!` listener parameter and its `use` (4 lines) | declared deviation (c) |
| `crates/local-shell/src/event_loop_tests.rs` | fixture type, construction, `grid_text()` reader; assertions untouched | declared deviation (b) |
| `crates/terminal/Cargo.toml` | `+oneterm-vt`, `+parking_lot`, description | yes |
| `Cargo.toml` | `parking_lot` workspace dep; `oneterm-vt` + `oneterm-pty` at `opt-level 3` in `fast-dev` | partly (see Finding 7) |
| `scripts/dependency-graph-policy.json` | `oneterm-terminal → oneterm-vt` | yes |
| `docs/agents/{structure,crate-dependency-rules}.md`, `docs/terminal-backend.md` | the rows `migration.md` assigns to this packet | yes |

`git diff 3538047...HEAD --stat -- crates/vt crates/terminal-view crates/state crates/app
crates/workspace crates/tools` → **empty**. So: **no consumer edit** (`render/frame.rs` and
`input/mouse.rs` are untouched, as designed) and — worth stating — **no `crates/vt`
accessors were added by this packet**; the shim is built entirely on API that `US-0074…0080`
already shipped (`RenderState::{rows,changed,cursor,selection,modes,size,viewport_top,
scroll_offset,hyperlink,graphic_offset,invalidate}`, `Terminal::{feed,render_update,resize,
screen,grid,interner,placements,take_graphics,color,cursor_style,mode_snapshot,hit_test,
selection_*,select_all}`, `Screen::{row,row_text,rows,cols,history_len,screen_top,
visible_top,scroll_offset}`). That is a real plus: the engine could not have been bent to
fit the shim.

**The `fast-dev` opt-level change is the right fix, not a mask.** The list already carried
`alacritty_terminal = { opt-level = 3 }` for exactly the reason the engine now needs it, so
adding `oneterm-vt` restores like-for-like. Proof it is not papering over a shim cost: with
the profile fix in place the residual gap was still 60x, and §6 below shows 100 % of that
residue is the integrity walk, not the shim. (`oneterm-pty` is a freeloader — see Finding 7.)

**Trailers.** All six non-merge commits carry `Co-Authored-By: Claude Fable 5.1` and a
`Claude-Session` line (`e9ba1d6` carries a different session URL, which is allowed). The
merge commit `4b640eb` carries neither — nit.

## 2. Quality gate and test dispositions

`pwsh scripts/ci-local.ps1` → `ci-local: all checks passed.` (exit 0), all nine steps.
Raw totals, parsed from the log:

```
sections=57 passed=1557 failed=0 ignored=8
```

Identical to the packet's claim. `oneterm-terminal` alone: 264 passed.

Judgement on each disposition the packet lists:

- **`child_exit_sets_alive_false_and_code` (deleted) — coverage genuinely preserved, and
  the replacement is better placed.** `pump_publish_exit_and_closed_flush_deferred_first`
  (`backend_tests.rs:572-607`) asserts `Exited(Some(7))` in order after the deferred `Bell`,
  then `!f.state.alive()` and `f.state.exit_code() == Some(7)`. The real child-exit path is
  covered end to end elsewhere and was not touched:
  `crates/local-shell/src/event_loop_tests.rs:159 child_exit_without_status_still_ends_the_session`,
  `:179 child_exit_with_status_records_code_and_closes`,
  `:465 loop_child_exit_ends_the_session_and_stops_the_thread`, which drive
  `event_loop.rs:531 publish_child_exit → pump.publish_exit_blocking`. The deleted test
  exercised `Event::ChildExit`, which only alacritty's own event loop emitted and OneTerm
  never ran. **Net coverage loss: none.**
- **The resize tests moved to `legacy_resize.rs` (old engine).** They do run: 15 `#[test]`
  functions, all present in the CI log. The same scenarios **are** exercised against the new
  engine — `crates/vt/src/reflow/reflow_tests.rs` has 14 `keep_viewport_top_*` tests with
  matching names (`grow_keeps_rows_cursor_and_history`, `grow_larger_than_history`,
  `repeated_grows_and_column_change`, `widen_joins_wrapped_rows_and_keeps_the_top_row`,
  `narrow_with_a_mid_screen_cursor_pulls_split_rows_back`, `widen_during_alt_screen_…`, …).
  And the shim's path reaches them: `TerminalModel::resize_grid` →
  `Terminal::resize(Size, ResizePolicy::KeepViewportTop)` (`model.rs:216-219`), pinned by
  `model_tests.rs:94 resize_grid_applies_the_backend_policy`, which asserts both policies'
  cursor line and row text through the adapter. **Double-covered, old and new.**
- The ~22 rewritten `backend_tests`, `line_accounting`, `content::tests`, `search::tests`
  and the two `model::tests` were read: assertions are equivalent, several are stronger
  (`snapshot_has_cells_and_bounds` now pins exact cell counts and characters).

## 3. The differential (the important one)

`crates/terminal/tests/us0081_parity.rs` (untracked scratch, written for this review). It
feeds identical bytes to the **old** `alacritty_terminal::Term` — driven by
`Processor::<StdSyncHandler>::advance`, snapshotted through `TerminalContent::refill`
copied verbatim from `3538047:crates/terminal/src/content.rs` — and to the **new**
`Engine`, snapshotting after every 4 KiB chunk plus one empty feed, and diffs every field
of `TerminalContent`: per cell `c` + zero-width followers + `fg` + `bg` + `flags`
(WIDE_CHAR / WIDE_CHAR_SPACER / LEADING_WIDE_CHAR_SPACER / WRAPLINE / all four underline
variants / BOLD / DIM / ITALIC / INVERSE / HIDDEN / STRIKEOUT) + hyperlink + graphic ref +
`point`; cursor point and shape; `mode`; `display_offset`; `total_lines`; `selection`;
`terminal_bounds`; `damage`; `graphics` (count and pixels).

**Corpus: 81 streams** — the 45 vendored alacritty recordings, OneTerm's `sixel_basic`, and
35 hand-written streams (title BEL and ST, bell, OSC 8 hyperlink, OSC 52, OSC 7, OSC 9;7
agent status, OSC 133, colour query and colour set, DA/DSR, `2J`/`3J`, every SGR attribute,
256/truecolor, CJK, emoji, combining marks, wrap, wide-at-edge, hidden cursor, cursor
shapes, alt screen, bracketed paste, mouse 1000/1002/1003/1006/1005, app modes, scrollback,
tabs + scroll region, insert/delete, erase, DECALN, reverse video, RIS, NUL and C0, invalid
UTF-8).

### Result

| Field | Differences |
| --- | --- |
| `cells[*].{c, zerowidth, fg, bg, flags, point}` | **0** |
| `cells[*].graphic` (`GraphicCell{id,col,row}`) | **0** |
| `cursor.point`, `cursor.shape` | **0** |
| `display_offset`, `terminal_bounds` | **0** |
| `selection` (range and text) | **0** |
| `graphics` (count and RGBA bytes) | **0** |
| `cells[*].hyperlink` | 12 cells — **id string only** (`0_alacritty` → `1`), URI identical |
| `mode` | `LINE_WRAP` + `URGENCY_HINTS` dropped (all 81), `ORIGIN` dropped (15) |
| `total_lines` | 1 stream (`oneterm/sixel_basic`): old 10, new 9 |
| `damage` | 48 snapshots differ in shape; **0 unsound** (see below) |

Zero differences in the fields that paint pixels. Cursor parity includes the reference's
two special cases without either being special-cased in the shim: shape becomes `Hidden`
under `\x1b[?25l` (the engine's `cursor_style()` already folds `Mode::ShowCursor` in,
`crates/vt/src/terminal/mod.rs:425-437`) and the wide-char-spacer column adjustment
(`vendor/alacritty_terminal/src/term/mod.rs:2466-2468`) never fired differently across the
CJK, emoji and wide-at-edge streams.

**Damage soundness.** Since the two engines legitimately differ in how much they
over-damage, the differential also checks the property that actually matters: every row
whose rendered content changed since the previous snapshot, plus the cursor row when a
*visible* cursor moves (the view early-returns on `CursorShape::Hidden`,
`crates/terminal-view/src/render/cursor.rs:43`), must appear in the new snapshot's
`Partial` list. **Result: 0 violations over all 81 streams.** Where the two differ the new
list is narrower — the reference damages a line on any write, the engine on an actual
change. Nothing is under-damaged, so no stale row can survive a frame.

**Adjudication of the five deltas** (cross-checked against the intake docs):

| Delta | Covered? | User-visible? |
| --- | --- | --- |
| `LINE_WRAP`, `URGENCY_HINTS` dropped | not declared; `research/api-surface.md:189` lists both under "**Unused mode bits** … OneTerm just never queries them" | no — `grep` over `crates/*/src` finds no reader outside `crates/tools`' corpus dumper |
| `ORIGIN` dropped | same line of `api-surface.md`; no deviation row | no reader |
| hyperlink auto-id form | designed in `low-level-design/cell-and-style.md:135-138` (per-terminal counter, not the reference's process-global `AtomicU32`); the string form is not written down | no — the view hashes `id + uri` (`render/frame.rs:306-312`); distinctness per link run and verbatim explicit `id=` were both verified (`old ["A=0_alacritty","B=1_alacritty","C=q"]` vs `new ["A=1","B=2","C=q"]`) |
| sixel `total_lines` −1 | **nowhere in the intake** | scrollbar length only, Sixel sessions only |
| narrower damage | only the specific case D2; no general rule | no (soundness proved above) |

## 4. Events

The differential also replays every hand stream through both event paths and prints them.
Payload and order are identical for **title (0 and 2), title reset, bell, clipboard store,
clipboard load, OSC 7 / 9;7 / 133 passthrough, colour query (same 256/257/258 indices),
`ClearScreen`, and every reply** (DA1, DA2, DSR 5, DSR 6 — the reply bytes reach the
transport first, R-37, and `replies_are_written_before_the_rest_of_the_batch_is_routed`
pins it). Colour-reply formatting was read against the fork's `dynamic_color_sequence`:
same prefix, same 16-bit doubling, same terminator. Nothing is fired under the lock that
was not before — `OscRouter::drain` is called from `advance` exactly where `send_event`
used to run, and the deferred tier is untouched.

Two differences:

- **DA1 `ESC[?62;4c` → `ESC[?62;4;22c`** — declared, D13
  (`low-level-design/dispatch-and-modes.md:199,389`).
- **DA2 `ESC[>0;2601;1c` → `ESC[>0;502;1c`** — the formula is unchanged; the number moves
  because `CARGO_PKG_VERSION` is now the workspace's `0.5.2`. Not declared anywhere.
- **One extra `SessionEvent::Output` per chunk** — see Finding 1.

## 5. Locking and threading

`Arc<FairMutex<Engine>>` over `parking_lot::FairMutex` behind `crates/terminal/src/sync.rs`
(HLD decision 7). `lock_unfair` / `try_lock_unfair` map onto `lock` / `try_lock` with a
`ponytail:` comment naming the ceiling and the packet that deletes the call sites
(`sync.rs:31-46`); the two call sites are `crates/local-shell/src/event_loop.rs`'s read
loop, which `US-0083` owns. The pump holds the lock across `feed` + `drain` + the colour
replies and releases it before `finish_batch*` (`pump.rs:141-154`) — the same window the
old code held. `render_update` under the lock is bounded by the changed rows plus one
viewport copy.

**Three-thread stress** (`lock_stress_three_threads`, 10 s, feed flood + resize loop over
five sizes + snapshot loop, each asserting the snapshot stays dense, damage rows in bounds,
offset ≤ total):

```
stress: fed 23285980 bytes, 5705 resizes, 5704 snapshots in 10 s
test lock_stress_three_threads ... ok
```

No deadlock, no panic, integrity held (the engine's own `assert_integrity` was live in this
build, so the grid invariants were checked ~11 000 times under concurrent resize and feed).

Demand/yield is unwired, as the packet says. Nothing regresses: the old `MAX_LOCKED_READ`
backstop lives in the read loops, which this packet does not touch.

## 6. Performance

Measured with `flood_bench` in the scratch test: 4 MiB of coloured text through 4 KiB
chunks, grid 120x30, scrollback 10 000, snapshot after every chunk, **both engines in the
same process**. Per-chunk microseconds.

| Build | new feed | new snapshot | new total | old total | new/old |
| --- | --- | --- | --- | --- | --- |
| `fast-dev`, as on this branch | 2659 | 2603 | **5398 ms** | 87 ms | **62x** |
| `fast-dev` + `8f60fc9` cherry-picked (the real fix) | 92 | 75 | **174 ms** | 80 ms | **2.2x** |
| `fast-dev`, `TerminalGrid::assert_integrity` forced to return (local hack, reverted) | 79 | 59 | 144 ms | 85 ms | 1.7x |
| `--release` (`debug_assertions` off) | 51 | 35 | 90 ms | 43 ms | 2.1x |

The packet's attribution is **confirmed by direct isolation**, which is stronger than the
packet's own indirect evidence (shrinking the scrollback): gating
`TerminalGrid::assert_integrity` alone removes 97 % of the debug-build cost, and the real
upstream fix (`feat/vt-engine` @ `d8c619a`, commit `8f60fc9`) lands in the same place — 5398 ms
→ 174 ms on this branch's code with no other change. Both temporary edits were reverted;
`git status` is clean apart from the untracked scratch test.

What is left after the integrity fix is the shim's own cost: **~2x the old engine per byte,
in both release and debug**, i.e. ~90 µs of feed + ~75 µs of snapshot per 4 KiB chunk. That
is the full-viewport legacy-cell rebuild `US-0082` removes, it is well inside a frame
budget, and it is not a user-visible regression.

App-level frame times (`type` of a 10 MB file, `yes` for 10 s, old binary vs new) could not
be re-measured: the desktop session is disconnected (§7), and the packet's own numbers are
consistent with the engine-level measurements above.

## 7. GUI walks — not reproducible here

`quser` reports session 1 in state **`Disc`** for the whole review. An instance was started
from this worktree's `target/fast-dev/oneterm.exe` (pid 26448, owner's pid 27376 recorded
first and untouched); it launched and created a window (`hwnd=10225882`), but
`Graphics.CopyFromScreen` fails with *"The handle is invalid"* and `PrintWindow` returns an
all-black bitmap — the standard disconnected-session result. The instance was stopped by
pid; `Get-Process oneterm` afterwards shows only 27376. **No `US-0081-verify-` screenshots
exist, because none could be captured.**

What was verified instead: the implementer's thirteen PNGs were opened and read. They are
genuine and show what the write-up claims — `US-0081-08a-sixel-snake.png` shows the libsixel
snake decoded, drawn at the cursor with the prompt and a later `echo` on the rows below it;
`US-0081-06b-reflow-narrow.png` shows `LINE-1 … LINE-12` each wrapped across two rows in
order with the earlier scrollback reflowed, which is the `KeepViewportTop` path. The
pixel-by-pixel old-vs-new sampler comparison in `US-0081-gui-walk.md` §1-2 is the strongest
piece of evidence in the packet and could not be re-run without building the base binary
(disk budget). The two gaps the packet already declares — IN-0027's font walk and any SSH
walk — remain open; this review cannot close them and **cannot confirm them either**.

## 8. Code quality

- No `unsafe` anywhere in the changed files.
- No `unwrap` / `expect` / `panic!` on a runtime path: the only hits are
  `content.rs:264` and `logging.rs:371-403`, all inside `#[cfg(test)]`.
- `clippy --workspace --all-targets -D warnings` green, so no dead code.
- `engine_shim.rs` reads well: coordinates explained once at the top with the consumer's
  file:line, every conversion in one place, the `NamedColor` match written out member by
  member with the reason, `legacy_mode`'s exclusive-mouse-mode note correct.
- Residual `alacritty_terminal`: `crates/terminal` (value vocabulary + the `cfg(test)`
  legacy module), `crates/terminal-view` (reads the snapshot), `crates/tools` (old engine
  for the corpus), `crates/local-shell` (one test uses `SelectionType`, which is in the
  adapter's public signature) — all justified. **`crates/ssh` is not**: after this change no
  `crates/ssh` code references the crate at all (only three comments), so that manifest line
  is deletable today, not at `US-0087` (Finding 6).

## 9. Packet and DB

Packet: every required section present; the deviations, the eight gaps and every
test disposition are recorded honestly, including the two failed walks and the missing
`vt-diff` run. Harness DB row `US-0081` exists (main checkout `harness.db`, gitignored):
status `implemented`, risk lane `high_risk`, all four proof flags set, `verify_command`
`pwsh scripts/ci-local.ps1`, `last_verified_result` `pass`, evidence and notes consistent
with the packet. Three inaccuracies, listed below.

---

## Findings

### Major

**1. Two `SessionEvent::Output` per chunk where the old path emitted one, and the first one
arrives before the batch's reliable events.**
`crates/terminal/src/backend/osc_router.rs:141` (`VtEvent::Repaint => self.forward(SessionEvent::Output)`)
runs inside `TerminalPump::advance` (`pump.rs:86-88`), and then the read loop's
`finish_batch*` forwards `Output` again (`pump.rs:161-167`). The old listener never emitted
`Event::Wakeup` from the parser at all — the pump was the only source. Verified through the
pump (temporary probe, reverted):

```
VERIFIER events: [Title("t"), Output, Output]
```

This contradicts the packet's own acceptance ("the same `SessionEvent` sequence reaches the
UI in the same order", "delivery ordering … byte-for-byte what it was") and the pump module
doc's promise that "reliable events emitted during the batch are seen before that batch's
`Output`" — the first `Output` now precedes the flush of deferred reliable events. User
impact is small because `Output` is coalescible and lossy (`event_sink.rs:77-92`) and the
second `Output` follows immediately, but it doubles the repaint hints under load and can
show one frame whose content is updated before that batch's title/cwd/agent events land.
**Fix:** drop the `VtEvent::Repaint` arm to `{}` in `OscRouter::drain` (the pump already
owns the repaint hint), or stop passing `repaint` at `finish_batch`. One line either way;
add the assertion to `forwards_title_and_wakeup`'s pump-level sibling.

### Minor

**2. Stale deviation citation in the shim.** `crates/terminal/src/engine_shim.rs` —
`legacy_flags`'s doc says blink and overline "are dropped rather than approximated
(deviation D11, `US-0086`)". D11 was **superseded**: `dispatch-and-modes.md:387` marks it
"*superseded* — now correction C11 (blink and overline stored)" under `US-0076`, and
`IN-0029.md:525` moves it out of `US-0086`. The behaviour is right — the legacy `Flags`
type has no blink or overline bit, so the drop is structural in `TerminalContent` and not a
regression — but the citation points at a retired row. **Fix:** reword to "the legacy
`Flags` has no blink/overline bit; the snapshot cannot carry C11's stored attributes until
`US-0085`".

**3. Five snapshot-level deviations are undeclared** (§3's table): `LINE_WRAP` /
`URGENCY_HINTS` / `ORIGIN` dropped from `TerminalContent.mode`, the hyperlink auto-id
string form, the extra `Output`, the Sixel `total_lines` −1, and narrower-but-sound damage.
None has a consumer, and the C/D deviation tables structurally cannot express them (they
compare `grid.expect` / `state.expect`, not `TerminalContent`). **Fix:** a short
"Snapshot deviations" table in the packet, next to the three scope deviations, with the
"no reader" evidence for each.

**4. Sixel `total_lines` is one smaller than the old engine's** (`oneterm/sixel_basic`:
old 10, new 9, viewport cells identical). Engine-level, not the shim; invisible to the
corpus gate because neither `grid.expect` nor `state.expect` records history depth.
Reaches the user as a scrollbar one row short after an image. **Fix:** raise with
`US-0080`'s owner or record as a gap here.

**5. DA2 now answers `ESC[>0;502;1c` instead of `ESC[>0;2601;1c`** (the version number is
the workspace version now, not the fork's). Undeclared; harmless but it is what programs
read to identify the terminal. **Fix:** one line in the packet or in
`dispatch-and-modes.md:200`.

**6. `crates/ssh/Cargo.toml:20 alacritty_terminal.workspace = true` is now dead.** No
`crates/ssh` source references the crate. The packet says all five manifest lines "become
deletable at `US-0085`" — this one is deletable now. **Fix:** correct the packet's
"Residual `alacritty_terminal`" section (or drop the line, though that is out of scope).

**7. `oneterm-pty = { opt-level = 3 }` rides along.** `Cargo.toml:231`'s comment says "the
engine and the transport the swap moved the hot path onto", but the transport was
`oneterm-pty` before the swap too, so it is an unrelated (harmless, probably beneficial)
build change bundled into this packet. **Fix:** split the comment, or say plainly that it
was added opportunistically while measuring.

**8. "The ten `model.rs` resize tests".** `crates/terminal/src/legacy_resize.rs` holds
**15** `#[test]` functions (13 `keep_viewport_top_*`, 2 `default_grow_*`). The packet and
the DB row both say ten. **Fix:** say fifteen.

**9. The merge commit `4b640eb` carries no trailers** (all six authored commits do). Nit.

**10. Scratch file left behind:** `crates/terminal/tests/us0081_parity.rs` is untracked in
the worktree. It is a genuine old-vs-new differential over the 45 recordings and 35 hand
streams plus a damage-soundness property, a three-thread lock stress and an old/new flood
bench — i.e. most of what gap 8 says `vt-diff` would have done. Either delete it or adopt
it (it cannot survive `US-0087`, which deletes the old engine, so if adopted it should be
scheduled to retire with it).

---

## Commands run (all inside the worktree)

```
pwsh scripts/ci-local.ps1
cargo test -p oneterm-terminal --test us0081_parity -- --nocapture --test-threads 1
cargo test --profile fast-dev -p oneterm-terminal --test us0081_parity -- --ignored --nocapture
cargo test --release -p oneterm-terminal --test us0081_parity flood_bench -- --ignored --nocapture
git cherry-pick -n 8f60fc9 ; cargo test --profile fast-dev … ; git reset --hard HEAD
```

Temporary edits made and reverted: `crates/vt/src/grid/terminal_grid.rs` (integrity gate),
`crates/terminal/src/backend/backend_tests.rs` (event-count probe), plus the cherry-pick.
`git status --porcelain` at the end: `?? crates/terminal/tests/` only.
