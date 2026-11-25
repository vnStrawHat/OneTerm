# High-Level Design: Embeddable VT core

Intake: IN-0038
Lane: high_risk
Date: 2026-09-15

## Idea

`oneterm-vt` stops being "OneTerm's engine that happens to be a crate" and becomes a terminal core
any project can depend on. Three things change: everything that is terminal semantics rather than
OneTerm policy moves **in** (OSC parsing, key and mouse encoding, scrollback search); the OSC layer
gains an extension and override mechanism neither `alacritty_terminal` nor `rio-vt` has; and the
crate gets the packaging a distributed library needs -- a README, a compiled headless example, a
CHANGELOG, the licence text, `#![warn(missing_docs)]`, and rustdoc that does not cite documents only
this repository holds. **Owner ruling 2026-09-15: it is not published to crates.io**; other projects
consume it as a git dependency on this repository, and the packaging is what makes that usable.

Nothing that is a policy, a backend or a UI moves in. The line is: **the core reports, the embedder
decides.** OSC 52 is the model that already works -- the engine decodes the base64 and emits
`ClipboardStore`; `crates/terminal` decides whether a remote host may write your clipboard. Every
OSC moving in this intake follows it.

## Diagram

```text
                        +-----------------------------------------------+
   PTY / SSH bytes ---> |                 oneterm-vt                    |
                        |                                               |
                        |  parser/    state machine, OSC_INLINE=2048,   |
                        |             OSC_LARGE=8MiB, DCS_MAX=16MiB     |
                        |  terminal/  dispatch: CSI, ESC, DCS, OSC      |
                        |               osc/ -> OscRoutes + built-ins   |
                        |  grid/      rows, scrollback, RowId           |
                        |  reflow/    resize policies                   |
                        |  selection/ ranges, semantic expand           |
                        |  snapshot/  pull-side read model + damage     |  (was: render/)
                        |  graphics/  Sixel decode + cell anchoring     |
                        |  input/     key_encode + mouse_encode         |  (moved in, US-0099)
                        |  search/    scrollback search, [regex] feat   |  (moved in, US-0100)
                        |  event/     VtEvent values in an EventBatch   |
                        |  pty/       ConPTY / openpty, [pty] DEFAULT   |  (moved in, US-0104)
                        +-----------------------------------------------+
                            |  feed(bytes, &mut EventBatch, now)
                            |  -> FeedStats, batch of VtEvent VALUES
                            v
                        +-----------------------------------------------+
                        |          the embedder (crates/terminal)       |
                        |  TerminalHandle (the lock)                    |
                        |  OscRouter: policy + fan-out                  |
                        |  TerminalSecurityPolicy  url_policy  paste    |
                        |  osc_agent/ (OSC 20308)  <- worked example    |
                        |  logging  palette  content  session           |
                        +-----------------------------------------------+
                            |                         ^
                            v                         |
                        oneterm_vt::pty / russh   SessionEvent -> GPUI
```

What stays outside, and why (each row is R6/R7's boundary, not taste):

| Outside the core | Why |
| --- | --- |
| ~~PTY / ConPTY~~ -- **moved in** at `US-0104` | Superseded by the owner ruling of 2026-09-15. This row read that a core which spawns processes cannot be used by a project that already owns its own process model, and that `rio-vt`'s default-on PTY feature was the mistake this crate would not make. The ruling takes `rio-vt`'s shape on purpose, and the objection is answered by the feature rather than by exclusion: `--no-default-features` compiles no platform code, spawns no thread and resolves to six leaf dependencies, so an embedder with its own process model pays nothing. See [`low-level-design/pty.md`](low-level-design/pty.md). |
| SSH transport (`crates/ssh`) | a protocol client with a hidden tokio runtime, a key store and an auth policy is not terminal semantics. R7. |
| The lock | `IN-0029` decision 6 and 7: the engine holds no lock and no interior mutability, so the embedder picks `parking_lot`, `std`, a channel or nothing. `crates/vt/src/lib.rs` says so and `US-0090` made it true. |
| Clipboard backend | reading and writing the OS clipboard is a platform capability with a policy attached. The engine decodes and reports. |
| `TerminalSecurityPolicy` | size caps, the notification rate limit and the two remote-clipboard gates are product choices, and OneTerm's are not another embedder's. Decision (f). |
| `url_policy`, `paste`, `logging`, `completion` | adapter behaviour built on top of grid text, not terminal semantics. |
| GPUI and any renderer | R7, and the mission statement's "contains no rendering". |
| OSC 20308 agent channel | it is a proposal OneTerm authored, not a terminal standard. Keeping it outside is what proves the extension mechanism works. |

## UI Wireframe

`N/A -- no UI surface.` This intake changes a Rust library boundary. No OneTerm screen, dialog,
menu, keybinding or setting changes. The deprecated `OSC 9;7` alias stays (intake Open Decision 1,
ruled by the owner: custom spellings are handled outside the engine), so no wire spelling changes.

## Data Flow

1. The embedder builds a `Config` -- scrollback limit, default cursor style, semantic escape
   characters, `product_name`, and an `OscRoutes` table -- and calls `Terminal::new(size, config)`.
2. The embedder reads bytes from its own transport and calls
   `feed(&bytes, &mut batch, Instant::now())` while holding its own lock.
3. The parser runs the state machine. On an OSC it resolves the payload ceiling from
   `OscRoutes::allows_large(code)` before buffering (`OSC_INLINE` or `OSC_LARGE`) -- unchanged from
   today, and the reason a hostile 8 MiB payload cannot be bought by an unclaimed number.
4. Dispatch looks the number up **once** in `OscRoutes`:
   - `Builtin` -> the engine's own arm runs and pushes its typed event.
   - `BuiltinAndForward` -> the arm runs **and** the raw parameters are also pushed as
     `VtEvent::Osc`.
   - `Forward` -> the arm is skipped; only `VtEvent::Osc` is pushed.
   - `Drop` -> nothing is pushed; `FeedStats::unhandled_sequences` moves.
   No embedder code runs here. No allocation beyond the batch arena. No indirect call.
5. `feed` returns `FeedStats`; the batch holds the events as values, with payloads as spans into the
   batch's own arena.
6. The embedder drops its lock (or not -- its choice) and drains the batch, applying its policy:
   sanitise the title, rate-limit the notification, gate the clipboard, turn `Cwd { host, path }`
   into a `PathBuf` it trusts, answer a `ColorQuery` from its theme.
7. For drawing, the embedder pulls: `snapshot_update(&mut SnapshotState)` returns only what changed
   since the last pull, keyed by `RowId` (`DEC-0015`). The engine never pushes a frame and never
   knows a frame exists.

## Target module map

| Module | Visibility | Contents | Change |
| --- | --- | --- | --- |
| `parser` | `pub` | state machine, `Dispatch`, `OscParams`, `OSC_INLINE`, `OSC_LARGE`, `MAX_OSC_PARAMS`, `DCS_MAX_BYTES`, `StringTerm` | unchanged |
| `grid` | `pub` | `Pos`, `RowId`, `RowRef`, `SeqNo`, `Size`, `Viewport`, `Screen`, scrollback constants | unchanged |
| `intern` | `pub` | `Hyperlink`, `Extras`, `GraphicId`, `Interner` | unchanged |
| `terminal` | private, re-exported | `Terminal`, `Config`, `Mode`, `CursorShape`, `KeyboardFlags`, `ColorKey`, and the OSC layer | `OscClaims` -> `OscRoutes`; six new built-in arms (`US-0098`) |
| `event` | private, re-exported | `VtEvent`, `EventBatch`, `FeedStats`, `ClipboardKind`, `StringTerm` | seven new typed variants (`US-0098`) |
| `snapshot` | private, re-exported | `SnapshotState`, `SnapshotUpdate`, `SnapshotRow`, `SnapshotCell`, `SnapshotContent`, `SnapshotCursor`, `SnapshotPlacement`, `StyleRun`, `ModeSnapshot`, `Palette` | renamed from `render` (`US-0101`) |
| `selection` | private, re-exported | `Selection`, `SelectionKind`, `SelectionRange`, `Side` | unchanged |
| `graphics` | private, re-exported | `GraphicData`, `VIRTUAL_CELL`, Sixel | unchanged |
| `cell`, `reflow`, `width` | private, re-exported | `Cell`, `Style`, `Attrs`, `Color`, `ResizePolicy`, `cluster_width` | `cluster_width` gains its caller (`US-0102`) |
| **`input`** | **`pub`** | `KeySpec`, `NamedKey`, `KeyMods`, `encode_key`; `TerminalMouseButton`, `MouseModifiers`, `encode_mouse_*`, `encode_wheel_event` | **new**, moved from `crates/terminal` (`US-0099`) |
| **`search`** | **`pub`** | `SearchOptions`, `SearchMatch`, `GridText`, `search_grid_text`; `SearchPattern::Regex` behind the `regex` feature | **new**, moved from `crates/terminal` (`US-0100`) |
| **`pty`** | **`pub`**, `#[cfg(feature = "pty")]` | `PseudoConsole`, `Options`, `Shell`, `WindowSize`, `GlyphWidth`, `ChildEvent`, `EventedReadWrite`, `EventedPty`, `OnResize`, `SignalMask` (Unix), `PipeReader` / `PipeWriter` (Windows), `PTY_CHILD_EVENT_TOKEN`, `PTY_READ_WRITE_TOKEN` | **new**, moved from `crates/pty` (`US-0104`); the whole module disappears under `--no-default-features` |

`input`, `search` and `pty` are `pub mod` rather than root re-exports because they are
self-contained namespaces an embedder may want to `use` wholesale, and because `encode_key` or
`PseudoConsole` at the crate root would read as if the crate were an input library or a process
launcher. That is the same reason `grid`, `intern` and `parser` are `pub mod` today.

`pty` is the only module gated by a feature, and the only one that contains `unsafe`, platform FFI,
a spawned thread or a blocking `Drop`. Everything above it in the table is true at every feature
setting.

## Dependency budget

The crate's whole `cargo tree -p oneterm-vt -e normal` is seven lines today. The rule after this
intake, **replacing** the earlier "default features add nothing" (which the owner ruling of
2026-09-15 made false): **`--no-default-features` adds nothing, and it is a CI target.** One default
feature, `pty`, adds dependencies; every other feature is default-off, and no feature may add a
platform dependency beyond `pty`'s three.

| Build | Direct | Distinct crates | Notes |
| --- | --- | --- | --- |
| `--no-default-features`, any target | 6 | 6 | identical to the crate on `main` today |
| default, `x86_64-pc-windows-msvc` | 8 | 16 | `+polling`, `+windows-sys 0.59` and their transitives |
| default, `x86_64-unknown-linux-gnu` | 8 | 11 | `+polling`, `+libc` and their transitives |

| Feature | Default | Adds | Why it is a feature and not a dependency |
| --- | --- | --- | --- |
| (none) | -- | `bitflags`, `log`, `memchr`, `rustc-hash`, `unicode-segmentation`, `unicode-width` | the six the engine cannot do without |
| `pty` | **on** | `polling` (public), plus `windows-sys` on Windows or `libc` on Unix | owner ruling 2026-09-15: a terminal core that cannot open a terminal is a surprise, so it ships on. It is a feature and not a dependency because an embedder that already owns its process model must be able to compile the engine with no platform code, no thread and no blocking `Drop`. `rio-vt`'s shape; `US-0104`; [`low-level-design/pty.md`](low-level-design/pty.md). |
| `regex` | **off** | `regex 1` | literal and whole-word search need no regex; an embedder who wants `SearchPattern::Regex` pays 3 crates (`regex`, `regex-automata`, `regex-syntax`) and about 1.5 MB of compiled matcher. OneTerm's own search is literal, so OneTerm does not turn it on. |
| `serde` | **off** | `serde 1` (derive) | proposed, pending intake Open Decision 3. `Serialize`/`Deserialize` on the plain-data types only, never on `Terminal`, `EventBatch` or any span type. |
| `vt-paranoid` | off | nothing | existing: the whole-history integrity walk for property tests and CI |

Rules the packets enforce:

- A feature never changes behaviour, only availability. `search_grid_text` on a literal pattern
  returns the same matches with and without `regex`, and `feed` parses the same bytes with and
  without `pty`.
- `cargo build -p oneterm-vt --no-default-features` and `--all-features` must both be clean and are
  both CI targets (`US-0097`), and the `--no-default-features` step carries a `cargo tree` assertion
  that the six-dependency claim is still literally true (`US-0104`).
- No feature except `pty` may add a platform dependency, and `pty` may add only `polling`,
  `windows-sys` and `libc`. `vt` gains no OneTerm dependency in any feature combination.
- No optional dependency may appear in the signature of a non-feature-gated item. `SearchPattern`
  is `#[non_exhaustive]` so its `Regex` variant can be feature-gated without breaking the match
  arms an embedder wrote.

## Public API surface after the move

Full before-and-after list, with the semver promise, is in
[`low-level-design/api-surface.md`](low-level-design/api-surface.md). The shape:

```rust
// The one object.
pub struct Terminal;
impl Terminal {
    pub fn new(size: Size, config: Config) -> Terminal;
    pub fn feed(&mut self, bytes: &[u8], batch: &mut EventBatch, now: Instant) -> FeedStats;
    pub fn resize(&mut self, size: Size, policy: ResizePolicy);
    pub fn snapshot_update(&mut self, state: &mut SnapshotState) -> SnapshotUpdate<'_>;
    // selection, scrollback, colours, modes, config access ...
}

// Configuration, including the OSC routing table and the product identity.
pub struct Config {
    pub scrollback_limit: u32,
    pub osc_routes: OscRoutes,
    pub default_cursor_style: CursorStyle,
    pub semantic_escape_chars: String,
    pub product_name: Option<Cow<'static, str>>,
}

// Extension point.
pub struct OscRoutes;
pub enum OscRoute { Builtin, BuiltinAndForward, Forward, Drop }

// Output, as values.
pub struct EventBatch;
pub enum VtEvent { /* #[non_exhaustive] */ }
pub struct FeedStats;

// Pull-side read model (renamed).
pub struct SnapshotState;
pub struct SnapshotUpdate<'_>;

// Namespaced helpers.
pub mod input;   // encode_key, encode_mouse_*
pub mod search;  // SearchOptions, SearchMatch, search_grid_text
pub mod grid;    // Pos, RowId, Size, Viewport, Screen
pub mod parser;  // the state machine and its limits
pub mod intern;  // Hyperlink and friends

// The transport. Default-on, and the only feature-gated module.
#[cfg(feature = "pty")]
pub mod pty;     // PseudoConsole, Options, EventedReadWrite / EventedPty / OnResize
```

Three rules make this survivable as an external contract:

1. Every public enum an embedder matches on is `#[non_exhaustive]`, so adding an OSC event or a
   route mode is a minor bump, not a major one.
2. Every public struct an embedder constructs is either `#[non_exhaustive]` with a builder-style
   `impl` (`Config`, `OscRoutes`) or a plain frozen value type (`Pos`, `Size`, `Rgb`, `RowId`).
3. Nothing in a public signature names an optional dependency's type.

## Semver, MSRV and publish policy

- **Version.** The crate keeps inheriting `[workspace.package] version`, so it releases on
  OneTerm's cadence and its version number tracks the application. That is what `rio-vt` does. The
  cost is that a patch release of the app republishes the engine with no engine change; the benefit
  is one version source and no drift. `packaging.md` records the alternative (an independent
  `version = ` in `crates/vt/Cargo.toml`) and why it is not taken.
- **Pre-1.0.** The crate is `0.x`, so cargo treats a **minor** bump as breaking. That is the honest
  signal while the OSC mechanism is settling, and it is the same place `rio-vt` is.
- **Breaking change policy.** Any removal or signature change of a `pub` item, any new required
  `Config` field, or a new `VtEvent` variant on an enum that is not `#[non_exhaustive]` is a minor
  bump with a CHANGELOG entry naming the item.
- **MSRV.** `rust-version` stays inherited at 1.96.0. An MSRV raise is a minor bump plus a
  CHANGELOG line; it is never a patch. CI does not currently build at the MSRV -- that gap is
  recorded in `packaging.md` rather than papered over.
- **Publish.** **Owner ruling 2026-09-15: nothing goes to crates.io.** `crates/vt/Cargo.toml`
  carries an explicit `publish = false` so the decision is visible where a reader looks for it, and
  `scripts/verify-dependency-graph.py` asserts that no crate in the workspace is publishable. What
  CI proves instead is that the crate *packages*: `cargo package -p oneterm-vt` plus a check that
  the file list carries `README.md`, `CHANGELOG.md`, `LICENSE`, `NOTICE` and the example and reaches
  nothing outside `crates/vt`. A consumer pins a tag, and the promise above applies to tags.

## Documentation, README and example plan

- **README** at `crates/vt/README.md`, referenced by `readme = "README.md"`: what the crate is, what
  it deliberately is not (no rendering, no policy), the six-dependency tree -- pasted from
  `cargo tree -e normal --no-default-features` once `US-0104` makes the two builds differ -- the
  twenty-line quick start, the OSC routing table with one override example, the feature table
  (`pty` is the one default-on row), and the MSRV. Every code block in it is either the example file's content or is compiled by a doctest;
  nothing is retyped prose. `rio-vt`'s README drifted from its manifest within seven weeks -- that
  is the failure this rule exists to prevent.
- **Example** at `crates/vt/examples/headless.rs`: create a terminal, feed bytes including an OSC
  the example claims for itself, drain the batch and print the events, pull a snapshot and print the
  visible rows as text. No dependency beyond the crate. `cargo build --examples -p oneterm-vt` is a
  CI target, so the example cannot rot.
- **CHANGELOG** at `crates/vt/CHANGELOG.md`, Keep-a-Changelog shape, starting with an `Unreleased`
  section that this intake's packets fill in. It documents the **crate's** API, not OneTerm's
  features.
- **Rendered documentation** is `cargo doc -p oneterm-vt --no-deps --open`, run by the consumer.
  There is no docs.rs page and no `[package.metadata.docs.rs]` table, because the crate is not
  published (owner ruling 2026-09-15). GitHub Pages is the obvious public host if one is ever
  wanted; nobody has asked.
- **`#![warn(missing_docs)]`** at the crate root. It is a `warn` and the workspace lint table turns
  warnings into errors in CI, so it is effectively a deny without making a local
  work-in-progress build fail.

## The doc-comment self-containment rule

`crates/vt/src` carries **105 rustdoc lines** citing `US-NNNN`, `DEC-NNNN`, `IN-NNNN` or
`docs/spec-intakes/...` paths, across 37 non-test files. In the rendered API documentation every one
of those is a dangling reference to a document the reader cannot open. They are also the crate's best commentary, so they
are not deleted. The rule `US-0097` applies, mechanically:

1. A citation that explains **why the code is the way it is** moves from `///` or `//!` to a plain
   `//` comment on the line above. It stays in the source for the next maintainer and leaves the
   rendered docs. This is the majority case.
2. A citation a reader genuinely needs moves into the doc comment as an absolute link to the public
   repository, for example
   `<https://github.com/vnStrawHat/OneTerm/blob/main/docs/spec-intakes/IN-0029-vt-engine/low-level-design/parser.md>`.
   Reserve this for the handful of module-level "Design:" lines where the design document really is
   the specification.
3. What is left in `///` must be true for an embedder who has never seen OneTerm. "R-22", "trap 38",
   "correction C7" and "deviation D13" are internal vocabulary: keep the sentence, drop the label,
   or move the whole line to `//`.
4. `python scripts/check-doc-paths.py` does not look inside `crates/`, so rule 1 is enforced by a
   grep in `US-0097`'s acceptance criteria, not by an existing script.

## `Config::product_name`

Two replies leak the crate's own identity to the host program:

- `XTVERSION` (`CSI > 0 q`) answers `DCS > | OneTerm(<CARGO_PKG_VERSION>) ST` --
  `crates/vt/src/terminal/dispatch.rs:1130`.
- `DA2` (`CSI > c`) answers `CSI > 0 ; <version digits> ; 1 c` -- `dispatch.rs:467`.

For an embedder that is wrong twice: the name is not theirs, and the version is the engine's, not
the product's. `Config` gains:

```rust
/// What the terminal calls itself in `XTVERSION` and `DA2`.
///
/// `None` answers with the engine's own name and version. An embedder that
/// ships a product should set this: programs such as `tmux` and `vim` key
/// capability detection off the `XTVERSION` string.
pub product_name: Option<Cow<'static, str>>,
```

`None` -> `oneterm-vt(<engine version>)`. OneTerm's `adapter_config` sets
`Some("OneTerm(<app version>)".into())`, which preserves today's reply byte-for-byte because the
workspace version is the app version. `DA2`'s numeric field is derived from the same string's
trailing version digits when they parse, and falls back to the engine's number otherwise; that
keeps `DA2` a number as the protocol requires while letting the product own it.

## Detail Design

Required for the high-risk lane. Five concerns, one file each:

- [x] Detail design: **required (high-risk)**
- [`low-level-design/osc-extension.md`](low-level-design/osc-extension.md) -- `OscRoutes`, the
  built-in arms, the new typed events, the migration of every adapter parser, and why this differs
  from alacritty's `Handler` and rio's `EventListener`. The key design of the intake.
- [`low-level-design/api-surface.md`](low-level-design/api-surface.md) -- the exact `pub` list
  before and after, the `render` -> `snapshot` rename, `#[non_exhaustive]`, sealing, and the semver
  promise.
- [`low-level-design/encoding-and-search.md`](low-level-design/encoding-and-search.md) -- moving
  `key_encode`, `mouse_encode` and `search`, the `regex` feature, which tests travel, and what
  `crates/terminal` keeps.
- [`low-level-design/packaging.md`](low-level-design/packaging.md) -- the manifest, README,
  example, CHANGELOG, licence text, `missing_docs`, MSRV, and the effect on
  `third-party-notices.py`, `cargo-deny`, `check-english.py` and `check-doc-paths.py`.
- [`low-level-design/pty.md`](low-level-design/pty.md) -- the owner ruling of 2026-09-15: the
  pseudo-console transport moves into the crate behind a default-on `pty` feature. The file move,
  the feature, what "overridable" means and the decision that the transport traits are gated with
  the feature, the dependency arithmetic in both modes, the R1-R12 replacement text, the bundled
  Windows console host and why it stays out of the published package, and the threading model.

Reason: the intake changes a public contract and makes it external, which is a high-risk trigger on
its own; and the OSC mechanism is the one part where a wrong shape would be expensive to undo once
another project depends on a tag.
