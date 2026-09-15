# Independent verification: US-0102, the eight conformance gaps

ID: US-0102
Intake: IN-0038
Verified: 2026-09-15
Branch: `feat/vt-conformance-gaps` @ `ef9f7c32` (base `main` @ `98a72148`)
Verifier: independent worktree, adversarial, own tests only

## Verdict

**PASS-WITH-NOTES.**

All eight items do what the packet says they do, every reference claim I could check against xterm's
`ctlseqs` and the VT510 manual came out in the implementation's favour (including the one the
implementer *corrected* mid-flight), the parity corpus is unchanged, the performance claim
reproduces, and the `esctest` blocker is real and correctly diagnosed. Eleven findings follow. Two
are genuine functional gaps in the new print path (F1, F11), four are documentation or evidence
statements that are false as written (F3, F4, F5, F6), and the rest are smaller.

Nothing found is a reason to hold the packet on behaviour: every functional finding is reachable only
through `? 2027`, which nothing OneTerm ships sets and no recording exercises, and each of the eight
items satisfies the acceptance line written for it. F1, F11 and F6 should become their own follow-up
packets; F2–F5 and F8 are corrections to text that is already written.

The one thing that comes close to a `FAIL` is not a bug: `cell-and-style.md` recorded "a cross-chunk
pending-cluster buffer" as part of what mode 2027 needs, and this packet **deleted that sentence
while rewriting the paragraph**, without building the buffer and without carrying the gap forward in
the packet's own "Gaps carried forward" list. The project came out of `US-0102` knowing less about
mode 2027's remaining work than it knew going in. That is what F1 is really about; the miscount
itself was always going to be a second packet.

## How this was verified

Own tests, written against the public API before reading `terminal_tests.rs` or `mouse_tests.rs`:

- `crates/vt/tests/verify_us0102.rs` — 57 tests, all green on `ef9f7c32`.
- `crates/vt/tests/verify_us0102_probe.rs` — diagnostics that produced the split-point tables below.

Both files are in the verifier's worktree and are **not committed**.

References consulted: xterm `ctlseqs` (X10 mouse, `Ps = 1015`, tertiary DA), the VT510 manual
(DECSC, DECSTR, DECRQCRA), and the `esctest2` repository.

**The branch moved while this ran.** `feat/vt-conformance-gaps` was rebased onto `af5df2e7` (the
`US-0101` merge) and its tip is now `ffcd3f85`. Diffing `ef9f7c32..ffcd3f85` over every file
`US-0102` touched shows only `US-0101`'s `render` -> `snapshot` rename and its knock-on doc
comments; `crates/vt/src/width.rs` and `crates/vt/src/terminal/dispatch.rs` are byte-identical. Every
finding below therefore still applies at `ffcd3f85`, with the line numbers unchanged for those two
files.

---

## Findings

### F1. `? 2027` miscounts any grapheme cluster split across two `feed` calls — HIGH

`crates/vt/src/terminal/dispatch.rs:970` (`Dispatch::print_str`) segments **only the run it was
handed** and keeps no carry:

```rust
fn print_str(&mut self, text: &str) {
    self.state.dispatched = true;
    if self.state.modes.contains(Mode::GraphemeClusters) {
        self.print_clusters(text);
        return;
    }
```

A pty read can end anywhere, and `crates/vt/src/parser/utf8.rs` hands over one `print_str` per
validated run plus a separate `print_char` for a codepoint carried across the chunk boundary. So a
cluster that straddles two `feed` calls is measured as two clusters.

Measured on `ef9f7c32`, feeding the mode, then the head, then the tail, and reading the cursor
column:

| Sequence | bytes | interior split points | wrong | observed columns |
| --- | ---: | ---: | ---: | --- |
| `U+1F468 ZWJ U+1F469 ZWJ U+1F467 ZWJ U+1F466` (family) | 25 | 24 | **24** | 4 or 6, want 2 |
| `U+1F44D U+1F3FD` (thumbs up + skin tone) | 8 | 7 | **7** | 4, want 2 |
| `1 U+FE0F U+20E3` (keycap) | 7 | 6 | **3** | 3, want 2 |
| `U+1F1E9 U+1F1EA` (flag) | 8 | 7 | 0 | 2 |
| `e U+0301` | 3 | 2 | 0 | 1 |

Flags and combining marks survive by luck: the tail half measures 0 and attaches to the cell the
head already took. A ZWJ or modifier tail measures 2 and takes a cell of its own.

This is not a newly invented requirement. `cell-and-style.md` listed it: *"The rest of 2027 (a
`GraphemeCursor` print path, **a cross-chunk pending-cluster buffer**, and a ConPTY glyph-width
flag …) is deferred"*. `US-0102` rewrote that paragraph to say the mode is live
(`docs/spec-intakes/IN-0029-vt-engine/low-level-design/cell-and-style.md:193`) and deleted the
mention of the buffer without building it and without recording it as a carried-forward gap.

It also makes the packet's Reconciliation claim that `US-0086`'s **R-56** is closed "in full"
an overstatement. `US-0086`'s own one-line form of R-56 ("recognised, inert, `NotSupported`") is
indeed closed; the scope `cell-and-style.md` gave it is not.

Impact today is nil — nothing OneTerm ships sets `? 2027` and no recording does — but `oneterm-vt`
is published as an embeddable core, and an embedder that turns the mode on gets column drift on
ordinary emoji output at ordinary read boundaries.

Tests: `v6_a_cluster_split_across_two_feeds_is_miscounted`,
`v6_skin_tone_and_keycap_splits_are_miscounted` (both assert the **defect**, so they go red when a
carry lands), `v6_a_flag_pair_split_across_two_feeds_is_not_miscounted`,
`v6_a_combining_mark_split_across_two_feeds_is_not_miscounted`.

### F2. The published encoder contract is stated too broadly — MEDIUM

`crates/vt/src/input/mouse.rs:9`:

> An encoder returns an **empty** `Vec` for an event the current mode does not report, so the caller
> writes nothing.

and `crates/vt/CHANGELOG.md`, under `? 9`:

> the encoders return an **empty** `Vec` for an event the mode does not report, so a caller writes
> nothing

and the packet's acceptance line 1:

> With `? 9 l`, a press produces none.

None of the three is true of the encoder. With `ModeSnapshot::mouse == None` — which is exactly what
`? 9 l` leaves behind — `encode_mouse_press` still returns the full six-byte legacy report
`ESC [ M SP ! !`. The X10 branch at `crates/vt/src/input/mouse.rs:98` only fires when the snapshot
*is* X10; with no reporting at all the function falls through to the legacy encoder, as it always
has.

`oneterm-terminal` is safe: `crates/terminal/src/model.rs:268` gates on `modes.mouse.is_some()`
before calling. So this is a contract-documentation defect, not a live bug — but the third clause of
acceptance line 1 is ticked and has no test behind it, and a new embedder reading the published
crate's own words would send mouse reports with mouse reporting off.

Test: `v1_x10_reset_does_not_silence_the_encoder_only_the_caller`.

### F3. `cell-and-style.md` was only half-corrected, and now contradicts `pty.md` — MEDIUM

The packet's Reconciliation says `cell-and-style.md` "said the 2027 print path was deferred whole
(R-56)" and was corrected. The paragraph above the bullets was. Two statements below them were not:

- `.../cell-and-style.md:203` — *"`oneterm-pty` spawns with `PSEUDOCONSOLE_GLYPH_WIDTH_WCSWIDTH`,
  matching the engine **unconditionally**. The column-drift failure mode that would have followed
  from a mid-session mode change **cannot occur, because there is no mode change**."* There is now a
  mode change; that is what this packet shipped. `pty.md`'s new "Open gap, carried by `US-0102`"
  paragraph says the drift **can** occur, in the same intake.
- `.../cell-and-style.md:207` — *"Width today is therefore per scalar … A ZWJ family emoji lands as
  base plus a ZWJ tail and the next emoji starts a new cell — visibly wrong, and exactly what the
  engine does today."* True only with the mode reset.

`dispatch-and-modes.md:186` has the milder form of the same problem: *"The ConPTY glyph-width axis is
unchanged — the pseudo-console is spawned with `PSEUDOCONSOLE_GLYPH_WIDTH_WCSWIDTH` and never
changes mid-session"*. Literally true of the ConPTY flag, and it reads as "so there is no
disagreement", which is the opposite of what `pty.md` now records.

`pty.md` itself is correct and is a genuine improvement: the old text promised a refusal that never
existed, and the new text says so plainly.

### F4. A new deviation from DEC and xterm was introduced and not registered — MEDIUM

The packet asserts, as item 4's regression: *"`DECSC` / `DECRC` do not save the locking set (existing
reference behaviour)"*. Confirmed as behaviour, and confirmed as **alacritty's** behaviour
(`active_charset` lives on `Term`, not on the cursor). It is **not** the reference this engine's
design document holds itself to elsewhere:

- VT510, DECSC: saves "character sets (G0, G1, G2, or G3) currently in GL and GR" and "any single
  shift 2 (SS2) or single shift 3 (SS3) functions sent".
- xterm's `CursorSave` stores `curgl`, `curgr` and `gsets[]`.

So after `US-0102` the engine has two DEC deviations on this axis: the locking-set *invocation* is
not saved, and the pending single shift is not saved either — the second one is new, because before
this packet there was no single shift to lose. `dispatch-and-modes.md:66` still reads "save / restore
cursor, style template and charset designations" and no `D*` / `C*` / `R-*` id was added, against
that document's own rule that *"Every other difference is a defect until a correction id says
otherwise"*.

Tests: `v4_decsc_decrc_do_not_save_the_locking_set_or_the_single_shift` (pins today's behaviour and
names the deviation in its doc comment).

### F5. The corpus claim is false for item 3 — MEDIUM

Packet Evidence, § Corpus:

> **None** of these eight sequences appears in the corpus, which is what makes that a real signal
> rather than a tautology.

`CSI ? 5 h` / `CSI ? 5 l` (DECSCNM, item 3) appears **nine times across six of the 47 recordings**.
Scanned every `recording` file in `crates/tools/corpus`:

| Recording | occurrences |
| --- | --- |
| `vttest_origin_mode_1` | `?5l`, `?5h`, `?5l` |
| `vttest_origin_mode_2` | `?5l`, `?5h`, `?5l` |
| `vttest_scroll` | `?5l`, `?5h`, `?5l` |
| `vttest_cursor_movement_1` | `?5l` |
| `vttest_insert` | `?5l` |
| `vttest_tab_clear_set` | `?5l` |

The other seven sequences (`? 9`, `? 1015`, `? 2027`, `ESC n` / `ESC o` / `ESC N` / `ESC O`,
`OSC 17`, `OSC 19`, `CSI = c`) really are absent — verified by the same scan.

The second half of the sentence does not hold for item 3 either: the `.expect` format is
grid-only (`# OneTerm VT parity expectation: the grid, cell-exact`), so it records no
`ModeSnapshot` and no `FeedStats`. Before this packet `? 5` was an unrecognised private mode; after
it, it sets a flag and bumps the generation. The corpus could not have seen that change in either
direction. The corpus result is still a valid *no-cell-changed* signal for item 3 — it is just not
the signal the sentence claims.

### F6. DECSCNM is inert end to end; the documents describe wiring that does not exist — MEDIUM

`grep -rn reverse_video crates/ --include=*.rs` finds the field's definition, the snapshot
assignment, the engine's own tests and this verifier's tests. **No renderer reads it.** Nothing in
`crates/terminal` or `crates/terminal-view` swaps the two defaults.

The packet describes it otherwise, in three places:

- item 3's "Where" column: *"the palette resolution the embedder already does"*;
- `dispatch-and-modes.md:164`: *"the embedder swaps the two defaults when it resolves the palette"*;
- `CHANGELOG.md`: same sentence.

A program that sets `? 5` on OneTerm today sees no visual change at all. The engine-side half is
correct and is arguably the whole of this packet's scope, but "the embedder swaps" is written in the
present tense about something nobody does. The manual walk in the Verification Plan ("a full-screen
program that sets `DECSCNM`") is the check that would have caught it, and it is recorded as not
performed.

Test: `v9_decscnm_is_engine_only_today`.

### F7. No `## Harness Row` section — LOW

Every other packet in `IN-0038` carries one (`BUG-0058`, `US-0097`, `US-0098`, `US-0099`, `US-0100`,
`US-0104`); `US-0102` and `US-0101` do not. So there is no snippet to check against the `story`
schema, no `*_proof` flags to compare with the packet's own proof block, and no `intake_id`. For the
record, the values the snippet would need are unchanged: `story(...17 columns...)` with the four
`*_proof` columns as `0`/`1`, and `intake_id = 43` is `IN-0038`.

### F8. The `osc-sequences-checklist.md` acceptance line overstates what changed — LOW

Acceptance: *"`docs/osc-sequences-checklist.md` lists OSC 1, 17 and 19 with their new status and the
file that implements them."* The OSC 17 and 19 rows do exactly that. The OSC 1 row
(`docs/osc-sequences-checklist.md:73`) is untouched by this packet, still marked `◐`, and names no
file. Harmless — item 5 landed in `US-0098` — but the box is ticked for work that was not done.

### F9. `? 9` outranks the extended encodings, undocumented — LOW

`crates/vt/src/input/mouse.rs:98` places the X10 branch **before** the encoding branch, so with
`? 9` and `? 1006` both set the report is the legacy six-byte form and the SGR encoding is ignored.
That is a defensible reading (X10 predates the extensions) but it is not stated anywhere, and I did
not find text in `ctlseqs` that settles it — xterm's extended-coordinate handling is applied in the
same emit path as the X10 report, which suggests xterm would use SGR here. Recorded rather than
called wrong.

Test: `v2_x10_ignores_the_extended_encodings` (pins today's answer, and says so).

### F10. `REP` and `? 2027` disagree about what "the preceding character" is — LOW

`crates/vt/src/terminal/dispatch.rs:206` sets `preceding_char` to the cluster's **last** scalar, so
after a ZWJ family `CSI b` repeats the trailing emoji rather than the cluster; and `REP` itself
dispatches through the per-scalar `input()` even while the mode is set. Trap 43's stated contract is
that `preceding_char` is "the raw scalar", which is now ambiguous under the mode. Undocumented.

Test: `v6_rep_after_a_cluster` (records the answer without asserting a reference).

### F11. Under `? 2027` a bare `U+FE0F` takes two columns — MEDIUM

`crates/vt/src/width.rs:57`:

```rust
if cluster.contains(&EMOJI_PRESENTATION) {
    return 2;
}
```

The "an explicit presentation selector decides" rule fires before anything checks that the cluster
has a base. A stray variation selector — VS16 with nothing in front of it — therefore measures **2
columns** and takes a two-cell hole in the row. With the mode reset the same byte is width 0 and
attaches to the previous cell, which is what every other terminal does. VS15 (`U+FE0E`) has the
milder form of the same problem: 1 column instead of 0.

Measured, cursor column after the scalar plus one `a`:

| Input | `? 2027` set | `? 2027` reset |
| --- | ---: | ---: |
| `U+0301` | 1 | 1 |
| `U+200D` | 1 | 1 |
| `U+20E3` | 1 | 1 |
| `U+FE0E` | **2** | 1 |
| `U+FE0F` | **3** | 1 |

A bare selector is rare in well-formed output, but F1 manufactures them: splitting a keycap
sequence in front of its `U+FE0F` leaves `FE0F 20E3` as its own cluster, which is why the keycap row
in F1's table reads 3 rather than 2.

`cluster_width` shipped in `IN-0029` and its unit tests cover VS16 only inside a cluster that has a
base (`crates/vt/src/width_tests.rs:60`, `:79`); no test feeds it alone. `US-0102` is what gave the
function a caller, so this is where the gap becomes reachable. The fix is one guard: a presentation
selector decides the width of a cluster that has a base scalar, and answers 0 otherwise.

Test: `v9_a_leading_zero_width_cluster_takes_columns_only_for_vs16`.

---

## What held up

Everything below was attacked and did not move.

### Item 1 — `? 9`, X10 mouse

- Press only, `CSI M Cb Cx Cy`, `Cb = button - 1 + 32` (32 / 33 / 34 for left / middle / right),
  coordinates `+ 32`. Matches xterm's "X10 compatibility mode" paragraph.
- Modifiers are not encoded: a ctrl+alt+shift press is byte-identical to a plain one.
- Release, drag, hover, wheel-up and wheel-down all encode to an **empty** `Vec`.
- The four reporting modes replace each other on set and only themselves on unset; `? 9` takes part
  in that correctly (`? 9 h` after `? 1003 h` wins; `? 9 l` after `? 1003 h` leaves `? 1003`).
- `DECRQM` answers `1` set and `2` reset, and `? 1000 h` makes `? 9` answer `2`.

### Item 2 — `? 1015`, urxvt mouse

**The implementer's mid-flight correction is right and the packet's original acceptance line was
wrong.** xterm `ctlseqs`, `Ps = 1 0 1 5`: *"the normal mouse response is altered to use `CSI`
followed by semicolon-separated encoded button value, the `Cx` and `Cy` ordinates and final
character `M`. This uses the **same button encoding as X10**, but printing it as a decimal integer
rather than as a single byte."* X10's button byte already carries `+ 32`, so a plain left press is
`32`, not `0`. `ESC [ 32 ; 300 ; 300 M` is correct; `ESC [ 0 ; 300 ; 300 M` would not be.

- Decimal, every byte below `0x80`.
- No 223 ceiling: a press at row 1500 / column 1000 encodes `ESC [ 32 ; 1000 ; 1500 M`, where the
  legacy form saturates both coordinates at 255.
- Semantics unchanged by the transport: release is the fixed button 3 (`35`), shift rides in the
  value (`38` for a shifted right press), hover keeps both offsets (`67`), wheel up is `96`.
- The three encodings replace each other on set and only themselves on unset.
- `? 1005` and `? 1006` produce exactly what they produced before.

### Item 3 — `? 5`, DECSCNM

- Reaches `ModeSnapshot::reverse_video`; every cell and every row's text on a four-row screen is
  byte-identical before and after `? 5 h`, and `? 5 l` restores exactly what was there.
- A `Repaint` is emitted, which is the only signal a renderer has.
- `DECRQM` answers `1` / `2`.
- `RIS` clears it. `DECSTR` (`CSI ! p`) does **not** — and that is spec-correct: DECSCNM is not on
  VT510's DECSTR reset list.
- The alternate screen shares the flag rather than carrying its own copy, which matches xterm, where
  reverse video is a widget-wide attribute.
- `CSI ? 5 W` (DECST8C) does not touch it.

(See F6 for what happens after the snapshot.)

### Item 4 — LS2 / LS3 / SS2 / SS3

- `ESC * 0` + `ESC n` and `ESC + 0` + `ESC o` both print from the designated set; `SI` returns to
  G0.
- `ESC N` / `ESC O` cover exactly one printed character and the next comes from the locking set.
- The shift survives an intervening escape sequence (`ESC N` `CSI 1 m` `q`) **and** an intervening
  C0 control (`ESC N` `BEL` `q`, `ESC N` `CR LF` `q`). Both are right: DEC's rule is "the next
  graphic character", and neither a control nor an escape sequence is one.
- A pending shift does not leak past the `feed` that consumed it.
- `RIS` clears both the locking set and a pending shift.
- `ESC * B` designates ASCII into G2, so `LS2` then prints plain text.

(See F4 for DECSC / DECRC.)

### Item 5 — `OSC 1`

Regression only, as the packet says: one `VtEvent::IconName`, no `VtEvent::Title`.

### Item 6 — `? 2027`

- ZWJ family: 8 columns reset, 2 columns set, and the whole cluster is in the one cell.
- Regional-indicator pair: 2 columns. `e` + U+0301: 1 column.
- `DECRQM` answers `1` / `2` and no longer `NotSupported`.
- A wide cluster at the last column wraps as a unit under DECAWM and does not corrupt the row with
  DECAWM off.
- Setting and then clearing the mode leaves no residue: six mixed fixtures measure exactly what they
  measure with the mode never touched.
- Controls still split the run under the mode.
- Under IRM the cluster inserts as one cell.
- **Bounded and linear.** 10 000 combining marks on one base: 1 column, well under the 2 s guard.
  20 000 / 10 000 wall-clock ratio is far below the 8x quadratic guard. 200 000 marks: 1 column, and
  the cell does **not** keep every mark (the grapheme arena's cap holds).

(See F1 for the chunk boundary.)

### Item 7 — `OSC 17` / `OSC 19`

- `OSC 17;rgb:ff/00/00` and `OSC 19;#0000ff` store into
  `ColorKey::SelectionBackground` / `SelectionForeground`.
- `OSC 17;?` and `OSC 19;?` emit `VtEvent::ColorQuery` with the right key and with the terminator
  the question used (`BEL` answered `BEL`, `ST` answered `ST`).
- `query_prefix()` is `"17"` / `"19"`.
- `RIS` clears both.
- `OSC 117` / `OSC 119` are not implemented: they are counted, they do not clear the colours, and
  they are not in `OscRoutes::BUILTIN`.
- A missing, empty or unparseable payload is counted and stores nothing. A second parameter is
  counted and the first still applies — xterm's advancing form is deliberately not implemented and
  the reason given (advancing from 17 lands on OSC 18, the Tektronix cursor) is correct.

### Item 8 — `DA3`

- `CSI = c` and `CSI = 0 c` both answer `DCS ! | 00000000 ST`. Matches xterm, which reports zeros
  for the site code and the serial number in DECRPTUI.
- `CSI = 1 c` and `CSI = 2 c` answer nothing and are counted — the `Ps == 0` gate is shared with
  DA1 and DA2 at `crates/vt/src/terminal/dispatch.rs:1112`, which is exactly xterm's rule.
- `Config::product_name` does not reach the reply, so the unit id is not a per-embedder
  fingerprint.
- DA1 (`CSI c`, `ESC Z`) and DA2 (`CSI > c`) are unchanged.
- `ESC =` is still DECKPAM and not DA3.

---

## Performance

Reproduced the packet's comparison with interleaved runs of two release binaries built from
`98a72148` and `ef9f7c32` in this worktree, `vt-bench grid --mib 32`, three interleaved cycles
(main, branch, main, branch, main, branch); each run is itself a median of three. Medians of the
three cycle values, MiB/s, higher is better:

| Fixture | `main` | this branch | ratio |
| --- | ---: | ---: | ---: |
| `plain_ascii` | 65.3 | 68.0 | 1.04 |
| `cjk_wide` | 93.0 | 101.9 | 1.10 |
| `dense_cells` | 152.2 | 187.4 | 1.23 |
| `long_lines` | 72.1 | 71.8 | 1.00 |
| `heavy_sgr` | 167.6 | 209.2 | 1.25 |
| `tui_redraw` | 112.8 | 114.2 | 1.01 |
| `scroll_region` | 44.4 | 47.4 | 1.07 |
| `scrolling` | 64.5 | 70.7 | 1.10 |
| `sixel` | 42.8 | 36.3 | 0.85 |
| `osc_9_7` | 90.7 | 92.8 | 1.02 |

**The claim holds: no regression on any fixture, and none of the three the packet names.** The
absolute numbers are lower than the packet's (65 / 93 / 152 against 76.6 / 109.4 / 191.0) because
this host was busier; the *direction* is the same. This machine is too noisy to read anything
finer — per-run spread within one binary reached 50 per cent on `heavy_sgr` and `scrolling`, and
`sixel`'s 0.85 sits inside a 32.6–44.3 range on `main` alone. That noise is itself the packet's
point about needing interleaved runs, and it is why nothing here should be read as a 23 per cent
*improvement* either.

### The inline attributes

- `#[inline(always)]` on `Screen::print_with_width` (`crates/vt/src/grid/screen.rs:1099`) —
  **justified.** The split exists so the `? 2027` path can pass a width in; without the attribute
  LLVM stopped inlining it into `Screen::print` and put a call frame back on the per-character path.
  The comment says "measured, not assumed" and the measurement is the ten per cent the packet
  documents. The cost is that the rare cluster path also carries a full copy of a large function;
  that is the right trade for a per-character path.
- `#[inline(never)]` on `Handler::print_clusters` (`crates/vt/src/terminal/dispatch.rs:196`) —
  **justified**, and the ordinary cold-path idiom: `print_str` is inlined into the parser's ground
  state and must not carry the segmentation machinery.

Neither is a smell on its own. What they *do* signal is that this print path is one refactor away
from a silent ten per cent loss with nothing to catch it: the benches are "recorded, never gated".
That is a standing property of the engine, not a defect introduced here.

---

## `esctest`

The evidence document's blocker is **real and correctly diagnosed**, verified independently:

- `esctest2`'s assertions are `AssertScreenCharsInRectEqual` and relatives, and they read the screen
  back with `DECRQCRA` (`CSI * y`). Confirmed from the `esctest2` repository.
- There is no headless or non-pty mode that would sidestep it. `esctest.py` has `--include`,
  `--expected-terminal`, `--max-vt-level`, `--xterm-checksum`, `--logfile`, `--stop-on-failure` and
  `--test-case-dir`; none of them removes the readback channel.
- `crates/vt/src/terminal/dispatch.rs` really has no `* y` arm — `b'*'` appears once in the file, as
  a charset-designation intermediate at line 1019. So every rectangle assertion would hang to a
  timeout and both the before and after counts would be about zero, exactly as claimed.
- `--include` could run the reply-only subset (`DA`, `DSR`, `DECRQM`, `DECSCUSR`). The evidence's
  judgement that this "would buy a second opinion rather than new coverage" is fair; those replies
  are byte-asserted in the engine's own tests and in mine.

**Not writing the `#[cfg(unix)]` harness was the right call.** It could not be compiled on this
host, no check that runs here would see it, and it could not be exercised anywhere until
`DECRQCRA` lands. Shipping it would have been unverified code on a promise, which is what the
evidence document says.

One small imprecision, not worth a finding: "Over a pty there is exactly one way to satisfy them"
overlooks that the original `esctest` also had a non-pty backend for iTerm2. For a generic terminal
over a pty the statement is right.

---

## Records checked

| Claim | Result |
| --- | --- |
| Packet ticks match reality | Mostly. Acceptance 1's third clause (`? 9 l` -> no press) is ticked and false at the encoder (F2); the `osc-sequences-checklist` box overstates (F8); the `esctest` box is correctly left unticked with a reason |
| `evidence/US-0102-esctest.md` | Accurate; blocker independently confirmed |
| `pty.md` `? 2027`-vs-`WcsWidth` correction | Correct, and a real improvement over the refusal it used to promise |
| `cell-and-style.md` correction | **Incomplete** — F3 |
| `CHANGELOG.md` | Accurate except the encoder sentence (F2). The **Breaking** claim is right: `Mode`, `ColorKey`, `MouseReporting`, `MouseEncoding` and `ModeSnapshot` carry no `#[non_exhaustive]`, so the added variants and the added field really are breaking |
| `docs/osc-sequences-checklist.md` rows | OSC 17 / 19 correct, with the implementing file named; OSC 1 untouched (F8) |
| `public-api.unix.txt` / `public-api.windows.txt` | **+8 lines each**, exactly as claimed: 2 `ColorKey` variants, 3 `Mode` variants, 1 `ModeSnapshot` field, 1 `MouseEncoding` variant, 1 `MouseReporting` variant |
| `--diff-platforms` six `pty` lines | Verified by `scripts/vt-public-api.py --diff-platforms` in the gate run below |
| Harness snippet | **Absent** — F7 |
| Widened citation grep | **Zero.** The `ci-local` grep (`^\s*//[/!]` + `US-`/`BUG-`/`DEC-`/`IN-`/`crates/`/`docs/`) finds nothing in `crates/vt/src`. Widening to *every* `//` comment finds 49 hits, all in ordinary `//` comments that never reach rustdoc, and none of them added by this branch. `crates/vt/CHANGELOG.md` cites no packet id either |

---

## Gates

Run in the verifier's worktree at `ef9f7c32`, `CARGO_BUILD_JOBS=3`.

| Command | Result |
| --- | --- |
| `cargo test -p oneterm-vt` | PASS |
| `cargo test -p oneterm-vt --no-default-features` | PASS |
| `cargo test -p oneterm-vt --features vt-paranoid` | PASS |
| `cargo test -p oneterm-vt --features regex` | PASS |
| `cargo test -p oneterm-terminal` | PASS |
| `cargo test --workspace` | PASS |
| `cargo test -p oneterm-tools --test corpus_check` | PASS — all 46 recordings byte-identical |
| `python scripts/vt-public-api.py --check --no-doc` | PASS |
| `python scripts/vt-public-api.py --diff-platforms` | PASS |
| `python scripts/check-english.py` | PASS |
| `python scripts/check-doc-paths.py` | PASS |
| `pwsh scripts/ci-local.ps1 -Full` | PASS |
| `cargo test -p oneterm-vt --test verify_us0102` | PASS — 57 tests, this verifier's |

Every row above is a step of the one `pwsh scripts/ci-local.ps1 -Full` run except the last, which is
this verifier's own test target. That run finished `ci-local: all checks passed` **with the two
untracked verifier test files present**, so `cargo fmt --check`, both `clippy -D warnings` passes,
`cargo package --list`, the rustdoc self-containment grep and `cargo deny check licenses bans
advisories` all cover them. `check-english.py` and `check-doc-paths.py` were re-run by hand
afterwards, against the final text of this document: 854 files and 197 paths, both clean.

The `ci-local -Full` log is kept in the verifier's scratchpad, not in the repository.

---

## Could not be verified

- **The E2E walk.** Not attempted, for the same reason the implementer did not: `htop` under `? 9`
  and `? 1015`, a `DECSCNM` program, a `G2` line-drawing TUI and an emoji file under `? 2027` all
  need a real Windows session, and the owner runs their editor inside this terminal. Every claim in
  this document rests on byte-feed tests through the public API. F6 is the finding that walk would
  most likely have produced.
- **`esctest` itself.** Windows host, and blocked on `DECRQCRA` regardless.
- **Whether xterm prefers the SGR encoding over the X10 report when both are set** (F9). I could not
  find a statement in `ctlseqs` that settles it and did not read xterm's `button.c`.
- **`harness.db`.** Not opened. F7 is "there is no snippet", not "the snippet is wrong".

---

## Recommended follow-ups

1. **A packet for the cross-chunk cluster carry** (F1), which can carry the `cluster_width` base
   guard (F11) with it — the two share a symptom. F1 is what `cell-and-style.md` originally asked
   for. Until they land, `? 2027`'s row in the mode table should say so.
2. **A packet, or a line in the embedder guide, for `ModeSnapshot::reverse_video`** (F6) — either
   wire the palette swap or stop writing "the embedder swaps" in the present tense.
3. **Text corrections**, none of which need code: F2 (three places), F3 (two sentences in
   `cell-and-style.md`, one in `dispatch-and-modes.md`), F4 (a deviation id), F5 (the corpus
   sentence), F8 (one acceptance box).

---

# Final re-check at `6394f5b9`

Re-verified: 2026-09-15
Branch: `feat/vt-conformance-gaps` @ `6394f5b9` (base `main` @ `af5df2e7`)
Rework under test: `8145d2ab` (the carry), `e02d96aa` (the shifts and the mouse guard),
`c5fc49fa` (`terminal-view`), `303ced99` and `6394f5b9` (records)

## Verdict: **PASS**

All eleven findings above are closed. The four that needed code were fixed at the level they were
wrong at, not papered over: the carry lives in the engine, the mouse guard lives in the encoder
rather than in a caller's checklist, the `cluster_width` guard is one `let ... else`, and `DECSCNM`
reaches pixels through `resolve_style`. Nothing regressed; the benches are within noise. Two new
observations follow, both `LOW`, neither a reason to hold anything.

The report above is left exactly as it was written, so the two documents read as a before and after.

## How this was re-checked

New tests, written against the public API and deliberately separate from the suite the implementer
adopted:

- `crates/vt/tests/verify_us0102_final.rs` — **21 tests, all green**.
- `crates/vt/tests/verify_us0102_final_probe.rs` — diagnostics behind the tables below.

Both are in the verifier's worktree and are **not committed**. The adopted
`crates/vt/tests/verify_us0102.rs` was compared against the verifier's original: it differs only
where a test that asserted a defect now asserts the fix, each with a doc comment saying so, and the
adopted `US-0102-verify.md` is byte-identical to what was written (line endings aside).

## F1, the cross-chunk carry

`State::cluster_carry` holds the last printed cluster as `RowId` + column + scalars + charset;
`print_clusters` re-segments `carry.text + text` and re-places the head only when the joined text's
first cluster is longer than the carry.

Attacked, all green:

| Attack | Result |
| --- | --- |
| Every interior split, **two** `feed` calls: family (24), skin tone (7), keycap (6), flag (7), `e`+acute (2) | every one measures what the unsplit sequence measures |
| Every interior split pair, **three** `feed` calls: 276 + 21 + 15 + 21 combinations | same |
| One scalar per `feed`, family and keycap | 2 columns, cluster whole |
| Breakers that do not move the cursor: `BEL`, `CSI SGR`, `ESC 7`, `OSC`, `DCS`, `APC`, a mode change, a `? 1049` round trip | carry broken, tail takes its own cells (column 4, not 2) |
| Breakers that move the cursor: `CR`, `CUP` | tail starts its own cluster at column 0 and overwrites the head rather than joining it |
| `LF` | tail lands on the next row |
| `Terminal::resize` | carry broken |
| Next chunk starts with a base that does not continue (`emoji` then `abc`; `e`+acute then `o`+diaeresis) | nothing reprinted, nothing lost |
| Head wrapped to the next row before the tail arrived | the replacement follows the head to the row it landed on |
| Growing 1 -> 2 in the **last** column, DECAWM on and off | no stray head; the grown cluster wraps as a unit |
| Shrinking 2 -> 1 (wide base + `VS15`), at the margin and mid-row | the orphaned half of the wide pair is repaired |
| Carry across a scrollback-limit change between the halves | no panic, head intact |
| Charset: head mapped through `G0` DEC graphics, tail joins | head keeps its mapping |
| Charset: `SS2` covers the head, tail must not consume a second shift | correct — the next `q` is plain ASCII |
| Mode reset, and set-then-cleared mid-cluster | per-scalar behaviour, no stale carry |

Cap and cost, measured by feeding one base then N combining marks **one scalar per `feed`**:

| marks | `dropped_cluster_carries` | cursor column |
| ---: | ---: | ---: |
| 30 | 0 | 1 |
| 31 | 0 | 1 |
| 32 | 1 | 1 |
| 33 | 1 | 1 |
| 40 | 1 | 1 |
| 10 000 | 1 | 1 |
| 20 000 | 1 | 1 |

So the first drop is at 33 scalars (base + 32), which is `CLUSTER_CARRY_MAX` read as the coordinator
describes it. 10 000 and 20 000 marks finish in well under the linearity guard (20k/10k ratio far
below 8x), and the cell itself stays capped by the grapheme arena at 16 scalars.

**The `index_of == None` fallback in `replace_carried` is genuinely unreachable, and cannot panic
if it were not.** Every route that moves the carried row off the screen — a control, an escape, a
`CSI`, an `OSC`, a `DCS`, an `APC`, a resize — breaks the carry first, and `replace_carried` runs
before any printing inside `print_clusters`, so nothing in the same run can scroll the row out from
under it. `index_of` returns an `Option` and the `None` arm prints where the cursor is; there is no
indexing and no unwrap on that path. The comment calling it unreachable is right.

## F11, F2, F4, F6

- **F11.** A bare `U+FE0F`, `U+FE0E`, `U+0301`, `U+200D`, `U+20E3` and a bare `FE0F U+20E3` pair are
  all zero-width under `? 2027` now and join the cell on their left, matching the mode-reset
  behaviour. A selector still decides for a base it follows: `U+2714` + `VS16` is 2 columns,
  `U+231A` + `VS15` is 1. The recorded limitation holds and is recorded: `a` + `VS16` is still 2.
- **F2.** With `ModeSnapshot::mouse == None` — no mode at all, after `? 9 l`, after `? 1003 l`, and
  in the "encoding set without a reporting mode" cases `? 1006 h` and `? 1015 h` — press, release,
  hover, drag and both wheel directions all encode to an empty `Vec`, with and without modifiers.
  `? 9` still reports the press (`ESC [ M SP ! !`) and nothing else, and `? 1000` still encodes a
  release, so the new guard silenced only what it should.
- **F4.** `ESC 7` / `ESC 8`, `CSI s` / `CSI u` and `? 1048 h` / `l` all save and restore the
  locking-set invocation; a pending single shift is saved and restored too, and saving with none
  pending restores none. The slot is per screen (a `DECSC` on the alternate screen does not
  overwrite the primary's), `DECRC` with no prior `DECSC` falls back to `G0` / ASCII, and `RIS`
  clears both slots. Correction C12 is registered in `dispatch-and-modes.md`'s corrections table
  with "measured free" and the reason.
- **F6.** Read rather than re-tested at the `gpui` layer: `resolve_style` swaps `Color::Foreground`
  and `Color::Background` and returns every other colour untouched, after the `INVERSE` swap — so
  `INVERSE` under `DECSCNM` composes to the correct double negation on default-coloured cells.
  `StyleKey` carries `reverse_video` and `PlanCache::update` sets `restyled` from
  `self.style != Some(style_key)`, so the cached plans really are invalidated when the mode flips.
  The named test `decscnm_swaps_the_two_defaults_and_nothing_else` asserts the default pair swaps,
  that an `Ansi(1)`-on-`Ansi(4)` cell is byte-identical, and that contrast enforcement still runs.

## Records

Every one closed as claimed, checked against the files:

- The cross-chunk buffer sentence is **restored** in `cell-and-style.md` and now describes what
  shipped, including the cap and the counter; the two false statements (`unconditionally` / "there
  is no mode change", and "width today is therefore per scalar") are rewritten, and the ConPTY
  disagreement is stated as a live carried gap rather than an impossibility.
- The `? 2027` mode-table row says the same thing; R-56's wording is truthful.
- The corpus claim now names nine occurrences across six recordings and says exactly what the
  grid-only `.expect` format can and cannot prove.
- `Mode::MouseX10`'s rustdoc documents that `? 9` outranks `? 1006`, with the reason.
- The `REP` + `? 2027` disagreement and the residual `VS16`-widens-any-base limitation are carried
  forward as gaps.
- The OSC 1 acceptance line is unticked and explained.
- A `## Harness Row` section exists, with the right column list and order, `*_proof` as `0`/`1`
  (`1,1,0,1`) and `intake_id = 43`.
- `crates/vt/public-api.unix.txt` and `public-api.windows.txt` each gain exactly one line,
  `FeedStats::dropped_cluster_carries`; `scripts/vt-public-api.py --diff-platforms` still reports
  "the delta is 6 lines, all inside `oneterm_vt::pty`".
- `verify_us0099_equiv.rs` pins the one case F2's fix changes, one case wide, rather than skipping
  the comparison.

## Two new observations

### N1. `? 1049` / `? 47` / `? 1047` still do not carry the locking set — LOW

Correction C12 covers `DECSC` / `DECRC`, `CSI s` / `CSI u` and `? 1048`. The alternate-screen modes
use the grid's own cursor save, which does not touch `State::active_charset`. Measured: with `G2`
designated as line drawing and `LS2` locked, `? 1049 h`, `SI` on the alternate screen, `? 1049 l` —
back on the primary the locking set is `G0`, where xterm's `1049` restores the cursor it saved and
its `CursorSave` carries `curgl` and `gsets[]`.

Defensible as it stands: this engine deliberately keeps the invocation on the terminal rather than
on the cursor, and C12's per-screen slot is the `DECSC` half of that choice. No recording sends any
of the four shifts, so nothing in the corpus reaches it. Worth one line in the corrections table
next to C12, saying the alternate-screen modes are out of its scope.

### N2. The Harness Row snippet is not runnable — LOW

The schema half of F7 is closed: the column list, its order, the `*_proof` flags and `intake_id`
are all correct. But the snippet is raw `SQL` with placeholders — `'...see Evidence...'`,
`'...set on the verified run...'`, `'...see Gaps carried forward...'` — where every sibling packet
in this intake (`BUG-0058`, `US-0097` .. `US-0100`, `US-0104`) carries a runnable Python snippet
with the real values. As written it cannot be pasted and run.

One more nit, not worth a number: `FeedStats::dropped_cluster_carries`'s rustdoc says "a rising
count means something is feeding one unbounded cluster". True, but the count rises **once per
over-long cluster**, not once per dropped scalar — the table above shows 1 for 33 marks and 1 for
20 000 — so it should not be read as a rate.

## Performance

Same method as before: two release binaries built in this worktree from `af5df2e7` and `6394f5b9`,
`vt-bench grid --mib 32`, interleaved. Three cycles were ambiguous on the `CSI`-heavy fixtures
(`tui_redraw` 0.93, `heavy_sgr` 0.94) — `break_cluster()` now runs on every `csi`, `esc`, `execute`,
`osc_dispatch`, `dcs_hook` and `apc_start`, so that is exactly where an always-on cost would land
and it was worth resolving. Two further cycles settled it as noise. Medians of **five** interleaved
cycles, MiB/s:

| Fixture | base `af5df2e7` | `6394f5b9` | ratio |
| --- | ---: | ---: | ---: |
| `plain_ascii` | 78.3 | 76.9 | 0.98 |
| `long_lines` | 81.7 | 81.5 | 1.00 |
| `heavy_sgr` | 226.7 | 223.5 | 0.99 |
| `tui_redraw` | 126.9 | 123.9 | 0.98 |
| `scroll_region` | 60.9 | 61.3 | 1.01 |
| `cjk_wide` | 118.4 | 119.0 | 1.01 |
| `dense_cells` | 185.0 | 193.9 | 1.05 |
| `scrolling` | 82.3 | 81.8 | 0.99 |
| `sixel` | 48.0 | 47.8 | 1.00 |
| `osc_9_7` | 111.8 | 113.3 | 1.01 |

**Every fixture is inside +/- 5 per cent**, the packet's own threshold, and the three the packet
names are within 2. The carry costs a single `Option` store per non-print dispatch and an
allocation only when a carry is live under `? 2027`, which is what the numbers say.

## Gates

At `6394f5b9`, `CARGO_BUILD_JOBS=3`, with the two untracked verifier files present.

| Command | Result |
| --- | --- |
| `cargo test -p oneterm-vt` | PASS |
| `cargo test -p oneterm-vt --no-default-features` | PASS |
| `cargo test -p oneterm-vt --features vt-paranoid` | PASS |
| `cargo test -p oneterm-vt --features regex` | PASS |
| `cargo test -p oneterm-terminal` | PASS |
| `cargo test -p oneterm-terminal-view` | PASS |
| `cargo test --workspace` | PASS |
| `cargo test -p oneterm-tools --test corpus_check` | PASS — all 46 recordings byte-identical |
| `python scripts/vt-public-api.py --check --no-doc` | PASS |
| `python scripts/vt-public-api.py --diff-platforms` | PASS — 6 lines, all in `oneterm_vt::pty` |
| `python scripts/check-english.py` | PASS |
| `python scripts/check-doc-paths.py` | PASS |
| `pwsh scripts/ci-local.ps1 -Full` | PASS — `ci-local: all checks passed` |
| `cargo test -p oneterm-vt --test verify_us0102_final` | PASS — 21 tests, this verifier's |

Every row except the last is a step of the one `ci-local -Full` run. Its log is in the verifier's
scratchpad, not in the repository.

## Still not verified

- **The E2E walk**, for the same reason as before: it needs a real Windows session and the owner
  runs their editor inside this terminal. `? 5` now has a renderer-layer test, which is the half
  that walk would most likely have caught; `? 9`, `? 1015` and `? 2027` against real programs remain
  untried.
- **`esctest`**, still blocked on `DECRQCRA` and still correctly diagnosed.
- **Whether xterm prefers SGR over the X10 report when both are set.** The rework documents the
  engine's answer and its reasoning, which is a fair reading; I still did not read xterm's
  `button.c` to settle it.
- **`harness.db`.** Not opened. N2 is "the snippet is not runnable", not "the row is wrong".
