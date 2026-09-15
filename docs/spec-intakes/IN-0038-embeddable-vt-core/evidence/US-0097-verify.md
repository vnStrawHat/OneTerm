# Independent verification: US-0097 (publishable crate, self-contained docs, product_name)

Intake: IN-0038
Packet: `docs/spec-intakes/IN-0038-embeddable-vt-core/US-0097-publishable-crate.md`
Lane: high_risk (public contract)
Verifier: independent agent, own worktree
Date: 2026-09-15

Branch under test: `feat/vt-publishable` @ `2e1495c`, base `main` @ `92ae9a6`.
Worktree: `.claude/worktrees/agent-ac3c935ebbaf6b1fc`, `CARGO_BUILD_JOBS=4`.
Nothing was pushed. `cargo publish` was never run without `--dry-run`. `harness.db` was read from a
temporary copy only and never written. No GUI was driven and `oneterm.exe` was never touched.

## Verdict

**PASS-WITH-NOTES.**

Every mechanical claim the packet makes is either true or true with a measurable correction. The
crate packages, documents, builds and tests cleanly; the behaviour change is exactly the two reply
strings and nothing else; `pwsh scripts/ci-local.ps1 -Full` is green including `cargo deny`.

The notes are what a high-risk public-contract packet should not carry into a first publish: a
documentation link that does not resolve, a tarball with no licence text, a new public field that
can break a reply's own framing and panic the engine, a "public API" gate that is not reading the
public API, and three blocking Open Decisions claimed as ruled but recorded nowhere. None is a
correctness regression. F1, F2, F3, F4 and F6 should close before the owner's first real
`cargo publish`.

## Findings

### F1 (Medium) The README's embedder-guide link is a dead claim, not marked as future

`crates/vt/README.md:119-123`

```
## Documentation

- The API reference: <https://docs.rs/oneterm-vt>.
- The embedder's guide, rendered beside it: <https://docs.rs/oneterm-vt/latest/oneterm_vt/guide/>.
- Locally, both at once: `cargo doc -p oneterm-vt --no-deps --all-features --open`.
```

There is no `guide` module. `crates/vt/src/lib.rs:31-42` declares `grid`, `intern` and `parser` and
nothing else; `git ls-tree -r main -- crates/vt` has no `docs/guide` and no guide source. `US-0103`
exists only as a packet document (`main` @ `01624f5` added the document, not the guide).

The packet's Plan says the section is left "in place with the URL, so the README is never missing
it", but the rendered text states the guide as a fact and line 123 says "Locally, both at once".
Published at this commit, the crate's own front page links a docs.rs 404 and tells the reader a
local command renders something that does not exist. The claim under review ("stated as future
(US-0103) and not a dead claim") is not what the file says.

Fix: one clause -- "the embedder's guide (in progress, `US-0103`) will render at ..." -- or drop the
two lines until `US-0103` lands.

### F2 (Medium) The published tarball carries no licence text and no NOTICE

`cargo package --list -p oneterm-vt` -> 64 files. None is `LICENSE`, `LICENSE-APACHE` or `NOTICE`.

The workspace `LICENSE` (Apache-2.0, 11.3 KiB) and `NOTICE` (2.2 KiB) live at the repository root,
outside the package directory, so `cargo package` cannot see them. Apache-2.0 section 4(a) requires
a copy of the licence with every distribution of the Work, and 4(d) requires the NOTICE file to
travel with derivative distributions. The generated manifest (`target/package/oneterm-vt-0.5.2/Cargo.toml`)
carries `license = "Apache-2.0"` and no `license-file`.

No gate catches this: `cargo publish --dry-run` does not require the file, and
`scripts/third-party-notices.py` only walks the graph reachable from `oneterm-app`.

Fix: copy `LICENSE` (and `NOTICE`) into `crates/vt/`, or add an `include`/`license-file` entry.

### F3 (Medium) `Config::product_name` is spliced verbatim into the DCS reply, unvalidated and undocumented

`crates/vt/src/terminal/dispatch.rs:1157-1163`

```rust
(b'q', [b'>']) => {
    if args.next_or(0) == 0 {
        let name = self.product_name().to_owned();
        self.reply(&format!("\x1bP>|{name}\x1b\\"));
```

Nothing validates `name`, and the field's rustdoc (`crates/vt/src/terminal/mod.rs:79-87`) says
nothing about control bytes, string terminators or length.

Proven by `verify_hostile_product_name_breaks_dcs_framing` (test file below): with
`product_name = Some("Ev\x1b\\il")`, `CSI > 0 q` answers

```
ESC P > | E v   ESC \   i l   ESC \
```

-- a complete, well-formed XTVERSION answer naming the product "Ev", followed by `il` as loose bytes
the program prints to its own screen, followed by a second stray ST. A `0x9c` (8-bit ST) and a `BEL`
both survive into the reply verbatim (`verify_c1_st_and_bel_in_product_name`), and a 64 KiB name
produces a 65 542-byte single reply the embedder must write back
(`verify_product_name_length_is_unbounded`).

This is embedder-supplied, not stream-supplied, so it is not a remote vulnerability. It is still a
new public field on the one crate whose reply bytes the CHANGELOG declares a contract
(`crates/vt/CHANGELOG.md:20-24`), with no stated constraint on its value. A doc sentence is the
cheap fix; rejecting or escaping `ESC`, `0x9c` and `0x07` at `Terminal::new` is the safe one.

### F4 (Medium) `DA2`'s number from `product_name` panics in a debug build, and collides above 99

`crates/vt/src/terminal/dispatch.rs:794-811`

```rust
fn trailing_version(name: &str) -> Option<u32> { ... Some(version_number(inner)) }

pub(super) fn version_number(version: &str) -> u32 {
    ...
    major * 10_000 + minor * 100 + patch
}
```

`u32` arithmetic with no guard. `product_name = Some("P(429497.0.0)")` overflows:
`verify_da2_version_overflows_on_a_huge_major` passes as `should_panic(expected = "attempt to
multiply with overflow")` in a debug build; a release build wraps silently.

Before this packet `version_number` was only ever called with `env!("CARGO_PKG_VERSION")`. `US-0097`
is what makes it reachable from a public configuration field, so the panic path is new even though
the function is not. `crates/vt/README.md:17` tells the reader the engine "never panics on input";
this is config rather than stream input, but the distinction is not one the README draws.

Secondary: the packing collides above 99. `P(1.1.0)` and `P(1.0.100)` both answer `CSI > 0 ; 10100 ; 1 c`
(`verify_da2_version_packing_collides_above_99`), and `P(999.999.999)` answers `10090899`. The
field's rustdoc (`mod.rs:85-87`) documents "a trailing `(<major>.<minor>.<patch>)`" without saying
the minor and patch are capped at 99 by the encoding.

### F5 (Medium) `public-api.txt` is not the public API, and the gate on it fires on private refactors

`crates/vt/public-api.txt` (764 lines) and `scripts/vt-public-api.py:50-62`.

The script enumerates every rustdoc HTML item page. 75 of the file's 164 item lines are under module
paths no embedder can ever write:

| Path prefix | Item lines | Why unnameable |
| --- | --- | --- |
| `oneterm_vt::cell`, `::event::*`, `::graphics`, `::reflow`, `::render::*`, `::selection`, `::terminal::*`, `::width` | 40 | the module is `pub(crate)` (`crates/vt/src/lib.rs:31-42`) |
| `oneterm_vt::grid::{screen,row,anchor,terminal_grid}` | 22 | private `mod` (`crates/vt/src/grid/mod.rs:17-20`) |
| `oneterm_vt::parser::{osc,params}` | 11 | private `mod` (`crates/vt/src/parser/mod.rs:16-17`) |

Consequence, reproduced: renaming the private module `parser::params` to `parser::parameters` -- a
pure internal rename, `oneterm_vt::parser::Params` untouched -- fails the gate:

```
oneterm-vt's public API surface changed.
If that was intended, run `python scripts/vt-public-api.py --write`,
commit the diff, and add a CHANGELOG entry naming the item.
-struct oneterm_vt::parser::params::Params
+struct oneterm_vt::parser::parameters::Params
```

A gate that demands a CHANGELOG entry for a file rename teaches its users to run `--write` without
reading, which is the exact failure the gate exists to prevent. Cheapest fix: keep only paths whose
first module segment is `grid`, `intern` or `parser` (the three `pub mod`s) -- every item is already
listed at its re-export path as well.

What does work, verified: the rename of a genuinely public item is caught (`cluster_width` ->
`cluster_width_probe` produced the expected two-line diff and exit 1), two consecutive runs are
byte-identical, and signature changes are not detected -- as the script's own docstring and the
packet both admit.

### F6 (Medium) The packet claims owner rulings the intake does not record

`US-0097-publishable-crate.md` asserts three owner rulings:

- Scope: "Out of scope, on the owner's ruling: a `serde` feature."
- Gaps: "The owner ruled `oneterm-vt(<crate version>)`" (the `product_name` default).
- Gaps: "the owner's ruling for this packet was `--dry-run` only."

In `IN-0038.md:286-305` the matching Open Decisions 2 (first publish), 3 (serde) and 5
(`product_name` default) are all still `[ ]` with no ruling text. Open Decision 6 (repository
layout), which the intake itself says "blocks `US-0097`'s docs.rs and README links", is also `[ ]`.
The intake shows exactly what a recorded ruling looks like: Open Decision 1 is `[x]` and carries
"**Owner ruling (2026-09-15): ...**".

So the packet that freezes a public contract shipped the default `XTVERSION` string, the publish
policy and the docs.rs/GitHub links on rulings that exist nowhere in the record. Either the intake
rows are ticked with their ruling text, or the packet stops asserting them.

### F7 (Medium) The new publish step makes `ci-local` unrunnable with uncommitted work, and adds a network dependency

`scripts/ci-local.sh:44`, `scripts/ci-local.ps1:53`: `cargo publish -p oneterm-vt --dry-run`.

Reproduced with a single untracked file under `crates/vt/tests/`:

```
    Updating crates.io index
error: 1 files in the working directory contain changes that were not yet committed into git:
crates\vt\tests\verify_us0097_product_name.rs
to proceed despite this and include the uncommitted changes, pass the `--allow-dirty` flag
EXIT=101
```

The gate aborts there, before all six Python policy checks. `AGENTS.md` section 4 instructs every
agent to run this script *before completing a task* -- that is, with uncommitted work in the tree,
which is now the one state in which it cannot pass. The step also prints "Updating crates.io index",
so `ci-local` now needs network where it previously did not (the `third-party-notices` step is
advertised as offline).

Fix: `--allow-dirty` in the two local scripts only. `.github/workflows/ci.yml` checks out clean and
must keep the strict form.

### F8 (Low) Evidence over-claims in the packet, item by item

| Packet text | Measured at `2e1495c` / `main@92ae9a6` |
| --- | --- |
| "Six module headers keep their design document as an absolute `https://github.com/...` link" | 24 such rustdoc lines in 23 files |
| "both `tests/*.rs` headers were rewritten too" | `crates/vt/tests/parser_limits.rs:11` still reads "the intake's headline defect (`IN-0029` P1 ...)" in a `//!` line, and it ships in the package |
| "The one internal record id that would have shipped was a file name, `tests/us0087_cleanup_rows.rs`" | `src/terminal/verify_bug0058_tests.rs` also ships; its module name is spelled in `src/terminal/mod.rs:24-25` |
| "The CI grep also covers `BUG-NNNN` ... it caught four more lines, all of them `BUG-0058`" | 8 additional lines; four are `BUG-0051` (`reflow/columns.rs:261`, `reflow/reflow_tests.rs:537,720,726`) |
| "Packaged 64 files, 857.7KiB (219.0KiB compressed)" | 64 files confirmed; 874.1 KiB (219.2 KiB compressed) at HEAD |
| Harness snippet `evidence=`: "packages 63 files" | contradicts the packet's own Acceptance, which says 64 |

Measured correct and re-derived independently: the 106-line / 44-file baseline (exactly 106/44 on
`main` @ `92ae9a6` with the packet's pattern), the 0-line post-state under the acceptance grep,
764 lines in `public-api.txt`, 374 + 6 + 3 test counts, and the headless example output byte for
byte.

### F9 (Low) A botched rewrite left a dangling doc fragment

`crates/vt/src/grid/row.rs:427-428`

```rust
    /// `DCH`: a plain shift left by `n`, not an `end`-clamped swap.
    /// `end`-clamped swap.
    pub(crate) fn delete_cells(&mut self, col: u16, n: u16, template: Cell) {
```

The pass replaced the first line of a two-line sentence and left the second behind. `pub(crate)`, so
it never renders on docs.rs, but it ships in the tarball and reads as an error.

### F10 (Low) The three CI lists are not quite the same set

`.github/workflows/ci.yml:169-170` runs `cargo run -p oneterm-vt --example headless`.
`scripts/ci-local.sh:37-50`, `scripts/ci-local.ps1:40-66` and `AGENTS.md` section 4 do not.

Everything else matches, in the same relative order, in all three: the two feature-matrix builds
with `--examples`, the doc build under `RUSTDOCFLAGS=-D warnings`, `vt-public-api.py --check --no-doc`,
`cargo publish --dry-run`, then the self-containment grep. The `vt-package` job is `runs-on:
ubuntu-latest` (`ci.yml:142`), so it is not Windows-only by accident; the two bash/PowerShell greps
are equivalent, and both correctly treat "matches exist but all are github.com links" as a pass.

### F11 (Low) Rustdoc inaccuracies found reading the rewritten pages as an outsider

Six pages read closely (`terminal::Config` and `Terminal`, `cell`, `events::vt_event`, `grid::row`,
`grid` root types, `render::state` and `render` root, `graphics`). The rewrites are generally strong:
every load-bearing invariant I spot-checked survived its citation being stripped -- the `graphics`
module's three rules, `Cell::is_blank` vs `is_erasable`, `RowMut::repair`'s wrap-flag reasoning,
`take_graphics` being the only drain, and `RowId`'s identity guarantee (`grid/mod.rs:42-46`), which
is the one the CHANGELOG promises. The following are small and real:

- (a) `Config::scrollback_limit` (`terminal/mod.rs:70-72`) says "clamped to `SCROLLBACK_MAX`", but
  `Terminal::set_scrollback_limit` (`mod.rs:385-387`) stores the raw value in `Config`. Only
  `Screen` clamps (`grid/screen.rs:307,1621`), so `terminal.config().scrollback_limit` can report a
  number the grid never used.
- (b) `Terminal::colors` (`mod.rs:507-509`) says "`OSC 4`, `OSC 10` or `OSC 11`", but `ColorKey`
  (`terminal/color.rs:25-40`) also covers `OSC 12` (Cursor) plus the Bright and Dim keys.
- (c) `Terminal::new` (`mod.rs:156-158`) does clamp its `Size` but the doc does not say so, while
  `Screen::new`'s does. An embedder reading `Size`'s "clamped ... by `Size::clamped`" may think the
  call is theirs to make.
- (d) `VtEvent::Repaint` (`events/vt_event.rs:41-45`) now says damage "is the per-row sequence number
  a snapshot reports" and dropped the intra-doc link to `RenderState`, which is the type the reader
  has to find next. The word "snapshot" also does not match any name in today's API (the rename is
  `US-0101`'s).
- (e) `RowHeader::occ` (`grid/row.rs:62-65`) dropped "deliberately **not** checked by
  `assert_integrity`" along with its `(R-10)` tag. The statement itself carries no record id and was
  worth keeping.

### F12 (Info) A `Cargo.toml` comment ships internal record ids

`crates/vt/Cargo.toml:20-27` cites "IN-0029 R-28, `testing-and-bench.md` M12" in the `[features]`
comment. `Cargo.toml.orig` ships in the tarball, so an outside reader sees it. The self-containment
rule as written covers `///` and `//!` only, so this is inside the rule; flagged because it is the
same class of text reaching the same reader.

### F13 (Low) The owning contract documents were not reconciled with what was built

- `low-level-design/packaging.md` section "Manifest" still shows `[features] default = []`, `regex`
  and `serde`, and a `description` naming key/mouse encoding and OSC handling. None of that is in
  `crates/vt/Cargo.toml`, correctly -- but the contract document still specifies it.
- `low-level-design/api-surface.md` section "Verification" still says `vt-public-api.py` runs
  `cargo doc --output-format json` and is "about 40 lines". It reads HTML and is 130 lines.
- `packaging.md` section "README" point 4 says the quick-start block is "the twenty-line body of
  `examples/headless.rs` ... so it cannot compile in the example and rot in the README". The
  README's block is a separate, shorter snippet. Both compile, but the stated anti-drift property is
  not the one that was built.

The packet discloses each deviation in its own prose; the documents an agent reads first were not
updated.

### F14 (Low) `AGENTS.md` was edited but is not in the packet's Reconciliation list

`git diff main..HEAD -- AGENTS.md` adds seven lines to section 4. The packet's Reconciliation
"Changed" list names `docs/PROJECT.md`, `docs/agents/structure.md`, `docs/agents/dependencies.md`,
`scripts/README.md`, `ci.yml` and the two `ci-local` scripts, but not `AGENTS.md`. The edit itself is
correct and useful.

## What was verified clean

**Claim 1 -- only `oneterm-vt` is publishable.** `grep -rn publish crates/*/Cargo.toml` yields 20
`publish.workspace = true` lines plus one `publish = true` at `crates/vt/Cargo.toml:15`. The
assertion is at `scripts/verify-dependency-graph.py:117-127` and reads `cargo metadata`'s
`publish: null`. Break test: deleting `publish.workspace = true` from `crates/pty/Cargo.toml` (the
"crate omits the key" case) makes the script exit 1 with
`oneterm-vt must be the only publishable crate; found ['oneterm-pty', 'oneterm-vt']`. Restored, then
`Dependency graph policy passed for 21 workspace packages and 21 explicit members.`

**Claim 2 -- packaging.** `cargo publish -p oneterm-vt --dry-run` exits 0:
`Packaged 64 files, 874.1KiB (219.2KiB compressed)`, `Verifying`, `Finished`, `warning: aborting
upload due to dry run`. `cargo package --list -p oneterm-vt` returns the same 64 paths and contains
`README.md`, `CHANGELOG.md`, `examples/headless.rs` and `public-api.txt`. No path is outside
`crates/vt`. `fuzz/` is absent because it declares its own `[workspace]` table
(`crates/vt/fuzz/Cargo.toml`). `target/package/oneterm-vt-0.5.2/Cargo.toml` has six registry
dependencies, one dev-dependency, and **no** path dependency.

**Claim 3 -- self-containment and rustdoc.** The packet's acceptance grep over `crates/vt/src`,
minus `https://github.com/` lines, returns **0**. `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt
--no-deps --all-features` exits 0 with no warning. `#![warn(missing_docs)]` is at
`crates/vt/src/lib.rs:16` and `grep -rn 'allow(missing_docs)' crates/vt` returns nothing. The
extensions to the claim (tests, examples, Markdown) are covered in F8(b) and F1.

**Claim 4 -- README.** All three Rust blocks are doctests of the file itself
(`crates/vt/src/lib.rs:20-22`, `#[cfg(doctest)] #[doc = include_str!("../README.md")]`);
`cargo test -p oneterm-vt --doc` runs three and passes. Break test: deleting one token from each
(`render_update` -> `render_updat` at line 62, `claim` -> `clam` at 83, `product_name` ->
`product_nam` at 105) fails all three with `E0599`, `E0599` and `E0560`. Reverted. Badges
(`README.md:3-4`) point at `crates.io/crates/oneterm-vt` and `docs.rs/oneterm-vt`. "What it is not"
matches the HLD's "What stays outside" table one-for-one. The MSRV paragraph
(`README.md:127-134`) agrees with `CHANGELOG.md:15` (rule 3). "no `unsafe` block in the library
itself" (line 35) is true -- the only `unsafe` in the crate is the counting allocator in
`tests/parser_limits.rs`. The non-Rust blocks are a dependency tree and a `cargo run` line. F1 is
the exception.

**Claim 5 -- `product_name`.** Re-measured independently, not taken from the packet.
`Config::default()` -> `ESC P > | oneterm-vt(0.5.2) ESC \`. `Some("X")` -> `ESC P > | X ESC \`.
`Some("OneTerm(0.5.2)")` -> `ESC P > | OneTerm(0.5.2) ESC \` and `DA2` -> `CSI > 0 ; 502 ; 1 c`, which
is byte-identical to what `main` replies today. A name with no parsable trailing version falls back
to the engine's number rather than zero. `crates/terminal/src/handle.rs:208` sets
`Some(concat!("OneTerm(", env!("CARGO_PKG_VERSION"), ")"))`, so OneTerm's bytes are unchanged. Pinned
by `product_name_owns_xtversion_and_da2` and `da1_da2_dsr_xtversion_answers`. Hostile inputs: F3, F4.

**Claim 6 -- the example.** `cargo run -p oneterm-vt --example headless` prints exactly the block
pasted into the packet, including `104 bytes fed, 0 unhandled sequences` and the four framed rows.
The README does not quote the output, so there is nothing for it to disagree with (`README.md:66-71`
only shows the invocation).

**Claim 7 -- the surface script.** Covered in F5. Determinism: two consecutive
`python scripts/vt-public-api.py --no-doc` runs diff clean. `--check --no-doc` against the committed
764-line file: `public API surface unchanged`. One caveat beyond the admitted signature blindness:
`--no-doc` trusts whatever is in `target/doc`, so run out of order it can validate stale HTML; a
rebuild does prune removed pages, which I confirmed.

**Claim 8 -- CI parity.** Covered in F10.

**Claim 9 -- regression.** `cargo test --workspace` exit 0, every suite green.
`cargo test -p oneterm-vt --features vt-paranoid` -> 374 + 6 + 6 + 3, all green.
`cargo test -p oneterm-tools --test corpus_check` -> 2 passed.

The non-doc diff of `crates/vt/src` (`git diff main..HEAD -- crates/vt/src`, dropping `///`, `//!`,
`//` and blank lines) is 75 lines, and every production-code hunk is one of:

1. `crates/vt/src/lib.rs:16,20-22` -- `#![warn(missing_docs)]` and the `ReadmeDoctests` struct.
2. `crates/vt/src/events/vt_event.rs` -- two rustfmt reformats only (`CellContent::Cluster` and
   `RenderUpdate::Partial` expanded to multi-line because their fields gained doc comments). No
   semantic change.
3. `crates/vt/src/terminal/dispatch.rs` -- new `product_name()`, `product_version_number()`,
   `ENGINE_PRODUCT_NAME`, `trailing_version()`; the `DA2` site now calls
   `self.product_version_number()` instead of `version_number(env!("CARGO_PKG_VERSION"))`; the
   `XTVERSION` site now formats `self.product_name()` instead of the hard-coded `OneTerm({version})`.
4. `crates/vt/src/terminal/mod.rs` -- `use std::borrow::Cow`, the `product_name` field, and
   `product_name: None` in `Default`.
5. `crates/vt/src/terminal/terminal_tests.rs` -- the new test, and the one literal in
   `da1_da2_dsr_xtversion_answers` that changed from `OneTerm(..)` to `oneterm-vt(..)`.

Nothing else. The claim "no behaviour change except the two reply strings" holds.

**Claim 10 -- records and scripts.** `python scripts/check-english.py` (829 files),
`check-doc-paths.py` (195 paths in 11 documents), `third-party-notices.py --check`,
`verify-dependency-graph.py` and `completion-catalog.py validate` all pass. The harness snippet was
checked against a read-only copy of `D:\TrungKFC-Research\Rust\myTerm2\harness.db`: the `story`
table has exactly the 17 columns the snippet's `ROW` dict names, in a compatible set;
`intake_id = 43` is correct (`intake.id 43` <-> `document_number 38` <-> the IN-0038 doc path); the
`0/1` proof flags match the packet's PROOF block and the convention of the neighbouring rows
(`BUG-0058` is `1,1,0,1` on intake 43). No `US-0097` row exists, which the packet discloses. The
`63 files` string inside the snippet is F8. The 106/44 vs 105/37 baseline discrepancy is disclosed
and, independently measured at `main@92ae9a6`, the 106/44 figure is exact. Over-ticking beyond that
is F6 and F8.

**Claim 11 -- the gate.** `pwsh scripts/ci-local.ps1 -Full` on a clean tree:

```
advisories ok, bans ok, licenses ok

ci-local: all checks passed.
```

exit 0.

## What could not be verified

- **The E2E walk.** The owner runs this session inside OneTerm, so driving the GUI to watch a real
  `tmux -V` read the `XTVERSION` string is unsafe here. The packet discloses this and `e2e_proof=0`
  is the honest flag. The reply is pinned by unit tests against exact byte strings and OneTerm's
  half is one `concat!`, but nobody has watched a real program consume it.
- **The docs.rs sandbox build.** No offline sandbox here. Inferred safe: pure Rust, no build script,
  six leaf dependencies, `cargo doc --all-features` clean locally.
- **MSRV 1.96.0.** No 1.96.0 toolchain is installed and CI does not build at the MSRV either. The
  `rust-version` field remains a claim, which `README.md:130-131` says in as many words.
- **crates.io name availability** for `oneterm-vt` -- the registry was not queried.
- **The `857.7 KiB` figure** at whatever commit the packet measured it on; at `2e1495c` it is
  874.1 KiB.

## Commands run

```
git reset --hard feat/vt-publishable                       # 2e1495c, merge-base 92ae9a6
python scripts/verify-dependency-graph.py                  # clean, and with publish key removed from crates/pty
cargo build --workspace --all-targets
cargo publish -p oneterm-vt --dry-run [--allow-dirty]
cargo package --list -p oneterm-vt
RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps --all-features
python scripts/vt-public-api.py --check [--no-doc]         # clean, and against two break tests
python scripts/vt-public-api.py --no-doc                   # twice, byte-identical
cargo run -p oneterm-vt --example headless
cargo test -p oneterm-vt --doc                             # clean, and with one token deleted per block
cargo test -p oneterm-vt --test verify_us0097_product_name
cargo test --workspace
cargo test -p oneterm-vt --features vt-paranoid
cargo test -p oneterm-tools --test corpus_check
python scripts/check-english.py
python scripts/check-doc-paths.py
python scripts/third-party-notices.py --check
pwsh scripts/ci-local.ps1 -Full
grep -rn '^[[:space:]]*//[/!].*\(US-0...\|BUG-0...\|DEC-0...\|IN-0...\|docs/spec-intakes\)' crates/vt/src
  # at HEAD and against a `git archive main` copy, with and without the BUG- alternative
sqlite3 (read-only copy of harness.db): schema of `story` and `intake`, rows for intake 43
```

## Tests written by this verification

`crates/vt/tests/verify_us0097_product_name.rs` -- six tests, public API only, uncommitted and in
the verifier's worktree. Four re-measure the packet's own claims independently; two are the hostile
cases the packet does not cover.

| Test | What it pins |
| --- | --- |
| `verify_default_identity_is_the_engine` | the three documented reply strings, re-derived |
| `verify_hostile_product_name_breaks_dcs_framing` | F3: an injected `ESC \` truncates the name and leaks bytes |
| `verify_c1_st_and_bel_in_product_name` | F3: `0x9c` and `BEL` pass through verbatim |
| `verify_product_name_length_is_unbounded` | F3: a 64 KiB name yields a 65 542-byte reply |
| `verify_da2_version_packing_collides_above_99` | F4: `1.1.0` and `1.0.100` answer the same number |
| `verify_da2_version_overflows_on_a_huge_major` | F4: `P(429497.0.0)` panics in debug |

All six pass at `2e1495c`. The file must be removed or committed before `scripts/ci-local.*` can run
in this worktree (F7).

## Recommendation

Close F1, F2, F3, F4 and F6 before the owner's first real `cargo publish`; F1 and F2 are what an
outside reader hits first, F6 is what the record is for. F5 and F7 are cheap and should land with
them, because both gates will otherwise be worked around rather than used. F8 through F14 are
accuracy and tidiness and can ride on the next packet in the intake.

---

# Re-verification at `3fd76f7`

Branch under test: `feat/vt-publishable` @ `3fd76f7`, base `main` @ `92ae9a6`.
Same worktree, same rules: nothing pushed, `cargo publish` never run in any form, `harness.db` not
written, no GUI, no commits to the implementer's branch.

Everything above this line is the first pass at `2e1495c` and is left unedited. This section
re-checks each finding against the reworked code, adds the checks the coordinator asked for, and
adds what a real consumer outside the workspace found.

## Re-verification verdict

**PASS-WITH-NOTES.**

Thirteen of the fourteen findings are closed, and closed properly: with code, with a gate that fails
when the property is removed, and with the numbers re-measured rather than re-asserted. The
`product_name` sanitiser held against every hostile value I could construct, including several the
adopted suite does not cover.

Two things remain, one of them new and only a real consumer could find it.

- **R1 (Medium).** The README's Install block does not work, in two independent ways. It is the one
  instruction a stranger follows first, and after the no-publish ruling it is the crate's only
  supported consumption path.
- **R2 (Medium).** The decisions are now recorded, which was the substance of F6. They are recorded
  as "Owner ruling 2026-09-15" in thirteen places across seven files, including a comment in shipped
  source -- while the packet's own close-note says they were recorded with the provenance
  "Coordinator default 2026-09-15, presented to the owner, no objection recorded". That string
  appears nowhere in the repository.

Four low notes and two informational ones follow them.

## The rulings: recorded and implemented?

| Ruling | Recorded | Implemented |
| --- | --- | --- |
| No crates.io publish | `IN-0038.md:286-294`, Open Decision 2 ticked | `crates/vt/Cargo.toml:14` explicit `publish = false`; no `[package.metadata.docs.rs]`; `scripts/verify-dependency-graph.py:165-176` asserts the publishable set is **empty** |
| No badge or docs.rs link | `packaging.md:108-110,143-146` | no `crates.io`, `docs.rs` or `shields.io` string anywhere in `crates/vt/` except two sentences explaining their absence; the intake's remaining hits are ruling text, my own report, or R5 |
| Git-dependency Install section | `packaging.md:111-115` | `crates/vt/README.md:6-18`, the first section after the title |
| No `serde` | `IN-0038.md:295-299`, Open Decision 3 ticked | no `serde` dependency and no `serde` feature in `crates/vt/Cargo.toml` |
| `product_name` default `oneterm-vt(<version>)` | `IN-0038.md:300-307`, Open Decision 5 ticked | `dispatch.rs:791` `ENGINE_PRODUCT_NAME`; re-measured below |
| Monorepo | `IN-0038.md:308-313`, Open Decision 6 ticked | every repository link is an absolute `https://github.com/vnStrawHat/OneTerm/...` URL; the Install block names the same repository |
| `cargo publish --dry-run` gone | -- | absent from `.github/workflows/ci.yml`, `scripts/ci-local.sh`, `scripts/ci-local.ps1` and `AGENTS.md`; replaced by `cargo package` plus the file-set check |

Open Decision 4 (MSRV policy) is still `[ ]`. The intake says it blocks nothing and the packet
carries the MSRV gap in Gaps, so that is consistent.

**Break test on the new publish assertion.** Setting `publish = true` on `crates/vt/Cargo.toml`
makes `python scripts/verify-dependency-graph.py` exit 1 with
`no crate in this workspace is published; found publishable: ['oneterm-vt']`. Restored, it passes.

## The packaging gate

```console
$ cargo package -p oneterm-vt --allow-dirty --list | python scripts/verify-dependency-graph.py --package-list -
Dependency graph policy passed for 21 workspace packages and 21 explicit members.
Package set passed: the oneterm-vt package carries CHANGELOG.md, LICENSE, NOTICE, README.md,
examples/headless.rs, and reaches nothing outside crates/vt.
```

67 files on a clean tree, 68 with my own test file present, which is the difference.
`crates/vt/LICENSE` and `crates/vt/NOTICE` are byte-identical to the repository root's.

Three negative tests, all run:

| Injected fault | Result |
| --- | --- |
| a file list with no `LICENSE` / `NOTICE` | exit 1, `the oneterm-vt package does not contain LICENSE` and `... NOTICE` |
| a file list containing `../../LICENSE` | exit 1, `the oneterm-vt package reaches outside crates/vt: ['../../LICENSE']` |
| `crates/vt/LICENSE` actually moved away, real `cargo package --list` piped in | exit 1, `the oneterm-vt package does not contain LICENSE` |

Restored after each.

## R1 (Medium, new): the README's Install block does not work

A scratch consumer was built **outside** the workspace, in the session scratchpad, with
`cargo init --name vtconsumer`, depending on the crate exactly as the README says to and running the
README's first code block verbatim plus the Identity section's `product_name` field. It was deleted
afterwards.

### (a) The tag the README names resolves to a commit with no crate in it

```console
$ # oneterm-vt = { git = "file:///D:/TrungKFC-Research/Rust/myTerm2", tag = "v0.5.2" }
$ cargo build
    Updating git repository `file:///D:/TrungKFC-Research/Rust/myTerm2`
error: no matching package named `oneterm-vt` found
location searched: Git repository file:///D:/TrungKFC-Research/Rust/myTerm2?tag=v0.5.2
```

`v0.5.2` is `5c351d2`, and `git ls-tree -r v0.5.2 -- crates/vt` is **empty**: the engine landed
after that tag. The README's one copy-pasteable line, its first code block, fails outright.

Picking a different existing tag does not help either. `crates/vt` keeps `version.workspace = true`
and the workspace is still at `0.5.2`, so no tag that could carry this crate exists until the
application releases `0.5.3`. The README should name the tag that will carry it and say it does not
exist yet, or show a `rev`, which does.

### (b) On Windows the git dependency fails under the default `CARGO_HOME`

With `branch = "feat/vt-publishable"` instead:

```console
$ cargo build
    Updating git repository `file:///D:/TrungKFC-Research/Rust/myTerm2`
error: failed to get `oneterm-vt` as a dependency of package `vtconsumer v0.1.0 (...)`
Caused by:
  path too long: 'C:/Users/trunglt/.cargo/git/checkouts/myTerm2-4081fd9f879fe89a/3fd76f7/docs/
  spec-intakes/IN-0009-prevent-terminal-completion-from-slicing-strings-at-invalid-utf-8-boundaries-
  when-matching-prefixes-from-history-or-catalogs/BUG-0011-prevent-unicode-prefix-match-crash.md';
  class=Filesystem (30)
```

(One line in reality; wrapped here.) 268 characters. A git dependency makes cargo check out the
**whole repository**, `docs/` included, and one of OneTerm's own intake directory names is 118
characters long.

This is not the consumer's environment:
`HKLM\SYSTEM\CurrentControlSet\Control\FileSystem\LongPathsEnabled` is `1` on this machine and
`git config core.longpaths` is `true`, and libgit2 -- which is what cargo uses -- honours neither.

Proof that path length is the whole cause: the identical build with `CARGO_HOME=C:\ch`, which
removes 28 characters of prefix, succeeds.

Windows is, in this repository's own words, "OneTerm's primary and only QA-tested platform"
(`crates/vt/fuzz/Cargo.toml`). The crate's only supported consumption path is broken there, for any
consumer who has not moved `CARGO_HOME`, by the repository's own directory naming.

### What did work

With a short `CARGO_HOME`, everything the packet promises a stranger:

```console
$ cargo build           # cold, including fetching the six dependencies from crates.io
   Compiling oneterm-vt v0.5.2 (file:///D:/.../myTerm2?branch=feat%2Fvt-publishable#3fd76f79)
   Compiling vtconsumer v0.1.0 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 10.61s

$ cargo run -q
rows: 24
title: Some("a title")
identity replies: "\u{1b}P>|vtconsumer(9.9.9)\u{1b}\\\u{1b}[>0;90909;1c"
```

**10.8 s wall clock for the first build**, from an empty `CARGO_HOME`. The README's first block
compiles unmodified outside the workspace, `Config::product_name` is reachable and works, and
`XTVERSION` and `DA2` answer the consumer's own identity. The six-leaf dependency claim is visible
in the build log: nothing else was compiled.

## R2 (Medium; F6 half-open): the rulings are recorded as the owner's

F6's substance is closed. Open Decisions 2, 3, 5 and 6 are `[x]` with dated text, the code matches
each one, and the packet no longer leans on rulings that exist nowhere.

What is not closed is the attribution. The packet's own reconciliation table says:

> `IN-0038.md` Open Decisions 2, 3, 5 and 6 are ticked with their ruling text and the provenance
> "Coordinator default 2026-09-15, presented to the owner, no objection recorded". The packet no
> longer calls them owner rulings.

`grep -rn 'Coordinator default'` over the repository returns **nothing**. `grep -rc 'Owner ruling'`
returns 13 matches in 7 files:

| File | Matches |
| --- | --- |
| `docs/spec-intakes/IN-0038-embeddable-vt-core/IN-0038.md` | 5 |
| `.../high-level-design.md` | 2 |
| `.../low-level-design/packaging.md` | 2 |
| `.../US-0097-publishable-crate.md` | 1 |
| `.../US-0103-embedder-guide.md` | 1 |
| `crates/vt/Cargo.toml` | 1 |
| my own report above | 1 |

`docs/PROJECT.md:31` and `docs/agents/structure.md:254` carry the same phrase in lower case.

So the attribution is not only unchanged from what F6 objected to, it is stronger and copied into
more places -- including a source comment that ships to every consumer of the crate. Whoever wrote
the close-note understood that the distinction mattered, and then did not write it down.

This verifier has no evidence that the owner ruled on anything. What is on the record is that an
agent wrote "Owner ruling" into seven documents. Either the owner confirms these five rulings and
the text stands, or the provenance the packet itself proposed gets written. The second is a
search-and-replace; the first is a question only the owner can answer.

## R3 (Low, new): the public-API gate now under-reports nested public modules

`scripts/vt-public-api.py:63-65` keeps a page only when its module path is `"."` or one of the
single-segment `pub mod` names it reads out of `lib.rs`. A **nested** public module is therefore
dropped silently.

Proven: adding

```rust
/// Verifier probe: a nested public module.
pub mod probe {
    /// Verifier probe type.
    pub struct Probe;
}
```

to `crates/vt/src/grid/mod.rs` -- a brand-new public path `oneterm_vt::grid::probe::Probe` -- leaves
`python scripts/vt-public-api.py --check` at `public API surface unchanged`, **exit 0**. Reverted.

The fix for F5 swung from over-reporting to under-reporting. It does not bite today, because the
three `pub mod`s are flat, and `US-0099` / `US-0100` add `pub mod input` and `pub mod search` at the
crate root, which are caught. Anything nested inside those would not be. Collecting module paths
transitively, or accepting a path whose every segment is a public module, closes it.

## R4 (Low, new): the sanitiser is re-run per query and is not bounded by its own cap

`sanitize_product_name` (`dispatch.rs:803-815`) is called from `product_name()`, which runs on every
`XTVERSION` and every `DA2`. It iterates `name.chars()` over the **whole** configured string,
because a dropped control never advances `out.len()` and so never reaches the `break` the 64-byte
cap provides.

Measured (`verify_sanitising_is_repeated_per_query_and_is_not_bounded_by_the_cap`, debug build,
2 000 `CSI > 0 q` queries in one `feed`):

| `product_name` | wall clock |
| --- | --- |
| 1 MiB of `ESC` | 11.64 s |
| `"Prod(1.0.0)"` | 1.9 ms |

`product_name` is the embedder's value, not the stream's, so this is a cost note and not a
vulnerability: no real name is long, and for a real name the loop ends at 64 bytes. Recorded because
sanitising once in `Terminal::new` removes both the walk and the per-query `String` allocation, and
the field is `Cow<'static, str>` precisely so that cost can be paid once.

## R5 (Low, new): `IN-0038.md`'s body still describes the pre-ruling plan

The Open Decisions section is correct; the prose above it was not updated and now contradicts it:

| Line | Text |
| --- | --- |
| `IN-0038.md:41` | "`crates/vt` publishes to crates.io (`publish = true`)" |
| `IN-0038.md:186-187` | "after `US-0097` a crates.io release exists and semver applies" |
| `IN-0038.md:195` | "Packaging gains a crates.io publish path (`US-0097`)" |
| `IN-0038.md:237` | the `US-0103` row: "rendered to HTML by `cargo doc` and by docs.rs" |
| `IN-0038.md:257-258` | "**External systems and side effects.** One new one: crates.io. `US-0097` makes the crate publishable" |
| `IN-0038.md:262` | "see ... `packaging.md` for MSRV and docs.rs" |

`high-level-design.md`, `packaging.md` and `api-surface.md` were all reconciled; the intake itself
was ticked but not re-read.

## R6 (Low): residual numbers that still disagree with each other

- `US-0097-publishable-crate.md:57` and `:241` say `876.3 KiB`; the F8 row at `:357` says
  `875.6 KiB`.
- `:59` says `public-api.txt` ships "at 20 KiB"; `:255` says "at 19 KiB"; the file is 16.6 KiB.
- The F3 row at `:352` says the sanitiser is "Pinned by five tests";
  `crates/vt/tests/product_name.rs` has eight.
- Evidence says "**23** module headers ... one per file"; measured 24 rustdoc lines across 23 files,
  so one file carries two.

Every one of these is an order of magnitude smaller than the errors F8 raised, and the direction of
travel is right. Listed so the next packet's Evidence starts from measured numbers.

## R7 (Info): the package gate's exit status comes from the pipeline's last command

`scripts/ci-local.sh:47-52`, `scripts/ci-local.ps1:56-62` and `.github/workflows/ci.yml:181-184` all
run `cargo package ... --list | python scripts/verify-dependency-graph.py --package-list -` and
check the status of the pipeline, which in both shells is python's. A `cargo package` failure is
caught only because python then reads an empty list and says
`--package-list read an empty file list` -- which it does, so the path is covered. It is covered by
accident rather than by design; `set -o pipefail` around that one line, or `$PIPESTATUS`, would make
it deliberate.

## R8 (Info, not this packet): one workspace test is load-sensitive

`cargo test --workspace` failed once, on
`oneterm-terminal handle::tests::a_pump_yields_to_the_demand_within_a_bounded_number_of_chunks`,
while this machine was saturated. It passed **3/3** re-run in isolation, and again inside the green
`ci-local -Full` run below. It is a two-thread scheduling test with a hard-coded
`sleep(Duration::from_micros(250))` (`crates/terminal/src/handle.rs:315-345`), and nothing in this
packet's production diff reaches it. Recorded so the next person who sees it red does not chase
`US-0097`.

## Each earlier finding, re-checked

| # | State | Evidence at `3fd76f7` |
| --- | --- | --- |
| F1 dead guide link | **closed** | `README.md:135-136`: "An embedder's guide is planned ... It does not exist yet." No `docs.rs` URL anywhere in `crates/vt/`; `:132` points at `cargo doc` and `:133-134` says why there is no docs.rs page. |
| F2 no licence in the tarball | **closed** | `crates/vt/LICENSE` and `crates/vt/NOTICE` present and byte-identical to the root's; both in the 67-file package list; all three negative tests above fail correctly. |
| F3 `product_name` framing | **closed** | `sanitize_product_name` drops `0x00..=0x1f`, `0x7f` and `0x80..=0x9f`, cuts to 64 bytes on a character boundary, and an empty result falls back to the engine. My seven new hostile tests all pass; table below. |
| F4 `DA2` panic and collision | **closed** | `version_number` clamps each component with `.min(99)` before packing. `P(429497.0.0)` answers `999999`-shaped bytes, `P(4294967295.4294967295.4294967295)` answers `ESC [ > 0 ; 999999 ; 1 c`, and `P(4294967296.0.0)` and `P(99999999999999999999.0.0)` fall back to the engine number. No panic in a debug build in any case I could construct. The collision is documented on the field and on `version_number`. |
| F5 `public-api.txt` | **closed**; see R3 | 688 lines, 100 items, only the four nameable prefixes (`oneterm_vt`, `::grid`, `::intern`, `::parser`). Private `parser::params` -> `parameters` rename: **exit 0**, "public API surface unchanged". Public `cluster_width` -> `cluster_width_probe`: **exit 1** with the expected two-line diff. Two consecutive runs byte-identical. |
| F6 unrecorded rulings | **half closed**; see R2 | decisions recorded and implemented; attribution unresolved. |
| F7 `ci-local` and a dirty tree | **closed** | `cargo publish --dry-run` gone everywhere; the gate is `cargo package --allow-dirty --list` piped into the file-set check, and it is offline. Proven by a green `-Full` run with an untracked file under `crates/vt/`. |
| F8 evidence over-claims | **closed**; see R6 | 23 files re-counted, 67 files, the `BUG-` lines identified as plain `//` comments, `tests/parser_limits.rs` cleaned (the grep over `crates/vt/tests` now returns nothing but my own file), `verify_bug0058_tests.rs` renamed to `dcs_routing_tests.rs`, the harness snippet says 67. |
| F9 dangling doc fragment | **closed** | `crates/vt/src/grid/row.rs:430` is one line. |
| F10 three CI lists differ | **closed** | `cargo run -p oneterm-vt --example headless` is in `ci.yml:169`, `ci-local.sh:42`, `ci-local.ps1:45` and `AGENTS.md` section 4, which also names the packaged-file-set step. One residual difference: `ci.yml:179-180` runs `cargo package -p oneterm-vt`, which compiles the packaged copy, before the `--list` check; the local scripts run only `--list`, so they do not prove the packaged crate builds standalone. |
| F11 rustdoc inaccuracies | **closed**, all five | `Config::scrollback_limit` says the grid clamps and the field reports what you asked for; `Terminal::colors` names `OSC 12` and the bright and dim keys; `Terminal::new` says it clamps; `VtEvent::Repaint` links `Terminal::render_update` and `RenderState`; `RowHeader::occ` has its integrity-assertion sentence back. |
| F12 record ids in `Cargo.toml` | **closed** | the `[features]` comment carries no record id and no internal document name. |
| F13 contract docs not reconciled | **closed** | `packaging.md:27-47` shows the manifest as shipped; `:122-131` explains the whole-file doctest route and says it is not what the document first proposed; `api-surface.md:222-223` says the script reads HTML, "not `cargo doc --output-format json` as this document first proposed". |
| F14 `AGENTS.md` missing | **closed** | listed in Reconciliation. |

## `product_name`, attacked again

`crates/vt/tests/verify_us0097_rework.rs`, seven tests, all passing, public API only:

| Test | Attack | Result |
| --- | --- | --- |
| `verify_a_four_byte_character_straddling_the_cap_is_dropped_whole` | 61, 62 and 63 ASCII bytes then U+1F600, so the character crosses byte 64 | dropped whole; the reply stays valid UTF-8; 60 + 4 = 64 fits exactly |
| `verify_the_cap_stops_rather_than_skipping_to_a_shorter_character` | an oversized character followed by three ASCII bytes that would have fitted | `break`, not `continue`: the tail is not appended. Worth knowing, not wrong |
| `verify_control_only_names_all_fall_back_to_the_engine` | every C0 (`0x00-0x1f`), `DEL`, every C1 (`0x80-0x9f`), and all three concatenated | all four fall back to `oneterm-vt(<version>)`, and `DA2` follows the same fallback, so the two answers cannot disagree |
| `verify_da2_handles_u32_max_and_beyond` | `u32::MAX` in every component, `u32::MAX + 1`, a 20-digit major, negative and empty components | saturates or falls back; no panic in a debug build in any case |
| `verify_controls_do_not_consume_the_byte_budget` | 64 `ESC` bytes then `Prod` | answers `Prod`: the cut is applied after the drop, not before |
| `verify_da2_reads_only_the_trailing_parenthesis_group` | `P(1.2.3)(4.5.6)` and `(1.2.3)P` | reads the trailing group only, and falls back when there is no trailing group |
| `verify_sanitising_is_repeated_per_query_and_is_not_bounded_by_the_cap` | 2 000 queries against a 1 MiB control-only name | R4 |

The `ESC \`, `0x9c`, `BEL`, empty-name and 64 KiB cases from the first pass are now in the crate's
own `crates/vt/tests/product_name.rs` (8 tests), adopted and rewritten by the implementer, and all
8 pass.

## Production diff, re-listed

`git diff main..HEAD -- crates/vt/src`, dropping `///`, `//!`, `//` and blank lines: **95 lines**,
and every hunk is one of:

1. `lib.rs:16,20-22` -- `#![warn(missing_docs)]` and the `ReadmeDoctests` struct.
2. `events/vt_event.rs` -- two rustfmt reformats only (`CellContent::Cluster` and
   `RenderUpdate::Partial` expanded to multi-line because their fields gained doc comments). No
   semantic change.
3. `terminal/dispatch.rs` -- `product_name() -> String`, `product_version_number()`,
   `ENGINE_PRODUCT_NAME`, `PRODUCT_NAME_MAX`, `sanitize_product_name`, `trailing_version`, the
   `.min(99)` added to `version_number`'s component map, and the two reply sites.
4. `terminal/mod.rs` -- `use std::borrow::Cow`, the `product_name` field, `product_name: None` in
   `Default`, and the test-module rename `verify_bug0058_tests` -> `dcs_routing_tests`.
5. `terminal/terminal_tests.rs` -- the new test, and the one literal that changed from
   `OneTerm(..)` to `oneterm-vt(..)`.

Nothing else. The claim that the only behaviour change is the two reply strings plus the sanitiser
and the `DA2` clamp holds.

## Test and gate results at `3fd76f7`

| Command | Result |
| --- | --- |
| `cargo test -p oneterm-vt` | 374 + 6 + 8 + 3 doctests, plus my 7, all green |
| `cargo test --workspace` | green apart from the load-sensitive failure in R8 |
| `cargo test -p oneterm-vt --features vt-paranoid` | 374 + 6 + 8, green |
| `cargo test -p oneterm-tools --test corpus_check` | 2 passed |
| acceptance grep over `crates/vt/src` | 0 |
| repository-link rustdoc lines | 24 lines in 23 files, the allowed form |
| `cargo run -p oneterm-vt --example headless` | byte-identical to the packet's pasted output |
| `python scripts/vt-public-api.py --check` | `public API surface unchanged`, 688 lines, deterministic over two runs |
| the packaging gate | passes; all three negative tests fail correctly |
| `pwsh scripts/ci-local.ps1 -Full` | **green**, with an untracked file under `crates/vt/` present: `advisories ok, bans ok, licenses ok` then `ci-local: all checks passed.`, exit 0 |

## Still open

1. **R1** -- the README's Install block, both halves. Nobody can follow it as written today.
2. **R2** -- who ruled. The packet proposed the honest provenance and then did not write it.
3. R3, R4, R5, R6, R7 -- cheap, and none blocks anything.
4. Carried from the first pass and still true: no E2E walk (the owner runs this session inside
   OneTerm), no MSRV job, and `vt-public-api.py` does not detect signature changes.

## Scratch artefacts

The scratch consumer was deleted. One artefact could not be removed by this session: `C:\ch`, a
throwaway `CARGO_HOME` created to isolate R1(b); the tool guard refuses to delete a path at a drive
root. It holds only a cargo registry cache and is safe to delete by hand.
