# US-0081 — Frame time before and after the engine swap

Recorded, never gated (`IN-0029.md` acceptance: "no packet's exit criteria contain a
performance number"). This is a **finding**, not a claim of improvement.

## How

Both engines were built from this worktree as
`cargo build -p oneterm-app --profile fast-dev --features terminal-diagnostics`, the
"before" one from a detached checkout of the base `458aa78`, and were driven by the same
script (`meas81.ps1`): launch, `echo`, then a continuous `for /L … @echo` flood for ~25 s,
then `type` of a 10 MB file. Numbers come from the app's own
`crates/terminal-view/src/render/diagnostics.rs` line, emitted every five seconds:
`prepaint` and `paint` microseconds for the last frame, plus p95 / p99 over the frame ring.

`prepaint` is the number that matters here: it contains `TerminalSession::snapshot_into`,
which is the whole adapter path — take the lock, `render_update`, rebuild
`TerminalContent`.

Scrollback was the shipped default (10 000 rows) unless stated.

## Result

| Build | flood, prepaint | flood, p95 | 10 MB `type`, prepaint |
| --- | --- | --- | --- |
| **base** `458aa78`, old engine | ~200 us | 1.2-3.8 ms | ~130-190 us |
| **new**, engine at `dev` opt-level 0 | ~37-46 ms | 39-48 ms | ~44 ms |
| **new**, engine at opt-level 3 (this packet's `Cargo.toml` fix) | ~6.0-8.7 ms | 6.5-12 ms | ~6 ms |
| **new**, opt-level 3, scrollback 200 instead of 10 000 | **~300-800 us** | 2.5-3.9 ms | ~350 us |

Raw series: [`US-0081-frame-time-after.log`](US-0081-frame-time-after.log) is one of the
new-engine runs; the parser used to tabulate them is `frames.py` in the session scratchpad.

## What the numbers say

**1. A build-configuration defect this packet introduced, and fixed.**
`[profile.fast-dev.package]` lists the crates on the hot path and had
`alacritty_terminal = { opt-level = 3 }`. The swap moved that path onto `oneterm-vt`,
which was not in the list, so the engine ran unoptimized in the profile the project uses to
run the app. Adding `oneterm-vt` and `oneterm-pty` to the list took the flood from ~44 ms
to ~6.5 ms per frame. The release profile was never affected.

**2. An engine defect, still open, owned by another packet.**
The remaining gap to the base — ~6.5 ms versus ~200 us — is **not** the shim. The fourth
row isolates it: with the same binary and the same workload, dropping the scrollback from
10 000 rows to 200 collapses prepaint to ~300-800 us. The cost therefore scales with
*history depth*, which the shim never walks: it copies the viewport.

What does walk history is `TerminalGrid::assert_integrity`. It calls
`Screen::assert_integrity` and `Screen::assert_interned_ids_resolve` on **both** screens,
and each walks every live row from `oldest` to `newest` — the whole scrollback — inspecting
every cell. It runs once per `Terminal::feed` **and** once per `RenderState::begin_update`,
i.e. twice per frame under output.

That contradicts R-28, which the design states as: *"`assert_integrity()` in debug builds,
**once per `feed` / `resize` / `render_update`, not per mutation**. The full two-screen walk
is behind the `vt-paranoid` feature used by the property tests and the fuzz targets; the
always-on check is O(1) (counters, ranges, the active screen's cursor)."* The shipped
always-on tier is O(live rows x cols), not O(1).

Consequences, stated plainly:

- **Release builds are unaffected** — both functions return immediately when
  `cfg!(debug_assertions)` is false, so no user-visible behaviour changes.
- **Every `fast-dev` run and every `cargo test` pays it.** At the default scrollback that
  is ~6 ms per frame of pure assertion, and it grows with the session's history.
- It is not this packet's to fix: `US-0075` owns `Screen::assert_integrity` and `US-0079`
  owns the `begin_update` call site, and R-28 is their acceptance criterion. Recorded as a
  gap in [`../US-0081-engine-shim.md`](../US-0081-engine-shim.md).

**3. The shim's own cost is in the same band as the engine it replaced.** With the
assertion cost removed from the picture (the 200-row row), prepaint is ~300-800 us against
the base's ~200 us, for a path that rebuilds the full viewport of legacy cells every frame
exactly as `TerminalContent::refill` did. `US-0082` removes that rebuild by giving the view
`RenderRow` directly.

## Caveats

- One machine, one session, `fast-dev` (debug assertions on, first-party crates at
  opt-level 3), 52-row grid at 2560x1032. Not a benchmark suite; `vt-bench` is.
- The flood's Ctrl-C did not land in the measurement runs (the session had gone
  disconnected), so both engines ran the flood straight into the 10 MB `type`. The
  workload is identical for both, which is what the comparison needs.
- A single 702 ms prepaint frame appears once in the 200-row run during the 10 MB `type`.
  It is one frame out of ~140 and was not chased down; noted so it is not lost.
