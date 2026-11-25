# Low-Level Design: Public API surface

Intake: IN-0038
HLD: [`../high-level-design.md`](../high-level-design.md)
Topic: api-surface
Date: 2026-09-15

## Concern

The exact set of `pub` items `oneterm-vt` exposes before and after this intake, the `render` ->
`snapshot` rename, which types become `#[non_exhaustive]`, what is sealed, and what the crate
promises about all of it once anything outside this repository depends on it.

## Before

`crates/vt/src/lib.rs` on `main` @ `36977ca`: three `pub mod` and 42 root re-exports.

```rust
pub mod grid;      // Pos, RowId, RowRef, SeqNo, Size, Viewport, Screen, DEFAULT_SCROLLBACK, SCROLLBACK_MAX, ...
pub mod intern;    // Extras, ExtrasId, GraphicId, Hyperlink, HyperlinkId, Interner
pub mod parser;    // Parser, Dispatch, OscParams, Params, ParamGroups, StringTerm,
                   // OSC_INLINE, OSC_LARGE, MAX_OSC_PARAMS, DCS_MAX_BYTES

pub use cell::{Attrs, Cell, CellContent, CellWidth, Color, NamedColor, Rgb, Semantic, Style};
pub use event::{ClipboardKind, EventBatch, FeedStats, StringTerm, VtEvent};
pub use graphics::{GraphicData, VIRTUAL_CELL};
pub use grid::{Pos, RowId, RowRef, SeqNo, Size, Viewport};
pub use intern::{Extras, ExtrasId, GraphicId, Hyperlink, HyperlinkId, Interner};
pub use reflow::ResizePolicy;
pub use render::{ModeSnapshot, MouseEncoding, MouseProtocol, MouseReporting, Palette,
                 RenderCell, RenderContent, RenderCursor, RenderPlacement, RenderRow,
                 RenderState, RenderUpdate, StyleRun};
pub use selection::{Selection, SelectionKind, SelectionRange, Side};
pub use terminal::{ColorKey, Config, CursorShape, KeyboardFlags, Mode, OscClaims, Terminal};
pub use width::{cluster_width, scalar_width};
```

`Config` is a plain struct with four `pub` fields. `VtEvent` has 13 variants and is exhaustive.
Nothing is `#[non_exhaustive]`. Nothing is sealed.

## After

### Added

| Item | Packet | Kind |
| --- | --- | --- |
| `pub mod input` -- `KeySpec`, `NamedKey`, `KeyMods`, `encode_key`, `TerminalMouseButton`, `MouseModifiers`, `encode_mouse_press`, `encode_mouse_release`, `encode_mouse_move`, `encode_wheel_event` | `US-0099` | moved in |
| `pub mod search` -- `SearchOptions`, `SearchPattern`, `SearchMatch`, `GridText`, `search_grid_text` | `US-0100` | moved in |
| `pub mod pty` (`#[cfg(feature = "pty")]`) -- `PseudoConsole`, `Options`, `Shell`, `WindowSize`, `GlyphWidth`, `ChildEvent`, `EventedReadWrite`, `EventedPty`, `OnResize`, `PTY_CHILD_EVENT_TOKEN`, `PTY_READ_WRITE_TOKEN`; `SignalMask` on Unix; `PipeReader`, `PipeWriter` on Windows | `US-0104` | moved in from `oneterm-pty` |
| `OscRoute`, `OscRoutes` | `US-0098` | replaces `OscClaims` |
| `Progress`, `ShellMark` | `US-0098` | new |
| Seven `VtEvent` variants (`Cwd`, `IconName`, `Notification`, `Pointer`, `Progress`, `ShellMark`, `CursorStyleChanged`) | `US-0098` | new |
| `Config::product_name` | `US-0097` | new field |
| `MouseEncoding::Urxvt` (`? 1015`), `MouseProtocol::X10` (`? 9`) | `US-0102` | new variants |

### Removed

| Item | Packet | Replacement |
| --- | --- | --- |
| `OscClaims`, `::NATIVE`, `::is_native`, `::claim`, `::claim_large`, `::is_claimed`, `::allows_large` | `US-0098` | `OscRoutes` and `OscRoute` |
| `Config::osc_claims` | `US-0098` | `Config::osc_routes` |
| `Render*` names | `US-0101` | `Snapshot*` (below) |

No deprecation shims anywhere. Nothing outside this repository depends on the crate yet, so
`US-0097` is the last moment these renames are free; after it they cost a minor bump each. The
owner ruling of 2026-09-15 -- consumed by git tag rather than published to crates.io -- does not
buy any more time here. A consumer pinning a tag is as broken by a rename as one pinning a version,
and the first outside pin is the deadline either way.

### Renamed: `render` -> `snapshot`

| Before | After | External uses today |
| --- | --- | --- |
| `mod render` | `mod snapshot` | 0 (private module) |
| `RenderState` | `SnapshotState` | 24 |
| `RenderUpdate` | `SnapshotUpdate` | 23 |
| `RenderRow` | `SnapshotRow` | 7 |
| `RenderContent` | `SnapshotContent` | 8 |
| `RenderCell` | `SnapshotCell` | 3 |
| `RenderCursor` | `SnapshotCursor` | 3 |
| `RenderPlacement` | `SnapshotPlacement` | 3 |
| `Terminal::render_update` | `Terminal::snapshot_update` | -- |
| `StyleRun`, `ModeSnapshot`, `Palette`, `MouseEncoding`, `MouseProtocol`, `MouseReporting` | unchanged | 51, 23, 1, and mouse |

`ModeSnapshot` keeps its name -- it is already right -- and `Palette` and `StyleRun` never said
"render".

**Why `snapshot` and not `frame`.** Both candidates mean "no drawing", so the tie-break is what each
implies about *cadence and ownership*. "Frame" is a unit of animation: a frame rate, a frame buffer,
a frame budget. Calling the module `frame` would say the engine produces one of these per display
refresh, on a clock, pushed to a consumer -- which is the opposite of what it does. The module is a
**pull-side, change-scoped read model**: the embedder asks when it wants to, gets only the rows whose
`SeqNo` moved since its last ask (`DEC-0015`), and may ask twice in one display frame or not at all
for a minute. "Snapshot" names exactly that -- a consistent read of engine state at an instant, taken
by the reader -- and it composes correctly in the type names: `SnapshotUpdate` reads as "what changed
since your last snapshot", while `FrameUpdate` would read as "the next frame", implying a cadence the
engine does not have and must not acquire. It also matches the vocabulary already in the repository:
`docs/terminal-backend.md` section 5.2 is titled "Snapshot vs live borrow", and `ModeSnapshot`
already exists in this very module.

**One collision to resolve in the same packet.** `crates/terminal/src/content.rs:119` already
publishes `oneterm_terminal::SnapshotCell`, used by `terminal-view` in three files. `US-0101`
renames the adapter's type to `ContentCell`, which is what it actually is -- an owned, narrowed row
cell for the URL and completion scanners, not an engine snapshot cell. Four files change; the engine
keeps the better name.

### Becomes `#[non_exhaustive]`

An external embedder writes `match` arms against these, and this intake and `US-0102` both add
variants. Marking them now is free; marking them after the first release is a breaking change.

| Type | Why |
| --- | --- |
| `VtEvent` | seven variants added here, more whenever an OSC built-in is added |
| `OscRoute` | a fifth routing mode is conceivable |
| `Progress`, `ShellMark` | both mirror evolving de-facto specs |
| `Mode` | grows every time a private mode is implemented (`US-0102` adds two) |
| `MouseProtocol`, `MouseEncoding` | `US-0102` adds `X10` and `Urxvt` |
| `NamedColor`, `Semantic`, `CellWidth`, `SelectionKind`, `CursorShape`, `ResizePolicy` | small, but all are "the set we know about today" |
| `Config` | a new configuration knob must not be a breaking change; with `#[non_exhaustive]`, `Config::default()` plus field assignment is the only construction form, which is already how every call site builds it |
| `FeedStats` | a new counter is a routine engine change |
| `SearchOptions`, `SearchPattern` | `SearchPattern::Regex` is feature-gated, so the enum must be non-exhaustive or a `--no-default-features` build changes the match arms an embedder must write |
| `KeySpec`, `NamedKey` | new named keys arrive with keyboard protocols |
| `pty::ChildEvent`, `pty::GlyphWidth`, `pty::Options` | `US-0104`. `ChildEvent` has one variant today and will gain signal reporting; `GlyphWidth` tracks mode 2027 and its successors; `Options` is a configuration struct and grows for the same reason `Config` does. `Shell` and `WindowSize` stay exhaustive -- they are frozen value types the embedder constructs. |

Deliberately **exhaustive** (frozen value types an embedder constructs and destructures, where
non-exhaustive would be a usability tax for no benefit): `Pos`, `Size`, `RowId`, `SeqNo`, `Rgb`,
`Color`, `Style`, `Attrs`, `Viewport`, `SelectionRange`, `Side`, `StringTerm`, `ClipboardKind`,
`KeyMods`, `MouseModifiers`, `SearchMatch`, `StyleRun`.

### Sealed

One trait is public and must not be implemented outside the crate:

```rust
// crates/vt/src/parser/mod.rs
pub trait Dispatch: private::Sealed { /* .. */ }
mod private { pub trait Sealed {} impl Sealed for crate::terminal::dispatch::Handler<'_> {} }
```

`Dispatch` is `pub` only because `parser::Parser::advance<D: Dispatch>` is `pub` -- the session
logger in `crates/terminal/src/logging.rs` drives its own parser. It is the engine's internal
semantic contract, it changes whenever a sequence class is added, and an external implementation
would break on every such change. Sealing it keeps `parser` usable as a raw state machine while
making the trait's evolution a non-event.

Nothing else is sealed. `Interner`, `Screen` and the grid types are concrete.

## Interfaces

The crate root after the intake:

```rust
#![warn(missing_docs)]

pub mod grid;
pub mod input;    // US-0099
pub mod intern;
pub mod parser;
pub mod search;   // US-0100

/// The pseudo-console transport. Default-on, and the only feature-gated
/// module: `--no-default-features` compiles no platform code (US-0104).
#[cfg(feature = "pty")]
pub mod pty;      // US-0104

pub use cell::{Attrs, Cell, CellContent, CellWidth, Color, NamedColor, Rgb, Semantic, Style};
pub use event::{ClipboardKind, EventBatch, FeedStats, Progress, ShellMark, StringTerm, VtEvent};
pub use graphics::{GraphicData, VIRTUAL_CELL};
pub use grid::{Pos, RowId, RowRef, SeqNo, Size, Viewport};
pub use intern::{Extras, ExtrasId, GraphicId, Hyperlink, HyperlinkId, Interner};
pub use reflow::ResizePolicy;
pub use selection::{Selection, SelectionKind, SelectionRange, Side};
pub use snapshot::{ModeSnapshot, MouseEncoding, MouseProtocol, MouseReporting, Palette,
                   SnapshotCell, SnapshotContent, SnapshotCursor, SnapshotPlacement,
                   SnapshotRow, SnapshotState, SnapshotUpdate, StyleRun};
pub use terminal::{ColorKey, Config, CursorShape, KeyboardFlags, Mode, OscRoute, OscRoutes,
                   Terminal};
pub use width::{cluster_width, scalar_width};
```

## The semver promise

Stated in the README and the CHANGELOG, and binding from the first tag anything outside this
repository pins. The crate is consumed as a git dependency rather than from crates.io (owner ruling
2026-09-15), so **the promise below applies to tags exactly as it would to releases**: the version
in a tag's `Cargo.toml` is the number these rules are about, and a consumer reads it the same way.
The rules themselves are unchanged:

1. The crate is `0.x`. Cargo treats a **minor** bump as breaking, and so does this crate: every
   removal, signature change, or new variant on an exhaustive enum is a minor bump with a CHANGELOG
   entry naming the item.
2. A new variant on a `#[non_exhaustive]` enum, a new field on a `#[non_exhaustive]` struct, a new
   method, a new module, or a new default-off feature is a **patch** bump.
3. An MSRV raise is a **minor** bump. Never a patch.
4. `parser::Dispatch` is sealed; its method set changes at patch level.
5. Anything reachable only with a non-default feature carries the same promise as the default
   surface. A feature is never a stability escape hatch.
6. Behaviour is not the API, with one exception: **the reply bytes for `DA1`, `DA2`, `DSR`,
   `DECRQM`, `XTVERSION` and the OSC colour queries are a contract.** Programs parse them. Changing
   one is a minor bump and a CHANGELOG line, even though no Rust signature moved. `Config::
   product_name` exists so an embedder can change the identity half without the engine changing the
   shape half.
7. `FeedStats` counter *semantics* are documented per field and are part of the contract; a counter
   that starts counting a different thing is a minor bump.
8. **`polling` is a public dependency of the `pty` feature** (`US-0104`). `polling::Poller`,
   `polling::Event` and `polling::PollMode` appear in the `EventedReadWrite` signatures, so a
   `polling` **major** bump is a breaking change to this crate and is a **minor** bump with a
   CHANGELOG line naming the old and new versions. It is the crate's only public dependency, and
   [`pty.md`](pty.md) records the redesign that would remove it and the trigger for doing so. No
   other optional dependency's type may appear in a public signature -- that rule is unchanged and
   is why `SearchPattern::Regex` carries no `regex` type.
9. `pty::PseudoConsole` is a **different type on each platform**: `windows::PseudoConsole` and
   `unix::PseudoConsole` share the trait set, not the inherent API. Portable embedder code goes
   through `EventedPty + OnResize`; anything else is platform code and is documented as such.

What is explicitly **not** promised: grid internals reachable through `grid::Screen`, the exact
`SeqNo` values, allocation behaviour, and performance. `DEC-0015`'s row-identity guarantee (a
`RowId` names the same content for as long as that content is live) **is** promised, because an
embedder's caches are keyed by it.

## Edge Cases and Failure Modes

- [ ] An embedder constructs `Config` with a struct literal -- breaks at `#[non_exhaustive]`. This
  is intended and is the point; `Config { ..Default::default() }` is the supported form and is what
  every call site in this repository already does.
- [ ] `--no-default-features` changes `SearchPattern`'s variant set -- handled by
  `#[non_exhaustive]`; an embedder's `match` needs a `_` arm either way.
- [ ] `terminal-view` names `RenderState` 24 times -- a mechanical rename in one packet, caught by
  the compiler, not by review.
- [ ] Two `SnapshotCell` types during the `US-0101` rename -- avoided by renaming the adapter's to
  `ContentCell` in the same commit; the compiler catches any miss.
- [ ] `missing_docs` fires on the 42 re-exports -- it does not; re-exports inherit the item's docs.
  It fires on undocumented `pub` fields and variants, which is exactly the gap `US-0097` closes.

## Verification

- [x] **The surface is enumerated, not asserted by eye.** `cargo public-api` is not a dependency
  this repository has; instead `US-0097` adds `scripts/vt-public-api.py` (about 160 lines). It reads
  the **HTML** rustdoc emits, not `cargo doc --output-format json` as this document first proposed:
  the JSON format is nightly-only and `rust-toolchain.toml` pins stable `1.96.0`. It prints every
  public item, sorted, with its fields, variants, associated constants and inherent methods under
  it, and it keeps only items reachable through the crate's public module paths, so renaming a
  private module is not a public-API change. The output is committed and a CI step diffs it, so any
  change to the public surface has to be an intentional line in a diff.

  **Two files, not one, from `US-0104`.** `pub mod pty` publishes `PipeReader`, `PipeWriter` and
  `Options::escape_args` on Windows and `SignalMask` and `Options::child_signal_mask` on Unix, and
  `cargo doc` renders only the host's half, so the snapshot is `crates/vt/public-api.windows.txt`
  and `crates/vt/public-api.unix.txt`. The script selects by host for `--check` and `--update` and
  prints which file it used; `--diff-platforms` reports every line the two disagree on and fails if
  any of them is outside `oneterm_vt::pty`. That is the invariant the split has to keep: the
  engine's surface is identical on both platforms and only the transport's cfg-gated items
  differ.

  What that costs: an item, field, variant or method that is **added, removed or renamed** is
  caught; a **signature change is not**. Rustdoc JSON is the upgrade, and the script switches to it
  the day the pinned toolchain can produce it.
- [ ] `cargo doc -p oneterm-vt --no-deps` is warning-free with `#![warn(missing_docs)]` and the
  workspace lint table (which denies warnings in CI).
- [ ] `cargo build -p oneterm-vt --no-default-features` and `--all-features` both clean.
- [ ] `cargo test --workspace` after each rename packet, with no `#[allow]` added anywhere.
- [ ] A compile-fail test (`trybuild` is **not** added; a `#[doc = include_str!]` doctest marked
  `compile_fail` instead) proves `Dispatch` cannot be implemented outside the crate and that `Config`
  cannot be built by struct literal.
