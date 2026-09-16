> Re-run on 2026-09-16 after four of the five gaps below were closed: [`vt-library-evaluation-2026-09-16.md`](vt-library-evaluation-2026-09-16.md).

# Choosing a terminal core: oneterm-vt vs alacritty_terminal vs rio-vt

An outside evaluation, written from the position of an independent developer starting a new
terminal application in Rust: own GPU renderer, tabs and splits, SSH later. Which VT core?

Evaluated 2026-09-15 on `x86_64-pc-windows-msvc`, rustc 1.96.0, `CARGO_BUILD_JOBS=4`.
`oneterm-vt` was judged only from what an outsider can reach: the crate README and CHANGELOG,
the rustdoc and embedder's guide rendered by `cargo doc -p oneterm-vt --no-deps --all-features`,
the public API, the CI workflow, and a consumer crate built outside the repository against a git
dependency. The design documents in `docs/spec-intakes/` were not read. Network access from
cargo worked, so all three candidates were built and run; nothing below is from documentation
alone.

---

## 1. Fact sheets

### 1.1 oneterm-vt

| | |
| --- | --- |
| Version examined | 0.5.2, commit `a2c28cc5`, branch `main` |
| Licence | Apache-2.0 (`LICENSE` + `NOTICE` beside the crate) |
| Edition / MSRV | 2024 / 1.96.0 (stated as intent, checked by hand, not by a job) |
| Published | **No.** `publish = false`, git dependency only, no docs.rs, and no tag yet carries the crate (`v0.5.3` is the first that can) |
| Features | `pty` (default on), `vt-paranoid`, `regex` |
| Dependencies | 6 leaf crates with `--no-default-features`; 16 with the default `pty`. `polling` is the one public dependency, under `pty` only |
| Source size | 29,905 lines of Rust; 54 `unsafe` occurrences, **all** in `src/pty`, zero in the engine |
| Tests | 695 passing (659 unit + integration, 36 doctests), 4 ignored, 47.6 s. One Linux-only `cargo-fuzz` target |
| Docs | `#![warn(missing_docs)]`, plus a 13-chapter embedder's guide rendered as `oneterm_vt::guide`, every Rust block a doctest |

The guide is the crate's distinguishing artefact. Thirteen chapters: what it is, embedding,
threading, events, OSC routing, input, search, images, resize, hostile-input ceilings,
conformance, versioning, transport. Chapter 11 publishes a table of known gaps by name.
Chapter 1 publishes a comparison against the other two candidates, which is unusual and, as far
as I could check it, accurate.

### 1.2 alacritty_terminal

| | |
| --- | --- |
| Version examined | 0.26.0 from crates.io (published 2026-04-06); `master` manifest reads 0.26.1-dev |
| Licence / edition / MSRV | Apache-2.0 / 2024 / 1.85.0 |
| Published | Yes. 802,305 downloads on 0.26.0 alone; release line back to at least 0.24.x. docs.rs coverage 63.04% |
| Features | `default = ["serde"]`. That is the entire feature set |
| Dependencies | 46 crates with defaults, 35 with `--no-default-features` |
| Parser | Delegated to `vte` 0.15, a separate crate with its own release cycle |
| PTY | `tty` module, unconditional. `rustix-openpty`, `signal-hook`, `miow`, `piper`, `polling` are not behind any feature |
| Evidence | 48 recorded reference cases under `alacritty_terminal/tests/ref`; no benches in the crate (`vtebench` is a separate project) |

### 1.3 rio-vt

| | |
| --- | --- |
| Version examined | 0.5.26 from crates.io (published 2026-08-23); workspace version 0.5.27 |
| Licence / edition / MSRV | MIT / 2021 / 1.96.1 |
| Published | Yes, but **first published 2026-07-26** -- seven weeks before this evaluation. 50,847 downloads. docs.rs coverage 57.07% |
| Release churn | 8 versions inside one month (0.5.15 through 0.5.26, 2026-08-06 to 2026-08-23) |
| Features | `pty` (default), `renderer`, `rio-window`, `clipboard`, `graphics`, `x11`, `wayland`, `wgpu` |
| Dependencies | 66 crates with defaults, 49 with `--no-default-features` |
| Parser | Its own. Re-exports `corcovado` and `teletypewriter` into its public surface |
| Benchmarks | `criterion` as a dev-dependency, so benches exist in tree |

`rio-vt` is an extraction of the Rio application's internals, and the seams still show. Its
constructor is:

```
Crosswords::new(dimensions, cursor_shape, event_proxy, window_id, route_id, scrollback_limit)
```

A terminal core does not need a window id or a route id. Those are application concepts that
arrived with the code.

---

## 2. The build test

Three consumer crates, all outside any of the three repositories, all doing the same minimal
embed: feed one chunk of bytes containing SGR, a title OSC and an absolute cursor move, then
read the four-row grid back as text.

A fourth crate, `vtprobe`, follows the oneterm-vt guide further: chapter 2 (embed), chapter 5
(route a private OSC and observe it), chapter 6 (encode a key under kitty keyboard flags) and
chapter 13 (spawn `cmd /c echo hi` through the bundled pseudo-console and read it).

| | oneterm-vt (`vtmin`) | alacritty_terminal (`alacprobe`) | rio-vt (`rioprobe`) |
| --- | --- | --- | --- |
| Manifest line | git, `branch = "main"`, `default-features = false` | `cargo add alacritty_terminal` | `cargo add rio-vt` |
| Compile iterations to a running grid | **1** | 2 | 2 |
| Crates in `cargo tree -e normal` | **7** (6 + the crate) | 47 (46 + the crate) | 67 (66 + the crate) |
| Same, `--no-default-features` | **7** | 36 | 50 |
| Release build, cold | **3.1 s** | 14.1 s | 20.5 s |
| Release `.exe` | **359,424 B** | 370,688 B | 1,172,480 B |
| Minutes to first rendered grid | **~10** | ~15 | ~15 |

The oneterm-vt README's dependency claims are exact. `--no-default-features` gave precisely the
six leaf crates it names (`bitflags`, `log`, `memchr`, `rustc-hash`, `unicode-segmentation`,
`unicode-width`), and the default build gave 16. CI enforces the six (see appendix).

`vtprobe`, the four-chapter version with the pseudo-console, also compiled and ran on the first
attempt, with no correction to any guide example. It spawned `cmd /c echo hi`, polled it,
fed the bytes to the engine and read `hi` out of the grid. That is the strongest single result
in this evaluation: the guide is executable, not aspirational.

### Friction found

**oneterm-vt.** One real defect. Four public methods return types that no outside crate can
name:

```
Terminal::resize(..)     -> ResizeOutcome    error[E0425]: cannot find type `ResizeOutcome`
Terminal::cursor_style() -> CursorStyle      error[E0425]: cannot find type `CursorStyle`
Terminal::sync()         -> &SyncState       (module `terminal` is private)
Terminal::placements()   -> &[Placement]     (module `graphics` is private)
```

They are usable by field access -- the guide's own chapter 9 doctest reads `outcome.reflowed`
-- but they cannot be stored in a struct, returned from a function, or matched on by path. For
an embedder writing images, `placements()` is the direct path and it is unnameable.

**alacritty_terminal.** `Processor::new()` fails to infer its timeout type parameter; the fix
is `Processor::<vte::ansi::StdSyncHandler>::new()`, which is not in any example on docs.rs.
There is no exported concrete size type, so every consumer writes its own `Dimensions` impl.

**rio-vt.** The README's own quick-start does not compile: `WindowId::from(0)` is ambiguous
(needs `0u64`), and there is no `Default` for `WindowId`, so the two obvious repairs both fail
before the third works. Blank cells did not trim as whitespace in my probe, so
`visible_rows()` output needed handling the other two did not.

---

## 3. Rubric

Scores are 1-5, higher is better, for this application.

| # | Criterion | oneterm-vt | alacritty_terminal | rio-vt |
| --- | --- | --- | --- | --- |
| a | VT/xterm coverage | 3 | 4 | 4 |
| b | Grid, scrollback, reflow, damage | 5 | 4 | 3 |
| c | API and threading model | 4 | 3 | 2 |
| d | Extensibility | 5 | 2 | 2 |
| e | Dependency weight, platform coupling | 5 | 3 | 2 |
| f | Documentation and onboarding | 5 | 2 | 2 |
| g | Tests and conformance evidence | 4 | 4 | 3 |
| h | Maintenance, licence, release story | 2 | 5 | 3 |
| i | Performance claims and evidence | 2 | 3 | 3 |
| j | Fit for this application | 5 | 3 | 2 |
| | **Total (50)** | **40** | **33** | **26** |

**(a) Coverage.** alacritty delegates to `vte`, the de-facto reference parser, and implements
kitty keyboard end to end; rio covers Sixel and the iTerm2 image form. oneterm-vt has broad
CSI, twenty built-in OSC numbers, full mouse, both width rules with mode 2027, and Sixel -- but
no `DECRQCRA`, no `DECRQSS`, no `XTGETTCAP`, no kitty graphics protocol, and kitty keyboard is
tracked and reported rather than encoded. Evidence: guide chapter 11, which states all of this
itself, plus my own probe (section 4, gap 2).

**(b) Grid and damage.** oneterm-vt is the only one of the three where damage is per consumer:
each `SnapshotState` carries its own read position over sequence-numbered rows, so a renderer
and, say, a tab thumbnail do not clear each other's damage. `snapshot_update` returns
`Full` / `Partial { scrolled }` / `Unchanged`, and `RowId` names the same content across a
reflow -- exactly the key a GPU glyph cache wants. Both others expose
`damage(&mut self) -> TermDamage` plus `reset_damage()`: one damage cursor per terminal, taken
under a write lock. rio additionally allocates a `Vec<Row<Square>>` on every `visible_rows()`
call. Evidence: docs.rs signatures for both, guide chapters 3 and 9.

**(c) API and threading.** oneterm-vt has no `Mutex`, no `RefCell`, no atomic, no global, and
never calls into embedder code: `feed` fills an `EventBatch` of values, drained after the call.
Holding your own lock across `feed` cannot deadlock against your own code. `Send` and `Sync`
are automatic, and `&mut self` on the three mutating methods is what serialises. Both others
use an `EventListener` callback invoked from inside the terminal, and both hold `parking_lot`
internally. rio's ~60 event variants and its `WindowId` / `route_id` constructor arguments cost
it another point. oneterm-vt loses one point for the unnameable return types.

**(d) Extensibility.** `OscRoutes` is the single largest design difference between these three
crates. Per OSC number you choose `Builtin`, `BuiltinAndForward`, `Forward` or `Drop`, and a
payload ceiling. Supporting a new OSC number is one `route()` call plus a `match` arm in your
own code. In both others the OSC number is a compile-time `match` -- in alacritty's case inside
a different crate -- and anything unknown is dropped. Adding one means forking. Verified: my
`vtprobe` routed OSC 31337 and received the raw parameters, while keeping OSC 9's built-in
handler and its typed event as well. Bring-your-own-PTY is comparable: oneterm-vt publishes
three traits and can drop the transport entirely; alacritty's `tty` cannot be removed at any
feature setting.

**(e) Dependency weight.** 7 crates against 47 and 67, measured, not claimed. No build script,
no proc macro, and no `unsafe` in the engine. `--no-default-features` removes every line of
platform code and is a CI target with an asserted crate count.

**(f) Documentation.** A 13-chapter guide whose every example compiles, against 63% and 57%
rustdoc coverage with no narrative documentation at all. This is not close.

**(g) Tests.** alacritty's 48 recorded reference cases plus seven years of field use is real
evidence of a kind oneterm-vt cannot match. oneterm-vt's 695 tests are runnable by an outsider
in 48 seconds, there is a fuzz target and a whole-history integrity mode, and chapter 11 lists
the gaps by name rather than claiming a score. Equal, for different reasons.

**(h) Maintenance.** This is where oneterm-vt loses. It is not on crates.io, no tag carries it
yet, `cargo add` does not work, there is no docs.rs page, its version number belongs to a
different product, and the visible bus factor is one. alacritty has multiple maintainers, a
seven-year release line and 800k downloads on the current version. rio sits between: published,
but by one person, seven weeks old as a separate crate, and churning eight releases a month.

**(i) Performance.** oneterm-vt makes no performance claim anywhere in its README, changelog or
guide, which is honest, but it also ships nothing an evaluator can run: no `benches/`, no
criterion, one internal `snapshot_bench.rs` module. rio ships criterion benches. alacritty has
`vtebench` next door. None of the three offers a number you can compare directly.

**(j) Fit.** Splits mean many terminals and, per terminal, potentially more than one consumer
of the grid; per-consumer damage is worth real frames. SSH later means the transport must be
replaceable without carrying a PTY you do not use; only oneterm-vt lets you compile it out. A
GPU renderer wants style runs and stable row identity, which the snapshot already provides.

---

## 4. Recommendation

Ranked, for this application:

1. **oneterm-vt**, conditionally. It wins the rubric 40-33-26 and it wins the two criteria that
   are hardest to retrofit: an event model with no callbacks into my code, and per-consumer
   damage. The condition is that I accept a git pin and am willing to fork if the upstream
   stops moving -- which is realistic at 30k lines, six dependencies, no `unsafe` in the engine
   and 695 tests I can run myself.
2. **alacritty_terminal**, and it is the correct choice for anyone who cannot accept that
   condition. I would pay the 33-to-40 rubric gap for crates.io, a docs.rs page, several
   maintainers and 48 reference tests, and I would work around the callback model and the
   unremovable `tty` module.
3. **rio-vt**. Seven weeks old as a standalone crate, eight releases in its first month, an
   application's window id in the core constructor, 66 dependencies, and a README example that
   does not compile. Revisit in a year.

Deciding factors, in order: the event model (values, not callbacks, so the lock story is mine);
per-consumer damage for splits; a removable transport for SSH; the dependency budget; and
against all of that, the fact that oneterm-vt is not a package I can install.

I want to be plain about the inversion here. On engineering, oneterm-vt is the better core and
it is not particularly close. On supply chain, it is the worst of the three by a wide margin,
and supply chain is the kind of risk that shows up two years in, not two weeks in. My pick is
oneterm-vt with the fork budget written down in advance. If I were shipping commercially with
no appetite for that, I would take alacritty_terminal and be slightly unhappy about it.

---

## 5. The five gaps that would change the decision

Closing these would move oneterm-vt from "conditional first" to "unconditional first".

1. **Four public methods return unnameable types.** `Terminal::resize`, `cursor_style`, `sync`
   and `placements` return `ResizeOutcome`, `CursorStyle`, `&SyncState` and `&[Placement]`,
   none of which is reachable from outside the crate by any path. Verified by compile error
   against `oneterm_vt::*`, `oneterm_vt::grid::*`, `oneterm_vt::reflow::*`,
   `oneterm_vt::terminal::*` and `oneterm_vt::graphics::*` -- the last three are private
   modules. This is a two-line fix (re-export them) and it is the only outright defect I found
   in the public surface.

2. **Kitty keyboard is answered but not honoured.** Feeding `CSI > 31 u` sets the flags, and
   `CSI ? u` then replies `CSI ? 31 u` -- the terminal tells the program it has the protocol.
   `encode_key` continues to return `ESC O A` for Up and `a` for the `a` key. A program that
   negotiates kitty keyboard and is told yes will send keys it cannot receive. Chapter 6
   documents this honestly, but documentation does not reach the program inside the terminal;
   chapter 11's own rule -- never claim a capability that does not exist -- is violated here.
   Either implement the encoding or refuse the push.

3. **Not published, and not taggable yet.** No crates.io, no docs.rs, no tag that carries the
   crate, and a version number owned by a different application's release cycle. `cargo add
   oneterm-vt` is the first thing an evaluator types, and it fails. Publishing is the single
   highest-leverage change available: it would move criterion (h) from 2 to 4 on its own, and
   the crate is already packaged for it (`cargo package` runs in CI).

4. **No performance evidence at all.** No benches, no criterion, no throughput number, no
   `vtebench` result. For a crate whose main pitch to a GPU-renderer author is a damage model,
   the absence of any measurement is conspicuous. A `benches/` directory with feed throughput
   and `snapshot_update` cost at a deep scrollback would settle it.

5. **`DECRQCRA` is missing, so there is no third-party conformance score.** Chapter 11 says so
   directly: `esctest` reads the screen back with rectangle checksums, so without `CSI * y` the
   harness cannot run, and no external number exists for this engine. `DECRQSS` and
   `XTGETTCAP` are absent for the same family of reasons and break tmux and neovim capability
   probing. Implementing `DECRQCRA` alone converts "we test ourselves thoroughly" into a number
   an outsider can compare against alacritty.

Honourable mention, not in the five: images are Sixel only. The kitty graphics protocol and
the iTerm2 `OSC 1337` inline form are both absent, and for a terminal shipping in 2026 that is
a feature gap users will notice -- though `OscRoutes` means an embedder can decode 1337 itself,
which is more than either competitor allows.

---

## Appendix: commands and measurements

All runs on `x86_64-pc-windows-msvc`, rustc/cargo 1.96.0, `CARGO_BUILD_JOBS=4`, 2026-09-15.
Repository at commit `a2c28cc5`.

### Documentation build

```
$env:RUSTDOCFLAGS="-D warnings"; cargo doc -p oneterm-vt --no-deps --all-features
  -> 3.53 s (warm dependency cache), exit 0, no warnings
  -> target/doc/oneterm_vt/guide/index.html, 13 chapter modules ch01..ch13
```

### Consumer crate `vtprobe` (guide chapters 2, 5, 6, 13)

```
oneterm-vt = { git = "file:///D:/TrungKFC-Research/Rust/myTerm2", branch = "main" }
polling = "3"

cargo build           -> Locking 29 packages; Finished in 8.00 s; FIRST TRY, no errors
cargo run             -> see transcript below
cargo build --release -> 4.92 s; target/release/vtprobe.exe = 564,736 B
cargo tree -e normal  -> 19 unique entries (oneterm-vt subtree: 16 dependency crates)
```

Run transcript, abridged:

```
[ch2] fed 37 bytes, update = Full; row 0 |hello world|; row 2 |  row three|; 2 style runs
[ch5] raw OSC 31337 -> "hello=world;second"   (routed Forward by the embedder)
[ch5] raw OSC 9 -> "a notification"  + typed event Notification { .. }  (BuiltinAndForward)
[ch5] XTVERSION reply = "\x1bP>|vtprobe(0.1.0)\x1b\\"
[ch6] plain Up = "<ESC>[A";  Ctrl+C = "<03>";  Up under DECCKM = "<ESC>OA"
[ch6] keyboard_flags = KeyboardFlags(DISAMBIGUATE_ESC_CODES | REPORT_EVENT_TYPES |
      REPORT_ALTERNATE_KEYS | REPORT_ALL_KEYS_AS_ESC | REPORT_ASSOCIATED_TEXT)
[ch6] Up under kitty flags = "<ESC>OA"    <-- gap 2
[ch6] 'a' under kitty flags = "a"         <-- gap 2
[ch6] kitty query reply = "<ESC>[?31u"    <-- gap 2: the terminal says yes anyway
[ch13] child pid = Some(1692); grid |hi|; saw the child's output: true
[all] done in 34.4 ms
```

### `default-features = false`

```
cargo tree -e normal ->
  vtprobe
  `-- oneterm-vt v0.5.2
      |-- bitflags |-- log |-- memchr |-- rustc-hash
      |-- unicode-segmentation `-- unicode-width
cargo check -> exactly two errors, both "unresolved import" for `oneterm_vt::pty`
               and `polling`. Nothing else in the engine moved.
```

### Unnameable-type probe (gap 1)

```
let _: Option<oneterm_vt::ResizeOutcome> = None;          E0425 cannot find type
let _: Option<oneterm_vt::grid::ResizeOutcome> = None;    E0425 cannot find type
let _: Option<oneterm_vt::reflow::ResizeOutcome> = None;  E0603 module `reflow` is private
let _: Option<oneterm_vt::CursorStyle> = None;            E0425 cannot find type
let _: Option<oneterm_vt::terminal::CursorStyle> = None;  E0603 module `terminal` is private
let _: Option<oneterm_vt::grid::Placement> = None;        E0425 cannot find type
let _: Option<oneterm_vt::graphics::Placement> = None;    E0603 module `graphics` is private
```

Inferred types, from deliberate `let _: () =` mismatches:
`ResizeOutcome`, `CursorStyle`, `&SyncState`, `&[Placement]`, `&TerminalGrid`
(only the last is reachable, as `oneterm_vt::grid::TerminalGrid`).

### Like-for-like minimal embeds

```
vtmin     (oneterm-vt, no-default-features)  7 crates,  release 3.1 s,    359,424 B
alacprobe (alacritty_terminal 0.26.0)       47 crates,  release 14.1 s,   370,688 B
rioprobe  (rio-vt 0.5.26)                   67 crates,  release 20.5 s, 1,172,480 B

alacprobe --no-default-features             36 crates
rioprobe  --no-default-features             50 crates
```

All three printed the same grid: `row 0 |hello world|`, `row 2 |  row three|`.

### Test suite and source facts

```
cargo test -p oneterm-vt --all-features  -> 695 passed, 0 failed, 4 ignored, 47.6 s
                                            (lib 496, 13 integration binaries, 36 doctests)
find crates/vt/src -name '*.rs' | xargs wc -l   -> 29,905 total
grep -rn unsafe crates/vt/src --exclude-dir=pty -> 5 hits, all in comments
grep -rn unsafe crates/vt/src/pty               -> 54 hits
crates/vt/fuzz/fuzz_targets/parser.rs           -> cargo-fuzz, nightly, Linux only
crates/vt/examples/headless.rs                  -> the example chapter 2 walks
```

### CI evidence visible to an outsider (`.github/workflows/ci.yml`)

A dedicated `vt-package` job builds and tests the crate with no default features and with all
of them, runs the `headless` example, documents it with denied rustdoc warnings, runs
`cargo package`, tests the `vt-paranoid` and `regex` features, checks the public API against a
stored surface file, asserts the two platform surfaces differ only inside `::pty`, and asserts
by `cargo tree` that `--no-default-features` is exactly six leaf dependencies. An outsider can
read all of it, and it is why the README's dependency numbers can be trusted.

### Sources consulted for the other two

```
raw.githubusercontent.com/raphamorim/rio/main/{rio-vt/Cargo.toml, rio-vt/README.md, Cargo.toml}
docs.rs/rio-vt/0.5.26/rio_vt/{index.html, crosswords/struct.Crosswords.html}
crates.io/api/v1/crates/rio-vt/versions
raw.githubusercontent.com/alacritty/alacritty/master/{alacritty_terminal/Cargo.toml, Cargo.toml}
docs.rs/alacritty_terminal/0.26.0/alacritty_terminal/{index.html, term/struct.Term.html}
api.github.com/repos/alacritty/alacritty/contents/alacritty_terminal/tests/ref  (48 entries)
crates.io/api/v1/crates/alacritty_terminal/versions
```

All four consumer crates were created under the evaluation scratchpad and deleted afterwards.
