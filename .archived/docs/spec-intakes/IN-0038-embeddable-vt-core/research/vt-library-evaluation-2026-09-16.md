# Choosing a terminal core, re-run: oneterm-vt vs alacritty_terminal vs rio-vt

A second outside evaluation by the same developer who wrote
[`vt-library-evaluation.md`](vt-library-evaluation.md) on 2026-09-15. Same position: starting a new
terminal application in Rust, own GPU renderer, tabs and splits, SSH later. Same rubric, same
probes, so the two reports compare line by line.

The reason for the re-run: the `oneterm-vt` maintainers say they closed gaps 1, 2, 4 and 5 from
that report and deliberately kept gap 3. This report checks that claim from outside.

Re-evaluated 2026-09-16 on `x86_64-pc-windows-msvc`, rustc/cargo 1.96.0, `CARGO_BUILD_JOBS=4`.
`oneterm-vt` was judged only from what an outsider can reach: `README.md`, `CHANGELOG.md`, the
rustdoc and embedder's guide rendered by `cargo doc -p oneterm-vt --no-deps --all-features`, the
public API surface files, `.github/workflows/ci.yml`, and four consumer crates built outside the
repository against the git dependency. Repository at commit `3052c8de`, branch `main`, which is
what the git dependency resolved to in every consumer build.

---

## 1. What changed since 2026-09-15

Four of the five gaps are closed. The one that is not closed is the one the maintainers said they
were keeping, and it is still the one that decides the recommendation.

| Gap (2026-09-15) | Status today | How I checked |
| --- | --- | --- |
| 1. Four public methods return unnameable types | **Closed, and then some.** Eleven types are now at the crate root: the four named plus `ModeState`, `StrSpan`, `ByteSpan`, `ParamSpans`, `ColorOverrides`, `Watermark`, `Invalidation` | A consumer struct with all eleven as fields compiles and is populated from live calls. `Selection::invalidated_by` is now callable at all |
| 2. Kitty keyboard answered but not honoured | **Closed.** `encode_key` and the new `encode_key_event` follow the protocol | Pushed `CSI > 31 u`, queried `CSI ? u`, encoded four chords in press/repeat/release. Every byte matches the kitty specification |
| 3. Not published, not taggable | **Open, deliberately.** Still `publish = false`, still version `0.5.2`, still no tag that carries the crate, `crates.io/api/v1/crates/oneterm-vt` still answers 404 | Manifest, `git tag --list`, crates.io API |
| 4. No performance evidence | **Closed.** README and new guide chapter 14 publish a ten-fixture table with the machine, the spread and the command; a `vt-bench grid --check` trip-wire ships | Ran the documented command and the trip-wire |
| 5. `DECRQCRA` missing, so no third-party score | **Closed as a capability, open as a number.** `DECRQCRA`, `DECRQSS` and `XTGETTCAP` all answer; an `esctest` CI job now exists. No score is published anywhere in the repository | Fed all three from a consumer; read `ci.yml` |

### Scores, old and new

Scores are 1-5, higher is better, for this application. Neither `alacritty_terminal` nor `rio-vt`
has released since 2026-09-15 (crates.io: newest is 0.26.0 from 2026-04-06 and 0.5.26 from
2026-08-23), so neither was re-scored; their columns are reproduced unchanged.

| # | Criterion | oneterm-vt old | oneterm-vt new | alacritty | rio-vt |
| --- | --- | ---: | ---: | ---: | ---: |
| a | VT/xterm coverage | 3 | **4** | 4 | 4 |
| b | Grid, scrollback, reflow, damage | 5 | 5 | 4 | 3 |
| c | API and threading model | 4 | **5** | 3 | 2 |
| d | Extensibility | 5 | 5 | 2 | 2 |
| e | Dependency weight, platform coupling | 5 | 5 | 3 | 2 |
| f | Documentation and onboarding | 5 | 5 | 2 | 2 |
| g | Tests and conformance evidence | 4 | 4 | 4 | 3 |
| h | Maintenance, licence, release story | 2 | 2 | 5 | 3 |
| i | Performance claims and evidence | 2 | **4** | 3 | 3 |
| j | Fit for this application | 5 | 5 | 3 | 2 |
| | **Total (50)** | **40** | **44** | **33** | **26** |

Three criteria moved, all for `oneterm-vt`: (a) +1, (c) +1, (i) +2. Criterion (h), the one that
decides whether the recommendation is conditional, did not move at all.

---

## 2. Fact sheets

### 2.1 oneterm-vt

| | 2026-09-15 | 2026-09-16 |
| --- | --- | --- |
| Version examined | 0.5.2, commit `a2c28cc5` | 0.5.2, commit `3052c8de` |
| Licence / edition / MSRV | Apache-2.0 / 2024 / 1.96.0 | unchanged |
| Published | No | **No.** `publish = false`, 404 on crates.io, newest tag `v0.5.2` predates the crate |
| Features | `pty` (default), `vt-paranoid`, `regex` | unchanged |
| Dependencies | 6 leaf with `--no-default-features`, 16 with `pty` | unchanged, and I re-measured both |
| Source size | 29,905 lines | **32,823 lines** (+9.8%) |
| `unsafe` in the engine | 0 | **0** (6 textual hits, all comments or one test name) |
| `unsafe` in `src/pty` | 54 | 54 |
| Tests | 695 passing, 4 ignored | **847 passing, 4 ignored**, 18 integration binaries |
| Guide | 13 chapters | **14 chapters**, 2,607 lines of Markdown, every Rust block a doctest |
| Performance evidence | none | a published ten-fixture table, a `vt-bench` binary, a committed baseline, a `--check` trip-wire |

The guide is still the crate's distinguishing artefact and it grew the right chapter. Chapter 14
publishes a number *and* a list of what the number is not: no renderer, no pseudo-console, no
thread hand-off, and explicitly not a comparison against another engine. It also publishes the
spread per fixture and tells you to read no difference smaller than it. That is the most
self-limiting performance section I have seen in a Rust crate, and it is why I scored (i) 4 rather
than 5: it is one machine, one operating system, a median rather than a distribution, and the
author's own hardware.

Chapter 11 now pins the `DECRQCRA` checksum variant in prose *and* in a doctest, and says plainly
that a program written against xterm's default will disagree. Gate `Config::allow_screen_readback`
defaults to `false`, so the sequence answers nothing and is counted unhandled unless the embedder
opens it. I verified both halves.

### 2.2 alacritty_terminal

Unchanged since 2026-09-15 and reproduced for comparison. Version 0.26.0 (published 2026-04-06) is
still the newest on crates.io; `master` still reads 0.26.1-dev. Apache-2.0 / 2024 / 1.85.0. Feature
set is still `default = ["serde"]` and nothing else. Parser still delegated to `vte` 0.15. The
`tty` module is still unconditional. 48 recorded reference cases, no benches in the crate.

Re-measured on this machine: 40 dependency crates resolved with defaults, 33 with
`default-features = false`. The consumer still needs `Processor::<vte::ansi::StdSyncHandler>::new()`
to satisfy inference, and still has to write its own `Dimensions` impl.

### 2.3 rio-vt

Unchanged since 2026-09-15. Version 0.5.26 (published 2026-08-23) is still the newest of 25
versions on crates.io; no release in the intervening three weeks, which quiets but does not settle
the churn concern from the last report. MIT / 2021 / 1.96.1.

Re-measured: 52 dependency crates with defaults, 43 with `default-features = false`. The seam is
still visible and I found one more of it this time. To drive `Crosswords` through rio's own I/O
machinery you need `performer::Machine`, whose constructor is:

```
Machine::new(Arc<FairMutex<Crosswords<U>>>, pty: T, event_proxy: U, WindowId, route_id: usize)
```

That requires a `teletypewriter::EventedPty`. The transport-free path exists -- a
`performer::handler::Processor` fed byte by byte, which is what my probe used -- but it is not
documented as such on docs.rs, and finding it took three pages of API browsing. Blank cells still
do not trim as whitespace, so the probe's output needed handling the other two did not.

---

## 3. The build test, re-run

Same shape as before: three consumer crates doing the same minimal embed, plus a `vtprobe2` that
follows the guide further. All four were created outside every repository and deleted afterwards.

| | oneterm-vt (`vtmin2`) | alacritty_terminal (`alacprobe2`) | rio-vt (`rioprobe2`) |
| --- | --- | --- | --- |
| Manifest line | git, `branch = "main"`, `default-features = false` | `alacritty_terminal = "0.26.0"` | `rio-vt = "0.5.26"` |
| Compile iterations to a running grid | **1** | 1 | 2 |
| Dependency crates, defaults | **7** | 40 | 52 |
| Same, `default-features = false` | **7** | 33 | 43 |
| Release build, cold | **3.5 s** | 15.4 s | 22.5 s |
| Release `.exe` | 373,248 B | **368,128 B** | 1,170,944 B |

Counting method note, because the numbers moved: the old report counted lines of
`cargo tree -e normal`, which double-counts a subtree that appears twice, and for alacritty and rio
it quoted the crates-io dependency totals across all targets. This report counts **unique package
names resolved for `x86_64-pc-windows-msvc`**, for all three crates, and excludes the consumer
crate itself. The old 7/47/67 and today's 7/40/52 are the same story measured more carefully;
`oneterm-vt`'s figure is unaffected because its tree has no repeats.

The README's dependency claims are still exact. `--no-default-features` gave precisely the six leaf
crates it names; the default `pty` build gave 16 crates in the tree, which is the number the README
prints for this target. CI still asserts the six.

`alacprobe2` compiled first try this time, because I already knew the `StdSyncHandler` turbofish
from the last report. That is knowledge an evaluator does not have on day one, so I have left the
"friction found" note below rather than letting the 1 speak for itself.

### `vtprobe2`: guide chapters 2, 5, 6, 13, 14

Two compile iterations, four errors. **None of the four was a guide defect.** All four were mine,
from writing against the public-API surface listing (which carries item names but not signatures)
instead of against the guide: a struct variant I matched as a tuple variant, `Pos::row` being a
`RowId` rather than an integer, `Mode::inert_state` returning an `Option`, `Side` living at the
crate root rather than in `grid`, and a missing wildcard arm on a `#[non_exhaustive]` enum the crate
warned me about by name.

To separate my errors from the crate's, I pasted four guide examples **verbatim** into a second
binary in the same consumer crate: chapter 6's `DISAMBIGUATE_ESC_CODES` example, chapter 6's
`KeyEvent` release example, and chapter 11's `DECRQCRA` and `DECRQSS`/`XTGETTCAP` examples. All four
compiled and their assertions passed, unchanged, on the first attempt. The guide is still
executable rather than aspirational, and it is now executable about the new features too.

Run transcript, abridged:

```
[ch2] fed 47 bytes, update = Full; row 0 |hello world|; row 2 |  row three|; 2 style runs
[ch5] raw OSC 31337 -> "hello=world;second"
[ch5] OSC 9 typed event = true, raw forward = true
[ch5] XTVERSION reply = <ESC>P>|vtprobe2(0.1.0)<ESC>\
[ch6] legacy: Up = <ESC>[A; Ctrl+C = <03>; Up under DECCKM = <ESC>OA
[ch13] child pid = Some(17576); grid |hi|; saw the child's output: true
[ch14] plain_ascii-like, 8 MiB, 5 cycles: median 72.6 MiB/s, 13.1 ns/byte, spread 5%
[all] done in 709.0 ms
```

The three legacy encodings are byte-identical to the 2026-09-15 transcript. That is the claim the
changelog makes about the keyboard change -- identical for any program that never negotiated -- and
it holds.

---

## 4. The four closed gaps, checked

### 4.1 Nameable types (gap 1)

A consumer struct with all eleven types as fields compiles, and every field was filled from a live
call rather than a default:

```
ResizeOutcome{reflowed:true, rows_trimmed:0}  CursorStyle{Block,false}  SyncState.is_set=false
placements=0  ModeState=NotSupported  StrSpan=present  ByteSpan=present  ParamSpans=present
ColorOverrides.get=None  Watermark(SeqNo(1))  Invalidation::Reset kills the selection = true
```

The last one matters more than the spelling fix: `Selection::invalidated_by` takes `Invalidation`
as an **argument**, so before this release it was a public method no embedder could call at all.
CI now carries `vt-public-api.py --check-nameable`, which fails on a public signature naming a type
defined in a private module, and the changelog says the script's allow-list is gone entirely. This
class of defect cannot come back silently.

### 4.2 Kitty keyboard (gap 2)

Pushed `CSI > 31 u` (all five flags), queried `CSI ? u`, got `CSI ? 31 u` back -- the same answer as
last time. The difference is what the encoder does next.

| Chord | `encode_key` | press | repeat | release |
| --- | --- | --- | --- | --- |
| `Up` | `CSI A` | `CSI A` | `CSI 1;1:2A` | `CSI 1;1:3A` |
| `a` | `CSI 97;;97u` | `CSI 97;;97u` | `CSI 97;1:2;97u` | `CSI 97;1:3u` |
| `ctrl+c` | `CSI 99;5u` | `CSI 99;5u` | `CSI 99;5:2u` | `CSI 99;5:3u` |
| `shift+1` | `CSI 49;2;33u` | `CSI 49;2;33u` | `CSI 49;2:2;33u` | `CSI 49;2:3u` |

Checked against <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>. Every line is correct:

- `Up` uses the letter form `CSI A`, not a private-use number; the specification's rule is that the
  leading number is always `1` and must be omitted when the modifier field is also absent. A press
  with no modifiers and event type 1 is therefore bare `CSI A`, and repeat and release carry the
  `1;1:2` / `1;1:3` modifier-and-event field. All three are exactly right.
- Modifier encoding is `1 + shift*1 + alt*2 + ctrl*4`: ctrl alone is `5`, shift alone is `2`. Both
  correct.
- Event types are press 1 (omitted), repeat 2, release 3. Correct.
- `shift+1` reports the **un-shifted** code 49 as the primary, with `!` (33) in the associated-text
  field. This is the rule implementations most often get wrong, and the crate's derived PC-101
  shift table gets it right.
- No associated text on a release, which is the only sensible reading of a field that describes
  text the event inserts.

Last time this line read "the terminal says yes anyway". It does not any more.

### 4.3 Performance (gap 4)

Ran the documented command from chapter 14 on this machine:

```
cargo run -p oneterm-tools --release --bin vt-bench -- grid --mib 32
```

| Fixture | published MiB/s | published spread | my MiB/s | my spread |
| --- | ---: | ---: | ---: | ---: |
| `plain_ascii` | 74.9 | 4% | 46.9 | 17% |
| `long_lines` | 80.9 | 2% | 53.8 | 22% |
| `heavy_sgr` | 226.5 | 5% | 146.9 | 25% |
| `tui_redraw` | 125.6 | 3% | 91.5 | 26% |
| `scroll_region` | 61.0 | 5% | 40.8 | 11% |
| `cjk_wide` | 115.0 | 4% | 71.8 | 44% |
| `dense_cells` | 195.6 | 8% | 125.1 | 68% |
| `scrolling` | 80.1 | 8% | 52.2 | 6% |
| `sixel` | 46.7 | 8% | 32.1 | 16% |
| `osc_9_7` | 102.2 | 31% | 74.3 | 23% |

My medians land at a flat 0.61 to 0.76 of the published ones, and my spreads are three to eight
times wider. Chapter 14 predicts exactly this and tells you how to read it: "run it with nothing
else running on the machine ... the spread column is how you tell whether the second rule was
actually kept." Mine says it was not. So this run **neither confirms nor contradicts** the
published figures; it confirms that the method is reproducible and self-diagnosing, which is more
than the other two crates offer.

`vt-bench grid --check` passed, with every fixture between 0.61x and 0.76x its baseline against a
half-baseline failure band. The trip-wire works, and a uniform factor across all ten fixtures is
the signature of a busy machine rather than a code regression.

The independent throughput probe in my own consumer crate -- a plain-ASCII fixture through
`Terminal::feed` at 64 KiB a chunk, five cycles -- gave 72.6 MiB/s with a 5% spread, which is
within a few percent of the published `plain_ascii` figure of 74.9. Two measurements at 46.9 and
72.6 on the same machine on the same afternoon is the noise this section is honest about.

### 4.4 Screen readback and capability probes (gap 5)

| Sequence | Gate | Bytes back |
| --- | --- | --- |
| `CSI 1;1;1;1;2;3 * y` (`DECRQCRA`) | shut (default) | nothing; `FeedStats::unhandled_sequences` = 1 |
| `CSI 1;1;1;1;2;3 * y` | open | `DCS 1 ! ~ 014A ST` |
| `DCS $ q m ST` (`DECRQSS`, SGR) | n/a | `DCS 1 $ r 0 m ST` |
| `DCS $ q r ST` (`DECRQSS`, `DECSTBM`) | n/a | `DCS 1 $ r 1;4 r ST` |
| `DCS $ q s ST` (a setting it does not have) | n/a | `DCS 0 $ r ST` |
| `DCS + q 544e ST` (`XTGETTCAP`, `TN`) | n/a | `DCS 1 + r 544e=787465726D2D323536636F6C6F72 ST` |

The `014A` is arithmetic I can check by hand: the rectangle is rows 1-2, columns 1-3 of a grid
holding `AB` and `CD`, so `0x41 + 0x42 + 0x20 + 0x43 + 0x44 + 0x20 = 330 = 0x14A`. The blank
counts as `U+0020`, the total is not negated, and no attribute contributes -- exactly the variant
chapter 11 pins.

The `XTGETTCAP` payload decodes to `xterm-256color`. The engine reads no terminfo database and no
environment variable; the table is compiled in.

`ci.yml` gained a `vt-esctest` job that clones `esctest2`, bridges it through a pty and publishes
its log as a build artifact, explicitly as a report and never a gate. That is the right call for a
VT220-level engine measured by a VT420-level harness, and chapter 11 explains why. **But there is
still no number an outsider can read from the repository.** Gap 5 is closed as a capability -- the
harness can run at all, which it could not before -- and open as a score.

---

## 5. Recommendation

Ranked, for this application:

1. **oneterm-vt**, still conditionally, and the condition is now a single item. It wins the rubric
   44-33-26, up from 40-33-26, and it now wins criteria it used to lose. The condition is that I
   accept a git pin by commit sha and write down a fork budget.
2. **alacritty_terminal**, still the correct choice for anyone who cannot accept that condition,
   and the gap it has to make up is eleven points instead of seven.
3. **rio-vt**. Nothing changed. No release in three weeks, an application's window id still in the
   core constructor, 52 dependencies, a byte-feeding path that is not documented as one.

**Is oneterm-vt now "unconditional first"? No, and gap 3 alone is why.** Every engineering reason I
had for hedging in the last report is gone. The unnameable types are gone, the keyboard protocol it
advertises is the keyboard protocol it speaks, there is a performance number with a machine and a
spread beside it, and the sequences a conformance harness needs all answer. What is left is not an
engineering objection at all. It is that `cargo add oneterm-vt` still fails, `crates.io` still
answers 404, no tag carries the crate, and `publish = false` sits in the manifest above a comment
recording that as an owner ruling rather than an oversight. I cannot write a dependency line that
names a version; I can only name a commit. Two years from now that is the risk that will matter,
and it is unchanged from the day I first wrote it down.

So: pick `oneterm-vt`, pin it by sha, keep the fork budget. That is the same sentence as last time,
and every word of the hedging around it has been deleted.

---

## 6. Remaining gaps, in priority order

1. **Not published and not taggable.** The only blocker. Worse in one narrow sense than it was:
   everything in this report sits under `## [Unreleased]` in the changelog against a version number
   of `0.5.2`, so there is no way to ask for "the release with `DECRQCRA`" except by commit sha.
   The README says `v0.5.3` will be the first tag that can carry the crate. Publishing, or even
   just tagging, moves criterion (h) from 2 to 4 on its own and makes the recommendation
   unconditional.
2. **No published conformance score.** The `esctest` job exists and its log is a CI artifact on a
   Linux job. Nothing in the repository an outsider reads carries a pass count. Printing the group
   tally into the README would finish what `DECRQCRA` started.
3. **Left-right margins.** `DECSLRM`, `DECLRMM` and the `DECSACE` rectangle modes are counted
   unhandled. Chapter 11 names this as the largest single gap and as what the `esctest` groups fail
   on. It is now the largest *conformance* gap as well, since the readback sequences landed.
4. **Images are Sixel only.** No kitty graphics protocol, no iTerm2 `OSC 1337` inline form. For a
   terminal shipping in 2026 this is the gap users notice. `OscRoutes` still means an embedder can
   decode 1337 itself, which neither competitor allows.
5. **Kitty keyboard ceilings, three of them, all stated rather than discovered.** Modifier values 1
   through 8 only, because `KeyMods` has no super, hyper, meta, caps lock or num lock; no
   private-use functional keys, so a `Super` press under `REPORT_ALL_KEYS_AS_ESC` has nothing to be
   delivered as; and the un-shifted primary code is derived from a PC-101 table rather than looked
   up, so an unusual layout reports the key it produced. None blocks this application.
6. **`Config` is not `#[non_exhaustive]` and just grew a field.** `allow_screen_readback` is a
   source break for any embedder who wrote an exhaustive struct literal, and every future knob will
   be another minor bump. Chapter 12 records the trade (setters instead of struct-literal
   construction) and chose the literal; that choice gets more expensive with each field.
7. **Package hygiene.** `cargo package -p oneterm-vt --list` ships twelve integration tests named
   after internal work-packet identifiers: `verify_us0102.rs`, `reverify_us0105.rs`,
   `us0098_verify.rs` and nine more. CI has two jobs that reject those identifiers in rustdoc text
   and in the guide's Markdown, and neither looks at file names. Cosmetic, but it is the first
   thing in the tarball that tells a consumer they are holding somebody else's internal artefact.
8. **One un-negotiated `DECRQCRA` checksum variant.** `CSI Ps * x` (`XTERM_CHECKSUM`) is counted
   unhandled, so a program written against xterm's default -- negated, with attributes folded in --
   will disagree. Chapter 11 states this and gives the reason. Since no program other than a test
   harness is known to send the sequence, this is last on the list.

Dropped from the list since 2026-09-15: gaps 1, 2, 4 and the capability half of 5.

---

## 7. What regressed, and what the new features cost

Nothing I measured got slower or stopped working. The costs are all size and churn.

- **The engine grew 9.8%**, 29,905 to 32,823 lines. The minimal consumer's release binary grew
  3.8%, 359,424 to 373,248 bytes, and its cold release build went 3.1 s to 3.5 s. That is a fair
  price for four closed gaps, and the binary is still within 1.4% of alacritty's.
- **`Config` gained a field without becoming `#[non_exhaustive]`.** Documented as a minor bump.
  Anyone who wrote `Config { scrollback_limit: n, osc_routes: r, ... }` exhaustively has a source
  break; anyone who wrote `..Config::default()` does not.
- **`ColorOverrides::set`, `reset`, `reset_indexed` and `reset_all` were withdrawn to
  `pub(crate)`.** The changelog calls this "breaking on paper only" and it is right: the type had
  no name outside the crate until this release, so nobody could have held one.
- **`encode_key` returns different bytes for any program that has negotiated kitty keyboard or
  `modifyOtherKeys`.** This is the point of the release, not an accident, and the changelog amends
  clause 6 of the promise to cover the input encoders. I re-verified the default path byte for
  byte: `Up`, `Ctrl+C` and `Up` under `DECCKM` are identical to the 2026-09-15 transcript.
- **Twelve work-packet-named test files ship in the package** (item 7 above). New since the last
  report, which recorded 13 integration binaries against today's 18.

Nothing on this list would change my ranking.

---

## Appendix: commands and measurements

All runs on `x86_64-pc-windows-msvc`, rustc/cargo 1.96.0, `CARGO_BUILD_JOBS=4`, 2026-09-16.
Repository at commit `3052c8de`. The machine was **not** quiet; see section 4.3.

### Documentation build

```
$env:RUSTDOCFLAGS="-D warnings"; cargo doc -p oneterm-vt --no-deps --all-features
  -> 4.89 s, exit 0, no warnings
  -> target/doc/oneterm_vt/guide/, 14 chapter modules ch01_overview .. ch14_performance
scripts/vt-docs.ps1 runs the same invocation and prints the guide index path.
```

### Consumer crate `vtprobe2` (guide chapters 2, 5, 6, 13, 14 + the new probes)

```
oneterm-vt = { git = "file:///D:/TrungKFC-Research/Rust/myTerm2", branch = "main" }
polling = "3"

cargo build    -> 2 iterations, 4 errors, all mine (see section 3)
cargo run --release
cargo build --release -> 5.8 s cold; target/release/vtprobe2.exe = 653,312 B
cargo tree -e normal  -> 19 lines, 17 unique packages (16 of them the oneterm-vt subtree)
cargo tree -e normal -p oneterm-vt --prefix none | sort -u -> 16 crates, matching the README
```

### The eleven-type nameability probe (old gap 1)

All eleven stored as struct fields in the consumer and populated from live calls:

```
ResizeOutcome  <- Terminal::resize(Size, ResizePolicy)
CursorStyle    <- Terminal::cursor_style()
SyncState      <- *Terminal::sync()
Vec<Placement> <- Terminal::placements().to_vec()
ModeState      <- Mode::AltScreen.inert_state()
StrSpan        <- VtEvent::Title(span)
ByteSpan       <- VtEvent::Reply(span)
ParamSpans     <- VtEvent::Osc { params, .. }
ColorOverrides <- Terminal::colors().clone()
Watermark      <- SnapshotState::watermark()
Invalidation   <- Selection::invalidated_by(&grid, Invalidation::Reset) -> true
```

Every one of these was an `E0425` or `E0603` on 2026-09-15.

### Kitty keyboard (old gap 2)

```
feed  "\x1b[>31u"   -> keyboard_flags = KeyboardFlags(DISAMBIGUATE_ESC_CODES |
                       REPORT_EVENT_TYPES | REPORT_ALTERNATE_KEYS |
                       REPORT_ALL_KEYS_AS_ESC | REPORT_ASSOCIATED_TEXT)
feed  "\x1b[?u"     -> reply "\x1b[?31u"
encode_key / encode_key_event results: see the table in section 4.2
Specification checked: https://sw.kovidgoyal.net/kitty/keyboard-protocol/
```

### Screen readback (old gap 5)

```
Config::default()                        -> "\x1b[1;1;1;1;2;3*y" answers nothing,
                                            FeedStats::unhandled_sequences == 1
Config { allow_screen_readback: true }   -> "\x1bP1!~014A\x1b\\"
"\x1bP$qm\x1b\\"                         -> "\x1bP1$r0m\x1b\\"
"\x1bP$qr\x1b\\"                         -> "\x1bP1$r1;4r\x1b\\"
"\x1bP$qs\x1b\\"                         -> "\x1bP0$r\x1b\\"
"\x1bP+q544e\x1b\\"                      -> "\x1bP1+r544e=787465726D2D323536636F6C6F72\x1b\\"
```

### Performance (old gap 4)

```
cargo run -p oneterm-tools --release --bin vt-bench -- grid --mib 32   -> table in section 4.3
cargo run -p oneterm-tools --release --bin vt-bench -- grid --check    -> PASS, ratios 0.61-0.76
in-consumer plain-ASCII feed, 8 MiB, 5 cycles                          -> 72.6 MiB/s, spread 5%
```

### Like-for-like minimal embeds

```
vtmin2    (oneterm-vt, no-default-features)   7 crates,  release 3.5 s,    373,248 B
alacprobe2 (alacritty_terminal 0.26.0)       40 crates,  release 15.4 s,   368,128 B
rioprobe2 (rio-vt 0.5.26)                    52 crates,  release 22.5 s, 1,170,944 B

alacprobe2 default-features = false          33 crates
rioprobe2  default-features = false          43 crates
```

All three printed the same grid: `row 0 |hello world|`, `row 2 |  row three|`. rio's blank cells
still do not trim as whitespace.

### Test suite, source facts and package contents

```
cargo test -p oneterm-vt --all-features -> 847 passed, 0 failed, 4 ignored (20 binaries)
find crates/vt/src -name '*.rs' | xargs wc -l   -> 32,823 total
grep -rn unsafe crates/vt/src (excluding pty)   -> 6 hits: 5 comments, 1 test function name
grep -rn unsafe crates/vt/src/pty               -> 54 hits
wc -l crates/vt/docs/guide/*.md                 -> 2,607 across 15 files (14 chapters + ch07 regex)
cargo package -p oneterm-vt --list              -> README, CHANGELOG, LICENSE, NOTICE,
                                                   15 guide chapters, examples/headless.rs,
                                                   18 tests/ files (12 work-packet-named)
```

### CI evidence visible to an outsider (`.github/workflows/ci.yml`)

The `vt-package` job is as before, plus two additions that matter here:
`python scripts/vt-public-api.py --check-nameable --no-doc`, which fails on a public signature
naming a type defined in a private module, and a new `vt-esctest` job that clones `esctest2`,
bridges it through a pty with `--expected-terminal xterm --xterm-checksum 334 --max-vt-level 4`,
and uploads the log as an artifact, recorded and never gated. The two "must stand alone" jobs
(rustdoc text and guide Markdown may not cite internal document identifiers) are unchanged, and
neither of them looks at test file names.

### Sources consulted for the other two

```
crates.io/api/v1/crates/alacritty_terminal/versions  -> newest 0.26.0, 2026-04-06, not yanked
crates.io/api/v1/crates/rio-vt/versions              -> newest 0.5.26, 2026-08-23, 25 versions
crates.io/api/v1/crates/oneterm-vt                   -> HTTP 404
docs.rs/rio-vt/0.5.26/rio_vt/{index,performer,performer::parser,performer::handler,ansi}
raw.githubusercontent.com/raphamorim/rio/main/rio-vt/src/performer/mod.rs
sw.kovidgoyal.net/kitty/keyboard-protocol/
```

All four consumer crates were created under this evaluation's scratchpad and deleted afterwards.
