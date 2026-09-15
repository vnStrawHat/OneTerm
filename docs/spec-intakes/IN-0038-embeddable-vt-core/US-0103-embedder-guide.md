# Work: oneterm-vt embedder guide (HTML)

ID: US-0103
Intake: IN-0038
Created: 2026-09-15

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

- Change type: new capability
- Risk lane: normal. The guide ships no runtime behaviour; its only teeth are that its code blocks
  are doctests, so a wrong sentence about the API fails the build rather than misleading a reader.
- Spec Intake: `IN-0038`

## Outcome

> **Owner ruling 2026-09-15.** `oneterm-vt` is not published to crates.io, so there is no docs.rs
> page to render the guide. Everything below stands except its rendering host: the chapters are
> rendered by `cargo doc -p oneterm-vt --no-deps --open`, which every consumer of the git dependency
> can run, and read as Markdown on GitHub. GitHub Pages is the obvious public host if one is ever
> wanted; nobody has asked. Where a criterion below names docs.rs or `cargo publish --dry-run`, read
> `cargo doc` and `cargo package -p oneterm-vt --list` instead.

`oneterm-vt` ships a thirteen-chapter **embedder's guide** -- prose that teaches someone how to build
a terminal on top of the crate -- rendered to HTML by `cargo doc` alongside the API reference, and
reproducible with one command.

The guide is the answer to a question the API reference cannot answer: the reference says what
`OscRoutes::route` takes; the guide says why you would call it and what OneTerm does with it.
`US-0097` gives the crate a README (what it is, what it is not, twenty lines of quick start). This
packet gives it the document you read after you have decided to use it.

Source of truth is Markdown under `crates/vt/docs/guide/`, so `python scripts/check-english.py`
covers every word of it and a reader on GitHub sees the same chapters as a reader of the rendered
HTML.

## Scope

- [x] In scope: `crates/vt/docs/guide/*.md` (thirteen chapters); `crates/vt/src/guide.rs` and its
  `pub mod guide` line in `lib.rs`; `scripts/vt-docs.ps1` and `scripts/vt-docs.sh`; the `cargo doc`
  step in `scripts/ci-local.sh`, `scripts/ci-local.ps1` and `.github/workflows/ci.yml`; one row in
  `scripts/README.md`; the "Documentation" section of the README that `US-0097` writes.
- [x] Out of scope: the API reference itself -- `#![warn(missing_docs)]` and the per-item doc lines
  are `US-0097`'s, and this packet assumes they are done.
- [x] Out of scope: publishing the guide to GitHub Pages. With no docs.rs page (owner ruling
  2026-09-15) that is the only candidate for a public rendered URL, and it is one workflow file and
  no source change; see **Follow-up** below. Until somebody asks, `cargo doc` and the Markdown on
  GitHub are the two ways to read it.
- [x] Out of scope: mdBook, or any second documentation toolchain. Evaluated and rejected below.
- [x] Out of scope: translating the guide. English only, per `scripts/check-english.py`.
- [x] Out of scope: a tutorial that builds a renderer. The crate has no renderer by decision (f);
  the guide stops at the snapshot. The **transport** is no longer out: the owner ruling of
  2026-09-15 reversed decision (f) for the PTY, so chapter 13 exists and is gated on `US-0104`.

## The rendering decision

**Chosen: rustdoc-hosted chapters.** Each chapter is an empty module carrying its Markdown:

```rust
// crates/vt/src/guide.rs
//! The embedder's guide: how to build a terminal on top of this crate.
//!
//! Each chapter below is one Markdown file under `crates/vt/docs/guide/`.

#[doc = include_str!("../docs/guide/01-overview.md")]
pub mod ch01_overview {}

#[doc = include_str!("../docs/guide/02-embedding.md")]
pub mod ch02_embedding {}
// ... ten more
```

This is the pattern `tokio` uses for its topic pages and `bevy` for its cheatbook-adjacent module
docs. What it buys, in order of importance:

1. **The code blocks are doctests.** `cargo test -p oneterm-vt --doc` compiles every snippet in
   every chapter against the real crate. A guide whose examples cannot rot is the whole reason to
   prefer this over a static site. The intake's own evidence section records `rio-vt`'s README
   calling an `EventListener` method the trait does not have, seven weeks after publication; that
   failure is structurally impossible here.
2. **Zero new tooling.** No binary to install, no second job in CI, no lockfile for a doc generator.
   `cargo doc` already runs in CI for `US-0097`'s `missing_docs` gate; this packet adds
   `--all-features` and `-D warnings` to the same invocation.
3. **One site, not two.** docs.rs renders the guide and the API reference in the same tree, with the
   same sidebar, the same search index and the same intra-doc links: a chapter can write a
   `[VtEvent::Cwd]` link and rustdoc resolves it to the actual item page. A separate mdBook site
   cannot link into rustdoc without hand-written, version-pinned URLs that break on every release.
4. **It is free for the embedder.** Someone who adds `oneterm-vt` to `Cargo.toml` and runs
   `cargo doc --open` gets the guide, offline, at the version they actually depend on.

Module names are `chNN_<slug>` because rustdoc sorts a module list alphabetically and there is no
way to impose a reading order otherwise. The numeric prefix is the cost of using rustdoc as a book;
it is visible in the URL (`.../oneterm_vt/guide/ch05_osc/index.html`) and that is accepted.

**Why not mdBook.** mdBook produces a nicer book -- a real table of contents, client-side search, a
version selector, print output -- and if the audience were non-Rust readers it would win. It was
rejected on three counts. It is an **extra binary** every contributor and the CI runner must install
and keep pinned, for a crate whose entire pitch is a six-line dependency tree. It is a **second
toolchain in CI**, a second cache, a second thing that breaks on a runner image bump. And it
produces a **second doc site that must be kept aligned with rustdoc** by hand: two sets of links,
two deployment targets, and -- the fatal one -- code blocks that mdBook can only test by
`mdbook test`, which needs the crate's rlib on a path it does not know about, so in practice book
snippets drift exactly the way `rio-vt`'s README did. The trigger to switch: **the guide needs its
own site with search and per-version navigation, or it acquires readers who are not Rust
programmers** (protocol implementers, packagers, or a C or Python binding's users). Either of those
makes mdBook's cost worth paying; neither is true at one crate with two known consumers. Revisit at
the second external embedder.

## Chapter outline

Thirteen files under `crates/vt/docs/guide/`. One line each is the chapter's job, not its table of
contents.

| # | File | Purpose |
| --- | --- | --- |
| 1 | `01-overview.md` | What the crate is, what is deliberately out (no renderer, no policy -- decision f), and the comparison table against `alacritty_terminal` and `rio-vt` lifted from the intake's parity inventory, with the OSC-extensibility row as the headline. Its "PTY in the core" row reads "yes, default-on feature, and the only one of the three where turning it off leaves a six-dependency crate" (`US-0104`). |
| 2 | `02-embedding.md` | Embedding in ten minutes: `Terminal::new`, `feed`, draining an `EventBatch`, taking a snapshot, `resize` -- the headless example walked line by line, ending with a working program. |
| 3 | `03-threading.md` | The threading and locking model: the engine is synchronous and has no interior mutability, **the embedder owns the lock**, nothing the engine does can call back into embedder code, and `FeedStats` is how you decide when to wake a renderer. |
| 4 | `04-events.md` | Every `VtEvent` variant: when it fires, what the embedder must do about it, which ones are queries that demand a reply and how to write that reply back to the PTY, and why an event may not outlive its batch. |
| 5 | `05-osc.md` | OSC routing and extension: `OscRoutes`, the four routes (`Builtin`, `BuiltinAndForward`, `Forward`, `Drop`), size ceilings and the `large` opt-in; worked twice, with OneTerm's OSC 20308 agent channel as the "add a number" case and the `OSC 9;7` alias as the "wrap a built-in" case. |
| 6 | `06-input.md` | Input encoding: keys and mouse to bytes, the Kitty keyboard protocol flags, `modifyOtherKeys`, and which mouse protocol and encoding the application selected. Gated on `US-0099`. |
| 7 | `07-search.md` | Searching the scrollback: `SearchPattern`, literal and whole-word without the `regex` feature, regex with it, incremental search across a live feed, and what a match means in `RowId` terms. Gated on `US-0100`. |
| 8 | `08-graphics.md` | Sixel: how an image is decoded, where it is placed relative to the grid, how placements move with scrollback and when they are evicted, and what the embedder has to draw. |
| 9 | `09-resize.md` | Resize and reflow: what reflow guarantees and what it does not, what happens to the cursor, the selection and image placements, and the cost model of a resize on a full history. |
| 10 | `10-limits.md` | Hostile input: every ceiling (OSC inline and large, parameter counts, history bounds), the counters that report what was dropped, and the rule that no byte sequence may panic the engine. |
| 11 | `11-conformance.md` | Conformance: what is supported, the frozen parity corpus, the known gaps, and the `esctest` pass/fail counts `US-0102` produces. Gated on `US-0102`. |
| 12 | `12-versioning.md` | Versioning, MSRV and the changelog policy: the semver promise from `api-surface.md`, the MSRV rule from `packaging.md`, and what an embedder should pin -- including the `polling` public-dependency clause. |
| 13 | `13-pty.md` | **Pty**: the default-on `pty` feature and why it ships on; the evented, runtime-free IO model and the loop the embedder writes (`Poller`, the two tokens, `feed` under the embedder's lock); the `EventedReadWrite` / `EventedPty` / `OnResize` contract, with the loopback implementation as the worked override; what `--no-default-features` gives instead (`feed` is the transport seam, and the traits are gone with the feature -- stated plainly, not implied); the blocking `Drop` and `CHILD_EXIT_GRACE`; and the Windows ConPTY host situation -- the crate ships no console host, so an embedder gets the inbox `conhost.exe`, which swallows Sixel, unless they bundle their own pair. Gated on `US-0104`. |

Chapters 6, 7, 11 and 13 are written last because their subject matter does not exist until their
gating packet lands. A chapter file is **not** created empty as a placeholder: thirteen modules,
thirteen files, or the packet is not done.

**Why 13 and not a renumber.** The PTY sits logically next to chapter 3 (threading), but rustdoc
orders the chapter list alphabetically by `chNN_` module name, so inserting it there renumbers ten
files and ten module names for a reading-order gain. It is appended instead, and that is acceptable
precisely **because** the feature is optional: chapters 1 to 12 are true at every feature setting,
and a reader who never enables `pty` never needs chapter 13.

## Local render, and CI

`scripts/vt-docs.ps1` and `scripts/vt-docs.sh`, about fifteen lines each, doing one thing:

```bash
#!/usr/bin/env bash
set -euo pipefail
export RUSTDOCFLAGS="-D warnings"
cargo doc -p oneterm-vt --no-deps --all-features
echo "file://$(pwd)/target/doc/oneterm_vt/guide/index.html"
```

CI runs the same `cargo doc` invocation with the same `RUSTDOCFLAGS`, added to
`scripts/ci-local.sh`, `scripts/ci-local.ps1` and `.github/workflows/ci.yml`. `US-0097` already adds
a `cargo doc -p oneterm-vt --no-deps --all-features` step for the `missing_docs` gate; this packet
**tightens that existing step** with `RUSTDOCFLAGS="-D warnings"` rather than adding a second one,
so there is one doc build in CI, not two.

**Follow-up, not this packet:** GitHub Pages. `actions/deploy-pages` over `target/doc` is one
workflow file, no source change, and buys a stable non-docs.rs URL. It is worth doing only if the
crate acquires readers who are not already `cargo doc` users.

## Acceptance

Each criterion below is a command a verifier who distrusts this packet can run.

- [x] `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps --all-features` exits 0 and
  prints no warning.
- [x] `target/doc/oneterm_vt/guide/index.html` exists and links to exactly thirteen chapter modules.
- [x] Thirteen chapters, in **fourteen** files: `ls crates/vt/docs/guide/*.md | wc -l` is 14 and
  `grep -c 'include_str!' crates/vt/src/guide.rs` is 14, because chapter 7's regex section is a
  file of its own, appended by `cfg_attr` only when the feature that gives it a subject is on.
  `target/doc/oneterm_vt/guide/index.html` still links exactly thirteen chapter modules, which is
  the number that matters to a reader.
- [x] `cargo test -p oneterm-vt --doc` is green, and **every chapter contributes at least one
  doctest**: `cargo test -p oneterm-vt --doc -- --list | grep -c 'docs/guide/'` is at least 13.
  **Built as the packet says, after a correction.** The first attempt left chapter 13 un-gated
  and made its poll loop ```rust,ignore```, on the reasoning that gating the module would hide a
  chapter from a `--no-default-features` reader. Independent verification found the cost of that:
  the one block rustdoc did **not** compile was the one an embedder copies, and it was wrong --
  `polling` 3.x takes `&mut Events`, not `&mut Vec<_>`. So `guide::ch13_pty` is now
  `#[cfg(feature = "pty")]` after all, the loop is ```rust,no_run``` (compiled on every build that
  has the feature, never executed), and the `not(feature = "pty")` arm keeps a short chapter 13 in
  the sidebar saying which feature it documents, so nothing is hidden. Measured: 30 guide doctests
  across 13 chapters with the feature on, 1 `ignore`d (chapter 7's `regex` block, whose variant
  does not exist in a default build), and 28 with it off.
- [x] Deleting one character from any chapter code block makes `cargo test -p oneterm-vt --doc`
  fail. Spot-checked on three chapters and recorded in Evidence; this is the same cannot-rot check
  `US-0097` applies to the README.
- [x] `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps --all-features` exits 0. There
  is no `[package.metadata.docs.rs]` table any more (owner ruling 2026-09-15), so this is the whole
  render check.
- [x] Public-API coverage. If `US-0097` shipped `scripts/vt-public-api.py` and
  the host's public-API snapshot (`public-api.windows.txt` or `public-api.unix.txt`,
  `US-0104`), a cross-check reads that file and asserts every entry is named in at
  least one chapter or in the API reference. Otherwise the same check by grep, run from the
  repository root:
  ```bash
  # every public module and every VtEvent variant is named somewhere in the guide
  for name in $(grep -o '^pub mod [a-z_]*' crates/vt/src/lib.rs | awk '{print $3}'); do
    grep -qr "\b$name\b" crates/vt/docs/guide/ || echo "MISSING: $name"
  done
  for name in $(sed -n '/^pub enum VtEvent/,/^}/p' crates/vt/src/event.rs \
                 | grep -o '^\s*[A-Z][A-Za-z]*' | tr -d ' '); do
    grep -qr "\b$name\b" crates/vt/docs/guide/ || echo "MISSING: $name"
  done
  ```
  Must print nothing. See the note under **Gaps** on why the criterion is worded this way.
- [x] `python scripts/check-english.py` passes: every chapter is ASCII English.
- [x] No chapter links into `docs/spec-intakes/`, per `US-0097`'s self-containment rule. Measured:
  ```bash
  grep -rn 'docs/spec-intakes\|US-0[0-9]\{3\}\|DEC-0[0-9]\{3\}\|IN-0[0-9]\{3\}' \
    crates/vt/docs/guide/ | grep -v 'https://github.com/'
  ```
  returns 0 lines. An absolute link to the public repository is the allowed form.
- [x] `scripts/vt-docs.sh` and `pwsh scripts/vt-docs.ps1` each print a path that exists.
- [x] `cargo package -p oneterm-vt --list` contains `docs/guide/` -- the chapters must be
  **inside** the package, or `include_str!` fails to build for anybody who depends on the crate by
  git (owner ruling 2026-09-15; this criterion said `cargo publish --dry-run` and docs.rs).
- [x] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0038-embeddable-vt-core/low-level-design/packaging.md` -- the manifest, the
  docs.rs metadata this packet's dry run exercises, the self-containment rule the chapters obey, and
  the "effect on existing checks" table this packet adds a row to.
- `docs/spec-intakes/IN-0038-embeddable-vt-core/low-level-design/api-surface.md` -- the semver
  promise chapter 12 restates, and `scripts/vt-public-api.py`, which the coverage check prefers.
- `docs/spec-intakes/IN-0038-embeddable-vt-core/low-level-design/osc-extension.md` -- the four routes
  and the two worked examples chapter 5 teaches.
- `docs/spec-intakes/IN-0038-embeddable-vt-core/high-level-design.md` -- the module layout chapter 1
  describes, and the doc-comment self-containment rule.
- `US-0097` -- the README this guide is linked from, and the `missing_docs` gate it depends on.
- `US-0099`, `US-0100`, `US-0102`, `US-0104` -- gating packets for chapters 6, 7, 11 and 13.
- `docs/spec-intakes/IN-0038-embeddable-vt-core/low-level-design/pty.md` -- the feature, the
  threading and IO model, the trait-gating decision and the missing-console-host consequence that
  chapter 13 teaches.
- `scripts/README.md` -- every script and which CI runs it; `vt-docs` is a new row.
- `docs/agents/code-style.md` -- Rust conventions for `guide.rs`.

### Documentation Action

**Update required**, in three places outside the new files:

- `US-0097`'s README outline gains a **Documentation** section naming the guide and
  the local render script. Done in this packet's edit to `US-0097`.
- `packaging.md` gains a short **Guide** subsection pointing here, and a row in its
  "Effect on the repository's existing checks" table for the tightened `cargo doc` step.
- `scripts/README.md` gains the `vt-docs.ps1` / `vt-docs.sh` row.

No OneTerm-facing contract changes: the guide documents an existing API and adds no behaviour.

### Reconciliation

Done. The README edit and the `scripts/README.md` row landed here; `packaging.md` needed **no
change** -- its **Guide** section already describes this packet's shape, and its
"Effect on the repository's existing checks" table already carries the
`RUSTDOCFLAGS="-D warnings" cargo doc` row marked "added in `US-0097`", which is the step that
covers the guide. Adding a second doc step would have been the duplication that row exists to
prevent. The thirteen chapter files are listed in Evidence, and the README's Documentation section
points at a `cargo doc` command rather than a URL, per the owner ruling.

## Context

The guide's raw material already exists and should be moved, not invented:

| Chapter | Where the material is today |
| --- | --- |
| 1 | `IN-0038.md`, "Parity inventory vs the two reference cores" |
| 3 | `IN-0029-vt-engine/low-level-design/events-and-api.md`, the "events are values, never callbacks" rule; `docs/terminal-backend.md` for how OneTerm itself holds the lock |
| 4 | the `VtEvent` enum's own rustdoc, after `US-0097` |
| 5 | `low-level-design/osc-extension.md`; `docs/osc-agent-status.md` for the 20308 wire spec |
| 9 | `IN-0029-vt-engine/low-level-design/damage-and-render-state.md` |
| 10 | the `OSC_INLINE = 2048` and `OSC_LARGE = 8 MiB` ceilings and the drop counters in `crates/vt/src/terminal/osc.rs` |
| 11 | `US-0102` Evidence; `IN-0029-vt-engine/low-level-design/testing-and-bench.md` for the corpus |
| 12 | `api-surface.md` and `packaging.md` |
| 13 | `low-level-design/pty.md`; the moved `crates/vt/src/pty/mod.rs` rustdoc; `crates/local-shell/src/event_loop.rs` as the worked loop; `DEC-0013` for the console-host situation and `DEC-0016` for the blocking drop |

Every one of those sources lives in `docs/spec-intakes/`, which a reader of the crate alone cannot
open --
which is exactly why the guide has to restate them rather than link them.

## Plan

- [x] `crates/vt/src/guide.rs` with thirteen `#[doc = include_str!]` modules, and `pub mod guide;` in
  `lib.rs`. Write all thirteen module stubs and all thirteen files first, one heading each, so the
  build is green from the first commit and each chapter is then filled in isolation.
- [x] Chapters 1, 2, 3, 4, 5, 8, 9, 10, 12 -- writable as soon as `US-0101` has fixed the API names.
- [x] Chapters 6, 7, 11, 13 -- after `US-0099`, `US-0100`, `US-0102` and `US-0104` respectively.
- [x] `scripts/vt-docs.sh` and `scripts/vt-docs.ps1`; one row in `scripts/README.md`.
- [x] ~~Tighten the existing `cargo doc` step~~ -- **not done, and not needed.** `US-0097` had
  already added `RUSTDOCFLAGS="-D warnings"` and `--all-features` to all three files; this branch
  changes no `cargo doc` invocation anywhere. What it does add to those three files is a **new**
  gate: a second self-containment grep over the guide chapters, which the old one could not see
  because it matches `///` and `//!` lines only. Recorded in `packaging.md`'s checks table.
- [x] Add the "Documentation" section to `US-0097`'s README outline, and the **Guide** subsection to
  `packaging.md`.
- [x] Run the acceptance commands; paste the doctest count and the coverage-check output into
  Evidence.

## LOC budget

| Artefact | Budget |
| --- | --- |
| `crates/vt/docs/guide/*.md` | budgeted about 1 300 lines across thirteen chapters; **actual 1 956**, average 150. Over by a third, and not padding: chapters 4 (twenty variants plus twenty-two intra-doc link definitions), 5 (four routes and two worked examples) and 13 (a full poll loop) each run to about 180. No acceptance criterion counts lines |
| `crates/vt/src/guide.rs` | budgeted about 43 lines; **actual 95** -- 50 before verification rework, then the two feature splits (chapter 13's `cfg` arms, chapter 7's `cfg_attr` tail) and the prose each off-state arm puts in place of the section it replaces |
| `scripts/vt-docs.sh`, `scripts/vt-docs.ps1` | about 15 lines each |
| CI and README edits | under 20 lines total |

No production Rust. No new dependency, default or optional.

## Decisions

No decision record of its own. The rendering choice is recorded in **The rendering decision** above
with its rejected alternative and its revisit trigger; it binds this packet only, and a future
switch to a static site is a new packet, not an amendment to this one.

## Verification Plan

- Focused: `cargo test -p oneterm-vt --doc`, plus the three character-deletion spot checks.
- Unit: `cargo test -p oneterm-vt` -- unchanged, and must stay so; this packet adds no runtime code.
- Integration: `cargo test --workspace`.
- Platform: `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps --all-features`;
  `cargo package -p oneterm-vt --list`; `pwsh scripts/ci-local.ps1`;
  `python scripts/check-english.py`.
- E2E: none. A guide has no runtime. The nearest thing is reading the rendered
  `target/doc/oneterm_vt/guide/index.html` in a browser and following every intra-doc link in
  chapter 4, which is manual and is recorded as such.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Run on `x86_64-pc-windows-msvc`, branch `docs/vt-embedder-guide` off `main` at `af5df2e7`.

**Chapters.** Thirteen chapters under `crates/vt/docs/guide/`, 2024 lines of Markdown after the
verification rework: `01-overview.md` 105, `02-embedding.md` 214, `03-threading.md` 132,
`04-events.md` 184, `05-osc.md` 186, `06-input.md` 177, `07-search.md` 93 plus
`07-search-regex.md` 44, `08-graphics.md` 136, `09-resize.md` 100, `10-limits.md` 141,
`11-conformance.md` 160, `12-versioning.md` 132,
`13-pty.md` 220. Chapters 1, 4, 5, 6, 10 and 11 were rewritten against the
conformance work after the rebase; see **Rebased onto the closed gaps** below. `grep -c 'include_str!' crates/vt/src/guide.rs` is 13.

**Render.** `RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps` and the same with
`--all-features` both exit 0 with no warning. `target/doc/oneterm_vt/guide/index.html` links
exactly thirteen `chNN_*/index.html` modules.

**Doctests.** **There is no `ignore` block left in the guide.** Measured in four feature states,
all green and all with 0 ignored: default 34 passed, `--all-features` 35, `--features regex` 35,
`--no-default-features` 32, where chapter 13's two blocks and chapter 7's regex block are compiled
out with their features. Of the 34 in the default run, 30 come from the guide, at least one from
every chapter: 01 x1, 02 x8, 03 x1, 04 x2, 05 x4, 06 x2, 07 x3, 08 x1, 09 x1, 10 x1, 11 x1,
12 x2, 13 x2. Chapter 13's poll loop is `no_run` -- compiled, never executed, because it spawns a
child and then loops forever; every other block runs.

**Cannot rot.** One character deleted from a code block in `04-events.md`, `07-search.md` and
`10-limits.md` in turn; `cargo test -p oneterm-vt --doc` exited 101 each time and passed again
after the file was restored.

**Public-module and event coverage.** The packet's grep, with two corrections. Its second loop
is re-pointed at `crates/vt/src/events/vt_event.rs` (the packet names `crates/vt/src/event.rs`,
which does not exist; the module is `event` and its files live in `events/`). And a module now
counts as named only in its module-path form or as a backticked identifier --
`grep -qrE "oneterm_vt::$name\b|\`$name\`|\`$name::"` -- never as the bare English word, because
verification found the first version passing for `guide` on the sentence "every other design
decision in this guide follows from" and for `intern` on a bare list of module names. Chapter 1
now names `oneterm_vt::guide` and chapter 10 has a section on `oneterm_vt::intern`. Under the
stricter grep: 7 public modules and 20 `VtEvent` variants, nothing missing.

**`#[non_exhaustive]` inventory**, regenerated rather than recalled, which is how chapter 12's list
was fixed:
```
$ grep -rn -A3 '#\[non_exhaustive\]' crates/vt/src --include='*.rs' | grep -E 'pub (enum|struct)'
events/vt_event.rs-37-pub enum Progress {
events/vt_event.rs-55-pub enum ShellMark {
events/vt_event.rs-76-pub enum VtEvent {
input/key.rs-31-pub enum NamedKey {
input/key.rs-116-pub enum KeySpec {
search/mod.rs-57-pub struct SearchOptions {
search/mod.rs-89-pub enum SearchPattern<'a> {
terminal/osc.rs-38-pub enum OscRoute {
```
Eight types: seven enums that need a wildcard arm, and one **struct** that needs
`SearchOptions::default()` plus assignment instead. The chapter had omitted `KeySpec` and listed
`SearchOptions` among the enums.

**Self-containment.** The widened citation grep over `crates/vt/docs/guide --include='*.md'`,
excluding `https://github.com/`, returns 0 lines.

**Package.** `cargo package -p oneterm-vt --allow-dirty --list` carries 13 `docs/guide/*.md`
entries, and piped into `verify-dependency-graph.py --package-list -` it passes: the package
carries `CHANGELOG.md`, `LICENSE`, `NOTICE`, `README.md`, `examples/headless.rs` and reaches
nothing outside `crates/vt`.

**Public API.** `python scripts/vt-public-api.py --check --no-doc` reports the surface
**unchanged**, and neither snapshot needed an edit. That is not an oversight: the script lists one
line per rustdoc *item* page, and `guide` and its thirteen chapter modules contain no item at all,
so a module with no items adds no line. `--diff-platforms` still reports its six `oneterm_vt::pty`
lines and nothing outside them. Carried forward as a gap below.

**Scripts.** `pwsh scripts/vt-docs.ps1` printed
`file:///D:/.../target/doc/oneterm_vt/guide/index.html` and `scripts/vt-docs.sh` printed the same
path in its shell's spelling; both exist.

**Gate.** `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test -p oneterm-vt`, `python scripts/check-english.py` (867 files),
`python scripts/check-doc-paths.py` (197 paths in 11 documents) and `pwsh scripts/ci-local.ps1
-Full` all pass; the `ci-local` line is in the report.

Known gaps to carry forward:

- **The public-API coverage criterion is weak as worded.** "Every public item is mentioned in at
  least one chapter **or the API reference**" is satisfied by every item automatically, because
  `#![warn(missing_docs)]` already puts every public item in the API reference. The check written
  into Acceptance is therefore the stronger, non-vacuous version -- every public **module** and
  every `VtEvent` **variant** is named in a chapter -- which is measurable and is what a reader
  actually needs. The original wording is recorded here so the difference is deliberate and visible.
- **Chapter 2 duplicates `examples/headless.rs`.** `US-0097` owns the example; chapter 2 walks
  through it. Two copies of the same twenty lines is the drift the intake criticises `rio-vt` for.
  Both copies are compiled -- the example by `cargo build --examples`, the chapter by the doctest --
  so neither can rot silently, but they can disagree with each other. The cheap guard, and what this
  packet does: chapter 2 quotes the example in numbered fragments rather than as one block, so a
  divergence is visible rather than plausible. A generator that splices the file into the chapter
  was considered and rejected as more machinery than the problem.
- **Hidden doctest lines are visible on GitHub.** rustdoc hides a code line that starts with a hash
  and a space; GitHub's Markdown renderer does not. Chapters are read in both places. Rule: use
  hidden lines only for `use` statements and `fn main`, never for anything a reader needs, so the
  GitHub view is noisier but never wrong.
- **Chapter ordering costs a `chNN_` prefix in every URL.** That is the price of using rustdoc as a
  book. If it becomes intolerable, that is the mdBook trigger.
- **The LOC budget is optimistic.** Chapters 4 (about twenty `VtEvent` variants) and 5 (four routes,
  two worked examples) will each exceed 100 lines; the 1 200 total holds only if chapters 1, 3, 9
  and 12 come in short. Treat 1 200 as a target, not a gate; no acceptance criterion counts lines.
- **`RUSTDOCFLAGS` invalidates the shared doc cache.** Setting it changes the fingerprint, so the
  first `cargo doc` after a plain build rebuilds. Costs CI seconds, not minutes; recorded so it is
  not mistaken for a broken cache.
- **GitHub Pages is not set up.** With no docs.rs page (owner ruling 2026-09-15) there is no
  public rendered URL at all: a reader runs `cargo doc` or reads the Markdown on GitHub.
- **A new public module is invisible to the public-API snapshot.** `scripts/vt-public-api.py` reads
  one line per rustdoc *item* page and walks module links only to find which modules are public, so
  `pub mod guide` plus thirteen empty chapter modules changed neither snapshot. The gate is
  therefore not the review line it claims to be for a module that carries no item. It is left alone
  here rather than widened: adding module lines rewrites both snapshots for every module in the
  crate, which is a change to a shared contract file and belongs to whoever owns that script, not
  to a documentation packet. Anybody adding a public module should know the snapshot will not
  notice.
- **Chapter 11 is written against this tree and has a pending list.** The seven conformance gaps
  it tabulates -- mouse `? 9` and `? 1015`, `DECSCNM`, `LS2` / `LS3` / `SS2` / `SS3`, `? 2027`
  wiring, `OSC 17` / `OSC 19`, `DA3` -- are still open at `af5df2e7`, and `US-0102` closes them but
  is not merged. The chapter names the pending state in a marked paragraph rather than claiming
  either state; when `US-0102` merges, the gaps table shrinks and that paragraph goes. That edit is
  the one thing in this packet that is known to be needed and not yet done.

## Rebased onto the closed gaps

Chapter 11 was written against a tree in which seven conformance gaps were open, and carried a
marked paragraph saying a separate piece of work would close them. That work merged; this branch is
rebased onto it and the paragraph is gone. What changed in the guide:

- **Chapter 11** now lists `? 5`, `? 9`, `? 1015` and `? 2027` among the supported modes, `DA3`
  among the supported `CSI` sequences, `LS2` / `LS3` / `SS2` / `SS3` among the supported `ESC`
  sequences with the note that `DECSC` / `DECRC` save the invoked set and any pending single shift,
  and `OSC 17` / `19` among the twenty built-in numbers. "Recognised but inert" is down from two
  modes to one, `? 9001`. `? 2027` gets three paragraphs of its own, because it is the one closed
  gap with behaviour an embedder has to understand rather than merely have: the cross-chunk carry,
  its 32-scalar bound and the counter that reports it, and the rule that a presentation selector
  needs a base. The `WcsWidth` ConPTY caveat is stated where a Windows embedder will meet it.
- **The known-gaps table is four rows, all new**: `DECRQCRA`, which is also why there is still no
  `esctest` score -- the harness reads the screen back with rectangle checksums, so it cannot run
  at all -- plus `DECRQSS` and `XTGETTCAP` unanswered, and the absent `Config` flag that would let
  an embedder refuse `? 2027` on a `WcsWidth` session.
- **Chapter 11's doctest** asserted `DA3` was counted unhandled, which is now false. It asserts
  that `DA1`, `DA2` and `DA3` are all answered, and uses `DECRQCRA` as its example of a gap that is
  counted rather than guessed at.
- **Chapter 6** gains two tables -- the four reporting modes and the four encodings -- the rule
  that an encoder returns an empty `Vec` for an event the mode does not report, the two
  reference asymmetries (`? 9` outranks the extended encodings; setting one reporting mode clears
  the other three, unsetting clears only itself), and the reason to keep checking
  `mouse_reporting()` anyway: an empty answer means the event is yours, not that there is nothing
  to do.
- **Chapter 10** gains `FeedStats::dropped_cluster_carries` and the 32-scalar carry bound, with the
  once-per-cluster counting rule stated twice because it is the one way to misread the number.
- **Chapters 1, 4 and 5** move from eighteen built-in OSC numbers to twenty, and chapter 4's
  `ColorQuery` row and prose gain `OSC 17` / `19` -- a highlight colour the engine never draws,
  which is exactly why the query comes to you.

No new `VtEvent` variant arrived, so chapter 4's table is still the whole enum at twenty variants;
the addition was one `FeedStats` field. The public-API snapshots came over in the rebase unchanged
by this branch, and `--check` and `--diff-platforms` both still pass.

## Verification notes closed

Independent verification of `6d8333a6` returned **PASS-WITH-NOTES**: every gate green, every
measurable claim true, and four prose sentences about the API false. The full report is at
[`evidence/US-0103-verify.md`](evidence/US-0103-verify.md). What each note cost and what closed it:

| Note | Closed by |
| --- | --- |
| 1. Chapter 13's poll loop does not compile against `polling` 3.x (`&mut Events`, `events.iter()`) | The loop is now compiled, not `ignore`d. `guide::ch13_pty` is `#[cfg(feature = "pty")]` so the block may name the transport, the block is `no_run` so it compiles without spawning a child, and the `not(feature = "pty")` arm keeps a short chapter 13 in the sidebar of a build that has no transport. The two lines were wrong exactly as reported. |
| 2. Chapter 13 never says to depend on `polling` | A new section, "You bring `polling`", with the manifest line, why the major must match, and why a `pub use polling;` is deliberately not offered -- it would be a permanent public item in this crate carrying a semver promise about somebody else's. |
| 3. Chapter 12's `#[non_exhaustive]` list omits `input::KeySpec` and miscategorises `SearchOptions` | The list is regenerated from a grep (quoted in Evidence above) and split: seven enums that need a wildcard arm, one struct that needs `default()` plus assignment, with a doctest of that shape. |
| 4. Chapter 4's "four questions ... three of them" matches nothing in its own section | Rewritten to the two the engine cannot answer and the one it already has, with a cross-reference to where the bytes go. |
| 5. Chapter 2 never shows the print loop it says the program has | Fragment 8 added, with the `row_text` call site, and the closing sentence now says eight fragments. The related gap -- chapter 4 saying "write them to the process input" without naming `EventedReadWrite::writer` -- is closed by the same cross-reference. |
| 6. A Plan tick claims a `cargo doc` tightening this branch did not do | Unticked and replaced with what the branch did add to those three files. |
| 7. The one genuinely new CI gate is undocumented in its owning design | `packaging.md`'s checks table gains a row for the guide self-containment grep, and its point 9 no longer describes the guide as planned work. |
| 8. The coverage grep is vacuous for `guide` and `intern` | The grep now matches a module path or a backticked identifier, never the English word. Chapter 1 names `oneterm_vt::guide`; chapter 10 gains a section on `oneterm_vt::intern`. |
| 9. `guide.rs` is 50 lines, not 51 | LOC table corrected, and corrected again for the `cfg` split: 71. |
| 10. Chapter 1 drops the intake's "not measured" hedge on `rio-vt`'s dependency count | Hedge restored. |
| 11. Chapter 3 says `Terminal` is not `Sync`; the API reference says it is | Rewritten. `Terminal` is `Send` **and** `Sync`, both automatic; the chapter now says what each buys, and that `&mut self` on `feed` is what serialises mutation -- not a missing `Sync`. |
| 13. Chapter 7's `regex` block was the last thing in the guide that nothing compiled | The same split one level down: `07-search-regex.md` is appended by `cfg_attr` under `feature = "regex"`, so the block is a live doctest where the variant exists. The chapter itself is **not** gated -- `regex` is off by default, so stubbing chapter 7 in the common build would be the cure killing the patient -- and the `not(feature)` arm replaces only the tail section. The guide now has zero `ignore` blocks in every feature state. Raised after the report, in the same spirit as note 1. |
| 12. Chapter 13 says the transport's threads are "all joined on drop"; none is joined | Rewritten to the real lifecycle: the Windows pipe threads are parked in a blocking read or write and cannot be joined, the Unix reaper deliberately outlives the drop, none of them holds anything of yours, and the bounded wait in `drop` is for the **child**, not for them. |

Two notes are recorded rather than closed. The verifier's own "could not be verified" list stands
as written -- no Unix host, no real browser, and the two reference crates checked against this
intake's parity inventory rather than their own sources. And the honest scope of the design is now
on the record: **the doctests cover the code blocks, not the prose.** Eleven of the twelve notes
above are sentences the build could not have caught, and four of them were false. A guide whose
examples cannot rot is still a guide whose paragraphs can.

## Handoff

The packet splits cleanly along its gates: nine chapters are writable once `US-0101` lands, and
chapters 6, 7, 11 and 13 wait for `US-0099`, `US-0100`, `US-0102` and `US-0104`. A session that
writes only the scaffolding plus the nine unblocked chapters leaves the build green and the packet
honestly incomplete; the stop condition for "done" is the thirteen-file and thirteen-doctest count
in Acceptance.
