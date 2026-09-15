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
6. Behaviour is not the API, with one exception: **the reply bytes for `DA1`, `DA2`, `DSR`,
   `DECRQM`, `XTVERSION` and the OSC colour queries are a contract**, because programs parse them.
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
  and without any of its code running inside `feed`. `OscRoutes::BUILTIN` publishes the eighteen
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

### Changed

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
- With no `product_name` set, `XTVERSION` now answers `oneterm-vt(<version>)` instead of naming
  the application this engine was extracted from.
- Every public item is documented; `#![warn(missing_docs)]` keeps it that way.
