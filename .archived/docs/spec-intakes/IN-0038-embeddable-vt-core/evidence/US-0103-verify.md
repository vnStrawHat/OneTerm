# US-0103 independent verification: the `oneterm-vt` embedder's guide

Verifier: independent session, own worktree
`D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-a2420e9c48412e1a1`.
Tree under test: branch `docs/vt-embedder-guide` at `6d8333a6`, base `main` at `af5df2e7`
(`git merge-base` confirms `af5df2e7`). Host `x86_64-pc-windows-msvc`, rustc/rustdoc
1.96.0 (ac68faa20 2026-05-25), `CARGO_BUILD_JOBS=3`.

Nothing was pushed, nothing was committed to the implementer's branch, `harness.db` was
read from a copy and never written, and no `oneterm.exe` was touched. The one child
process spawned (chapter 13's `cmd /c echo hi`) exited on its own; the `cmd`/`conhost`/
`OpenConsole` pid set was identical before and after the run.

## Verdict

**PASS-WITH-NOTES.**

Every measurable claim in the packet is true as written: thirteen chapters, 1 792 lines,
27 guide doctests over thirteen chapters, 31 passed / 2 ignored identical under all three
feature settings, rustdoc clean under `-D warnings`, no broken link in the rendered guide,
public-API snapshots genuinely unchanged, every gate green. The guide is accurate about
the API on nearly every point checked in chapters 1, 4, 5, 10, 12 and 13.

Four things to fix before merge, in order:

1. **Finding 1** -- the single block in the guide that rustdoc does *not* compile is wrong,
   and it is the block an embedder copies (chapter 13's poll loop).
2. **Finding 12** -- chapter 13 says the transport's threads are "all joined on drop";
   none of them is joined anywhere, and the crate's own rustdoc says the Windows ones
   cannot be.
3. **Finding 11** -- chapter 3 states `Terminal` is not `Sync`; the API reference rendered
   from the same `cargo doc` run says it is.
4. **Finding 3** -- chapter 12's `#[non_exhaustive]` list omits a public type and
   miscategorises a struct as an enum.

None of them fails a gate, and that is the point: every one is a **prose** claim about the
API, and the packet's whole safety argument -- "a wrong sentence about the API fails the
build rather than misleading a reader" (packet, Classification) -- only covers the code
blocks. The doctests are green and four prose sentences are still false. That is worth
recording as the honest scope of the design, not as a defect in it.

## 1. Render and link check (the packet's E2E criterion, done headlessly)

```
pwsh -NoProfile -File scripts\vt-docs.ps1
```

exit 0, no warning, and it printed
`file:///D:/.../target/doc/oneterm_vt/guide/index.html`, which exists.

Then a headless link walk (script: `scratchpad/linkcheck.py`, reproduced under
"Scripts" below) over `target/doc/oneterm_vt/guide/**/*.html`:

| Measure | Result |
| --- | --- |
| HTML pages under `guide/` | 14 (index + 13 chapters) |
| `href`/`src` attributes examined | 428 |
| External URLs (not fetched) | 0 |
| **Relative targets that do not exist on disk** | **0** |
| Fragment targets checked | 245, of which 231 resolved |
| Chapters linked from `guide/index.html` | 13, `ch01_overview` .. `ch13_pty`, ascending |

The 14 unresolved fragments are all the same shape --
`../../../src/oneterm_vt/guide.rs.html#14` and its twelve siblings, plus
`#1-50` from the index. The **file** exists in every case; only the `#NN` anchor is
absent from the static HTML, because rustdoc 1.96 emits source line numbers as
`<span data-nosnippet>` (50 of them in that file, one per line) and materialises the
`id="NN"` anchors in JavaScript at view time. These are rustdoc's own "source" links,
not links the guide wrote. **Not defects.**

No browser was used; this is the manual browser pass of the packet's Verification Plan
done mechanically, and it is stronger than the plan's "follow every intra-doc link in
chapter 4" because it follows every link on every chapter page.

## 2. Doctests

```
cargo test -p oneterm-vt --doc                          -> 31 passed; 0 failed; 2 ignored
cargo test -p oneterm-vt --doc --all-features           -> 31 passed; 0 failed; 2 ignored
cargo test -p oneterm-vt --doc --no-default-features    -> 31 passed; 0 failed; 2 ignored
cargo test -p oneterm-vt --doc -- --list                -> 33 tests; 27 name docs/guide/
```

Identical under all three settings, as claimed. Per chapter, from `--list`:
01 x1, 02 x7, 03 x1, 04 x2, 05 x4, 06 x2, 07 x3, 08 x1, 09 x1, 10 x1, 11 x1, 12 x1,
13 x2 -- every chapter contributes at least one. Fence census across the thirteen files:
25 ```rust```, 2 ```rust,ignore```, 5 ```text```, 1 ```toml``` (27 Rust blocks, matching
the count). Both ignored blocks state their reason in the block
(`07-search.md:92`, `13-pty.md:52`).

`crates/vt/src/guide.rs` carries no `cfg` attribute, so `ch13_pty` is genuinely not
feature-gated and its text compiles in a `--no-default-features` build. Confirmed.

**Mutation checks**, on three chapters *different* from the implementer's (04/07/10), so
this is not a re-run of their spot check. Each token deleted or mangled, `cargo test
-p oneterm-vt --doc` run, then the file byte-restored (`git status` clean afterwards):

| Chapter | Mutation | Result |
| --- | --- | --- |
| `01-overview.md:30` | `Terminal::new` -> `Terminal::nw` | exit 101, `E0599`, 30 passed / 1 failed |
| `05-osc.md:33` | `has_builtin` -> `has_bultin` | exit 101, `E0599`, 30 passed / 1 failed |
| `13-pty.md:37` | dropped the `Instant::now()` argument to `feed` | exit 101, `E0061`, 30 passed / 1 failed |

The cannot-rot property holds -- **for the blocks rustdoc compiles**. See finding 1 for
the blocks it does not.

## 3. Outsider walk-through

A consumer crate was created outside the repository (scratchpad only; `cargo init` run
there and nowhere else), depending on the branch by git exactly as an embedder would:

```toml
oneterm-vt = { git = "file:///D:/TrungKFC-Research/Rust/myTerm2", branch = "docs/vt-embedder-guide" }
polling = "3"
```

`cargo build` resolved 29 packages and compiled
`oneterm-vt v0.5.2 (file:///...?branch=docs%2Fvt-embedder-guide#6d8333a6)` -- so the
`include_str!` chapters do travel with a git dependency, which is the real form of the
`cargo package --list` criterion.

The program followed chapters 2, 5, 6 and 13 by copying their fragments in order. It ran
to completion, exit 0:

```
=== chapter 2 ===
title      "headless demo"
Cwd { host: StrSpan { start: 13, len: 9 }, path: StrSpan { start: 22, len: 4 } }
osc 1337   "SetUserVar=demo"
Repaint
stats.bytes = 104
-- screen (Full) --
|hello world                     |
|                                |
|  row three                     |
|                                |
=== chapter 5 ===
Forward event for OSC 20308, fields = [[49], [101, 121, ...]]   (b"1", b"eyJ2IjoxfQ==")
route defaults as documented
=== chapter 6 ===
flags before push: KeyboardFlags(0x0)
flags after `CSI > 1 u`: KeyboardFlags(DISAMBIGUATE_ESC_CODES)
ArrowUp under kitty flags = Some([27, 91, 65])      (ESC [ A -- legacy, as chapter 6 says)
ArrowUp with DECCKM set = Some([27, 79, 65])        (ESC O A)
modify_other_keys = 0
=== chapter 13 ===
child exited: Some(ExitStatus(ExitStatus(0)))
screen text = ["hi"]
dropped the console (this blocks, per chapter 13)
```

Every behavioural promise those four chapters make held: chapter 2's screen is what the
chapter's byte string says it should be and `state.rows().len()` is 4; chapter 5's private
OSC 20308 arrives as a `Forward` `VtEvent::Osc` with parameter 0 being the number itself;
chapter 6's `encode_key` really does ignore the kitty flags it reports and really does
read `DECCKM`; chapter 13's default pty really does run `cmd /c echo hi` and deliver `hi`
to `feed`.

**One chapter fragment did not compile** (finding 1) and two places assumed knowledge an
outsider does not have (findings 2 and 5). The consumer crate was deleted afterwards.

## Findings

### 1. Chapter 13's poll loop does not compile -- MEDIUM, fix before merge

`crates/vt/docs/guide/13-pty.md:77` and `:82`:

```rust
let mut events = Vec::new();
...
    poller.wait(&mut events, None)?;
    for event in &events {
```

`polling` 3.x -- the version this crate depends on and the version an embedder will
resolve -- declares
`pub fn wait(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<usize>`.
Copied verbatim into the consumer crate, this is:

```
error[E0308]: mismatched types
   --> src\main.rs:195:19
    |
195 |             .wait(&mut events, Some(...))
    |              ---- ^^^^^^^^^^^ expected `&mut Events`, found `&mut Vec<_>`
```

It compiles as `polling::Events::new()` plus `for event in events.iter()`.

This is exactly the class of failure the packet says cannot happen here -- the packet's
"The rendering decision", point 1, cites `rio-vt`'s README calling an `EventListener`
method the trait does not have and says "that failure is structurally impossible here".
It is not impossible for a ```rust,ignore``` block, and chapter 13's acceptance is
satisfied by its *other*, live block, so nothing checks the one an embedder copies.

The block cannot simply be un-ignored (it names `oneterm_vt::pty`, absent under
`--no-default-features`, and would spawn a child). Cheapest fix that restores the
property: keep the loop `ignore` but move the `polling` mechanics into a live
`# #[cfg(feature = "pty")]`-free helper, or -- simpler -- correct the two lines and add a
`compile_fail`-free companion test in `crates/vt/src/pty/loopback_tests.rs` that uses the
same shapes. `loopback_tests.rs:104-127` already registers with these tokens and does
compile; the chapter drifted from it.

### 2. Chapter 13 never tells the embedder to depend on `polling` -- MEDIUM

`oneterm-vt` does not re-export `polling`: `crates/vt/src/pty/mod.rs:56` is
`use polling::{Event, PollMode, Poller};` with no `pub use`, and `polling` appears in no
`pub use` in `crates/vt/src/lib.rs`. `EventedReadWrite::register` therefore takes an
`&Arc<polling::Poller>` the embedder must construct from *their own* `polling`
dependency, at a semver-compatible major, or the types will not unify.

Chapter 13 opens with `let poller = Arc::new(polling::Poller::new()?)` and never says
where `polling` came from. Chapter 12:100-103 names it "this crate's **only** public
dependency" but frames that as a versioning consequence, not as an instruction. An
outsider hits `error[E0433]: cannot find module or crate `polling` in this scope` on their
first build -- verified by removing `polling` from the consumer's manifest and rebuilding;
I only got past it because I had read `crates/vt/Cargo.toml`. One sentence in
chapter 13 ("add `polling = \"3\"` to your own manifest; it is not re-exported") closes
it.

### 3. Chapter 12's `#[non_exhaustive]` list is wrong in two ways -- LOW/MEDIUM

`crates/vt/docs/guide/12-versioning.md:53-59`, heading "Which enums you must write a
wildcard arm for", list "`VtEvent`, `OscRoute`, `Progress`, `ShellMark`, `NamedKey`,
`SearchPattern` and `SearchOptions`", followed by "A new variant on any of them is a
patch release".

- **Missing.** `oneterm_vt::input::KeySpec` is `#[non_exhaustive]`
  (`crates/vt/src/input/key.rs:116`) and public
  (`crates/vt/src/input/mod.rs:17`, `pub use key::{KeyMods, KeySpec, NamedKey, encode_key};`).
  A reader who matches on `KeySpec` needs the wildcard arm and the chapter does not tell
  them. The full set of `#[non_exhaustive]` public types in the crate is
  `Progress`, `ShellMark`, `VtEvent`, `KeySpec`, `NamedKey`, `SearchOptions`,
  `SearchPattern`, `OscRoute` -- eight, not seven.
- **Miscategorised.** `SearchOptions` is a **struct**
  (`crates/vt/src/search/mod.rs:57`), not an enum. It has no variants and no wildcard arm;
  what `#[non_exhaustive]` costs a caller there is that it cannot be built with a struct
  literal, so the guidance should be `..SearchOptions::default()`. As written the sentence
  is wrong for the one item on the list where the advice differs.

### 4. Chapter 4's "four questions" arithmetic does not match its own section -- LOW

`crates/vt/docs/guide/04-events.md:44-45`: "Four of those are questions. Three of them the
engine cannot answer". The section that follows covers three events, and says of the first
(`04-events.md:47`) "**`Reply` is not a question**". That leaves two the engine cannot
answer (`ColorQuery`, `ClipboardLoad`) and no fourth question named anywhere. Neither
number is reachable from the text. A reader who counts stops to look for a variant that
is not there.

### 5. Chapter 2 does not assemble into the program it describes -- LOW

`02-embedding.md:3-5` says the example "feeds one chunk of bytes, prints the events, and
prints the screen", and the packet's own outline calls it "the headless example walked
line by line, ending with a working program". Fragments 1-7 give the imports, the bytes,
the config, the feed, the event loop, the snapshot and `row_text` -- but never the call
site for `row_text`. `crates/vt/examples/headless.rs:71-73` has it:

```rust
for row in state.rows() {
    println!("|{}|", row_text(row));
}
```

I had to invent that loop (and the `main` scaffolding) to finish the walk. It is two lines
and a reader will get there, but "walked line by line, ending with a working program" is
not what the chapter delivers.

Related, same severity: `04-events.md:48` says of `Reply` "write them to the process input
exactly as given" and `:26` says of `ClipboardLoad` "format and write the reply yourself",
with no pointer to *where* the write goes. `EventedReadWrite::writer` is chapter 13, and
chapter 4 never names it. One cross-reference fixes it.

### 6. A Plan tick claims work this packet did not do -- LOW (record accuracy)

Packet Plan: "- [x] Tighten the existing `cargo doc` step in `scripts/ci-local.sh`,
`scripts/ci-local.ps1` and `.github/workflows/ci.yml` with `RUSTDOCFLAGS=\"-D warnings\"`
and `--all-features`."

`git diff af5df2e7..6d8333a6` changes no `cargo doc` invocation in any of those three
files. The flags were already there before this branch: `scripts/ci-local.sh:64,68`,
`scripts/ci-local.ps1:70-71`, `.github/workflows/ci.yml:204-207`, all from `US-0097`. The
Reconciliation paragraph says exactly this ("needed **no** change") -- so the prose is
honest and the checkbox contradicts it. Untick or reword.

### 7. The one genuinely new CI step this packet adds is undocumented in its owning LLD -- LOW

The diff adds a **new** gate, not a tightened one: a guide self-containment grep, in
`.github/workflows/ci.yml` (+10 lines, "The embedder guide must stand alone"),
`scripts/ci-local.ps1` (+14) and `scripts/ci-local.sh` (+9). It is a good gate and it
works (verified below).

But the packet's Documentation Action says packaging.md needs only "a row ... for the
tightened `cargo doc` step", and Reconciliation concludes packaging.md "needed **no**
change". `packaging.md`'s "Effect on the repository's existing checks" table (its last
row, the `RUSTDOCFLAGS` one, marked "added in `US-0097`") therefore has no row for the
gate this packet actually added, and the owning low-level design does not describe a
check CI now enforces.

Also stale in the same document: `packaging.md:167-169` still reads "The embedder's guide
is named as **planned** work ... `US-0103` replaces the line with a local path". The line
*has* been replaced (`crates/vt/README.md`, Documentation section), so that paragraph now
describes a README that no longer exists.

### 8. The coverage grep is vacuous for two of its seven modules -- LOW (beyond the recorded gap)

The packet records the public-API coverage criterion as "weak as worded" and substitutes
the module-and-variant grep. Run independently, that grep passes: 7 public modules, 20
`VtEvent` variants, nothing missing. But:

- `guide` "passes" on `01-overview.md:22`, "every other design decision in this guide
  follows from" -- the English word, not the module.
- `intern` "passes" on its single occurrence, `02-embedding.md:24`, which is a bare list of
  module names. The guide teaches nothing about `intern`.

So the substitute check is itself partly vacuous. Not a blocker -- the recorded gap is in
the right spirit -- but the "every public module is named in a chapter" claim is worth
half of what it sounds like.

### 9. `guide.rs` is 50 lines, not 51 -- NIT

`wc -l crates/vt/src/guide.rs` is 50 and `git diff --stat` reports `50 ++`. The packet's
LOC table says "budgeted about 43 lines; **actual 51**". The chapter total (1 792) is
exactly right.

### 10. A hedge dropped between the intake and the published chapter -- NIT

`01-overview.md:66` states rio-vt's "Dependencies with the transport off" as "still 15".
`IN-0038.md:145` marks that cell "**not measured**; `default-features = false` still
leaves 15 unconditional". The published text presents as measured what the source record
marks as not measured. Everything else in that table matches `IN-0038.md:137-158` row for
row: licence, edition/MSRV, parser, 11+3 / 15 / 6 dependencies, PTY rows, event delivery,
13 / ~60 / 20 variants, 14 / 17 / 18 OSC numbers, extensibility, locks.

### 11. Chapter 3 says `Terminal` is not `Sync`; the API reference beside it says it is -- LOW/MEDIUM

Found incidentally -- chapter 3 is outside the chapters I was asked to audit, so treat
this as a sample, not a sweep of that chapter.

`crates/vt/docs/guide/03-threading.md:3-5`:

> `Terminal` is `Send` because everything in it is, and it is not `Sync` because `feed`
> takes `&mut self`.

Both halves of that sentence are wrong.

- `Terminal` **is** `Sync`. `target/doc/oneterm_vt/struct.Terminal.html`, "Auto Trait
  Implementations", lists `Freeze`, `RefUnwindSafe`, `Send`, **`Sync`**, `Unpin`,
  `UnsafeUnpin`, `UnwindSafe`. The guide therefore contradicts the API reference rendered
  from the same `cargo doc` run, on the same site, one click away.
- The stated reason is not how `Sync` works. `Sync` is about shared references crossing
  threads; a method taking `&mut self` has no bearing on it. `Terminal` is
  `struct Terminal { parser: Parser, state: State }`
  (`crates/vt/src/terminal/mod.rs`) with no `Cell`, no `RefCell` and no raw pointer, so
  the auto impl applies -- which is exactly the "no interior mutability" property the rest
  of the chapter (correctly) sells.

Harmless in practice, because an embedder still needs a lock to get `&mut`, which is the
chapter's real point. But it is a false statement about the public API in the document
whose whole premise is that it cannot say false things about the public API, and it is
falsifiable in one click.

### 12. Chapter 13 says the transport's threads are joined on drop; none of them is -- MEDIUM

`crates/vt/docs/guide/13-pty.md:117-119`:

> Internally it runs two threads on Windows (a pipe reader and a pipe writer) and one on
> Unix (a reaper that turns child exit into a pollable event). **All are joined on drop**
> and none of them calls into your code.

The thread inventory is right. The joining is not: **there is no `.join()` anywhere in
`crates/vt/src/pty/`** outside tests. Both spawn sites discard the handle:

- `crates/vt/src/pty/windows/pipe.rs:363-367`, `spawn_pipe_thread`, ends `.spawn(body).map(drop)`
  -- and its own rustdoc at `:355-356` says the opposite of the chapter in as many words:
  "A pipe thread that **cannot be joined**: it is parked in a blocking `ReadFile` or
  `WriteFile` and only returns when the pipe breaks."
- `crates/vt/src/pty/unix.rs:206-217`, `reap_in_background`, likewise `.spawn(...).map(drop)`.
  The comment below it (`unix.rs:219-226`) explains that the reaper *owns* the `Child` and
  outlives the drop on purpose, which is the design the chapter describes as the reverse.

This is in the same paragraph as the (correct and load-bearing) "**Dropping a pseudo-console
is an action, not a release** ... the drop therefore blocks", so a reader will reasonably
conclude that when `drop` returns, no thread of this crate is still running against their
memory. That is not true, and it is the kind of thing an embedder builds shutdown ordering
on. The honest sentence is the one `pipe.rs` already wrote.

### 13. The implementer wrote `harness.db` itself -- flagged, not judged

`harness.db` (repository root, not in the worktree) carries a `story` row
`('US-0103', 'oneterm-vt embedder guide rendered by rustdoc', '2026-09-15', 'normal',
 '.../packaging.md', '.../US-0103-embedder-guide.md', 'implemented', 1, 1, 0, 1, ...)`.
The schema shape is correct and the contents agree with the packet (unit / integration /
platform proof set, e2e clear). Recording it per instruction: the coordinator owns the DB.
I read it from a copy and wrote nothing.

## 4. Accuracy audit, chapters 1, 4, 5, 10, 12, 13

Everything below was checked against the code in this tree. Findings 1, 3, 4, 10, 11 and 12
above are the wrong claims; the rest of what follows is confirmed correct.

**Chapter 1.** Comparison table matches `IN-0038.md:137-158` on every row (see finding 10
for the one hedge). "Six leaf crates" is literally true:
`cargo tree -p oneterm-vt --no-default-features -e normal,build` is exactly
`bitflags, log, memchr, rustc-hash, unicode-segmentation, unicode-width`, no transitive
edge and no build-script edge -- so "no build script, no proc macro, no platform code"
holds. The `feed`/`snapshot_update` diagram matches the real signatures.

**Chapter 4.** All 20 `VtEvent` variants exist and all 20 are in the table; the enum is
`#[non_exhaustive]` and deliberately not `Clone`
(`crates/vt/src/events/vt_event.rs:74-76`, with the reason in its own rustdoc). Every
"fires when" cell agrees with the variant's rustdoc: `Repaint` at most once per batch and
appended last (`crates/vt/src/events/batch.rs:59,102,116`), `ScreenCleared` on `ED 2` /
`ED 3` / `RIS` and never `ED 0` / `ED 1`, `ClipboardStore` already base64-decoded and
UTF-8-checked with the failures counted in `malformed_sequences`, `Cwd` unresolved and
untrusted, `Progress` as `OSC 9;4`, `CursorStyleChanged` on `OSC 50` or `CSI SP q`. The 22
intra-doc link definitions at `04-events.md:154-175` all resolve (they are inside the 428
links the link check followed). `DECRPSS` in the `Reply` list (`04-events.md:27`) is the
reply-side name for `DCS $ q`; the engine's own spelling is `DECRQSS`
(`crates/vt/src/terminal/dispatch.rs:1304`) -- defensible, since `Reply` *is* the reply,
but the surrounding items (`DA1`, `DSR`, `DECRQM`) are all request names.

**Chapter 5.** `OscRoutes::BUILTIN` is exactly the eighteen numbers the chapter lists
(`crates/vt/src/terminal/osc.rs:81-83`). Four routes and their semantics match
`osc.rs:38-54`; typed-event-then-raw ordering for `BuiltinAndForward` is stated in
`osc.rs:42-44` and demonstrated by the chapter's own doctest at `05-osc.md:139-152`, which
passes. Default `Builtin` for an implemented number and `Drop` for everything else:
`osc.rs:158-166`. "Three bit tests": `osc.rs:88-93` plus `get`. Both debug assertions are
real and worded as the chapter says (`osc.rs:107-111` for `Builtin` on a number with no
handler, `osc.rs:152-156` for `large` on a `Drop`), including the route-first ordering.
"2048-entry bitmap ... sorted spill list": `osc.rs:26-30`, `overrides()`. "Not live, no
setter": `osc.rs:62-68`. `OSC 7` semantics match the `Cwd` rustdoc verbatim in substance.

**Chapter 10.** Every published ceiling is the real constant:
`OSC_INLINE = 2048`, `OSC_LARGE = 8 * 1024 * 1024` (`crates/vt/src/parser/osc.rs:11,15`),
`MAX_OSC_PARAMS = 16` (`osc.rs:19`), `MAX_PARAMS = 32`, `MAX_INTERMEDIATES = 2`
(`crates/vt/src/parser/params.rs:13,17`), `DCS_MAX_BYTES = 16 * 1024 * 1024`
(`crates/vt/src/parser/mod.rs:36`), `SCROLLBACK_MAX = 1_000_000`,
`DEFAULT_SCROLLBACK = 10_000` (`crates/vt/src/grid/mod.rs:38,40`), viewport clamp
1024 x 2048 via `Size::clamped` (`grid/mod.rs:34,36,108-111`), image `MAX_DIMENSION = 4096`
(`crates/vt/src/graphics/mod.rs:47`). All of the named ones are genuinely `pub` and
reachable (`parser` and `grid` are public modules). All nine `FeedStats` fields exist with
the meanings the chapter gives, including `grapheme_truncated` and `style_table_exhausted`
being reserved and always 0. The chapter's doctest asserting `unhandled_sequences == 1`
for `CSI ? 9999 z` and per-call reset passes.

**Chapter 12.** MSRV 1.96.0 and edition 2024 are the workspace values
(`Cargo.toml:32,36`), inherited by `crates/vt/Cargo.toml`. `publish = false` with the
owner-ruling comment. Feature table correct: `default = ["pty"]`;
`pty = ["dep:polling", "dep:windows-sys", "dep:libc"]`; `vt-paranoid` and `regex`
default-off. "`regex` ... and its three dependencies" is exact --
`cargo tree --features regex` adds `regex`, `aho-corasick`, `regex-automata`,
`regex-syntax`. `polling` really is the only public dependency and really is `pty`-only.
The `SearchPattern` wildcard-arm doctest compiles in both feature states (it is one of the
27). See finding 3 for the two errors in the `#[non_exhaustive]` list.

**Chapter 13.** Trait set and names are right: `EventedReadWrite`, `EventedPty`,
`OnResize`, `PTY_CHILD_EVENT_TOKEN = 1`, `PTY_READ_WRITE_TOKEN = 2`
(`crates/vt/src/pty/mod.rs:72,75,159-215`). `Options` fields match, including the
Windows-only `escape_args` and Unix-only `child_signal_mask` and the "this crate never
touches the calling process's environment" rule (`pty/mod.rs:113-133`). `WindowSize`
pixel fields "reach the child as the Unix pixel fields and are unused by ConPTY" is the
module's own wording (`pty/mod.rs:135-138`). Two Windows threads / one Unix reaper, joined
on drop, no callback into embedder code: consistent with `pty/windows.rs` and
`pty/unix.rs` -- but "**All are joined on drop**" is false, see finding 12.
"Deregister before dropping the poller" matches
`EventedReadWrite::deregister`'s own rustdoc. **The console-host section is exactly
right**: the loader prefers a `conpty.dll` next to the running executable and falls back to
the system host (`crates/vt/src/pty/windows/conpty.rs:106`), the bundled pair is
`conpty.dll` plus `x64\OpenConsole.exe` (`conpty.rs:57-58`), and the inbox host swallowing
Sixel is stated at `conpty.rs:88`. The blocking drop is real, with a two-second bound
(`crates/vt/src/pty/windows/child.rs:53,179-202`); the chapter says "a bounded grace
period" and does not name `CHILD_EXIT_GRACE`, which is correct for a guide since the
constant is private -- but the packet's chapter outline promises the name. `Shell::new`
taking both verbatim: `pty/mod.rs:97-104`. Platform-difference bullets match
`--diff-platforms`' six lines exactly.

## 5. Chapter 11's pending list

`11-conformance.md:102-106` is a paragraph opening with a bold
"**Pending, and not yet merged into this tree.**" It is unmistakable and it does not claim
either state; it says the table is current and expected to shrink. **Clearly marked: yes.**

All seven items are genuinely absent at this tree, checked in code, not taken on trust:

| Item | Verified absent |
| --- | --- |
| mouse `? 9` (X10) | not in `Mode::from_private` (`crates/vt/src/terminal/mode.rs:154-181`) |
| mouse `? 1015` (urxvt) | not in `from_private` |
| `DECSCNM` `? 5` | not in `from_private`; the string `DECSCNM` appears nowhere in `crates/vt/src` |
| `LS2` / `LS3` / `SS2` / `SS3` | no match in `crates/vt/src` except an unrelated comment at `input/key.rs:260` |
| `? 2027` wiring | `2027 => Mode::GraphemeClusters` exists (`mode.rs:177`) but is in the "recognised but inert" list, as the chapter says |
| `OSC 17` / `OSC 19` | absent from `OscRoutes::BUILTIN` (`terminal/osc.rs:81-83`) and from dispatch |
| `DA3` (`CSI = c`) | no `DA3` in `terminal/dispatch.rs`; the chapter's own doctest asserts it is counted unhandled and passes |

`harness.db` shows `US-0102` as `implemented` on some other branch; it is not in this tree,
which is exactly what the paragraph says.

## 6. Gates and records

| Check | Result |
| --- | --- |
| Citation grep over `crates/vt/docs/guide/**/*.md` (minus `https://github.com/`) | **0 lines** |
| Same grep over `crates/vt/src/guide.rs`, `README.md`, `CHANGELOG.md` | **0 lines** |
| Same grep over every `///` / `//!` line in `crates/vt/src` | **0 lines** |
| `python scripts/vt-public-api.py --check --no-doc` | "public API surface unchanged (public-api.windows.txt)" |
| `python scripts/vt-public-api.py --diff-platforms` | 6 lines, all inside `oneterm_vt::pty` |
| `python scripts/check-english.py` | passed, 867 files |
| `python scripts/check-doc-paths.py` | passed, 197 paths in 11 documents |
| `ls crates/vt/docs/guide/*.md \| wc -l` | 13 |
| `grep -c 'include_str!' crates/vt/src/guide.rs` | 13 |
| chapter line count | 1 792 exactly, per-file counts all match the Evidence section |
| coverage grep (packet's own, re-run) | 7 public modules, 20 `VtEvent` variants, nothing missing (but see finding 8) |
| `cargo package -p oneterm-vt --allow-dirty --list` | 13 `docs/guide/*.md` entries plus `src/guide.rs` |
| git dependency build from outside the repo | succeeds; the chapters travel |

The guide.rs header no longer carries a repository path -- `6d8333a6` is the fix, and the
widened grep now returns zero on it, so the implementer's report of the grep catching its
own header is corroborated by the commit.

**Public-API snapshot gap, confirmed as recorded.** `crates/vt/public-api.windows.txt`
contains zero occurrences of `guide` and no module lines at all -- every line names an
item. A public module carrying no item is therefore invisible to the gate, exactly as the
packet's Gaps section says. The gap is real, correctly diagnosed, and correctly left to
whoever owns the script.

**README.** `crates/vt/README.md` Documentation section replaced the "planned" line with a
real one, names the thirteen chapters, gives
`cargo doc -p oneterm-vt --no-deps --all-features --open`, links `docs/guide/` for the
Markdown view, names both render scripts and is explicit that there is no hosted copy.
Accurate.

**CHANGELOG.** `crates/vt/CHANGELOG.md` Added section gains a `guide` entry stating it adds
no item and no dependency and that every block is a doctest. Accurate.

**scripts/README.md** gains the `vt-docs.ps1` / `vt-docs.sh` row, including the ordering
constraint against `vt-public-api.py --no-doc`. Both scripts carry the same warning in
their own headers, and both print the index path.

**Harness snippet schema.** The packet's `HARNESS:STATUS` and `HARNESS:PROOF` blocks are
well-formed, single-marked (`Implemented`), and the proof block's four ticks plus the clear
E2E box match what was actually run. See finding 11 for the DB row.

## 7. `pwsh scripts/ci-local.ps1 -Full`

Run to completion on this tree, log kept privately at
`<scratchpad>\ci-local-full.log` (4 949 lines). Exit 0, final line:

```
ci-local: all checks passed.
```

Twenty-three steps, every one green, in this order: `cargo fmt --check`; both
`cargo clippy --workspace --all-targets -D warnings` passes (plain and
`terminal-diagnostics`); `cargo test --workspace`; `cargo test -p oneterm-vt` under
`vt-paranoid`, under `regex`, and `--no-default-features`; `cargo build -p oneterm-vt`
`--no-default-features --examples` and `--all-features --examples`;
`cargo tree -p oneterm-vt -e normal --no-default-features`;
`cargo run -p oneterm-vt --example headless`; `cargo doc -p oneterm-vt --no-deps` and the
same `--all-features`, both under `RUSTDOCFLAGS='-D warnings'`, both silent;
`vt-public-api.py --check --no-doc` ("unchanged") and `--diff-platforms` (6 pty lines);
`cargo package --list` into `verify-dependency-graph.py --package-list -`; **both**
rustdoc self-containment greps, including the new
`==> rustdoc self-containment (crates/vt/docs/guide)` this packet adds, silent;
`verify-dependency-graph.py`; `check-doc-paths.py` (197 paths / 11 documents);
`test_check_english.py`; `check-english.py` (867 files); `completion-catalog.py validate`;
`third-party-notices.py --check`; and, from `-Full`,
`cargo deny check licenses bans advisories` -> "advisories ok, bans ok, licenses ok".

`cargo run -p oneterm-vt --example headless` in that log printed the same four screen rows
the outsider walk-through produced from chapter 2's fragments, which is the cheapest
possible evidence that the chapter and the example have not diverged.

Separately, `bash scripts/vt-docs.sh` was run and exits 0, printing
`file:///d/TrungKFC-Research/.../target/doc/oneterm_vt/guide/index.html`. Nit, not a
finding: under Git Bash on Windows that is `pwd`'s `/d/...` spelling, so the `file://` URL
it emits is not one a Windows browser will open. The path exists, the script is a POSIX
script, and the PowerShell twin prints the correct Windows URL -- so the criterion holds;
it is only the Windows-under-Git-Bash combination that produces an unclickable line.

## What could not be verified

- **Unix.** Windows host only. Chapter 13's Unix half (`openpty`, `SignalMask`,
  `child_signal_mask`, the reaper thread, the `TIOCSWINSZ` pixel fields) was read against
  `crates/vt/src/pty/unix.rs` but never executed. `--diff-platforms` is the only evidence
  that the Unix snapshot is consistent.
- **The `regex`-feature block in chapter 7** is `ignore`d and was not compiled by me
  either. It is in the same unchecked class as finding 1; note that it also references a
  `term` binding the block never defines, which would not compile as written. Lower stakes
  than chapter 13 because the surrounding prose carries the API names.
- **Rendering in a real browser.** Deliberately replaced by the headless link walk.
  Typography, sidebar behaviour and rustdoc's client-side search were not looked at.
- **`alacritty_terminal` and `rio-vt` facts** in chapter 1 were checked against
  `IN-0038.md`'s parity inventory, not against those crates' own sources. The inventory is
  dated 2026-09-15 and names its versions; I did not fetch either crate.
- **`scripts/vt-docs.sh`** under a real POSIX shell. It was run under Git Bash on Windows
  (exit 0, path printed and present); an actual Linux or macOS run was not done.

## Commands

```powershell
git reset --hard docs/vt-embedder-guide          # 6d8333a6, base main af5df2e7
$env:CARGO_BUILD_JOBS = 3

pwsh -NoProfile -File scripts\vt-docs.ps1
python <scratchpad>\linkcheck.py .

cargo test -p oneterm-vt --doc
cargo test -p oneterm-vt --doc --all-features
cargo test -p oneterm-vt --doc --no-default-features
cargo test -p oneterm-vt --doc -- --list

cargo tree -p oneterm-vt --no-default-features -e normal,build
cargo tree -p oneterm-vt --no-default-features --features regex -e normal
cargo package -p oneterm-vt --allow-dirty --list

python scripts\vt-public-api.py --check --no-doc
python scripts\vt-public-api.py --diff-platforms
python scripts\check-english.py
python scripts\check-doc-paths.py
bash <scratchpad>\coverage.sh <worktree>

# out-of-tree consumer, scratchpad only, deleted afterwards
cargo init --name guide_consumer ; cargo build ; cargo run

pwsh -NoProfile -File scripts\ci-local.ps1 -Full
```

## Scripts

Written to the session scratchpad, not to the repository:

- `<scratchpad>\linkcheck.py` -- parses every `.html` under
  `target/doc/oneterm_vt/guide/`, extracts every `href`/`src`, resolves relative paths
  against the file system, checks in-page and cross-page fragments, and reports the
  chapter list and its order from `guide/index.html`.
- `<scratchpad>\coverage.sh` -- the packet's own public-module and `VtEvent`-variant
  coverage grep, run independently.
- `<scratchpad>\harness_read.py` -- dumps `harness.db`'s schema and the `US-0103` /
  `IN-0038` rows from a **copy**; the live database was never opened for writing.
- `<scratchpad>\consumer\` -- the outsider walk-through crate. Deleted.
- `<scratchpad>\ci-local-full.log` -- the full gate log.

---

# Final re-check at 20d5b205

Branch `docs/vt-embedder-guide` at `20d5b205`, base `main` at `5d6c63f3` (the merge of
`US-0102`), confirmed by `git merge-base`. Same host, same rules, `CARGO_BUILD_JOBS=3`.
Nothing pushed, nothing committed, nothing outside the repository and the scratchpad.

## Verdict: PASS-WITH-NOTES (two new findings; every earlier substantive finding closed)

All four false prose claims are fixed, the two outsider blockers are fixed, the `ignore`
blocks are gone in every feature state, chapter 11 is folded against `5d6c63f3`, and the
record edits landed. What is left is one more prose-vs-code mismatch in chapter 11
(finding 14, the inert-mode paragraph) and a set of stale counts in the packet's own
Evidence (finding 15). Neither blocks a merge; finding 14 is a two-sentence edit.

### All four false prose claims are fixed, and fixed properly

| Was | Now | How I checked |
| --- | --- | --- |
| chapter 13's poll loop did not compile | a ```rust,no_run``` block using `polling::Events` and `events.iter()`, compiled on every build that has `pty`; `guide.rs:79-81` gates `ch13_pty` on the feature and `:83-95` supplies a stub arm so the chapter keeps its sidebar slot under `--no-default-features` | it is one of the 35 doctests that pass under `--all-features` |
| "All are joined on drop" | "**None of them is joined, and drop does not wait for them**", with the Windows-parked-in-a-blocking-read reason, the Unix reaper's deliberate outliving, and the grace period correctly attributed to the *child* rather than the threads | zero `.join()` in `crates/vt/src/pty/` outside tests; `windows/pipe.rs:367`, `unix.rs:217`, `windows/child.rs:53` |
| "not `Sync` because `feed` takes `&mut self`" | "`Terminal` is both `Send` and `Sync`, and both are automatic ... no interior mutability anywhere in it, so the compiler grants both", followed by what each one actually buys | `target/doc/oneterm_vt/struct.Terminal.html` auto-trait list |
| a seven-item list mixing a struct in with the enums | "Eight public types are marked": **seven enums** (`VtEvent`, `OscRoute`, `Progress`, `ShellMark`, `input::KeySpec`, `input::NamedKey`, `search::SearchPattern`) and **one struct** (`search::SearchOptions`, with start-from-the-default guidance) | exact set-for-set match against `grep -A3 '#[non_exhaustive]' crates/vt/src` |

### Everything else on the list, confirmed

- **"You bring `polling`"** is a section of its own (`13-pty.md:49-68`) with the
  `polling = "3"` manifest block, the same-major reason, and an explicit note that
  `pub use polling;` is deliberately not offered. Zero `pub use polling` in
  `crates/vt/src`, so the text is accurate. My finding 2 is closed.
- **Chapter 2 fragment 8** ("Printing the screen", `02-embedding.md:174`) exists and is a
  live doctest. Finding 5 closed.
- **Chapter 7's regex tail** is `07-search-regex.md`, spliced by
  `#[cfg_attr(feature = "regex", doc = include_str!(...))]` with a prose stub on the other
  arm. Its block is live and defines its own `term`, which the old `ignore`d one did not.
- **Zero `ignore` blocks** in any chapter: the fence census is 28 ```rust```, 1
  ```rust,no_run```, 5 ```text```, 2 ```toml```. Doctests **34 / 35 / 35 / 32** for
  default / `--all-features` / `--features regex` / `--no-default-features`, **0 failed and
  0 ignored in all four**. 28 of the 34 are the guide, every chapter contributing at least
  one; chapter 7's two are attributed to `guide.rs` rather than to a `.md` path because
  they arrive through the `cfg_attr` splice, so the packet's
  `--list | grep -c 'docs/guide/'` criterion now reads 26 rather than 28. Still far past
  its threshold of 13, but the packet's wording will undercount from here on.
- **Chapter 11 folded against `5d6c63f3`**, pending paragraph gone. `? 5` with the
  screen-flag-never-a-cell-attribute text, `? 9`, `? 1015`, `? 2027` with the cross-feed
  carry, the 32-scalar bound, the once-per-cluster counter and the `VS16`-needs-a-base
  rule, the `GlyphWidth::WcsWidth` caveat, `DA3`, `LS2`/`LS3`/`SS2`/`SS3` with the
  "consumed by the next printed character and by nothing else" rule, and twenty built-in
  OSC numbers. The gaps table is `DECRQCRA`, `DECRQSS`, `XTGETTCAP` and the missing
  `? 2027` refusal flag, and the `esctest` section now explains mechanically why there is
  no score rather than declining to give one. Its doctest (DA1, DA2 and DA3 all answered;
  `DECRQCRA` counted unhandled and never answered) passes.
- **Chapter 6** carries both mouse tables -- mode to `MouseReporting` to what it sends, and
  mode to `MouseEncoding` to the report shape -- and the empty-`Vec` rule at `:114-121`,
  including the distinction that an empty answer is not the same as `None`.
  `MouseReporting::X10` and `MouseEncoding::Urxvt` exist at `snapshot/modes.rs:16,37`.
- **Chapter 10** documents `dropped_cluster_carries` twice, as a ceiling row (`:26`) and as
  a counter (`:103`); the field is real at `events/vt_event.rs:233`.
- **The coverage grep is no longer vacuous.** `guide` now matches `oneterm_vt::guide` at
  `01-overview.md:100` and `intern` has real content at `10-limits.md:47-61`
  (`oneterm_vt::intern`, the three tables it owns, `Terminal::interner`). Re-ran the
  packet's grep independently: 7 modules, 20 variants, nothing missing. Finding 8 closed.
- **`packaging.md`**: the checks table gains a row for the gate this packet actually adds
  (`:347`, "a second self-containment grep, over `crates/vt/docs/guide`", naming both CI
  homes and the `guide.rs` header it caught), and point 9 (`:167-172`) is rewritten in the
  past tense to describe the README that now exists. Findings 6 and 7 closed at the
  document level.

### Gates

Link check on the re-rendered guide: 14 pages, **432 links, 0 missing file targets**, 249
fragments checked, and `guide/index.html` links exactly 13 chapter modules in ascending
order. The same 14 unresolved fragments as the first pass, all of them rustdoc's own
`guide.rs.html#NN` source anchors, which rustdoc 1.96 materialises in JavaScript.
Citation greps: **0** over the chapters with the `ci.yml` pattern (`crates/` and `docs/`
included) and **0** over every `///` / `//!` line in `crates/vt/src`.
`vt-public-api.py --check` reports the surface unchanged; `--diff-platforms` is 6 lines,
all inside `oneterm_vt::pty`. `check-english.py` passes 873 files; `check-doc-paths.py`
passes 197 paths in 11 documents. The guide is now 2 024 lines over 14 files (13 chapters
plus the regex splice) and `guide.rs` is 95 lines, so the packet's LOC table needs a
refresh.

### Outsider walk, chapter 13 only

Consumer crate in the scratchpad, depending on `oneterm-vt` by git at
`branch = "docs/vt-embedder-guide"` plus `polling = "3"` exactly as the new section
instructs. **The chapter's loop compiled unmodified this time.** The only edits were ones
the chapter tells a reader to make: their own `Shell`, a bounded `wait` so the walk
terminates, and the real `feed` call the chapter leaves as a comment. Output:

```
child exited: Some(ExitStatus(ExitStatus(0)))
screen = ["hi"]
dropped
```

Exit 0. Pid-tracked: the runner pid was dead afterwards and no `cmd.exe` or
`OpenConsole.exe` with an `echo hi` command line survived. Consumer deleted.

### Ten factual sentences spot-checked, chapters 11 and 13

1. `? 5` reaches the embedder as `ModeSnapshot::reverse_video` -- `snapshot/modes.rs:70`. OK
2. `? 9` X10 mouse is recognised -- `terminal/mode.rs:186`, `9 => Mode::MouseX10`. OK
3. `? 1015` urxvt encoding is recognised -- `terminal/mode.rs:195`, `1015 => Mode::UrxvtMouse`. OK
4. the cluster carry is bounded at 32 scalars and counted once per cluster --
   `dispatch.rs:42` `CLUSTER_CARRY_MAX = 32`, and `:373-374`. OK
5. `DA3` is answered -- `dispatch.rs:1012`, "`DA3` (`CSI = c`) answers
   `DCS ! | <8 hex digits> ST`". OK
6. `LS2` is `ESC n`, `LS3` is `ESC o`, `SS2` is `ESC N`, `SS3` is `ESC O` --
   `dispatch.rs:1205-1209`. OK
7. twenty built-in OSC numbers including `17` and `19` -- `terminal/osc.rs:81-83`,
   `BUILTIN: [u32; 20]`, exactly the list the chapter prints. OK
8. `DECRQCRA` is absent -- zero matches anywhere in `crates/vt/src`, and the chapter's own
   doctest asserts it is counted unhandled. OK
9. `pty::GlyphWidth::WcsWidth` exists and is public -- `pty/mod.rs:83,86`. OK
10. chapter 13's console-host paragraph -- `windows/conpty.rs:106` prefers a `conpty.dll`
    beside the running executable and falls back to the system host, `:57-58` names
    `x64\OpenConsole.exe`, and `:88` states the inbox host loses Sixel. OK

### New finding -- 14. Chapter 11's "Recognised but inert" is wrong in both directions -- LOW/MEDIUM

`11-conformance.md:120-125` says:

> **One mode** is accepted so that a stream setting it is not noise, and does nothing:
> `? 9001`, win32 input mode ... and `DECRQM` answers "not supported".

`Mode::inert_state` (`crates/vt/src/terminal/mode.rs:225-241`) has **three** arms:

- `Mode::DecCoLm` (`? 3`) returns `ModeState::NotSupported`, with the comment "both `h` and
  `l` act, and the honest answer is still 'not supported', because the width never
  changes";
- `Mode::Win32Input` (`? 9001`) returns **`ModeState::Reset`**, not `NotSupported`
  (`ModeState` is `NotSupported = 0`, `Set = 1`, `Reset = 2`, at `mode.rs:282-289`), and
  the table's own rule at `:221-223` says why: an inert mode answers `NotSupported` only
  when it is not even stored, and `Reset` when it is stored and simply unread;
- `Mode::LineFeedNewLine` (ANSI `20`) returns `ModeState::Reset`, commented "LNM is tracked
  and read by nothing -- `LF` never implies `CR` here".

So the chapter undercounts the inert set by two, gives the wrong `DECRQM` answer for the
one mode it does name, and lists both of the modes it misses -- "`3` column mode" at `:59`
and "`20` newline mode" at `:65` -- in the **Supported** section. That last part is the one
thing the chapter's own opening paragraph promises never to do: "never claim a capability
that does not exist."

Not a gate failure and not a behaviour bug. It is the same class as findings 11 and 12 from
the first pass -- a prose claim about the API that no doctest can reach -- and it is the
only one I found this round. `? 3` is arguably defensible as "supported" because both `h`
and `l` are dispatched; `? 20` is not, and the inert count and the `? 9001` answer are
simply wrong against a table two files away.

### New finding -- 15. The packet's own Evidence numbers do not match the tree -- NIT (record accuracy)

Same class as first-pass finding 9, and none of it touches the guide itself:

- **Doctest attribution.** Packet Evidence (`US-0103-embedder-guide.md:370`): "Of the 34 in
  the default run, **30** come from the guide". Measured from
  `cargo test -p oneterm-vt --doc -- --list`: **28** -- 26 attributed to a
  `docs/guide/*.md` path and 2 to `guide.rs` (chapter 7's, which arrive through the
  `cfg_attr` splice rather than an `include_str!`).
- **Chapter 7's count.** The same sentence's per-chapter tally says `07 x3`. In the
  **default** run chapter 7 has **2**; the third is the regex block, which exists only
  under `--features regex` (35 - 34 = 1). The tally as printed also sums to 29, matching
  neither the 30 it claims nor the 28 measured.
- **LOC table, chapters.** `:318` says "actual 1 956"; `cat crates/vt/docs/guide/*.md |
  wc -l` is **2 024** across the 14 files.
- **LOC table, `guide.rs`.** `:319` says "actual 95", which is right, but the
  verification-response table at `:526` says "corrected again for the `cfg` split: **71**".
  The packet contradicts itself; `wc -l` is 95.

Nothing here is a claim a reader of the guide can trip over -- it is the packet's own
bookkeeping -- but the numbers are what a later reviewer will trust.

### Carried forward, unchanged

Findings 4, 9 and 10 of the first pass were nits and this round did not revisit them:
chapter 4's "four questions" arithmetic, the `guide.rs` line count in the packet's LOC
table (now 95 lines, so that table is stale regardless), and chapter 1's dropped "not
measured" hedge on rio-vt's dependency count. Finding 13, the implementer's own
`harness.db` write, still stands as a process note for the coordinator.

### `pwsh scripts/ci-local.ps1 -Full`

Exit 0, twenty-five steps, every one green; log kept privately at
`<scratchpad>\r2-ci-full.log`. Final line:

```
ci-local: all checks passed.
```

Both self-containment greps ran and were silent, including
`==> rustdoc self-containment (crates/vt/docs/guide)`, the gate this packet adds.
`cargo package -p oneterm-vt --list` piped into `verify-dependency-graph.py --package-list -`
passed, so the fourteen chapter files still travel inside the package and
`include_str!` will build for a consumer. `cargo deny check licenses bans advisories`:
"advisories ok, bans ok, licenses ok".
