# Low-Level Design: Packaging a published crate

Intake: IN-0038
HLD: [`../high-level-design.md`](../high-level-design.md)
Topic: packaging
Date: 2026-09-15

## Concern

Everything that has to be true for `crates/vt` to be a crate someone else can depend on:
`publish = true`, README, example, CHANGELOG, `#![warn(missing_docs)]`, MSRV, docs.rs metadata, and
the effect on the four repository scripts and `cargo-deny` that police the workspace today.

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
# The only crate in the workspace that overrides the inherited `publish = false`.
publish = true

[package.metadata.docs.rs]
all-features = true
rustdoc-args = ["--cfg", "docsrs"]

[features]
vt-paranoid = []
```

`keywords` is capped at five and `categories` at five by crates.io; both lists above are within it.
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
republishes the engine unchanged, and it means `python scripts/verify-dependency-graph.py` keeps
passing -- that script asserts every workspace package's version equals `[workspace.package]
version` and would fail on an independent number. An independent version was considered and
rejected: it buys tidier release notes and costs a second version source, a script change, and a
new way for the two to drift. `rio-vt` inherits its workspace version for the same reason.

**`publish = true` is the only per-crate override.** A reviewer must be able to `grep -n 'publish'
crates/*/Cargo.toml` and see exactly one line. `US-0097` adds a CI assertion for that, because a
stray `publish = true` on `oneterm-app` or `oneterm-ssh` would be a genuine accident.

## README

`crates/vt/README.md`, about 140 lines, in this order:

1. One-line description and the crates.io / docs.rs badges.
2. **What it is not**: no renderer, no PTY, no window, no clipboard backend, no policy. Say it
   third, before anyone reads far enough to be disappointed.
3. The dependency tree, pasted from `cargo tree -p oneterm-vt -e normal`. Six lines. This is the
   crate's strongest claim and it should be visible without scrolling twice.
4. Quick start: a short snippet -- `Terminal::new`, `feed`, drain the batch, pull a snapshot --
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
5. OSC routing: today's `OscClaims` -- `claim`, `claim_large`, `NATIVE` -- as a compiled block, and
   a sentence saying a new OSC number costs one call plus a `match` arm. `US-0098` rewrites this
   section around the `OscRoute` table when it replaces `OscClaims`.
6. Feature table: `vt-paranoid` only, default-off, with what it adds and why it is not for a release
   build. `regex` joins the table with `US-0100`; there is no `serde` row (Open Decision 3).
7. **Documentation**: the API reference on docs.rs, the local `cargo doc -p oneterm-vt --no-deps
   --all-features --open`, and a link to the repository for the full design. The embedder's guide is
   named as **planned** work, with no docs.rs URL: `oneterm_vt::guide` does not exist until
   [`US-0103`](../US-0103-embedder-guide.md) writes it, and a front page that links a 404 is the
   drift this section exists to prevent. `US-0103` replaces the line with the real link, and writes
   `scripts/vt-docs.sh` / `.ps1` if it wants a one-command local render of its own.
8. MSRV, licence, and a link to the repository's `docs/spec-intakes/IN-0029-vt-engine/` for the
   full design.

`rio-vt`'s README drifted from its own manifest within seven weeks of publication -- its feature
table names a default set the manifest contradicts, and its `EventListener` example calls a method
the trait does not have. Rule 4 above is the specific defence: every code block in the README is
compiled by something.

## Guide

The README is the shop window; the twelve-chapter embedder's guide is the manual. It is
[`US-0103`](../US-0103-embedder-guide.md)'s, not `US-0097`'s: Markdown chapters under
`crates/vt/docs/guide/`, pulled into the crate docs by `#[doc = include_str!]` modules so `cargo doc`
and docs.rs render them beside the API reference and their code blocks run as doctests. Two
consequences land on this document: the package must ship `docs/guide/` (checked in `US-0103`'s
`cargo publish --dry-run` criterion, because `include_str!` fails on docs.rs otherwise), and the
`cargo doc` step below is the one CI already runs under `-D warnings`, reused rather than doubled.

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
docs.rs each rendered one is a reference to a document the reader cannot open.

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
2. **OneTerm bumps its toolchain freely.** A published crate cannot. The policy proposed in the
   intake (Open Decision 4) is that an MSRV raise is a minor version bump plus a CHANGELOG line and
   never a patch. `US-0097` publishes that rule as the crate's promise -- `crates/vt/CHANGELOG.md`
   rule 3 and the README's Compatibility section both state it -- while Open Decision 4 itself is
   still unrecorded. A different ruling costs two sentences in two files.

## docs.rs

`all-features = true` so feature-gated items appear -- none today, `regex`'s with `US-0100` --
and `--cfg docsrs` so `#[cfg_attr(docsrs, doc(cfg(feature = "regex")))]` renders the feature badge
when there is one. Both keys are in the manifest from `US-0097` so the later packet adds items, not
metadata. The crate must build
on docs.rs's sandbox, which has no network and no Windows: it is pure Rust with six leaf
dependencies and no build script, so this is a non-issue -- but `US-0097` proves it with
`cargo doc -p oneterm-vt --no-deps --all-features` rather than assuming.

`vt-paranoid` is included by `all-features = true` and is doc-neutral (it gates assertions, not
items), so there is nothing to exclude.

## Effect on the repository's existing checks

| Check | Effect | Action |
| --- | --- | --- |
| `python scripts/third-party-notices.py --check` | **none**, confirmed at `US-0097`. The script walks the graph reachable from `oneterm-app` only, and the crate gained no dependency. When `US-0100` adds `regex` it stays none: the feature is default-off and therefore unreachable, and `regex 1.12.4` is already in `THIRD-PARTY-NOTICES.md` via `oneterm-highlight`. | verify it still passes; no regeneration expected |
| `python scripts/verify-dependency-graph.py` | **changed by `US-0097`.** It still requires `version.workspace = true`, and it now also asserts from `cargo metadata` that `oneterm-vt` is the **only** publishable package, and that `cargo package --list` ships `LICENSE` and `NOTICE`. | the publish-set and licence assertions live here |
| `python scripts/check-doc-paths.py` | **none.** Its `DOCUMENTS` list is `docs/architecture.md`, `docs/README.md`, `docs/terminal-backend.md`, `README.md`, `AGENTS.md` and `docs/agents/*.md`; it never looks in `crates/` or in `docs/spec-intakes/`. The new intake files are therefore unchecked, and the two documents this intake edits (`docs/README.md`, `docs/terminal-backend.md`) **are** checked, so every path added to them must exist. | run it; it is the gate on the two index edits |
| `python scripts/check-english.py` | scans `crates/`, `docs/`, `scripts/`, `AGENTS.md`, `README.md` and `Cargo.toml`. For Rust it inspects comments only, for Markdown the whole file. The new README, CHANGELOG, example and every intake document are in scope. | run it; all new text is ASCII English |
| `cargo deny check licenses bans advisories` | **none** at `US-0097`, which adds no dependency. `regex` (MIT OR Apache-2.0), which `US-0100` makes optional here, is already in the graph and already allowed by `deny.toml`. Nothing new to allow. | run with `ci-local --full` once at the end of the intake |
| `scripts/completion-catalog.py validate` | none | -- |
| New: `cargo publish -p oneterm-vt --dry-run` | packages the crate and checks the manifest, the `readme` path and the file list. Catches a missing README or an excluded example before a tag does. It refuses to run on a dirty tree and it updates the crates.io index, and `AGENTS.md` tells every agent to run `ci-local` **with uncommitted work**, so this strict form is the workflow's only. The local scripts run `cargo package -p oneterm-vt --allow-dirty --list` through the same publish-set check instead: offline, dirty-tree-proof, and it still catches a missing README, licence or example. | `.github/workflows/ci.yml` strict; `ci-local.{sh,ps1}` the `--list` form |
| New: `cargo build -p oneterm-vt --no-default-features --examples` and `--all-features --examples` | proves the feature matrix and that the example still builds | add to CI |
| New: `cargo run -p oneterm-vt --example headless` | proves the example still runs, not just compiles | `.github/workflows/ci.yml` |
| New: `crates/vt/public-api.txt` diff | see [`api-surface.md`](api-surface.md) | add to CI |
| New: `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps --all-features` | the `missing_docs` gate. `US-0097` adds it already tightened, rather than leaving the `-D warnings` to `US-0103`: it caught a public-to-private intra-doc link on the way in, and the guide chapters will fail the same step. One doc build, not two. | added in `US-0097` |

The crates.io package includes `crates/vt/**` only, confirmed at `US-0097` by reading the file list
`cargo package --list` prints. `Cargo.toml` needs no `exclude`: the parity corpus moved to
`crates/tools` at `US-0093`, so there is no large test data left, and `crates/vt/fuzz/` drops out on
its own because it declares its own `[workspace]` table. `public-api.txt` does ship, which is
harmless -- it describes the crate the reader is holding.

**Licence text travels with the package.** Apache-2.0 section 4(a) requires a copy of the licence
with every distribution and 4(d) requires the `NOTICE` file to travel with it, and `cargo package`
can only see files inside the package directory -- the repository root's copies are invisible to it.
`US-0097` therefore keeps a copy of `LICENSE` and `NOTICE` in `crates/vt/`, and the assertion that
both are in `cargo package --list` is in `scripts/verify-dependency-graph.py` beside the publish-set
check, so it is one gate in one language rather than a grep in two shells.

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
        "rename render to snapshot; publish the crate."
    ),
    risk_lane="high_risk",
    risk_flags="public-contract,external-consumers,crates-io-publish",
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
        "Owner decisions a-f are fixed and not to be re-litigated. Open decisions: "
        "OSC 9;7 removal, first publish, serde feature, MSRV policy, product_name "
        "default, repository layout."
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

- [ ] `cargo publish` refuses because a path dependency has no version -- `oneterm-vt` has **no**
  OneTerm dependency at all (R7), so there is nothing to version. This is the property that makes
  it the only publishable crate in the workspace, and `--dry-run` proves it.
- [ ] `publish = true` leaks onto another crate -- caught by the one-line `grep` assertion in CI.
- [ ] The README references a path that only exists in the repository -- allowed, provided it is an
  absolute `https://github.com/...` link. A relative `docs/...` link renders as a broken link on
  crates.io.
- [ ] docs.rs builds with `all-features` and `regex` fails to resolve -- cannot happen; `regex` is
  an ordinary crates.io dependency at the workspace pin.
- [ ] The version is bumped by an unrelated app release and the CHANGELOG has no entry -- acceptable
  and expected; the CHANGELOG gains a "no API change" line at release time. Documented at the top of
  the file so a reader is not confused by version gaps.
- [ ] Someone yanks or the name is taken on crates.io -- `oneterm-vt` is unregistered as of
  2026-09-15; `US-0097` re-checks immediately before the first publish and the owner reserves the
  name if the packet slips.

## Verification

- [ ] `cargo publish -p oneterm-vt --dry-run` succeeds, and the file list it prints contains
  `README.md`, `CHANGELOG.md`, `examples/headless.rs` and no directory larger than 1 MB.
- [ ] `cargo doc -p oneterm-vt --no-deps --all-features` warning-free with `missing_docs` on.
- [ ] `cargo build -p oneterm-vt --no-default-features`, `--all-features`, and `--examples` all clean.
- [ ] `grep -c 'publish = true' crates/*/Cargo.toml` totals 1.
- [ ] The self-containment grep above returns 0 lines.
- [ ] `python scripts/check-english.py`, `python scripts/check-doc-paths.py`,
  `python scripts/third-party-notices.py --check` and
  `python scripts/verify-dependency-graph.py` all pass.
- [ ] `pwsh scripts/ci-local.ps1 --full` green, including `cargo deny`.
