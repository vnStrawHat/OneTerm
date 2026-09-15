# Independent verification: US-0100 (scrollback search moves into oneterm-vt, regex behind a feature)

Intake: IN-0038
Packet: `docs/spec-intakes/IN-0038-embeddable-vt-core/US-0100-search-moves-in.md`
Contract: `docs/spec-intakes/IN-0038-embeddable-vt-core/low-level-design/encoding-and-search.md`
Lane declared by the packet: normal. Lane of the intake: high_risk (public contract).
Verifier: independent agent, own worktree
Date: 2026-09-15

Branch under test: `feat/vt-search` @ `ea9e82a`, merge-base `main` @ `0558fa2`.
Worktree: `.claude/worktrees/agent-ac2b25b17746d452a`, `CARGO_BUILD_JOBS=3`.
Nothing was pushed, nothing was committed, `harness.db` was opened read-only (`mode=ro`) and never
written, no GUI was driven and `oneterm.exe` was never touched. My own tests live only in this
worktree (path at the bottom) and were moved out of the tree before the `ci-local` run so the gate
measured the branch, not me.

## Verdict

**PASS-WITH-NOTES.**

Every load-bearing mechanical claim is true. The move is a real move: the twelve literal tests are
the same twelve, the production code of `from_terminal`, `find_in_line`, `matches_at` and
`is_word_boundary` is byte-identical to `main`, the search overlay in `crates/terminal-view` is
completely untouched, and the adapter delta is four files and one behavioural call site. The
dependency budget claims are exact to the crate. Six regex properties I asserted independently all
hold, including the ones the packet only asserted in prose.

The notes are: one documentation file that is now factually wrong and is shipped inside the crate
(N1, the only one I would block a merge on until it is fixed), a "two deltas and nothing else"
test-parity claim that is actually four (N2), an undocumented column index that can be one past the
last column (N3), and three record-level nits (N4-N6).

## What was verified, and how

### 1. The diff

`git diff --stat 0558fa2..HEAD` -- 22 files, +914 / -471. Every production hunk outside
`crates/vt/src/search/`:

| File | Hunk | Verdict |
| --- | --- | --- |
| `crates/vt/src/lib.rs:41` | `pub mod search;` plus the "three modules" -> "four modules" comment | as designed |
| `crates/vt/Cargo.toml:28-31,38-40` | `regex = ["dep:regex"]` feature, `regex = { workspace = true, optional = true }` | as designed |
| `crates/vt/public-api.txt` | +17 lines | matches the packet |
| `crates/vt/CHANGELOG.md:51-61` | three `### Added` bullets under `[Unreleased]` | present |
| `crates/terminal/src/lib.rs:29,50` | `pub(crate) mod search;` removed, `pub use oneterm_vt::search::{SearchMatch, SearchOptions};` added | as designed |
| `crates/terminal/src/model.rs:23,252` | import moves; `search_grid_text(&text, SearchPattern::Literal(query), options)` | **the one behavioural call site** |
| `crates/terminal/src/session.rs:29` | `use` path only | ok |
| `crates/terminal/src/test_support.rs:29` | `use` path only | ok |
| `crates/terminal/src/search.rs` | deleted, 418 lines | as designed |
| `Cargo.lock` | +1 line (`regex` under the `oneterm-vt` package) | matches |
| `.github/workflows/ci.yml:136-139`, `AGENTS.md:111`, `scripts/ci-local.ps1:39-41`, `scripts/ci-local.sh:36-38` | the `cargo test -p oneterm-vt --features regex` gate line | present in all four |

`git diff --stat 0558fa2..HEAD -- crates/terminal-view` is **empty**. The overlay, its highlight
state and its navigation are untouched; it still reaches the types through
`oneterm_terminal::{SearchMatch, SearchOptions}` (`crates/terminal-view/src/terminal_view/search.rs:24`),
which the re-export satisfies. The adapter's delta is therefore *better* than the packet's own
acceptance text, which only promised "unmodified except `use` paths".

I also checked the one API-shape risk the move creates for OneTerm itself: `SearchOptions` is now
`#[non_exhaustive]`, so a struct literal outside `oneterm-vt` no longer compiles. A repo-wide grep
for `SearchOptions\s*\{` finds four hits and all four are inside `crates/vt` (the definition and
three in-crate tests). Nothing in `crates/terminal` or `crates/terminal-view` constructs one by
literal, so the change breaks nothing here.

### 2. Test parity against `main`

`git show main:crates/terminal/src/search.rs` is byte-identical to `git show 0558fa2:...`
(SHA-256 `9869DD2E...`), so `main`'s tip and the merge base agree and the comparison is sound.

Production halves: `GridText::from_terminal`, `find_in_line`, `matches_at` and `is_word_boundary`
are **byte-identical** to `main` (character-for-character, including the comments). The only
production split is that `main`'s `search_grid_text` guarded `query.is_empty() || num_cols == 0` in
one place; the new code guards `num_cols == 0` in `mod.rs:220` and `query.is_empty()` in
`literal.rs:8`. Equivalent.

Test halves: I de-indented `main`'s `mod tests` and diffed it against the literal half of
`crates/vt/src/search/search_tests.rs` (script kept in my scratchpad). All twelve test names, all
inputs and all assertions are the same. The complete list of differences:

1. module doc header and `use` paths (`oneterm_vt::scalar_width` -> `crate::width::scalar_width`);
2. the fixture: `crate::test_engine::terminal(GridSize { .. })` + `test_engine::feed` becomes a
   local `terminal(cols, rows, bytes)` over `Terminal::new(Size { .. }, Config::default())`
   -- **declared**, and I verified the scrollback really is the same:
   `crates/terminal/src/handle.rs:224` is `DEFAULT_SCROLLBACK_LINES = DEFAULT_SCROLLBACK` and
   `crates/vt/src/terminal/mod.rs:112` is `scrollback_limit: DEFAULT_SCROLLBACK` (10 000);
3. `search_grid_text(&text, "cd", ..)` -> `SearchPattern::Literal("cd")` -- **declared**;
4. `assert_eq!(text.rows(), 2);` **added** at `crates/vt/src/search/search_tests.rs:148`
   -- not declared (see N2);
5. `text.oldest` -> `text.oldest()` at `search_tests.rs:151` and `:153` -- not declared, and not
   forced either: the tests are a child module, so the private field is still reachable (the same
   test keeps `text.num_cols` and `text.chars` as fields two lines above);
6. a stale comment fixed, "the eleven tests below" -> "the twelve tests below".

Five regex tests were added (`search::tests::regex_pattern::*`), so
`cargo test -p oneterm-vt --features regex --lib -- search:: --list` lists **17**, as claimed.

### 3. My own tests

`crates/vt/tests/verify_us0100.rs` -- deliberately an integration test, so it exercises the surface
as an *external* crate does and proves `pub` is really `pub`. 12 tests plus one `#[ignore]`d timing
test; all green.

| Check | Result |
| --- | --- |
| literal across an auto-wrap boundary (`cols=5`, `"abcdefghij"`, needle `"efg"`) | no match, as documented. `"abcde"` and `"fghij"` each match once |
| literal across the primary/alt screen switch (`DECSET/DECRST 1049`) | search follows the active screen: on the alt screen `ALTTEXT` = 1 hit, `PRIMARYTEXT` = 0; after `?1049l` the reverse. `from_terminal` is byte-identical to `main`'s, so this is preserved behaviour, not new |
| case-insensitive default with non-ASCII: `I`, U+0131 (Turkish dotless i), U+0130, `ss`, U+00DF | ASCII-only folding, exactly as on `main`. `I`/`i` fold; U+0131 matches only itself; U+0130 matches only itself; `"strasse"` does **not** find `Stra(U+00DF)e`; `"STRA(U+00DF)E"` does (the ASCII half folds, the sharp s is exact) |
| 100 000-row scrollback, linearity (debug build) | 25 000 rows: copy 27 ms, search 63 ms. 100 000 rows: copy **107 ms**, search **249 ms**, 100 000 hits. Ratios for 4x the rows: copy 3.96, search 3.95 -- linear |
| regex `^` / `$` per row | `^..` -> one hit per row at col 0. `ab$` on a 10-column row -> **no match** (the row is padded). `..$` -> `end_col == 10`. Confirms the packet's "`$` anchors to the last cell" |
| regex `\s+$` on a padded row | `cols=8`, `"ab"` -> one hit, `(start_col, end_col) == (2, 8)`. A blank row matches `^\s+$` as `(0, 8)` |
| empty-matching pattern on a 2048-column row (`x*`) | terminates, **2049 matches in 3 ms**, every one zero-width, first at col 0, last at col **2048** (see N3) |
| wide char under `.` | `cols=4`, one wide glyph: `.` -> 4 hits (spacer included); the literal pattern `\u{0}` -> 1 hit at col 1; the glyph itself -> `(0, 1)` |
| invalid regex | `Regex::new("(")` and `Regex::new("a{2,1}")` return `Err` in the caller. The engine takes `&Regex` so it can never see one. No panic path exists |
| catastrophic backtracking `^(a+)+$` on 60 `a`s + `b` | 0 hits in **0 ms**. The `regex` crate is linear-time, as the LLD says |
| `SearchOptions` future-field compatibility | `SearchOptions::default()` + field assignment compiles from another crate and gives the right answer (2 whole-word hits). The struct-literal form is rejected: **E0639** "cannot create non-exhaustive struct using struct expression" |
| `SearchPattern` Copy / Debug / not Eq | copied twice without a clone and `Debug`-formatted. `a == b` is rejected: **E0369** "binary operation `==` cannot be applied to type `SearchPattern<'_>`" |
| `SearchPattern` non-exhaustive from outside, **default features** | matching only `SearchPattern::Literal(_)` is rejected: **E0004** "non-exhaustive patterns: `_` not covered". So the wildcard arm really is forced, and the same `match` compiles either way |

The two compile-failure probes were transient files, run and then deleted.

### 4. Feature matrix

All five states green, `cargo test -p oneterm-vt`:

| State | Result | lib tests | Wall |
| --- | --- | --- | --- |
| default | ok | 386 passed, 2 ignored | 22.3 s (cold) |
| `--no-default-features` | ok | 386 | 3.9 s |
| `--features regex` | ok | **391** | 14.3 s |
| `--all-features` | ok | 391 | 17.5 s |
| `--features vt-paranoid` | ok | 386 | 19.9 s |

Doctests ran in every one of the five (`Doc-tests oneterm_vt`, 5 passed each), so the
`SearchPattern` wildcard-arm doctest really is compiled with the feature both off and on. The
literal suite is 12 tests in every state and identical: `default == no-default-features == 386` and
`regex == all-features == 391`, and 391 - 386 = the five regex tests.

- `cargo clippy -p oneterm-vt --all-targets --features regex -- -D warnings` -- **exit 0**.
- `cargo clippy --workspace --all-targets -- -D warnings` -- **exit 0**.
- `RUSTDOCFLAGS=-D warnings cargo doc -p oneterm-vt --no-deps --all-features` -- **exit 0**.

### 5. Dependency budget, notices, public API, rustdoc self-containment

Verbatim, this worktree:

```text
$ cargo tree -p oneterm-vt -e normal
oneterm-vt v0.5.2 (...\crates\vt)
├── bitflags v2.13.2
├── log v0.4.34
├── memchr v2.8.2
├── rustc-hash v2.1.2
├── unicode-segmentation v1.13.3
└── unicode-width v0.2.2
```

Seven lines, six dependencies. Unchanged from `main`.

```text
$ cargo tree -p oneterm-vt -e normal --features regex
oneterm-vt v0.5.2 (...\crates\vt)
├── bitflags v2.13.2
├── log v0.4.34
├── memchr v2.8.2
├── regex v1.12.4
│   ├── aho-corasick v1.1.4
│   │   └── memchr v2.8.2
│   ├── memchr v2.8.2
│   ├── regex-automata v0.4.14
│   │   ├── aho-corasick v1.1.4 (*)
│   │   ├── memchr v2.8.2
│   │   └── regex-syntax v0.8.11
│   └── regex-syntax v0.8.11
├── rustc-hash v2.1.2
├── unicode-segmentation v1.13.3
└── unicode-width v0.2.2
```

Exactly `regex`, `aho-corasick`, `regex-automata`, `regex-syntax` added, and nothing else. `memchr`
was already there.

- `python scripts/third-party-notices.py --check` -> "THIRD-PARTY-NOTICES.md is up to date." No
  regeneration.
- `python scripts/vt-public-api.py --check --no-doc` -> "public API surface unchanged", exit 0.
  `crates/vt/public-api.txt` lists `GridText` (+`from_terminal`, `oldest`, `rows`), `SearchMatch`
  (+`display_row`, `grid_line`, `end_col`, `row`, `start_col`), `SearchOptions` (+`case_sensitive`,
  `whole_word`), `SearchPattern` (+`Literal`, `Regex`) and `search_grid_text`. The script runs
  `cargo doc --all-features` (`scripts/vt-public-api.py:108`), which is why the feature-gated
  `Regex` variant legitimately appears.
- Rustdoc citation grep over `crates/vt/src/search/` -- **empty**. The whole crate has 23 `///`
  `//!` lines matching the pattern and every one is an allowed
  `https://github.com/vnStrawHat/OneTerm/...` link; none is in `search/`.
- `python scripts/verify-dependency-graph.py` -> passed, 21 packages. R7 holds: the optional
  dependency is third-party, so `vt` still depends on no OneTerm crate and no `gpui`.
- `python scripts/check-doc-paths.py` -> passed (197 paths, 11 documents).
- `python scripts/check-english.py` -> passed (835 files).

### 6. Records

- `docs/agents/structure.md`: `search.rs` leaves the adapter file list, `search/` joins the engine
  tree, the `vt` row names the optional `regex` dependency and scrollback search. Done.
- `docs/agents/dependencies.md`: the `oneterm-vt` row gains the feature, what it adds, and the
  sentence that OneTerm does not enable it. Done.
- `docs/terminal-backend.md`: the `IN-0038` forward pointer, the `search` paragraph and the adapter
  file layout. Done.
- `crates/vt/CHANGELOG.md`: three bullets under `[Unreleased] / Added`. Done. A new module plus a
  new default-off feature is a **patch** bump under the crate's own promise (rule 2); the version is
  workspace-inherited at `0.5.2` and the entry is under `[Unreleased]`, which is consistent with how
  the crate has been handled so far.
- `crates/terminal/Cargo.toml:19` is `oneterm-vt.workspace = true` with **no** `features = [...]`.
  OneTerm does not enable regex. Confirmed.
- Harness snippet: I opened `D:\...\myTerm2\harness.db` read-only. The `story` schema matches the
  packet's column list **exactly**, in order, with the four `*_proof` columns as `CHECK(... IN (0,1))`
  and `status`/`last_verified_result` CHECK sets that the snippet's values satisfy. `intake` row
  `43` is IN-0038 ("oneterm-vt becomes a public embeddable terminal core..."), so `intake_id=43` is
  right. The highest `US-` row present is `US-0097`; `US-0100` is genuinely not inserted, as the
  packet says.
- The packet's acceptance ticks match reality, including the honest ones: the manual Windows walk
  box is `[ ]` and `E2E proof` is `[ ]`. **No GUI walk was done by me either** -- I never launched
  or touched `oneterm.exe`.

### 7. `pwsh scripts/ci-local.ps1 -Full`

**Green. `EXIT=0`, 404 s wall (6 min 44 s), `CARGO_BUILD_JOBS=3`, run with my own test file moved
out of the tree.** All twenty steps passed in order, `cargo deny check licenses bans advisories`
included (`cargo-deny` is installed in this worktree):

```text
==> cargo fmt --all -- --check
==> cargo clippy --workspace --all-targets -- -D warnings
==> cargo clippy --workspace --all-targets --features oneterm-app/terminal-diagnostics -- -D warnings
==> cargo test --workspace
==> cargo test -p oneterm-vt --features vt-paranoid
==> cargo test -p oneterm-vt --features regex          <-- the gate line this packet adds
==> cargo build -p oneterm-vt --no-default-features --examples
==> cargo build -p oneterm-vt --all-features --examples
==> cargo run -p oneterm-vt --example headless
==> cargo doc -p oneterm-vt --no-deps --all-features
==> python scripts/vt-public-api.py --check --no-doc
==> cargo package -p oneterm-vt --list | verify-dependency-graph.py --package-list -
==> rustdoc self-containment (crates/vt/src)
==> python scripts/verify-dependency-graph.py
==> python scripts/check-doc-paths.py
==> python -m unittest scripts/test_check_english.py
==> python scripts/check-english.py
==> python scripts/completion-catalog.py validate
==> python scripts/third-party-notices.py --check
==> cargo deny check licenses bans advisories
EXIT=0 elapsed_s=404
```

`cargo package -p oneterm-vt --list` passed the packaging check, so `crates/vt/src/search/` is
inside the package and the crate still reaches nothing outside `crates/vt`.

## Findings

### N1 -- MEDIUM -- `crates/vt/README.md:138` is now false, and it ships inside the crate

`crates/vt/README.md:132-138`:

```
## Features

| Feature | Default | What it adds |
| --- | --- | --- |
| `vt-paranoid` | off | ... |

No feature adds a dependency today, and no feature changes behaviour -- only availability.
```

The `regex` feature is missing from the table, and the closing sentence is now wrong: `regex` adds
four crates. This is not an ordinary stale doc. `crates/vt/src/lib.rs:21-22` is
`#[doc = include_str!("../README.md")] pub struct ReadmeDoctests;`, so this paragraph is the crate's
own rustdoc front page, and `README.md` is one of the files `US-0097` deliberately put inside the
package for "somebody who has never seen the OneTerm repository". The packet's Reconciliation
section lists four documentation edits and `README.md` is not among them, with no reason recorded.

Fix: add the `regex` row to the table and change the sentence to name the one feature that does add
dependencies. Two lines.

### N2 -- LOW -- the test-parity claim is "two deltas"; it is four

`US-0100-search-moves-in.md:171-175` says "twelve after, same names, same inputs, same expectations.
**Two call sites changed and nothing else**". Two more changed:

- `crates/vt/src/search/search_tests.rs:148` adds `assert_eq!(text.rows(), 2);`, an assertion that
  does not exist on `main`. It is a good assertion (it covers the newly-`pub` `rows()`), but it is a
  new expectation inside a suite whose entire acceptance criterion is "unchanged in input and
  expectation", and it is undeclared.
- `search_tests.rs:151` and `:153` change `text.oldest` to `text.oldest()`. Same value, and not
  forced -- the tests are a child module of `search`, so the private field is still in scope, which
  the same test proves two lines earlier by reading `text.num_cols` and `text.chars` directly.

No assertion's *expectation* changed, so the substance of the criterion holds. The packet's prose
does not.

### N3 -- LOW -- a regex match can report `start_col == num_cols`, and the rustdoc does not say so

`crates/vt/src/search/regex.rs:42-43` maps byte offsets through `column_of`, whose `starts` table
carries a sentinel one past the last column (`regex.rs:33`). A zero-width match at end-of-row
therefore produces `SearchMatch { start_col: num_cols, end_col: num_cols }` -- a column index one
past the last valid column. I measured it: on a 2048-column row, `x*` yields 2049 matches and the
last sits at column 2048.

The range is empty, so a consumer that treats `start_col..end_col` as a range is fine; a consumer
that indexes `row[start_col]` to paint a highlight is not. `crates/vt/src/search/mod.rs:212-214`
documents that such a pattern "finds one empty match per position, and terminates", which is true
but does not warn that one of those positions is off the end of the row. The in-crate test
(`search_tests.rs:236`) asserts `cols == vec![0, 1, 2]` on a two-column grid, so this was seen and
not written down.

Fix: one sentence on `search_grid_text`. No code change needed -- the packet correctly keeps column
indices meaning the same thing for both matchers, and that is what produces the sentinel.

### N4 -- LOW -- risk lane recorded as `normal` for a change to a documented public contract

`US-0100-search-moves-in.md:21` is `Risk lane: normal`. The packet adds a public module, a public
feature and makes a re-exported type `#[non_exhaustive]` on the crate that
`IN-0038.md:187-188` itself calls "what puts this intake in the high-risk [lane]", and AGENTS.md's
harness section names public contracts as high-risk. In practice no gate was skipped -- the
high-risk lane's requirement is a low-level design before the packet, and
`low-level-design/encoding-and-search.md` exists and predates it -- so this is a records-accuracy
note, not a process failure.

### N5 -- LOW -- the debug assertion is the only guard on ignored `SearchOptions`

`crates/vt/src/search/regex.rs:15-18` is a `debug_assert!`, so in a release build an embedder who
passes `case_sensitive: true` with a regex gets it silently ignored. This is what the design asked
for (`encoding-and-search.md`, "debug-asserts that they are default") and it is documented on
`SearchOptions` and in the CHANGELOG, so it is correct as specified. Recorded only so nobody later
reads the CHANGELOG line "`search_grid_text` debug-asserts" as a release-build guarantee.

### N6 -- INFO -- the branch is two commits behind `main`

`main` is `a13002a`; the merge base is `0558fa2`. The two commits in between are the `US-0104`
records (docs only), so `git diff main..HEAD` shows spurious deletions of `US-0104-pty-in-core.md`
and `low-level-design/pty.md`. Every measurement above was taken against the merge base. The merge
needs a rebase or a merge commit; there is no content conflict in any file this packet touches.

### Cosmetic (no number)

`docs/agents/structure.md:194` now reads ``` `grid`, `intern` ``` / ``` `parser` and `search` ```
across a line break with the comma after `intern` dropped. One character.

## What could not be verified

- **The manual Windows search walk.** Not run, by instruction. The overlay is byte-for-byte
  unchanged and the adapter's behavioural delta is one argument, so `cargo test --workspace` covers
  the code that moved -- but nobody has yet opened the overlay on a live session, scrolled it and
  watched the highlights land. The packet's `E2E proof` is `0` and its acceptance box is unticked,
  which is the honest record. This packet is not fully accepted until somebody does that walk.
- **Non-Windows platforms.** Everything here ran on Windows 11 only.
- **Release-build behaviour of the regex debug assertion** (N5): not exercised; a release test run
  was out of scope.
- **`harness.db` contents after the fact.** The row is not inserted and I was forbidden to insert
  it, so I verified the snippet against the live schema rather than by executing it.

## Commands

```text
git reset --hard feat/vt-search                       # ea9e82a, base main 0558fa2
git diff --stat 0558fa2..HEAD
git show main:crates/terminal/src/search.rs           # SHA-256 == git show 0558fa2:...
cargo test -p oneterm-vt                              # 22.3 s, 386 lib tests
cargo test -p oneterm-vt --no-default-features        #  3.9 s, 386
cargo test -p oneterm-vt --features regex             # 14.3 s, 391
cargo test -p oneterm-vt --all-features               # 17.5 s, 391
cargo test -p oneterm-vt --features vt-paranoid       # 19.9 s, 386
cargo test -p oneterm-vt --features regex --lib -- search:: --list   # 17
cargo clippy -p oneterm-vt --all-targets --features regex -- -D warnings
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps --all-features
cargo tree -p oneterm-vt -e normal
cargo tree -p oneterm-vt -e normal --features regex
python scripts/third-party-notices.py --check
python scripts/vt-public-api.py --check --no-doc
python scripts/verify-dependency-graph.py
python scripts/check-doc-paths.py
python scripts/check-english.py
cargo test -p oneterm-vt --test verify_us0100 -- --nocapture                   # mine, 5 + 1 ignored
cargo test -p oneterm-vt --features regex --test verify_us0100 -- --nocapture  # mine, 12
cargo test -p oneterm-vt --features regex --test verify_us0100 -- --ignored --nocapture deep_scrollback
pwsh scripts/ci-local.ps1 -Full
```

## My test files (this worktree only, not committed)

- `crates/vt/tests/verify_us0100.rs` -- 12 tests + 1 `#[ignore]`d timing test. Written as an
  integration test on purpose, so it consumes the surface exactly as an external embedder would.
- Two transient compile-failure probes (`crates/vt/tests/zz_compile_fail_probe.rs`,
  `zz_probe2.rs`), run for E0639 / E0369 / E0004 and deleted.
