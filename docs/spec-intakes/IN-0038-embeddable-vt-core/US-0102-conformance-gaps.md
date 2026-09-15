# Work: close the eight known conformance gaps, and get an esctest report

ID: US-0102
Intake: IN-0038
Created: 2026-09-15

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability (eight small ones against published terminal specifications)
- Risk lane: normal
- Spec Intake: `IN-0038`

## Outcome

Eight sequences a published terminal core is expected to understand, and this one does not, are
implemented: mouse modes `? 9` and `? 1015`, `DECSCNM` (`? 5`), the `LS2` / `LS3` / `SS2` / `SS3`
locking and single shifts, `OSC 1`, mode `? 2027`'s wiring to the grapheme-cluster width function
that already exists, `OSC 17` / `OSC 19`, and `DA3`. A first `esctest` run is recorded as a report,
never as a gate.

These are grouped into one packet because each is a handful of lines against a specification, they
share one test file and one documentation update, and splitting them would be the
trivially-small-packet failure `docs/HARNESS.md` warns against. They are **not** grouped because
they are related: they are not.

## Scope

- [ ] In scope: the eight items below, their tests, `docs/osc-sequences-checklist.md`, and one
  `esctest` run whose output is attached to this packet.
- [ ] Out of scope: **answering** `DECRQSS` and `XTGETTCAP`. `BUG-0058` stops them being
  mis-routed; replying to them is a larger job (a settings-report formatter and a terminfo
  capability table) and deserves its own packet.
- [ ] Out of scope: an `esctest` CI job. The run is manual and its result is evidence, per
  `IN-0029`'s own open question about whether the CI minutes are worth it.
- [ ] Out of scope: OSC 777, OSC 1337, the Kitty graphics protocol, and the Kitty keyboard protocol's
  encoder wiring.

## The eight items

| # | Sequence | What it must do | Where |
| --- | --- | --- | --- |
| 1 | `? 9` (X10 mouse) | report a button press only, at `CSI M Cb Cx Cy`, never a release or a motion | `Mode::from_private`, `MouseProtocol::X10`, `ModeSnapshot::mouse`, and `input::mouse` |
| 2 | `? 1015` (urxvt mouse) | encode as `CSI Cb ; Cx ; Cy M`, decimal, no 223-column ceiling | `MouseEncoding::Urxvt`, `input::mouse` |
| 3 | `? 5` `DECSCNM` | swap the default foreground and background for the whole screen while set, without touching any cell | a `Mode` variant, a `ModeSnapshot` flag, and the palette resolution the embedder already does |
| 4 | `LS2` `ESC n`, `LS3` `ESC o`, `SS2` `ESC N`, `SS3` `ESC O` | select `G2` / `G3` as the locking set, or for exactly one printed character | `Handler::esc`, `State::active_charset`, plus a single-shift field |
| 5 | `OSC 1` | set the icon name; emit `VtEvent::IconName` | the OSC built-in match (`US-0098`'s table) |
| 6 | `? 2027` | when set, printing uses `width::cluster_width` (grapheme clusters) instead of `scalar_width` | `PrintMode` and the print path |
| 7 | `OSC 17` / `OSC 19` | set and query the selection background and foreground, as OSC 10/11 do | `ColorKey`, `osc_dynamic_color`, `query_color` |
| 8 | `DA3` `CSI = c` | reply with a DECRPTUI unit ID, `DCS ! | <8 hex digits> ST` | `identify_terminal` |

## Acceptance

Each item is accepted only with a test that feeds the bytes and asserts the observable result.

- [ ] **1.** `CSI ? 9 h`, then a press: exactly one `CSI M` report. A release and a motion produce
  **none**. With `? 9 l`, a press produces none.
- [ ] **2.** `CSI ? 1015 h`, then a press at column 300, row 300: the report is decimal
  `CSI 0 ; 300 ; 300 M` with no byte above 127. The same press under `? 1005` and under `? 1006`
  still produces what it produces today (regression).
- [ ] **3.** `CSI ? 5 h` sets `ModeSnapshot`'s reverse-video flag and **no cell's style changes**
  (asserted by comparing every cell before and after). `CSI ? 5 l` clears it. `DECRQM` on `? 5`
  reports the right state.
- [ ] **4.** `ESC * B` then `ESC n` prints from `G2`; `ESC + 0` then `ESC O` prints exactly one
  line-drawing character and the next character comes from the locking set. `DECSC` / `DECRC` do not
  save the locking set (existing reference behaviour, asserted as a regression).
- [ ] **5.** `OSC 1;icon ST` emits exactly one `VtEvent::IconName("icon")` and does **not** change
  the title. `OSC 0;both ST` still sets both title and icon name, as xterm does.
- [ ] **6.** With `? 2027` set, feeding a family emoji ZWJ sequence advances the cursor by the
  cluster width, not by the sum of scalar widths; with it clear, today's behaviour is unchanged.
  `width::cluster_width` gains its first caller and the "no caller yet by design" comment in
  `crates/vt/src/lib.rs` is deleted.
- [ ] **7.** `OSC 17;rgb:ff/00/00 ST` sets the selection background; `OSC 17;? ST` emits a
  `VtEvent::ColorQuery` with the matching `ColorKey`, terminated the way the question was. `OSC 19`
  likewise for the foreground.
- [ ] **8.** `CSI = c` replies `DCS ! | 00000000 ST` (or the chosen unit ID), and `CSI = 1 c` is
  unhandled and counted. `CSI c` (DA1) and `CSI > c` (DA2) reply exactly as before (regression).
- [ ] The 46 frozen parity corpus recordings replay byte-identically. **None** of these eight
  sequences appears in the corpus, so any diff means an item changed behaviour it should not have.
- [ ] `cargo test --workspace` and `cargo test -p oneterm-vt --features vt-paranoid` green.
- [ ] An `esctest` run on Linux is attached to Evidence as a pass/fail count per test group, with the
  count **before** this packet and after. No threshold is enforced.
- [ ] `docs/osc-sequences-checklist.md` lists OSC 1, 17 and 19 with their new status and the file
  that implements them.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md` -- the mode table,
  the charset rules, the DA replies, and the deviation and correction registers (`D*`, `C*`, `R-*`).
  Several of these gaps are recorded there as deferred.
- `docs/spec-intakes/IN-0029-vt-engine/US-0086-deferred-deviations.md` -- the packet that deferred
  them. It is the record of what was knowingly left out and must be cross-checked so this packet
  closes the right ones and does not silently reopen a deliberate deviation.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` -- the corpus contract,
  the five bench tiers, and the note that `esctest` is GPL-2.0, Linux-only and not vendored.
- `docs/osc-sequences-checklist.md` -- the OSC status table.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/cell-and-style.md` -- for item 3, to confirm
  reverse video is a screen-level flag and not a per-cell attribute.

### Documentation Action

**Update required**: `dispatch-and-modes.md` (the mode table gains `? 5`, `? 9`, `? 1015`, `? 2027`
becomes live, the charset section gains the shifts, the DA section gains DA3) and
`docs/osc-sequences-checklist.md` (OSC 1, 17, 19). `US-0086-deferred-deviations.md` is a historical
packet and is **not** rewritten; instead this packet's Evidence names which of its deferrals are now
closed.

Reason: `dispatch-and-modes.md` is the specification the engine is written against, and eight rows
of it become wrong.

### Reconciliation

Before completion, list both documents and name the `US-0086` deferrals closed.

## Context

Each gap was verified absent on `main` @ `36977ca`; the evidence per item is in the intake's "Known
conformance gaps" table, with file and line. The two that are more than a match arm:

- **Item 4** needs a single-shift field on `State` that is consumed by exactly one printed character
  and then cleared, including across an intervening escape sequence. `State::preceding_char` (which
  `REP` uses and which survives intervening sequences, trap 43) is the precedent for where it lives
  and how it is tested.
- **Item 6** is a print-path change. `width::cluster_width` is already implemented and tested and
  has no caller; `crates/vt/src/lib.rs` says so explicitly, which is why this was left as a wiring
  job rather than an implementation one. The cost is that the print path must segment on grapheme
  clusters when the mode is set, which is a `unicode-segmentation` call the crate already depends on.

## Plan

One commit per item, in this order -- cheapest and most isolated first, so a session boundary is
never mid-item:

- [ ] 5 (`OSC 1`), 8 (`DA3`), 7 (`OSC 17` / `19`) -- match arms.
- [ ] 3 (`DECSCNM`), 1 (`? 9`), 2 (`? 1015`) -- mode plus snapshot flag plus encoder.
- [ ] 4 (locking and single shifts) -- needs the new `State` field.
- [ ] 6 (`? 2027` wiring) -- the print path; last, because it is the only one that can affect
  throughput.
- [ ] `dispatch-and-modes.md` and `osc-sequences-checklist.md`.
- [ ] The `esctest` run and its before-and-after counts.
- [ ] CHANGELOG lines under `Unreleased` / `Added`.

## Decisions

None. Each item implements a published specification (xterm's control sequences document, ECMA-48,
the `? 2027` mode proposal); there is no choice future work inherits. Where a specification is
ambiguous -- the `DA3` unit ID in particular -- the packet records the value chosen in Evidence
rather than opening a decision record for eight hex digits.

## Verification Plan

- Focused: one test per item, listed in Acceptance, in
  `crates/vt/src/terminal/terminal_tests.rs` and `crates/vt/src/input/mouse_tests.rs`.
- Unit: `cargo test -p oneterm-vt`, `--features vt-paranoid`, `cargo test --workspace`.
- Integration: the parity corpus replay, which must be byte-identical.
- Performance: item 6 touches the print path, so the existing render and parser benches are run
  before and after and the numbers recorded. A regression above 5 percent on the print bench is a
  reason to reconsider how the mode is checked, not a reason to skip the item.
- Platform: `pwsh scripts/ci-local.ps1`; the Linux `esctest` run, report only.
- E2E: manual Windows walk -- `htop` under `? 9` and under `? 1015` (forced with `printf`), a
  full-screen program that sets `DECSCNM`, a line-drawing TUI that uses `G2`, and an emoji-heavy
  file `cat`-ed with `? 2027` set.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Record per item: the test name, the bytes fed, and the asserted result. Then: the corpus replay
result; the print-bench numbers before and after; the `esctest` counts before and after; the `DA3`
unit ID chosen; and which `US-0086` deferrals are now closed.

Known gaps carried forward after this packet: `DECRQSS` and `XTGETTCAP` are still unanswered (from
`BUG-0058`); OSC 777, OSC 1337, the Kitty graphics protocol and the Kitty keyboard encoder are
unimplemented; `esctest` is not in CI.

## Handoff

Each item is its own commit and its own stop condition. A session that lands items 5, 8 and 7 and
stops has left the tree in a shippable state.
