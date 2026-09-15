# Independent verification: US-0099 (key and mouse encoding move into `oneterm-vt`)

Intake: IN-0038 (high risk: public contract)
Packet: [`../US-0099-input-encoding-moves-in.md`](../US-0099-input-encoding-moves-in.md)
Branch under test: `feat/vt-input-encoding` @ `c135d40`, base `main` @ `0558fa2` (confirmed:
`git merge-base HEAD main` = `0558fa2`)
Verifier worktree: `.claude/worktrees/agent-ab2654ece2ab0655d`
Date: 2026-09-15

## Verdict

**PASS-WITH-NOTES.**

Every load-bearing claim holds and the central one -- "not one byte that reaches a PTY changes" --
is now proved far more strongly than the packet proved it: 19 200 000 byte comparisons against the
actual `main` implementation, zero mismatches (§ 2). The notes are one design deviation from an
accepted LLD whose stated justification does not survive contact with a sibling packet (F1), one
doc that was not reviewed (F7), six small record inaccuracies (F2-F6, F8), and one informational
note for the embedder guide (F9).

Nothing found here changes bytes on the wire. Nothing found here is a merge blocker on its own; F1
is worth settling before the first tag anything outside the repository pins, because after that it
costs a minor bump.

## 1. Every production hunk outside `crates/vt/src/input/`

`git diff 0558fa2..HEAD` touches 24 files (21 under `crates/`). Outside the new module, in full:

| File | What changed | Verdict |
| --- | --- | --- |
| `crates/vt/src/lib.rs:24,37-39` | `pub mod input;` plus the "four modules, not three" comment | correct |
| `crates/vt/src/terminal/mod.rs:44` | `use crate::input::{KeyMods, KeySpec};` | correct |
| `crates/vt/src/terminal/mod.rs:363-370` | new `Terminal::encode_key`, three lines, delegates to `crate::input::encode_key(key, mods, &self.mode_snapshot())` | correct; § 3 |
| `crates/vt/src/terminal/terminal_tests.rs:15,668-682,686` | `use crate::input::NamedKey`, the new `encode_key_reads_the_terminals_own_decckm`, one stale comment fixed | correct |
| `crates/terminal/src/lib.rs:20,24,47-52` | `pub(crate) mod key_encode` / `pub mod mouse_encode` deleted; re-export block now `pub use oneterm_vt::input::{...}` | correct; see **F6** |
| `crates/terminal/src/model.rs:18-22` | import path only | correct |
| `crates/terminal/src/session.rs:24,29` | import path only | correct |
| `crates/terminal/src/test_support.rs:22,28` | import path only | correct |
| `crates/local-shell/src/session_tests.rs:6` | `oneterm_terminal::mouse_encode::{..}` -> `oneterm_terminal::{..}` | correct |
| `crates/terminal-view/src/input/keys.rs:10,303,306` | `ModeSnapshot` added to the `use`; `send_key`'s third parameter `app_cursor: bool` -> `modes: &ModeSnapshot` | correct |
| `crates/terminal-view/src/input/keys_tests.rs` (13 lines) | call sites pass `&ModeSnapshot::default()` | correct |
| `crates/terminal-view/src/render/frame.rs:19-21,560-563,966` | `Frame::app_cursor() -> bool` becomes `Frame::modes() -> ModeSnapshot` | correct; see below |
| `crates/terminal-view/src/terminal_view/input.rs:111-113` | reads `frame.modes()` and passes `&modes` | correct; see below |

38 changed lines across four `terminal-view` files, exactly as the packet says.

**The one real risk in this diff, checked.** `terminal_view/input.rs:112` changed from
`let app_cursor = self.render_state.borrow().frame.app_cursor();` to
`let modes = self.render_state.borrow().frame.modes();`. If `modes()` returned a reference, the
`RefCell` borrow would now be live across the `session.update(cx, ..)` inside `send_key` and
re-entrant access would panic at runtime -- invisible to every unit test. It does not:
`ModeSnapshot` is `#[derive(Clone, Copy)]` (`crates/vt/src/render/modes.rs:45`) and
`Frame::modes()` returns it by value, so the borrow ends at the semicolon exactly as before. No
new panic path.

**Crate boundary.**

- `crates/vt` gained no dependency: `git diff 0558fa2..HEAD -- crates/*/Cargo.toml` is empty.
  `cargo tree -p oneterm-vt -e normal` is **7 lines / 6 leaves** (`bitflags 2.13.2`, `log 0.4.34`,
  `memchr 2.8.2`, `rustc-hash 2.1.2`, `unicode-segmentation 1.13.3`, `unicode-width 0.2.2`),
  byte-identical to the packet's paste.
- `cargo tree -p oneterm-terminal -e normal --depth 1` unchanged (`async-channel`, `base64`,
  `chrono`, `log`, `oneterm-core`, `oneterm-vt`, `parking_lot`, `serde`, `serde_json`).
- `grep -rn "gpui\|oneterm_core\|oneterm-core\|oneterm_terminal" crates/vt/src crates/vt/Cargo.toml
  crates/vt/README.md` -> **no matches**. R7 holds.
- The GPUI mapping stayed in the adapter: `crates/terminal-view/src/input/keys.rs:251-290` still
  maps `Keystroke` strings to `KeySpec`/`NamedKey`, and `mouse.rs` still owns scroll, selection and
  shift tracking. `crates/vt/src/input/` contains no platform type.

## 2. Byte equivalence, verified independently

The packet declines the acceptance criterion's cross-product test on the grounds that a second copy
of the encoder taken from the same source "proves the copy, not the behaviour". That reasoning is
wrong for the question actually being asked. The claim under test is *the move did not alter the
function*, and the reference implementation for that claim is the file on `main` -- which exists,
is immutable, and had **zero imports**. Comparing the moved function against it is exactly the
right test, and it is the test the LLD's Verification block asks for.

So it was run.

**Method.** `scripts/gen_equiv.py` (verifier scratchpad) extracts the production half of
`git show main:crates/terminal/src/key_encode.rs` and `mouse_encode.rs` -- everything before
`#[cfg(test)]` -- and pastes it, byte for byte and never retyped, into `mod orig_key` and
`mod orig_mouse` of a generated integration test. `key_encode.rs` has no imports at all;
`mouse_encode.rs` imports only `oneterm_vt::{ModeSnapshot, MouseEncoding}`, which resolve unchanged
inside a `crates/vt` integration test. The generated test then compares the two implementations over
a cross-product. Variant sets are pinned by two **exhaustive** `match` functions in each direction
(new -> old and old -> new), so a variant added or dropped by the move is a compile error, not a
silent skip.

**Test file:** `crates/vt/tests/zz_verify_us0099_equiv.rs` (721 lines, generated; parked at
`<scratchpad>/zz_verify_us0099_equiv.rs` so the local CI run below is not contaminated).

| Axis | Values |
| --- | --- |
| `KeySpec` | 75 -- all 38 `NamedKey` variants, plus 37 `Character` payloads: the whole xterm Ctrl table (`a A z Z 0 1 9 SPACE 2 @ 3 [ 4 \ 5 ] 6 ^ 7 _ / 8 ? , . ~ DEL`), `NUL`, the empty string, a combining pair `e+U+0301`, precomposed `U+00E9`, a 4-byte `U+1F600`, `U+10FFFF`, two-char `ab`, two-CJK `U+4E2D U+6587`, `\r\n`, `\t` |
| `KeyMods` | all 8 combinations of shift/ctrl/alt |
| `ModeSnapshot` | all 1 280 -- 2^7 boolean combinations (`app_cursor`, `alt_screen`, `app_keypad`, `bracketed_paste`, `show_cursor`, `insert`, `alternate_scroll`) x 10 mouse states (`None`, plus 3 `MouseReporting` x 3 `MouseEncoding`) |

> Note on the brief's kitty / `modifyOtherKeys` axes: `ModeSnapshot` carries neither field, so they
> are not reachable through `encode_key`'s signature. They are covered at the `Terminal::encode_key`
> level in § 3 instead, which is the only place they can be set.

**`encode_key`: 768 000 comparisons (75 x 8 x 1 280). 0 mismatches.**

The four mouse encoders were run over their own cross-product:

| Axis | Values |
| --- | --- |
| operation | press, release, move-with-button, hover (`button: None`), wheel |
| button | `Left`, `Middle`, `Right` (and `None` for hover) |
| `MouseModifiers` | all 8 combinations of shift/alt/ctrl |
| row, col | 10 x 10: `0, 1, 79, 190, 191, 222, 223, 2014, 65535, usize::MAX` -- the 1-byte cap, the `? 1005` two-byte cap and both saturating-add boundaries |
| wheel `delta_y` | `1.0, -1.0, 0.0, 0.5, -0.5, NaN, +inf, -inf` |
| `ModeSnapshot` | the same 1 280, so protocol `none / 1000 / 1002 / 1003` x encoding `default / 1005 / 1006` are all covered |

**`encode_mouse_*` + `encode_wheel_event`: 18 432 000 comparisons. 0 mismatches.**

Plus `named_key_variant_sets_are_identical` (38 variants, round-trip both ways) and
`mouse_button_variant_sets_are_identical` (3, both ways). Total **19 200 000** byte comparisons,
**zero** differences, on the exact inputs above.

This retires the packet's "Met differently, deliberately" note on the equivalence criterion: the
criterion as written is now met, and met wider than it was written.

**The move itself, diffed.** A separate script dedents `mod tests { }` out of both `main` files and
unified-diffs it against the new sibling files:

- `mouse_tests.rs`: **one changed line**, the `use` path
  (`oneterm_vt::{MouseProtocol, MouseReporting}` -> `crate::render::{..}`). 285 lines in, 285 out.
- `key_tests.rs`: the 54 added lines at the top (the `encode_key` shim and
  `only_app_cursor_is_read`) plus **one rustfmt reflow** (see **F5**). Every other line identical.

Test names: 51 on `main` (30 in `key_encode.rs`, 21 in `mouse_encode.rs`, counted from the files
themselves), 51 under `input::` on the branch after subtracting `only_app_cursor_is_read`. `diff`
of the two sorted name lists: **identical**. `cargo test -p oneterm-vt --lib -- --list | grep
'^input::'` reports 52 (51 + the new one).

`only_app_cursor_is_read` does flip all seven non-`app_cursor` fields, as claimed
(`key_tests.rs:21-56`) -- though § 2's 1 280-snapshot sweep now covers that ground far more widely.

## 3. `Terminal::encode_key` against the live mode table

**Test file:** `crates/vt/tests/zz_verify_us0099_terminal.rs` (parked at
`<scratchpad>/zz_verify_us0099_terminal.rs`). Modes are set only by feeding bytes -- no direct field
access -- so this exercises the real parser -> mode table -> encoder path. 6 tests, all green.

- `decckm_is_read_live_from_the_mode_table` -- for each of the four arrows: power-on `ESC [ x`,
  after `CSI ? 1 h` -> `ESC O x`, after `CSI ? 1 l` -> back to `ESC [ x`; then `Home`/`End`
  (`ESC O H` / `ESC O F` vs `ESC [ H` / `ESC [ F`). **Follows the live state.**
- `decckm_survives_the_alt_screen_round_trip` -- `CSI ? 1 h`, `CSI ? 1049 h`, back out; the encoder
  agrees with `mode_snapshot()` at every point.
- `kitty_flags_swap_with_alt_screen_and_never_reach_the_bytes` -- all 32 flag values pushed with
  `CSI > Ps u`; `Terminal::keyboard_flags()` returns each one (so the stack **is** live), and the
  arrow bytes never move. `CSI = 5 ; 1 u` (apply), `CSI < 1 u` (pop) likewise. The alt-screen round
  trip `CSI ? 1049 h` / `CSI ? 1049 l` **restores the primary screen's flags** (asserted, matching
  the pre-existing `the_keyboard_stack_swaps_with_the_screen`).
- `modify_other_keys_never_reaches_the_bytes` -- `CSI > 4 ; 0 m`, `; 1 m`, `; 2 m` against
  Ctrl+`a`, Ctrl+`2`, Ctrl+Enter, Ctrl+Tab: bytes identical at every level.
- `app_keypad_never_reaches_the_bytes` -- `ESC =` sets `app_keypad` (asserted via the snapshot);
  arrows, `Home`, `F1` and `5` are unchanged.

This **confirms the packet's Gap 3 empirically**: the kitty keyboard protocol is recognised, stored
and stacked by the engine, and completely ignored by the encoder. Correctly scoped out of this
packet; now measured rather than asserted.

## 4. Hostile inputs

`hostile_inputs_do_not_panic` (equivalence file) and
`terminal_encode_key_does_not_panic_on_hostile_text` (terminal file):

- a 4-byte char (`U+1F600`), `U+10FFFF`, `U+FEFF`, a combining pair, `NUL`, the **empty string**,
  multi-codepoint text -- across all 8 modifier combinations and all 1 280 snapshots. No panic;
  `Ctrl` + any of the non-ASCII or multi-codepoint cases returns `None` as documented, and `""`
  returns `None` under `Ctrl` (the `chars().next()?`) and `Some(vec![])` otherwise, unchanged from
  `main`.
- a 100 000-char `Character` payload, and a 50 000 x `U+1F600` payload through
  `Terminal::encode_key`. No panic, no overflow.
- `encode_mouse_press` / `encode_wheel_event` at `row = col = usize::MAX` under `? 1005` and `? 1006`
  -- `saturating_add` and the `.min(255)` / `.min(0x7ff)` caps hold. `delta_y = NaN` takes the
  `else` branch (code 65), identically in both implementations.

No API in this module takes a repeat count.

## 5. Gates

| Command | Result |
| --- | --- |
| `cargo test -p oneterm-vt` (default) | **pass** -- 427 lib (2 ignored) + 6 + 8 + 7 + 3 integration (+ 1 ignored doc target), 0 failed. The verifier's own two test binaries (5 + 6) were present in this run and also passed. |
| `cargo test -p oneterm-vt --no-default-features` | **pass** -- same counts |
| `cargo test -p oneterm-vt --features vt-paranoid` | **pass** -- same counts (lib 10.36 s vs 1.49 s, the whole-history walk) |
| `cargo test -p oneterm-terminal` | **pass** -- 239 passed, 0 failed |
| `RUSTDOCFLAGS=-D warnings cargo doc -p oneterm-vt --no-deps` | **pass**, warning-free (so `#![warn(missing_docs)]` is satisfied for all 60 new public items and the `[crate::input::encode_key]` / `[crate::Terminal::encode_key]` intra-doc links resolve) |
| `python scripts/vt-public-api.py --check` | **pass** -- "public API surface unchanged" |
| `crates/vt/public-api.txt` diff | +60 lines, **0 removals**: `pub mod input`'s 5 types (with every field and variant), its 5 functions, and `method encode_key` on `Terminal`. Matches the packet exactly. |
| CI rustdoc citation grep (`^\s*//[/!].*(US-0\d{3}|BUG-0\d{3}|DEC-0\d{3}|IN-0\d{3}\|docs/spec-intakes)` over `crates/vt/src`, minus `https://github.com/`) | **empty**. The one `US-0099` citation is in `crates/terminal/src/lib.rs:47`, outside the scanned tree; `crates/vt/src/lib.rs:37-39` and `terminal_tests.rs:686` use `//`, which the grep does not scan. |
| `python scripts/check-doc-paths.py` | **pass** -- 196 paths in 11 documents |
| `python scripts/check-english.py` | **pass** -- 837 files (838 once this report existed) |
| `cargo tree -p oneterm-vt -e normal` | 7 lines, 6 deps -- unchanged |
| `pwsh scripts/ci-local.ps1 -Full` | **pass**, exit code 0. Summary line: `ci-local: all checks passed.` `cargo-deny` was installed, so `-Full` ran it too: `advisories ok, bans ok, licenses ok`. This covers `cargo fmt --all -- --check`, both `cargo clippy --workspace --all-targets -- -D warnings` passes, `cargo test --workspace`, `cargo test -p oneterm-vt --features vt-paranoid`, `cargo package -p oneterm-vt` and its file-list check, the rustdoc self-containment grep, `verify-dependency-graph.py`, `check-doc-paths.py`, the `check_english` unit tests, `check-english.py`, `completion-catalog.py validate` and `third-party-notices.py --check`. Run with the verifier's own test files moved out of the tree. |

### The equivalence harness can fail

A test that compares two things and always passes proves nothing, so the harness was mutation-checked.
One byte of the **pasted original** was changed -- `NamedKey::F5 => b"\x1b[15~"` to `b"\x1b[16~"` -- and
the test was re-run:

```
MISMATCH spec=Named(F5) mods=KeyMods { shift: false, ctrl: false, alt: false }
  snap=ModeSnapshot { .. } new=Some([27, 91, 49, 53, 126]) old=Some([27, 91, 49, 54, 126])
```

It fails, on the right case, with the right bytes. The probe file was deleted immediately afterwards.

## 6. Findings

### F1 -- `#[non_exhaustive]` on `KeySpec` and `NamedKey`: the disclosed reason does not hold

**Severity: medium.** `crates/vt/src/input/key.rs:27` (`NamedKey`), `:108` (`KeySpec`).

`low-level-design/api-surface.md`, section "Becomes `#[non_exhaustive]`", names `KeySpec` and
`NamedKey` in its table ("new named keys arrive with keyboard protocols") and states the cost of not
doing it: "Marking them now is free; marking them after the first release is a breaking change."
The implementation applies it to neither. The packet discloses this (Gaps 4) and justifies it:

> No `#[non_exhaustive]` exists anywhere in `crates/vt` today, so adding it to three enums here
> would be a new convention smuggled in under a move.

**That premise is already false.** The sibling packet `US-0100` on `feat/vt-search` applies
`#[non_exhaustive]` to `SearchPattern` (`crates/vt/src/search/mod.rs:53`) and `SearchOptions`
(`:85`) -- from the same table, in the same intake, concurrently. It is not a new convention; it is
the intake's convention, and this packet is the only one that declines it.

It also has no other owner. `US-0099` is the only packet that creates `KeySpec` and `NamedKey`
inside `crates/vt`; `US-0101` renames `Render*`, `US-0102` adds mouse variants, `US-0103` writes
docs. If it is not done here it is not done at all, and after the first tag an outside consumer pins
it costs a minor bump per the crate's own promise (rule 1).

The cost of doing it is close to zero: nothing outside `crates/vt` matches exhaustively on either
type. `crates/terminal-view/src/input/keys.rs:251-290` only **constructs** `NamedKey` variants,
which `#[non_exhaustive]` permits from any crate.

Two parts of Gap 4 are, by contrast, **correct and should stand**:

- `TerminalMouseButton` appears only in `encoding-and-search.md`'s Interfaces sketch, **not** in
  `api-surface.md`'s table -- so that one is genuinely discretionary.
- Leaving `KeyMods` and `MouseModifiers` exhaustive is right: `api-surface.md` lists both under
  "Deliberately exhaustive".
- The same sketch spells `KeySpec` with `Char(char)` / `Text(String)` against the real
  `Character(String)`, so the sketch is demonstrably not normative on shape. But `api-surface.md`'s
  table is a list of decisions, not a sketch, and it is where the two names appear.

**Recommendation:** apply `#[non_exhaustive]` to `KeySpec` and `NamedKey`, or record an accepted
deviation *in `api-surface.md` itself* so the next packet does not re-litigate it. Not a merge
blocker; a before-first-tag blocker.

### F2 -- three wrong section numbers in the packet's Reconciliation

**Severity: low.** `US-0099-input-encoding-moves-in.md`, "Reconciliation", the
`docs/terminal-backend.md` paragraph.

The edits are right; the citations are not. Measured against `grep -n '^## ' docs/terminal-backend.md`:

| Packet says | Actually |
| --- | --- |
| "the crate-responsibility row in § 5" | § 3 "Responsibilities per crate" (line 102); § 5 is "Concurrency model" |
| "the directory tree at § 12" | § 11 "File layout (current)" (line 881); § 12 is the roadmap |
| "One further mention, in the § 14 migration checklist" | § 12 "Implementation order (roadmap)", line 942; § 14 is "Quick reference" |

The decision to leave line 942 alone is correct -- it is step 1 of a finished roadmap, written in
the crate names of its day ("`core`", a crate that no longer exists) -- but it is not a "§ 14
migration checklist". The other two citations in the same paragraph (§ "Data flow", § 10 step 1)
are right.

### F3 -- the Evidence diffstat is two files and 69 lines short

**Severity: low.** Packet, "Evidence and Gaps / The move".

Claimed: `git diff --stat main..HEAD -- crates/` is "19 files, 1239 insertions, 1078 deletions".
Measured at `0558fa2..c135d40`: **21 files, 1308 insertions, 1078 deletions**. The difference is
exactly `crates/vt/CHANGELOG.md` (+9) and `crates/vt/public-api.txt` (+60) -- both under `crates/`
and both part of this packet's own Reconciliation. The stat was evidently taken before they were
regenerated and not refreshed.

(Deletions match exactly, and every per-file figure quoted afterwards -- `key.rs` 287,
`key_tests.rs` 385, `mouse.rs` 195, `mouse_tests.rs` 285, `mod.rs` 21, `keys.rs` 6, `frame.rs` 13 --
is correct.)

### F4 -- "+70 production lines, all rustdoc" is +74, of which 62 are rustdoc

**Severity: low.** Packet, Acceptance (the LOC bullet) and Gaps 2.

Measured (production half only -- everything before `#[cfg(test)]`):

| | lines | `///` + `//!` | `//` | blank | code |
| --- | ---: | ---: | ---: | ---: | ---: |
| `main`: `key_encode.rs` + `mouse_encode.rs` | 426 | 72 | 14 | 34 | 306 |
| branch: `input/{key,mouse,mod}.rs` | 500 | 134 | 14 | 38 | 314 |
| **delta** | **+74** | **+62** | 0 | +4 | **+8** |

The packet's "425 / about 495 / +70" is fine as rounding. "All rustdoc" is not literally true: the
+8 code lines are `input/mod.rs`'s plumbing (7: two `mod`, two `pub use`) plus
`use crate::render::ModeSnapshot;` and `let app_cursor = modes.app_cursor;` in `key.rs`, less one in
`mouse.rs`. The stronger claim in the same bullet -- **"No logic was added"** -- is correct, and
§ 2 proves it to 19 200 000 cases.

### F5 -- one moved test body also took a rustfmt reflow

**Severity: low.** `crates/vt/src/input/key_tests.rs:126`.

The packet: "all 51 bodies are byte-identical apart from a four-space dedent ... and one `use` path
in `mouse_tests.rs`. Diff with `-w` to see it." There is one more difference, in
`ctrl_punctuation_and_digits_follow_xterm_table`: `main` had

```rust
        let s =
            encode_key(&KeySpec::Character(ch.into()), m(false, true, false), false).unwrap();
```

and the dedent let rustfmt rejoin it into one line. Purely cosmetic, semantically identical, and it
is the **only** such case in 617 moved test lines (my dedent-and-diff finds nothing else). But it is
a line join, not whitespace, so `diff -w` does not hide it and the "apart from" list is incomplete.

### F6 -- the re-export block is narrower than the LLD's, and framed differently

**Severity: low.** `crates/terminal/src/lib.rs:47-52`.

`encoding-and-search.md` § "What `crates/terminal` keeps" specifies:

```rust
pub use oneterm_vt::input::{KeyMods, KeySpec, MouseModifiers, NamedKey, TerminalMouseButton,
                            encode_key, encode_mouse_move, encode_mouse_press,
                            encode_mouse_release, encode_wheel_event};
```

The implementation re-exports the first six names and **omits the four `encode_mouse_*` /
`encode_wheel_event` functions**. On `main`, `mouse_encode` was a `pub mod`, so
`oneterm_terminal::mouse_encode::encode_mouse_press` was a reachable path; now no path to those four
exists through `oneterm-terminal` at all. Nothing breaks -- `grep -rn "encode_mouse_\|encode_wheel_event"
crates/` shows the only callers are `crates/terminal/src/model.rs:269-345`, in-crate -- and an
embedder should take them from `oneterm_vt::input` anyway. Flagged only because the LLD is explicit
and the packet does not list this among its deviations.

Second, smaller half: the LLD says `crates/terminal` re-exports the moved names "**for one
release**", whereas the doc comment at `lib.rs:47-49` frames the re-export as permanent ("so a
consumer keeps one `use`"). Somebody has to decide which; today they disagree.

### F7 -- `crates/vt/README.md` was not reviewed, and one line now reads wrong

**Severity: low.** `crates/vt/README.md:34`.

> - **Not a window.** No event loop, **no input handling**, no clipboard, no window title bar.

After this packet the crate publishes `input::encode_key` and four mouse encoders. The intent is
defensible -- the crate still never touches a platform event, and `input/mod.rs`'s own doc says so
clearly ("scrolling, selection and shift-tracking stays with the embedder's input handling") -- but
the README sentence is now ambiguous at best to the exact reader it was written for.

`README.md` is not a normal doc here: `US-0097` made it external contract text and
`crates/vt/src/lib.rs:21-22` pulls it into rustdoc via `#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests`. It appears in neither the packet's "Owning Docs Reviewed" nor its
"Reconciliation". One qualifier ("no platform event handling") closes it.

### F8 -- risk lane recorded as `normal` for a 60-line public-surface addition

**Severity: low (record only).** Packet, Classification.

`harness.db` files intake 43 as `risk_lane = 'high_risk'` with risk flag "public contract", and the
sibling `US-0097` as `high_risk`. This packet self-classifies `normal` on the grounds that it is a
boundary move with no behaviour change -- true of the bytes, but it adds 10 items / 60 public-API
lines to a crate whose public API the intake declares an external contract, and `AGENTS.md` names
public contracts as high risk.

No process step was actually skipped: the high-risk lane's requirement is a Low-Level Design before
the work packet, and `encoding-and-search.md` plus `api-surface.md` both exist and were followed
(except F1). This is a label, not a gap.

### F9 -- the public surface now mixes two `ModeSnapshot` conventions

**Severity: informational.** `crates/vt/src/input/key.rs:170`, `mouse.rs:130-196`.

```rust
pub fn encode_key(key: &KeySpec, mods: KeyMods, modes: &ModeSnapshot) -> Option<Vec<u8>>;
pub fn encode_mouse_press(row: usize, col: usize, button: TerminalMouseButton,
                          modes: ModeSnapshot, mods: MouseModifiers) -> Vec<u8>;
```

The snapshot is by reference and last in one, by value and third in the other; `encode_key` returns
`Option`, the mouse encoders do not. This is **correct per the LLD** -- "Exact parameter lists for
the four mouse functions are taken verbatim from `crates/terminal/src/mouse_encode.rs:125-187`; the
move must not change them" overrides that document's own Interfaces sketch, which shows
`&ModeSnapshot` first for the mouse functions. `ModeSnapshot` is `Copy`, so nothing is wrong. Noted
only so `US-0103`'s embedder guide does not have to discover it.

## 7. Packet ticks against reality

| Acceptance line | Tick | Reality |
| --- | --- | --- |
| `key_encode.rs` / `mouse_encode.rs` no longer exist | [x] | correct (both deleted, 572 + 475 lines) |
| every test moved unchanged in input and expectation | [x] | correct, with **F5** |
| equivalence test for the `NamedKey` x `KeyMods` cross-product | [x] "met differently" | the criterion as written is now **met** -- § 2 |
| `cargo tree -p oneterm-vt -e normal` still 7 lines | [x] | correct |
| `cargo tree -p oneterm-terminal` no new dependency | [x] | correct |
| `cargo test --workspace` green, `terminal-view` only `use` paths (+38 disclosed lines) | [x] | correct; 38 lines / 4 files confirmed |
| production lines within +-20 | [ ] missed by +70 | **+74**; **F4** |
| manual Windows walk | [ ] not done | still not done -- § 8 |
| Proof block: unit 1, integration 1, e2e **0**, platform 1 | | consistent with the above |

Deviations the packet discloses: the manual walk, the LOC budget, `#[non_exhaustive]`, and the
equivalence method. **F2-F7 are not disclosed** -- all low, all record-level except F7.

**Other records.**

- `crates/vt/CHANGELOG.md:51-59` -- `[Unreleased] / Added` gains the `input` module (all ten names)
  and `Terminal::encode_key`, with "Not one byte changed in the move". Correct placement: per the
  crate's own semver promise rule 2, a new module and a new method are patch-level.
- `docs/agents/structure.md` -- directory tree (both crates), the `lib.rs` "four modules" comment,
  and both crate-responsibility rows. All four edits present and accurate.
- `docs/terminal-backend.md` -- § "Data flow", § 3 table, § 10 step 1, § 11 tree. All four correct;
  only the section numbers *in the packet* are wrong (**F2**).
- `docs/spec-intakes/IN-0038-embeddable-vt-core/IN-0038.md:234` still carries the stale estimate
  "move about 1 050 (620 production, 430 test), net +0". The intake's table is a budget table with
  no status column, so nothing is untickled; noted for completeness, not counted as a finding.
- **Harness snippet.** The `story` schema in the packet's Python is byte-for-byte the live table's
  column list and order (verified read-only against `harness.db`): 17 columns, `*_proof` are
  `INTEGER NOT NULL DEFAULT 0 CHECK(... IN (0,1))`, `status` and `risk_lane` and
  `last_verified_result` all within their CHECK sets. `intake_id = 43` **is** IN-0038 (summary:
  "oneterm-vt becomes a public embeddable terminal core ..."). No row for `US-0099` exists yet
  (`intake_id = 43` currently holds `BUG-0058` and `US-0097` only) -- correct, the task forbids
  writing it. The snippet's `risk_lane="normal"` inherits **F8**.
- No commit was made to `feat/vt-input-encoding` by this verification, and `harness.db` was opened
  read-only.

## 8. What could not be verified

1. **The manual Windows walk -- still open, and still the packet's own stated blind spot.** No SSH
   host was reachable from this session, and the owner runs their editor inside the application
   under test, so `oneterm.exe` must not be driven or killed. Everything in §§ 2-4 is unit-level.
   What an accepting session still owes: `vim` over SSH -- arrows, Home/End, PageUp/PageDown,
   F1-F12, Ctrl and Alt chords, DECCKM entered and left; `htop` -- click, drag, release, wheel, with
   `? 1006` on and off. Given the 19 200 000-case equivalence result, the residual risk is no longer
   "the encoder changed" -- it is "the *adapter* wiring changed", i.e. the 38 `terminal-view` lines
   in § 1, which only a real walk exercises.
2. **Non-Windows platforms.** Everything here ran on Windows 11 with the pinned stable toolchain.
   The moved code is platform-independent (no `cfg`, no OS call), so the risk is nil, but it is
   untested here.
3. **A defect that already existed on `main`.** § 2 proves the move faithful, which by construction
   means a pre-existing bug moved with it. Two are visible in the code and are out of this packet's
   scope: `encode_key` ignores `alt` for `Insert`, `Tab` and F1-F24 (so `Alt+F5` == `F5`), and the
   kitty protocol is unwired (**Gap 3**, measured in § 3). Neither is a regression.
4. **The merge into the current `main`.** The branch is based on `0558fa2`; `main` has since taken
   `US-0104` (a13002a). `git diff main..HEAD` therefore shows `US-0104`'s files as deletions. The
   two packets touch disjoint files under `crates/vt/src/`, so a merge should be clean, but the
   merge itself was not performed or tested here.
5. **`cargo public-api`-grade signature checking.** `scripts/vt-public-api.py` catches added,
   removed and renamed items, not signature changes -- its own documented limit
   (`api-surface.md` Verification). The `encode_key` signature change from `bool` to `&ModeSnapshot`
   is therefore invisible to that gate; it is caught here by § 2 and by the compiler at every call
   site.

## Commands run

```powershell
git reset --hard feat/vt-input-encoding        # c135d40; merge-base with main = 0558fa2
git diff --stat 0558fa2..HEAD
git show main:crates/terminal/src/key_encode.rs   > <scratchpad>/orig_key_encode.rs
git show main:crates/terminal/src/mouse_encode.rs > <scratchpad>/orig_mouse_encode.rs
python <scratchpad>/cmp_tests.py               # dedent + unified-diff the moved test modules
python <scratchpad>/gen_equiv.py               # generate crates/vt/tests/zz_verify_us0099_equiv.rs
$env:CARGO_BUILD_JOBS=3
cargo test -p oneterm-vt --test zz_verify_us0099_equiv    -- --nocapture
cargo test -p oneterm-vt --test zz_verify_us0099_terminal -- --nocapture
cargo test -p oneterm-vt
cargo test -p oneterm-vt --no-default-features
cargo test -p oneterm-vt --features vt-paranoid
cargo test -p oneterm-terminal
cargo test -p oneterm-vt --lib -- --list       # 52 under input::
$env:RUSTDOCFLAGS="-D warnings"; cargo doc -p oneterm-vt --no-deps
python scripts/vt-public-api.py --check
python scripts/check-doc-paths.py
python scripts/check-english.py
cargo tree -p oneterm-vt -e normal
cargo tree -p oneterm-terminal -e normal --depth 1
python <scratchpad>/loc.py                     # production LOC split
pwsh scripts/ci-local.ps1 -Full                # verifier test files moved out first
# mutation check of the harness, then the probe deleted:
#   copy the equivalence test, change one byte of the PASTED ORIGINAL
#   (NamedKey::F5 => b"\x1b[15~"  ->  b"\x1b[16~"), re-run -> fails as expected
```

Verifier test files (in this worktree only, never committed; parked in the scratchpad while
`ci-local` ran so the gate measured the branch and not the verifier):

- `crates/vt/tests/zz_verify_us0099_equiv.rs` -- the 19 200 000-case cross-product
- `crates/vt/tests/zz_verify_us0099_terminal.rs` -- `Terminal::encode_key` against the live modes
