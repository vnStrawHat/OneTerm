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

```toml
[package]
name = "oneterm-vt"
description = "An embeddable terminal core: VT parser, grid with scrollback, reflow, selection, \
damage-tracked snapshots, Sixel, key and mouse encoding, and extensible OSC handling. No rendering, \
no PTY, no policy."
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
default = []
regex = ["dep:regex"]
serde = ["dep:serde", "bitflags/serde"]
vt-paranoid = []
```

`keywords` is capped at five and `categories` at five by crates.io; both lists above are within it.
The `description` is what appears in search results, so it leads with what the crate is and ends
with what it is not -- the three exclusions are the crate's selling point against `rio-vt`, which
ships a PTY by default.

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

`crates/vt/README.md`, about 120 lines, in this order:

1. One-line description and the crates.io / docs.rs badges.
2. **What it is not**: no renderer, no PTY, no window, no clipboard backend, no policy. Say it
   third, before anyone reads far enough to be disappointed.
3. The dependency tree, pasted from `cargo tree -p oneterm-vt -e normal`. Six lines. This is the
   crate's strongest claim and it should be visible without scrolling twice.
4. Quick start: the twenty-line body of `examples/headless.rs`, as a fenced block that is **also** a
   doctest, so it cannot compile in the example and rot in the README.
5. OSC routing: the `OscRoute` table, the three-line OSC 20308 example from
   [`osc-extension.md`](osc-extension.md), and a sentence pointing at the module docs.
6. Feature table: `regex`, `serde`, `vt-paranoid`, all default-off, with what each adds.
7. **Documentation**: the embedder's guide -- its docs.rs URL
   (`https://docs.rs/oneterm-vt/latest/oneterm_vt/guide/`) and the one-command local render,
   `scripts/vt-docs.sh` or `pwsh scripts/vt-docs.ps1`. Written by
   [`US-0103`](../US-0103-embedder-guide.md); `US-0097` leaves the section in place with the URL, so
   the README is never missing it.
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
`cargo doc` step below is the one CI already runs for `missing_docs`, tightened rather than doubled.

## Example

`crates/vt/examples/headless.rs`, about 70 lines, no dependency beyond the crate:

```text
1. Terminal::new(Size { rows: 24, cols: 80 }, config)
   where config routes OSC 1337 to Forward -- the "extend" demonstration.
2. feed() a byte string containing: printable text, SGR colour, a CUP, an
   OSC 0 title, an OSC 7 cwd, and an OSC 1337 the example handles itself.
3. Drain the batch and print each VtEvent, showing the typed Cwd and Title
   next to the raw Osc the example claimed.
4. snapshot_update() and print the visible rows as plain text.
```

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

The rule and its enforcement are in the HLD. The mechanical part `US-0097` owns:

```bash
# Must return 0 after the packet. This is the acceptance criterion.
grep -rn '^[[:space:]]*//[/!].*\(US-0[0-9]\{3\}\|DEC-0[0-9]\{3\}\|IN-0[0-9]\{3\}\|docs/spec-intakes\)' \
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
   never a patch; the owner has not ruled on it.

## docs.rs

`all-features = true` so `regex` and `serde` items appear; `--cfg docsrs` so
`#[cfg_attr(docsrs, doc(cfg(feature = "regex")))]` renders the feature badge. The crate must build
on docs.rs's sandbox, which has no network and no Windows: it is pure Rust with six leaf
dependencies and no build script, so this is a non-issue -- but `US-0097` proves it with
`cargo doc -p oneterm-vt --no-deps --all-features` rather than assuming.

`vt-paranoid` is included by `all-features = true` and is doc-neutral (it gates assertions, not
items), so there is nothing to exclude.

## Effect on the repository's existing checks

| Check | Effect | Action |
| --- | --- | --- |
| `python scripts/third-party-notices.py --check` | **none.** The script walks the graph reachable from `oneterm-app` only. `regex` and `serde` are default-off, so they are not reachable; and even enabled, `regex 1.12.4` and `serde 1` are already in `THIRD-PARTY-NOTICES.md` via `oneterm-highlight` and `oneterm-core`. | verify it still passes; no regeneration expected |
| `python scripts/verify-dependency-graph.py` | **none**, provided the crate keeps `version.workspace = true`. The script has no opinion on `publish`. | no change |
| `python scripts/check-doc-paths.py` | **none.** Its `DOCUMENTS` list is `docs/architecture.md`, `docs/README.md`, `docs/terminal-backend.md`, `README.md`, `AGENTS.md` and `docs/agents/*.md`; it never looks in `crates/` or in `docs/spec-intakes/`. The new intake files are therefore unchecked, and the two documents this intake edits (`docs/README.md`, `docs/terminal-backend.md`) **are** checked, so every path added to them must exist. | run it; it is the gate on the two index edits |
| `python scripts/check-english.py` | scans `crates/`, `docs/`, `scripts/`, `AGENTS.md`, `README.md` and `Cargo.toml`. For Rust it inspects comments only, for Markdown the whole file. The new README, CHANGELOG, example and every intake document are in scope. | run it; all new text is ASCII English |
| `cargo deny check licenses bans advisories` | `regex` (MIT OR Apache-2.0) and `serde` (MIT OR Apache-2.0) are already in the graph and already allowed by `deny.toml`. Nothing new to allow. | run with `ci-local --full` once at the end of the intake |
| `scripts/completion-catalog.py validate` | none | -- |
| New: `cargo publish -p oneterm-vt --dry-run` | packages the crate and checks the manifest, the `readme` path and the file list. Catches a missing README or an excluded example before a tag does. | add to `ci-local` and to `.github/workflows/ci.yml` |
| New: `cargo build -p oneterm-vt --no-default-features` and `--all-features` | proves the feature matrix | add to CI |
| New: `crates/vt/public-api.txt` diff | see [`api-surface.md`](api-surface.md) | add to CI |
| New: `cargo doc -p oneterm-vt --no-deps --all-features` | `US-0097` adds it for the `missing_docs` gate; `US-0103` tightens the same step with `RUSTDOCFLAGS="-D warnings"` so a broken intra-doc link in a guide chapter fails CI. One doc build, not two. | add in `US-0097`, tighten in `US-0103` |

The crates.io package will include `crates/vt/**` only. `Cargo.toml` needs no `exclude` today: the
parity corpus moved to `crates/tools` at `US-0093`, so there is no large test data left in the
crate. `US-0097` confirms this by reading the file list `cargo publish --dry-run` prints, and adds
an `exclude` for `fuzz/` if the number surprises.

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
