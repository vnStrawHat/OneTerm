# Low-Level Design: Parser

Intake: IN-0029
HLD: ../high-level-design.md
Topic: parser
Date: 2026-09-12

> One concern per file. Implementation-level mechanics for `crates/vt/src/parser/`
> (`state.rs`, `params.rs`, `osc.rs`, `utf8.rs` — a folder because it genuinely splits).

## Concern

The byte-level state machine: it turns a byte stream into `Action` values and nothing else. It
knows no grid, no cursor, no mode. It is a **module**, not a crate, with a narrow `Dispatch` trait
as its only outward coupling, so the state machine can be replaced without touching `dispatch/`.

Reference behaviour is [`../research/engine-semantics.md`](../research/engine-semantics.md)
§ 1.1-1.9; the deltas this design deliberately introduces are listed under "Deliberate
deviations", and each one is a differential-test exclusion with a reason.

## Design

### States

Fourteen states, Paul Williams' set, same names:

```
Ground  Escape  EscapeIntermediate
CsiEntry  CsiParam  CsiIntermediate  CsiIgnore
DcsEntry  DcsParam  DcsIntermediate  DcsPassthrough  DcsIgnore
OscString  ApcString          (APC, SOS and PM share one state; only APC dispatches)
```

`Utf8` is **not** a state: partial UTF-8 lives in a four-byte carry buffer consulted at the top of
`advance`, exactly as the reference does.

Transition table, differences from the reference marked **[new]**:

| State | Bytes | Action |
| --- | --- | --- |
| `Ground` | scan to the next `ESC` | `memchr(0x1B, bytes)` — **one SIMD scan per run**. An earlier draft scanned with `memchr3(0x1B, 0x0A, 0x0D)` (deviation P2); that is **redundant and slower**, because `print_run` already splits the returned run at every byte below `0x20`, so `LF` and `CR` are handled either way while ending the scan on them restarts it every line. Measured on CRLF-terminated fixtures: `plain_ascii` 1069 -> 1290 MB/s, `scroll_region` 903 -> 1188 MB/s, with all 120 unit tests, the differential suite and the extended suite green. Validate each run as UTF-8 once, then **one** `print_str(&str)` **[new]** instead of `print(char)` per character |
| | `0x00..=0x1F` | `execute(byte)` |
| | `0x7F` (`DEL`) | `execute(0x7F)` **[new, P8]**, and the dispatch layer routes it nowhere — xterm, kitty, Ghostty and foot all ignore `DEL` in ground. The reference `print`s it, so an embedder that writes whatever it is handed puts `U+007F` in a cell |
| | `0x80..=0x9F` arriving as an invalid UTF-8 lead of length 1 | `execute(byte)` — 8-bit C1 is executed, never an introducer (trap 48) |
| | `0x1B` | `reset_params()`, to `Escape` |
| `Escape` | `00-17,19,1C-1F` | `execute` |
| | `20-2F` | `collect`, to `EscapeIntermediate` |
| | `30-4F,51-57,59-5A,5C,60-7E` | `esc_dispatch`, to `Ground` |
| | `50` `P` | `reset_params()`, to `DcsEntry` |
| | `58` `X`, `5E` `^`, `5F` `_` | to `ApcString` (record which introducer) |
| | `5B` `[` | `reset_params()`, to `CsiEntry` |
| | `5D` `]` | `osc_start()`, to `OscString` |
| | `18`/`1A` | `execute`, to `Ground` |
| | `1B` | stay (idempotent; `ESC ESC ESC [ A` is one CUU) |
| `CsiEntry` | `20-2F` collect, to `CsiIntermediate`; `30-39` digit, to `CsiParam`; `3A` `:` sub-separator **with the separator recorded** **[new]**, to `CsiParam`; `3B` `;` separator, to `CsiParam`; `3C-3F` `<=>?` collect as a private marker, to `CsiParam`; `40-7E` `csi_dispatch` | |
| `CsiParam` | as above, except `3C-3F` goes to `CsiIgnore` (trap 23) | |
| `CsiIntermediate` | `20-2F` collect; `30-3F` to `CsiIgnore`; `40-7E` `csi_dispatch` | |
| `CsiIgnore` | `20-3F`, `7F` dropped; `40-7E` to `Ground` **with no dispatch** | |
| `DcsEntry` / `DcsParam` / `DcsIntermediate` | CSI param rules, but C0 bytes are **dropped, not executed**; `40-7E` triggers `dcs_hook`, to `DcsPassthrough` | |
| `DcsPassthrough` | `00-17,19,1C-7E` to `dcs_put`; `18`/`1A` `dcs_unhook` + `execute` + `Ground`; `1B` `dcs_unhook` + `reset_params()` + `Escape`; `7F` ignored; `9C` `dcs_unhook` + `Ground` | |
| `DcsIgnore`, `ApcString` | payload discarded (APC: streamed to the APC sink **[new]**, reserved for Kitty graphics); only `18`/`1A` and `1B` leave the state | |
| `OscString` | `00-06,08-17,19,1C-1F` dropped; `07` `osc_end(Bel)` + `Ground`; `18`/`1A` `osc_end` + `execute` + `Ground`; `1B` `osc_end(St)` + `reset_params()` + `Escape`; `3B` `;` new parameter; else push | |

### Parameters and sub-parameters

```rust
pub const MAX_PARAMS: usize = 32;         // matches the reference; Williams asks for >= 16
pub const MAX_INTERMEDIATES: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ParamSep { Semicolon, Colon }

pub struct Params {
    values: [u16; MAX_PARAMS],
    seps:   [ParamSep; MAX_PARAMS],   // separator that PRECEDES values[i] (i > 0)  [new]
    len: u8,
    overflowed: bool,                 // >= MAX_PARAMS: dispatch still fires, `ignore` set
}
```

- Digit accumulation is `value.saturating_mul(10).saturating_add(d)`, clamping at `u16::MAX`.
- `;` with nothing before it pushes `0`: `CSI ; 4 m` is `[0, 4]`, `CSI 4 ; m` is `[4, 0]`.
- A dispatch with no parameters still pushes the pending `0`, so `CSI m` is `[0]`.
- **The separator is kept, not flattened.** The reference stores sub-parameters as run lengths;
  that is enough for SGR but loses the structural difference the dispatch layer needs for trap 20
  (`38;5;n` consumes a following **parameter**, `38:5:n` reads a **sub-parameter**, and
  `38:2:<cs>:R:G:B` skips a colour-space id that `38:2:R:G:B` does not).
- Over-limit: the 33rd parameter and the third intermediate set `overflowed`; the dispatch still
  fires with `ignore = true`, and `dispatch/` drops the sequence. This is a **different path** from
  `CsiIgnore`, which dispatches nothing at all (trap 23).
- `reset_params()` clears intermediates, `overflowed`, the pending value and the list. It runs on
  `ESC`, on `ESC [`, on `ESC P`, and on leaving `DcsPassthrough` through `ESC` (trap 47).

### OSC: streamed with explicit caps

The reference accumulates into an unbounded `Vec<u8>` under `std` — a remote memory-exhaustion
vector reachable from any SSH session. This design bounds it in two tiers and **truncates rather
than errors**, because rejecting a long OSC would break OSC 52 for legitimate large clipboard
writes.

```rust
pub const OSC_INLINE: usize = 2048;            // no allocation below this
pub const OSC_LARGE:  usize = 8 * 1024 * 1024; // only for numbers the embedder marked large
pub const MAX_OSC_PARAMS: usize = 16;

struct OscAccumulator {
    inline: [u8; OSC_INLINE],
    spill:  Option<Vec<u8>>,
    len: usize,
    bounds: [(u32, u32); MAX_OSC_PARAMS],
    nparams: u8,
    truncated: bool,
    code: Option<u32>,            // parsed as soon as the first ';' arrives
}
```

- The OSC number is parsed at the first `;` (or at the terminator for a parameterless OSC), so the
  spill decision is made before the payload arrives.
- **The spill list is not hard-coded (R-55).** It comes from `Config::osc_claims`, where the
  embedder marks a number large (`claim_large`). `crates/terminal` marks 8 and 52; a future kitty
  or iTerm2 packet marks 99 and 1337 when it claims them. The engine therefore has no opinion
  about which OSC numbers deserve memory, and the clipboard *policy* — who may write, who may
  read, how much — stays where it already lives, in
  `crates/terminal/src/security_policy.rs`. `OSC_LARGE` is only a memory ceiling, not a policy.
- Truncation sets `truncated` and the sequence **still dispatches**, so an over-long title becomes
  a short title rather than a dropped one. `FeedStats::truncated_osc` counts it.
- Parameters past the 16th keep accumulating into the 16th, so the dispatch layer can re-split it
  on `;`. This is **not** the reference behaviour (P9): `vendor/vte/src/lib.rs:530` returns at
  `MAX_OSC_PARAMS` without extending the sixteenth slice, so every byte after the sixteenth `;`
  sits in `osc_raw` reachable by no parameter and is silently discarded. Joining is
  information-preserving, which is why it is kept. The sequence that actually reaches sixteen is
  **OSC 4 / OSC 104**, a bulk palette set (`4 ; idx ; spec ; idx ; spec …` passes sixteen at nine
  colours) — not OSC 8, which is three parameters. **`US-0076` must re-split parameter 16 on `;`**
  when it handles OSC 4, or the preserved information is read by nobody.
- Terminators: `BEL` and `ESC \`. C1 `ST` (`0x9C`) is payload, not a terminator (trap 24). `ESC \`
  produces the OSC dispatch and then an `esc_dispatch(b'\\')` that `dispatch/` ignores.
- An empty OSC (`ESC ] BEL`) dispatches with one empty parameter.

### DCS and APC: never buffered

`dcs_hook(params, intermediates, final_byte)` then one `dcs_put(byte)` per payload byte then
`dcs_unhook()`. The parser holds no payload buffer at all; the sink owns its bound. A new `hook`
implicitly ends a previous sequence.

```rust
pub const DCS_MAX_BYTES: usize = 16 * 1024 * 1024;  // per sequence, enforced by the parser
```

Past the cap the parser stops calling `dcs_put`, calls `dcs_unhook(aborted: true)` once, and drops
the remainder until the terminator, so the bound exists in one place even for a sink that forgot
one. **The relationship to the Sixel pixel cap is deliberate (R-55):** a 4096x4096 RGBA image is
64 MiB of pixels but cannot be produced from 16 MiB of Sixel payload, so `DCS_MAX_BYTES` is the
binding constraint and the pixel clamp is the backstop that catches a pathological raster-attribute
declaration. Both are stated in [`graphics.md`](graphics.md) as one pair, not two independent
numbers.

APC is identical, into `apc_put`; today the APC sink discards, and a later Kitty-graphics intake
claims it.

### UTF-8

Ported from the reference verbatim, because the recordings pin it:

1. `memchr3` to the next control byte; if the first byte is one, short-circuit.
2. `str::from_utf8(&bytes[..plain])`:
   - `Ok` -> one `print_str(s)`.
   - `Err` with `error_len() == Some(n)`: dispatch the valid prefix; if `n == 1` and the byte is
     `<= 0x9F`, `execute(byte)`; otherwise `print_str("\u{FFFD}")`; **skip `valid + n` bytes**.
   - `Err` with `error_len() == None`: if the cut was caused by a control byte,
     `print_str("\u{FFFD}")` and take the control transition; otherwise copy into the carry buffer
     and return.
3. `advance_partial_utf8` copies up to four bytes and: completes -> `print_str`; a shorter
   codepoint completed with a tail belonging to the next character -> print that one and return the
   consumed count; invalid -> `U+FFFD`; still incomplete -> consume and wait.

The reference's seven UTF-8 tests are ported as-is.

### C1

Seven-bit only. `0x9B`, `0x9D`, `0x90` are executed as C1 controls, never as introducers (trap 48);
the single exception is `0x9C` inside `DcsPassthrough`. `Config::accept_c1: bool` exists, defaults
to `false`, and is the hook a future `S8C1T` would flip; nothing sets it in this intake.

### Synchronized output (mode 2026)

**The parser does not participate.** The reference implements 2026 one layer up by buffering up to
2 MiB of unapplied bytes with a timeout, which is a memory amplifier any stream can aim at us and
which needs the byte-exact eight-byte memcmp scan that trap 41 describes.

Here, `CSI ? 2026 h` is an ordinary private-mode set handled in `dispatch/`; everything is parsed
and applied immediately, and the **renderer** skips frames while the mode is open
([`damage-and-render-state.md`](damage-and-render-state.md)). Consequences: `CSI ? 1 ; 2026 h`
works; there is no buffer to overflow; and a program that never closes its update costs frames,
not memory.

### No passthrough echo in v1 (R-35)

An earlier draft copied Windows Terminal's `FlushToTerminal` mechanism: a 4 KiB echo buffer
surviving chunk boundaries, a `Handled` return on all ten dispatch methods, and a
`VtEvent::Passthrough` carrying the original bytes of anything unrecognised.

**It is cut.** That mechanism exists because conhost is a **relay**: bytes it does not understand
must reach the terminal behind it. OneTerm is the terminal at the end of the chain, and no
document could name what the embedder would do with the bytes. Writing them back to the PTY would
echo conhost's own `ESC [ ? 9001 h`, `ESC [ ? 1004 h`, `ESC [ 6 n` and `ESC [ c` probes straight
back at it; dropping them makes the mechanism a no-op with a real cost — a per-byte append on
every escape sequence on the hot path, plus a `Handled` value contaminating every dispatch arm.

What replaces it: an unrecognised sequence is dropped, exactly as the engine being replaced drops
it, counted in `FeedStats::unhandled_sequences`, and logged at `debug` with its final byte and
intermediates. That is `docs/agents/error-policy.md`'s "optional telemetry" row.

It may return in `US-0086` **only if** a named consumer exists. SSH-to-ConPTY bridging is the only
candidate and is not in this intake.

### Batched print runs

`Dispatch::print_str(&mut self, s: &str)` is the primary entry point;
`Dispatch::print(&mut self, c: char)` has a default body that calls `print_str`, so an alternative
state machine can implement either. The reason to batch is **not** parser throughput — the parser
is already the cheap half at 322-1321 MB/s against 126-253 MB/s for the full pipeline — it is that
a whole run lets the print path do run-length writes, one width pass per run and one damage stamp
per run.

The run is split by the print path, not the parser, at the first byte that needs the slow path.

### Deliberate deviations

Each is excluded from the differential test with this reason.

| # | Deviation | Reason |
| --- | --- | --- |
| P1 | `print_str(&str)` instead of `print(char)` per character | enables the grid-side run writes; the observable grid is identical |
| P2 | *withdrawn* — the scan is plain `memchr(0x1B)`; `print_run`'s own `< 0x20` split makes a three-byte scan redundant and measurably slower | — |
| P8 | `DEL` (`0x7F`) is `execute`d in ground and routed nowhere, where the reference `print`s it | Matches xterm, kitty, Ghostty and foot, and keeps `DEL` out of the grapheme-cluster and width path |
| P9 | The sixteenth OSC parameter absorbs the rest of the payload, where the reference discards it (`vendor/vte/src/lib.rs:530` returns without extending it) | Information-preserving: the dispatch layer can re-split it. Reached by OSC 4 / 104, never by OSC 8 |
| V6 | A carry byte is **not** lost across a chunk boundary | Reference bug: `advance_partial_utf8` prints only the first character of a completed prefix but consumes the whole prefix (`vendor/vte/src/lib.rs:694-701`), so `C5 93 40 97` loses the `@` when chunked and keeps it when whole |
| V8 | An 8-bit C1 split across a chunk boundary still `execute`s | Reference bug: the carry path calls `print` with no control filtering, so `C2 9B` is `Execute(155)` in one buffer and `Print(U+009B)` in two |
| P3 | Separator kept per parameter | structural; the reference re-derives it from run lengths |
| P4 | OSC bounded at 2 KiB / 8 MiB with truncation | the reference is unbounded under `std`; a remote DoS vector |
| P5 | DCS bounded at 16 MiB in the parser | the reference delegates the bound to a sink that may not have one |
| P6 | Mode 2026 is not buffered in the parser | `CSI ? 1 ; 2026 h` now works; no 2 MiB buffer exists |
| P7 | APC is streamed to a sink instead of discarded | reserved for Kitty graphics; the sink discards today |

## Interfaces

```rust
// crates/vt/src/parser/mod.rs
pub struct Parser { /* state, params, intermediates, osc, utf8 carry */ }

pub enum StringTerm { Bel, St }

pub trait Dispatch {
    fn print_str(&mut self, s: &str);
    fn print(&mut self, c: char) { let mut b = [0u8; 4]; self.print_str(c.encode_utf8(&mut b)); }
    fn execute(&mut self, byte: u8);
    fn esc(&mut self, intermediates: &[u8], byte: u8);
    fn csi(&mut self, params: &Params, intermediates: &[u8], ignore: bool, byte: u8);
    fn osc(&mut self, code: Option<u32>, params: &OscParams<'_>, term: StringTerm, truncated: bool);
    fn dcs_hook(&mut self, params: &Params, intermediates: &[u8], byte: u8);
    fn dcs_put(&mut self, byte: u8);
    fn dcs_unhook(&mut self, aborted: bool);
    fn apc_start(&mut self, introducer: u8);
    fn apc_put(&mut self, byte: u8);
    fn apc_end(&mut self, aborted: bool);
}

impl Parser {
    pub fn new() -> Self;
    pub fn advance<D: Dispatch>(&mut self, d: &mut D, bytes: &[u8]);
    pub fn reset(&mut self);
}
```

No method returns `Handled`; nothing is echoed (R-35).

`crates/vt/src/strip.rs` reuses the same `Parser` with a `Dispatch` impl that keeps only
`print_str` and `execute`, replacing the second `vte::Parser` that
`crates/terminal/src/logging.rs:8`, `:57-83` runs today purely to strip escapes from the session
log.

## Edge Cases and Failure Modes

- [ ] **Sequence split across chunk boundaries** — parser state, parameter buffer, OSC accumulator
  and UTF-8 carry all survive between `advance` calls. Every `advance` test is run a second time
  feeding one byte at a time.
- [ ] **Trap 22 — a parameter of `0` means "default"** — the parser stores `0`; the rule lives in
  `dispatch/`'s `param_or(default)` helper.
- [ ] **Trap 23 — CSI ignore versus CSI overflow** — two paths, same visible result, different
  traces; both asserted.
- [ ] **Trap 24 — OSC terminators** — `0x9C` inside an OSC is payload; `ESC \` produces two
  callbacks.
- [ ] **Trap 41 — sync detection** — no longer applicable by design (P6).
- [ ] **Trap 47 — `ESC \` in an OSC versus a DCS** — in `OscString` it dispatches the OSC and
  enters `Escape`; in `DcsPassthrough` it unhooks **and** resets parameters.
- [ ] **Trap 48 — 8-bit C1 is not an introducer.**
- [ ] **Unterminated OSC / DCS at end of stream** — no dispatch, state retained, bounded buffers.
- [ ] **Adversarial input** — the parser never allocates unboundedly, never panics, never indexes
  without a bound. Every malformed case increments a `FeedStats` counter.

## Verification

`cargo test -p oneterm-vt parser::`

- [ ] `parser::tests::states_match_williams_table` — table-driven over every (state, byte class)
  pair, asserting the target state and the emitted action.
- [ ] `parser::tests::params_defaults_and_separators` — `CSI ; 4 m`, `CSI 4 ; m`, `CSI m`,
  `CSI 38:2:255:0:255;1 m`, `CSI 38;2;255;0;255;1 m`, `CSI ::::…:x` with 32 colons.
- [ ] `parser::tests::param_overflow_dispatches_with_ignore` and
  `parser::tests::private_marker_in_csi_param_dispatches_nothing` — trap 23.
- [ ] `parser::tests::param_value_saturates_at_u16_max`.
- [ ] `parser::tests::osc_terminators_bel_and_st`,
  `parser::tests::osc_keeps_c1_st_as_payload` — trap 24.
- [ ] `parser::tests::osc_truncates_at_inline_cap_and_still_dispatches`.
- [ ] `parser::tests::osc_spills_only_for_numbers_claimed_large` — R-55; asserts an unclaimed
  number truncates at 2 KiB and a claimed one does not.
- [ ] `parser::tests::osc_params_past_sixteen_join_into_the_last` — P9, with the OSC 4 shape.
- [ ] `parser::tests::dcs_streams_without_buffering`, `parser::tests::dcs_aborts_past_byte_cap`.
- [ ] `parser::tests::dcs_exit_via_esc_resets_intermediates` — trap 47.
- [ ] `parser::tests::c1_is_executed_not_an_introducer` — trap 48.
- [ ] `parser::tests::utf8_*` — the seven ported reference cases, each also byte-at-a-time.
- [ ] `parser::tests::print_runs_are_batched` — a 4 KiB ASCII run produces one `print_str`.
- [ ] `parser::tests::del_is_executed_not_printed` — P8.
- [ ] `parser::props::utf8_carry_does_not_swallow_the_next_character` — V6.
- [ ] `parser::props::a_split_c1_still_executes` — V8.
- [ ] `parser::tests::unhandled_sequence_is_counted_not_echoed` — R-35.
- [ ] `parser::tests::sync_mode_with_leading_param_is_recognised` — deviation P6.
- [ ] `strip::tests::removes_sequences_keeps_text`.
- [ ] `parser::props::arbitrary_bytes_never_panic_and_chunking_is_invariant`.

### The differential oracle (R-41)

> **Historical.** The oracle did its job through `US-0073`-`US-0086` and was **deleted with the
> fork at `US-0087`**: there is no vendored `vte` left to compare against, `crates/vt/tests/differential.rs`
> is gone, and the `vte` dev-dependency with it. The reasoning is kept because it is the argument
> for how a future oracle, against any reference, must be constructed.

The earlier design said "`vte 0.15` as a dev-dependency — the unmodified crates.io release, not
the fork", which was unobtainable: `Cargo.toml:243-244` patches `vte` workspace-wide, and `[patch]`
applies to every dependency kind including dev-dependencies, for exactly the window
`US-0073`-`US-0087` in which the oracle is the primary proof.

**The oracle compares at the raw state-machine level, and the vendored patches do not touch it.**
Verified against the patch series: `vendor/patches/vte/0001-OneTerm-fork-add-Handler-report_osc-single-pass-OSC-.patch`
and `vendor/patches/vte/0002-OneTerm-fork-forward-DCS-hook-put-unhook-to-Handler.patch` both
modify **only `src/ansi.rs`** — the semantic `Handler` layer. Neither touches `src/lib.rs` (the
`Parser` state machine, `Perform` trait and UTF-8 handling) or `src/params.rs`. The patched crate's
`vte::Parser` + `vte::Perform` are therefore byte-for-byte upstream, and a recording `Perform` that
logs `print` / `execute` / `csi_dispatch` / `esc_dispatch` / `osc_dispatch` / `hook` / `put` /
`unhook` is a pristine oracle even under `[patch]`.

No second vendored copy, no separate workspace, no `vte-oracle` package. The dev-dependency is
simply `vte = "0.15"`, resolving through the patch, and the test asserts at the `Perform` level.
A `US-0073` check re-runs the patch-scope verification (`grep '^+++' vendor/patches/vte/*.patch`)
so a future patch that reaches into `lib.rs` fails the oracle's own precondition instead of
silently weakening it. The oracle and the dev-dependency retired with the fork at `US-0087`.

`crates/vt/tests/differential.rs`, while it existed:

- [ ] `differential::action_traces_agree_on_ref_corpus` — all 45 vendored recordings through both
  state machines, normalising `print_str` runs to per-character `print` and applying the deviation table's
  exclusions.
- [ ] `differential::action_traces_agree_on_fuzz_seeds` — the same over the MIT-licensed Ghostty
  AFL++ seeds.
- [ ] `differential::chunk_splitting_is_invariant` — 1-, 7- and 64 KiB chunks produce one trace.
- [ ] `differential::oracle_precondition_patches_do_not_touch_the_state_machine` — a test that
  reads the patch files and asserts every `+++` line is `src/ansi.rs`.

Fuzzing is a scheduled Linux activity, not a packet exit criterion
([`testing-and-bench.md`](testing-and-bench.md), R-47).
