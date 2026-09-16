# Changelog

All notable changes to `oneterm-vt` are recorded here, in the shape
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) describes. An entry belongs here when it
changes what an embedder compiles against or what bytes the terminal replies with.

## The promise

The crate is not on crates.io: other projects depend on it by git, pinned to a tag. The promise
below applies to those tags exactly as it would to published releases.

The crate is `0.x`, and Cargo treats a minor bump as breaking. So does this crate:

1. A removal, a signature change, or a new variant on an exhaustive enum is a **minor** bump, with
   an entry below naming the item.
2. A new method, a new module, a new default-off feature, or a new variant on a
   `#[non_exhaustive]` enum is a **patch** bump.
3. Raising the minimum supported Rust version is a **minor** bump. Never a patch.
4. `parser::Dispatch` is the engine's internal semantic contract; its method set changes at patch
   level.
5. Anything reachable only with a non-default feature carries the same promise as the default
   surface. A feature is never a stability escape hatch.
6. Behaviour is not the API, with one exception: **the reply bytes for `DA1`, `DA2`, `DA3`, `DSR`,
   `DECRQM`, `XTVERSION`, `DECRQCRA`, `DECRQSS`, `XTGETTCAP` and the OSC colour queries, and the
   bytes the `input` encoders produce for a given (event, mode snapshot) pair, are a contract**,
   because programs parse them.
   Changing one is a minor bump and an entry below, even though no Rust signature moved.
   `Config::product_name` exists so an embedder can change the identity half of those replies
   without the engine changing the shape half.
7. `FeedStats` counter semantics are documented per field and are part of the contract. A counter
   that starts counting a different thing is a minor bump.

Not promised: grid internals reachable through `grid::Screen`, the exact `SeqNo` values,
allocation behaviour, and performance. The row-identity guarantee -- a `RowId` names the same
content for as long as that content is live -- **is** promised, because embedder caches are keyed
by it.

The version number is shared with the application this engine was written for, so a release can
carry no API change at all. Such a release says so below rather than being omitted.

## [Unreleased]

### Added

- `ResizeOutcome`, `CursorStyle`, `SyncState` and `Placement` are re-exported from the crate root.
  All four were already **returned** by a public `Terminal` method -- `resize`, `cursor_style`,
  `sync` and `placements` -- but were defined in a `pub(crate)` module, so no embedder could write
  the name: the values were usable by field access and could not be stored in a struct, returned
  from a function or matched on by path. Nothing changed about what the four methods do.

  `ResizeOutcome` and `Placement` are `#[non_exhaustive]`; neither was nameable before, so no
  outside code can be affected. `Placement`'s five fields and `SyncState::new` are documented now
  that they are publicly reachable.

  `scripts/vt-public-api.py --check-nameable` is the gate that keeps this from coming back: it
  fails CI on a public signature naming a type defined in a private module.
- `ModeState`, `StrSpan`, `ByteSpan`, `ParamSpans`, `ColorOverrides`, `Watermark` and
  `Invalidation` are re-exported from the crate root. Each was already returned or accepted by a
  public signature -- `Mode::inert_state`, ten `VtEvent` payloads, `VtEvent::Reply`,
  `VtEvent::Osc`, `Terminal::colors`, `SnapshotState::watermark` and `Selection::invalidated_by` --
  and none could be written down, so none could be stored in a struct or named in a signature of
  the embedder's own. `Invalidation` is the one that was worse than a spelling problem: it is an
  **argument**, so `Selection::invalidated_by` was a public method no embedder could call at all.

  `Invalidation` is `#[non_exhaustive]`; it was not nameable before, so no outside code can be
  affected, and an embedder constructs its variants rather than matching on them. `ModeState`'s
  four undocumented variants and `ColorOverrides::get` are documented now that they are publicly
  reachable.

  `scripts/vt-public-api.py` no longer carries an allow-list at all: the seven above were the last
  entries on it, and the gate now has nowhere to record an exception.
- **`DECRQCRA`, `DECRQSS` and `XTGETTCAP` are answered.** All three were parsed and counted
  unhandled; none was ever answered, which is why no outside conformance harness could score this
  engine and why tmux and neovim capability probes went unanswered.
  - `DECRQCRA` (`CSI Pid ; Pp ; Pt ; Pl ; Pb ; Pr * y`) replies `DCS Pid ! ~ xxxx ST`. **One
    checksum variant is implemented and none is negotiated**: xterm's `checksumExtension: 7` --
    the positive sum of **every** Unicode scalar value in each cell (the base character and each
    combining mark, which is xterm's `combData` walk while `csBYTE` is clear), masked to 16 bits,
    no attribute contribution, no negation, no trimming, an unwritten cell counting as `U+0020`.
    A program written against xterm's *default* (negated, with attributes) will disagree. One
    difference from xterm survives at extension 7 and is in the rectangle rather than the sum:
    xterm's `validRect` rejects a rectangle outside the page where this engine clamps it. Guide
    chapter 11 states the variant beside a doctest that pins the digits.
  - `DECRQSS` (`DCS $ q`) reports `m`, `r`, `SP q`, `" q` and `" p`; every other setting takes the
    invalid reply `DCS 0 $ r ST`.
  - `XTGETTCAP` (`DCS + q`) answers from a table compiled into the crate. The engine reads no
    terminfo database, no environment variable and no file.

  The reply bytes of all three are a contract from here on, under clause 6 above.
- `Config::allow_screen_readback`, **default `false`**, which gates `DECRQCRA`. Shut, the sequence
  answers nothing and is counted in `FeedStats::unhandled_sequences` -- byte for byte what the
  engine did before it was implemented -- so this release changes no reply an existing embedder
  can observe. A **minor** bump under clause 1: `Config` is not `#[non_exhaustive]`, and cannot be
  without replacing struct-literal construction with setters (see guide chapter 12).

- `guide::ch14_performance`, a chapter that publishes a measured throughput figure for the first
  time: the command that produced it, the machine it ran on, the spread across cycles, the
  transport ceiling beside it, and the list of things it is not evidence of -- no renderer, no
  pseudo-console, and no comparison against another engine. The repository gains a committed
  baseline and a `vt-bench grid --check` trip-wire that fails a fixture once it has become twice as
  slow; it runs by hand, never in continuous integration, and no number here is a promise.
  Performance stays outside the promise above.

- `input::KeyEvent`, `input::KeyEventKind`, `input::encode_key_event` and
  `Terminal::encode_key_event`: everything the enhanced keyboard protocols can report about one
  key event -- press, repeat or release, the shifted and base-layout keys, and the text the event
  would insert. `KeyEvent::new(key, mods)` builds a plain press; the optional fields are omitted
  from the encoding when the platform does not know them, which the protocol allows.

- `ModeSnapshot::keyboard_flags` and `ModeSnapshot::modify_other_keys`, the two keyboard protocols
  the encoders now read. See **Changed** for what the mark on `ModeSnapshot` costs you.

- `guide`, a public module that carries the embedder's guide: fourteen Markdown chapters rendered
  by `cargo doc` beside the API reference, one empty module each. It adds no item and no
  dependency, and every Rust block in it is a doctest, so a chapter that describes an API the
  crate no longer has fails the build rather than misleading a reader.

- `pty`, a cargo feature that is **on by default**, and the `oneterm_vt::pty` module behind it: a
  child process behind a ConPTY on Windows or an `openpty` on Unix, exposed as a passive pollable
  object. `PseudoConsole`, `Options`, `Shell`, `WindowSize`, `GlyphWidth`, `ChildEvent`, the
  `EventedReadWrite` / `EventedPty` / `OnResize` traits, `PTY_CHILD_EVENT_TOKEN` and
  `PTY_READ_WRITE_TOKEN`; `SignalMask` on Unix, `PipeReader` and `PipeWriter` on Windows. The code
  moved here from a separate crate; nothing about what it does changed.

  It adds `polling`, plus `windows-sys` on Windows or `libc` on Unix, and it is the only feature
  that adds a dependency. `--no-default-features` removes the module, the traits and all three, and
  leaves the same six leaf dependencies the crate had before.

  `polling` is therefore a **public** dependency of this crate under `pty`: `Poller`, `Event` and
  `PollMode` are in the `EventedReadWrite` signatures, so a `polling` major bump is a breaking
  change here and will get an entry naming both versions.

  The crate ships no Windows console host. See the feature table in `README.md` for what that means
  for Sixel in a local shell.
- `Config::product_name`: what the terminal calls itself in `XTVERSION` (`CSI > 0 q`) and `DA2`
  (`CSI > c`). `None` answers `oneterm-vt(<version>)`. The value is sanitised before it is used:
  C0, `DEL` and C1 controls are dropped so a name cannot end the DCS string early, and the result
  is cut to 64 bytes. `DA2` packs the trailing `(<major>.<minor>.<patch>)` into one number with
  each component saturating at 99.
- `README.md`, `CHANGELOG.md`, `LICENSE`, `NOTICE` and `examples/headless.rs`: the crate is
  packaged so another project can depend on it by git, and documented so somebody who has never
  seen the OneTerm repository can use it.
- `search`: scrollback search, in two phases. `GridText::from_terminal` copies one `char` per
  cell under the embedder's lock; `search_grid_text` matches against that copy without it, and
  returns `SearchMatch`es keyed by `RowId`. `SearchOptions` carries `case_sensitive` and
  `whole_word`. Neither form of match crosses a row boundary.
- The `regex` feature, **off by default**: it adds `SearchPattern::Regex(&regex::Regex)` and makes
  `regex` an optional dependency. With the feature off the crate still has no dependency beyond the
  six it always had. `SearchOptions` is ignored for a regex — the pattern owns its own `(?i)` and
  `\b` — in every build; a debug assertion fires when either field is non-default, but a
  release build carries no such check and silently ignores them. `SearchPattern` is
  `#[non_exhaustive]` so the same `match` compiles either way. A regular expression that can
  match the empty string reports one zero-width `SearchMatch` per position, the position past
  the last cell included, so `start_col` can be one past the last column.
- `SearchOptions` is `#[non_exhaustive]`: a future option must not be a breaking change.
  `SearchOptions::default()` plus field assignment is the construction form.
- `input`: key and mouse encoding. `KeySpec`, `KeyMods`, `NamedKey`, `encode_key`,
  `TerminalMouseButton`, `MouseModifiers`, `encode_mouse_press`, `encode_mouse_release`,
  `encode_mouse_move` and `encode_wheel_event` turn a key press or a mouse event into the bytes
  the program on the far end expects, following the X10, 1005 and SGR-1006 rules and the xterm
  control table. The types are framework-neutral and the functions have no side effects, so
  scrolling, selection and shift-tracking stay with the embedder's input handling. Not one byte
  changed in the move: the encoders and their whole test suite came across unaltered.
  `KeySpec` and `NamedKey` are `#[non_exhaustive]`: new named keys arrive with new keyboard
  protocols, so keep a `_` arm. `KeyMods`, `MouseModifiers` and `TerminalMouseButton` are
  deliberately exhaustive — a terminal has three mouse buttons and three modifiers that matter.
- `Terminal::encode_key`: `input::encode_key` with this terminal's own modes, so encoding one key
  does not need a `ModeSnapshot` fetched first.
- `OscRoute` and `OscRoutes`: what the engine does with one OSC number, as data. `Builtin` (its own
  handler runs), `BuiltinAndForward` (the handler runs **and** the raw parameters follow as
  `VtEvent::Osc`, typed event first), `Forward` (the handler is skipped) and `Drop`. An embedder can
  now add a number the engine has never heard of, override a built-in, or watch one, without a fork
  and without any of its code running inside `feed`. `OscRoutes::BUILTIN` publishes the twenty
  numbers the engine implements; `route`, `route_all`, `large`, `get`, `allows_large`,
  `has_builtin` and `overrides` are the whole surface.
- Six OSC numbers are now built in, each with a typed event: `OSC 1` icon name
  (`VtEvent::IconName`), `OSC 7` working directory (`VtEvent::Cwd { host, path }`, percent-decoded
  and with a `file:///C:/...` drive URL's leading slash stripped, but otherwise unresolved),
  `OSC 9` desktop notification (`VtEvent::Notification`) and `OSC 9;4` ConEmu taskbar progress
  (`VtEvent::Progress(Progress)`), `OSC 22` pointer shape (`VtEvent::Pointer`), `OSC 50`
  (`VtEvent::CursorStyleChanged`, alongside the cursor-shape change it already applied) and
  `OSC 133` (`VtEvent::ShellMark(ShellMark)`, alongside the cell semantics and anchors it already
  recorded).
- `Progress` and `ShellMark`, both `#[non_exhaustive]`.
- Eight conformance gaps closed, each against a published specification.
  - `? 5` (DECSCNM, reverse video), as `ModeSnapshot::reverse_video`. A screen-level flag and never
    a cell attribute: the embedder swaps the two defaults when it resolves the palette, and no
    cell's own style changes, so `? 5 l` restores exactly what was there.
  - `? 9` (X10 mouse), as `MouseReporting::X10`. A button **press** only, with no modifier bits, no
    release and no motion — the encoders return an **empty** `Vec` for an event the mode does not
    report, so a caller writes nothing.
  - `? 1015` (urxvt mouse), as `MouseEncoding::Urxvt`: `CSI Cb ; Cx ; Cy M`, the legacy values as
    decimal parameters, which is what lifts the 223-column ceiling. `Cb` keeps the `+ 32` offset
    and a release is still the fixed button 3; 1015 changes the transport, not the semantics.
  - `? 2027` (grapheme clustering) now **works** rather than being recognised and inert: while set,
    the print path segments its run into grapheme clusters and measures each with `cluster_width`,
    so a ZWJ family lands in one cell. While reset nothing changes.

    A cluster split by a `feed` boundary is measured whole: the engine carries the last cluster of
    a printed run and re-places it when the next run extends it. Any dispatch that is not a print
    breaks the carry, and so does a resize. A cluster past 32 scalars is not carried and the drop
    is counted, so a stream that feeds one unbounded cluster a scalar at a time cannot make the
    re-placing quadratic.

    `cluster_width` needs a **base** before a presentation selector decides anything: a cluster of
    combining scalars alone — a stray `VS16`, a leading combining mark, the tail of a keycap split
    in front of its selector — is zero-width and joins the cell on its left, as it does with the
    mode reset. A bare `VS16` used to measure two columns.
  - `LS2` (`ESC n`), `LS3` (`ESC o`), `SS2` (`ESC N`) and `SS3` (`ESC O`), which are what make a
    `G2` or `G3` designation printable at all. A single shift is consumed by the next printed
    character and by nothing else, so an intervening escape sequence does not eat it.
  - `OSC 17` and `OSC 19` (the selection background and foreground), set and queried as `OSC 10`
    and `OSC 11` are, through the new `ColorKey::SelectionBackground` and
    `ColorKey::SelectionForeground`. One parameter each, not xterm's advancing multi-parameter
    form. There is no `OSC 117` / `119` reset; `RIS` clears them.
  - `DA3` (`CSI = c`) answers `DCS ! | 00000000 ST`, xterm's DECRPTUI reply for a terminal with no
    manufacturing site and no serial number. Only `Ps == 0` answers, as for `DA1` and `DA2`.
- `FeedStats::dropped_cluster_carries`: grapheme clusters that grew past the cross-chunk carry
  limit under `? 2027`, so a continuation arriving in a later `feed` starts a cluster of its own.
  Zero for every well-formed stream.

### Changed

- **Behaviour, no signature: a Sixel's footprint in cells comes from your cell size, not from
  `VIRTUAL_CELL`.** An image now covers `ceil(width / cell_width)` x `ceil(height / cell_height)`
  cells against the size you passed to `Terminal::set_cell_pixels`, and the cursor walks down
  `bands * 6 / cell_height` rows. Before, both used the VT340 virtual cell of 10x20 whatever you
  had told the engine, so a program that sized an image from the `CSI 14 t` reply -- the number
  `set_cell_pixels` feeds -- had it drawn at `real_cell / (10, 20)` of the size it meant: smaller
  on a 9x18 cell, almost double on the 18x36 cell of the same font at 200 % display scale.

  **An embedder that never calls `set_cell_pixels` is unaffected, exactly.** The VT340 cell is the
  fallback, the walk is the identical `bands * 6 / 20`, and every byte of the reference corpus is
  unchanged. Both axes must be non-zero to count, so a half-set `(9, 0)` falls back whole rather
  than dividing by zero.

  `Placement::cols`/`rows` and `SnapshotPlacement::cols`/`rows` are what change value; no type,
  field or signature moved. A renderer should draw the image at `pixel_size` -- one image pixel to
  one device pixel -- anchored at the placement's top-left cell and **clipped** to `cols` x `rows`,
  rather than scaling it to the footprint as the old rule required. Guide chapter 8 has the
  contract and the two cases where the clip is not a no-op.
- **Breaking on paper only.** `ColorOverrides::set`, `reset`, `reset_indexed` and `reset_all` are
  `pub(crate)`; the public surface of the type is `get` and `iter`. All four need `&mut self` and
  the only accessor, `Terminal::colors`, hands out a shared reference -- and the type had no name
  outside the crate until this release, so no embedder could have held one, let alone called them.
  Publishing an unreachable mutator is a worse surface than not publishing it.
- **Breaking, and the point of the release.** `input::encode_key` and `Terminal::encode_key`
  honour the kitty keyboard protocol and xterm's `modifyOtherKeys`. The engine has answered
  `CSI ? u` with the pushed flags since the flag stack shipped and then sent legacy bytes anyway:
  a program that negotiated the protocol and was told "yes" received keys it could not parse.
  **The bytes returned for a given key therefore change whenever a program has pushed a kitty flag
  or set a `modifyOtherKeys` level.** With both at their defaults -- which is every program that
  never asked -- the bytes are identical to before, verified across the full cross-product of keys,
  modifiers and mode snapshots.

  Clause 6 of the promise above is amended by this release to cover the `input` encoders: a program
  parsing key bytes is in exactly the position of one parsing a `DA1` reply, so a change to them is
  a minor bump and an entry here.

  Guide chapter 6 has the decision ladder, the flag table, the three remaining ceilings -- modifier
  values `1`-`8` only, no private-use keypad, lock, media or modifier keys, and an un-shifted key
  code derived from the PC-101 shift relation rather than from your layout -- and a table of every
  place the legacy rung and the kitty rung deliberately disagree.

- **Breaking, clause 6.** `CSI ? u` answers the **live** keyboard flags rather than the top of the
  flag stack. `CSI = Ps ; Pm u` sets the live flags without pushing, so an application following the
  specification's own detection recipe -- set the enhancements, then query -- was told the terminal
  implements none of them. The query and the encoder now read the same value, which is the whole
  point of the query.

- **Breaking, clause 6.** `Ctrl+~` is `0x1e` on the legacy rung, not `~`. The specification's legacy
  ctrl table has the row and `ctrl_bytes` fell through to the character itself; `Ctrl+^` was already
  `0x1e`, so the two spellings of the same key now agree.

- **Breaking, clause 6.** `F15` is `CSI 28 ~` on the legacy rung, not `CSI 1 ; 2 R`. The old form is
  byte-identical to a Cursor Position Report for row 1, column 2, so a program reading replies and
  key bytes from one stream could not tell them apart -- and no keyboard flag removed it, because
  the three enhancement flags that leave a functional key on the legacy rung left it reachable.
  `CSI 28 ~` is the DEC VT220 code (terminfo `kf15` on vt220 and rxvt) and collides with nothing.
  This is the same exception the kitty specification makes for `F3`, for the same reason.

  These two are the only bytes on the legacy rung that moved in this release, and the equivalence
  run names both rather than allowing a tolerance: it asserts the new byte and the old one, counts
  the cases, and pins the count, so a third row cannot move unnoticed.

- **Breaking.** `ModeSnapshot` is `#[non_exhaustive]`. Two things change for you, and only the
  first is a benefit: a new field on it is a patch release from now on, and **you can no longer
  build one with a struct expression, functional update syntax included**. Code that wrote
  `ModeSnapshot { app_cursor: true, ..Default::default() }` stops compiling with `E0639`. Take one
  from `Terminal::mode_snapshot()`, which is what you want in almost every case, or start from
  `ModeSnapshot::default()` and assign each field. The two new fields above are the same minor
  bump, so this costs nothing extra in this release and saves one in the next.

  `input::KeyEvent` and `input::KeyEventKind` are marked too and cost nothing, being new:
  `KeyEvent::new(key, mods)` is the constructor and `..KeyEvent::new(..)` is likewise refused.

- **Breaking.** The read model is called *snapshot*, not *render*: the crate hands out a consistent
  read of engine state and never draws, and its largest module no longer claims otherwise. The
  module is private, so only the re-exported names and the one method are visible:

  | Before | After |
  | --- | --- |
  | `RenderState` | `SnapshotState` |
  | `RenderUpdate` | `SnapshotUpdate` |
  | `RenderRow` | `SnapshotRow` |
  | `RenderContent` | `SnapshotContent` |
  | `RenderCell` | `SnapshotCell` |
  | `RenderCursor` | `SnapshotCursor` |
  | `RenderPlacement` | `SnapshotPlacement` |
  | `Terminal::render_update` | `Terminal::snapshot_update` |

  `ModeSnapshot`, `Palette`, `StyleRun`, `MouseEncoding`, `MouseProtocol` and `MouseReporting` keep
  their names. Nothing else changed: same fields, same variants, same signatures, same behaviour.
  There is no deprecation shim, for the same reason as `OscRoutes` above.

- **Breaking.** `OscClaims` is replaced by `OscRoutes` and `Config::osc_claims` by
  `Config::osc_routes`. `OscClaims::NATIVE`, `is_native`, `claim`, `claim_large` and `is_claimed`
  are gone; `OscRoutes::BUILTIN`, `has_builtin`, `route` and `get` are their replacements, and
  `claim_large(n)` is `route(n, OscRoute::Forward).large(n, true)` for a number the engine does not
  implement, or `large(n, true)` alone for one it does. There is no deprecation shim: the crate has
  not been published, so this was the last moment the rename was free.
- **Breaking.** `VtEvent` is `#[non_exhaustive]`, so a `match` on it needs a `_` arm. A new built-in
  event is a patch-level change from here on rather than a break.
- `OSC 7`, `OSC 9`, `OSC 9;4` and `OSC 133` are parsed by the engine instead of being forwarded raw,
  so an embedder that parsed them itself should delete that code and match the typed events. The
  wire behaviour is unchanged, down to the percent decoding, the Windows drive-slash rule, and
  dropping an `OSC 7` URL that is not valid UTF-8 — with one deliberate correction: `OSC 9;4` reads
  its state and percentage as `u32` and **clamps** the percentage, where reading them as `u8` turned
  `9;4;1;1000` into `Set(0)` and a state above 255 into `Remove`.
- `OSC 22` and `OSC 1` are no longer counted in `FeedStats::unhandled_sequences`; they are handled.
- **Breaking.** `Mode`, `ColorKey`, `MouseReporting` and `MouseEncoding` each gain variants and all
  four are exhaustive: `Mode::{ReverseVideo, MouseX10, UrxvtMouse}`,
  `ColorKey::{SelectionBackground, SelectionForeground}`, `MouseReporting::X10` and
  `MouseEncoding::Urxvt`. `ModeSnapshot` gains the `reverse_video` field, so a struct literal over
  it needs the new field.
- **Reply contract.** `DECRQM` on `? 2027` answered `NotSupported` and now answers the mode's real
  state, because the mode has a reader. `? 5`, `? 9` and `? 1015` answered `NotSupported` as
  unrecognised numbers and now answer their real state too.
- **Behaviour.** `DECSC` / `DECRC` (and `CSI s` / `CSI u`, and `? 1048`) now save and restore the
  set invoked into GL and any pending single shift, as well as the `G0`-`G3` designations. VT510's
  `DECSC` saves "character sets currently in GL and GR" and "any single shift 2 or 3 sent", and
  xterm's `CursorSave` stores `curgl`, `curgr` and `gsets[]`; the engine used to save neither.
- **Behaviour, and a correction to this file.** An earlier line here said the mouse encoders
  "return an empty `Vec` for an event the mode does not report". That was true of `? 9`'s
  suppressed event kinds and **false** with no protocol on at all, where a press still encoded the
  legacy report. It is true now: `ModeSnapshot::mouse == None` encodes nothing, for every event.
  An embedder that called an encoder without checking its own modes used to send mouse reports to
  a program that never asked for them.
- With no `product_name` set, `XTVERSION` now answers `oneterm-vt(<version>)` instead of naming
  the application this engine was extracted from.
- Every public item is documented; `#![warn(missing_docs)]` keeps it that way.
