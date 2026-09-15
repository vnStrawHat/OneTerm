# Work: close the eight known conformance gaps, and get an esctest report

ID: US-0102
Intake: IN-0038
Created: 2026-09-15

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
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

- [x] In scope: the eight items below, their tests, `docs/osc-sequences-checklist.md`, and one
  `esctest` run whose output is attached to this packet.
- [x] Out of scope: **answering** `DECRQSS` and `XTGETTCAP`. `BUG-0058` stops them being
  mis-routed; replying to them is a larger job (a settings-report formatter and a terminfo
  capability table) and deserves its own packet.
- [x] Out of scope: an `esctest` CI job. The run is manual and its result is evidence, per
  `IN-0029`'s own open question about whether the CI minutes are worth it.
- [x] Out of scope: OSC 777, OSC 1337, the Kitty graphics protocol, and the Kitty keyboard protocol's
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

- [x] **1.** `CSI ? 9 h`, then a press: exactly one `CSI M` report. A release and a motion produce
  **none**. With `? 9 l`, a press produces none.
- [x] **2.** `CSI ? 1015 h`, then a press at column 300, row 300: the report is decimal
  `CSI 32 ; 300 ; 300 M` with no byte above 127. **Corrected while implementing**: this line said
  `CSI 0 ; ...`, which is wrong. `Cb` in urxvt mode is "the same as in normal mode", and normal
  mode's button byte already carries the `+ 32` offset, so a plain left press is 32 and not 0.
  xterm emits `button + 32` here. The same press under `? 1005` and under `? 1006`
  still produces what it produces today (regression).
- [x] **3.** `CSI ? 5 h` sets `ModeSnapshot`'s reverse-video flag and **no cell's style changes**
  (asserted by comparing every cell before and after). `CSI ? 5 l` clears it. `DECRQM` on `? 5`
  reports the right state.
- [x] **4.** `ESC * B` then `ESC n` prints from `G2`; `ESC + 0` then `ESC O` prints exactly one
  line-drawing character and the next character comes from the locking set. `DECSC` / `DECRC` do not
  save the locking set (existing reference behaviour, asserted as a regression).
- [x] **5.** `OSC 1;icon ST` emits exactly one `VtEvent::IconName("icon")` and does **not** change
  the title. `OSC 0;both ST` still sets both title and icon name, as xterm does. **Landed in
  `US-0098`**, not here: the OSC routing table shipped with `1` in `OscRoutes::BUILTIN` and an arm
  in `osc_builtin`, and `terminal_tests::osc_1_reports_the_icon_name` already covers it. Nothing
  was added for this item and nothing was found missing.
- [x] **6.** With `? 2027` set, feeding a family emoji ZWJ sequence advances the cursor by the
  cluster width, not by the sum of scalar widths; with it clear, today's behaviour is unchanged.
  `width::cluster_width` gains its first caller and the "no caller yet by design" comment in
  `crates/vt/src/lib.rs` is deleted.
- [x] **7.** `OSC 17;rgb:ff/00/00 ST` sets the selection background; `OSC 17;? ST` emits a
  `VtEvent::ColorQuery` with the matching `ColorKey`, terminated the way the question was. `OSC 19`
  likewise for the foreground.
- [x] **8.** `CSI = c` replies `DCS ! | 00000000 ST` (or the chosen unit ID), and `CSI = 1 c` is
  unhandled and counted. `CSI c` (DA1) and `CSI > c` (DA2) reply exactly as before (regression).
- [x] The 46 frozen parity corpus recordings replay byte-identically. **None** of these eight
  sequences appears in the corpus, so any diff means an item changed behaviour it should not have.
- [x] `cargo test --workspace` and `cargo test -p oneterm-vt --features vt-paranoid` green.
- [ ] An `esctest` run on Linux is attached to Evidence as a pass/fail count per test group, with the
  count **before** this packet and after. No threshold is enforced. **Not met, and not by the
  platform**: `esctest` reads the screen back with `DECRQCRA` (`CSI * y`), which this engine does
  not implement, so every rectangle assertion would fail on a timeout and both counts would be
  approximately zero. Full reasoning, and what it would take, in
  [`evidence/US-0102-esctest.md`](evidence/US-0102-esctest.md).
- [x] `docs/osc-sequences-checklist.md` lists OSC 1, 17 and 19 with their new status and the file
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

Updated:

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md` — the ESC table
  gains `LS2` / `LS3` / `SS2` / `SS3`; the mode table gains `? 5`, `? 9` and `? 1015` and `? 2027`
  becomes real rather than `NotSupported`; the Answers table gains DA3; the never-`Set` paragraph
  and the deviation register drop `? 2027`; the verification list names the seven new tests.
- `docs/osc-sequences-checklist.md` — Group C marks OSC 17 and 19 implemented with the file that
  implements them, and the two summary tables and the adapter notes follow.
- `crates/vt/CHANGELOG.md` — `Unreleased`, one bullet per item under `Added`, plus the breaking
  enum and `ModeSnapshot` changes and the `DECRQM` reply-contract change under `Changed`.
- `crates/vt/public-api.windows.txt` and `public-api.unix.txt` — eight added lines each; the two
  still differ only by the six `oneterm_vt::pty` lines.

Two documents outside the planned set were **found stale by this work** and corrected rather than
left to mislead:

- `.../low-level-design/cell-and-style.md` said the 2027 print path was deferred whole (R-56).
- `.../low-level-design/pty.md` promised that "the engine will refuse `CSI ? 2027 h` on a session
  whose transport was spawned with `WcsWidth`". No such refusal exists or was added — the engine
  compiles with no transport at all and cannot see the spawn flag. The paragraph now states the
  gap where the spawn flag is chosen.

`US-0086-deferred-deviations.md` is historical and was **not** rewritten. The deferrals it records
that this packet closes: **R-56** (`? 2027` recognised and inert, `cluster_width` shipped with no
caller) in full, and **D13's neighbours** in the Answers table only to the extent that DA3 was
absent. `? 9001` (R-36) and `? 3` (trap 40) stay deferred and stay inert; `? 45`'s closure was
`US-0086`'s own.

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

- [x] 5 (`OSC 1`), 8 (`DA3`), 7 (`OSC 17` / `19`) -- match arms.
- [x] 3 (`DECSCNM`), 1 (`? 9`), 2 (`? 1015`) -- mode plus snapshot flag plus encoder.
- [x] 4 (locking and single shifts) -- needs the new `State` field.
- [x] 6 (`? 2027` wiring) -- the print path; last, because it is the only one that can affect
  throughput.
- [x] `dispatch-and-modes.md` and `osc-sequences-checklist.md`.
- [ ] The `esctest` run and its before-and-after counts — blocked on `DECRQCRA`, see Evidence.
- [x] CHANGELOG lines under `Unreleased` / `Added`.

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
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Per item

| # | Test | Bytes fed | Asserted |
| --- | --- | --- | --- |
| 1 | `input::mouse::tests::x10_reports_a_press_and_nothing_else` | a press, a release, a hover, a drag and a wheel under `MouseReporting::X10` | the press is `ESC [ M SP ! !`; the other four encode to an **empty** `Vec`; a control-click is byte-identical to a plain click |
| 1 | `terminal::tests::mouse_modes_9_and_1015_reach_the_snapshot` | `CSI ? 9 h`, `CSI ? 9 $p`, `CSI ? 1000 h` | the snapshot reports `X10` with the default encoding, `DECRQM` answers `1`, and `? 1000` replaces it so `? 9` then answers `2` |
| 2 | `input::mouse::tests::urxvt_is_decimal_and_has_no_column_ceiling` | a press at row 299 / column 299, a release, a shifted right press, a hover | `ESC [ 32 ; 300 ; 300 M`, every byte below `0x80`; release is the fixed button 3 (`35`); modifiers ride in the value (`38`); motion keeps both 32s (`67`). The same press under `? 1005` and `? 1006` is unchanged |
| 3 | `terminal::tests::decscnm_is_a_screen_flag_and_touches_no_cell` | `CSI 31 m red CSI 0 m plain`, then `CSI ? 5 h` | `ModeSnapshot::reverse_video` flips, all ten cells and all ten styles compare equal before and after, `DECRQM` answers `1` then `2`, and `CSI ? 5 W` (DECST8C) does not toggle it |
| 4 | `terminal::tests::locking_and_single_shifts_reach_g2_and_g3` | `ESC * 0 ESC n q q`, `ESC + 0 ESC o q ...`, `ESC + 0 ESC O q q`, `ESC * 0 ESC N ESC [ 1 m q ESC [ 0 m q`, `ESC * 0 ESC 7 ESC n ESC 8 q`, `ESC * 0 ESC n ESC N ESC c q` | LS2 and LS3 lock; a single shift covers exactly one character and survives an intervening SGR; `DECSC` / `DECRC` do not save the locking set; `RIS` clears both |
| 5 | `terminal::tests::osc_1_reports_the_icon_name` | `OSC 1 ; icon ST` | already green on `main`; landed in `US-0098` |
| 6 | `terminal::tests::mode_2027_measures_grapheme_clusters` | a ZWJ family, with and without `CSI ? 2027 h`; then a regional-indicator flag and `e` + U+0301 | the cursor advances 8 columns reset and 2 set; `DECRQM` answers `1` then `2`; the flag is one wide cluster |
| 7 | `terminal::tests::osc_17_and_19_set_and_query_the_selection_colours` | `OSC 17 ; rgb:ff/00/00 BEL`, `OSC 19 ; #0000ff BEL`, `OSC 17 ; ? BEL`, `OSC 19 ; ? ST`, `OSC 17 ; #010101 ; #020202 BEL` | both colours store; both queries emit `VtEvent::ColorQuery` with the right `ColorKey` and the terminator they were asked with; `query_prefix` is `17` / `19`; a second parameter is counted |
| 8 | `terminal::tests::da3_answers_a_decrptui_unit_id` | `CSI = c`, `CSI = 1 c`, `CSI c`, `CSI > c` | `DCS ! \| 00000000 ST`; `CSI = 1 c` replies nothing and is counted; DA1 and DA2 are byte-identical to before |

**The `DA3` unit id chosen is `00000000`** — xterm's answer for a terminal with no manufacturing
site and no serial number, which a software terminal has not got. Deriving it from
`Config::product_name` was rejected: it would make the reply a per-embedder fingerprint, and no
program that asks does anything with the digits beyond checking that an answer arrived.

### Corpus

`cargo test -p oneterm-tools --test corpus_check` green: both
`the_engine_matches_the_frozen_oneterm_expectations` and
`the_engine_matches_the_frozen_alacritty_expectations` pass, so all 46 recordings replay
byte-identically. None of the eight sequences appears in the corpus, which is what makes that a
real signal rather than a tautology.

### Print bench

`vt-bench grid --mib 32`, interleaved with `main` three times because a single pair on this host
was unreadable — repeat runs of the *same* binary varied by up to 25 per cent depending on how
busy the machine was. Interleaving removes that.

The first honest measurement found a **real ten per cent regression** on `plain_ascii`, `cjk_wide`
and `dense_cells` — fixtures that never set `? 2027`. A three-way bisect (`main`, the charset-shift
commit, the 2027 commit) put all of it in the 2027 commit and none in the shifts. Two structural
causes, both fixed in `perf(vt): keep the mode 2027 wiring off the per-character path`:

1. `Screen::print` became a wrapper and LLVM stopped inlining the callee, putting a call frame back
   on the per-character path. `inline(always)` on `print_with_width`.
2. The cluster loop sat in `print_str`, which is inlined into the parser's ground state.
   `inline(never)` on a `print_clusters` of its own.

Final, median of three interleaved cycles, MiB/s:

| Fixture | `main` | this branch |
| --- | ---: | ---: |
| `plain_ascii` | 76.6 | 77.5 |
| `cjk_wide` | 109.4 | 119.9 |
| `dense_cells` | 191.0 | 197.2 |

Within noise in both directions; nothing above the packet's 5 per cent threshold.

### `esctest`

Not run, and the blocker is not the platform. See
[`evidence/US-0102-esctest.md`](evidence/US-0102-esctest.md): `esctest` reads the screen back with
`DECRQCRA` (`CSI * y`), which this engine does not implement, so a run would time out on every
rectangle assertion and report approximately zero both before and after.

### Gaps carried forward

- `DECRQCRA` (`CSI * y`) is unimplemented, which is what blocks `esctest` — a packet of its own,
  and a careful one: it is a screen-readback primitive that xterm gates behind `allowWindowOps`.
- **`? 2027` and ConPTY do not agree.** The engine accepts the mode on any session, including one
  whose pseudo-console was spawned with `PSEUDOCONSOLE_GLYPH_WIDTH_WCSWIDTH`; if a program sets it
  there, conhost measures scalars while the engine measures clusters and columns drift on a ZWJ
  sequence. The engine cannot check, because it compiles with no transport at all. Nothing OneTerm
  ships sets the mode and no recording in the corpus does. Recorded in `pty.md` where the spawn
  flag is chosen.
- No `OSC 117` / `119` reset for the two selection colours; `RIS` is the only thing that clears
  them. They are not in this packet's scope and no program was found to need them.
- `DECRQSS` and `XTGETTCAP` are still unanswered (from `BUG-0058`); OSC 777, OSC 1337, the Kitty
  graphics protocol and the Kitty keyboard encoder are unimplemented; `esctest` is not in CI.
- **E2E not done.** The manual Windows walk in the Verification Plan (`htop` under `? 9` and
  `? 1015`, a `DECSCNM` program, a `G2` line-drawing TUI, an emoji file under `? 2027`) was not
  performed; every claim above rests on byte-feed tests and the corpus.

## Handoff

Each item is its own commit and its own stop condition. A session that lands items 5, 8 and 7 and
stops has left the tree in a shippable state.
