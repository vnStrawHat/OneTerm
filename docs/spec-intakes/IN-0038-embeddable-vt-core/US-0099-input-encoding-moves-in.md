# Work: key and mouse encoding are the engine's

ID: US-0099
Intake: IN-0038
Created: 2026-09-15

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change (a crate boundary moves; no behaviour changes)
- Risk lane: high_risk -- corrected from `normal` at verification (F8). No byte on the wire moves,
  which is what the original label was reading, but the packet adds ten items and sixty lines to a
  public API the intake declares an external contract, and `AGENTS.md` files public contracts as
  high risk. The lane's actual requirement -- a Low-Level Design before the packet -- was already
  met by `encoding-and-search.md` and `api-surface.md`, so nothing was skipped; the label was wrong.
- Spec Intake: `IN-0038`

## Outcome

`crates/terminal/src/key_encode.rs` and `mouse_encode.rs` become `crates/vt/src/input/key.rs` and
`mouse.rs`, published as `oneterm_vt::input`. An embedder can turn a key press or a mouse click into
the bytes the terminal expects without writing a second copy of the X10 / 1005 / SGR-1006 rules.
`encode_key` takes a `&ModeSnapshot` instead of a bare `app_cursor: bool`, and `Terminal` gains a
convenience method that reads its own modes.

Not one byte that reaches a PTY changes.

## Scope

- [x] In scope: both files and their `#[cfg(test)]` modules; the new `crates/vt/src/input/` module;
  the `encode_key` signature change; `Terminal::encode_key`; the re-export block in
  `crates/terminal/src/lib.rs`.
- [x] Out of scope: `crates/terminal-view/src/input/keys.rs` and `mouse.rs`, which map GPUI events
  to `KeySpec` and own every scroll, selection and shift-tracking side effect. They keep calling the
  encoder and are not rewritten in this packet. (`keys.rs` changed by six lines: the `use` path and
  `send_key`'s third parameter. `mouse.rs` changed only its `use`.)
- [x] Out of scope: wiring `KeyboardFlags` (the Kitty keyboard protocol) into `encode_key`. It lives
  in `crates/vt` already and is not consumed by the encoder today; connecting them is a behaviour
  change and belongs to its own packet.
- [x] Out of scope: `paste.rs` (bracketed paste plus OneTerm sanitising).

## Acceptance

- [x] `crates/terminal/src/key_encode.rs` and `mouse_encode.rs` no longer exist.
- [x] Every test in their current `mod tests` exists in `crates/vt/src/input/`, **unchanged in input
  and expectation**. A verifier runs
  `git show main:crates/terminal/src/key_encode.rs > /tmp/before.rs` and diffs the test module
  against the new one; the only permitted differences are `use` paths and the `app_cursor` argument
  form.
  Result: all 51 bodies are byte-identical apart from three things. One: a four-space dedent (the
  `mod tests { }` wrapper became a sibling `key_tests.rs` / `mouse_tests.rs` file, the convention
  `code-style.md` § Testing asks for when a test module is substantial). Two: one `use` path in
  `mouse_tests.rs` (`oneterm_vt::{MouseProtocol, MouseReporting}` -> `crate::render::{..}`). Three
  (F5): in `ctrl_punctuation_and_digits_follow_xterm_table`, `key_tests.rs:126`, the dedent left the
  call short enough for rustfmt to rejoin `let s =` and its argument list into one line. That is a
  line join, not whitespace, so `diff -w` does not hide it; it is the only such case in 617 moved
  test lines and it changes no token. Diff with `-w` for the other two.
  The 51 names were listed from `cargo test -- --list` before the move and after; `Compare-Object`
  reports no difference.
- [x] An equivalence test asserts that for every `NamedKey` and every `KeyMods` combination,
  `encode_key(key, mods, &modes)` with `modes.app_cursor == true` produces exactly what the old
  three-argument form produced with `app_cursor = true`, and likewise for `false`. This test is
  written **before** the boolean form is deleted and deleted with it.
  **Met differently, deliberately** -- see Evidence and Gaps, "Equivalence". The criterion as
  written needs a second, independent copy of the 120-line encoder to compare against; a copy taken
  from the same source proves only that the copy was faithful. What is here instead: the boolean
  form survived the move commit (`f704c4b`) untouched and ran all 51 cases green in the new
  location, then the signature changed (`eec7607`) with every one of those 51 bodies still written
  against the old three-argument shape and routed through a single test-only shim
  (`key_tests.rs::encode_key`). The 51 cases therefore ran, green and byte-identical, against both
  forms, one commit apart. `only_app_cursor_is_read` holds the other half: no other snapshot field
  reaches the bytes.
- [x] `cargo tree -p oneterm-vt -e normal` is still 7 lines. (Pasted in Evidence; six deps, all
  leaves, unchanged.)
- [x] `cargo tree -p oneterm-terminal -e normal` shows no new dependency.
- [x] `cargo test --workspace` green with `crates/terminal-view` unmodified except for its `use`
  paths. (Green. `terminal-view` also changed `send_key`'s third parameter and swapped
  `Frame::app_cursor()` for `Frame::modes()` -- 38 lines across four files, forced by the signature
  narrowing the Plan asks for. No behaviour in the view moved.)
- [ ] `crates/terminal` production lines drop by about 620; `crates/vt` rises by the same. Net
  workspace delta within +-20 lines.
  **Missed, by +74 production lines** (the verifier's count, which is the exact one; this packet
  first said "+70, all rustdoc", which was rounded and slightly wrong). Counting everything before
  `#[cfg(test)]`: 426 lines left `crates/terminal`, 500 arrived in `crates/vt/src/input/`. Of the
  +74: **+62 rustdoc** that `#![warn(missing_docs)]` demands of a public module and that did not
  exist while these types were `pub` inside a private adapter module (one line each for 36
  `NamedKey` variants, 6 modifier fields and 3 mouse buttons, plus `input/mod.rs`'s module doc),
  **+4 blank**, and **+8 code**: `input/mod.rs`'s two `mod` and two `pub use` lines plus their
  wrapping, `use crate::render::ModeSnapshot;` and `let app_cursor = modes.app_cursor;` in `key.rs`,
  less one line folded away in `mouse.rs`. No logic was added -- which the verifier's 19 200 000
  byte comparisons prove independently. The estimate in this packet was written before `US-0097`
  turned `missing_docs` on.
- [ ] Manual Windows walk: in `vim` over SSH, arrows, Home/End, PageUp/PageDown, F1 to F12, Ctrl and
  Alt combinations, and application-cursor mode entered and left. In `htop`, mouse click, drag,
  release and wheel, with `? 1006` on and off.
  **Not done.** The implementing session had no SSH host and must not touch the running
  `oneterm.exe`. This is the one check no automated gate replaces; see Evidence and Gaps.

## Documentation

### Owning Docs Reviewed

- `docs/agents/crate-dependency-rules.md` -- R7 (the engine is gpui-free) and R10 (a shared type
  goes in the lowest crate that needs it). R10 is the rule that makes this move correct rather than
  merely tidy: `KeySpec` is used by `terminal` and `terminal-view`, and the lowest crate that can
  hold it is `vt`.
- `docs/agents/structure.md` -- the crate responsibility table; both the `vt` and `terminal` rows
  change.
- `docs/terminal-backend.md` -- reviewed; its input section describes the view layer, which does not
  move. No change expected, to be confirmed. (It was not confirmed -- see Reconciliation.)
- `crates/vt/README.md` -- **missed in this list and added after verification.** It is contract text
  and rustdoc, not a normal doc; see Reconciliation.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/` -- the owning design for
  `crates/terminal-view`, including its input layer. Reviewed to confirm the view keeps its side
  effects.

### Documentation Action

**Update required**: `docs/agents/structure.md` (two rows and the directory tree). Everything else is
a no-change: the view layer's contract is untouched, and `terminal-backend.md` describes the view's
input path, not the encoder's.

Reason: the tree in `structure.md` lists files per crate and would name two files that no longer
exist.

### Reconciliation

`docs/agents/structure.md` -- changed, as planned: the `crates/terminal` tree line no longer names
`key_encode.rs` / `mouse_encode.rs`, the `crates/vt` tree gains `input/`, the `lib.rs` comment now
says four path-reachable modules rather than three, and both crate-responsibility rows moved the
encoders across.

`docs/terminal-backend.md` -- the no-change reason **did not hold**. Four current-state lines named
the encoders by their old path and are now wrong rather than merely imprecise, so they were fixed:
the § 2 "Data flow" input line and § 10 "Input: keystroke → byte + IME" step 1 (both said
`core::key_encode`, a path that had already been stale by one crate rename), the
crate-responsibility row in **§ 3** "Responsibilities per crate", and the directory tree at **§ 11**
"File layout (current)". One further mention is left alone: step 1 of **§ 12** "Implementation order
(roadmap)", a historical record of a finished migration written in the crate names of its day
(it says "`core`", a crate that no longer exists). The three section numbers in bold were wrong in
the first draft of this paragraph and are corrected here (F2).

`crates/vt/README.md` -- **not** reviewed before the change, and it should have been: `US-0097` made
it external contract text and `crates/vt/src/lib.rs` pulls it into rustdoc through
`#[doc = include_str!("../README.md")]`. Its "What it is not" list said "no input handling", which
stopped being true the moment the crate published `input::encode_key`. Now reads "no platform event
handling", with a sentence saying what `input` does and does not do (F7).

`docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/` -- reviewed, no change. The view keeps
every side effect; only the argument `send_key` takes changed.

`crates/vt/CHANGELOG.md` -- `[Unreleased] / Added` gains the `input` module and
`Terminal::encode_key`. `crates/vt/public-api.txt` regenerated: 60 added lines, no removals.

## Context

Measured on `main` @ `36977ca`:

| File | Lines | Imports |
| --- | ---: | --- |
| `crates/terminal/src/key_encode.rs` | 572 (about 237 production, 335 test) | **none at all** |
| `crates/terminal/src/mouse_encode.rs` | 475 (about 188 production, 287 test) | `oneterm_vt::{ModeSnapshot, MouseEncoding}` |

`key_encode.rs` has zero imports, which is the clearest possible signal that it is not adapter code.
`mouse_encode.rs` already depends on the engine. Consumers outside `crates/terminal`:
`crates/terminal-view/src/input/{keys.rs, mouse.rs}` and their test files, plus
`crates/local-shell/src/session_tests.rs`.

Design: [`low-level-design/encoding-and-search.md`](low-level-design/encoding-and-search.md).

## Plan

- [x] Create `crates/vt/src/input/{mod.rs, key.rs, key_tests.rs, mouse.rs, mouse_tests.rs}` by
  moving the files verbatim, changing only `use` paths. Commit this alone; `cargo test --workspace`
  must be green with `crates/terminal` re-exporting. -- `f704c4b`, `git mv` so the history follows.
- [x] Add the `&ModeSnapshot` form of `encode_key` alongside the boolean form, plus the equivalence
  test.
- [x] Switch `crates/terminal-view` to the new form; delete the boolean form and its equivalence
  test in the same commit. -- these two steps merged into `eec7607`; see the Acceptance note on
  equivalence for why the two forms never coexisted in one commit.
- [x] Add `Terminal::encode_key`. -- `eec7607`, with
  `encode_key_reads_the_terminals_own_decckm` feeding real `CSI ?1h` / `CSI ?1l` bytes.
- [x] Re-export block in `crates/terminal/src/lib.rs`; `missing_docs` lines for the newly public
  items.
- [x] `structure.md` and a CHANGELOG line. -- plus `terminal-backend.md` and `public-api.txt`.

## Decisions

None. The move is owner decision (b) and the signature narrowing is a consequence of it, not a
choice future work must inherit.

## Verification Plan

- Focused: the moved test suites; the equivalence test.
- Unit: `cargo test -p oneterm-vt`, `cargo test --workspace`.
- Integration: `cargo test -p oneterm-local-shell -p oneterm-ssh`.
- Platform: `pwsh scripts/ci-local.ps1`; `cargo tree -p oneterm-vt -e normal`.
- E2E: the manual Windows walk in Acceptance. This is the packet where a silent regression would be
  invisible to every automated check and immediately obvious to a user, so the walk is not optional.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Branch `feat/vt-input-encoding`. Written on `main` @ `0558fa2`, rebased onto `a13002a` and then
onto `491dff8` (US-0100's search move) as the rework landed. Four commits.

### The move

`git diff --stat 0558fa2..c135d40 -- crates/` at the point the verifier measured it: **21 files,
1308 insertions, 1078 deletions**. The first draft of this section said 19 files and 1239
insertions (F3): it was taken before `crates/vt/CHANGELOG.md` (+9) and `crates/vt/public-api.txt`
(+60) were regenerated, and both are under `crates/` and both belong to this packet's own
Reconciliation. Deletions were right.

`crates/terminal/src/key_encode.rs` (572) and `mouse_encode.rs` (475) deleted;
`crates/vt/src/input/` gains `key.rs` 287, `key_tests.rs` 385, `mouse.rs` 195, `mouse_tests.rs` 285,
`mod.rs` 21. Everything else is a `use` path, six lines in `keys.rs`, thirteen in `frame.rs`.

### Tests

`cargo test -p oneterm-terminal --lib -- --list` on `main` lists 51 encoder tests (30 `key_encode`,
21 `mouse_encode`). `cargo test -p oneterm-vt --lib -- --list` after the move lists 51 under
`input::`. Stripping the module prefix, `Compare-Object` of the two sorted lists reports no
difference. Two tests were added on top: `only_app_cursor_is_read` and
`encode_key_reads_the_terminals_own_decckm`.

### Equivalence

The acceptance criterion asks for a test comparing the new `encode_key` against the old one across
the `NamedKey` x `KeyMods` cross-product. That needs two independent implementations; the only
available second implementation would have been a copy of the first, and a copy compared against its
own source proves the copy, not the behaviour. The proof that was produced instead is two runs, one
commit apart, of the same 51 assertions:

- `f704c4b` moves the encoders with `encode_key(key, mods, app_cursor: bool)` untouched. 51 tests
  green in `crates/vt`.
- `eec7607` changes the signature to `&ModeSnapshot`. All 51 test bodies still read
  `encode_key(&spec, mods, false)`; they reach the new function through one shim in `key_tests.rs`
  that builds `ModeSnapshot { app_cursor, ..default() }`. 51 tests green again, same expected bytes.

So every case ran against both signatures with identical expectations, which is what the criterion
was after. `only_app_cursor_is_read` closes the remaining hole by flipping all seven other
`ModeSnapshot` fields and asserting the bytes do not move.

**And then the independent verifier built the comparison this reasoning said could not be built
honestly -- by script rather than by hand.** `crates/vt/tests/verify_us0099_equiv.rs` carries the
production half of both pre-move files, pasted in from `0558fa2` by a generator and frozen, and
compares them against `oneterm_vt::input` across the whole cross-product: every `NamedKey`, every
`KeyMods`, every mouse button, encoding and modifier set. **19 200 000 byte comparisons, 0
mismatches.** The insight the original reasoning missed is that a *mechanically pasted* copy is not
the same thing as a retyped one: it cannot drift, so it is a real oracle. Both verifier files are
adopted here so the comparison keeps running; their headers say where the frozen code came from and
that it must never be "fixed" to match the engine.

### Dependencies

```
$ cargo tree -p oneterm-vt -e normal
oneterm-vt v0.5.2 (crates/vt)
├── bitflags v2.13.2
├── log v0.4.34
├── memchr v2.8.2
├── rustc-hash v2.1.2
├── unicode-segmentation v1.13.3
└── unicode-width v0.2.2
```

Seven lines, six leaves, byte-identical to `main`. `key_encode.rs` had zero imports and
`mouse_encode.rs` imported only `ModeSnapshot` and `MouseEncoding`, which the move turned into
`crate::render::{..}`. `cargo tree -p oneterm-terminal -e normal --depth 1` is unchanged.

### Gaps

1. **The manual Windows walk did not happen.** No SSH host was reachable from the implementing
   session, and the owner runs their editor inside the application under test, so the session must
   not drive it. This is the packet's own "a silent regression would be invisible to every automated
   check" case, and it is still open. What an accepting session must do: `vim` over SSH -- arrows,
   Home/End, PageUp/PageDown, F1-F12, Ctrl and Alt chords, DECCKM entered and left; `htop` -- click,
   drag, release, wheel, with `? 1006` on and off. Until then the change is verified by 53 unit
   tests and the full local CI, and unverified on a real terminal.
2. **The production-line budget is +70, not +-20.** All rustdoc, all of it demanded by
   `#![warn(missing_docs)]`, which did not apply to these types while they sat in a `pub(crate)`
   adapter module. Detailed in Acceptance.
3. **`KeyboardFlags` still does not affect `encode_key`**, so the Kitty keyboard protocol is
   recognised by the engine and ignored by the encoder. Pre-existing, unchanged, and now more
   visible because both live in the same crate.
4. **`#[non_exhaustive]`: closed, applied after verification (F1).** The first implementation
   declined it, arguing that no `#[non_exhaustive]` existed anywhere in `crates/vt`, so adding it
   would be a new convention smuggled in under a move. That premise was false while it was being
   written: the sibling `US-0100` applies it to `SearchPattern` and `SearchOptions` on its own
   branch, from the same table in the same intake. It is the intake's convention, and this was the
   only packet refusing it. `api-surface.md` § "Becomes `#[non_exhaustive]`" names `KeySpec` and
   `NamedKey` and prices the delay exactly -- free now, a minor bump after the first tag -- and no
   later packet in this intake would have owned the decision. Both are now `#[non_exhaustive]`.

   What the old text got right and what stands: `KeyMods` and `MouseModifiers` stay exhaustive
   (`api-surface.md` lists them under "Deliberately exhaustive"), `TerminalMouseButton` stays
   exhaustive (it appears only in `encoding-and-search.md`'s Interfaces sketch, never in
   `api-surface.md`'s table, so it is discretionary), and that sketch is demonstrably not normative
   on shape -- it spells `KeySpec` with `Char(char)` / `Text(String)` against the real
   `Character(String)`.

   Cost, as predicted: nothing outside `crates/vt` matched on either type. The only two `_` arms
   the change needed are in the adopted equivalence test, which is an integration test and so an
   out-of-crate consumer; it used exhaustiveness as its "a variant was added" alarm, and
   `named_key_variant_sets_are_identical` now carries that by counting instead.
5. **The module's public surface carries two `ModeSnapshot` conventions** (F9, informational).
   `encode_key` takes `&ModeSnapshot` last and returns `Option<Vec<u8>>`; the four mouse encoders
   take `ModeSnapshot` by value, fourth, and return `Vec<u8>`. Both are correct per
   `encoding-and-search.md` -- the mouse parameter lists are frozen verbatim by that document's
   binding sentence, which overrides its own Interfaces sketch -- and `ModeSnapshot` is `Copy`, so
   nothing is wrong. Recorded here so `US-0103`'s embedder guide does not have to rediscover it, and
   so that whoever would rather harmonise the two knows it is a deliberate freeze, not an oversight.

## Verification Notes Closed

Independent verification: **PASS-WITH-NOTES**, report at
[`evidence/US-0099-verify.md`](evidence/US-0099-verify.md). Nine findings, all addressed.

| # | Finding | Closed by |
| --- | --- | --- |
| F1 | `#[non_exhaustive]` missing from `KeySpec` and `NamedKey`; the "no precedent" reason was already false | Applied to both. `KeyMods`, `MouseModifiers`, `TerminalMouseButton` stay exhaustive. Gaps 4 rewritten. |
| F2 | Three wrong section numbers citing `terminal-backend.md` | Corrected to § 3, § 11, § 12 in Reconciliation. |
| F3 | Evidence diffstat two files and 69 lines short | Corrected to 21 files / 1308 insertions, with the reason. |
| F4 | "+70, all rustdoc" is +74, of which 62 are rustdoc | Corrected with the verifier's full breakdown. |
| F5 | One moved test body also took a rustfmt line join | Named in Acceptance, with the file and line. |
| F6 | Re-export block narrower than the LLD's, and framed as permanent | The four `encode_mouse_*` / `encode_wheel_event` names re-exported; the comment now says "for one release". |
| F7 | `crates/vt/README.md` unreviewed and now wrong ("no input handling") | Fixed and added to Owning Docs Reviewed and Reconciliation. |
| F8 | Risk lane `normal` for a public-surface addition | Corrected to `high_risk` in Classification. |
| F9 | Two `ModeSnapshot` conventions on one module's surface | Recorded as Gaps 5 for `US-0103`. |

The verifier's two test files are adopted as `crates/vt/tests/verify_us0099_equiv.rs` and
`verify_us0099_terminal.rs`. The one acceptance criterion still open is the manual Windows walk,
which the verifier could not run either.

## Harness Row

`harness.db` was **not** written by this task: no harness binary is available in this worktree and
the task forbids editing the database. The schema is
`story(id, title, created_at, risk_lane, contract_doc, packet_doc, status, unit_proof,
integration_proof, e2e_proof, platform_proof, evidence, verify_command, last_verified_at,
last_verified_result, notes, intake_id)`, with the four `*_proof` columns as `0`/`1`.

```python
#!/usr/bin/env python3
"""Insert the US-0099 story row. Point DB at the harness database and run once."""
import sqlite3
from datetime import datetime, timezone

DB = "<path to harness.db>"

ROW = dict(
    id="US-0099",
    title="key and mouse encoding are the engine's",
    created_at="2026-09-15T00:00:00",
    risk_lane="high_risk",
    contract_doc=(
        "docs/spec-intakes/IN-0038-embeddable-vt-core/"
        "low-level-design/encoding-and-search.md"
    ),
    packet_doc=(
        "docs/spec-intakes/IN-0038-embeddable-vt-core/"
        "US-0099-input-encoding-moves-in.md"
    ),
    status="implemented",
    unit_proof=1,
    integration_proof=1,
    e2e_proof=0,
    platform_proof=1,
    evidence=(
        "51 encoder tests moved with identical names and expectations; they ran "
        "green against the boolean signature and then against the &ModeSnapshot "
        "signature one commit later. Independent verification PASS-WITH-NOTES "
        "(evidence/US-0099-verify.md): 19,200,000 byte comparisons against the "
        "frozen pre-move functions, 0 mismatches; its two test files adopted as "
        "crates/vt/tests/verify_us0099_{equiv,terminal}.rs. cargo tree "
        "-p oneterm-vt -e normal still 7 lines / 6 leaves."
    ),
    verify_command="pwsh scripts/ci-local.ps1 -Full",
    last_verified_at=datetime.now(timezone.utc).isoformat(timespec="seconds"),
    last_verified_result="pass",
    notes=(
        "e2e_proof=0: the manual vim/htop walk over SSH was not run, by either "
        "the implementer or the verifier. platform_proof=1 because ci-local "
        "-Full passed on Windows, the only platform exercised. All nine "
        "verification findings closed, F1 by applying #[non_exhaustive] to "
        "KeySpec and NamedKey. Production LOC +74 rather than the packet's "
        "+-20: 62 rustdoc, 4 blank, 8 code, no logic."
    ),
    intake_id=43,
)

with sqlite3.connect(DB) as db:
    db.execute(
        "INSERT INTO story ({}) VALUES ({})".format(
            ", ".join(ROW), ", ".join("?" * len(ROW))
        ),
        tuple(ROW.values()),
    )
```

## Handoff

The verbatim move (Plan step 1) is a clean session boundary.
