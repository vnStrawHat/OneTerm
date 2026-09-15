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
