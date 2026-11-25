# Independent verification — US-0074 (cell, style and grapheme storage)

Verifier: independent agent. Date: 2026-09-12.
Subject: branch `worktree-agent-ae2a4340bbcef8125` @ `70d0b12`, worktree
`D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-ae2a4340bbcef8125`.
Base: merge-base with `feat/vt-engine` is `f3cf1a5`; `feat/vt-engine` has since moved to
`e8ecdef` (US-0071 merged). The three-dot and two-dot diffs are byte-identical, so the move
does not touch this packet.

Environment precondition: `Get-PSDrive D` → **76.98 GB free** (used 262.49 GB). Above the 15 GB
floor, so verification proceeded. All builds went to the worktree's own `target/`
(`CARGO_TARGET_DIR` was unset; the adversarial crate was built into
`…\agent-ae2a4340bbcef8125\target\adversarial`). The main checkout was read only.

**Verdict: merge after fixes.** One mandatory fix (commit trailer), no code blockers.

---

## 1. Pass/fail table

| # | Check | Result | Raw evidence |
| --- | --- | --- | --- |
| 1 | Diff scope | **PASS** | `git diff feat/vt-engine...HEAD --stat` = `git diff f3cf1a5..HEAD --stat` = 15 files, 2162 insertions, 6 deletions. Only `crates/vt/*` (7 new files + manifest), root `Cargo.toml` (+13), `Cargo.lock` (+12), `scripts/dependency-graph-policy.json` (+3/-1), `docs/agents/{structure,crate-dependency-rules,dependencies}.md`, and the packet. |
| 1b | No LLD/HLD/IN-0029 edits | **PASS** | The only `docs/spec-intakes/IN-0029-vt-engine/` path in the diff is `US-0074-cell-and-style.md`. `low-level-design/cell-and-style.md` at `f3cf1a5` is byte-identical to the copy on `feat/vt-engine` @ `e8ecdef` (`diff` → no output). |
| 2 | `pwsh scripts/ci-local.ps1` | **PASS** | `ci-local: all checks passed.` `[exited with code 0]`. All nine steps ran: fmt, clippy `-D warnings`, `cargo test --workspace`, dependency graph, doc paths, English unittest, English check, completion catalog, third-party notices. |
| 2b | Raw `test result:` totals | **PASS — packet's numbers confirmed exactly** | Aggregated over the CI log: **52 sections, 1188 passed, 0 failed, 5 ignored**. `oneterm-vt` unit binary: `test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s`. `Dependency graph policy passed for 20 workspace packages and 20 explicit members.` `Doc path check passed for 118 current paths in 10 documents.` `THIRD-PARTY-NOTICES.md is up to date.` |
| 3 | Bit layout conformance | **PASS** | `cell.rs:28-36` constants match the LLD field for field; `cell.rs:51` `const _: () = assert!(size_of::<Cell>() == 8);`. `cell_tests.rs:80-110 the_bit_layout_is_the_documented_one` pins each field's raw word (`1<<21`, `1<<22`, `1<<24`, `1<<26`, `1<<32`, `1<<48`) and asserts `full.to_bits() & (0b11111 << 27) == 0`. Two proptest properties re-assert the reserved-bit zero over random fills. |
| 3b | `Cell(0)` is blank | **PASS** | `cell_tests.rs:23-35`: `to_bits()==0`, content `Scalar(' ')`, `Narrow`, `Semantic::None`, not protected, `StyleId::DEFAULT`, `ExtrasId::NONE`, `is_blank()`, `is_erasable()`. No `has_extras` bit exists (R-34); `extras_id() != ExtrasId::NONE` is the test. |
| 3c | Accessors / `Style` / `Attrs` / `Color` | **PASS** | All nine LLD `impl Cell` entries present, plus `protected`, `with_width/semantic/protected/extras`, `to_bits`, `text_char`. `Attrs` has exactly the LLD's 14 flags + `ALL_UNDERLINES`; `Style{fg,bg,underline_color,attrs}`; `Color::{Named,Palette,Rgb}`; `NamedColor` carries the 29 keys. |
| 3d | Style/extras ladder | **PASS** | `intern.rs:121-143`: reuse → `entries.len() < 65_535` insert → id 0 + `warn` once + counter. `TABLE_LIMIT = 65_535` is the LLD's exact bound. No sweep and no renumbering path exists anywhere in the crate, so an id cannot move. |
| 3e | Grapheme arena | **PASS** | `GRAPHEME_MAX_LEN = 16`, `GRAPHEME_SWEEP_ENTRIES = 65_536`, `GRAPHEME_SWEEP_CHARS = 1<<20`; layout `chars: Vec<char>` / `spans: Vec<(u32,u8)>` / `index: FxHashMap<Box<[char]>,u32>` matches the LLD verbatim. GC by remap (`sweep` → `GraphemeRemap`) is implemented and content-preserving. Overflow path (a fourth step) noted in §3, F5. |
| 3f | Hyperlink table | **PASS** | `intern.rs:377-407`: `next_implicit: u32` on the table instance, no atomic and no global anywhere in the crate. |
| 3g | Width rules | **PASS** | `scalar_width` = `UnicodeWidthChar::width` clamped to 2; `cluster_width` implemented and unit-tested; the ConPTY glyph-width axis appears only as a `width.rs:16-19` doc note, exactly as documented, with no code. |
| 3h | Debug integrity assertions (O(1) tier) | **PASS** | `InternTable::debug_assert_integrity` (index/entries length agreement, id 0 never overwritten) fires on every insert; `GraphemeArena::insert` asserts index/spans agreement and that the span lies inside the arena; `Cell::with_content` asserts a grapheme id inside the content bits; `cluster_width` debug-asserts its input is one cluster. Matches `testing-and-bench.md` R-28's O(1) budget; the full two-screen walk correctly waits for a screen. |
| 4 | Tests: LLD verification list | **PASS (18/21, 3 re-homed with cause)** | Every LLD row exists except `wide_char_at_last_column_wrap_on_and_off`, `insert_mode_over_wide_char_repairs_the_pair` and `zero_width_at_column_zero_attaches_to_column_zero`, all three re-homed to US-0075. See §3, F4. |
| 4b | Exhaustion tests are real | **PASS (styles) / PASS-with-injection (arena)** | `style_ids_never_change_once_assigned` interns **70 000 distinct RGB styles**, asserts `entries() == 65_535`, that exactly `70_000 - 65_534 = 4 466` fell back to id 0, and then re-resolves every issued id to the value it was issued for — then interns 100 more and re-checks. `style_table_exhaustion_…_logs_once` fills the table, drives 10 fallbacks and asserts **one** captured `log::warn` through a real `log::Log` implementation. The arena's 21-bit exhaustion uses the crate-private `with_id_limit(4)` rather than 2 097 152 real clusters; `grapheme_sweep_trigger_is_the_documented_constant` pins that the production constructor uses `CONTENT_LIMIT == 2_097_152`. Acceptable trade for a CI gate. |
| 4c | proptest round-trips | **PASS** | Three properties (`packed_scalar_cells_round_trip`, `packed_grapheme_cells_round_trip`, `writing_one_field_never_disturbs_another`) run inside the 32-test suite; `proptest` is `[dev-dependencies]` only (`cargo tree -p oneterm-vt -e dev` shows it under `[dev-dependencies]`, absent from `-e normal`). |
| 4d | Width matrix (verifier's own) | **PASS** | See §2. ZWJ family / skin tone / flags / CJK = 2; combining mark = 1; lone ZWJ = 0; a 17-codepoint ZWJ cluster = 2; empty = 0. |
| 5 | No `unsafe` | **PASS** | `Select-String unsafe` over `crates/vt/src` → no match. |
| 5b | No `unwrap`/`expect`/`panic!` outside tests | **PASS** | Only five `unwrap_or` sites (`cell.rs:143,253`, `intern.rs:293,354`, `width.rs:67`), all infallible-by-construction fallbacks with a comment. No bare `unwrap()`, `expect(`, `panic!`, `todo!`. |
| 5c | Dead code / `#[allow]` | **PASS** | No `#[allow]` anywhere in `crates/vt`; clippy `-D warnings --all-targets` is clean, so no unused private item survives. |
| 5d | Module layout / test placement | **PASS (with a style note)** | `lib.rs` is declarations + re-exports only. Three concept files, no folder-with-one-file (R-49). Tests live in sibling `*_tests.rs`, which is `code-style.md`'s pattern — see §3, F8 on the `#[path]` spelling. |
| 5e | Public surface | **PASS** | `lib.rs` re-exports exactly the set `events-and-api.md` requires (`Cell, CellContent, CellWidth, Color, NamedColor, Style, Attrs, Rgb`) plus `Semantic`, the ids, `Interner`, `Extras`, `GRAPHEME_MAX_LEN`, and the two width functions. `GraphemeArena::with_id_limit` and `InternTable::new` are private — no visibility was widened for testing. |
| 5f | No new external dependency | **PASS** | `Cargo.lock` diff is **+12 lines, −0**: the `oneterm-vt` package entry alone. No new third-party package resolved. `cargo tree -p oneterm-vt -e normal` → `bitflags 2.13.0`, `log 0.4.32`, `rustc-hash 2.1.2`, `unicode-segmentation 1.13.3`, `unicode-width 0.2.2`; **no `oneterm-*`, no `gpui*`** (R6/R7 satisfied). Versions match `high-level-design.md:113-122` exactly. |
| 6 | Performance sanity | **PASS** | Release: **12.0 ns/op** for 600 000 style lookups over a 60 000-entry table; **8.1 ns/op** for 50 000 grapheme re-interns over a 50 000-entry arena. Debug: 94.8 / 80.1 ns/op. Flat in table size — hash lookup, not a linear scan. |
| 7 | Packet completeness | **PASS** | All 12 `docs/templates/work.md` sections present, in order. Every Acceptance tick is backed by a named test I located and ran. The Gaps section is honest and, unusually, self-incriminating (it records the disk-full re-runs). |
| 7b | `harness.db` row | **PASS** | Read-only `sqlite3` on the main checkout: `story` row `US-0074`, status `implemented`, `unit_proof=1`, `last_verified_result='pass'`, `intake_id=34`, `contract_doc` and `packet_doc` both correct, and the evidence blob's raw totals (52 / 1188 / 0 / 5) match my own CI run exactly. |
| 7c | Commit trailer | **FAIL** | See §3, F1. |

---

## 2. Verifier's own adversarial tests (scratchpad, not committed)

`…\scratchpad\adversarial\tests\adversarial.rs`, seven tests against the worktree crate as an
external consumer. **7 passed in both debug (`debug_assert!`s live) and release.**

```
running 7 tests
lone RI cluster_width = 1
handshake cluster len = 7
test a1_width_matrix ... ok
test a2_grapheme_cap_boundary_and_prefix_collision ... ok
test a3_sweep_survives_unissued_and_duplicate_ids ... ok
test a4_predicate_corners ... ok
test a5_wide_repair_last_column_clears_an_unrelated_neighbour ... ok
600 000 style lookups over a 60 000-entry table: 7.2074ms (12.0 ns/op)
needs_sweep fired at entries=65536 chars=1048561 (GRAPHEME_SWEEP_CHARS=1048576)
test a7_chars_trigger_can_never_fire_before_the_entries_trigger ... ok
50 000 grapheme re-interns over a 50 000-entry arena: 402.6µs (8.1 ns/op)
test a6_interner_lookup_is_not_a_linear_scan ... ok
test result: ok. 7 passed; 0 failed; 0 ignored
```

What each probed, and what it found:

- **A1 width matrix.** All the brief's cases hold. Two values the LLD does not pin, recorded
  here so US-0075 inherits them: a **lone regional indicator** measures **1** (one RI is not a
  flag, so the rule falls through to the scalar width), and a **17-codepoint ZWJ cluster**
  measures **2** — `cluster_width` has no length ceiling of its own, the 16-codepoint cap is
  purely a storage rule. Both are defensible; neither is written down.
- **A2 cap boundary.** 16 codepoints is kept whole with `truncated() == 0`; 17 truncates,
  counts, and **collides with the 16-codepoint cluster's id**. Two visibly different clusters
  sharing a 16-codepoint prefix therefore render identically and silently. That is the
  design's stated "truncated, not rejected" choice, and the counter is the observability —
  no finding, but US-0075 should read the counter into `FeedStats` as planned.
- **A3 hostile sweep.** Duplicate ids, id 0, and an id **outside the id space entirely**
  (`GraphemeId(9_999_999)`) all survive: the unissued id maps to 0, never to another cell's
  cluster. This is the failure mode `high-level-design.md` risk 16 calls "a missed live
  reference corrupts text silently", and it degrades correctly.
- **A4 predicate corners.** Found the `is_blank()` behaviour in F3 below.
- **A5 trap 6 aliasing.** Confirmed the last-column `Wide` fallback clears an **unrelated**
  left neighbour's glyph. This faithfully reproduces `term/mod.rs:1007-1024`
  (`if wide && col < cols-1 { clear right } else if col > 0 { clear left }`), so it is correct
  as parity — recorded so it is not later mistaken for a bug.
- **A6 timing.** Numbers above; no O(n) scan.
- **A7 sweep trigger.** Found F2 below.

---

## 3. Findings

### F1 — Commit trailer is the wrong co-author line — **MAJOR** (mandatory before merge)

`70d0b12` ends with:

```
Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_018tAaHVGPnSzrEHLVcg3sx1
```

The project requires `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. The
`Claude-Session` line is correct. The commit is unpushed and unmerged.
**Fix:** `git commit --amend` the trailer on the worktree branch before merge. Nothing else in
the message needs changing — the body is accurate and well-scoped.

### F2 — `GRAPHEME_SWEEP_CHARS` is unreachable: it can never be the trigger that fires — **MINOR (design/LLD)**

`intern.rs:298` — `spans.len() >= GRAPHEME_SWEEP_ENTRIES || chars.len() >= GRAPHEME_SWEEP_CHARS`.

With `GRAPHEME_MAX_LEN = 16`, `chars ≤ 16 × entries`, so `chars ≥ 1 048 576` requires
`entries ≥ 65 536` — which is already the first clause. Proved empirically by A7: filling the
arena with the longest clusters the cap allows fires `needs_sweep()` at
**entries = 65 536, chars = 1 048 561** — fifteen codepoints short of the second trigger, at
its theoretical maximum. The second clause is dead code in the mathematical sense.

The implementation is faithful to the LLD; the **LLD is what is wrong**, twice: it says both
constants "have a test that drives them", and no such test can exist for the chars clause.
**Fix (LLD, design agent):** either lower `GRAPHEME_SWEEP_CHARS` to something reachable
(256 KiB makes it a genuine second trigger for long-cluster streams) or delete the clause and
the "both have a test" sentence. `intern_tests.rs:293` currently only pins the constant's
*value*, which is the honest thing to do given the above.

### F3 — `is_blank()` is true for a default-styled wide spacer — **MINOR (doc)**

`cell.rs:214-218`. `is_blank()` checks content, style and extras but **not width**, so
`Cell::EMPTY.with_width(CellWidth::WideSpacer).is_blank() == true` while
`cell != Cell::EMPTY`. This is *literally* the LLD's table row ("content is `' '` **and**
`style_id == 0` **and** `extras_id == 0`"), and it is harmless for the stated use — a spacer
paints nothing, so a renderer fast path that skips it is right. But the doc comment calls
`is_blank()` the answer to "is there provably nothing to draw?" **and** the LLD lists
`Cell::EMPTY` comparisons as a caller, and those two readings disagree on exactly these two
shapes. `WideSpacer`/`LeadingWideSpacer` are the only cells where `is_blank() && !is_erasable()`.
**Fix (one line, in the packet's crate):** extend the `is_blank` doc comment to say it ignores
width by design and that a caller wanting "identical to a fresh cell" must use `== Cell::EMPTY`.
No behaviour change.

### F4 — Traps 5/7/8 and half of trap 6 are re-homed, but two design docs still name `cell::tests::` for them — **MINOR (LLD), justified deviation**

The packet's deviation 3 is **correct and well-argued**: all three tests exercise the print
path (wrap flag, insert-mode row shift, cursor), none of which exists in a storage-only packet.
The cell-level halves that *can* be tested without a grid **are** tested
(`wide_pair_repair_on_overwrite` covers trap 6's two same-row sub-cases via
`repair_wide_pair_in_row`; `width_enum_covers_every_wide_pair_shape` covers the shapes).

But the re-homing is recorded only in this packet. Two design documents still assign them here:

- `low-level-design/cell-and-style.md` Verification list, three rows.
- `low-level-design/testing-and-bench.md` trap map: rows 5, 6, 7, 8 all say owning LLD
  `cell-and-style`, test `cell::tests::…`.

**Fix (LLD, design agent):** move those four test names to `grid::tests::` / US-0075 in both
documents, keeping trap 6's same-row half where it is. While there, note a pre-existing
contradiction this packet did not create: `testing-and-bench.md` names trap 7's test
`insert_mode_over_wide_char_leaves_orphan_spacer` while `cell-and-style.md` names it
`insert_mode_over_wide_char_repairs_the_pair` — correction C4 changed the behaviour and only
one document followed.

### F5 — The grapheme arena's fourth ladder step is right, and belongs in the LLD — **MINOR (LLD), justified deviation**

Packet deviation 4. The LLD's only answer to arena growth is `Terminal::sweep_graphemes`, which
does not exist yet, so without the implementer's addition a full 21-bit id space would wrap or
panic on untrusted input — precisely what `error-policy.md` forbids. `intern.rs:255-265` falls
back to a reserved id-0 cluster, counts it, and warns once: the same shape as the style ladder's
step 3, and exercised by `grapheme_arena_exhaustion_falls_back_to_a_blank_and_logs_once`.
**Fix (LLD):** add the fourth step to the arena section. The code is right as it stands.

### F6 — `is_erasable(&Interner)` and `text_char(&GraphemeArena)` — **MINOR (LLD), deviations are correct**

Packet deviations 1 and 2. The LLD's `is_erasable(self, ex: &ExtrasTable)` **cannot** evaluate
its own stated rule — "default fg/bg, none of `INVERSE`, any underline, `STRIKEOUT`" lives in
the `Style` behind `style_id`, not in the extras table. Likewise `text_char(self) -> char`
cannot answer for a grapheme cell, whose content bits hold an id. Both signatures are the only
implementable readings. **Fix: the LLD, not the packet** — update both lines in the Interfaces
block.

### F7 — `unicode-width` is `0.2.x`, not "2.x" — **MINOR (LLD)**

Packet deviation 7 is right and the packet is the document that is correct.
`high-level-design.md:113` already says `0.2.x`, `Cargo.lock` resolves `0.2.2`, and no 2.x
release exists. **Fix:** one word in `cell-and-style.md`'s Width section.

### F8 — `#[path = "cell_tests.rs"] mod tests;` vs `code-style.md`'s `mod cell_tests;` — **MINOR (style, informational)**

`cell.rs:409-411`, `intern.rs:467-469`, `width.rs:84-86`. `code-style.md` § Testing spells the
sibling-file pattern as `#[cfg(test)] mod workspace_tests;`, which would give test paths
`cell_tests::…`. The implementer used `#[path]` so the paths stay `cell::tests::…`, which is
what the LLD's verification list and `testing-and-bench.md`'s trap map name — and each of the
three sites carries a comment saying exactly that. The deviation is deliberate, documented and
self-consistent; I would leave it and, if anyone cares, add the `#[path]` variant to
`code-style.md` as the escape hatch for when a design doc pins test paths.

### F9 — Smaller notes, none blocking

- `intern.rs:151` — the "unreachable" resolve fallback is `&self.entries[0]`, an index. It is
  unreachable (the constructor always pushes, nothing ever removes) but it is the one
  potentially-panicking expression in the crate. `self.entries.first().unwrap_or(&T::default())`
  does not typecheck against the `&T` return; a `debug_assert!(!self.entries.is_empty())` above
  it would document the invariant at zero cost. Optional.
- `HyperlinkTable` is the one table with **no** ladder: an implicit OSC 8 link is one entry per
  occurrence and its `index` map grows alongside `entries`, so a stream of un-`id=`-ed links
  grows memory without bound. The packet declares this as a gap and assigns the cap to US-0076
  (which owns OSC 8, RIS and reset). Correctly scoped, but it is the most substantive risk this
  packet leaves open — US-0076 must not drop it.
- `unicode-segmentation` is a **normal** dependency whose only production use is the
  `is_at_most_one_cluster` helper inside `cluster_width`'s `debug_assert!` (`width.rs:77-80`).
  In a release build nothing calls it. It is correct to declare it now (the mode 2027 print path
  is its real consumer, per the HLD's table), so: leave it, do not move it to dev-dependencies.
- Doc comments cover 17/30 public items in `cell.rs`, 30/45 in `intern.rs`, 2/2 in `width.rs`.
  The undocumented ones are trivial accessors and enum variants under a well-documented type;
  no `missing_docs` lint is enabled in `[workspace.lints]`. Nothing to fix.
- `crates/vt/tests/` holds only the 2.8 MiB `corpus/` directory and **no top-level `.rs`**, so
  turning `crates/vt` into a real package does not accidentally compile the corpus as
  integration tests. Verified.
- The LLD derives `Default` on `Style`; the implementation writes a manual `impl Default`
  returning `Style::DEFAULT` (fg/bg are `Named`, which `derive` cannot produce). Necessary and
  correct — `InternTable::new` relies on `T::default()` being the id-0 value.

---

## 4. LLD-vs-packet reconciliation list

| # | Item | Who is right | Change |
| --- | --- | --- | --- |
| 1 | `is_erasable(self, ex: &ExtrasTable)` | **Packet** (`&Interner`) | Fix `cell-and-style.md` Interfaces |
| 2 | `text_char(self) -> char` | **Packet** (`&GraphemeArena`) | Fix `cell-and-style.md` Interfaces |
| 3 | Traps 5/7/8 + cross-row half of 6 owned here | **Packet** (re-home to US-0075) | Fix `cell-and-style.md` Verification **and** `testing-and-bench.md` trap map rows 5-8; also resolve the trap-7 test-name contradiction between the two docs |
| 4 | Arena has only three ladder steps | **Packet** (a fourth is required) | Add step 4 to `cell-and-style.md` Graphemes |
| 5 | Counters on `FeedStats` | **Packet** (they live on the tables until `FeedStats` exists at US-0079) | No doc change needed; the packet's Handoff already carries it |
| 6 | `unicode-width` "2.x" | **Packet** (`0.2.x`) | Fix `cell-and-style.md` Width; HLD is already right |
| 7 | No micro-benchmark | **Packet** | None. The LLD asks for none and `testing-and-bench.md` has no tier for it; my A6 timing (12 ns/op release) is enough to rule out the failure mode a benchmark would catch |
| 8 | "Both sweep constants have a test that drives them" | **Neither** — the chars clause is unreachable (F2) | Fix `cell-and-style.md`: lower `GRAPHEME_SWEEP_CHARS` or delete the clause and the claim |
| 9 | "warn **once per session**" | **Packet** (once per table instance) | Optional wording fix in `cell-and-style.md`; per-terminal matches the `HyperlinkTable` reasoning in the same document |
| 10 | `#[derive(Default)]` on `Style` | **Packet** (manual impl) | Cosmetic; the LLD snippet is illustrative |

Note for the design agent: items 1, 2, 4, 6 and 8 are corrections the *implementation* proves,
not preferences. Items 3 and 9 are bookkeeping.

---

## 5. Verdict

**Merge after fixes.**

The code is the strongest part of this packet. The bit layout is exact, the ladders are the
LLD's ladders, an id demonstrably never moves under 70 000 distinct styles, the GC is
content-preserving under a 2 000-cluster stream and degrades safely under three separate
hostile inputs I invented, interning is constant-time, there is no `unsafe`, no `unwrap`, no
`#[allow]`, and the whole crate is a leaf with zero new packages in `Cargo.lock`. All nine
CI steps are green and the packet's raw numbers reproduce to the digit.

Blocking before merge:

1. **F1** — amend the commit trailer to `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.

Recommended in the same pass (one line of code, no behaviour change):

2. **F3** — extend the `is_blank()` doc comment to say it ignores width.

Owned by the design agent, must not be lost (they are LLD corrections, not packet rework):

3. **F2, F4, F5, F6, F7** and the §4 table — in particular F2, which is a genuine hole in the
   design that only implementation exposed.
