# Low-Level Design: performance evidence

Intake: [`IN-0039`](../IN-0039.md)
HLD: [`../high-level-design.md`](../high-level-design.md)
Topic: performance-evidence
Date: 2026-09-15

> One concern per file: turning the benchmark this repository already has into evidence an outsider
> can read and reproduce, and a trip-wire that catches a tenfold regression.

## Concern

The evaluation, criterion (i), scored `oneterm-vt` **2 out of 5** -- its second-worst score, and
the only criterion where it lost to both competitors:

> `oneterm-vt` makes no performance claim anywhere in its README, changelog or guide, which is
> honest, but it also ships nothing an evaluator can run: no `benches/`, no criterion, one internal
> `snapshot_bench.rs` module. ... For a crate whose main pitch to a GPU-renderer author is a damage
> model, the absence of any measurement is conspicuous.

The gap is not that the engine is unmeasured. It is measured in five tiers by
`crates/tools/src/bin/vt-bench.rs`, against fixtures ported from vtebench, at a fixed 160x45
geometry, with a counting allocator for live heap. The gap is that **the measurement is invisible
from outside the repository and produces no number anybody keeps.**

## Design: what is measured, and by what

### Nothing new is built

`vt-bench` already exists and already does the work:

| Tier | What it measures | Geometry |
| --- | --- | --- |
| 1 | parser only, against a no-op `Dispatch` | 160x45 |
| 2 | parse plus grid (`Terminal::feed`) | 160x45 |
| **3** | parse plus grid plus one `snapshot_update` and `map_colors` per simulated frame | 160x45 |
| 4 | resize latency at 0 / 10 000 / 100 000 scrollback rows | its own |
| 5 | live heap after filling N rows of plain / unicode / styled / mixed content | 160x45 |

Tier 3 is already designated the primary metric by
[`IN-0029/low-level-design/testing-and-bench.md`](../../IN-0029-vt-engine/low-level-design/testing-and-bench.md),
and it is the right one to publish: it is the number a GPU-renderer author is buying, because it is
the tier that includes the damage model the evaluation's criterion (b) gave 5 out of 5.

`vt-bench` already takes `--json` and `--out`, so machine-readable output exists too.

### `criterion` is deliberately not added

The obvious move is `[dev-dependencies] criterion` and a `benches/` directory, because that is what
`rio-vt` does and what a reviewer expects to see. Not taken:

- The measurement already exists, in a binary that is already built, tested and documented. A
  second way to run the same fixtures is a second thing to keep in step.
- `criterion` brings a dependency tree into a crate whose entire pitch is a six-leaf dependency
  tree. It is a dev-dependency and therefore does not reach an embedder's build -- but "we added a
  benchmark framework to the crate that brags about having six dependencies" is a bad look for a
  gain that is zero.
- `criterion`'s value is statistical rigour on microbenchmarks. Tier 3 is a throughput number over
  a fixed 4 MiB fixture; its noise floor is dominated by the machine, not by sampling.
- `docs/agents/dependencies.md` requires every dependency to be declared once in the workspace and
  justified. There is no justification here that survives being written down.

Recorded so the next reviewer does not re-propose it: **add `criterion` the day somebody needs a
distribution rather than a median.** Not before.

## Design: where the evidence is published

Three places, each for a different reader, all generated from one run.

### 1. The README, above the fold

An evaluator reads the README first and stops there. The table goes under a new `## Performance`
heading, immediately after `## Dependencies`, and it is about fifteen lines:

```text
## Performance

Measured on <CPU>, Windows 11, rustc 1.96.0, release profile, 2026-09-15.
Reproduce with one command:

    cargo run -p oneterm-tools --release --bin vt-bench -- tier3

| Fixture           | MiB/s | What it is                         |
| ----------------- | ----: | ---------------------------------- |
| scrolling         |       | plain text scrolling the screen    |
| unicode           |       | mixed-width, combining, emoji      |
| sgr-churn         |       | a style change every few cells     |
| alt-screen        |       | a full-screen TUI redraw loop      |
| sixel             |       | an image-heavy stream              |

Tier 3 is parse plus grid plus one snapshot per simulated frame -- the whole
path a renderer pays. Read it next to the transport ceiling: a ConPTY running
`cmd.exe` delivers about 1.2 MiB/s, and a DOOM-fire-class producer about
30 MiB/s, so the engine is not the bottleneck at any shell rate. The other
four tiers, the geometry and the fixture sources are in guide chapter 14.
```

Two rules for that table, and they are the ones that make it evidence rather than marketing:

- **The machine is named.** A throughput number with no machine is a number with no meaning.
- **The transport ceiling is beside it**, which is `IN-0029`'s existing "reporting rule": no engine
  number is read in isolation.

The cells are left empty above because this design does not invent numbers. `US-0107` fills them
from the run it performs, and its Evidence section carries the raw output.

### 2. Guide chapter 14

A new chapter, `crates/vt/docs/guide/14-performance.md`, about 130 lines: all five tiers, the
fixture list and where each came from, the geometry and why it is fixed, the counting allocator,
the hygiene rules (never in parallel, never piped from the generator), and the honest limits --
one machine, one operating system, medians not distributions, and no comparison against
`alacritty_terminal` or `rio-vt` because a fair cross-engine harness is a project of its own.

That last paragraph is the chapter's most valuable one. The crate's credibility comes from guide
chapter 11 listing its own gaps by name; a performance chapter that does the same is consistent
with it, and one that does not undoes it.

**Cost the packet must pay: the guide stops being thirteen chapters.** Four strings say "thirteen":
`crates/vt/README.md`, `crates/vt/CHANGELOG.md`, `crates/vt/src/guide.rs` and
`crates/vt/src/lib.rs`. All four change in the same commit, and `guide.rs` gains
`pub mod ch14_performance {}`. CI's "the embedder guide must stand alone" grep applies to the new
chapter like every other, so it may cite no `US-`, `docs/` or `crates/` path -- only
`https://github.com/...` links.

### 3. The committed baseline

`crates/tools/bench-baseline.json`, written by `vt-bench tier3 --json`, committed, and carrying the
machine description in the file. It is what the guard compares against and it is what makes "is
this slower than it was?" a question with an answer.

## Design: the regression guard

```text
cargo run -p oneterm-tools --release --bin vt-bench -- tier3 \
    --check crates/tools/bench-baseline.json [--tolerance 0.5]
```

Exit non-zero when any fixture's throughput is below `tolerance x baseline`. **Default tolerance
0.5** -- a fixture has to get twice as slow before the guard fires.

### Why the band is that wide, and why it does not run in CI

`IN-0029` rule **R-29**: "No numeric target appears in any packet's exit criteria ... the phase plan
records ratios against the old engine, never an absolute microsecond figure." The rule exists
because a threshold measured on a shared runner measures the runner.

A guard with a 2x band does not violate R-29 in spirit: it is not an exit criterion for any packet
and it is not a performance target. It is a trip-wire for the class of mistake that actually
happens -- an accidental clone in the print path, an `O(n)` scan added to a per-cell loop, a
`snapshot_update` that stops being incremental. Those are tenfold regressions, not fifteen-percent
ones.

**It runs by hand, on the baseline machine**, and not in `.github/workflows/ci.yml`:

- GitHub's shared runners vary by more than 2x between classes, so a CI threshold would be either
  useless or flaky, and a flaky performance gate is worse than none -- people learn to re-run it.
- The baseline is one machine's. Comparing another machine against it is meaningless.
- The two moments that want the answer are "before I tag a release of the engine" and "I just
  touched the print path". Both are a maintainer at a keyboard on the machine the baseline came
  from, not a runner. Guide chapter 14 and the README name the command for both.

This is IN-0039's Open Decision 5. If the owner wants it in CI as well, it goes in **report-only**,
printing the ratio and never failing, exactly like the `esctest` job -- one extra step, and the
design does not resist it.

### Refreshing the baseline

Committing a new baseline is a normal reviewed diff, and the rule is one line: **a baseline is
refreshed only in a commit that says why the number moved.** A baseline quietly regenerated to make
a guard pass is the failure mode this whole section exists to prevent, and no script can stop it --
only the review can.

## Edge Cases and Failure Modes

- [ ] **The baseline file is missing** -> `--check` exits non-zero with "no baseline", not with a
      pass. A missing baseline is a failure, never a skip.
- [ ] **A fixture is added or renamed** -> a fixture in the run with no baseline entry is reported
      and fails; a baseline entry with no fixture is reported and fails. Neither is silently
      ignored.
- [ ] **The machine is slower than the baseline machine** -> the guard fires and it is a false
      positive. That is why the band is 2x and why the file names the machine; the checklist says
      to run it on the baseline machine or to read the ratio rather than the verdict.
- [ ] **A reader takes the number as a promise** -> the semver promise's clause 6 explicitly does
      **not** cover performance: "Not promised: grid internals ... allocation behaviour, and
      performance." Publishing a number does not change that, and the chapter says so in the same
      breath as the table. A published measurement is evidence, not a contract.
- [ ] **A debug build** -> `vt-bench` refuses to `--check` outside a release profile. A debug
      number compared against a release baseline is a guaranteed false failure. `cfg!(debug_assertions)`
      is the test, it is one line, and it turns the most likely user error into a clear message.
- [ ] **`vt-paranoid` is on** -> the whole-history walk costs milliseconds per `feed` and makes
      every number meaningless. `--check` refuses under that feature too, for the same reason and
      in the same line.
- [ ] **The sixel fixture on a machine with a different allocator** -> tier 3 does not measure heap;
      tier 5 does, and tier 5 is not guarded. Only tier 3 has a guard, deliberately.

## Verification

- [ ] `cargo run -p oneterm-tools --release --bin vt-bench -- tier3` runs on the maintainer's
      machine and its full output is pasted into the packet's Evidence, with the CPU, the operating
      system and the toolchain named.
- [ ] The README table and guide chapter 14 carry **the same numbers as that output**, checked by
      reading, not asserted. A published number that does not match the attached run is the one
      failure mode that would make this packet worse than doing nothing.
- [ ] `vt-bench tier3 --check crates/tools/bench-baseline.json` passes immediately after the
      baseline is written, and **fails when the baseline is edited to half the throughput** --
      attach both, because a guard that has never been seen to fail has not been tested.
- [ ] `--check` refuses in a debug build and under `vt-paranoid`, with the message attached.
- [ ] `cargo doc -p oneterm-vt --no-deps --all-features` is warning-free with chapter 14 included,
      and every Rust block in the chapter compiles as a doctest like every other chapter's.
- [ ] CI's "the embedder guide must stand alone" grep passes over the new chapter.
- [ ] `cargo test --workspace` -- `crates/tools` gains code and keeps its tests green.
- [ ] `grep -rn 'thirteen' crates/vt` returns nothing about the guide.
