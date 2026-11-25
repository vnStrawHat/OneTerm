# Independent verification — US-0073 (parser core), IN-0029

Verifier: independent agent (not the implementer). Date: 2026-09-12.
Implementer's own evidence: [`US-0073-verify.md`](US-0073-verify.md) — separate file, not edited here.

- Branch: `worktree-agent-a450783bce74d5dc6`, packet commit `e2246ac`, base `f3cf1a5`.
- **Merge commit `506e3c1`** — `Merge feat/vt-engine into US-0073 (parser core)`, `feat/vt-engine` @ `1ef1414`
  (US-0071 pty, US-0074 cell/style, US-0075 grid). Everything below is measured on the **merged** tree.
- Worktree: `D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-a450783bce74d5dc6`, own `target/`.
  Free space on `D:` at start: **58.72 GB** (> 15 GB, proceed).
- Not committed: this file, and `crates/vt/tests/ext_differential.rs` (the verifier's extended
  differential, left untracked; source also in the session scratchpad).

## Step 0 — merge resolution

Four conflicts, resolved as the union the packet's Handoff describes.

| File | Resolution |
| --- | --- |
| `crates/vt/Cargo.toml` | upstream file + `memchr.workspace = true` and `vte.workspace = true` (dev) with this packet's oracle comment; upstream description kept |
| `crates/vt/src/lib.rs` | upstream file + `pub mod parser;` |
| `Cargo.lock` | union of the `oneterm-vt` dependency list (both sides) |
| `scripts/dependency-graph-policy.json` | textual merge produced a **duplicate** `"oneterm-vt": []` key; took `feat/vt-engine`'s file verbatim, as the Handoff says |
| root `Cargo.toml` | auto-merged (member, path entry, `memchr`/`vte` workspace deps) |

Trailers on `506e3c1` and on `e2246ac` are both exact.

---

## Verdict

**Merge after fixes.** Three fixes, **none of them in the parser**: two stale doc rows the merge
makes factually wrong, and one evidence correction. Zero blockers. The state machine itself is the
strongest-verified code in this intake: 2 000 newly generated buffers on a different seed, ~70
hand-written adversarial cases, every split point of seven sequences, and 1/2/3/5/7-byte chunking
produced **zero** unfiltered divergences from the oracle and **zero** chunk-invariance failures.

---

## Pass / fail table

| # | Check | Result | Evidence |
| --- | --- | --- | --- |
| 1 | Scope of `f3cf1a5...e2246ac` | **PASS** | 17 files: `crates/vt/src/parser/{mod,state,params,osc,utf8,parser_tests}.rs`, `crates/vt/tests/differential.rs`, `crates/vt/fuzz/*`, `crates/vt/Cargo.toml`, `crates/vt/src/lib.rs`, root `Cargo.toml`+`Cargo.lock`, `scripts/dependency-graph-policy.json`, the packet and its evidence file. **No LLD edited, no grid/cell/render file touched.** |
| 2 | `pwsh scripts/ci-local.ps1` on the merged tree | **PASS** | `ci-local: all checks passed.` **55 sections, 1303 passed, 0 failed, 5 ignored, 0 warnings.** Expected ≥ 1267 + ~36 = 1303 — exact. `verify-dependency-graph.py`: *"Dependency graph policy passed for 21 workspace packages and 21 explicit members."* |
| 3 | Differential + my extension | **PASS** | see § Differential |
| 4 | Caps and memory | **PASS** | see § Memory, measured with a counting `GlobalAlloc` |
| 5 | `Dispatch` vs `dispatch-and-modes.md` | **PASS (1 minor gap)** | see § Trait |
| 6 | V6 — reference carry bug | **CONFIRMED** | see § V6 |
| 7 | Tier 1 throughput | **PASS as recorded, evidence inaccurate** | see § Throughput — **M-A** |
| 8 | Fuzz target | **PASS** | see § Fuzz |
| 9 | Code quality | **PASS** | no `unsafe`, no `unwrap`/`expect` outside tests, no new external crate |
| 10 | Packet, DB row, trailer | **PASS** | see § Records |

---

## Differential (item 3)

### The packet's own runner

```
$ cargo test -p oneterm-vt --test differential -- --nocapture
running 5 tests
test declared_osc_differences_are_exercised ... ok
test oracle_precondition_patches_do_not_touch_the_state_machine ... ok
test action_traces_agree_on_ref_corpus ... ok
test chunk_splitting_is_invariant ... ok
accepted declared differences: Filters { truncated_osc: 0, del_executed: 5747, joined_osc_params: 0 }
test action_traces_agree_on_generated_streams ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.60s
```

### Filter list, judged one by one

| # | Filter | Judgment |
| --- | --- | --- |
| P1 | `print_str` split back into chars by the recorder | Legitimate. `parser.md:229`. The recorder normalises, it does not hide. |
| P4 | truncated OSC accepted against the reference's unbounded one | Legitimate deviation (`parser.md:232`), **but the filter is too wide** — see **m-C**. |
| P5 | `Unhook` compared without `aborted` | Legitimate: `vte::Perform::unhook()` has no counterpart. The cap itself is proven by unit test, not here. |
| P7 | `apc_*` dropped; the reference emits nothing for SOS/PM/APC | Legitimate (`parser.md:235`). |
| P8 | `Execute(0x7F)` accepted against `Print('\u{7f}')` | **Correct, see below.** Documented in `differential.rs:29` and packet V2; **missing from `parser.md`'s P1-P7 table.** |
| P9 | 16th OSC parameter joined | **Correct, see below.** Packet V7; `parser.md:126-127` cites it wrongly. |

P2, P3 and P6 need no filter and have none — verified: P2/P3 produce identical action traces, and P6
(mode 2026) lives above the `Perform` level entirely.

### Judgment — P8, `DEL` executed rather than printed

**OneTerm is right; the reference is the outlier.**

- The oracle prints it: `vendor/vte/src/lib.rs:723` — `'\x00'..='\x1f' | '\u{80}'..='\u{9f}' => execute`,
  everything else `print`. So `0x7F` reaches `print(U+007F)`, and an embedder that forwards `print`
  to the grid writes a glyph into a cell **and advances the cursor**, corrupting column alignment for
  the rest of the line.
- xterm, kitty, Ghostty and foot all **ignore** `DEL` in ground: no cell written, no cursor motion.
- `execute(0x7F)` plus a dispatch layer that maps no handler to `0x7F` reproduces xterm exactly, and
  it keeps `DEL` out of the grapheme-cluster and width path entirely, which `print` does not.
- The LLD already agrees: its Ground row (`parser.md:39`) says `execute` for `0x00..=0x1F, 0x7F`. Only
  the deviation table forgot the row.

Risk profile is the right shape: over the 45 **real** recordings the filter fires **0** times; it fires
30 230 times over my 2 000 generated buffers and once over my hand-written set. It never touches real
terminal output.

**Action: doc-only.** Add P8 to `parser.md` § Deliberate deviations. Do not change the code. US-0076's
`Handler` must simply not route `0x7F` anywhere.

### Judgment — P9, the 16th-parameter join

**OneTerm is right; the LLD's citation *and* its rationale are both wrong.**

- The packet's V7 is correct that this is **not** the reference behaviour. Confirmed at the source:
  `vendor/vte/src/lib.rs:530` — `MAX_OSC_PARAMS => return`, which returns **without** incrementing
  `osc_num_params` and **without** extending `osc_params[15]`. So in `vte` every byte after the
  16th `;` sits in `osc_raw` and is reachable by no parameter slice. It is silently discarded.
- But `parser.md:126-127`'s motive — *"what OSC 8's `;`-joined URIs rely on"* — is also wrong. OSC 8 is
  `OSC 8 ; params ; URI ST`: three parameters. It can never reach sixteen. The number that can is
  **OSC 4 / OSC 104**: a bulk palette set is `4 ; idx ; spec ; idx ; spec …` and exceeds 16 parameters
  with nine colours.
- Joining is still the better of the two, because it is **information-preserving**: the dispatch layer
  can re-split parameter 16 on `;`, which it cannot do with bytes `vte` has thrown away.

**Action: doc-only in this packet**, plus one carry-forward. Fix both the citation and the rationale at
`parser.md:126-127`. US-0076 must confirm that OSC 4 with more than eight colour pairs is re-split from
parameter 16 rather than dropped — otherwise the join is preserved information nobody reads.

### My extension — 2 000 buffers, new seed

`crates/vt/tests/ext_differential.rs` (untracked). Seed `0xDEAD_BEEF_1234_5677` (the packet uses
`0x0129_0073_C0FF_EE01`), 500 rounds × 4 families = **2 000 buffers**, lengths random in 1..8192,
against the same `accept()` the packet uses verbatim so only *new* disagreements surface. Families:
uniform noise; escape-biased over a 34-byte alphabet widened with `9B 9D 90 00 F0 9F 98`; three-way
recording splices; 64 byte-substitutions per recording (harsher than the packet's bit flips).

```
ext_generated: 2000 buffers, filters=Filters { truncated_osc: 0, del_executed: 30230,
               joined_osc_params: 0 }, divergences=0
```

### My extension — hand-written nasties

~70 cases, each run against the oracle **and** re-run at 1-, 2-, 3-, 5- and 7-byte chunking against the
whole-buffer trace. Covers every case the review asked for: `ESC` inside OSC (bare, doubled, and
`ESC [ 31 m` inside an OSC); `CAN`/`SUB` mid-CSI, mid-OSC, mid-DCS, mid-APC, and inside `Escape`;
8-bit C1 raw, in two-byte form, mid-multibyte, inside `DcsPassthrough`, and `0x9C` inside OSC and APC;
overlong 2/3/4-byte forms, surrogates, `F5`, `FE FF`, lone continuations, truncated tails;
OSC terminated by `BEL`, by `ESC \`, by a bare `ESC` at EOF, and unterminated at EOF (same four for DCS,
plus 8-bit `ST`); leading zeros, empty params, a 32-colon run, a 40-semicolon run, mixed separators,
`CSI 38:2::r:g:b m`, `u16` saturation, `CSI m`, `ESC [ ? > = <` markers, a marker mid-param,
2 and 3 intermediates (CSI and ESC), 33 params, 33 sub-params, OSC with 15/16/17/24 parameters,
a realistic OSC 8 hyperlink whose URI contains `;`, `NUL` bytes, `DEL` after `ESC` and in five states.

```
ext_nasties: filters=Filters { truncated_osc: 0, del_executed: 1, joined_osc_params: 3 }
--- oracle divergences (0) ---
--- chunk-invariance failures (0) ---
```

### My extension — every split point

Seven sequences (Sixel DCS with `ESC \`, OSC 52 with `ESC \`, OSC with `BEL`, APC, an SGR with colon
sub-params, a mixed UTF-8 + C1 + CSI run, and an `OSC ... 9C ... ESC \` + `DCS ... 9C` pair) fed as two
chunks at **every** offset `0..=len`, including `0x1B 0x5C` split down the middle:

```
ext_split: failures=0
```

### Disagreement list

**Empty.** Across 2 000 generated buffers, ~70 hand-written nasties, 45 recordings and every split point
of seven sequences, there is **no disagreement with the oracle that is not covered by a declared
filter**, and **no chunk-invariance failure**.

---

## Memory and caps (item 4)

Measured with a counting `#[global_allocator]` recording live and peak bytes, `--release`,
`--test-threads=1`. Raw:

```
(a) unterminated OSC 20 MiB, code 0 UNCLAIMED : peak heap = 2048 B
(b) unterminated OSC 20 MiB, code 52 CLAIMED   : peak heap = 12584960 B (12.00 MiB)
(c) unterminated DCS 20 MiB                    : peak heap = 2048 B; dcs_put = 16777216
                                                 (cap 16777216); unhook = 1 (aborted 1)
(d) unterminated APC 20 MiB                    : peak heap = 2048 B; apc_put = 16777216;
                                                 end = 1 (aborted 1)
(e) parser live heap after a completed 8 MiB claimed OSC: 83785 B (was 81737 B before)
    payload kept = 8388608 B
(f) 1000 hostile OSC+DCS rounds: live heap 95833 -> 95833 B (delta 0)
```

And the cap behaviour, by claim:

```
unclaimed 0,  8 KiB : code=Some(0)  nparams=3 payload_kept=2048    truncated=true
claimed  52, 8 KiB : code=Some(52) nparams=3 payload_kept=8195    truncated=false
claimed  52, 9 MiB : code=Some(52) nparams=3 payload_kept=8388608 truncated=true
unclaimed 52, 9 MiB : code=Some(52) nparams=3 payload_kept=2048    truncated=true
```

Every clause of the acceptance criterion holds:

- **2 KiB inline**: an unclaimed number stops at exactly 2048 bytes and **still dispatches**, with
  `truncated = true`. A 20 MiB unterminated OSC costs **2048 bytes of heap**, total.
- **8 MiB only for `osc_allows_large`**: the same 9 MiB payload keeps 2048 bytes unclaimed and exactly
  `8 388 608` claimed. The claim is genuinely load-bearing, not decorative.
- **DCS/APC streamed, abort at 16 MiB**: `dcs_put` is called exactly `DCS_MAX_BYTES` times and then
  stops; `dcs_unhook(aborted = true)` fires **exactly once**; peak heap **2048 bytes** — the parser
  holds no payload buffer at all, as `parser.md:134` requires. APC identical.
- **No `Vec` grows past its cap**: 1 000 alternating hostile OSC+DCS rounds move the live heap by
  **0 bytes**. `OscAccumulator::start`'s `spill.shrink_to(OSC_INLINE)` (`osc.rs:98-102`) works — after a
  completed 8 MiB claimed OSC the parser's retained heap is back to ~84 KB.
- `nparams = 3` and `code = Some(n)` on every row confirms **parameter 0 is still the code bytes**,
  which `dispatch-and-modes.md:263-265` indexes against.

One observation, **m-B**: (b) peaks at **12.00 MiB** against a documented 8 MiB ceiling. That is `Vec`
doubling during the spill grow (`osc.rs:114-118` — old 4 MiB buffer still live while the 8 MiB one is
filled). Bounded, constant, and it never exceeds 1.5×, but the HLD memory table says 8 MiB.

---

## The `Dispatch` trait (item 5)

Checked against `dispatch-and-modes.md`, which never restates the trait — it defers at `:426`
(*"`impl Dispatch for Handler<'_> { /* parser::Dispatch, see parser.md */ }`"*) — so the normative list
is `parser.md:242-255`. The implemented trait is **character-for-character that list**, plus
`osc_allows_large`.

| Hook the dispatch packet needs | Present | Note |
| --- | --- | --- |
| `print_str(&str)` primary, `print(char)` defaulting to it | yes | `mod.rs:46-53`. The `&str` contract holds: a run is handed over whole and split only at C0/`DEL`/C1. |
| `execute(u8)` | yes | C0 table `dispatch-and-modes.md:38-47` |
| `esc(&[u8], u8)` | yes | **ignore flag missing — m-A** |
| `csi(&Params, &[u8], bool, u8)` | yes | `ignore` present and **verified correct against the oracle over 2 000 buffers** |
| colon sub-parameters preserved | yes | `Params::groups()` yields one `&[u16]` per `;`-parameter with its `:`-sub-parameters appended — exactly the direct read `dispatch-and-modes.md:139-148` asks for, so `38:2:r:g:b` (5) vs `38:2:cs:r:g:b` (6) is a `group.len()` check. `sep()`/`values()` keep the flat view for `38;5;n`. Verified by `ext_nasties` cases `sgr-38-2-colon`, `sgr-38-2-colon-empty`, `sgr-38-5-semi`, `mixed-sep-run`. |
| CSI private marker reachable | yes | `state.rs:128-131` collects `0x3C..=0x3F` into the **same** `Intermediates` buffer, exposed through `csi(intermediates)`. `MAX_INTERMEDIATES = 2` fits the worst real case `CSI ? Ps $ p` exactly. |
| `osc(Option<u32>, &OscParams, StringTerm, bool)` | yes | numbered code + params + terminator + truncated, all four. `code: Option<u32>` is a superset of `events-and-api.md:47`'s `code: u32`; `None` is US-0076's to drop-and-count. |
| `dcs_hook` / `dcs_put` / `dcs_unhook(aborted)` | yes | DCS overflow **is** observable — `dcs_hook` passes `&Params`, so `params.ignored()` carries it; verified against the oracle's `hook(ignore)` over 2 000 buffers. |
| `apc_start(introducer)` / `apc_put` / `apc_end(aborted)` | yes | `dispatch-and-modes.md` asks for none; `parser.md:31,49` asks for the introducer and it is there. |
| no `Handled` return, nothing echoed (R-35) | yes | every method returns `()` |

**Synchronized output (mode 2026) — which layer?** The parser correctly does **nothing**. `parser.md:177-187`:
*"**The parser does not participate.**"*; the mode is an ordinary private-mode CSI owned by `dispatch/`
(`dispatch-and-modes.md:180`, `:380` D4, state in `Handler.sync` at `:422`), and the **renderer** skips
frames. Verified: no 2026 sniffing, no eight-byte memcmp, no byte buffer anywhere in `crates/vt/src/parser/`,
and `parser::parser_tests::sync_mode_with_leading_param_is_recognised` exists (the name
`testing-and-bench.md:313` pins) — `CSI ? 1 ; 2026 h` parses as an ordinary two-parameter private mode,
which is the case the reference's memcmp misses. My `ext_nasties` case `sync-2026` agrees with the oracle.

**`osc_allows_large` on the trait instead of `Config` (packet V1) is the right call**, not a compromise.
R-55's requirement (`parser.md:118-123`) is *"not hard-coded; comes from the embedder"*, and a `Handler`
answering from `Config::osc_claims` satisfies it while keeping `Parser` free of any `Config` reference —
which is *more* faithful to `parser.md:13-15` (*"a narrow `Dispatch` trait as its only outward coupling"*)
than threading `&Config` through `advance` would be. It also makes the parser-side test
`osc_spills_only_for_numbers_claimed_large` — which the LLD demands at `:304-305` — writable at all.
The `{ false }` default is justified by `strip.rs` (`parser.md:266-269`), which legitimately wants it.

---

## V6 — the reference carry-buffer bug (item 6)

**Confirmed, reproduced directly against `vte`.** `vendor/vte/src/lib.rs:694-701`: on `Err` with
`valid_up_to() > 0`, `advance_partial_utf8` prints only the **first** character (`:698`) but returns
`valid_bytes - old_bytes` (`:701`) — it consumes the **whole** valid prefix. Any second character that
fitted into the four-byte carry is dropped.

```
$ cargo test ... ext_v6_reference_carry_bug_reproducer -- --nocapture
vte     chunked : [Print('œ'), Execute(151)]
vte     whole   : [Print('œ'), Print('@'), Execute(151)]
oneterm chunked : [Print('œ'), Print('@'), Execute(151)]
oneterm whole   : [Print('œ'), Print('@'), Execute(151)]
```

`C5 | 93 40 97`: `vte` loses the `@` when the stream is chunked, and does **not** lose it when the same
bytes arrive in one buffer — i.e. the reference is chunk-**dependent** here, which is a defect by any
reading. OneTerm (`utf8.rs:121-131`) consumes only `c.len_utf8()` and hands the tail back, so both
chunkings produce one trace. **The new parser's behaviour is the correct one.** The differential cannot
see this because the runner feeds whole buffers; `props::utf8_carry_does_not_swallow_the_next_character`
is the right place for it.

**A second, unclaimed reference bug the new parser also fixes.** `vte`'s `advance_partial_utf8` calls
`performer.print(c)` unconditionally (`lib.rs:682`, `:698`) with no control filtering, while its ground
path executes `U+0080..=U+009F`. So a two-byte C1 split across a chunk boundary changes meaning:

```
vte     C1 split [Print('\u{9b}')] vs whole [Execute(155)]
oneterm C1 split [Execute(155)]    vs whole [Execute(155)]
```

`C2 | 9B` is CSI in one buffer and a printable glyph in two. OneTerm routes the carry through
`print_char` (`utf8.rs:147-153`), which applies the same ground rules, and is invariant. This deserves a
**V8** row in the packet's deviation table — it is a correctness win the packet is not claiming.

---

## Throughput (item 7) — **finding M-A**

Three-run medians, same machine, same fixtures (`vt-bench fixtures --out`), `--release`.
Old = `vt-bench parser --mib 20`; new = `parser::parser_tests::bench_note::tier1_parser_throughput`.

| Fixture | old MiB/s | new MiB/s | ratio |
| --- | ---: | ---: | ---: |
| `osc_9_7` | 93.0 | 459.6 | **4.94x** |
| `dense_cells` | 329.5 | 411.6 | 1.25x |
| `tui_redraw` | 435.9 | 505.2 | 1.16x |
| `heavy_sgr` | 380.1 | 418.1 | 1.10x |
| `scrolling` | 1311.0 | 1298.8 | 0.99x |
| `long_lines` | 1498.8 | 1364.6 | 0.91x |
| `sixel` | 683.2 | 615.4 | 0.90x |
| `plain_ascii` | 1208.3 | 1069.1 | **0.88x** |
| `scroll_region` | 1106.0 | 903.3 | **0.82x** |
| `cjk_wide` | 1110.6 | 858.5 | **0.77x** |

Two problems with the packet's recorded figure.

**(1) The range does not reproduce.** The packet and the DB row record *"0.85x to 7.71x"*. A fresh
side-by-side on one machine gives **0.77x at the bottom and 4.94x at the top**. `cjk_wide` and
`scroll_region` are both below the stated floor. The implementer compared against US-0072's *recorded
baseline file* rather than a same-session run; R-29 makes the number non-gating, but the recorded range
is still wrong and should be restated as measured.

**(2) The `plain_ascii` regression is real, I found its cause, and the cause is removable.**
`memchr`-style scanning **is** present (`utf8.rs:19`), but it is `memchr3(0x1B, 0x0A, 0x0D)` — deviation
P2. `plain_ascii.vt` is CRLF-terminated 160-column lines, so the SIMD scan **restarts every ~80 bytes**
instead of running once over 20 MiB, and each returned run is then re-scanned byte-by-byte by
`print_run` (`utf8.rs:162-199`). The same shape explains `scroll_region` (bare line feeds) and
`cjk_wide`.

The decisive point: **P2 is redundant.** `print_run` already splits the run at every byte below `0x20`
(`utf8.rs:168`), so `LF` and `CR` never need to end the `memchr` scan — they are handled either way. I
tested it: changing `utf8.rs:19` to `memchr::memchr(0x1B, bytes)` and nothing else left

- all **120** `oneterm-vt` unit tests green,
- all **5** `tests/differential.rs` tests green,
- all **6** of my extended tests green (2 000 buffers, nasties, every split point, memory),

and moved tier 1 to:

| Fixture | new (P2, `memchr3`) | new (`memchr(ESC)`) | vs old |
| --- | ---: | ---: | ---: |
| `plain_ascii` | 1069.1 | **1290.1** | 0.88x -> **1.07x** |
| `scroll_region` | 903.3 | **1188.4** | 0.82x -> **1.07x** |
| `scrolling` | 1298.8 | **1412.0** | 0.99x -> 1.08x |
| `long_lines` | 1364.6 | **1500.3** | 0.91x -> 1.00x |
| `cjk_wide` | 858.5 | **903.6** | 0.77x -> 0.81x |

The experiment was **reverted**; the tree is unchanged. This is a `parser.md` change (P2's rationale is
in the LLD) plus one line of code, so it needs the design owner — hence a finding, not a fix.

---

## Fuzz target (item 8)

- `crates/vt/fuzz/` is structurally sound: valid manifest, `cargo-fuzz = true` metadata, a `[[bin]]`
  named `parser` with `test/doc/bench = false`, `libfuzzer-sys 0.4`, `oneterm-vt` by path, and its **own
  `[workspace]` table** so the root workspace never sees it. `cargo build --workspace --all-targets`,
  `cargo fmt --all` and `clippy --workspace --all-targets` all pass with it present — confirmed, CI green.
- The target itself (`fuzz_targets/parser.rs`) implements `Dispatch` completely and correctly, feeds one
  parser in **two chunks** (so the carry and the OSC accumulator cross a boundary), and claims OSC 8 and
  52 large so the 8 MiB tier is reachable by the fuzzer. That is the right target for this parser.
- `cargo fuzz` is not runnable here and is **not required**: `testing-and-bench.md:185-200` (R-47) makes
  fuzzing Linux-only, a scheduled job, *"not a pull-request gate and **not** a packet exit criterion"*.
- The plain random-bytes `#[test]` substitute exists and runs:
  `parser::parser_tests::props::arbitrary_bytes_never_panic_and_chunking_is_invariant` — 10^6
  pseudo-random bytes plus mutated corpus slices, green in the CI run above.
- **CI wiring**: none is required. No sentence in `testing-and-bench.md`, `parser.md` or
  `dispatch-and-modes.md` wires a fuzz job into `.github/workflows/ci.yml`; §5 stops at *"a scheduled
  job"*. The only CI-wiring clause in the file (`:250-256`) is the Windows **bench** job and is
  explicitly owned by **US-0072**. Correctly not done here. (Recorded as **m-H**: because the fuzz crate
  is outside the workspace, nothing formats, lints or type-checks it, so it can silently rot against a
  future `Dispatch` change.)

---

## Code quality (item 9)

- **`unsafe`: none** anywhere in `crates/vt/src/parser/`. Notable, because the oracle it replaces uses
  `MaybeUninit::uninit().assume_init()` and `from_utf8_unchecked` on this exact path
  (`vendor/vte/src/lib.rs:577-587`, `:684`, `:696`).
- **`unwrap` / `expect`: none** outside `#[cfg(test)]`. Every fallible read goes through `get()`,
  `saturating_*` or `checked_*`; `parse_code` (`osc.rs:194-204`) is `checked_mul`/`checked_add` throughout.
- **One panic macro**: `state.rs:38` `unreachable!("ground is handled by the run scanner")`. Provably
  unreachable — `mod.rs:162` tests `state == Ground` before every call — but see **m-E**.
- **Table-driven vs match, readability**: `state.rs` is one `fn advance_<state>` per state, dispatched by
  a 14-arm match at `:37-53` that names all fourteen Williams states in the LLD's order, and each function's
  arms are in byte order matching `parser.md:44-63` row for row. It reads **better** than a generated
  table because each deviation carries its comment in place (`state.rs:88` ESC idempotence, `:126-127` the
  private marker, `:147` trap 23, `:177` DCS drops C0, `:250` the one 8-bit `ST`, `:294` trap 24). I
  cross-read all fourteen against both `parser.md` and `vendor/vte/src/lib.rs:167-410`; the only
  intentional differences are the declared ones.
- **Docs on public items**: every `pub` item in `mod.rs`, `params.rs` and `osc.rs` has a doc comment, and
  each cap constant carries its rationale rather than just its value. No `missing_docs` lint is configured
  workspace-wide, so this is voluntary.
- **No new external dependency.** `Cargo.lock` diff is 8 lines: the `oneterm-vt` package gains `memchr`
  and `vte`, and **no new `[[package]]` entry appears** — both were already resolved. `memchr` is in
  `dependencies.md` policy terms a SIMD byte scan; `vte` is dev-only and retires at US-0087.
  `third-party-notices.py --check` green.
- `cargo test -p oneterm-vt` runs in **0.52 s** (debug, 120 tests) against the 60 s budget of
  `testing-and-bench.md:58-59`.

---

## Records (item 10)

- **Packet** `US-0073-parser-core.md`: complete and unusually honest. Outcome, Scope (in/out), Acceptance,
  Documentation Action with the two rows it does **not** own, Context, Plan, Decisions, Verification Plan,
  a seven-row deviation table (V1-V7), Gaps, and a Handoff that describes the merge I performed
  correctly, line for line.
- **Harness DB row `US-0073`** (`harness.db`, `story` table, `intake_id 34`): present. `status = implemented`,
  `unit_proof = 1`, `integration_proof = 1`, `e2e_proof = 0`, `platform_proof = 0`,
  `last_verified_result = pass`, `verify_command = pwsh scripts/ci-local.ps1`,
  `contract_doc = .../low-level-design/parser.md`, `packet_doc` correct. Evidence and notes fields carry
  the full deviation and gap list. (The row's `1086 passed / 2 ignored` is the pre-merge base; the merged
  tree is `1303 / 5` — expected, the merge is mine.)
- **Trailer on `e2246ac`**: exact.
  ```
  Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_018tAaHVGPnSzrEHLVcg3sx1
  ```

---

## Findings

### Blocker

None.

### Major

**M-A — `docs/agents/dependencies.md:65` and `docs/agents/structure.md:243` are now factually wrong.**
Before the merge the packet could legitimately call these *"rows owned elsewhere"* — they did not exist.
On the merged tree they **do** exist (US-0071/US-0074/US-0075 added them) and each enumerates
`oneterm-vt`'s *complete* dependency set: `bitflags`, `rustc-hash`, `unicode-width`,
`unicode-segmentation`, `log`, `proptest` dev-only. Neither lists **`memchr`** (runtime) or **`vte`**
(dev, the oracle). A reader of either doc would conclude the parser has no SIMD scan and no oracle.
*Fix*: add `memchr 2.x` to both dependency lists and `vte 0.15 dev-only (differential oracle,
retires at US-0087)` to the `dependencies.md` row. Two short edits; the packet's stated write-scope
exclusion no longer applies because the rows are now wrong rather than absent.

**M-B — the recorded tier 1 range is not reproducible, and the regression it reports is removable.**
Packet Acceptance and the DB row say *"0.85x to 7.71x"*; a same-session three-run median gives
**0.77x - 4.94x** (see § Throughput). Separately, the cause of the sub-1.0x column is deviation **P2**
(`crates/vt/src/parser/utf8.rs:19`, `memchr3(0x1B, 0x0A, 0x0D)`), which is **redundant** — `print_run`
at `utf8.rs:168` already splits on every byte below `0x20`, so `LF`/`CR` need not end the scan.
Reverting that one call to `memchr(0x1B, …)` kept all 120 unit tests, all 5 differential tests and all
6 of my extended tests green while moving `plain_ascii` 0.88x -> **1.07x** and `scroll_region`
0.82x -> **1.07x**. *Fix*: restate the measured range in the packet and the DB row; and either take the
one-line change (needs a `parser.md` P2 edit, so: design owner) or record why `memchr3` is kept.

**M-C — `parser.md` does not carry P8 or the corrected P9.** The deviation table stops at P7, yet the
differential declares and exercises two more (`differential.rs:29-30`). The packet flags both (V2, V7),
but the LLD is the contract US-0076 reads. *Fix*: add P8 (`DEL` executed — the LLD's own Ground row at
`:39` already says so) and correct `parser.md:126-127`, which asserts the join *"is the reference
behaviour"* when `vendor/vte/src/lib.rs:530` proves it is not, and attributes the need to OSC 8 when the
number that actually exceeds 16 parameters is OSC 4/104.

### Minor

**m-A — `Dispatch::esc` drops the ignore flag.** `crates/vt/src/parser/mod.rs:60`:
`fn esc(&mut self, intermediates: &[u8], byte: u8)`. `state.rs:59` / `:97` call `collect()`, which sets
`params.mark_overflow()` on the third intermediate — but `esc` passes no `Params` and no `bool`, so
`ESC SP ! # 8` is indistinguishable at the handler from a well-formed `ESC SP ! 8`. `vte` passes
`ignoring` here; `dispatch-and-modes.md:33-34` says *"A sequence with `ignore` set … or with more than
two intermediates is dropped whole"*. `csi` and `dcs_hook` are both fine. The differential **cannot**
catch this: the recorder discards vte's flag at `differential.rs:150`. Impact is near zero (no real
escape has three intermediates). *Fix*: `fn esc(&mut self, intermediates: &[u8], ignore: bool, byte: u8)`,
or record it as a deviation.

**m-B — claimed-OSC peak heap is 12.0 MiB against a documented 8 MiB ceiling.** `crates/vt/src/parser/osc.rs:114-118`:
the spill `Vec` doubles, so the 4 MiB buffer is still live while the 8 MiB one fills. Bounded, constant,
never above 1.5x — but the HLD memory table says 8 MiB. *Fix*: document 1.5x, or grow the spill in fixed
`OSC_INLINE`-multiple steps with `reserve_exact`.

**m-C — the P4 filter is too wide.** `crates/vt/tests/differential.rs:279-282`: once `truncated` is set,
**any** `Osc`/`Osc` pair is accepted, whatever the payload. A genuine bug inside a truncated OSC would be
masked. *Fix*: require the new parser's parameters to be a **prefix** of the reference's.

**m-D — test paths do not match the contractual names.** `testing-and-bench.md:292-320` pins the trap-map
names as `parser::tests::*`; the real paths are `parser::parser_tests::*`. The **code** is right —
`code-style.md:255-258` mandates the sibling `*_tests.rs` file — so the docs are the stale side, but the
trap map is contractual and someone owes the edit. (US-0075's verifier hit the same class of divergence.)

**m-E — one panic macro on an untrusted-input path.** `crates/vt/src/parser/state.rs:38`
`unreachable!(...)` in a module whose contract is *"never panics on input"* (`mod.rs:151`). It is
provably unreachable, but a `debug_assert!(false, ...)` followed by a no-op would make that structural
rather than a proof a future edit could break.

**m-F — `OscParams` forces US-0076 to allocate.** `dispatch-and-modes.md:265` needs *"the URI rejoined
from `params[2..]`"*, but `OscParams` (`osc.rs:32-59`) yields disjoint slices with the `;` removed, so
rejoining needs a scratch buffer — against `events-and-api.md:132-134` clause 2 (*"never allocates in
the steady state"*). The payload **is** already contiguous and the bounds **are** already stored, so a
`joined_from(i) -> &[u8]` accessor would make it free. Cheap to add now, awkward later.

**m-G — `FeedStats::malformed_sequences` has no signal.** `events-and-api.md:113-122` declares the
counter and `:306` demands a test per field, but invalid UTF-8 reaches the handler as
`print_str("\u{FFFD}")` (`utf8.rs:50`, `:57`, `:134`), indistinguishable from a genuine `U+FFFD` in the
stream. Outside this packet's file scope; US-0079 will have no case to write unless a hook or a flag
carries it.

**m-H — `crates/vt/fuzz/` is invisible to every gate.** Its own `[workspace]` table keeps it out of
`cargo fmt --all`, clippy and `cargo build --workspace` — deliberate and correct for Windows, but it
means nothing type-checks `fuzz_targets/parser.rs` against the `Dispatch` trait it implements. The first
signature change breaks it silently. *Mitigation*: a Linux CI `cargo check` step, when one exists.

**m-I — `advance_ground`'s carry copy has no bound check.** `crates/vt/src/parser/utf8.rs:64-66`:
`end = self.partial_utf8_len + extra` indexes a `[u8; 4]`. Safe only because `partial_utf8_len` is
provably `0` at that point and `extra <= 3` (a UTF-8 tail cannot be longer). `vte` has the identical
shape, so this is inherited, not introduced — but a `debug_assert_eq!(self.partial_utf8_len, 0)` would
pin the invariant that keeps it from being a panic.

### Credit where it is due

- The chunk-invariance property is the real find of this packet, and it earned its keep twice: V6, and
  the second reference bug documented above that the packet has not claimed.
- Zero `unsafe` on a byte-level parser path where the reference uses `MaybeUninit::uninit().assume_init()`
  and `from_utf8_unchecked`.
- The oracle precondition is **asserted** rather than trusted (`differential.rs:449-474`), including the
  vacuous-patch guard. That is the right instinct.
- `osc_9_7` at **4.94x** is the number that matters for OneTerm specifically: the agent-status channel is
  OSC-dense, and the old engine's unbounded `Vec` growth was costing five times the throughput as well as
  being the DoS vector the caps close.

---

## Commands run

```
git merge feat/vt-engine                            # -> 506e3c1, four conflicts resolved as the union
git diff f3cf1a5...e2246ac --stat                   # 17 files, scope clean
pwsh scripts/ci-local.ps1                           # all checks passed; 55 sections / 1303 / 0 / 5
python scripts/verify-dependency-graph.py           # 21 packages, 21 members
cargo test -p oneterm-vt --test differential -- --nocapture
cargo test -p oneterm-vt --lib                      # 120 passed in 0.52 s
cargo test -p oneterm-vt --release --test ext_differential -- --nocapture --test-threads=1
vt-bench fixtures --out <scratch> ; vt-bench parser --mib 20        # x3, old engine
cargo test -p oneterm-vt --release --lib parser::parser_tests::bench_note -- --nocapture   # x3, new
# P2 experiment: utf8.rs:19 memchr3 -> memchr, full suites re-run, then REVERTED
```
