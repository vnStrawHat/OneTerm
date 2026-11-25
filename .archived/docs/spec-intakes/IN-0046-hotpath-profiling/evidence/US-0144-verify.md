# US-0144 independent verification

- Date: 2026-09-25
- Commit under test: `e62c2898` (branch `perf/highlight-scan-scratch`, one commit on main
  `7d8de84b`)
- Packet: [`US-0144`](../US-0144-highlight-scan-scratch.md); intake [`IN-0046`](../IN-0046.md);
  owning doc [`terminal-semantic-highlighting.md`](../../../terminal-semantic-highlighting.md) § 7
- Host: Windows 11 Enterprise 10.0.26200, toolchain 1.96.0, worktree's own `target/`,
  `CARGO_BUILD_JOBS=2`, two other agents building in parallel throughout. No app instance was
  launched.

## Verdict: PASS (findings F1-F6, none blocking)

Classes are unchanged: the golden file was re-recorded here on main's scanner and is the
same blob as the committed one, and 60 adversarial lines recorded on main also pass on the
fix. The scratch clears between lines, the ASCII path equals the identity map, and both
off-by-one mutations of `char_index` fail the committed golden. The findings concern claims
and records. The remaining `regex` allocation is not "one per non-ASCII line": it is one per
fallback search, up to 114 on a 5,000-char CJK line. The committed corpus does not pin the
Unicode `\b` behaviour that the packet gives as the reason to keep that allocation. Two
secondary timing claims rest on one noisy run. The packet contains two backspace bytes.

## Evidence

### 1. Output identity

- Provenance. The packet (Plan, first item; Commands) and the commit message say the golden
  was recorded on main's scanner before the change. It is one commit, so git cannot show the
  order. Reproduction: `crates/highlight/src/scanner/mod.rs` and `crates/highlight/src/lib.rs`
  restored from `7d8de84b`, then
  `ONETERM_RECORD_SCAN_GOLDEN=1 cargo test -p oneterm-highlight --test scan_line_output`.
  `git hash-object --no-filters` of the re-recorded file = `47e969456682620053477347017001e3fa3fe149`
  = `git rev-parse e62c2898:crates/highlight/tests/scan_line_output.golden`. Byte-identical,
  138 lines.
- Adversarial corpus (local, not committed). Added 9 base lines, times the 6 variants, plus a
  5,000-char CJK line (the IN-0044 scan bound) times 6 variants: 198 hashes. Recorded on main's
  scanner, then checked on the fix: **pass**. The added base lines were:
  ASCII except one trailing `é`; `日` at index 0; combining `e\u{301}`; the ZWJ family
  `👨‍👩‍👧‍👦`; the astral `𝔘` glued to a date; ten spaces; a line with ESC, TAB, BEL, CR, NUL and DEL
  (`is_ascii` true); `$é ls`; `é$ ls`. Empty and all-space lines were already in the corpus. A
  lone continuation byte cannot be expressed, because the API takes `&str`.
- Mutations against the **committed** golden, one at a time: ASCII branch `(byte + 1).min(len)`
  fails; `byte.min(len - 1)` fails; the map branch `get(byte).map(|c| c + 1)` fails. The claim
  that an off-by-one fails the test holds.
- Equivalence by reading the code: main's map for an ASCII line is `[0, 1, ..., len]`, and the
  lookup past the end returns the last entry, `len`. `byte.min(len)` is the same function,
  including `""` (0).

### 2. Scratch semantics

- Local test: one `ScanScratch` across the sequence CJK, short ASCII, longer emoji+CJK, `ok`,
  a 100,000-char CJK line, short ASCII, `é` line, a 100,000-char ASCII line, CJK, `""`,
  `error`, then the same sequence reversed. The sequence ran under 4 profiles and 4 roles,
  and every scan was compared with a fresh `scan_line`: **pass**. No stale map entries: the map
  is cleared before every non-ASCII line, and an ASCII line leaves it empty, which switches
  `char_index` to the identity branch. After the 100k line, 100 short scans allocate 0 times.
- Capacity is kept, not shrunk. The scratch holds the longest line it has scanned: 4 B per
  char for `chars`, plus 8 B per UTF-8 byte for the map. The logical line is bounded by the
  viewport, because `class_rows_into` only walks `frame` rows, so a 250x60 all-CJK screen
  holds about 15k chars: about 60 KB plus 360 KB per view, retained. This is the same policy
  as the neighbouring `Scratch` buffers (`line_text`, `char_rows`, ...). Acceptable, and not
  stated in the packet.
- Ownership: `Scratch` is a field of the per-view render state (`render/state.rs`) and of
  `PlanCache` (`plan_cache.rs`). `SemanticOverlay::scan_into(&self, .., &mut ScanScratch,
  &mut Vec<u8>)` borrows both buffers mutably, so the borrow checker rules out re-entry
  and aliasing. The type is auto `Send`/`Sync` and is never shared across threads.

### 3. API

- `ScanScratch` is `pub` with private fields and only `#[derive(Default)]`. That is the
  smallest surface that lets a caller own it. There are no accessors, and no `Clone`/`Debug`.
- `scan_line_into` gained a parameter, which breaks callers. All three are in the workspace
  and were updated: `SemanticOverlay::scan_into` (and through it
  `row_plan::scan_logical_line`), `highlight-bench` `measure` and its test. The crate uses
  `publish.workspace = true`, like the rest of the internal crates. Only `oneterm-vt` has
  external consumers.
- `scan_line` (one throwaway scratch per call) has no production caller. It is called only
  from `scanner_tests.rs` and `tests/scan_line_output.rs`.

### 4. The remaining `regex` allocation

Measured with a counting allocator, after warm-up, per `scan_line_into` in output mode:

| line | allocs | bytes |
| --- | ---: | ---: |
| `日本語 error at /etc/hosts 中文 2026-09-22 192.168.0.1` | 1 | 16 |
| `é no digits here` | 1 | 16 |
| `é` | 0 | 0 |
| `é 00:1a:2b:3c:4d:5e` | 2 | 32 |
| `2026-09-22 10:00 日` | 2 | 32 |
| `日 00:1a:… ×3 MACs` | 2 | 32 |
| `plain ascii 2026-09-22` | 0 | 0 |
| 5,000-char CJK/date line (IN-0044 bound) | **114** | **1,824** |

Standalone regexes on the first line: the date/time regex with Unicode `\b` makes 1 alloc of
16 B. The same pattern with `(?-u:\b)` makes 0, and so does the pattern without `\b`. The MAC
regex (also Unicode `\b`) accounts for the second allocation on MAC lines. The claim "inside
`regex`, Unicode `\b` next to non-ASCII" holds. The claim "one per non-ASCII output-mode
line" does not (F2).

Alternatives: `(?-u:\b)` changes matches. Measured on text: `日本語2026-09-22` goes from no
match to `9..19`, `𝔘2026-09-22 10:00:00` from the time only to the whole date, `caféMon`
and `Jané` from none to `Mon`/`Jan`, and `é00:1a:2b:3c:4d:5e` from no MAC to a MAC. On the
committed 138-hash corpus, however, `(?-u:\b)` in both regexes **passes**. Only the extended
corpus fails it, at `𝔘2026-09-22 …` (F3). The `regex` crate has no lookaround, so an
"ASCII-only lookaround" is not available. Keeping the allocation is the right call for a
no-behaviour-change packet.

### 5. Measurements

- hotpath 2-tab TUI: not re-run. It needs six fat-LTO release builds plus 18 minutes of
  runs on a machine shared with two builders. Spot-check of `research/raw/us-0144/*.json`:
  - `scan_line_into`: before-time 77,127 calls, avg 2.92 µs, total 225.35 ms; after-time
    80,434, avg 1.88 µs.
  - Allocation counts: before-count total 319,913, avg 4; after-count total 1,607, avg 0.
  - Bytes: before-bytes 106.7 MB, avg 1.3 KB; after-bytes 1.1 MB, avg 14 B.
  - `class_rows_into` avg 141.28 → 92.73 µs.

  All match the packet. The scan improvement is consistent across all three builds: before
  2.10 / 2.69 / 2.92 µs, after 1.80 / 1.91 / 1.88 µs. The `class_rows_into` and
  `PlanCache::update` improvements are not consistent (F4).
- `highlight-bench`, release, paired in this worktree under the same load. Main ran twice
  (5 and 9 runs) and the fix three times (5, 5 and 9). Median ns/char per shape and length:

  | shape | chars | main | main2 | fix | fix2 | fix3 | main2→fix3 |
  | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
  | prompt | 80 | 3.54 | 3.58 | 2.11 | 2.04 | 2.83 | -21 % |
  | prompt | 2000 | 1.13 | 1.22 | 1.02 | 1.03 | 1.01 | -17 % |
  | plain | 80 | 12.26 | 12.71 | 10.35 | 10.82 | 9.71 | -24 % |
  | plain | 2000 | 8.95 | 9.04 | 8.37 | 8.44 | 8.26 | -9 % |
  | keyword-log | 80 | 15.45 | 15.15 | 14.64 | 12.68 | 12.14 | -20 % |
  | keyword-log | 2000 | 13.62 | 12.03 | 15.85 | 16.21 | 11.16 | -7 % |
  | cjk | 80 | 51.55 | 52.05 | 50.14 | 50.61 | 49.75 | -4 % |
  | cjk | 2000 | 68.67 | 71.30 | 70.87 | 70.58 | 71.81 | +1 % |
  | cjk | 8000 | 69.41 | 73.47 | 74.40 | 77.41 | 70.99 | -3 % |

  The ASCII shapes improve robustly. CJK shows no measurable change (F5). The committed
  baseline `crates/tools/highlight-bench-baseline.json` is **not gated**: the tool writes
  `"note": "Recorded, never gated"`, and nothing reads the baseline. Leaving it unrefreshed
  fails nothing. Against the baseline, the ASCII cells are faster now and the CJK cells
  slightly slower, which is load noise.
- `frame_time_under_output` (`fast-dev`), flood avg/p50/p95:
  - main: 1770/1654/2706 µs.
  - fix, two runs: 1816/1684/2816 and 1917/1655/3132 µs.

  p50 is unchanged, and the p95 difference is load noise. This agrees with the packet.

### 6. Records

- Packet: follows `docs/templates/work.md` (Status, Classification, Outcome, Scope,
  Acceptance, Documentation with Owning Docs Reviewed, Action and Reconciliation, Plan,
  Verification Plan, Proof block, Evidence and Gaps). Created 2026-09-25.
- `docs/terminal-semantic-highlighting.md` § 7 item 3 is updated, and so is the `IN-0046`
  candidate list. IN-0044's `US-0135` scan bounds (`class_scans`, `class_rows_scanned`) are
  unaffected, because the number of scans does not change.
- Proposed harness `story` row: see the report to the coordinator.

### 7. Gate

`CARGO_BUILD_JOBS=2 pwsh scripts/ci-local.ps1` (after deleting `target/release`), final line:
`ci-local: all checks passed.` (exit 0; the gate ran with this verification file present)

## Findings

- **F1 (low, records).** `US-0144-highlight-scan-scratch.md` lines 151-152 contain two literal
  U+0008 (backspace) bytes where `` `\b` `` was meant. They render as empty code spans ("a
  Unicode `` next to ..."). `check-english.py` does not catch control bytes. Fix: write
  `` `\b` ``.
- **F2 (medium-low, claim accuracy).** The remaining allocation is not bounded at one per
  non-ASCII output-mode line. `regex` boxes a 16 B `MatchError` once per search whose lazy
  DFA quits on a Unicode `\b`, for both the date/time **and the MAC** regex. The count is
  0 for `é`, 2 for a MAC or trailing-CJK line, and 114 (1.8 KB) per scan of a 5,000-char CJK
  line. That is still far below main, whose map alone for that line was about 120 KB. Four
  texts overstate it:
  - § 7 of the owning doc: "a steady-state scan allocates nothing";
  - the packet's Acceptance and Evidence;
  - the commit message;
  - the comment in `tests/scan_allocations.rs`.

  The test's exact `== other.len() * 6` also couples the gate to `regex` internals: a
  `cargo update` of `regex` can flip it either way. Fix: reword the texts as "one 16 B
  allocation per fallback search inside `regex`, date/time or MAC regex". Consider asserting
  `<=` or asserting only the ASCII zero.
- **F3 (low, test coverage).** The committed corpus does not pin the behaviour the packet
  cites for keeping the allocation. Both `\b` in `rules.rs` were swapped to `(?-u:\b)`, and
  all 138 committed hashes still pass. Fix: add a base line with a word char glued to a date,
  a month and a MAC, for example `日本語2026-09-22 caféMon é00:1a:2b:3c:4d:5e`. The line must
  be recorded on main's scanner, which is still possible, because the classes are unchanged.
- **F4 (low, measurement).** `class_rows_into` avg 141 → 93 µs (-34 %) and `PlanCache::update`
  598 → 518 µs (-13 %) each come from the single timing-build pair. The count and bytes
  builds of the same runs give `class_rows_into` before 117.5 / 92.2 µs vs after 93.9 /
  88.4 µs, and `update` before 532 / 435 µs vs after 514 / 487 µs. Those two rows are within
  run-to-run noise. Only the `scan_line_into` rows are robust.
- **F5 (low, measurement).** The packet's `highlight-bench` table shows CJK -5 % to -15 %.
  That is not reproduced under paired load: -4 % to +7 %, which is noise. The ASCII
  improvements reproduce: -7 % to -42 %. `keyword-log` at 2000 is noisy, at +16 % in two
  runs and -7 % in the third.
- **F6 (info).** Two small inconsistencies in the packet. Acceptance says "three profiles",
  but the corpus uses four (the allocation test uses three). Acceptance also says
  "bytes per call 0", while the table honestly shows 14 B avg.

## Gaps

- The hotpath TUI load was not re-run; its raw JSON was spot-checked instead.
- There are no quiet-machine timings. The bench baseline remains to be refreshed "as its
  own act", as the packet says.
- The adversarial corpus and the probes stay local. The probe source is kept in the session
  scratchpad, not in the repository.
