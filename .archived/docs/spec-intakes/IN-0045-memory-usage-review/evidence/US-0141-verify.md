# US-0141 independent verification

- Date: 2026-09-25
- Commit under test: `d3591773` (branch `feat/ballast-16mib`, one commit on main `5351f09a`)
- Packet: [`US-0141`](../US-0141-oom-ballast-16mib.md)
- Decision: [`DEC-0020`](../../../decisions/DEC-0020-oom-ballast-size.md)
- Host: Windows 11 Enterprise 10.0.26200, release build, private `USERPROFILE`, own pid only

## Verdict: PASS (one documentation gap, F1, non-blocking)

The diff is exactly what the packet claims: the `BALLAST_SIZE` constant, the module-header
sizing rule, and the doc updates it lists. Nothing else in `oom.rs` changed — the retry loop
(20 ms × 150) and the one-shot release are byte-identical to `5351f09a`. The sizing rule in
the header matches `DEC-0020`'s Decision section number for number. Re-measured commit
(134.4 MB on a clean run) matches the packet's claim and `DEC-0020`'s expected 182 − 48 = 134.
Gate subset (`cargo test -p oneterm-app`, `clippy -D warnings`, `fmt --check`,
`check-doc-paths.py`, `check-english.py`) is green. One doc the packet's own Documentation
scan should have caught still states "64 MiB" for the ballast in the present tense (F1);
it is a one-line fix and does not affect behavior or code review.

## Findings

### F1 (minor, docs): a fourth doc still states "64 MiB allocation" for the ballast

`docs/spec-intakes/IN-0043-run-shell-as-administrator/low-level-design/elevated-instance.md:102-104`:

> Step 2 runs before the ballast on purpose: a malformed command line must produce a message
> and an exit, not a 64 MiB allocation first.

This is the same sentence `crates/app/src/lib.rs:83` used to carry, word for word, before
this commit updated the `lib.rs` comment to "16 MiB allocation" (`git diff 5351f09a d3591773
-- crates/app/src/lib.rs`). The `elevated-instance.md` copy was not updated. It is a
documentation-only miss: the LLD describes ordering ("parse before ballast"), which is still
true, and cites no behavior that changed. The packet's "Owning Docs Reviewed" / "Documentation
Action" lists six files plus `lib.rs`'s own comment but not this IN-0043 doc, so the scan
missed a docs-grep hit rather than making a judgment call to leave it. Fix: change "64 MiB" to
"16 MiB" on that line, matching the `lib.rs` edit in this same commit.

No other stale "64 MiB" ballast reference exists. A grep for `64 MiB|64 \* 1024 \* 1024|64MB|64
MB` under `docs/` and `crates/` (PowerShell `Select-String`, `rg` unavailable in this
environment) returns 40 hits; every other hit is one of:

- historical/context prose in `DEC-0020`, `DEC-0005`, `IN-0012` LLD/HLD, `IN-0045.md`,
  `memory-attribution.md`, `agent-load-phase-2.md`, `history-storage-assessment.md` that
  correctly states what the ballast **was** (v0.4.1 through `5351f09a`) or what a table row
  used to say, each already paired with the new 16 MiB figure or an explicit "(today)" /
  "(before it)" / "(v0.4.1)" qualifier;
- Sixel's unrelated 64 MiB / 64 MB per-image RGBA cap (`crates/terminal-view/src/render/
  graphics.rs:5`, `crates/vt/src/parser/mod.rs:35`, and the IN-0018/IN-0028/IN-0029/IN-0038
  docs describing it), which the packet correctly left untouched.

## 1. Diff scope (`oom.rs`)

`git diff 5351f09a d3591773 -- crates/app/src/oom.rs`: two hunks.

1. A 17-line insertion in the module doc comment (the sizing rule and its two measured
   inputs), placed between the existing "how the allocator works" prose and the "best effort
   by design" trade-offs list. Comment-only.
2. `BALLAST_SIZE`: `64 * 1024 * 1024` → `16 * 1024 * 1024`, with the doc comment above it
   reworded to point at the module header instead of repeating "64 MiB". `BALLAST_ALIGN`,
   `RETRY_DELAY` (`Duration::from_millis(20)`), `MAX_RETRIES` (`150`), `BALLAST`,
   `ballast_layout`, `init_ballast`, `release_ballast`, `OomResilientAlloc` and its
   `GlobalAlloc` impl, and both `#[test]` functions are byte-identical — confirmed by reading
   the full post-image (`crates/app/src/oom.rs`, 238 lines) end to end, not just the diff
   hunks. No test assertion encodes the ballast size, so no test needed to change.

## 2. Header rule vs. `DEC-0020`'s Decision section

| `oom.rs` header | `DEC-0020` Decision | Match |
| --- | --- | --- |
| "the ballast is at least the largest routine single allocation, plus about 1 s of OneTerm's heaviest measured net commit growth" | identical sentence, quoted verbatim in `DEC-0020` | Yes |
| "the history ring, 6.3 MB at 100,000 lines (0.8 MB at the default 10,000)... assumes `BUG-0078`... without it the decision says 32 MiB" | fact 3 table: 6.3 MB after `BUG-0078`; "If `BUG-0078` has not landed, use 32 MiB" | Yes |
| "10.2 MB at 1280x800, 15.9 MB at 1920x1040, so about 5 MB per second... Gross churn (150 to 240 MiB per 3 s)" | fact 4: identical two numbers, "over 3 s"; same gross-churn range | Yes |
| "6.3 MB + 5 MB, rounded up to a power of two, is 16 MiB" | "Today that is 6.3 MB + 5 MB. Rounded up to a power of two, it is 16 MiB" | Yes, near-verbatim |
| "about 1 s" framing (rule) alongside "over 3 s" framing (growth figures), both present | Both framings present in `DEC-0020`: "about 1 s of ... net commit growth" (rule) and "10.2 MB at 1280x800 and 15.9 MB at 1920x1040" over "3 s" (fact 4) | Yes, and internally consistent: 15.9 MB / 3 s ≈ 5.3 MB/s ≈ the header's "about 5 MB per second", so "about 1 s" of that rate is ≈5 MB, matching the header's "+5 MB" term |

No numeric or wording drift between the two documents.

## 3. Stale "64 MiB" sweep

Command (PowerShell, `rg` not on PATH in this environment):

```powershell
Get-ChildItem -Path docs -Recurse -Include *.md | Select-String -Pattern "64 MiB|64 \* 1024 \* 1024|64MB|64 MB"
Get-ChildItem -Path crates -Recurse -Include *.rs | Select-String -Pattern "64 MiB|64 \* 1024 \* 1024|64MB|64 MB"
```

Classification: see F1 above for the one ballast-related miss; all remaining hits are
historical prose (already paired with the current 16 MiB number or an explicit past-tense/
"today" qualifier) or Sixel's unrelated per-image cap, which correctly stays at 64 MiB.

## 4. Gate subset run

`CARGO_TARGET_DIR=D:\TrungKFC-Research\Rust\myTerm2\target` for every cargo command (shared,
disk-constrained target dir; full `ci-local` intentionally not run — the coordinator runs it
on `main` after the `BUG-0078` + `US-0141` merge):

| Command | Result |
| --- | --- |
| `cargo test -p oneterm-app` | 26 passed (3 suites), including `oom::tests::ballast_lifecycle_and_normal_alloc` and `oom::tests::retry_recovers_after_transient_failure_and_releases_ballast` |
| `cargo clippy -p oneterm-app --all-targets -- -D warnings` | clean |
| `cargo fmt --all -- --check` | clean |
| `python scripts/check-doc-paths.py` | passed, 207 current paths in 11 documents |
| `python scripts/check-english.py` | passed, 1021 files |

## 5. Re-measurement

`cargo build -p oneterm-app --release` (first attempt succeeded; no retry needed, so the
`CARGO_BUILD_JOBS=2` fallback in the verification brief was not exercised). Binary copied to
`oneterm-us0141v.exe` for a unique perf-counter identity, then
`docs/spec-intakes/IN-0045-memory-usage-review/research/measure.ps1 -Mode S1` twice, private
`USERPROFILE`/`HOME`, own launched pid only:

| Run | WS | Private WS | Commit |
| --- | --- | --- | --- |
| 1 | 137.3 | 51.3 | 141.6 |
| 2 | 135.4 | 49.9 | 134.4 |

Run 2 (134.4 MB commit, 49.9 MB private WS) matches the packet's reported 133.9 MB mean and
`DEC-0020`'s expected 182 − 48 = 134 within noise. Run 1 is a startup-outlier commit reading
(141.6 MB), the same pattern the packet's own main-build table shows on a first run (126.9 WS
vs. 93.4 WS between its two main runs); private WS on both runs of this branch (51.3, 49.9)
is unchanged within noise from main's reported 49.0-49.2. This branch does not include
`BUG-0078` (base `5351f09a`, before the `BUG-0078` merge at `5f5ed715`), so the glyph table is
still preallocated and these commit figures are the "before `BUG-0078`" baseline the packet
itself measured — consistent with `IN-0045`'s Handoff note that the two land together. Cleanup
verified: `Get-Process oneterm*` afterward shows only the owner's own running instance (pid
12948, `dist\`), untouched; both measurement pids and the scratch `home-us0141v-*` directories
were removed by `measure.ps1`'s own `finally` block and an explicit follow-up delete.

## 6. Gaps

- Full `ci-local.ps1` was not run per the verification brief (disk-constrained shared target;
  the coordinator runs it on `main` after this merges with `BUG-0078`).
- `vmregions.ps1` was not re-run in this verification; the packet's own reading (pid 32476:
  one 16.0 MB committed block, 0.0 MB resident, no 32 MB+ block) was taken as given per the
  brief's scope, which asked only for a fresh `measure.ps1 -Mode S1` re-measurement.
- The `BUG-0012` acceptance item "OneTerm survives an agent-driven OOM spike" remains
  unobserved at any ballast size, as both the packet and `DEC-0020` Consequences already state.
- At this exact commit, standalone (without `BUG-0078`), `BALLAST_SIZE` is 16 MiB while
  `DEC-0020`'s own rule calls for 32 MiB until `BUG-0078` lands (the 24.8 MB glyph-table
  allocation would not fit the first retry). This is not a defect in the packet — the
  packet's Handoff section states the merge-order requirement explicitly, and this
  verification's precondition (main already carries `BUG-0078` at `2e69a8a5`) is what makes
  the combined result correct. Flagged here only so the merge is not done with this commit
  standing alone ahead of `BUG-0078`.
