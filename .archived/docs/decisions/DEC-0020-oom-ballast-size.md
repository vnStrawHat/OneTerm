# DEC-0020 OOM ballast size: 16 MiB, after BUG-0078

Date: 2026-09-25

## Status

Accepted by the owner on 2026-09-25: **16 MiB, landing with or after `BUG-0078`**.
Implementation packet: `US-0141` under `IN-0045`.

## Context

`crates/app/src/oom.rs` (`BUG-0012`, `DEC-0005` "OOM handling") commits a 64 MiB ballast at
startup and never touches it.

**What happens when an allocation fails:**

1. The allocator frees the ballast.
2. It retries at once, then every 20 ms, 150 times (about 3 s).
3. If every retry fails, Rust aborts as before.

**The failure it guards against.** A sibling process, typically `rustc` or LLVM started by a
coding agent, drives system commit to the limit. On Windows every process then gets NULL,
and Rust's infallible allocation path aborts with `0xc0000409`. This happened twice on the
measuring machine during this research: fat-LTO `rustc` died with `LLVM ERROR: out of
memory` at the 31.7 GB commit limit.

**What the ballast costs.** Measured in `IN-0045`:

- It adds 64 MiB to commit, and so to the status bar `MEM`.
- It adds 0 to the private working set, because it is never resident.

The owner asked for a reasoned size. Evidence:
[`IN-0045/research/agent-load-phase-2.md`](../spec-intakes/IN-0045-memory-usage-review/research/agent-load-phase-2.md)
§ 4.

**Four facts bound the size:**

1. **An untouched committed block still counts against the commit limit.** Windows charges
   commit at `MEM_COMMIT`, not when a page is first written. On a machine near the limit,
   the ballast holds 64 MiB of the charge from startup. Freeing it only gives that 64 MiB
   back.
2. **The freed ballast is not reserved for OneTerm.** It returns to the system-wide pool,
   and the process that caused the spike can take it as easily as OneTerm. What OneTerm
   can count on is narrower:
   - the first retry, which runs right after the free with no sleep before it;
   - whatever else OneTerm allocates in the next few milliseconds.

   After that, survival depends on the retry loop and on the sibling dying.
3. **The largest routine single allocation:**

   | When | Largest allocation |
   | --- | --- |
   | Today | 24.8 MB: the glyph table, on every tab or pane open |
   | After `BUG-0078` | 6.3 MB: the history ring at 100,000 lines. It is 0.8 MB at the default 10,000 |
   | Otherwise | 2.2 MB or less: style interner rehash, PTY read buffer, gpui arena and scene growth |

   Sixel images (up to 64 MiB each) are not routine.
4. **OneTerm's net commit growth over 3 s at the heaviest measured load** (two tabs of a
   claude-like TUI): 10.2 MB at 1280x800 and 15.9 MB at 1920x1040. It happens while
   history fills. Once history is full, net growth is about 0.

   Gross allocation over the same 3 s is 150 to 240 MiB. That is churn the heap serves
   from freed blocks. It is not new commit, so it is not what a retry needs.

## Decision

- Set `BALLAST_SIZE` to **16 MiB**, in the same change as `BUG-0078` or after it.
- Keep the retry loop unchanged: 20 ms × 150.
- Keep the one-shot release.
- Record the sizing rule in the module header of `oom.rs`, so the number is re-derived
  rather than copied:

  > The ballast is at least the largest routine single allocation, plus about 1 s of
  > OneTerm's heaviest measured net commit growth.

  Today that is 6.3 MB + 5 MB. Rounded up to a power of two, it is 16 MiB.

**If `BUG-0078` has not landed**, use **32 MiB**. The 24.8 MB glyph-table allocation on
tab open must fit in the first retry.

**Why 16 is enough:**

- It covers any routine single allocation 2.5 times over after `BUG-0078` (6.3 MB), and
  14 times over at the default limit (1.1 MB).
- It covers the whole measured 3 s of net growth at 1280x800. It covers about 3 s at
  1920x1040.
- More does not buy proportionally more. Past the first few milliseconds the freed
  headroom is shared with the process causing the spike (fact 2).

**Why not less.** At 8 MiB a single 6.3 MB ring allocation, plus anything else in flight,
leaves no margin.

**What it saves:** 48 MB of commit and of the status bar figure, on every instance, from
startup. Private working set does not change.

**Other platforms:**

- Linux: overcommit makes an untouched block cost no resident memory. The size matters
  only for `Committed_AS`.
- macOS: the block is committed lazily.

The change is Windows-motivated, and harmless elsewhere.

## Alternatives

- [x] Selected: 16 MiB after `BUG-0078` (32 MiB if it slips), described above.
- [ ] 0, 8 MiB, 64 MiB (today), and reserve-only. They were not selected for the reasons
  in the table.

| Option | Commit cost | Private WS cost | What it covers | Verdict |
| --- | --- | --- | --- | --- |
| 0 (drop the ballast, keep the retry loop) | 0 | 0 | Nothing immediate: the first failing allocation sleeps until the sibling frees, up to 3 s of UI freeze, and fails if the pressure persists | Rejected: turns every spike into a freeze |
| 8 MiB | 8 MiB | 0 | The largest routine single allocation after `BUG-0078`, with no margin for concurrent growth | Rejected: too tight at 100,000 lines |
| **16 MiB** | **16 MiB** | **0** | The largest routine allocation 2.5 times over, plus 1 to 3 s of the heaviest measured net growth | **Recommended (after `BUG-0078`)** |
| 32 MiB | 32 MiB | 0 | The heaviest 3 s of net growth twice over; the 24.8 MB glyph table before `BUG-0078` | Fallback if `BUG-0078` slips |
| 64 MiB (today) | 64 MiB | 0 | Four times the heaviest measured 3 s of net growth | Rejected: the extra 48 MiB is headroom OneTerm cannot keep for itself once freed (fact 2), and it is charged on every instance from startup |
| `MEM_RESERVE` only | 0 | 0 | **Nothing.** Reserving address space charges no commit. Committing it at the moment of failure needs the commit the system has run out of, so it gives no headroom. It is the same as option 0 with more code | Rejected |

## Consequences

- [x] Commit drops by 48 MB at every tab count. Confirm with `measure.ps1` S1: the
  expected value is 182 − 48 = 134 MB before `BUG-0078`. Measured by `US-0141`: 133.9 MB.
  Since `US-0137` the status bar `MEM` shows the private working set, which this does not
  change.
- [x] `oom.rs` header and `DEC-0005`'s "64 MiB" wording are updated in the implementing
  change. `IN-0012`'s `low-level-design/oom-allocator.md` table row says 64 MiB and must
  change with it.
- [ ] Trade-off: the margin is the measured load on this machine. A new routine allocation
  larger than about 8 MB must revisit this number. Examples: a larger ring, a new
  per-view table, or a frame buffer copied in Rust.
- [ ] Still unproven in the field. The `BUG-0012` acceptance item "OneTerm survives an
  agent-driven OOM spike" is manual and has not been observed at any ballast size.
