# BUG-0061 -- independent verification

Verifier: independent agent, own worktree
(`.claude/worktrees/agent-a245468d8ab813fa6`), branch under test
`fix/adapter-cell-pixels` @ `88e413c3`, base `main` @ `980bf5da`.
Date: 2026-09-16. Nothing committed; nothing pushed; `oneterm.exe` never touched;
`harness.db` opened read-only.

## Verdict

**PASS with two records/doc corrections and one quantified follow-up. No blocker.**

The fix is in the right place, is the smallest shape that works, and the claims about
ordering, the trait surface and the device-pixel unit all hold under inspection and under
tests I wrote myself. Two items should be corrected at or before merge: the E2E criterion's
"about 10 % smaller" is only true at scale ~1.0 and is misleading on a scaled display
(F7), and the packet's `harness.db` snippet contradicts the packet's own status/proof
ticks (F8).

## Findings

### F1 -- Defect reproduced on `main`. PASS (informational)

`git grep -n set_cell_pixels main -- crates/` returns only
`crates/vt/src/terminal/mod.rs:506` (the definition),
`crates/vt/src/terminal/terminal_tests.rs:1000` (the engine's own test), the two
`public-api.*.txt` lines and `crates/vt/docs/guide/08-graphics.md:113`. No embedder call
exists on `main`, so `Terminal::new` leaves `cell_pixels` at `(0, 0)`
(`crates/vt/src/terminal/mod.rs:272`) and the `14` arm of
`crates/vt/src/terminal/dispatch.rs:948-952` multiplies the grid by zero. The branch pins
the before-state in the test itself:
`crates/terminal/src/session.rs:865` asserts `ESC[14t` -> `ESC[4;0;0t` before the call and
`ESC[4;432;720t` after `set_cell_pixels(9, 18)` at 24x80. Root cause as stated.

### F2 -- Ordering: the first prepaint alone is enough. PASS

`crates/terminal-view/src/render/element.rs:106-118` pushes the cell size, and the grid
check that calls `session.resize` is at `:119-125` -- the only `session.resize` caller in
the whole crate (`grep -rn "\.resize(" crates/terminal-view/src`, the other hits are
`Vec::resize`). I did not take the implementer's test on trust and wrote my own, which
draws **exactly one** frame (no `first_frame` helper, an explicit
`assert_eq!(probe.snapshot_calls(), 1)`) and then parses the `CSI 14 t` reply:

- `crates/terminal-view/src/render/element_tests.rs` ::
  `verify_first_prepaint_alone_makes_csi_14_t_non_zero` -- **passed** (kept in my worktree,
  not committed).

Residual, inherent and not a defect of this packet: a child that emits `CSI 14 t` in the
window between the PTY spawn and the first prepaint still sees `0;0`. Real shells take
longer to start than one frame.

### F3 -- Re-sent on a DPI-only and a font-only change. PASS (DPI not executed)

The guard is `state.last_cell_pixels != Some(metrics.device)`, and `metrics` comes from the
one cache whose key (`crates/terminal-view/src/render/state.rs:144-150`) carries `font`,
`font_size`, `line_height_factor`, `cell_width_override` **and** `scale_factor`. A
scale-only change therefore moves `metrics.device` and re-pushes, with no grid change
needed -- which is exactly the case `PtySession::resize` would have dropped
(`crates/terminal/src/session.rs:638-642` early-returns when `needs_resize` is false), so
the "separate method, not a wider resize" rationale is correct on its own terms.

Not executed for the scale axis: the GPUI test window hard-codes `scale_factor() -> 2.0`
(`gpui-pre-0.3.2/src/platform/test/window.rs:243`), so no test can move it. The packet
declares this gap in its own words and drives the same mechanism through the font size
instead. Accepted as an honest gap.

### F4 -- Not under one lock with `resize`. LOW, not a blocker

`TerminalModel::set_cell_pixels` (`crates/terminal/src/model.rs:176`) takes `term.lock()`
on its own; `needs_resize` (`:157`) and `resize_grid` (`:168`) each take it again, with
`owner.pty_resize` between them. So between `element.rs:113` and `element.rs:125` a reader
thread can feed `CSI 14 t` and be answered with the new pixels and the old rows (or, on a
later frame, the reverse). The window is two statements of one prepaint, the answer is
self-correcting on the next query, and the resize path already had a three-acquisition
window of the same kind before this packet. Worth a sentence only if someone wants the pair
atomic; making it atomic would need a combined engine-lock hop.

### F5 -- The reply is in device pixels, post-scale and snapped. PASS

`CellMetrics::device` is filled from `snap_device(raw, scale)`
(`crates/terminal-view/src/render/metrics.rs:45-60`, helper at `:72-77`), i.e.
`(logical * scale).round().max(1)` -- the same whole-device-pixel value the quad edges and
the shape rasteriser use, clamped to at least 1 on each axis, and clamped again into `u16`
at `element.rs:114-117`. The 1.25 / 1.5 scale case the brief asked for is already pinned by
an existing test, `crates/terminal-view/src/render/metrics.rs` ::
`metrics_snap_cell_to_device_pixels` (scales 1.0, 1.25, 1.5, 2.0; asserts
`device == (logical*scale).round()` and pins `device == (11, 25)` at 1.5). Device pixels is
the unit Sixel consumers expect. Correct.

### F6 -- Every `TerminalInput` impl compiles. PASS

`grep -rn "impl.*TerminalInput for" crates/` returns exactly two:
`crates/terminal/src/session.rs:615` (`PtySession<O>`) and
`crates/terminal/src/test_support.rs:603` (`FakeTerminalSession`). Both implement the new
method; the fake mirrors it into its own real `oneterm_vt::Terminal`, which is what makes
the view-level reply assertable. `TerminalSession` has `TerminalInput` as a supertrait
(`session.rs:405-406`), so the doc's `TerminalSession::set_cell_pixels` spelling is right.
No third implementor, no `Box<dyn TerminalInput>` shim left behind.

### F7 -- "about 10 % small" is scale-dependent; on a scaled display the image is LARGER. MEDIUM (doc / E2E criterion)

The painter draws an image at
`m.cell_width * (w / 10)` by `m.line_height * (h / 20)`
(`crates/terminal-view/src/render/element.rs:374-378`), and `m.cell_width` /
`m.line_height` are **logical** pixels. So, measured in the device pixels that `CSI 14 t`
now reports, the size error per axis is

```
factor = device_cell / VIRTUAL_CELL        (VIRTUAL_CELL = (10, 20))
```

- device cell `(9, 18)`  (scale 1.0)  -> `0.90 x 0.90`  -- the packet's "10 % small". True.
- device cell `(11, 23)` (scale 1.25) -> `1.10 x 1.15`  -- about 10-15 % **too large**.
- device cell `(18, 36)` (scale 2.0)  -> `1.80 x 1.80`  -- **80 % too large**.

A DPI-aware program sizes its image from the device-pixel answer, so the error tracks the
scale factor. For the owner's 384x576 WEBP:

| | cols | rows |
|---|---|---|
| engine allocates (`ceil(px / VIRTUAL_CELL)`, `placement.rs:45-46`) | 39 | 29 |
| an app using a real `(9, 18)` device cell expects | 43 | 32 |
| an app using a real `(18, 36)` device cell expects | 22 | 16 |

The drawn quad and the reserved cells both derive from `VIRTUAL_CELL`, so they always agree:
the image never overlaps neighbouring text, the aspect ratio is preserved whenever
`cell_h == 2 * cell_w` (a 9x19 cell stretches 5 %), and the dragon stays plainly
recognisable. **Not a blocker**; the follow-up the packet already records is the right fix.

What should change now is the E2E criterion, step 4. As written ("the image may appear
about 10 % smaller ... a mosaic is a failure; a slightly small picture is this packet's
success") it will mislead the owner on any display above 100 % scaling, where the dragon
will be conspicuously **bigger** than in Windows Terminal -- and that is still a pass.
Suggested wording: "the size may be off by `device_cell / (10, 20)` in each axis, larger or
smaller depending on display scaling; only a block mosaic is a failure."

### F8 -- The `harness.db` snippet contradicts the packet's own ticks. LOW (records)

Schema check (read-only against the repository `harness.db`): the `story` table has exactly
the 17 columns the snippet names, in that order -- `id, title, created_at, risk_lane,
contract_doc, packet_doc, status, unit_proof, integration_proof, e2e_proof, platform_proof,
evidence, verify_command, last_verified_at, last_verified_result, notes, intake_id`. Leaving
`intake_id` for the coordinator is fine, as is the ready `verify_command`.

But the row's values are the packet's *opening* state, not its closing one: `status="planned"`,
`unit_proof=0`, `integration_proof=0`, `evidence=None`, `last_verified_at=None`, while the
packet's own blocks tick Implemented, Unit proof, Integration proof and Verify command
passed. Inserted verbatim the database would say "planned, unproven" about a packet that
documents two passing proofs and a green `-Full` gate. The coordinator should insert
`status="implemented"`, `unit_proof=1`, `integration_proof=1`, `e2e_proof=0`,
`platform_proof=0`. `e2e_proof=0` is correct and deliberate.

### F9 -- One reviewed doc is now stale in the literal reading. LOW

`docs/spec-intakes/IN-0029-vt-engine/low-level-design/graphics.md:71` still reads "The
engine never learns the real font cell size." After this packet the engine does learn it --
it simply does not use it for *placement*, which is the sentence's intent. The packet lists
this file as reviewed with an explicit "still correct" no-change reason, which is now half
right. A five-word qualifier ("never learns it *for placement*") would settle it; it can
also ride along with the placement-scale follow-up that will delete the sentence anyway.

`docs/terminal-backend.md:293-303` (the changed section 5.3) is accurate, names the right
types and the right order, and states plainly that the reported size is a report and not a
placement input. Good as written.

### F10 -- The flake claim is plausible. PASS (informational)

`handle::tests::a_pump_yields_to_the_demand_within_a_bounded_number_of_chunks` lives in
`crates/terminal/src/handle.rs`, which this branch does not touch (`git diff --stat
main...HEAD` lists only `model.rs`, `session.rs`, `test_support.rs`, `element.rs`,
`element_tests.rs`, `state.rs`, the packet and `docs/terminal-backend.md`). A lock-fairness
timing assertion under a loaded machine is a credible flake, not a regression from this
change. It did not reproduce in my runs.

## Commands run

All from the verifier's worktree, `CARGO_BUILD_JOBS=2`.

| Command | Result |
|---|---|
| `git reset --hard fix/adapter-cell-pixels` | `88e413c3`, as specified |
| `git grep -n set_cell_pixels main -- crates/` | no embedder caller on `main` (F1) |
| `grep -rn "impl.*TerminalInput for" crates/` | two impls, both updated (F6) |
| `cargo test -p oneterm-terminal cell_pixels` | 1 passed |
| `cargo test -p oneterm-terminal-view` | 306 passed, 0 failed, 3 ignored |
| `cargo test -p oneterm-terminal-view verify_first_prepaint` | 1 passed (my own test, F2) |
| `cargo fmt --all -- --check` | clean, including my added test |
| `pwsh scripts/ci-local.ps1 -Full` | see below |

`ci-local.ps1 -Full` covers the per-crate gates the brief lists separately
(`cargo test --workspace`, `cargo test -p oneterm-vt` and its feature runs,
`scripts/check-english.py`, `scripts/check-doc-paths.py`, the dependency-graph and
public-API checks, `cargo deny`).

`ci-local.ps1 -Full` result: **green**, `ci exit=0`, final line `ci-local: all checks
passed.`, with `advisories ok, bans ok, licenses ok` from `cargo deny`. Every step ran with
my extra test present and my extra evidence file in the tree, so `cargo fmt --check` and
`check-english.py` ("English contributor-text check passed for 906 files") covered them
too. Notable step results: `cargo test --workspace` -- no failure anywhere in the log
(`grep -c "test result: FAILED"` -> 0); the `handle.rs` pump test the packet reports as a
flake did **not** reproduce; `check-doc-paths.py` -- "Doc path check passed for 198 current
paths in 11 documents"; `verify-dependency-graph.py`, `vt-public-api.py --check` /
`--diff-platforms`, `completion-catalog.py validate` and `third-party-notices.py --check`
all passed. Private log:
`<scratchpad>/ci-full.log` (not in the repository).

## Not verified

- **E2E.** No real program was run; `oneterm.exe` is off limits to this verifier by
  standing instruction, and the owner's OpenTUI run after merge remains the only proof that
  a Sixel actually appears. The packet's `e2e_proof 0` is honest.
- **A real DPI-scale change.** The test platform window pins `scale_factor` at 2.0
  (F3); the scale path is verified by reading the cache key and by the existing
  `metrics_snap_cell_to_device_pixels` unit test, not by moving a real display's scaling.
- **The size error at a real 125 %/150 % display (F7)** is arithmetic from the source, not
  a measurement.
- **`harness.db` was not written**, by instruction; only its schema was read.
