# US-0076 — independent adversarial verification

Verifier: independent agent. Branch `worktree-agent-a134c2d2c50be5a81` @ `19681af`, base
`feat/vt-engine` @ `cc14802`. Nothing was fixed and nothing was committed; every tamper below was
reverted and the tree is clean (`git status --porcelain` empty at the end of each step).

Verdict: **merge after fixes.** The parity gate is real — I tried to fool it and could not. The
engine is in unusually good shape. But the packet's headline claim, *"there is no difference at
all"*, is true **only of the 45-recording corpus**: my own differential fixtures find four
divergence families, three of which are covered by declared correction ids and one of which
(M1) contradicts the text of the deviation that is supposed to cover it. One evidence claim
(M3) is false and I disproved it by mutation.

---

## 1. Pass / fail summary

| # | Gate | Result | Evidence |
|---|---|---|---|
| 1 | Scope, out-of-scope edits, trailers | **PASS** | §2 |
| 2 | `pwsh scripts/ci-local.ps1` green | **PASS** | 1518 passed / 0 failed / 8 ignored, 57 sections |
| 3a | Byte tamper of a frozen `grid.expect` fails readably | **PASS** | §3.1 |
| 3b | A broken dispatch rule turns the gate red, naming recordings | **PASS** | §3.2 |
| 3c | `cargo test --workspace` runs the new-engine gate | **PASS** | §3.3 |
| 3d | `vt-diff` over corpus + fixtures + 20 independent fixtures | **PASS with findings** | §3.4, §4 |
| 4 | C9 "free" claim reproduced; C-id spot-checks | **PASS** | §5 |
| 5 | Behaviour audit vs `engine-semantics` §2 | **PASS with findings** | §6 |
| 6 | Seven selection wrappers, `invalidated_by` before the op | **PASS** | §7 |
| 7 | Render / events | **PASS with gaps** | §8 |
| 8 | Code quality: no `unsafe`, no `unwrap`, no new deps | **PASS** | §9 |
| 9 | Packet completeness, `harness.db` row | **PASS with contradiction** | §10 |

Disk checked before starting: `D:` free 46.7 GB (> 15 GB). Worktree has its own `target/`, no
`CARGO_TARGET_DIR` override, so no shared-target clobber.

---

## 2. Scope and out-of-scope edits

`git diff cc14802...HEAD --stat`: 29 files, +5774 / −63. Four commits.

**Trailers — PASS.** All four commits carry an identical, well-formed trailer pair
(`Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` + the same `Claude-Session` URL) and
`Refs: IN-0029`. Verified with `git log -1 --format='%B' | tail -3` on each of the four SHAs.

**Out-of-scope edits — all justified, all documented in the packet's own table.** My judgement:

| File | Change | Necessary? | Semantics changed? | Test? |
|---|---|---|---|---|
| `parser/mod.rs` | re-export `ParamGroups` | yes | no | n/a |
| `parser/osc.rs` | `params_full()` opens the last slot one parameter earlier | **yes** — without it a bulk `OSC 4` loses separators; P9 is assigned to this packet | **yes**, deliberately | `terminal::tests::osc_parameters_past_the_sixteenth_are_re_split`; `parser::tests::osc_params_past_sixteen_join_into_the_last` expectation moved with it |
| `grid/screen.rs` | `set_region_raw` | yes — the reference's one-based test can produce an empty region `set_region`'s `top < bottom` contract cannot express | additive only | indirect |
| `grid/screen.rs` | `row_mut` allocates with `Cell::EMPTY`, not the erase cell | **yes — real bug** | yes | **NO — see M3** |
| `grid/row.rs` | `clear_wrap_at` + 5 call sites | yes — G1 flag lifetime | yes | `terminal::tests::overwriting_the_last_cell_clears_the_wrap_flag` (but incomplete — see M2) |
| `intern.rs`, `intern_tests.rs`, `render/render_tests.rs` | `HyperlinkTable::intern` returns `Option` | yes — the ladder is assigned here | yes | `hyperlink_table_exhaustion_drops_the_attribute_and_logs_once` |

Two of these change semantics in files this packet does not own (`parser/osc.rs`,
`grid/screen.rs`). Both are genuine defect fixes with stated reasons, and both are disclosed in
the packet. Acceptable, but they need the owning packets' blessing at merge.

---

## 3. Is the gate real?

I read `crates/tools/src/corpus_replay_new.rs` end to end. It is real:

* `replay_new` feeds the recording bytes through the **new** engine: `term.feed(&recording.bytes,
  &mut batch, now)` (`corpus_replay_new.rs:78`).
* `check_recording` (`corpus.rs:796-810`) reads the **frozen files from disk**
  (`grid_expect_path()`, `state_expect_path()`) and compares. There is no path where the new
  engine writes an expectation: `vt-corpus bless` hard-refuses `--engine new`
  (`vt-corpus.rs:156-159`).
* The comparison is **cell-exact on all six fields** — `content`, `attrs`, `fg`, `bg`,
  `underline`, `hyperlink` (`CELL_FIELDS`, compared per field at `corpus.rs:733-750`) — plus the
  row `wrap` flag (`corpus.rs:715`) and the four geometry fields. `encode_cell` reconstructs all
  15 reference `Flags` bits including `WIDE_CHAR`, `WIDE_CHAR_SPACER`,
  `LEADING_WIDE_CHAR_SPACER` and `WRAPLINE`.
* `state.expect` is **not skipped**: `diff_state` (`corpus.rs:755-786`) takes the **union** of
  expected and actual keys and reports a missing key as `<absent>`, so an encoder that silently
  stopped emitting `palette.*`, `modes`, `cursor`, `pending_wrap`, `tab_stops` or `scroll_region`
  would fail rather than pass.
* The comparator itself is **unmodified** by this branch. The diff to `corpus.rs` is +16/−6 and
  touches only `KNOWN_DEVIATIONS` (adding C12–C15) and the engine selector. `diff_grid` /
  `diff_state` are untouched. The gate was not weakened to fit the implementation.

### 3.1 Tamper (a) — one byte of a frozen expectation

Changed `row 9 wrap=0 1*0065;` → `1*0066;` in `sgr/grid.expect` (one byte):

```
FAIL  sgr
        grid: row 9 col 0 content: expected "0066", got "0065"
1 recordings, 0 passed, 1 failed (New engine)   EXIT=1
```

Readable diff, correct exit code. **Both** engines fail it (§3.3), which proves the frozen file
is genuinely the baseline for both and is not regenerated.

### 3.2 Tamper (b) — break a dispatch rule

| Tamper | Result |
|---|---|
| `[1] => style.attrs.insert(Attrs::BOLD)` → `[1] => {}` (SGR 1 no longer bolds) | **45 → 25 passed, 20 failed**, each named: `colored_underline`, `csi_rep`, `decaln_reset`, `deccolm_reset`, `delete_chars_reset`, `erase_chars_reset`, `fish_cc`, `grid_reset`, `history`, `indexed_256_colors`, `insert_blank_reset`, `issue_855`, `ll`, `origin_goto`, `row_reset`, `scroll_up_reset`, `tmux_git_log`, `tmux_htop`, `wrapline_alt_toggle`, `zsh_tab_completion` |
| `2 => erase_display(DisplayClear::All)` → `DisplayClear::Below` (ED 2 skips rows above) | **45 → 42 passed, 3 failed**: `vttest_origin_mode_1`, `vttest_origin_mode_2`, `vttest_tab_clear_set` |

One attribute bit turns 20 of 45 recordings red. ED 2 turns only 3 because most recordings issue
`ED 2` with the cursor already home, where `Below` ≡ `All` — an honest, explainable result.
Both reverted.

### 3.3 The gate runs in `cargo test --workspace` — confirmed

`crates/tools/tests/corpus_check.rs` has two `#[test]`s, `Engine::Old` and `Engine::New`. With
tamper (a) applied:

```
test the_alacritty_reference_corpus_matches_its_frozen_expectations ... FAILED
test the_new_engine_matches_the_frozen_expectations ... FAILED
test result: FAILED. 0 passed; 2 failed;   EXIT=101
```

Drift fails CI. In the clean CI run the new-engine gate is present and green
(`ci-local.txt:1293`).

### 3.4 `vt-diff` — reproduced

```
45 recordings, 45 identical, 0 differing     (corpus)
10 recordings, 10 identical, 0 differing     (vt-bench fixtures, 160x45, 256 KiB)
```

Both claims in the packet reproduce exactly.

---

## 4. My own fixtures — 20 seeded 64 KiB streams

Generated independently of the implementer's generators (random mixes of CSI / SGR / scroll /
erase / insert / wide chars / OSC, seeds 1000-1019; `scratchpad/genfix.py`). **20 of 20 differed.**
That headline is misleading on its own, so I bisected it by feature family
(`genfam.py`), then to single sequences (`genmicro.py`), then to hand-written minimal cases
(`genrepro.py`).

Fourteen of twenty families are byte-identical: plain text+SGR, cursor motion, erase/insert/delete,
scroll regions and scrolling, DECSC/DECRC, `?45`, `?1049`, OSC, queries, control characters, raw
C1 bytes, wide+combining characters, DECCOLM, RIS. The divergences reduce to **four causes**:

| Cause | Minimal reproducer | Reference behaviour | New engine | Declared? |
|---|---|---|---|---|
| **DECSTR** `CSI ! p` | `s02`: `ESC[1;31m ESC[!p Q` → attrs `BOLD` vs `-` | no handler at all (`soft_reset` absent from `vendor/vte/src/ansi.rs`) | implements it | **yes — C9** |
| **Legacy alt screen** `?47` / `?1047` / `?1048` | `a01`–`a03` | distinct semantics | collapsed into the `?1049` swap | **yes — C8**, packet Gap 4 |
| **Synchronised output** `?2026` | `y01`: `abc ESC[?2026h DEF` → `DEF` absent vs present | parser **buffers bytes** until close/timeout | applies immediately; suppression is at the render layer | **yes — by design**, `damage-and-render-state.md` § Synchronized output + D4. Not a defect |
| **DECAWM off, pending wrap** | `w01`, `w03`, `w04`, `w05` | sets `input_needs_wrap` unconditionally | only when autowrap is on | **partly — G3, but its stated scope is wrong.** See M1 |

A second pass excluding only the *declared* triggers (`genclean.py`, 10 × 64 KiB) left a small
residue traced to `CSI ?5W` (**C10**) and `ESC c` (**C6**) — both declared — and, after excluding
those too, a final residue of 3–46 differences per fixture in **wide characters at the last
column with scrollback**. See M2.

**Conclusion: no divergence I found is outside the declared C/D/G id set except M1's widened
scope and M2.** That is a strong result for a reimplementation of this size.

---

## 5. The C9 "free" claim, and C-id spot-checks

**C9 reproduced — the claim is honest.** `grid_reset/recording` is 2046 bytes and contains
`ESC [ ! p` exactly once, at offset **1602**:

```
...<ESC>[?2004l\r\r\n<ESC>c<ESC>]104<07><ESC>[!p<ESC>[?3;4l<ESC>[4l<ESC>><0d><ESC>[1m<ESC>[7m%<ESC>[27m<ESC>[1m<ESC>[0m
```

So `grid_reset` really does send DECSTR; it is immediately preceded by `ESC c` (RIS) and
`OSC 104`, which already reset everything DECSTR would touch, and immediately followed by
`ESC[?3;4l`, `ESC[4l`, `ESC>` and a run of SGR that re-establishes the rest. The soft reset's
effects are genuinely overwritten. `terminal::tests::decstr_soft_reset_scope` exists and passes.

**Spot-checks of the measurement tables (re-measured by scanning all 45 recordings myself,
`scan.py` / `scan_sgr.py`):**

| Id | Design claim | My measurement | Verdict |
|---|---|---|---|
| C10 (`CSI ?5W`) | "none of the 45. Free" | none of the 45 | **accurate** |
| C11 (SGR 5/6/53/55) | "none of the 45 — no recording sends those parameters" | none of the 45 (a naive `;`-split says six recordings, but every hit is the `5` of `38;5;N`; with 38/48/58 sub-parameters skipped correctly, zero) | **accurate** |
| C8 (`?47`/`?1047`/`?1048`) | "none of the 45" | none of the 45 | **accurate** |
| C6 (RIS + OSC 104) | "1 recording: `grid_reset` (one RIS, one OSC 104)" | `grid_reset`: RIS ×1, OSC 104 ×1 | **accurate** |
| C7 (OSC 4) | "`indexed_256_colors` sends 240" | `indexed_256_colors`: 240 | **accurate** |
| G3 (`?7l`) | "4 recordings: `vttest_origin_mode_1`, `vttest_origin_mode_2`, `vttest_scroll`, `vttest_tab_clear_set`" | exactly those four | **accurate** |

The measurement work behind the corrections tables was done honestly and I could not fault it.

---

## 6. Behaviour audit against `engine-semantics` §2

Audited table by table against the vendored reference. Broadly **correct**: DECRQM wire values
(1/2/0) and the full `Mode::PRIVATE` sweep; DA2; DSR 5/6 with the declared origin-relative CPR
(C5); DECSTBM edge cases byte-for-byte including the degenerate zero-height region; DECSC/DECRC
per screen, saving charset designations and (reference-faithfully) not saving origin mode;
`?1048` as DECSC/DECRC without a swap; RIS's full effect list including palette, title and stacks;
OSC 0/2 with `.trim()` and `;`-rejoin; the title stack (D14, cap 16, drops oldest); OSC 4 complete
pairs (C7) and index >255 rejection; OSC 104's trap-26 behaviour; OSC 8 with `id=` matching and
the bound ladder; OSC 133 marks as anchors; OSC 9;7 through the claim bitmap; mouse-mode
exclusivity asymmetry; DECSCUSR 0-6 and blink; the kitty stack including the D15 fix that stops
overflow corrupting the **title** stack (directly pinned by a test); the DEC special graphics map;
HTS/TBC/CHT/CBT and `CSI ?5W`. All 29 tests named in the LLD's Verification list exist; 61
`terminal::tests` confirmed by running them.

**DA1** answers `?62;4;22c`, not the `?62;4c` in `research/api-surface.md` §3 — the research doc
is superseded by **D13** in the LLD, which declares the third parameter. Not a defect.

**DECALN not homing** and **SUB as a no-op** both match the vendored reference and contradict the
LLD text; the packet declares both (Gaps 2 and 3). The reference is the right choice here,
because the gate measures against it and neither has a correction id that would let a difference
be declared. **No new C-id is needed for either**: DECALN's cursor position is compared through
`state.expect`'s `cursor` field and three recordings send `ESC # 8`, so homing would have turned
those recordings red — the implementer's choice is the only one that passes. SUB is sent by no
recording at all, so it is unmeasurable either way; a Gaps entry is the correct record.

**D7 / D8 / D10 are implemented here** though the deviation table assigns them to `US-0086`. They
produce only *answers*, which the parity harness discards — confirmed by my `f13_queries` fixture
(DA1/DA2/DSR/DECXCPR/XTVERSION/modifyOtherKeys, 16 KiB): **identical**. So they turn no recording
and the packet-column conflict is recorded rather than resolved, which is the right call.

Findings from this audit are M4, M6 and the minor list.

---

## 7. Selection wrappers and the invalidation obligation — PASS

All seven wrappers exist in `crates/vt/src/terminal/mod.rs:221-289` and match `selection.md`
§ Interfaces. The obligation is met at **every** invalidating site — the predicate is evaluated
**before** the grid is mutated in all six: `ED` (`dispatch.rs:201` then `:209`), `EL` (`:1042`
then `:1043`), `RIS` (`:377` then `:378`), alt swap (`:257` then `:258`), `DECCOLM` (`:233` then
`:235`) and `DECALN` (`:243` then the fill). `DECSTR` correctly does not evaluate it (it erases
nothing). `IL`/`DL`/`SU`/`SD`/`RI`/LF-scroll use the tracked-anchor mechanism by design.
No after-the-fact evaluation anywhere.

---

## 8. Render and events

`render_update` (`mod.rs:197-212`) does no work of its own and delegates to
`RenderState::begin_update` — substantively the shim, though 14 lines, not three. `feed`
(`mod.rs:166-192`) takes a caller-owned `&mut EventBatch`, clears it, and the caller drains via
`batch.iter()` outside the lock; `Repaint` is appended once. Every event in
`research/api-surface.md` §3 has a `VtEvent` variant, a polled mode, an `OscClaims` claim, or a
named owner — **nothing is unaccounted for**; `ColorQuery` and `ClipboardLoad` are improvements on
the reference's `Arc<dyn Fn>` callbacks.

Two gaps: **cursor shape and `lines_produced` do not reach `RenderState`** (only via a second
`&self` call), which is what the LLD specifies but will surprise the adapter at `US-0081`; and
`feed`'s contract clause 1 (a debug assertion on an undrained batch) is not implemented.

---

## 9. Code quality — PASS

* `unsafe`: **zero** in `crates/vt/src/terminal/*.rs`.
* `unwrap()` / `expect(` / `panic!` outside `#[cfg(test)]`: **zero**. Every slice/array index in
  the five new files is provably in bounds (charset index `0..=3`, keyboard stack index `≤ MAX-1`,
  `ColorKey::index()` bounded by `COLOR_COUNT`, OSC bitmap word guarded by `code < 2048`).
* Dispatch is match tables throughout; the longest function (`csi`, 286 lines) is one flat
  `match (byte, intermediates)` whose arms are 1-8 lines each — the idiomatic shape, not nesting.
* **No new external dependency.** The only manifest additions are `vt-paranoid = []` (a feature,
  no dep) and the internal `oneterm-vt` workspace edge for `vt-diff`, mirrored correctly into
  `scripts/dependency-graph-policy.json` with `oneterm-vt` still a leaf.
* Two dead public methods and an over-wide re-export list (minor, below).

---

## 10. Packet completeness and the database row

Every template section is present and substantively filled. `harness.db` (repo root) has the
`US-0076` row: `status implemented`, `risk_lane high_risk`, `unit_proof 1`, `integration_proof 1`,
`e2e_proof 0`, `platform_proof 0`, `verify_command "pwsh scripts/ci-local.ps1"`,
`last_verified_result pass` — consistent with the packet's PROOF block and with Gaps 10's honest
"no integration, E2E or platform proof beyond the corpus".

**Contradiction:** the seven selection wrappers appear under **`Out of scope`** (lines 58-60) *and*
as a ticked **Acceptance** criterion (lines 79-80), and they are the entire content of commit
`19681af`. Also Outcome line 33 says `(C1-C14)` where Evidence and Gaps discuss `C15`.

---

## 11. Findings, ranked

### BLOCKER — none

No silent grid corruption, no wrong reply to a sequence a real program sends, no weakened gate,
no `unsafe`, no panic path, no new dependency.

### MAJOR

**M1 — deviation G3's stated scope is wrong, and the unstated part loses a line break.**
`crates/vt/src/grid/screen.rs:1082-1089`:

```rust
fn advance(&mut self, mode: PrintMode) {
    if self.cursor.pos.col + 1 < self.cols {
        self.cursor.pos.col += 1;
    } else if mode.autowrap {          // <-- reference has no such condition
        self.cursor.pending_wrap = true;
    }
}
```

The reference (`vendor/alacritty_terminal/src/term/mod.rs:1147-1151`) sets `input_needs_wrap`
**unconditionally**. `grid-and-scrollback.md:277-284` declares this as G3 and asserts *"The
printing result is identical, but the flag is read by two other paths … observable **only**
through `EL 0` or `HT`."* **Both halves of that sentence are false.** Minimal reproducers, 10
columns:

| Case | Input | Reference | New engine |
|---|---|---|---|
| `w01` | `ESC[?7l` + 10×`a` + `ESC[?7h` + `X` | `X` at row+1 col 0 (wraps) | `X` overwrites col 9 — **the line break is lost** |
| `w04` | `ESC[?7l` + 10×`a` + `ESC[0K` | col 9 keeps `a` | col 9 **erased** |
| `w05` | `ESC[?7l` + 10×`a` + combining mark | attaches at col 9 | attaches at col 8 |
| `w02` | control, wrap on throughout | — | identical ✔ |

`w01` is a **printing-result** difference and a third observable path (re-enabling DECAWM), which
G3 does not name. The gate cannot catch it: the four recordings that send `?7l` never re-enable it
at the last column.
*Fix:* either drop the `mode.autowrap` condition (reference parity, one line, and G3 shrinks to
nothing), or — if the deviation is kept — correct `grid-and-scrollback.md:277-284` to say the
printing result is **not** identical and add the `?7l`→`?7h` and zero-width cases, and give the
widened deviation a declared window. The code is `US-0075`'s, but `US-0076` is the parity gate and
its evidence currently claims no differences exist.

**M2 — a residual wrap-flag divergence survives on wide characters at the last column.**
After excluding every declared trigger, 4 of 10 narrow combination fixtures still differ, always
with `grid: wrap: expected "1", got "0"` as the first difference. Bisected to a single byte
offset: the divergence appears between bytes 4918 and 4946 of
`scratchpad/combo/c8_wide_at_edge/recording`, at

```
<ESC>[30;160H  ＡあＡＡあ世カ界Ａカ世Ａ한
```

— the cursor placed on the **last column** of a 160-column screen, followed by wide glyphs that
cannot fit. The reference keeps `WRAPLINE`; the new engine's row `WRAPPED` flag ends up clear.
This is the defect-2 `clear_wrap_at` fix (`grid/row.rs`) interacting with the
leading-wide-spacer path: the fix is real but incomplete. It does not reproduce from a clean
screen (`z1`-`z4`, `x1`-`x3` are all identical), so it needs accumulated scrollback state; the
saved 4946-byte prefix is a deterministic reproducer.
**Why it matters:** the wrap flag is what reflow reads to rejoin logical lines, which is exactly
the harm the packet's own defect-2 narrative describes. A cleared flag splits a wrapped CJK line
on window resize. No corpus recording reaches it.
*Fix:* order the `WRAPPED` set after the leading-spacer write (or exempt the spacer write from
`clear_wrap_at`), and add a wide-char-at-last-column case to
`overwriting_the_last_cell_clears_the_wrap_flag`.

**M3 — the Evidence over-claims: defect 1 has no regression test. Disproved by mutation.**
The packet says *"Three real defects the gate found, **each fixed with a named regression
test**"*. Defects 2 and 3 do have named tests (verified). Defect 1 (`Screen::row_mut`
materialising with the erase cell) does not — the packet's own bullet names none, it only says
"(`US-0075` code.)". I reinstated the bug and ran the suite:

```
# with the defect reinstated in crates/vt/src/grid/screen.rs
cargo test -p oneterm-vt  ->  329 passed; 0 failed   (and 5, 5, 0 in the other targets)
vt-corpus check --engine new -> FAIL sgr;  45 recordings, 44 passed, 1 failed
```

**All 339 unit tests pass with the bug present.** Only the `sgr` recording catches it. If the
corpus were ever trimmed, the regression returns silently.
*Fix:* one test — set a background-erase colour, write a single glyph onto a fresh row, assert the
untouched columns are default — and correct the sentence in Evidence.

**M4 — DECRQM answers `Set` for `?45`, a mode that does nothing.**
`dispatch.rs:366` routes `Mode::ReverseWrap` to the generic `self.state.modes.contains(other)`,
so `CSI ?45h` then `CSI ?45$p` replies `?45;1$y`. But `ReverseWrap` has no reader outside
`mode.rs` (grep confirms: `mode.rs:44,95,134,164,202` only) and `Screen::backspace` is
unconditional. The packet admits this (Gap 1). Two lines up, `Win32Input` gets this right by
answering `Reset` for an accepted-but-unimplemented mode.
*Fix:* `Mode::ReverseWrap => ModeState::Reset,` plus the expectation in
`decrqm_answers_match_the_mode_table`.

**M5 — two live `hit_test` implementations that disagree.**
`terminal/mod.rs:268-289` re-implements `selection::hit_test` (`selection/mod.rs:298-315`) rather
than delegating, despite the comment above it claiming "seven **one-line wrappers**". They
disagree off the right edge (`mod.rs:283` forces `Side::Right`; `selection/mod.rs:304` does not),
and each has its own green test pinning the opposite answer. Both are public API.
*Fix:* make the wrapper delegate and move the off-edge rule into `selection/mod.rs`.

**M6 — `Config::accept_c1` is a dead knob.** `terminal/mod.rs:70` and `:80` are its only two
occurrences in the whole workspace; the parser hard-codes the `false` behaviour. Setting it `true`
silently does nothing, and it is part of the LLD's published `Config`.
*Fix:* thread it into the parser, or delete it and record the deferral.

**M7 — the packet contradicts itself on the selection wrappers** (Out of scope *and* ticked
Acceptance); `(C1-C14)` vs `C15`. *Fix:* move the bullet into `In scope`.

### MINOR

1. `feed`'s contract clause 1 — the undrained-batch `debug_assert` — is not implemented and not
   declared (`mod.rs:167`); `events-and-api.md` names `event::tests::undrained_batch_asserts_in_debug`.
2. `terminal_tests.rs:1641` `the_invalidation_predicate_runs_before_the_operation` **does not test
   ordering**: all nine cases are ordering-insensitive, so a mutant that moved the predicate below
   the mutation would stay green. The obligation is met but unpinned.
3. Cursor shape and `lines_produced` never reach `RenderState`; LLD-conformant but undeclared in
   Gaps, and the adapter will need a second locked read.
4. `set_scrollback_limit` (`mod.rs:334`) never calls `prune_selection()`, unlike `resize` and
   `feed` — an anchor-entry leak that self-heals on the next feed.
5. Dead public surface: `Terminal::grid_mut()` (`mod.rs:354`, hands out unrestricted `&mut
   TerminalGrid`) and `Terminal::stats()` (`mod.rs:439`); plus nine re-exports wider than
   `events-and-api.md` prescribes.
6. DECSTR does not reset charsets or DECCKM — the common reason to send `CSI ! p` is to recover
   from a wedged line-drawing state. Either extend it or add a Gaps row.
7. OSC 52's selection byte is first-byte-wins (`dispatch.rs:1420`), where the LLD says it "must be
   `c`, `p` or `s`; anything else drops the request". Reference-faithful, but it is a widening on a
   security-relevant write path, and the engine has **no refusal hook at all** — policy lives in
   `crates/terminal/src/security_policy.rs` by design, so the "hook can refuse" requirement has no
   engine-side answer.
8. RIS clears the title without emitting `TitleReset`, so the OS window keeps a stale title
   (reference has the same wart).
9. `?9` (X10 mouse) and `?1015` (urxvt) are silently absent from the mode table without being on
   the LLD's "deliberately not implemented" list.
10. A claimed `OSC 133` with an unknown sub-code is counted `unhandled` *and* forwarded, inflating
    the stat on every prompt for shells that send `133;P`.

---

## 12. Verdict

**Merge after fixes.**

The parity gate is genuine, sensitive and un-blessable, the comparator was not weakened, the
measurement tables are accurate everywhere I checked, and an independent 64 KiB-scale differential
finds the engine byte-exact against the reference outside the declared deviation set. That is a
high-quality piece of work.

Required before merge: **M3** (false evidence claim + the missing test), **M1** (either the
one-line parity fix or a corrected G3 scope with a declared window — the current text is provably
wrong and it hides a lost line break), **M2** (incomplete wrap-flag fix, feeds reflow) and **M7**
(self-contradictory scope). M4-M6 are small and should ride along. The minor list can follow in
`US-0086`/`US-0079` as their owners dictate.

Reproducers for every claim above are in the verifier scratchpad:
`genfix.py`, `genfam.py`, `genmicro.py`, `genrepro.py`, `genclean.py`, `gencombo.py`,
`scan.py`, `scan_sgr.py`, and the bisected `combo/c8_wide_at_edge/recording`.
