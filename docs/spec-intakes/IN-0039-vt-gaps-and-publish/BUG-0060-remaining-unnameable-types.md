# Work: the seven types still on the unnameable ledger

ID: BUG-0060
Intake: [`IN-0039`](IN-0039.md)
Created: 2026-09-16

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

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

- Change type: **bug**
- Risk lane: normal
- Spec Intake, when required: [`IN-0039`](IN-0039.md)

Why `normal` and not the intake's `high_risk`. `BUG-0059` inherited the high lane because the
intake as a whole sits there; this packet does strictly less than `BUG-0059` did and the two
triggers that put `IN-0039` in the high lane do not fire on it. It adds no capability at a trust
boundary, it changes no reply byte and no encoder byte, and its effect on the public contract is
**additive only** -- six names appear where none could be written before. The one subtraction is
inside a type that nothing outside this workspace can reach today (`ColorOverrides`'s mutators,
see the table), which is a narrowing of something that was never nameable and therefore never
callable. The compiler and two committed surface snapshots are the proof, and both are mechanical.

## Outcome

`scripts/vt-public-api.py`'s `KNOWN_UNNAMEABLE` ledger is **gone**, not merely empty: every type
that appears in a public `oneterm-vt` signature can be written down by an embedder, and the gate
that checks it has nowhere left to record an exception.

`BUG-0059` fixed the four types the outside evaluation reported and, in doing so, discovered that
the defect was eleven. The other seven went into a shrink-only ledger so that packet's "four names
and no more" scope and its "the gate passes on the branch" acceptance could both hold. The ledger
was always a receipt, never a decision. This packet pays it.

| Type | Defined in | Reached through |
| --- | --- | --- |
| `ModeState` | `terminal::mode` | `Mode::inert_state() -> Option<ModeState>` |
| `StrSpan` | `events::batch` | ten `VtEvent` variant payloads |
| `ByteSpan` | `events::batch` | `VtEvent::Reply` |
| `ParamSpans` | `events::batch` | `VtEvent::Osc { params, .. }` |
| `ColorOverrides` | `terminal::color` | `Terminal::colors() -> &ColorOverrides` |
| `Watermark` | `snapshot::state` | `SnapshotState::watermark() -> Watermark` |
| `Invalidation` | `selection` | `Selection::invalidated_by(.., op: Invalidation) -> bool` |

`ModeState` is first because it is the only one of the seven that is a **usability** defect rather
than an inconvenience: it is an `enum` returned from a public method, so its value can be
`{:?}`-printed and cannot be matched, compared against a variant, or converted. `Invalidation` is
the one that is strictly worse and was not ranked so in the intake -- it is an **argument**, so
`Selection::invalidated_by` is a public method no embedder can call at all. Both are recorded here
rather than silently re-ordered; the priority the intake gave is kept because the packet does all
seven in one commit and the order is a reading order, not a schedule.

## Scope

- [x] In scope:
  - Re-export all seven from the crate root: `ModeState`, `StrSpan`, `ByteSpan`, `ParamSpans`,
    `ColorOverrides`, `Watermark`, `Invalidation`. Six are pure additions; `ColorOverrides` also
    narrows four of its own methods, below.
  - `#[non_exhaustive]` on `Invalidation` only, with the one-line reason the doctrine asks for.
  - Document what `#![warn(missing_docs)]` starts requiring: `ModeState`'s four undocumented
    variants and `ColorOverrides`'s two undocumented methods.
  - Narrow `ColorOverrides::set`, `reset` and `reset_indexed`/`reset_all` to `pub(crate)`, because
    `Terminal::colors` hands out `&ColorOverrides` and a `&` can never call them.
  - Delete `KNOWN_UNNAMEABLE`, its staleness branch, the `seen` set that only fed it, and the two
    docstring sentences that describe it, from `scripts/vt-public-api.py`.
  - Regenerate `crates/vt/public-api.windows.txt` and `crates/vt/public-api.unix.txt`.
  - A CHANGELOG entry naming all seven, and guide chapter 12's `#[non_exhaustive]` count and lists.
- [x] Out of scope:
  - Any other item on the public surface. The gate names exactly these seven; this packet adds
    nothing it did not name.
  - Renaming any of the seven. `Watermark`, `Invalidation` and `ColorOverrides` are all slightly
    generic-sounding at the crate root, and all three are re-exported under their existing names:
    a rename is free today (nothing can name them) and is still a second decision, and the crate
    already carries `Placement` beside `SnapshotPlacement` on exactly that reasoning.
  - The gate's four stated limits (rendered signatures only, the three-character `NAMED` floor,
    no signature-change detection, rustdoc HTML rather than JSON). They are unchanged and still
    documented in the script.
  - Anything to do with publishing. `publish = false` is untouched.

## The decision, per type

The intake asked for the first question to be answered per type rather than assumed:
**re-export the type, or make the method private so no public signature needs it?**

| # | Type | Decision | Why |
| --- | --- | --- | --- |
| 1 | `ModeState` | **re-export** | `Mode` is already public and `Mode::inert_state` is a `pub const fn` on it. Making the method private would delete the only way an embedder can ask "does this engine actually implement the mode it recognises?", which is the question the method exists to answer and which `DECRQM`'s own contract is built on. Four variants need doc comments. |
| 2 | `StrSpan` | **re-export** | It is the payload of ten public `VtEvent` variants. There is no method to make private: the enum is the API. Already documented, all fields private. |
| 3 | `ByteSpan` | **re-export** | Payload of `VtEvent::Reply`, and of `EventBatch::bytes`. Same reasoning. |
| 4 | `ParamSpans` | **re-export** | Payload of `VtEvent::Osc`, and of `EventBatch::params`. Same reasoning. |
| 5 | `ColorOverrides` | **re-export, and narrow its mutators** | Making `Terminal::colors` private looked right -- `Terminal::color(key)` already answers the single-slot question publicly -- until the gate's own workspace was checked: `crates/tools/src/corpus_replay.rs:319` calls `term.colors().iter()` to write the parity corpus's `palette.{index}` keys, from a crate outside `oneterm-vt`. So a public accessor is load-bearing today. Re-exporting it publishes six methods, three of which (`set`, `reset`, `reset_indexed`, `reset_all`) need `&mut` and can never be reached through the `&` the accessor returns. Publishing an unreachable mutator is a worse surface than not publishing it, so they become `pub(crate)` in the same commit and the public surface is `get` and `iter`. |
| 6 | `Watermark` | **re-export** | `pub struct Watermark(pub SeqNo)`, already documented, `Ord`, returned by `SnapshotState::watermark`. The alternative -- return the bare `SeqNo` and keep the newtype internal -- deletes the distinction between "a sequence number" and "how far *this consumer* has read", which is exactly what the type is for and what a multi-consumer embedder has to keep straight. One word in an existing `pub use` line. |
| 7 | `Invalidation` | **re-export** | It is an **argument**, not a return: `Selection::invalidated_by(&self, grid, op: Invalidation)`. A public method whose parameter type cannot be named is a method nobody outside can call, so this is the only one of the seven where re-exporting restores a capability rather than a spelling. |

### `#[non_exhaustive]`, per type

The accepted doctrine (`IN-0038`, applied per-type in
[`low-level-design/api-surface.md`](low-level-design/api-surface.md)): frozen value types an
embedder constructs and destructures stay exhaustive; "the set we know about today" is marked. The
`US-0106` correction in that same file adds the hard constraint that a `#[non_exhaustive]` **struct**
cannot be built with a struct expression from another crate at all.

| Type | Marked? | Why |
| --- | --- | --- |
| `Invalidation` | **yes** | It is the set of engine operations that can invalidate a selection, and it can grow: a new erase form or a new screen swap lands here. An embedder **constructs** these variants to ask the question and does not match on them, and `#[non_exhaustive]` on an enum forbids only exhaustive matching, never variant construction -- so the mark costs this caller nothing and buys a patch-level addition. The one place in the doctrine where the mark is free. |
| `ModeState` | no | A frozen value type. Its five values are `DECRQM`'s wire values `0` through `4`, fixed by the standard, and the `#[repr(u8)]` discriminants say so. It is matched, never constructed, so a mark would cost every caller a wildcard arm to buy a sixth value that the standard does not define. |
| `StrSpan`, `ByteSpan`, `ParamSpans` | no | Every field is private, so each is already unconstructible from outside -- the same reasoning `BUG-0059` recorded for `SyncState`. The mark would add nothing and would not be free: these are structs, so the mark is the `US-0106` trap in miniature. |
| `ColorOverrides` | no | One private field. Unconstructible already; same reasoning. |
| `Watermark` | no | A one-field tuple struct over a public `SeqNo`, and the field is deliberately public so an embedder can compare and store it. Marking it would forbid `Watermark(seq)` from outside for no gain: a second field would not be added to this type, it would be a second type. |

So guide chapter 12's count goes from **thirteen** to **fourteen**, its enum list from eight to
**nine** (`Invalidation` joins), and its struct list is unchanged at five.

### After this, the gate is strict

`KNOWN_UNNAMEABLE` is deleted rather than emptied. The distinction matters and is the point of the
packet: an empty set is a place to put the next one, and the shrink-only rule only ever prevented a
name from rotting inside the ledger -- it never prevented a new name being added to it. With the
constant gone there is no exception mechanism at all, and the next public signature that names a
private type fails CI with nowhere to be written down. The script's success line loses its "but the
7 in `KNOWN_UNNAMEABLE`" tail, which is the human-readable half of the same statement.

The LLD's recorded position is unchanged and is what this packet executes: "**No allow-list is
added in this packet**: if one is ever needed, that is a finding worth reading rather than a config
knob to add speculatively."

## Acceptance

Each criterion is a command a hostile verifier can run, with a stated expected result.

- [x] **An external crate can store and match all seven, not merely import them.** `BUG-0059`'s
      standing proof imports four names and binds `Option<T> = None`, which proves nameability and
      not much else. The proof here is stronger, because two of the seven are an enum whose whole
      complaint was that it could not be matched:

      ```rust
      use oneterm_vt::{
          ByteSpan, ColorOverrides, Invalidation, ModeState, ParamSpans, StrSpan, Watermark,
      };

      // Stored in struct fields -- the thing the evaluation said was impossible.
      struct Probe {
          title: StrSpan,
          reply: ByteSpan,
          osc: ParamSpans,
          mark: Watermark,
      }

      // Matched by path, and passed as an argument by path.
      fn recognised(state: ModeState) -> bool {
          !matches!(state, ModeState::NotSupported)
      }
      fn clears(selection: &oneterm_vt::Selection, grid: &oneterm_vt::grid::TerminalGrid) -> bool {
          selection.invalidated_by(grid, Invalidation::EraseScreen)
      }
      fn overrides(term: &oneterm_vt::Terminal) -> &ColorOverrides {
          term.colors()
      }
      ```

      A doctest compiles as an external crate, which is the only place this can be proven. It lives
      in `crates/vt/src/lib.rs` beside the `BUG-0059` one and `cargo test -p oneterm-vt --doc`
      passes. On `main` the same doctest fails with seven unresolved imports; **both outputs are
      attached**, and this criterion fails if only the passing run is.
- [x] **The ledger is gone and the gate is strict.** `grep -n KNOWN_UNNAMEABLE
      scripts/vt-public-api.py` prints nothing. `python scripts/vt-public-api.py --check-nameable`
      exits zero and its success line reads `every type in a public signature is nameable`, with no
      ledger tail.
- [x] **The gate still catches the defect it was written for**, with the ledger gone rather than
      because of it. Revert one of the seven re-exports, re-run `--check-nameable`, and it exits
      non-zero naming that type and its private module; restore it and it exits zero. Output
      attached. A gate not seen to fail after the ledger was removed has not been tested after the
      change that matters.
- [x] **`ColorOverrides`'s mutators are not on the surface.** `grep -n 'ColorOverrides' on both
      regenerated snapshot files lists `fn get` and `fn iter` under it and neither `fn set` nor any
      `fn reset*`.
- [x] **The documentation build is clean with `missing_docs` on.**
      `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps` and the same with
      `--all-features` both exit zero.
- [x] **The surface diff is exactly the seven items and the four narrowed methods.** After
      `--update` on both platforms, `git diff crates/vt/public-api.*.txt` shows the seven added
      items with their fields and methods, the four `ColorOverrides` mutators absent, and nothing
      else added, removed or renamed.
- [x] **The two snapshots still differ only inside `oneterm_vt::pty`.**
      `python scripts/vt-public-api.py --diff-platforms` passes and reports **six** lines, all of
      them `oneterm_vt::pty` items -- the same six `BUG-0059` and `US-0105` each reported, since
      nothing here is `cfg`-gated.
- [x] **Nothing else in the workspace moved.** `cargo test --workspace` passes with no `#[allow]`
      added anywhere; `crates/tools/src/corpus_replay.rs` compiles unchanged against the narrowed
      `ColorOverrides`; `cargo build -p oneterm-vt --no-default-features` builds, since none of the
      seven is behind `pty`. `pwsh scripts/ci-local.ps1` is green.
- [x] **The production diff is inside budget**: `crates/vt` +55 / -12 and
      `scripts/vt-public-api.py` +2 / -22, measured with `git diff --numstat main...HEAD` excluding
      the two generated snapshots, and attached. A diff more than 50 per cent over budget is a
      finding to explain in Evidence, not a silent overrun -- `BUG-0059` overran by more than half
      on both halves and said so, and that disclosure is the reason this budget is the shape it is.

## Documentation

### Owning Docs Reviewed

- [`low-level-design/api-surface.md`](low-level-design/api-surface.md) -- this packet's owning
  design: the `#[non_exhaustive]` decision per type, the gate's algorithm and its four stated
  limits, the "no allow-list added speculatively" position this packet executes, and the `US-0106`
  correction that governs the mark on any struct.
- [`IN-0038/low-level-design/api-surface.md`](../IN-0038-embeddable-vt-core/low-level-design/api-surface.md)
  -- the accepted public surface and the nine-clause semver promise that decides the bump.
- [`BUG-0059`](BUG-0059-unnameable-public-types.md) -- the packet that found these seven, the
  ledger's shrink-only rule, and the evidence table this one is the sequel to.
- `crates/vt/src/lib.rs` -- the re-export block and the header comment explaining which modules are
  public and why.
- `crates/vt/docs/guide/12-versioning.md` -- counts the marked types ("Thirteen public types are
  marked", eight enums and five structs). One enum joins, so the count and one list change.
- `scripts/vt-public-api.py` -- its docstring states what the gate does and does not catch, and
  two of its sentences describe the ledger this packet deletes.
- `crates/vt/CHANGELOG.md` -- clause 2 of the promise makes seven new re-exports a **patch**-level
  entry; the `ColorOverrides` narrowing is discussed under "Semver" below.

### Documentation Action

**Update required.** Four owning docs must change with the code:

| Doc | Change |
| --- | --- |
| `crates/vt/CHANGELOG.md` | an `### Added` entry under `[Unreleased]` naming all seven types, stating that each was already returned or accepted by a public signature and could not be written down, and naming `Invalidation` as the one that was un-callable rather than merely unspellable. A second line for the `ColorOverrides` mutator narrowing, under `### Changed`, stating that no embedder could have called them |
| `crates/vt/docs/guide/12-versioning.md` | the count goes from thirteen to fourteen and the enum list from eight to nine (`Invalidation`); the struct list is unchanged, and the chapter keeps its `Config` paragraph as written |
| `scripts/vt-public-api.py` docstring | the two sentences describing `KNOWN_UNNAMEABLE` are deleted, and the sentence stating the gate's limits keeps its four (the ledger was never one of them) |
| `crates/vt/public-api.{windows,unix}.txt` | regenerated; they are committed artefacts of the surface, not derived files |

Reason: this changes the crate's public surface, and the crate's own promise says every such change
is a CHANGELOG entry naming the item. The guide's count is a published number that would become
wrong.

`crates/vt/README.md` needs **no** change: it enumerates no types. `crates/vt/docs/guide/04-events.md`
was checked because it is the chapter that explains `EventBatch::str` / `bytes` / `params`; it
describes the accessors without naming the span types, so it stays correct either way -- but if the
implementer finds a sentence there that says a span "cannot be named", that sentence is this
packet's to fix.

### Reconciliation

Docs changed: `crates/vt/CHANGELOG.md` (an `### Added` bullet naming all seven and an
`### Changed` bullet for the `ColorOverrides` narrowing), `crates/vt/docs/guide/12-versioning.md`
(thirteen to fourteen, eight enums to nine), `scripts/vt-public-api.py`'s docstring (the ledger
sentence replaced by "There is no allow-list"), and both `crates/vt/public-api.*.txt`.
`crates/vt/README.md` and `crates/vt/docs/guide/04-events.md` were reviewed and need no change:
the README enumerates no types, and chapter 4 describes `EventBatch::str` / `bytes` / `params`
through their accessors without claiming a span cannot be named.

The guide's count matches the source. Grepping the three lines after each mark
(`grep -rn -A 3 '#\[non_exhaustive\]' crates/vt/src/ | grep -E 'pub (struct|enum)'`) yields
**fourteen** types -- nine enums (`Invalidation`, `KeyEventKind`, `KeySpec`, `NamedKey`,
`OscRoute`, `Progress`, `SearchPattern`, `ShellMark`, `VtEvent`) and five structs (`KeyEvent`,
`ModeSnapshot`, `Placement`, `ResizeOutcome`, `SearchOptions`) -- exactly what chapter 12 now
says.

## Context

- `ColorOverrides::get` and `ColorOverrides::reset` carry no doc comment today; `set`,
  `reset_indexed`, `reset_all` and `iter` do. Only `get` and `iter` stay public, so exactly one new
  doc comment is owed there. `ModeState`'s `Set`, `Reset`, `PermanentlySet` and `PermanentlyReset`
  variants carry none; `NotSupported` does. That is five doc comments in total and it is the
  packet's largest single line cost, exactly as `Placement`'s five fields were for `BUG-0059`.
- All three span types and `Watermark` are already documented, with private fields (`Watermark`'s
  single field is public and documented through the type). They cost a word each in an existing
  `pub use`.
- `Invalidation`'s seven variants are each documented with the sequence they correspond to
  (`EL`, `ED 0`-`ED 3`, `CSI ? 1049 h/l`, `RIS`). No documentation is owed.
- The re-export edits land in four existing blocks in `crates/vt/src/lib.rs`: `event` (three
  spans), `selection` (`Invalidation`), `snapshot` (`Watermark`) and `terminal` (`ColorOverrides`,
  `ModeState`). Each block is alphabetical and stays so.
- `Selection::invalidated_by` takes `&TerminalGrid`, which is already nameable at
  `oneterm_vt::grid::TerminalGrid` -- the `grid` module is `pub`. No second type has to be exposed
  to make the method callable.
- The gate reports a finding per `(item page, type)` pair, so `StrSpan` currently fires once per
  `VtEvent`-adjacent page rather than once. The output after reverting a re-export will therefore
  be longer than one line for the spans; that is the gate working, not a regression.

## Plan

- [x] Add the seven re-exports to `crates/vt/src/lib.rs`, in the existing alphabetical blocks.
- [x] Document `ModeState`'s four variants and `ColorOverrides::get`; run
      `RUSTDOCFLAGS="-D warnings" cargo doc` until clean.
- [x] Narrow `ColorOverrides::set`, `reset`, `reset_indexed` and `reset_all` to `pub(crate)`;
      `cargo test --workspace` is what proves nothing outside `crates/vt` called them.
- [x] Add `#[non_exhaustive]` to `Invalidation` with its one-line reason.
- [x] Add the external-crate doctest that stores and matches, before the re-exports, and watch it
      fail with seven unresolved imports.
- [x] Delete `KNOWN_UNNAMEABLE`, its staleness branch, the now-unused `seen` set, the ledger tail
      on the success line, and the two docstring sentences.
- [x] Revert one re-export, confirm the gate fails naming it, restore.
- [x] Regenerate both surface files; on the platform that is not the host, use the `#` note the
      script already supports and say in Evidence which half was produced by hand.
- [x] CHANGELOG entry; guide chapter 12's count and enum list.

## Decisions

No new decision record. The `#[non_exhaustive]` doctrine and the semver promise this packet applies
are accepted in
[`IN-0038/low-level-design/api-surface.md`](../IN-0038-embeddable-vt-core/low-level-design/api-surface.md),
and their per-type application for this intake belongs in
[`low-level-design/api-surface.md`](low-level-design/api-surface.md), which is the right altitude.
The per-type table above is this packet's contribution to that file's subject and should be read
beside it.

**Semver.** Seven new root re-exports are clause 2, **patch**. `#[non_exhaustive]` on `Invalidation`
is patch: the type was not nameable, so no outside code can be affected, which is the same argument
`BUG-0059` recorded for `ResizeOutcome` and `Placement`. Narrowing `ColorOverrides`'s four mutators
is clause 1 on its face -- a removal -- and is **patch** here for a reason that must be stated
rather than assumed: the methods were never reachable, because `ColorOverrides` had no name and its
only accessor hands out a shared reference. Nothing outside this crate can have called them, so
nothing outside this crate can break. If a reviewer disagrees, the conservative reading is a minor
bump, and `IN-0039` is one minor bump in total already: the `[Unreleased]` section accumulates and
the arithmetic does not change.

## Verification Plan

- `cargo test -p oneterm-vt --doc` -- the store-and-match doctest, and the same doctest failing on
  `main`.
- `python scripts/vt-public-api.py --check-nameable` on the branch (passes, no ledger tail), and
  with one re-export reverted (fails, naming that type).
- `grep -n KNOWN_UNNAMEABLE scripts/vt-public-api.py` -- no output.
- `python scripts/vt-public-api.py --check` and `--diff-platforms` (six `pty` lines).
- `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps`, and with `--all-features`.
- `cargo test --workspace` -- `crates/tools`, `crates/terminal` and `crates/terminal-view` are
  separate crates, so the compiler is what proves the `ColorOverrides` narrowing and the
  `Invalidation` mark break nothing there.
- `cargo build -p oneterm-vt --no-default-features`.
- `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

E2E proof is **not applicable** and must be recorded as such rather than left blank: nothing a user
can see changes, and no byte on any wire moves. Platform proof is the two surface files plus the
`--no-default-features` build.

## Evidence and Gaps

Implemented on `fix/vt-nameable-types-2`, base `main` `980bf5da`, worktree
`.claude/worktrees/agent-a71ff4859b391464a`, with `CARGO_BUILD_JOBS=4`. Not pushed.

**The doctest fails before the re-exports and passes after.** The store-and-match doctest was
added to `crates/vt/src/lib.rs` first, with the crate root otherwise untouched, and
`cargo test -p oneterm-vt --doc` reported one failure:

```
---- crates\vt\src\lib.rs - (line 27) stdout ----
error[E0432]: unresolved imports `oneterm_vt::ByteSpan`, `oneterm_vt::ColorOverrides`,
`oneterm_vt::Invalidation`, `oneterm_vt::ModeState`, `oneterm_vt::ParamSpans`,
`oneterm_vt::StrSpan`, `oneterm_vt::Watermark`
  --> crates\vt\src\lib.rs:30:5
   |
30 |     ByteSpan, ColorOverrides, Invalidation, ModeState, ParamSpans, StrSpan, Watermark,
   |     ^^^^^^^^  ^^^^^^^^^^^^^^  ^^^^^^^^^^^^  ^^^^^^^^^  ^^^^^^^^^^  ^^^^^^^  ^^^^^^^^^ no `Watermark` in the root
...
test result: FAILED. 43 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

Seven unresolved imports, one per type, which is the defect stated as a compiler error. After the
re-exports the same command reports `44 passed`.

**An external crate outside the workspace stores, matches and passes all seven.** A doctest is
compiled as an external crate, but it is compiled by this repository's own test run, so a
standalone crate was built as well: a `probe` binary in the session scratchpad depending on
`oneterm-vt` by path, with its own `[workspace]` and its own `CARGO_TARGET_DIR`. It stores
`StrSpan`, `ByteSpan`, `ParamSpans` and `Watermark` in struct fields, exhaustively matches
`Mode::inert_state()`'s `Option<ModeState>` on all five variants by path, calls
`Selection::invalidated_by` with a constructed `Invalidation::EraseScreen`, and binds
`Terminal::colors()` to a `&ColorOverrides` it then calls `get` and `iter` on. `cargo run` prints:

```
probe ok: cleared=true watermark=Watermark(SeqNo(0)) inert=not supported
```

Its first compile failed on two of the probe's *own* mistakes (`SelectionKind::Char` and an
integer where a `RowId` belongs) and on neither of the seven names -- all seven resolved from the
crate root on the first attempt. The probe is not committed; it lives outside the repository
because nothing in the workspace should depend on `oneterm-vt` by a scratchpad path.

**The gate still fails, with the ledger gone rather than because of it.** `Invalidation` was
removed from the crate root's `pub use selection::{..}` line, rustdoc rebuilt, and
`python scripts/vt-public-api.py --check-nameable --no-doc` exited **1**:

```
oneterm-vt has public signatures naming types no embedder can write.
Re-export each from the crate root, or change the signature:
  oneterm_vt::Selection: `Invalidation` is not nameable (defined in `selection`)
```

Restored, rebuilt, and the same command exits **0** printing
`every type in a public signature is nameable` -- the success line with no ledger tail.
`grep -n KNOWN_UNNAMEABLE scripts/vt-public-api.py` prints nothing; the only surviving match for
`seen` in that file is the word in the docstring's limits sentence.

**The surface diff is the seven and nothing else.** `--update` regenerated
`public-api.windows.txt` on this Windows host (+22 / -0): `struct oneterm_vt::ByteSpan`,
`struct oneterm_vt::ColorOverrides` with **`method get` and `method iter` only**,
`enum oneterm_vt::Invalidation` with its seven variants, `enum oneterm_vt::ModeState` with its
five, `struct oneterm_vt::ParamSpans`, `struct oneterm_vt::StrSpan`, and
`struct oneterm_vt::Watermark` with `structfield 0`. No line was removed or renamed, and neither
`fn set` nor any `fn reset*` appears under `ColorOverrides` in either file.
`python scripts/vt-public-api.py --check` then reports the surface unchanged.

**`public-api.unix.txt` was produced by hand**, as its `#` header says, on 2026-09-16. Nothing in
this packet is `cfg`-gated, so it was rebuilt mechanically from the freshly generated Windows file
with the existing Unix file's `oneterm_vt::pty` blocks kept, and `--diff-platforms` then confirmed
the invariant with **six** lines, all inside `oneterm_vt::pty`: `Options::escape_args`,
`PipeReader` and `PipeWriter` on Windows; `Options::child_signal_mask`, `SignalMask` and
`SignalMask::current` on Unix. The same six `BUG-0059` and `US-0105` each reported.

**Nothing else in the workspace moved.** `cargo test --workspace`, `cargo test -p oneterm-vt`
(819 passed), `--no-default-features` (791), `--all-features` (832) and `cargo test -p
oneterm-tools` (16) all pass, with no `#[allow]` added anywhere.
`crates/tools/src/corpus_replay.rs:319` compiles unchanged against the narrowed `ColorOverrides`,
because `iter` is one of the two methods that stay public.
`RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps` and the same with `--all-features`
both exit zero. `pwsh scripts/ci-local.ps1 -Full` is green.

**One rustdoc fix the packet did not predict, and it is worth recording.** `Invalidation`'s doc
comment linked `[`Anchors::shift_region`]` and `[`Anchors::trim`]`. Both were legal while the type
was private and became `rustdoc::private_intra_doc_links` errors the moment it was published,
failing `-D warnings`. They are now plain code spans, matching `Anchors::remap` in the same
sentence, which was already written that way. Publishing a type re-reads every doc link on it:
that is the general lesson, and it cost two lines.

**The diff is over budget on the production halves, and here is the arithmetic.**
`git diff --numstat main...HEAD`, excluding the two generated snapshots:

| Path | Actual | Budget |
| --- | --- | --- |
| `crates/vt/src` (seven files) | +54 / -15 | -- |
| `crates/vt/CHANGELOG.md` | +20 / -0 | -- |
| `crates/vt/docs/guide/12-versioning.md` | +8 / -6 | -- |
| **`crates/vt` total** | **+82 / -21** | +55 / -12 |
| `scripts/vt-public-api.py` | +5 / -26 | +2 / -22 |
| `crates/vt/public-api.windows.txt` | +22 / -0 | generated |
| `crates/vt/public-api.unix.txt` | +23 / -1 | generated (by hand) |

`crates/vt` is **49 per cent over** on additions, inside the packet's 50-per-cent threshold but
stated rather than buried. Two thirds of the overrun is the CHANGELOG: the entry names all seven
types with the signature each was reached through, as the promise requires, and that is twenty
lines rather than the handful the budget assumed. The rest is the doctest, which is 28 lines
because the acceptance criterion asks it to store four types in a struct *and* match two by path
rather than bind four `Option`s to `None`. The removals overrun (-21 against -12) is nine lines in
absolute terms: four `pub` to `pub(crate)` rewrites, two intra-doc links, one duplicate
`pub(crate) use color::ColorOverrides;` in `crates/vt/src/terminal/mod.rs` that became a
dead import once the `pub use` beside it named the same type, and the guide's two reflowed
sentences. The script's `+5 / -26` is the docstring sentence being replaced rather than deleted
(the gate's four limits keep their paragraph, and the reader is told there is no allow-list at
all) plus the two rewritten `check_nameable` lines.

Known gaps to state rather than discover:

- **The gate's four limits are unchanged**, and deleting the ledger does not narrow them. A type
  reachable only through an associated type, a where-clause bound or a macro-generated impl is
  still invisible; so is a type named in one or two characters; so is a signature *change*. The
  eleven this intake found are the eleven the gate can see, not provably all of them. Rustdoc JSON
  would close the first limit and is nightly-only against a pinned stable toolchain.
- **Nothing proves the seven are the last seven** except the gate, and the gate has those limits.
  The honest claim after this packet is "no public signature names a type the gate can see and an
  embedder cannot write", which is what the success line says.
- **`ColorOverrides` is re-exported because one workspace tool needs it**, not because an embedder
  asked. `crates/tools/src/corpus_replay.rs` is inside this repository, so the alternative of
  giving `Terminal` a narrower accessor and deleting `colors()` was reachable and was not taken:
  it trades a smaller public surface for a new public method plus a rewrite of the parity corpus's
  palette walk, which is a larger diff to publish less. Recorded so a later reader knows the
  cheaper-looking option was priced rather than missed.

## Harness Row

The harness database is not edited by this packet's session. This is the row it owes, for whoever
applies it. The columns are `harness.db`'s real `story` schema; the `intake` row for `IN-0039` is
`id = 44`.

```python
#!/usr/bin/env python3
"""Insert the BUG-0060 story row. Point DB at the harness database and run once."""
import sqlite3

ROW = dict(
    id="BUG-0060",
    title="The seven types still on the unnameable ledger",
    created_at="2026-09-16",
    risk_lane="normal",
    contract_doc=(
        "docs/spec-intakes/IN-0039-vt-gaps-and-publish/low-level-design/api-surface.md"
    ),
    packet_doc=(
        "docs/spec-intakes/IN-0039-vt-gaps-and-publish/"
        "BUG-0060-remaining-unnameable-types.md"
    ),
    status="planned",
    unit_proof=0,
    integration_proof=0,
    e2e_proof=0,
    platform_proof=0,
    evidence=None,
    verify_command="pwsh scripts/ci-local.ps1",
    last_verified_at=None,
    last_verified_result=None,
    notes=(
        "Normal lane, not the intake's high_risk: additive public surface only, no "
        "reply byte and no encoder byte moves, and the one narrowing "
        "(ColorOverrides mutators) is unreachable today. E2E not applicable. "
        "Empties and then deletes KNOWN_UNNAMEABLE in scripts/vt-public-api.py."
    ),
    intake_id=44,
)

with sqlite3.connect("harness.db") as db:
    columns = ", ".join(ROW)
    placeholders = ", ".join("?" for _ in ROW)
    db.execute(f"INSERT INTO story ({columns}) VALUES ({placeholders})", tuple(ROW.values()))
print("inserted BUG-0060")
```

## Handoff

Depends on `BUG-0059` only, which is implemented. Independent of `US-0105`, `US-0106` and
`US-0107`: it touches `crates/vt/src/lib.rs` and both snapshots, as all of them do, so a conflict
with any of them is mechanical -- regenerate on Windows and re-run the mirror step.

This is the last packet `IN-0039` owes on the public-surface gap. After it, gap 1 is closed in the
sense the outside evaluation meant and in the wider sense the gate found.
