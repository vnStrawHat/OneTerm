# Low-Level Design: public API surface

Intake: [`IN-0039`](../IN-0039.md)
HLD: [`../high-level-design.md`](../high-level-design.md)
Topic: api-surface
Date: 2026-09-15

> One concern per file: the four types an outside crate cannot name, the gate that catches the
> fifth, and the semver arithmetic for everything this intake adds. The accepted surface and its
> nine-clause promise live in
> [`IN-0038/low-level-design/api-surface.md`](../../IN-0038-embeddable-vt-core/low-level-design/api-surface.md);
> this file amends it and does not restate it.

## Concern

The evaluation's gap 1, verbatim:

> Four public methods return types that no outside crate can name:
>
> ```text
> Terminal::resize(..)     -> ResizeOutcome    error[E0425]: cannot find type `ResizeOutcome`
> Terminal::cursor_style() -> CursorStyle      error[E0425]: cannot find type `CursorStyle`
> Terminal::sync()         -> &SyncState       (module `terminal` is private)
> Terminal::placements()   -> &[Placement]     (module `graphics` is private)
> ```
>
> They are usable by field access ... but they cannot be stored in a struct, returned from a
> function, or matched on by path. ... This is a two-line fix (re-export them) and it is the only
> outright defect I found in the public surface.

It is not quite a two-line fix, and the difference is the interesting part.

## The four types

| Type | Defined in | Reachable path today | Fields |
| --- | --- | --- | --- |
| `ResizeOutcome` | `crates/vt/src/reflow/mod.rs` | none (`reflow` is `pub(crate)`) | `reflowed: bool`, `rows_trimmed: u32` -- both documented |
| `CursorStyle` | `crates/vt/src/terminal/mode.rs` | none (`terminal` is `pub(crate)`) | `shape: CursorShape`, `blinking: bool` -- both documented; `CursorShape` is already re-exported |
| `SyncState` | `crates/vt/src/snapshot/sync.rs` | none (`snapshot` is `pub(crate)`) | all private; the API is `new`, `begin` and friends |
| `Placement` | `crates/vt/src/graphics/mod.rs` | none (`graphics` is `pub(crate)`) | `id: GraphicId`, `anchor: AnchorId`, `cols: u16`, `rows: u16`, `pixel_size: (u32, u32)` -- **none documented** |

## Design: re-export, and pay the two real costs

```rust
// crates/vt/src/lib.rs
pub use graphics::{GraphicData, Placement, VIRTUAL_CELL};
pub use reflow::{ResizeOutcome, ResizePolicy};
pub use snapshot::{ /* ... */ SyncState /* ... */ };
pub use terminal::{ /* ... */ CursorStyle /* ... */ };
```

That is the four lines. The two costs the evaluation could not see from outside:

**1. `missing_docs` starts firing.** The crate carries `#![warn(missing_docs)]` and CI runs
`RUSTDOCFLAGS="-D warnings" cargo doc`. A `pub` field inside a `pub(crate)` module is exempt today;
re-exporting its type makes it public and the lint fires. `Placement`'s five fields are
undocumented and `SyncState::new` has no doc comment. That is about ten lines of documentation and
it is the reason the fix is +16 rather than +4.

**2. Two more types join the `#[non_exhaustive]` doctrine**, per the rule already accepted in
IN-0038: "frozen value types an embedder constructs and destructures" stay exhaustive; everything
that is "the set we know about today" is marked.

| Type | Marked? | Why |
| --- | --- | --- |
| `ResizeOutcome` | **yes** | it is returned, never constructed by an embedder, and a resize will report more than two facts eventually |
| `Placement` | **yes** | same, and its `pixel_size` tuple is visibly an early shape |
| `SyncState` | not needed | every field is already private, so it is already unconstructible |
| `CursorStyle` | **no** | a frozen value type: shape plus blink is what `DECSCUSR` sets, and an embedder wants to build one as a fallback. It matches the accepted doctrine's exhaustive list (`Pos`, `Size`, `Rgb`, `Style`, ...) |

### `Placement` beside `SnapshotPlacement`

Two public types now carry almost the same name, and that is deliberate rather than overlooked:

| | `Placement` | `SnapshotPlacement` |
| --- | --- | --- |
| Where it comes from | `Terminal::placements()` | `SnapshotState::placements` |
| How it names a position | `anchor: AnchorId` -- it follows its content through scroll and reflow | `row: RowId` plus `col: u16` -- resolved at snapshot time |
| Who reads it | an embedder not going through a `SnapshotState` | the painter |

Rejected alternative, recorded: **rename it to `GraphicPlacement`.** It is free today (the type is
unnameable, so nothing can break) and it would remove the near-collision. Not taken, because
`SnapshotPlacement` already carries its qualifier and `Placement` is the unqualified thing the
snapshot one is a snapshot *of*; renaming both to match would be a bigger diff than the defect.
The doc comment on each now names the other in one line, which is what actually helps a reader.

Rejected alternative, recorded: **delete `Terminal::placements()`.** One fewer public method and no
new name -- the laziest possible fix. Not taken, because the evaluation specifically calls it "the
direct path" for an embedder writing images, and removing a method the only outside review praised
to avoid documenting five fields is the wrong trade.

## Design: the gate

`scripts/vt-public-api.py` catches an item that is added, removed or renamed. It does **not** catch
a public signature referring to a private type -- which is exactly the defect. About 50 lines close
that hole, reusing the machinery already there.

```text
For each public item page rustdoc emitted (the script already walks these):
  parse the item's own signature block -- rustdoc wraps it in <pre class="rust item-decl">
  collect every <a class="struct|enum|trait|type|union|primitive" href="...">
  resolve each href to the module path it points at
  if that module path is not in public_modules():
      record  "<item path>: <type name> is not nameable (defined in <private module>)"
Exit non-zero with the full list.
```

Three things make this cheap rather than a parser project:

- `public_modules()` already exists and already walks the crate root's module links, which is the
  hard half.
- rustdoc already emits the signature as HTML with one `<a>` per named type, so the "types in this
  signature" question is a regex over anchors rather than a Rust parse.
- The same walk already opens every item page for `members()`, so the file read is free.

Known limits, stated because a gate whose limits are unstated gets over-trusted:

- **Only named types in the rendered signature.** A type that appears solely in an associated type,
  a where-clause bound, or a macro-generated impl is not checked.
- **Primitives, tuples and `std` types are skipped** -- their links leave the crate, and an
  off-crate link is not a private module.
- **Re-export cycles.** A type reachable through two paths, one public and one private, passes on
  the public one. That is correct: the embedder can name it.

Invoked as `python scripts/vt-public-api.py --check-nameable`, added to the existing `vt-package`
CI job beside `--check` and `--diff-platforms`. **The acceptance bar is that it fails on `main` and
passes on the branch** -- a gate that has never been seen to fail has not been tested.

## Semver arithmetic for this whole intake

Against the nine-clause promise in the accepted IN-0038 design. The crate is `0.x`, so a **minor**
bump is the breaking one.

| Packet | Change | Clause | Bump |
| --- | --- | --- | --- |
| `BUG-0059` | four new root re-exports | 2 (a new item) | patch |
| `BUG-0059` | `#[non_exhaustive]` on `ResizeOutcome` and `Placement` | neither type was nameable, so no outside code can be affected | patch |
| `US-0105` | `KeyEvent`, `KeyEventKind`, `encode_key_event`, `Terminal::encode_key_event` | 2 | patch |
| `US-0105` | two new fields on `ModeSnapshot`, which is exhaustive today | 1 (a new field on an exhaustive struct) | **minor** |
| `US-0105` | `#[non_exhaustive]` on `ModeSnapshot`, in the same packet | folded into the same minor bump; after it, a field is a patch | -- |
| `US-0105` | `encode_key` returns different bytes under non-empty flags | 6 -- the reply-bytes clause covers what the terminal *answers*; this is what the terminal *sends on the user's behalf*, which the clause does not name. **The clause is amended by this intake to include the `input` encoders**, because a program parsing key bytes is in the same position as one parsing a `DA1` reply | **minor** |
| `US-0106` | `Config::allow_screen_readback` | `Config` is **not** `#[non_exhaustive]` today (see below), so clause 1 | **minor** |
| ~~`US-0106`~~ | ~~`#[non_exhaustive]` on `Config`, in the same packet~~ | **withdrawn** -- the mark is impossible for a struct an embedder constructs; see the correction below | -- |
| `US-0106` | three new reply forms for sequences that previously answered nothing | 6 | **minor**, and a CHANGELOG line naming each |
| `US-0107` | nothing in `crates/vt`'s API | -- | none |

So the intake is **one minor bump in total**, not three: the CHANGELOG's `[Unreleased]` section
accumulates all four packets' entries and the next release carries them as one version. That is
how the crate's own CHANGELOG preamble already describes its relationship to the application's
release line, and it is unchanged by this intake.

The crate is consumed as a git dependency (IN-0038 Open Decision 2, restated by the owner
2026-09-15), so this arithmetic binds **tags**. It binds them exactly as it would bind registry
releases: a consumer pinning a tag is as broken by a rename as one pinning a version, which is the
sentence the accepted IN-0038 design already uses and this one does not soften.

### The one clause this intake amends

Clause 6 of the accepted promise reads:

> Behaviour is not the API, with one exception: **the reply bytes for `DA1`, `DA2`, `DSR`,
> `DECRQM`, `XTVERSION` and the OSC colour queries are a contract.**

`US-0105` shows the list is incomplete in a way that matters. The bytes `input::encode_key` returns
are parsed by the program on the other end of the pseudo-console exactly as a `DA1` reply is, and
changing them breaks that program in the same way. The amended clause, which `US-0105` writes into
the README, the CHANGELOG and guide chapter 12 (the packet that makes the old wording false is the
packet that fixes it), with `US-0106` adding the three query names it introduces:

> **the reply bytes for `DA1`, `DA2`, `DA3`, `DSR`, `DECRQM`, `DECRQSS`, `DECRQCRA`, `XTGETTCAP`,
> `XTVERSION` and the OSC colour queries, and the bytes the `input` encoders produce for a given
> (event, mode snapshot) pair, are a contract.**

## The public surface after this intake

Only the deltas; everything else is unchanged from the accepted list.

### Added

| Item | Packet |
| --- | --- |
| `ResizeOutcome`, `CursorStyle`, `SyncState`, `Placement` (root re-exports) | `BUG-0059` |
| `input::KeyEvent`, `input::KeyEventKind`, `input::encode_key_event` | `US-0105` |
| `Terminal::encode_key_event` | `US-0105` |
| `ModeSnapshot::keyboard_flags`, `ModeSnapshot::modify_other_keys` | `US-0105` |
| `Config::allow_screen_readback` | `US-0106` |

### Newly `#[non_exhaustive]`

`ResizeOutcome`, `Placement` (`BUG-0059`); `ModeSnapshot`, `KeyEvent`, `KeyEventKind` (`US-0105`);
`Config` (`US-0106`).

**`Config` is not marked today, and the accepted design says it should be.**
[`IN-0038/low-level-design/api-surface.md`](../../IN-0038-embeddable-vt-core/low-level-design/api-surface.md)
lists it under "Becomes `#[non_exhaustive]`" with the reason "a new configuration knob must not be
a breaking change"; guide chapter 12 counts the marks that shipped and says **eight** -- seven
enums plus `search::SearchOptions` -- and `Config` is not among them. `grep -rn non_exhaustive
crates/vt/src` confirms it: the mark was designed and not applied.

`US-0106` is the packet that discovers this, because it is the packet that adds the knob the mark
exists to protect. It applies the mark in the same commit. That is cheap and compatible: every
construction site in this repository already uses `Config { ..Config::default() }` (checked:
`crates/terminal/src/handle.rs`, `test_engine.rs`, `test_support.rs`), which is the supported form
under the mark.

Recorded as a **drift finding against IN-0038**, not as new scope: the accepted design and the
shipped code disagree, the packet that touches the type fixes it, and guide chapter 12's count
becomes nine.

> **CORRECTION (`US-0106`, 2026-09-16). The three paragraphs above are wrong, and so is the
> `IN-0038` clause they inherit. `Config` must NOT be marked.**
>
> `Config { ..Config::default() }` is **not** the supported construction form under
> `#[non_exhaustive]`. Rust forbids a struct expression for a non-exhaustive struct outside the
> defining crate **entirely**, functional-update syntax included. The mark was applied during
> `US-0106` and the compiler rejected it (`E0639`) in this crate's own integration tests, the
> `headless` example, six guide doctests and `crates/terminal`. Marking `Config` would require
> replacing struct-literal construction with setters across the whole surface, which costs the
> ergonomics the type's own documentation recommends in order to buy patch-level field additions.
>
> The mark is therefore **not** applied. Guide chapter 12's count stays at **eight** and the
> chapter now states why the type a reader would expect to be marked is not. A new `Config` field
> is a **minor** bump under clause 1, and `Config::allow_screen_readback` is one.
>
> `ResizeOutcome`, `Placement`, `ModeSnapshot`, `KeyEvent` and `KeyEventKind` are unaffected: the
> rule applies to any non-exhaustive struct, so `BUG-0059` and `US-0105` must check the same thing
> for the structs among them -- an enum's `#[non_exhaustive]` costs a `match` arm and nothing else,
> but a struct an embedder constructs cannot carry the mark at all.

### Removed

Nothing. No deprecation shim anywhere, and none is needed: nothing is removed.

## Edge Cases and Failure Modes

- [ ] **`missing_docs` fires on `Placement`'s five fields** -- expected, and it is the packet's
      work rather than a surprise. The doc build is the proof.
- [ ] **The nameability gate fires on something legitimate**, such as a type reachable only through
      `grid` that rustdoc renders oddly. The output names the item and the module, so the fix is
      either a re-export or an entry in a short allow-list. **No allow-list is added in this
      packet**: if one is ever needed, that is a finding worth reading rather than a config knob to
      add speculatively.
- [ ] **Both committed surface files change** -- `public-api.windows.txt` and `public-api.unix.txt`
      each gain the same four items, and `--diff-platforms` must still report that the two differ
      only inside `oneterm_vt::pty`. The Windows file is regenerated on the maintainer's machine
      and the Unix file by CI, or by hand with the `#` note the script already supports.
- [ ] **An embedder was already reaching these types through field access** -- the evaluation's
      `vtprobe` does exactly this (`outcome.reflowed`). Re-exporting breaks nothing: field access
      keeps working and a name is now also available.

## Verification

- [ ] `python scripts/vt-public-api.py --check-nameable` **fails on `main`, naming all four types**,
      and passes on the branch. Attach both outputs to the packet's Evidence.
- [ ] `python scripts/vt-public-api.py --check` after `--update` on both platforms; the diff shows
      exactly the four added items and nothing else.
- [ ] `python scripts/vt-public-api.py --diff-platforms` still passes.
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps` and `--all-features`, both
      clean with `missing_docs` on.
- [ ] A doctest that does what the evaluation's probe did, so the defect can never come back
      silently:

      ```rust
      use oneterm_vt::{CursorStyle, Placement, ResizeOutcome, SyncState};
      let _: Option<ResizeOutcome> = None;
      let _: Option<CursorStyle> = None;
      let _: Option<SyncState> = None;
      let _: Option<Placement> = None;
      ```

      A doctest runs as an **external** crate, which is the only place this can be proven.
- [ ] `cargo test --workspace` -- `crates/terminal` and `crates/terminal-view` must still compile;
      `#[non_exhaustive]` does not apply within a crate but these are other crates, so if either
      builds a `ResizeOutcome` or a `Placement` by literal the compiler says so.
