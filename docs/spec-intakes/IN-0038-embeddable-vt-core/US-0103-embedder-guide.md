# Work: oneterm-vt embedder guide (HTML)

ID: US-0103
Intake: IN-0038
Created: 2026-09-15

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
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

`oneterm-vt` ships a twelve-chapter **embedder's guide** -- prose that teaches someone how to build a
terminal on top of the crate -- rendered to HTML by `cargo doc`, published by docs.rs alongside the
API reference, and reproducible locally with one command.

The guide is the answer to a question the API reference cannot answer: the reference says what
`OscRoutes::route` takes; the guide says why you would call it and what OneTerm does with it.
`US-0097` gives the crate a README (what it is, what it is not, twenty lines of quick start). This
packet gives it the document you read after you have decided to use it.

Source of truth is Markdown under `crates/vt/docs/guide/`, so `python scripts/check-english.py`
covers every word of it and a reader on GitHub sees the same chapters as a reader on docs.rs.

## Scope

- [ ] In scope: `crates/vt/docs/guide/*.md` (twelve chapters); `crates/vt/src/guide.rs` and its
  `pub mod guide` line in `lib.rs`; `scripts/vt-docs.ps1` and `scripts/vt-docs.sh`; the `cargo doc`
  step in `scripts/ci-local.sh`, `scripts/ci-local.ps1` and `.github/workflows/ci.yml`; one row in
  `scripts/README.md`; the "Documentation" section of the README that `US-0097` writes.
- [ ] Out of scope: the API reference itself -- `#![warn(missing_docs)]` and the per-item doc lines
  are `US-0097`'s, and this packet assumes they are done.
- [ ] Out of scope: publishing the guide to GitHub Pages. docs.rs renders it for free at the URL the
  README links. A `gh-pages` deployment is one workflow file and no source change; see
  **Follow-up** below.
- [ ] Out of scope: mdBook, or any second documentation toolchain. Evaluated and rejected below.
- [ ] Out of scope: translating the guide. English only, per `scripts/check-english.py`.
- [ ] Out of scope: a tutorial that builds a renderer. The crate has no renderer by decision (f);
  the guide stops at the snapshot.

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

Twelve files under `crates/vt/docs/guide/`. One line each is the chapter's job, not its table of
contents.

| # | File | Purpose |
| --- | --- | --- |
| 1 | `01-overview.md` | What the crate is, what is deliberately out (no renderer, no PTY, no policy -- decision f), and the comparison table against `alacritty_terminal` and `rio-vt` lifted from the intake's parity inventory, with the OSC-extensibility row as the headline. |
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
| 12 | `12-versioning.md` | Versioning, MSRV and the changelog policy: the semver promise from `api-surface.md`, the MSRV rule from `packaging.md`, and what an embedder should pin. |

Chapters 6, 7 and 11 are written last because their subject matter does not exist until their gating
packet lands. A chapter file is **not** created empty as a placeholder: twelve modules, twelve
files, or the packet is not done.

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

- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps --all-features` exits 0 and
  prints no warning.
- [ ] `target/doc/oneterm_vt/guide/index.html` exists and links to exactly twelve chapter modules.
- [ ] `ls crates/vt/docs/guide/*.md | wc -l` is 12, and
  `grep -c 'include_str!' crates/vt/src/guide.rs` is 12.
- [ ] `cargo test -p oneterm-vt --doc` is green, and **every chapter contributes at least one
  doctest**: `cargo test -p oneterm-vt --doc -- --list | grep -c 'docs/guide/'` is at least 12.
- [ ] Deleting one character from any chapter code block makes `cargo test -p oneterm-vt --doc`
  fail. Spot-checked on three chapters and recorded in Evidence; this is the same cannot-rot check
  `US-0097` applies to the README.
- [ ] docs.rs dry run: `RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo doc -p oneterm-vt --no-deps
  --all-features` exits 0, which is what `[package.metadata.docs.rs]` in `packaging.md` sets.
- [ ] Public-API coverage. If `US-0097` shipped `scripts/vt-public-api.py` and
  `crates/vt/public-api.txt`, a cross-check reads that file and asserts every entry is named in at
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
- [ ] `python scripts/check-english.py` passes: every chapter is ASCII English.
- [ ] No chapter links into `docs/spec-intakes/`, per `US-0097`'s self-containment rule. Measured:
  ```bash
  grep -rn 'docs/spec-intakes\|US-0[0-9]\{3\}\|DEC-0[0-9]\{3\}\|IN-0[0-9]\{3\}' \
    crates/vt/docs/guide/ | grep -v 'https://github.com/'
  ```
  returns 0 lines. An absolute link to the public repository is the allowed form.
- [ ] `scripts/vt-docs.sh` and `pwsh scripts/vt-docs.ps1` each print a path that exists.
- [ ] `cargo publish -p oneterm-vt --dry-run` still succeeds and its file list contains
  `docs/guide/` -- the chapters must be **inside** the published package, or `include_str!` fails to
  build on docs.rs.
- [ ] `pwsh scripts/ci-local.ps1` green.

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
- `US-0099`, `US-0100`, `US-0102` -- gating packets for chapters 6, 7 and 11.
- `scripts/README.md` -- every script and which CI runs it; `vt-docs` is a new row.
- `docs/agents/code-style.md` -- Rust conventions for `guide.rs`.

### Documentation Action

**Update required**, in three places outside the new files:

- `US-0097`'s README outline gains a **Documentation** section linking the guide's docs.rs URL and
  the local render script. Done in this packet's edit to `US-0097`.
- `packaging.md` gains a short **Guide** subsection pointing here, and a row in its
  "Effect on the repository's existing checks" table for the tightened `cargo doc` step.
- `scripts/README.md` gains the `vt-docs.ps1` / `vt-docs.sh` row.

No OneTerm-facing contract changes: the guide documents an existing API and adds no behaviour.

### Reconciliation

Before completion, confirm the three edits above landed, list the twelve chapter files, and confirm
that `US-0097`'s README "Documentation" section points at a URL that resolves.

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

Every one of those sources lives in `docs/spec-intakes/`, which a crates.io reader cannot open --
which is exactly why the guide has to restate them rather than link them.

## Plan

- [ ] `crates/vt/src/guide.rs` with twelve `#[doc = include_str!]` modules, and `pub mod guide;` in
  `lib.rs`. Write all twelve module stubs and all twelve files first, one heading each, so the build
  is green from the first commit and each chapter is then filled in isolation.
- [ ] Chapters 1, 2, 3, 4, 5, 8, 9, 10, 12 -- writable as soon as `US-0101` has fixed the API names.
- [ ] Chapters 6, 7, 11 -- after `US-0099`, `US-0100` and `US-0102` respectively.
- [ ] `scripts/vt-docs.sh` and `scripts/vt-docs.ps1`; one row in `scripts/README.md`.
- [ ] Tighten the existing `cargo doc` step in `scripts/ci-local.sh`, `scripts/ci-local.ps1` and
  `.github/workflows/ci.yml` with `RUSTDOCFLAGS="-D warnings"` and `--all-features`.
- [ ] Add the "Documentation" section to `US-0097`'s README outline, and the **Guide** subsection to
  `packaging.md`.
- [ ] Run the acceptance commands; paste the doctest count and the coverage-check output into
  Evidence.

## LOC budget

| Artefact | Budget |
| --- | --- |
| `crates/vt/docs/guide/*.md` | about 1 200 lines of Markdown across twelve chapters, average 100 |
| `crates/vt/src/guide.rs` | about 40 lines, all of it `#[doc = include_str!]` and module headers |
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
- Platform: `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps --all-features`; the same
  with `--cfg docsrs`; `cargo publish -p oneterm-vt --dry-run`; `pwsh scripts/ci-local.ps1`;
  `python scripts/check-english.py`.
- E2E: none. A guide has no runtime. The nearest thing is reading the rendered
  `target/doc/oneterm_vt/guide/index.html` in a browser and following every intra-doc link in
  chapter 4, which is manual and is recorded as such.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Record: the doctest list and count; the coverage-check output; the three deletion spot checks; the
`cargo publish --dry-run` file list showing `docs/guide/`; the rendered index path from each script.

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
- **GitHub Pages is not set up.** docs.rs is the only rendered URL until someone asks for another.

## Handoff

The packet splits cleanly along its gates: nine chapters are writable once `US-0101` lands, and
chapters 6, 7 and 11 wait for `US-0099`, `US-0100` and `US-0102`. A session that writes only the
scaffolding plus the nine unblocked chapters leaves the build green and the packet honestly
incomplete; the stop condition for "done" is the twelve-file and twelve-doctest count in Acceptance.
