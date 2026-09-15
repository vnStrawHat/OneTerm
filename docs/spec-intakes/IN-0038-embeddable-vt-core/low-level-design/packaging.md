# Low-Level Design: Packaging the crate other projects depend on

Intake: IN-0038
HLD: [`../high-level-design.md`](../high-level-design.md)
Topic: packaging
Date: 2026-09-15

## Concern

Everything that has to be true for `crates/vt` to be a crate someone else can depend on: a
README, an example, a CHANGELOG, the licence text inside the package, `#![warn(missing_docs)]`, an
MSRV, and the effect on the four repository scripts and `cargo-deny` that police the workspace
today.

**Owner ruling 2026-09-15: `oneterm-vt` is not published to crates.io.** Other projects consume it
as a git dependency on the OneTerm repository. That changes where the crate is fetched from and
nothing about what it has to contain: a consumer who writes a `git = ...` line still gets only the
files `cargo package` would have shipped, still reads the README as their front page, still needs
the licence text, and still cannot open a `docs/spec-intakes/` path. Every requirement below
survives the ruling; the registry-specific mechanics (`publish = true`, docs.rs metadata,
`cargo publish --dry-run`, the name reservation) do not, and each is marked where it changed.

## Manifest

As shipped by `US-0097`:

```toml
[package]
name = "oneterm-vt"
description = "An embeddable terminal core: VT parser, grid with scrollback, reflow, selection, \
damage-tracked snapshots and Sixel. No rendering, no PTY, no policy."
keywords = ["terminal", "vt100", "ansi", "emulator", "tui"]
categories = ["command-line-interface", "parser-implementations", "emulators"]
readme = "README.md"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true
# Owner ruling 2026-09-15: not published to crates.io. Other projects depend on
# it by git for now; everything above is kept so the flip is one line here.
publish = false

[features]
vt-paranoid = []
```

**`publish = false`, explicitly (owner ruling 2026-09-15).** The crate inherits `publish = false`
from the workspace anyway; the line is written out with its reason because the whole point of this
document is that somebody will ask why, and because the flip is then a one-line edit at a place a
reader has already been sent. Everything above it stays: `description`, `keywords`, `categories`,
`readme`, `license`, `repository` and `rust-version` cost nothing while unpublished, are all
correct today, and are exactly the fields that would otherwise have to be rewritten under time
pressure on the day the answer changes. `[package.metadata.docs.rs]` is the one key that went, and
only because it configures a builder that will never run: it says nothing true about the crate.

`keywords` and `categories` are capped at five each by crates.io, which is where they would go if
the ruling is ever reversed; both lists above are within it. Unpublished they are documentation, and
`categories` in particular is a compact statement of what the crate claims to be.
The `description` is what appears in search results, so it leads with what the crate is and ends
with what it is not -- the three exclusions are the crate's selling point against `rio-vt`, which
ships a PTY by default. It describes the crate **as it is today**, not as it will be after
`US-0098` and `US-0099`: key and mouse encoding and the OSC routing table are not in it yet, and
the sentence gains them in the packet that lands them.

**Two features this document proposed are not there.** There is no `serde` feature: Open Decision 3
is ruled against it, so `Serialize` / `Deserialize` on the plain-data types is not part of the
crate. And the `regex` optional dependency belongs to
[`US-0100`](../US-0100-search-moves-in.md), the packet that adds `SearchPattern::Regex` --
declaring it here would be a feature with no item behind it and a dependency nothing reaches.
`US-0100` adds the one line it needs:

```toml
[features]
regex = ["dep:regex"]
vt-paranoid = []
```

There is therefore no `default = []` line either: an absent `[features] default` **is** the empty
default set, so `cargo build --no-default-features` and a plain `cargo build` are the same
configuration today. That is why `US-0097`'s CI builds two configurations rather than three.

**Version inheritance.** The crate keeps `version.workspace = true`. That means an app patch release
moves the engine's version with it, unchanged, and it means
`python scripts/verify-dependency-graph.py` keeps passing -- that script asserts every workspace package's version equals `[workspace.package]
version` and would fail on an independent number. It also means the tag a consumer pins names a
version the crate itself reports. An independent version was considered and
rejected: it buys tidier release notes and costs a second version source, a script change, and a
new way for the two to drift. `rio-vt` inherits its workspace version for the same reason.

**Nothing in the workspace is publishable (owner ruling 2026-09-15).** `US-0097` adds the CI
assertion, and the ruling inverted it: `scripts/verify-dependency-graph.py` reads `cargo metadata`
and fails if **any** package is publishable, rather than requiring exactly one. A stray
`publish = true` on `oneterm-app` or `oneterm-ssh` was always a genuine accident; now so is one on
`oneterm-vt`. The script carries the flip path in a comment, so the day the owner decides to
publish, the assertion and the manifest change together and a reviewer sees both.

**The crate must stay a leaf.** The same script now also asserts that `oneterm-vt` depends on no
other OneTerm crate. Under `publish = true` that property was what made the crate publishable at
all; under a git dependency it is what makes the crate usable, because a consumer resolves only
what the manifest names. It is rule R7 either way, and it is now checked either way.

## README

`crates/vt/README.md`, about 140 lines, in this order:

1. One-line description of what the crate is. **No crates.io or docs.rs badges** (owner ruling
   2026-09-15): both would link a page that does not exist, which is the same dead-claim failure as
   the guide link in point 9.
2. **Install**, first because it is the question the ruling changed. The crate is not on crates.io,
   so the block is a git dependency on the OneTerm repository with a `tag`, and the paragraph under
   it says to pin a `tag` or a `rev` rather than a branch. A reader who copies `oneterm-vt = "0.5"`
   out of habit gets a resolver error and no explanation, so the README answers before they try.
   The same paragraph says the semver promise and the CHANGELOG apply to tags exactly as they would
   to releases.
3. **What it is not**: no renderer, no PTY, no window, no clipboard backend, no policy. Say it
   early, before anyone reads far enough to be disappointed.
4. The dependency tree, pasted from `cargo tree -p oneterm-vt -e normal`. Six lines. This is the
   crate's strongest claim and it should be visible without scrolling twice.
5. Quick start: a short snippet -- `Terminal::new`, `feed`, drain the batch, pull a snapshot --
   with `examples/headless.rs` linked below it for the longer version. It is **not** a copy of the
   example's body, as this document first proposed; instead `crates/vt/src/lib.rs` pulls the whole
   README into the crate behind

   ```rust
   #[cfg(doctest)]
   #[doc = include_str!("../README.md")]
   pub struct ReadmeDoctests;
   ```

   so every Rust block in the file is a doctest of the file itself and `cargo test --doc` compiles
   and runs all of them. That is the anti-drift rule below, reached by a stronger route: the README
   cannot contain a Rust block that nothing compiles, whether or not it came from the example.
6. OSC routing: today's `OscClaims` -- `claim`, `claim_large`, `NATIVE` -- as a compiled block, and
   a sentence saying a new OSC number costs one call plus a `match` arm. `US-0098` rewrites this
   section around the `OscRoute` table when it replaces `OscClaims`.
7. **Identity**: `Config::product_name`, the one field an embedder is expected to set, with what
   `XTVERSION` and `DA2` answer when it is left alone.
8. Feature table: `vt-paranoid` only, default-off, with what it adds and why it is not for a release
   build. `regex` joins the table with `US-0100`; there is no `serde` row (Open Decision 3).
9. **Documentation** (owner ruling 2026-09-15): `cargo doc -p oneterm-vt --no-deps --open` and a
   link to the repository for the full design. **No docs.rs URL anywhere**, for the crate or for
   the guide: docs.rs builds what it is given by the registry and it will never be given this. The
   embedder's guide is named as **planned** work for the same reason it always was -- the module
   does not exist until [`US-0103`](../US-0103-embedder-guide.md) writes it -- and a front page
   linking a page that cannot exist is the drift this section exists to prevent. `US-0103` replaces
   the line with a local path, and writes `scripts/vt-docs.sh` / `.ps1` if it wants a one-command
   render of its own.
10. MSRV, the semver promise by reference, and a **Licence** section pointing at the crate's own
   `LICENSE` and `NOTICE` -- the copies inside `crates/vt/`, which are the ones a consumer actually
   receives -- plus a link to the repository's `docs/spec-intakes/IN-0029-vt-engine/` for the full
   design.

`rio-vt`'s README drifted from its own manifest within seven weeks of publication -- its feature
table names a default set the manifest contradicts, and its `EventListener` example calls a method
the trait does not have. Rule 5 above is the specific defence: every code block in the README is
compiled by something.

## Guide

The README is the shop window; the twelve-chapter embedder's guide is the manual. It is
[`US-0103`](../US-0103-embedder-guide.md)'s, not `US-0097`'s: Markdown chapters under
`crates/vt/docs/guide/`, pulled into the crate docs by `#[doc = include_str!]` modules so
`cargo doc` renders them beside the API reference and their code blocks run as doctests. Two
consequences land on this document: the package must ship `docs/guide/` (checked in `US-0103`'s
`cargo package --list` criterion, because `include_str!` fails for anyone whose checkout lacks the
file), and the `cargo doc` step below is the one CI already runs under `-D warnings`, reused rather
than doubled. Rendering beside the API reference is now a local `cargo doc` and nothing else (owner
ruling 2026-09-15); the guide is no less useful for it, and `US-0103` inherits the constraint.

## Example

`crates/vt/examples/headless.rs`, about 90 lines, no dependency beyond the crate:

```text
1. Terminal::new(Size { rows: 4, cols: 32 }, config) where config claims
   OSC 7 and OSC 1337 -- the "extend" demonstration -- and sets product_name.
2. feed() a byte string containing: printable text, SGR colour, a CUP, an
   OSC 0 title, an OSC 7 cwd, and an OSC 1337 the example handles itself.
3. Drain the batch and print each VtEvent, showing the typed Title next to
   the raw Osc payloads the example claimed.
4. render_update() and print the visible rows as plain text.
```

Two names in the sketch above moved to later packets: the routing verb is `claim`, not `Forward`
(`US-0098`), and `OSC 7` arrives as a raw `VtEvent::Osc` rather than a typed `Cwd` for the same
reason. `render_update` becomes `snapshot_update` at `US-0101`, and the example changes with it.

It is built by CI (`cargo build -p oneterm-vt --examples`) and runnable
(`cargo run -p oneterm-vt --example headless`), and its output is small enough to paste into the
packet as evidence.

## CHANGELOG

`crates/vt/CHANGELOG.md`, Keep-a-Changelog shape, `## [Unreleased]` at the top. It documents the
**crate's API**, not OneTerm's features: an entry belongs here when it changes what an embedder
compiles against or what bytes the terminal replies with. `US-0097` seeds it with an `Unreleased`
section and each later packet adds its own lines, so the file is never written retroactively.

The semver promise from [`api-surface.md`](api-surface.md) is restated at the top of the file, above
the first release, because that is where someone evaluating the crate will look for it.

## `#![warn(missing_docs)]`

Added at the crate root in `US-0097`. It is a `warn`, not a `deny`, because the workspace `[lints]`
table already turns warnings into errors in CI while leaving a local work-in-progress build usable.

The work it creates is real and is budgeted in `US-0097`: every `pub` struct field, enum variant and
associated constant needs a line. The bulk is in `grid`, `parser` and the event types. Two rules
keep it from becoming noise:

- A field whose name is its documentation (`pub rows: u16`) gets a line saying the **unit and the
  bound**, not the name again: "Rows, clamped to `Size::MAX_ROWS` by `Size::clamped`."
- A variant that mirrors a wire value names the wire value: "`OSC 9;4` state `2`."

## The doc-comment self-containment pass

Measured: **105 rustdoc lines** in `crates/vt/src` cite `US-NNNN`, `DEC-NNNN`, `IN-NNNN` or a
`docs/spec-intakes/` path, spread over 37 non-test files; 202 lines counting plain `//` comments. On
`cargo doc` in somebody else's checkout, each rendered one is a reference to a document that
reader does not have. The ruling does not soften this: a git consumer has the crate directory and
no `docs/spec-intakes/` tree, which is the same reader with the same problem.

Re-measured at `US-0097` with the acceptance grep exactly as written below: **106 lines in 44
files**. The grep does not exclude the eight `*_tests.rs` / `*_bench.rs` files this count did, and
those ship in the package too, so all 44 were cleaned. The pattern CI runs also covers `BUG-NNNN`,
which this one omits; that alternative found eight further lines.

The rule and its enforcement are in the HLD. The mechanical part `US-0097` owns:

```bash
# Must return 0 after the packet. This is the acceptance criterion.
grep -rn '^[[:space:]]*//[/!].*\(US-0[0-9]\{3\}\|DEC-0[0-9]\{3\}\|IN-0[0-9]\{3\}\|docs/spec-intakes\)' \
  crates/vt/src --include='*.rs' | grep -v 'https://github.com/'

# What CI runs: the same, plus BUG-NNNN.
grep -rn '^[[:space:]]*//[/!].*\(US-0[0-9]\{3\}\|BUG-0[0-9]\{3\}\|DEC-0[0-9]\{3\}\|IN-0[0-9]\{3\}\|docs/spec-intakes\)' \
  crates/vt/src --include='*.rs' | grep -v 'https://github.com/'
```

The `grep -v` is the exception: a citation rewritten as an absolute link to the public repository is
allowed and is the intended form for the handful of module-level "Design:" lines. `python
scripts/check-doc-paths.py` does not look inside `crates/`, so this grep -- added to
`scripts/ci-local.sh` and `.ps1` -- is the only thing that enforces it.

## MSRV

`rust-version.workspace = true`, currently `1.96.0`, kept in step with `rust-toolchain.toml`.

Two honest gaps, recorded rather than papered over:

1. **CI does not build at the MSRV.** It builds on the pinned toolchain, which is the same number
   today and will not be after the next toolchain bump. Until an MSRV job exists, the `rust-version`
   field is a claim, not a proof. `US-0097` adds the field and records the gap; adding the job is a
   separate decision because it costs CI minutes on every push.
2. **OneTerm bumps its toolchain freely.** A crate other projects pin cannot. The policy proposed
   in the intake (Open Decision 4) is that an MSRV raise is a minor version bump plus a CHANGELOG
   line and never a patch. `US-0097` states that rule as the crate's promise -- `crates/vt/CHANGELOG.md`
   rule 3 and the README's Compatibility section both state it -- while Open Decision 4 itself is
   still unrecorded. A different ruling costs two sentences in two files.

## Rendered documentation

**There is no docs.rs page (owner ruling 2026-09-15).** docs.rs builds what the registry hands it,
and the registry will never be handed this crate, so `[package.metadata.docs.rs]` came out of the
manifest: a key that configures a builder which cannot run is a claim about a page that does not
exist. The API reference is `cargo doc -p oneterm-vt --no-deps --open`, run by the consumer in
their own checkout, which is where a git dependency's documentation has always come from.

Three things this costs, and what replaces each:

- **Feature badges.** `--cfg docsrs` was there so `#[cfg_attr(docsrs, doc(cfg(feature = ...)))]`
  renders one. There is no feature-gated item today, and `US-0100` can pass `--cfg docsrs` through
  `RUSTDOCFLAGS` locally if it wants the badge. Nothing is lost that exists yet.
- **A build on somebody else's machine.** docs.rs's sandbox has no network and no Windows, which
  made it a free portability check. CI replaces it directly: `RUSTDOCFLAGS="-D warnings" cargo doc
  -p oneterm-vt --no-deps --all-features` runs on every push, and the crate is pure Rust with six
  leaf dependencies and no build script, so there was never much to catch.
- **A URL to link.** The README and the guide both link local commands instead, which is why
  neither may carry a docs.rs address.

If a public rendered copy is ever wanted without publishing, the obvious host is **GitHub Pages**:
`cargo doc --no-deps` output is a static directory, and a workflow can push it to `gh-pages` on a
tag. Nobody has asked, so nobody should build it; recorded here so the next person who asks gets an
answer rather than a debate about publishing.

`vt-paranoid` stays doc-neutral -- it gates assertions, not items -- so `--all-features` renders the
same page as the default build.

## Effect on the repository's existing checks

| Check | Effect | Action |
| --- | --- | --- |
| `python scripts/third-party-notices.py --check` | **none**, confirmed at `US-0097`. The script walks the graph reachable from `oneterm-app` only, and the crate gained no dependency. When `US-0100` adds `regex` it stays none: the feature is default-off and therefore unreachable, and `regex 1.12.4` is already in `THIRD-PARTY-NOTICES.md` via `oneterm-highlight`. | verify it still passes; no regeneration expected |
| `python scripts/verify-dependency-graph.py` | **changed by `US-0097`, then by the owner ruling 2026-09-15.** It still requires `version.workspace = true`. It now also asserts that **no** package in the workspace is publishable (inverted from "exactly one"), that `oneterm-vt` depends on no other OneTerm crate, and, when given `--package-list -`, that the packaged file list carries `README.md`, `CHANGELOG.md`, `LICENSE`, `NOTICE` and `examples/headless.rs` and reaches nothing outside `crates/vt`. | every packaging assertion lives here, in one language |
| `python scripts/check-doc-paths.py` | **none.** Its `DOCUMENTS` list is `docs/architecture.md`, `docs/README.md`, `docs/terminal-backend.md`, `README.md`, `AGENTS.md` and `docs/agents/*.md`; it never looks in `crates/` or in `docs/spec-intakes/`. The new intake files are therefore unchecked, and the two documents this intake edits (`docs/README.md`, `docs/terminal-backend.md`) **are** checked, so every path added to them must exist. | run it; it is the gate on the two index edits |
| `python scripts/check-english.py` | scans `crates/`, `docs/`, `scripts/`, `AGENTS.md`, `README.md` and `Cargo.toml`. For Rust it inspects comments only, for Markdown the whole file. The new README, CHANGELOG, example and every intake document are in scope. | run it; all new text is ASCII English |
| `cargo deny check licenses bans advisories` | **none** at `US-0097`, which adds no dependency. `regex` (MIT OR Apache-2.0), which `US-0100` makes optional here, is already in the graph and already allowed by `deny.toml`. Nothing new to allow. | run with `ci-local --full` once at the end of the intake |
| `scripts/completion-catalog.py validate` | none | -- |
| New: `cargo package -p oneterm-vt --list` piped into `verify-dependency-graph.py --package-list -` | builds the file list a consumer receives and asserts what it must contain. Catches a missing README, licence, changelog or example before a tag does. `cargo publish --dry-run` is **gone everywhere** (owner ruling 2026-09-15): it talks to a registry this crate does not use, it updates the crates.io index, and it refuses to run on a dirty tree, which is the one state `AGENTS.md` guarantees when it tells every agent to run `ci-local` before finishing. The local scripts add `--allow-dirty`; the workflow does not need it. | `ci.yml` and `ci-local.{sh,ps1}`, same check, one Python implementation |
| New: `cargo package -p oneterm-vt` | actually builds the package and compiles it out of tree, which the `--list` form does not. Works unchanged with `publish = false` and needs no `--no-verify`. | `.github/workflows/ci.yml`, job "Packaged crate (oneterm-vt)" |
| New: `cargo build -p oneterm-vt --no-default-features --examples` and `--all-features --examples` | proves the feature matrix and that the example still builds | add to CI |
| New: `cargo run -p oneterm-vt --example headless` | proves the example still runs, not just compiles | `.github/workflows/ci.yml` |
| New: `crates/vt/public-api.txt` diff | see [`api-surface.md`](api-surface.md) | add to CI |
| New: `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps --all-features` | the `missing_docs` gate. `US-0097` adds it already tightened, rather than leaving the `-D warnings` to `US-0103`: it caught a public-to-private intra-doc link on the way in, and the guide chapters will fail the same step. One doc build, not two. | added in `US-0097` |

The package includes `crates/vt/**` only, confirmed at `US-0097` by reading the file list
`cargo package --list` prints and now asserted by the script that reads it. This is what a consumer
gets whether they fetch a tarball or a git checkout: `cargo` vendors the crate directory, not the
repository around it. `Cargo.toml` needs no `exclude`: the parity corpus moved to
`crates/tools` at `US-0093`, so there is no large test data left, and `crates/vt/fuzz/` drops out on
its own because it declares its own `[workspace]` table. `public-api.txt` does ship, which is
harmless -- it describes the crate the reader is holding.

**Licence text travels with the package.** Apache-2.0 section 4(a) requires a copy of the licence
with every distribution and 4(d) requires the `NOTICE` file to travel with it, and `cargo package`
can only see files inside the package directory -- the repository root's copies are invisible to it.
The ruling does not change this: a distribution by git tag is still a distribution, and a consumer
whose vendored copy has no licence text is in the same position as one who downloaded a tarball
without it. `US-0097` therefore keeps a copy of `LICENSE` and `NOTICE` in `crates/vt/`, the README's
Licence section points at those copies rather than the repository root, and the assertion that both
are in the packaged list is in `scripts/verify-dependency-graph.py` beside the rest, so it is one
gate in one language rather than a grep in two shells.

## Harness row

`harness.db` was **not** written by this task; no harness binary is available in this worktree and
the task forbids editing the database. The intake row must be inserted before any `US-`/`BUG-`
packet is tracked. The real schema is
`intake(created_at, input_type, summary, risk_lane, risk_flags, affected_docs, story_id, doc_path,
notes, document_number, design_doc)`, where `input_type` is one of
`new_spec` / `spec_slice` / `change_request` / `new_initiative` / `maintenance` /
`harness_improvement` and `risk_lane` is one of `tiny` / `normal` / `high_risk`.

```python
#!/usr/bin/env python3
"""Insert the IN-0038 intake row. Point DB at the harness database and run once."""
import sqlite3
from datetime import datetime, timezone

DB = "<path to harness.db>"

ROW = dict(
    created_at=datetime.now(timezone.utc).isoformat(timespec="seconds"),
    input_type="new_initiative",
    summary=(
        "Make oneterm-vt an embeddable terminal core: move OSC handling, key/mouse "
        "encoding and scrollback search in; add an OSC route/override mechanism; "
        "rename render to snapshot; package it for outside consumers."
    ),
    risk_lane="high_risk",
    risk_flags="public-contract,external-consumers,packaging",
    affected_docs=";".join([
        "docs/agents/crate-dependency-rules.md",
        "docs/agents/structure.md",
        "docs/agents/dependencies.md",
        "docs/terminal-backend.md",
        "docs/osc-sequences-checklist.md",
        "docs/osc-agent-status.md",
        "docs/README.md",
    ]),
    story_id=None,
    doc_path="docs/spec-intakes/IN-0038-embeddable-vt-core/IN-0038.md",
    notes=(
        "Owner decisions a-f are fixed and not to be re-litigated. Open decisions "
        "at intake: OSC 9;7 removal, first publish, serde feature, MSRV policy, "
        "product_name default, repository layout. Ruled 2026-09-15: not published "
        "to crates.io, consumed by git; no serde feature; product_name defaults to "
        "oneterm-vt(<version>); the crate stays in the monorepo."
    ),
    document_number=38,
    design_doc="docs/spec-intakes/IN-0038-embeddable-vt-core/high-level-design.md",
)

with sqlite3.connect(DB) as db:
    columns = ", ".join(ROW)
    placeholders = ", ".join("?" for _ in ROW)
    db.execute(f"INSERT INTO intake ({columns}) VALUES ({placeholders})", tuple(ROW.values()))
print("inserted IN-0038")
```

## Edge Cases and Failure Modes

- [ ] A consumer's build cannot resolve an OneTerm dependency of the engine -- `oneterm-vt` has
  **no** OneTerm dependency at all (R7), so there is nothing to resolve. That was the property that
  made it publishable; under a git dependency it is the property that makes it usable, and
  `verify-dependency-graph.py` now asserts it directly rather than inferring it from `publish`.
- [ ] `publish = true` leaks onto any crate -- caught by the inverted assertion in
  `verify-dependency-graph.py`, which now fails on a publishable package anywhere in the workspace
  (owner ruling 2026-09-15).
- [ ] The README references a path that only exists in the repository -- allowed, provided it is an
  absolute `https://github.com/...` link. A relative `docs/...` link is broken for a reader holding
  only the packaged crate, which is what a git consumer's vendored checkout contains.
- [ ] A feature-gated item fails to resolve in the doc build -- caught by CI's
  `RUSTDOCFLAGS="-D warnings" cargo doc --all-features`, which is the check docs.rs used to provide
  for free. There is no feature-gated item until `US-0100`.
- [ ] The version is bumped by an unrelated app release and the CHANGELOG has no entry -- acceptable
  and expected; the CHANGELOG gains a "no API change" line at release time. Documented at the top of
  the file so a reader is not confused by version gaps.
- [ ] A consumer pins `branch = "main"` and a later commit breaks them -- the README's Install
  section tells them to pin a `tag` or a `rev`, and the semver promise is written in terms of tags.
  This replaces the crates.io failure mode (a yanked or squatted name), which the ruling removed:
  nothing reserves `oneterm-vt` on the registry, and nothing needs to until the ruling changes.

## Verification

- [ ] `cargo package -p oneterm-vt` succeeds, and `cargo package -p oneterm-vt --list` piped into
  `python scripts/verify-dependency-graph.py --package-list -` passes: `README.md`, `CHANGELOG.md`,
  `LICENSE`, `NOTICE` and `examples/headless.rs` present, nothing outside `crates/vt`.
- [ ] `cargo doc -p oneterm-vt --no-deps --all-features` warning-free with `missing_docs` on.
- [ ] `cargo build -p oneterm-vt --no-default-features`, `--all-features`, and `--examples` all clean.
- [ ] `grep -c 'publish = true' crates/*/Cargo.toml` totals 0 (owner ruling 2026-09-15), and
  `verify-dependency-graph.py` fails if it ever does not.
- [ ] The self-containment grep above returns 0 lines.
- [ ] `python scripts/check-english.py`, `python scripts/check-doc-paths.py`,
  `python scripts/third-party-notices.py --check` and
  `python scripts/verify-dependency-graph.py` all pass.
- [ ] `pwsh scripts/ci-local.ps1 --full` green, including `cargo deny`.
