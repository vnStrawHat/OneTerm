# Work: scrollback search is the engine's, with regex behind an optional feature

ID: US-0100
Intake: IN-0038
Created: 2026-09-15

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change plus a new capability (the `regex` feature)
- Risk lane: normal
- Spec Intake: `IN-0038`

## Outcome

`crates/terminal/src/search.rs` becomes `oneterm_vt::search`. An embedder can search a terminal's
scrollback with the two-phase shape the module was designed around -- copy under your lock, match
without it -- and can opt into regular expressions with `features = ["regex"]`. The default
dependency set does not move: `cargo tree -p oneterm-vt -e normal` stays seven lines.

OneTerm's own search stays literal and does **not** enable the feature.

## Scope

- [x] In scope: `crates/terminal/src/search.rs` and its tests; the new `crates/vt/src/search/`
  module; `GridText` becoming `pub`; `SearchPattern`; the `regex` cargo feature; the re-export block
  in `crates/terminal/src/lib.rs`.
- [x] Out of scope: `crates/terminal-view/src/terminal_view/search.rs` -- the overlay, its highlight
  state and its navigation stay in the view.
- [x] Out of scope: `TerminalSession::search` and its `SessionKind` plumbing -- the trait is
  OneTerm's contract and does not move.
- [x] Out of scope: a regex search **UI**. This packet makes the engine capable; exposing it to a
  user is a separate product decision with its own packet.
- [x] Out of scope: matching across a wrapped-line boundary. The literal scanner cannot do it today
  and the regex path will not either; the limitation is documented, not fixed.
- [x] Out of scope: changing the copy strategy. `R-19` deliberately copies the grid; this packet
  documents the cost and leaves it.

## Acceptance

- [x] `crates/terminal/src/search.rs` no longer exists; `oneterm_vt::search` does.
- [x] Every test in the current `mod tests` exists in `crates/vt/src/search/`, unchanged in input and
  expectation. Verifier method: diff against `git show main:crates/terminal/src/search.rs`.
- [x] `cargo tree -p oneterm-vt -e normal` is exactly **7 lines** with default features.
- [x] `cargo tree -p oneterm-vt -e normal --features regex` adds exactly `regex`, `regex-automata`,
  `regex-syntax` and `aho-corasick`, and nothing else.
- [x] The whole literal test suite passes identically with and without `--features regex`. A feature
  changes availability, never behaviour.
- [x] `cargo build -p oneterm-vt --no-default-features` is clean, and `SearchPattern` still compiles
  in an embedder's `match` with a `_` arm (proved by a doctest).
- [x] `grep -n 'oneterm-vt' crates/terminal/Cargo.toml` shows **no** `features = [...]`: OneTerm does
  not enable regex.
- [x] `python scripts/third-party-notices.py --check` passes with no regeneration.
- [x] Regex cases, each a test: a literal-equivalent pattern returns the same matches as
  `SearchPattern::Literal`; an anchored `^` matches only at column 0; a pattern that can match the
  empty string terminates and advances at least one column per match; `case_sensitive` or
  `whole_word` with a regex trips a debug assertion and is documented as ignored.
- [x] `cargo test --workspace` green with `crates/terminal-view` unmodified except `use` paths.
- [ ] Manual Windows walk: open the search overlay in a session with several thousand scrollback
  rows, search a string that appears in history and on screen, navigate forwards and backwards,
  confirm the highlights land on the right cells after new output scrolls the grid.

## Documentation

### Owning Docs Reviewed

- `docs/agents/crate-dependency-rules.md` -- R7 and R10, as in `US-0099`.
- `docs/agents/dependencies.md` -- the "Terminal helpers" row already declares `regex 1` in the
  workspace; the `oneterm-vt` row gains the optional dependency.
- `docs/agents/structure.md` -- the crate responsibility table and the tree.
- `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md` -- `SearchMatch` is
  keyed by `RowId` because of this decision; the doc comment citing it must become a
  self-contained sentence under `US-0097`'s rule.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/` -- the owning design for the search
  overlay, which stays in the view.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` -- `R-19`, the rule
  that keeps the copy.

### Documentation Action

**Update required**: `docs/agents/structure.md` (two rows and the tree) and
`docs/agents/dependencies.md` (the `oneterm-vt` row gains `regex`, optional, default-off, and a
sentence saying OneTerm does not enable it).

Reason: `dependencies.md` is the authoritative list of what each crate depends on, and an optional
dependency that nothing in the workspace enables is exactly the kind of fact that rots silently if
it is not written down.

### Reconciliation

Four edits, not two:

- `docs/agents/structure.md` -- `search.rs` leaves the adapter's file list, `search/` joins the
  engine tree, the lib.rs comment now names four by-path modules, and the `vt` row gains the
  optional `regex` dependency and names scrollback search among the engine's responsibilities.
- `docs/agents/dependencies.md` -- the `oneterm-vt` row gains the `regex` feature: what it adds to
  the tree, and the sentence saying OneTerm does not enable it.
- `docs/terminal-backend.md` -- `search.rs` leaves the adapter file layout; the `search` paragraph
  names `oneterm_vt::search::GridText` and says the adapter passes `SearchPattern::Literal`; the
  `IN-0038` forward pointer records that the search half has landed.
- `AGENTS.md`, `scripts/ci-local.{ps1,sh}` and `.github/workflows/ci.yml` -- the gate gains
  `cargo test -p oneterm-vt --features regex`, because a feature that carries a matcher needs its
  suite run under it.

`docs/decisions/DEC-0015-...` was reviewed and **not** changed: the decision stands, and the only
thing `US-0097`'s rule required was removing its citation from `SearchMatch`'s doc comment, which
this packet did as part of the move.

## Context

`crates/terminal/src/search.rs` is 418 lines: `SearchOptions`, `SearchMatch` (with `grid_line` and
`display_row`), `GridText` (`pub(crate)`) with `from_terminal`, `search_grid_text`, and a
`#[cfg(test)]` `search_term` wrapper. It imports `oneterm_vt::{CellWidth, RowId, Terminal}` and
nothing else. Consumers outside the crate: `crates/terminal-view/src/terminal_view/search.rs`.

`regex 1` is already declared in root `[workspace.dependencies]` and `regex 1.12.4` is already in
`THIRD-PARTY-NOTICES.md` via `oneterm-highlight`, so the licence surface does not move in either
direction.

Design: [`low-level-design/encoding-and-search.md`](low-level-design/encoding-and-search.md).

## Plan

- [x] Move the file verbatim to `crates/vt/src/search/{mod.rs, literal.rs, search_tests.rs}`,
  changing only `use` paths and raising `GridText` to `pub` with documentation. Commit alone;
  workspace green with a re-export.
- [x] Introduce `SearchPattern`, with `Literal` only, and change `search_grid_text`'s signature.
  The existing tests move to the new signature mechanically.
- [x] Add the `regex` feature, `crates/vt/src/search/regex.rs`, `SearchPattern::Regex`, and its
  tests. Wire `#[cfg_attr(docsrs, doc(cfg(feature = "regex")))]`.
- [x] Document `GridText::from_terminal`'s cost in `char`s (rows times columns) rather than leaving
  it implicit, and document the wrapped-line limitation on `SearchPattern`.
- [x] Re-export block; `structure.md` and `dependencies.md`; CHANGELOG line.
- [x] Add `--features regex` to the `ci-local` scripts' `cargo test` line for `oneterm-vt`.

## Decisions

None of its own. Owner decision (a) settles the move and the feature gate. Whether OneTerm ever
exposes regex search to a user is a future product decision and is explicitly not taken here.

## Verification Plan

- Focused: the moved literal suite under both feature states; the four regex cases.
- Unit: `cargo test -p oneterm-vt`, `cargo test -p oneterm-vt --features regex`,
  `cargo test --workspace`.
- Integration: `cargo test -p oneterm-terminal`.
- Platform: both `cargo tree` invocations; `cargo build --no-default-features`;
  `python scripts/third-party-notices.py --check`; `pwsh scripts/ci-local.ps1`.
- E2E: the manual search walk in Acceptance.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

**Tests.** Twelve literal tests before the move (`git show main:crates/terminal/src/search.rs`),
twelve after, same names, same inputs, same expectations. Two call sites changed and nothing else:
the fixture (`crate::test_engine::terminal(GridSize { .. })` becomes
`Terminal::new(Size { .. }, Config::default())`, which resolves to the same scrollback limit) and
the one direct `search_grid_text` call, which now names `SearchPattern::Literal("cd")`. Five regex
tests were added, so `cargo test -p oneterm-vt --features regex search` runs 17.

**Dependency budget.**

```text
$ cargo tree -p oneterm-vt -e normal
oneterm-vt v0.5.2 (crates/vt)
|-- bitflags v2.13.2
|-- log v0.4.34
|-- memchr v2.8.2
|-- rustc-hash v2.1.2
|-- unicode-segmentation v1.13.3
`-- unicode-width v0.2.2

$ cargo tree -p oneterm-vt -e normal --features regex
    ... the same six, plus:
|-- regex v1.12.4
|   |-- aho-corasick v1.1.4
|   |   `-- memchr v2.8.2
|   |-- memchr v2.8.2
|   |-- regex-automata v0.4.14
|   |   |-- aho-corasick v1.1.4 (*)
|   |   |-- memchr v2.8.2
|   |   `-- regex-syntax v0.8.11
|   `-- regex-syntax v0.8.11
```

Exactly the four crates the acceptance criterion names, and nothing else. `Cargo.lock` grew by one
line -- `regex` listed under the `oneterm-vt` package -- because every one of those crates was
already resolved. `python scripts/third-party-notices.py --check` says
"THIRD-PARTY-NOTICES.md is up to date", with no regeneration.

**Feature states.** `cargo test -p oneterm-vt` (default), `--features regex`, `--features
vt-paranoid` and `--no-default-features` are all green, and the literal suite is identical under
each. `SearchPattern`'s `#[non_exhaustive]` wildcard arm is proved by a doctest that compiles in
both states.

**Public surface.** `crates/vt/public-api.txt` gained 17 lines: `GridText` (three methods),
`SearchMatch`, `SearchOptions`, `SearchPattern` and `search_grid_text`.
`python scripts/vt-public-api.py --check` passes against it.

Known gaps, all pre-existing and deliberately unchanged: the full-grid copy (`R-19`), no matching
across a wrapped line, ASCII-only case folding in the literal scanner, and no regex search UI.

New and recorded rather than fixed:

- **The manual Windows walk was not run.** The search overlay is untouched and the adapter's only
  change is `SearchPattern::Literal(query)` at one call site, so `cargo test --workspace` covers
  what changed -- but E2E proof is `0` and this packet's acceptance is not complete until somebody
  opens the overlay on a real session.
- **A regex sees the padding.** A row is the full grid width, so `$` anchors to the last cell and
  `\s` matches the blanks after the last glyph; a wide-character spacer reads as `'\0'` and `.`
  will match it. Documented on `SearchPattern::Regex`, not changed: trimming would make a column
  index mean something different for the two matchers.
- **`SearchPattern` is `Copy` and `Debug` but not `PartialEq`.** The design sketch derived `Eq`;
  `regex::Regex` does not implement it, and comparing two patterns is not something any caller
  does.

## Handoff

The verbatim move (Plan step 1) is a clean boundary. The regex half (steps 3 and 4) is independently
reviewable and could be deferred to its own packet if the feature turns out to be contentious --
say so rather than half-landing it.

## Harness Row

`harness.db` was **not** written by this task: no harness binary is available in this worktree and
the task forbids editing the database. The schema is
`story(id, title, created_at, risk_lane, contract_doc, packet_doc, status, unit_proof,
integration_proof, e2e_proof, platform_proof, evidence, verify_command, last_verified_at,
last_verified_result, notes, intake_id)`, with the four `*_proof` columns as `0`/`1`.

```python
#!/usr/bin/env python3
"""Insert the US-0100 story row. Point DB at the harness database and run once."""
import sqlite3
from datetime import datetime, timezone

DB = "<path to harness.db>"

ROW = dict(
    id="US-0100",
    title="scrollback search is the engine's, with regex behind an optional feature",
    created_at="2026-09-15T00:00:00",
    risk_lane="normal",
    contract_doc=(
        "docs/spec-intakes/IN-0038-embeddable-vt-core/"
        "low-level-design/encoding-and-search.md"
    ),
    packet_doc=(
        "docs/spec-intakes/IN-0038-embeddable-vt-core/"
        "US-0100-search-moves-in.md"
    ),
    status="implemented",
    unit_proof=1,
    integration_proof=1,
    e2e_proof=0,
    platform_proof=1,
    evidence=(
        "search.rs moved to oneterm_vt::search; 12 literal tests moved unchanged "
        "in input and expectation, 5 regex tests added (17 under --features regex). "
        "Default cargo tree -e normal still 7 lines / 6 deps; --features regex adds "
        "exactly regex, regex-automata, regex-syntax, aho-corasick. Cargo.lock +1 "
        "line, third-party-notices --check clean with no regeneration. "
        "public-api.txt +17 lines. Green under default, --features regex, "
        "--features vt-paranoid and --no-default-features."
    ),
    verify_command="pwsh scripts/ci-local.ps1 -Full",
    last_verified_at=datetime.now(timezone.utc).isoformat(timespec="seconds"),
    last_verified_result="pass",
    notes=(
        "OneTerm does not enable the regex feature: crates/terminal takes oneterm-vt "
        "with no features and the overlay passes SearchPattern::Literal. e2e_proof=0 "
        "because the manual Windows search walk was not run; the overlay is "
        "unmodified. platform_proof=1 because ci-local -Full passed on Windows, the "
        "only platform exercised."
    ),
    intake_id=43,
)

with sqlite3.connect(DB) as db:
    db.execute(
        "INSERT INTO story ({}) VALUES ({})".format(
            ", ".join(ROW), ", ".join("?" * len(ROW))
        ),
        tuple(ROW.values()),
    )
```
