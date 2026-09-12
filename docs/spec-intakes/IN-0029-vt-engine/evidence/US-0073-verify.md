# US-0073 — parser core, verification evidence

Date: 2026-09-12
Branch: `worktree-agent-a450783bce74d5dc6` off `feat/vt-engine` @ `f3cf1a5`
Machine: Intel Core i7-12700, 31.7 GB RAM, Windows 11 build 26200, toolchain pinned by
`rust-toolchain.toml` (1.96.0).

## 1. The quality gate

```
pwsh scripts/ci-local.ps1
...
==> python scripts/verify-dependency-graph.py
Dependency graph policy passed for 20 workspace packages and 20 explicit members.
==> python scripts/check-doc-paths.py
Doc path check passed for 116 current paths in 10 documents.
==> python -m unittest scripts/test_check_english.py
Ran 2 tests in 0.013s / OK
==> python scripts/check-english.py
English contributor-text check passed for 662 files.
==> python scripts/completion-catalog.py validate
[completion-catalog] all catalogs valid
==> python scripts/third-party-notices.py --check
THIRD-PARTY-NOTICES.md is up to date.

ci-local: all checks passed.
```

Raw totals, summed over every `test result:` line of that run:

| Sections | Passed | Failed | Ignored |
| ---: | ---: | ---: | ---: |
| 53 | 1086 | 0 | 2 |

Three of those sections are new: `oneterm-vt` unit tests (**31 passed**), `tests/differential.rs`
(**5 passed**) and the crate's empty doc-test section. The two ignored tests are pre-existing
(`shapes_tests::shape_bitmaps_for_visual_review` and its sibling), not this packet's.

**Read the total against this packet's base, not against `US-0072`'s note.** That note records
1156 passed, measured on its own worktree off `852206d`; this run is off `f3cf1a5`, a different
base. Nothing here removes a test: the change is additive apart from four lines in the root
manifest and two in `scripts/dependency-graph-policy.json`.

`THIRD-PARTY-NOTICES.md` is unchanged because both new direct declarations (`memchr 2.8.2`,
`vte 0.15.0`) were already in `Cargo.lock` and the notices file lists what is reachable from
`oneterm-app`, which `oneterm-vt` is not yet.

## 2. The differential oracle

```
cargo test -p oneterm-vt --test differential
running 5 tests
test declared_osc_differences_are_exercised ... ok
test oracle_precondition_patches_do_not_touch_the_state_machine ... ok
test action_traces_agree_on_ref_corpus ... ok
test chunk_splitting_is_invariant ... ok
test action_traces_agree_on_generated_streams ... ok
test result: ok. 5 passed; 0 failed; 0 ignored
```

| Input | Volume | Result |
| --- | --- | --- |
| The 45 vendored alacritty recordings | every `recording` file, fed whole | identical action traces, **with no declared difference used at all** (the test asserts the filter counters stayed at zero) |
| Generated streams | 64 rounds x 4 families = 256 buffers: uniform noise (4 KiB), an escape-biased stream (4 KiB), a splice of two recordings, a recording with 32 random bit flips | identical apart from the declared differences below |
| Chunk invariance | all 45 recordings at 1 B, 7 B and 64 KiB | one trace per recording, unchanged |
| Oracle precondition | `vendor/patches/vte/*.patch` | every `+++` header is `src/ansi.rs`; the state machine, `Perform` and `params.rs` are untouched upstream |

### Declared differences, and how often each fired

Counted by the comparator and printed by `action_traces_agree_on_generated_streams`:

```
accepted declared differences: Filters { truncated_osc: 0, del_executed: 5747, joined_osc_params: 0 }
```

| # | Difference | Filter | Fired |
| --- | --- | --- | ---: |
| P1 | `print_str(&str)` instead of `print(char)` per character | the recorder splits each run back into characters | n/a (normalisation) |
| P2 | `memchr3(ESC, LF, CR)` instead of `memchr(ESC)` | none needed — the actions are identical, only the scan differs | 0 |
| P3 | the separator is kept per parameter | none needed — `Params::groups()` reproduces the reference's grouping exactly, including the 32-colon case | 0 |
| P4 | OSC truncated at 2 KiB / 8 MiB | a `truncated` OSC is accepted against the reference's unbounded one | 0 in the corpus and the generated streams; driven deliberately by `declared_osc_differences_are_exercised` |
| P5 | DCS capped at 16 MiB, and `dcs_unhook` carries an `aborted` flag | `Unhook` is compared without the flag (the reference has no such concept and reports a `CAN`/`SUB` abort through the same callback). The cap cannot fire on inputs this size and is proven by `dcs_aborts_past_byte_cap` | n/a |
| P6 | mode 2026 is an ordinary private mode | none needed — the reference buffers above the `Perform` level | 0 |
| P7 | APC is streamed to a sink | the recorder drops `apc_*`; the reference emits nothing for SOS/PM/APC | n/a |
| P8 | `DEL` is executed, not printed | `Execute(0x7F)` accepted against `Print('\u{7f}')` | **5747** |
| P9 | parameters past the sixteenth join into the last **with** their separators | only the first fifteen are compared once either side reports sixteen | 0 in the corpus; driven deliberately |

P8 and P9 are **not** rows of the LLD's P1-P7 table; both are recorded as packet deviations (V2
and V7) with the reasoning, because the LLD's own text asks for the behaviour that produces them.

## 3. Cap behaviour

| Test | What it proves |
| --- | --- |
| `osc_truncates_at_inline_cap_and_still_dispatches` | 4 KiB of payload under an unclaimed number stops at exactly `OSC_INLINE = 2048` stored bytes, sets `truncated`, and **still dispatches** |
| `osc_spills_only_for_numbers_claimed_large` | the same payload under a claimed number (52) arrives whole; under an unclaimed one (1337) it truncates |
| `osc_that_never_terminates_stays_bounded` | 8 MiB + 16 KiB fed into an OSC that never terminates: nothing dispatches until the terminator, the payload stops at exactly `OSC_LARGE = 8 MiB`, `truncated` is set. This is the defect being designed out (HLD P1) |
| `dcs_aborts_past_byte_cap` | 16 MiB + 16 KiB into a DCS: the sink receives exactly `DCS_MAX_BYTES`, the abort is reported **once**, and the real terminator does not report a second end |
| `osc_params_past_sixteen_join_into_the_last` | the seventeenth parameter onward joins into the sixteenth, separators included |

## 4. Adversarial input

`parser::parser_tests::props::arbitrary_bytes_never_panic_and_chunking_is_invariant` drives
**1 000 000** pseudo-random bytes (deterministic xorshift, seed `0x5EED_1234_ABCD_0001`, a third of
them biased towards `ESC` and the introducers) through one parser in 8 KiB chunks, then asserts a
64 KiB sample produces one action trace at chunk sizes 1, 7, 997 and 4096.

`crates/vt/fuzz/` carries the `cargo-fuzz` target the scheduled Linux job runs
(`cargo +nightly fuzz run parser -- -rss_limit_mb=512`). It is **not built here**: libFuzzer is
unusable on `x86_64-pc-windows-msvc` and the pinned toolchain is stable, which is why R-47 keeps
fuzzing off the packet's exit criteria. The test above is the Windows substitute.

One real defect was found by the chunk-invariance property and fixed: the reference consumes the
whole valid prefix of its carry buffer when the buffer holds a completed character plus a second
one plus an error, which drops the second character — `C5 93 | 40 97` loses the `@`. The parser
consumes only the first character's bytes instead. Regression:
`utf8_carry_does_not_swallow_the_next_character`.

## 5. Tier 1 — parser throughput

```
cargo run --release -p oneterm-tools --bin vt-bench -- fixtures --out <dir> --mib 100
ONETERM_VT_BENCH_FIXTURES=<dir> cargo test --release -p oneterm-vt --lib -- --nocapture bench_note
```

100 MiB per fixture, median of three passes, release profile, a `Dispatch` that does nothing.
Old-engine column from [`US-0072-bench-baseline.md`](US-0072-bench-baseline.md).

| Fixture | old MiB/s | new MiB/s | ratio | new ns/B |
| --- | ---: | ---: | ---: | ---: |
| `plain_ascii` | 1192.0 | 1028.5 | 0.86x | 0.93 |
| `long_lines` | 1345.7 | 1377.1 | 1.02x | 0.69 |
| `heavy_sgr` | 379.1 | 321.9 | 0.85x | 2.96 |
| `tui_redraw` | 447.9 | 419.2 | 0.94x | 2.27 |
| `scroll_region` | 815.0 | 880.1 | 1.08x | 1.08 |
| `cjk_wide` | 587.4 | 637.4 | 1.09x | 1.50 |
| `dense_cells` | 220.1 | 331.3 | 1.51x | 2.88 |
| `scrolling` | 752.1 | 1318.4 | 1.75x | 0.72 |
| `sixel` | 443.3 | 562.6 | 1.27x | 1.70 |
| `osc_9_7` | 65.9 | 508.2 | **7.71x** | 1.88 |

**Recorded, never gated** (R-29). Read every row next to the ConPTY transport ceiling: about
1.2 MiB/s for a `cmd.exe` producer and about 30 MiB/s for a DOOM-fire-class one, one to two orders
of magnitude below the slowest row here.

Three caveats, and they matter more than the numbers:

1. **The two columns do not measure the same thing.** The old tier 1 runs
   `Processor::<StdSyncHandler>::advance` over a `NullHandler`, which is the state machine **plus**
   `vte`'s `ansi.rs` semantic translation. The new column is the state machine alone into a no-op
   `Dispatch`; its `dispatch/` half arrives at US-0076. The honest reading is "the new state
   machine is in the same class as the old parse path", not "27 % faster".
2. **The baseline's own variance is up to 30 %** on a machine that is also building, so
   `plain_ascii` at 0.86x and `long_lines` at 1.02x are the same measurement.
3. The two rows that move beyond noise have mechanical explanations: `osc_9_7` because OSC payload
   accumulation is a write into an inline array instead of a `Vec` push per byte with no fast path,
   and `scrolling` because `memchr3` takes line feeds out of the per-character loop.

## 6. Gaps

- `cargo-fuzz` unrun here (above). Linux CI owns it; not a packet gate (R-47).
- `strip.rs` and `strip::tests::removes_sequences_keeps_text` are named in the LLD's verification
  list but live outside `parser/` and outside this packet's write scope.
- `docs/agents/dependencies.md` § 3 has no `oneterm-vt` row yet for `memchr` and the `vte`
  dev-oracle, and `docs/agents/structure.md` has no `crates/vt` row. The HLD assigns the first to
  US-0082 and the crate row belongs with whoever lands the rest of the crate.
- **The base branch moved while this packet was in flight.** `feat/vt-engine` now carries US-0071,
  US-0074 and US-0075, so `crates/vt/Cargo.toml` and `crates/vt/src/lib.rs` exist upstream. The
  union is four lines and is listed in the packet's Handoff.
- No `FeedStats` yet: truncation and abort are reported through the `Dispatch` arguments, which is
  where US-0079 will read them.
