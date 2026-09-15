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
