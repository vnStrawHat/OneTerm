# Verification: US-0086 — deferred deviations and extension-point hardening

Verifier: independent agent, 2026-09-13. Branch `worktree-agent-a0e574cb1cd61742f`
(`ae7ce2a`, `90300fa`, `b653f19`, `b550714`), four commits off `feat/vt-engine` `ee9057a`.
Worktree `.claude/worktrees/agent-a0e574cb1cd61742f`, its own `target/`; the main checkout was
read only. No app launch, no process touched. Free space on `D:` at start: **58.69 GB**.

**Verdict: merge.** Every acceptance line is reproduced from raw commands, including the headline
attribution claim, which was re-established by mutation independently of the implementer's run.
Two low-severity findings, both design-owner questions rather than defects against the written
contract, and one is already this packet's own Gap 1.

## Pass / fail

| # | Check | Result | Evidence |
| --- | --- | --- | --- |
| 1 | Scope | **PASS** | `git diff ee9057a...HEAD --stat`: 7 files — `crates/vt/src/{grid/grid_tests.rs, grid/screen.rs, terminal/dispatch.rs, terminal/mode.rs, terminal/osc.rs, terminal/terminal_tests.rs}` and the packet. **No `crates/terminal`, no `crates/tools`, no vendored file** |
| 1b | `OscClaims` additive for `US-0088` | **PASS** | `claim` / `claim_large` / `is_claimed` / `allows_large` signatures unchanged; the additions are `pub const NATIVE: [u32; 14]`, `pub fn is_native`, and a `debug_assert!` inside `claim`. `US-0088` claims `20308`, which is not native, and `crates/terminal/src/handle.rs:164-168` claims 7/9/133 + `claim_large(52)` — none trips the assertion |
| 1c | Trailers | **PASS** | all four commits carry `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` **and** `Claude-Session: https://claude.ai/code/session_01C1Vip1PKvU8ayf3PZ4jPM9` (`git log --format=%(trailers:key=...)`). The model name is the implementing agent's own, consistent across all four |
| 2 | `pwsh scripts/ci-local.ps1` | **PASS, exit 0** | all ten steps. **62 test-result sections, 1927 passed / 0 failed / 15 ignored** — `cargo test --workspace` 58 sections 1554/0/12, `cargo test -p oneterm-vt --features vt-paranoid` 4 sections 373/0/3. Identical to the packet's recorded totals |
| 2b | Parity gate | **PASS** | `vt-corpus check --engine new` → `45 recordings, 45 passed, 0 failed (New engine)`; `--dir crates/vt/tests/corpus/oneterm` → `1 recordings, 1 passed, 0 failed` (`ok sixel_basic`). No `expected-diffs.json` anywhere |
| 2c | `vt-diff` | **PASS** | corpus `45 recordings, 45 identical, 0 differing`; `--fixtures` `10 recordings, 10 identical, 0 differing` |
| 2d | Verifier families | **PASS, re-derived** | live: fam 14/6, micro 26/2, repro 8/6, combo 9/1, clean 5/5, clean2 3/7, widecase 4/0, zcase 4/0, fuzz 0/20. See the mutation table below |
| 2e | Recording risk | **PASS** | `vt-corpus grep-deviations` → `D12 … vttest_cursor_movement_1 (?45: 0 set, 1 reset); vttest_insert …; vttest_origin_mode_1 …; vttest_origin_mode_2 …; vttest_scroll …; vttest_tab_clear_set …` — six recordings, each 0 set / 1 reset. Risk none, as the packet says |
| 3 | Reverse-wrap semantics | **PASS with 2 notes** | probes below |
| 4 | Hardening | **PASS** | `Mode::inert_state`, `OscClaims::NATIVE`, the assertion, APC/SOS/PM — all checked against the source of truth, below |
| 5 | Gaps | **Accepted** | Gap 1 confirmed real and correctly left to the owner; Gap 2 correctly out of scope; the eight stale doc lines are listed and correct |
| 6 | Code quality | **PASS** | no `unsafe`, no `unwrap`/`expect`/`panic!` on any runtime path in the changed files (the one `expect` is in a test asserting a private mode has a number); no dead code — `is_native` is used by `claim` and the tests; fmt + clippy `-D warnings` green inside the gate |
| 6b | Packet and DB | **PASS** | every template section present, status `Implemented`, proof block `unit/integration/verify` ticked and `e2e/platform` honestly unticked. `harness.db` (gitignored, main checkout): `('US-0086', 'Deferred deviations and extension-point hardening', 'implemented', 'high_risk', '…/US-0086-deferred-deviations.md', 1, 1, 0, 0)` with 2 230 characters of evidence |

### The attribution claim, re-established independently

The packet says `fam` moved 15/5 → 14/6 and that `f08_mode45` alone accounts for it. Rather than
trust the recorded baseline, the mutation was re-applied here — the single read at the `BS` call
site (`crates/vt/src/terminal/dispatch.rs:844`) forced to `false`, nothing else changed — and every
family re-run:

| Family | Live (`HEAD`) | Mutated (D12 off) |
| --- | --- | --- |
| `fam` | 14 identical / 6 differing | **15 / 5** |
| `micro` | 26 / 2 | 26 / 2 |
| `repro` | 8 / 6 | 8 / 6 |
| `combo` | 9 / 1 | 9 / 1 |
| `clean` | 5 / 5 | 5 / 5 |
| `clean2` | 3 / 7 | 3 / 7 |
| `widecase` | 4 / 0 | 4 / 0 |
| `zcase` | 4 / 0 | 4 / 0 |
| `fuzz` | 0 / 20 | 0 / 20 |

And per recording: `vt-diff --dir fam --filter f08` gives `f08_mode45 … **7252 differences**` live
and `f08_mode45 … identical` mutated. **D12 alone moves exactly one recording**, in the family
dedicated to `? 45`, against a vendored reference that does not implement the mode. The by-design
list gains no new family; the working tree was restored with `git checkout --` and is clean.
(`fuzz` 0/20 and the other differing families are pre-existing `US-0076` divergences: the mutation
column proves this packet moved none of them.)

## Semantics probes (written for this review, run, then deleted)

Two untracked integration tests were added under `crates/vt/tests/`, run, and removed; the
worktree is clean (`git status --porcelain` empty).

| Probe | Input | Observed | Judgement |
| --- | --- | --- | --- |
| DECRQM real | `? 45 h` → `? 45 $ p` / `? 45 l` → `$ p` | `ESC [ ? 45 ; 1 $ y` / `; 2 $ y` | correct — the mode has a reader, so `Set` is honest |
| `RIS` | `? 45 h`, `ESC c`, `$ p` | `; 2 $ y` | correct (`grid_reset` assigns `Modes::default()`) |
| `DECSTR` | `? 45 h`, `CSI ! p`, `$ p` | `; 1 $ y` — **not** reset | correct: `? 45` is not in DECSTR's reset list (DEC STD 070 / xterm), and `dispatch-and-modes.md` § Reset does not list it either |
| Wrapped row | 4 cols, `abcde`, `CUP 2;1`, `? 45 h`, `BS` | row 0 col 3, and a following print overwrites the last glyph (`abcX`) | matches R-08 and xterm |
| Not wrapped | `ab CR LF`, `? 45 h`, `BS` | no move | matches R-08 |
| Screen top | `CUP 1;1`, `? 45 h`, `BS` | no move | matches R-08 — history is not addressable |
| Trap 1, mode off | default, `BS` at column 0 | no move, pending wrap preserved | unchanged |
| Pending wrap, last column | `? 45 h`, `abcd`, `BS`, `X` | `abXd` | trap 1's other half is mode-independent — correct |
| Pending wrap **at** column 0 | 1 column, `ab` (row 0 `WRAPPED`, cursor row 1 col 0 with pending wrap), `? 45 h`, `BS`, `X` | crosses to row 0 col 0, clears the pending wrap, `X` overwrites `a` | correct: the cursor really moved, so clearing is right |
| Alt screen | `? 45 h`, `? 1049 h`, `$ p` | `; 1 $ y`, and `BS` at col 0 of an unwrapped alt row does not move | correct — the mode is not per-screen |
| **Scroll region** | 4 cols, `abcde`, `CSI 2;3 r`, `? 45 h`, `BS` at the region top | **crosses to row 0 col 3 — above the top margin** | see F1 |
| **Scroll region + DECOM** | same, plus `? 6 h` and `CUP 1;1` | **crosses to row 0 col 3 — out of the origin region** | see F1 |
| **`CUB`** | `? 45 h`, `CSI 1 D` at column 0 | no move | see F2 |

## Hardening, checked against the source of truth

- **`Mode::inert_state` is complete against the mode table.** `Mode::PRIVATE` has 23 entries and
  `Mode::from_private` recognises exactly 23 codes (1, 3, 6, 7, 12, 25, 45, 47, 1000, 1002, 1003,
  1004, 1005, 1006, 1007, 1042, 1047, 1048, 1049, 2004, 2026, 2027, 9001) — no private mode is
  missing from the walk. The table in `dispatch-and-modes.md` § Modes marks exactly three private
  modes non-`real`: `? 3` (`NotSupported`), `? 2027` (`NotSupported`), `? 9001` (`Reset`). Those
  are exactly the three rows of `inert_state`, and `assert_eq!(inert, 3)` pins the count. `? 45`
  left the table in the same commit that gave it a reader. **Every implemented mode answers `Set`
  after `h`** — the walk drives `CSI ? n h` then `CSI ? n $ p` through a real `Terminal` and reads
  the wire bytes, so a dispatch path that ignored the table would fail.
- **`OscClaims::NATIVE` matches the dispatcher exactly.** `crates/vt/src/terminal/dispatch.rs:1218-1290`
  has arms for `0 | 2`, `4`, `8`, `10..=12`, `22`, `50`, `52`, `104`, `110`, `111`, `112` — the
  fourteen numbers in `NATIVE`, in order. `133` is correctly **not** native: it falls into the
  `other` arm, sets the shell mark and then still forwards through `is_claimed`. So the shadowing
  hole is closed for 8/52/4/10-12/104/110-112 and correctly left open for 133.
- **Duplicate `claim` idempotent**: the claim set is a bitmap; the test compares a
  double-claimed `OscClaims` against a single-claimed one for equality, which is stronger than
  asserting the bit.
- **`claim_large` on a native number** registers both bits by design (`crates/terminal` needs
  `claim_large(52)`), the delivery half is documented as inert, and `is_native` is the way to ask.
- **APC / SOS / PM** joined `unhandled_sequences_are_counted_not_echoed`; all three are counted,
  print nothing and produce no reply. `DEL` is still asserted as an execute no-op, not unhandled.
- **Should registration *error* on shadowing rather than debug-assert?** No.
  `dispatch-and-modes.md` § "OSC registration" states the house rule — *"A claimed number whose
  handler is missing is a debug assertion, never a panic"* — and this is its exact mirror. A
  `Result`-returning `claim` would break the `&mut Self` chaining that `handle.rs:164` and
  `US-0088` build on, i.e. exactly the "no existing signature moves" constraint the packet is
  under. The implementer's choice is the one the design asks for.

## Findings

| # | Severity | Finding | Fix |
| --- | --- | --- | --- |
| F1 | low | **Reverse wrap crosses above the scroll-region top margin, and out of the DECOM region.** `crates/vt/src/grid/screen.rs:662` guards only `screen_top()`. With `CSI 2;3 r` (and with or without `? 6 h`) a `BS` at the region's top-left crosses into row 0, a row the program has told the terminal it is not addressing. xterm's `CursorBack` confines reverse wrap to the top margin. This is **not** a violation of R-08 as written — R-08 names only `WRAPPED` and the screen top — so it is the design owner's call | one line in `Screen::backspace`: also refuse when `previous` is above the region top (`self.region.top` mapped through `row_of_index`), *or* a sentence in R-08 declaring the divergence. Needs a mode-plus-region recording to matter in practice; the 45-recording gate cannot see it (no recording sets `? 45`) |
| F2 | info | **`CSI D` (CUB) does not reverse-wrap** (`dispatch.rs:960` → `move_backward`). xterm applies reverse wrap to `CUB` as well as to `BS`. The engine matches the vendored reference and matches R-08's BS-only wording | no code change; one clause in R-08 saying reverse wrap is `BS`-only, so the next reader does not re-discover it |
| F3 | info | **Gap 1 (LNM) is real and correctly deferred.** Confirmed independently: `Mode::LineFeedNewLine` has no reader anywhere in `crates/` — the only hit outside `mode.rs` is `crates/tools/src/corpus_replay_new.rs:333`, the state dump. LNM's DECRQM therefore answers a bit nothing acts on, which is the shape the rule forbids for private modes. But the rule is written against private modes, the mode table explicitly marks LNM's DECRQM `real`, and LNM is an ANSI mode outside `Mode::PRIVATE`, so the walk cannot cover it without a new list. Leaving it to the owner is the right call | owner: either add `Mode::LineFeedNewLine` to `inert_state` and give the walk an ANSI list, or write down why an ANSI mode is exempt |
| F4 | info | "Real" in the mode table means *the state is reported honestly*, not *the engine acts on it*: `? 12`, `? 1004`, `? 1005`, `? 1007`, `? 1042` are read only through `Terminal::mode` / `ModeSnapshot`, i.e. by the embedder. That is a legitimate reader, and the packet's own Gap 4 already says the table's correctness stays a review question | none; recorded so the owner knows the walk's limit was checked, not assumed |

## The stale doc lines, for the design owner

The packet's Reconciliation table was checked line by line against the docs at `f9d790a` and is
**correct and complete**. Restated for convenience:

1. `dispatch-and-modes.md:166` — the `ReverseWrap` row still says "additive feature D12, `US-0086`"
   and "until the mode has a reader, `CSI ? 45 $ p` answers `Reset`". Both are now false; the rule
   sentence itself must stay, because `? 3`, `? 2027` and `? 9001` still follow it.
2. `dispatch-and-modes.md:393` — D table, D12: packet column → implemented; risk measured none.
3. `dispatch-and-modes.md` — C table, C13 / C14 / C15 still say "to measure in `US-0076`"; they
   were measured free (`crates/tools/src/corpus.rs:44-53`).
4. `dispatch-and-modes.md` § "OSC registration" — add `OscClaims::NATIVE` / `is_native` and the
   `claim`-on-native assertion.
5. `grid-and-scrollback.md:580` — "`? 45` is an additive feature and lands in `US-0086`": landed.
6. `grid-and-scrollback.md:664` — `grid::tests::reverse_wrap_crosses_a_wrapped_row` is no longer
   deferred; it exists and passes.
7. `testing-and-bench.md` — trap table row 13 cites "grid (G4, `US-0086`)"; there is no G4.
8. `IN-0029.md` — packet list, `US-0086`: tick it.

Two lines to add while reconciling: `f08_mode45 → D12` in the by-design differential list, and
F1 / F2 above if the owner accepts them as divergences rather than fixing them.

## Commands, verbatim

```
git diff ee9057a...HEAD --stat
git log --format='%h %(trailers:key=Co-Authored-By,valueonly) %(trailers:key=Claude-Session,valueonly)' ee9057a..HEAD
pwsh scripts/ci-local.ps1                                   -> exit 0, 62 sections, 1927/0/15
cargo run -q -p oneterm-tools --bin vt-corpus -- check --engine new                  -> 45/45/0
cargo run -q -p oneterm-tools --bin vt-corpus -- check --engine new --dir crates/vt/tests/corpus/oneterm -> 1/1/0
cargo run -q -p oneterm-tools --bin vt-corpus -- grep-deviations                     -> D12: six recordings, 0 set / 1 reset each
cargo run -q -p oneterm-tools --bin vt-diff -- --quiet                               -> 45 identical, 0 differing
cargo run -q -p oneterm-tools --bin vt-diff -- --fixtures                            -> 10 identical, 0 differing
cargo run -q -p oneterm-tools --bin vt-diff -- --dir <family>   x9, live and mutated -> table above
cargo test -p oneterm-vt --test us0086_probe{,2} -- --nocapture   (probes, since deleted)
```
