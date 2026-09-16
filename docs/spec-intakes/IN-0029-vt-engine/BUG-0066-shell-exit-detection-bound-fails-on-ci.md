# Work: the real-shell round-trip bounds are stopwatches and a loaded CI runner trips them

ID: BUG-0066
Intake: IN-0029
Created: 2026-09-16

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug (a flaky gate, in test code only)
- Risk lane: normal
- Spec Intake, when required: IN-0029 (`US-0083`, the local-shell pump and its real-shell tests)

## Outcome

Every real-shell round trip in `crates/local-shell/src/session_tests.rs` passes on a loaded
two-vCPU CI runner while still failing for the defect it was written to catch, and a
failure of `spawned_shell_exit_is_detected` can be attributed from the CI log alone.

The reported failure, "Full workspace quality gate" on ubuntu-latest (2 vCPU),
`cargo test --workspace`, main @a6a72ac3:

```text
test session::session_tests::spawned_shell_exit_is_detected ... FAILED
thread 'session::session_tests::spawned_shell_exit_is_detected' (6138) panicked at
crates/local-shell/src/session_tests.rs:350:5:
shell exit not detected after 4s
test result: FAILED. 27 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 4.03s
```

This is `BUG-0064` one test later, in the same binary and the same job: a wall-clock number
measured on the owner's eight-core Windows host, guarding a property that is not a latency
property, on a shared runner that does not have the margin. Nothing in the Unix path moved
since the previous push, where this test passed (`BUG-0063` removed two derives, `BUG-0065`
changed comments, `US-0109` changed the workflow).

The old message is also the reason this packet needed a measurement campaign rather than a
read: "shell exit not detected after 4s" says nothing about whether the shell ever started,
ever printed, or exited a millisecond after the deadline.

## Scope

- [x] In scope: the wall-clock bounds of the real-shell round trips in
      `crates/local-shell/src/session_tests.rs`, the comment that justifies them, and the
      assertion messages of the four tests whose bound moved.
- [x] Out of scope: all production code. The pump, the reaper thread in
      `crates/vt/src/pty/unix.rs`, the child-exit branch in
      `crates/local-shell/src/event_loop.rs`, `SPAWN_GUARD`, the `session_orphan_tests`
      probe and its `LIVENESS_BOUND`, and the `US-0083` gap 6 race. No behavior changes.

## Acceptance

- [x] The bounds are no longer stopwatch readings from one machine: one named constant says
      what property it gates, and why the number is what it is.
- [x] A shell that never delivers its exit still fails, with the elapsed time, `alive()` at
      the end, and the terminal snapshot in the message.
- [x] The two tightest bounds in the file, both of which have already flaked in this repo's
      recorded history, move with it.
- [x] The two `!alive()` waits that follow a local `close()` are left alone, with the reason
      recorded.
- [x] The binary passes 10 consecutive runs pinned to two logical CPUs at
      `--test-threads=8`, the condition that reproduces the starvation shape.
- [x] `pwsh scripts/ci-local.ps1` passes.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/US-0083-local-shell-native.md` -- the owning packet.
  Its `:193-194` names three of the tests this packet touches (`e2e_echo_output_rendered_in_snapshot`,
  `selection_text_and_clear`, `mouse_drag_updates_selection_not_mouse_move`) as the tests
  that "failed immediately" on the pre-native version, which is the property they exist for
  -- output reaching the snapshot at all. It quotes no bound for any of them, and does not
  mention `spawned_shell_exit_is_detected`. **No change needed:** unlike `BUG-0064`, this
  packet moves no number the packet states.
- `docs/spec-intakes/IN-0029-vt-engine/US-0083-local-shell-native.md` gap 6 (`:349-372`) --
  the `TerminalHandle::lock_for_render` lost-demand race. It is the reason a *render* can
  starve under load, and it is worth distinguishing from this packet: child exit does not
  travel through `lock_for_render` at all. **No change needed**, still open, still owned by
  `crates/terminal`.
- `docs/spec-intakes/IN-0029-vt-engine/BUG-0064-flood-handover-bound-fails-on-ci.md` -- the
  sibling fixed a day earlier in the same binary. Its Context table ("the discriminating
  power sits almost entirely in does a frame arrive at all") is the precedent this packet
  applies, and its gap 1 predicted this shape of follow-up. **No change needed.**
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` -- the
  intake's testing contract. It sets bounds for the `oneterm-vt` integrity walk only and
  says nothing about real-shell round trips. **No change needed.**
- `docs/terminal-backend.md` `:525` -- "real-shell tests in `session_tests.rs` are what
  cover it". True before and after; no bound quoted. **No change needed.**
- `docs/spec-intakes/IN-0019-conpty-resize-scrollback-desync/BUG-0051-conpty-grow-resize-alignment.md`
  `:268-270` and `docs/spec-intakes/IN-0025-sftp-dual-pane/US-0062-sftp-dual-pane.md`
  `:184-187` -- prior art, found while checking for other quotations of the numbers. Each
  records a **past flake of one of the two 2 s echo tests this packet widens**, in both
  cases on a machine loaded by something else (GUI captures; the rest of the gate), in both
  cases passing again in isolation, in both cases dismissed as "crate untouched by this
  packet". They are evidence of runs that happened. **Not rewritten**, per the same rule
  `BUG-0064` applied to the `US-0083` measurement tables.

### Documentation Action

No contract change.

Reason: no owning document states a bound for any of these tests. `US-0083` names three of
them for the property they prove -- output reaching the snapshot -- which this packet does
not weaken. The two documents that do quote "2 s" are evidence records of past runs, not
contracts, and evidence is not rewritten. The justification for the numbers belongs next to
the numbers, so it went into the constant's rustdoc rather than into a design document.

### Reconciliation

Docs changed: none. The no-change reason above stands. Searched for every other quotation
of the touched tests and their bounds across `docs/`, `crates/` and `scripts/` with a Python
scan (the repository's `rg` is unavailable in this worktree); the full hit list is in
Evidence.

## Context

### What the tests actually discriminate

Every wait this packet touches is a *detects-at-all* gate. The table is the measurement, not
a guess: the whole lib binary pinned to two logical CPUs (`ProcessorAffinity = 3`) at
`--test-threads=8`, worst case of ten runs.

| Test | What it waits for | Old bound | Worst measured | Headroom |
| --- | --- | --- | --- | --- |
| `selection_text_and_clear` | `hello` reaches the snapshot | 2 s | **437 ms** | **4.6x** |
| `mouse_drag_updates_selection_not_mouse_move` | `hello_world` reaches the snapshot | 2 s | **415 ms** | **4.8x** |
| `spawned_shell_exit_is_detected` | `alive()` flips after `exit\r` | 4 s | 405 ms | 9.9x |
| `e2e_echo_output_rendered_in_snapshot` | `oneterm_e2e` reaches the snapshot | 6 s | 169 ms | 35x |

The test that failed on CI is **not** the one with the least margin. The two 2 s echo waits
are tighter, and both have already flaked in this repository's recorded history (`BUG-0051`,
`US-0062`), each time dismissed as noise because the crate was untouched. Fixing only the
test named in the report would have left the two likelier flakes in place, so all four move
to one constant.

### What starves them: concurrency in aggregate, not the flood test

The brief's hypothesis (b) was that
`a_flooding_loop_hands_the_engine_to_a_waiting_frame` -- which runs a flood thread, a UI
drain thread, a watchdog and a renderer thread for its whole duration -- is what starves a
real shell, and that machine-saturating tests should take `SPAWN_GUARD` or a shared quiet
guard. **The measurement says no.** Same binary, same two-CPU pin, ten runs each:

| Configuration | `spawned_shell_exit_is_detected` wait, ten runs | Median | Worst |
| --- | --- | --- | --- |
| `--test-threads=2` (what a 2-vCPU runner defaults to) | 30, 33, 35, 36, 38, 38, 40, 40, 44, 44 ms | **37 ms** | 44 ms |
| `--test-threads=8`, flood test **skipped** | 139, 146, 181, 195, 206, 218, 223, 267, 291, 306 ms | **212 ms** | 306 ms |
| `--test-threads=8`, flood test **running** | 205, 217, 230, 243, 256, 262, 272, 304, 309, 355 ms | **259 ms** | 355 ms |

Removing the flood test entirely moves the median by about 47 ms out of 259. Dropping from
eight concurrent tests to two moves it by **7x**. The starver is the aggregate: every test
in this binary spawns a real shell, and most never `close()` it, so by the time a late test
runs there are around twenty live shells plus their event-loop and reaper threads competing
for two cores. `SPAWN_GUARD` serialises only the spawn call, not the lifetime.

So hypothesis (b) would serialise the wrong thing: it would pay a large wall-clock cost to
remove the smaller half of a starvation that the other twenty-odd tests cause anyway. It is
rejected on the numbers, not on effort. A run at `--test-threads=32` confirms the shape from
the other side: the wait itself stays at 146-201 ms while *spawn* contention on `SPAWN_GUARD`
grows to 43-437 ms -- and spawn happens before the clock starts, so it never counted.

### The change

One constant, `SHELL_ROUND_TRIP = 15 s`, used by every real-shell wait in the file, with
rustdoc that states the property, the measured worst cases, and why the number is free. 15 s
is not new to the file: it is already the bound `assert_powershell_prompt_emits_cwd` uses, so
that call site changes its spelling and not its value.

`spawned_shell_exit_is_detected` also gets the diagnostic the old message lacked: elapsed
time, `alive()` re-read at message time, and the snapshot's non-blank lines. The grid
persists, so one snapshot at the end answers "did the shell ever print a prompt" without
polling it on every 5 ms tick. The three echo tests get the snapshot too -- two of them
asserted with no message at all.

**What is deliberately not touched.** `trait_alive_is_local_close:196` and
`close_returns_without_joining_the_owner_thread:220` also wait 2 s for `!s.alive()`, which
looks like the same line. It is not the same risk: `PtySession::close`
(`crates/terminal/src/session.rs:742-745`) calls `self.state.set_alive(false)` **before it
returns**, so the predicate is already true at the first poll and the wait cannot time out
under any load. Widening them would be noise.

**Why not tighten instead.** A hold of a second or more, a shell that never starts, and a
pump that never publishes the exit all still fail -- just at 15 s. Nothing that these tests
were written to catch survives the change, because none of them is a latency defect. Raising
a ceiling that no healthy run comes within 4x of costs the gate nothing; the confirmed price
is on the failing path only, which `BUG-0064`'s verifier already recorded as LOW (E1).

## Plan

- [x] Reproduce first: build the lib binary, pin it to two logical CPUs, measure the wait.
- [x] Measure with and without the flood test, and at 2 / 8 / 32 test threads, to decide
      between the brief's hypotheses (a), (b) and (c).
- [x] Measure every other real-shell wait in the file in the same runs, to find which bounds
      are actually at risk rather than assuming the reported one is the worst.
- [x] Check whether the two `!alive()`-after-`close()` waits are the same class (they are
      not; read `PtySession::close`).
- [x] Introduce the constant, move the four at-risk bounds, enrich the four messages.
- [x] Negative control: prove the enriched message renders with real content.
- [x] Confirm no owning doc quotes a bound that moved.
- [x] 10 runs pinned to two CPUs after the fix.
- [x] `pwsh scripts/ci-local.ps1`.

## Decisions

None. The choice is recorded in Context and binds no future work: these are test ceilings,
not a contract.

## Verification Plan

- Unit proof: the `oneterm-local-shell` lib binary, 10 consecutive runs pinned to two
  logical CPUs at `--test-threads=8` -- the configuration that reproduces the starvation
  shape, since a plain run on eight cores exercises none of it.
- Negative control: a deliberately unsatisfiable predicate, to prove the new assertion
  message carries elapsed time, `alive()` and the snapshot.
- Integration proof: the same binary under whole-workspace load inside
  `cargo test --workspace` in the gate run.
- No E2E or platform proof: these are unit tests of the local-shell session against the
  platform shell, and the changed lines are platform-independent. The failure itself is
  Linux-only and is not reproducible on the host -- see gap 1.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Method

`cargo test -p oneterm-local-shell --lib --no-run` gives
`target/debug/deps/oneterm_local_shell-f95f1dbcacce61ac.exe`. Each run is started with
`Start-Process -PassThru -NoNewWindow`, `ProcessorAffinity` set to `3` (two logical CPUs)
immediately after start, then `WaitForExit()`. Only pids started this way were ever waited
on; no process was enumerated or terminated. The per-wait numbers come from temporary
`eprintln!` probes around each `wait_until`, built, measured, and reverted with
`git checkout` before the real change -- they are not in the committed diff.

### Before: ten runs, two CPUs, `--test-threads=8`

All ten passed; the bound was never reached on this host. The point of the run is the
distribution, not the verdict.

```text
run  1: wall=8.52s   exit=308ms  hello=353ms  hello_world=415ms  e2e=114ms
run  2: wall=10.50s  exit=405ms  hello=430ms  hello_world=137ms  e2e=144ms
run  3: wall=8.65s   exit=281ms  hello=225ms  hello_world=109ms  e2e=127ms
run  4: wall=4.72s   exit=391ms  hello=437ms  hello_world= 92ms  e2e=115ms
run  5: wall=10.58s  exit=263ms  hello=253ms  hello_world=153ms  e2e=146ms
run  6: wall=10.50s  exit=150ms  hello=181ms  hello_world=153ms  e2e=169ms
run  7: wall=10.42s  exit=186ms  hello=147ms  hello_world=100ms  e2e=114ms
run  8: wall=8.66s   exit=131ms  hello=170ms  hello_world= 90ms  e2e=123ms
run  9: wall=10.52s  exit=218ms  hello=152ms  hello_world= 95ms  e2e= 91ms
run 10: wall=8.63s   exit=217ms  hello=120ms  hello_world=192ms  e2e= 84ms
```

An earlier ten-run set with only the exit probe compiled in gave 205-355 ms for the same
wait, so 405 ms is the worst of twenty runs.

### Before: is the flood test the starver?

Ten runs each, exit wait only, same pin. Full numbers in the Context table.

| Configuration | Median | Worst |
| --- | --- | --- |
| `--test-threads=2` | 37 ms | 44 ms |
| `--test-threads=8 --skip a_flooding_loop` | 212 ms | 306 ms |
| `--test-threads=8` (flood running) | 259 ms | 355 ms |

Six runs at `--test-threads=32`: wait 146, 148, 158, 164, 188, 201 ms; spawn (outside the
measured window, on `SPAWN_GUARD`) 43, 96, 208, 351, 390, 437 ms.

Host worst case is 405 ms against a CI failure above 4 s, so the Windows host does not
reproduce the magnitude -- see gap 1. It reproduces the **shape**: the wait is dominated by
how many other real shells are alive, and 7x is available from concurrency alone.

### Negative control: the message carries its evidence

Temporary edit only, reverted. Predicate replaced by one that never answers true, so the
full bound elapses with a shell that started, printed and exited normally:

```text
shell exit not detected in 15.0120911s (bound 15s); alive=false at the end; terminal
snapshot, blank lines dropped: ["C:\\Users\\trunglt>"]
```

All three requested data points are present, and they disagree usefully: `alive=false` with
`detected=false` says the exit *did* arrive and the wait missed it, which is a different
defect from the snapshot being `[]`. The same probe with the bound cut to 1 ms prints
`shell exit not detected in 6.2763ms (bound 1ms); alive=true at the end; terminal snapshot,
blank lines dropped: []` -- the "shell never started" signature. A CI log can now be read
without a runner.

### After: ten runs, two CPUs, `--test-threads=8`

```text
run  1: exit=0 wall=9.51s   test result: ok. 33 passed; 0 failed; 2 ignored; finished in 8.33s
run  2: exit=0 wall=8.54s   test result: ok. 33 passed; 0 failed; 2 ignored; finished in 8.51s
run  3: exit=0 wall=8.59s   test result: ok. 33 passed; 0 failed; 2 ignored; finished in 8.55s
run  4: exit=0 wall=6.75s   test result: ok. 33 passed; 0 failed; 2 ignored; finished in 6.73s
run  5: exit=0 wall=4.85s   test result: ok. 33 passed; 0 failed; 2 ignored; finished in 4.81s
run  6: exit=0 wall=10.34s  test result: ok. 33 passed; 0 failed; 2 ignored; finished in 10.31s
run  7: exit=0 wall=10.42s  test result: ok. 33 passed; 0 failed; 2 ignored; finished in 10.38s
run  8: exit=0 wall=8.60s   test result: ok. 33 passed; 0 failed; 2 ignored; finished in 8.58s
run  9: exit=0 wall=10.44s  test result: ok. 33 passed; 0 failed; 2 ignored; finished in 10.42s
run 10: exit=0 wall=10.47s  test result: ok. 33 passed; 0 failed; 2 ignored; finished in 10.44s
```

10/10, exit code 0 every time. Wall-clock range 4.81-10.44 s against 4.67-10.65 s before, so
raising four ceilings cost the binary nothing: a ceiling is only paid when it is hit.

### Every real-shell wait in `session_tests.rs`, and what happened to it

| Line (after) | Test | Waits for | Bound before | Bound after | Why |
| --- | --- | --- | --- | --- | --- |
| `:139` | `windows_powershell_prompt_emits_cwd_without_parser_errors`, `pwsh_prompt_emits_cwd_without_parser_errors` | `cwd().is_some()` | 15 s | `SHELL_ROUND_TRIP` | Value unchanged; it is where the 15 s came from. |
| `:196` | `trait_alive_is_local_close` | `!alive()` after `close()` | 2 s | **unchanged** | `close()` sets `alive=false` synchronously before returning (`crates/terminal/src/session.rs:742-745`); cannot time out. |
| `:220` | `close_returns_without_joining_the_owner_thread` | `!alive()` after `close()` | 2 s | **unchanged** | Same. |
| `:305` | `selection_text_and_clear` | `hello` in snapshot | 2 s | `SHELL_ROUND_TRIP` | Tightest in the file, 4.6x. Flaked before (`US-0062`). |
| `:328` | `mouse_drag_updates_selection_not_mouse_move` | `hello_world` in snapshot | 2 s | `SHELL_ROUND_TRIP` | 4.8x. Flaked before (`BUG-0051`). |
| `:388` | `spawned_shell_exit_is_detected` | `!alive()` after `exit\r` | 4 s | `SHELL_ROUND_TRIP` | The reported failure. |
| `:405` | `e2e_echo_output_rendered_in_snapshot` | `oneterm_e2e` in snapshot | 6 s | `SHELL_ROUND_TRIP` | Not at risk (35x); moved so one constant covers the class. |

Outside this file, `session_orphan_tests.rs` has `LIVENESS_BOUND = 5 s` (`:40`) and a 10 s
wait (`:175`). Both belong to the orphan probe's own design, which diffs this process's
children and holds `SPAWN_GUARD` across both snapshots; they are a different instrument and
are out of scope.

### Search for other quotations of the moved bounds

Python scan over `docs/`, `crates/` and `scripts/` (this worktree has no `rg`) for the four
test names, `SHELL_ROUND_TRIP` and `session_tests.rs`. Every hit was read. Only two quote a
bound that moved, both evidence records of past runs and both left as written:
`BUG-0051-conpty-grow-resize-alignment.md:268-270` and `US-0062-sftp-dual-pane.md:184-187`.
No contract, design or checklist document states any of these numbers.

### `pwsh scripts/ci-local.ps1`

Run in the worktree, Windows host, no `--full`, `CARGO_BUILD_JOBS=4`. All 25 steps passed,
exit 0:

```text
==> cargo fmt --all -- --check
==> cargo clippy --workspace --all-targets -- -D warnings
==> cargo clippy --workspace --all-targets --features oneterm-app/terminal-diagnostics -- -D warnings
==> cargo test --workspace
==> cargo test -p oneterm-vt --features vt-paranoid
==> cargo test -p oneterm-vt --features regex
==> cargo build -p oneterm-vt --no-default-features --examples
==> cargo test -p oneterm-vt --no-default-features
==> cargo build -p oneterm-vt --all-features --examples
==> cargo tree -p oneterm-vt -e normal --no-default-features
==> cargo run -p oneterm-vt --example headless
==> cargo doc -p oneterm-vt --no-deps
==> cargo doc -p oneterm-vt --no-deps --all-features
==> python scripts/vt-public-api.py --check --no-doc
==> python scripts/vt-public-api.py --check-nameable --no-doc
==> python scripts/vt-public-api.py --diff-platforms
==> cargo package -p oneterm-vt --list | verify-dependency-graph.py --package-list -
==> rustdoc self-containment (crates/vt/src)
==> rustdoc self-containment (crates/vt/docs/guide)
==> python scripts/verify-dependency-graph.py
==> python scripts/check-doc-paths.py
==> python -m unittest scripts/test_check_english.py
==> python scripts/check-english.py
==> python scripts/completion-catalog.py validate
==> python scripts/third-party-notices.py --check
ci-local: all checks passed.
```

Totals over the four test steps, **131 sections, 4533 passed, 0 failed, 24 ignored**:

| step | sections | passed | failed | ignored |
| --- | --- | --- | --- | --- |
| `cargo test --workspace` | 71 | 2046 | 0 | 12 |
| `cargo test -p oneterm-vt --features vt-paranoid` | 20 | 834 | 0 | 4 |
| `cargo test -p oneterm-vt --features regex` | 20 | 847 | 0 | 4 |
| `cargo test -p oneterm-vt --no-default-features` | 20 | 806 | 0 | 4 |

The integration proof is the `oneterm-local-shell` section inside `cargo test --workspace`,
where the binary runs alongside the rest of the gate rather than alone -- the condition that
produced the CI failure:

```text
running 35 tests
test session::session_tests::mouse_drag_updates_selection_not_mouse_move ... ok
test session::session_tests::spawned_shell_exit_is_detected ... ok
test session::session_tests::selection_text_and_clear ... ok
test result: ok. 33 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 8.36s
```

`python scripts/check-doc-paths.py` and `python scripts/check-english.py` were also run
directly against this packet before the gate: "Doc path check passed for 199 current paths
in 11 documents" and "English contributor-text check passed for 925 files".

### Gaps

1. **Not reproduced on a two-vCPU Linux runner.** The failure is a CI observation. The host
   is Windows, where `LocalShellConfig::default()` resolves to `cmd.exe` and not to the
   `bash -l` the runner uses (`crates/core/src/config/shell.rs:281-302`), so the absolute
   numbers cannot transfer -- a login `bash` sources `/etc/profile` and `/etc/profile.d/*`
   on startup and `~/.bash_logout` on exit, none of which `cmd.exe` does. What transfers is
   the shape, and the shape is confirmed: 7x from concurrency alone. The new bound is 37x
   the host worst case and under 4x the observed CI failure, which is the honest weak point
   of this fix -- see gap 2. The proof is the owner's next push.
2. **A bound is still a bound.** If ubuntu-latest exceeds 15 s, the conclusion is not
   "raise it again": it is that a shell round trip on that runner is not merely slow, and
   the new message will say which -- an empty snapshot means the shell never started, a
   populated one with `alive=true` means the exit never came back through the reaper thread
   and the `PTY_CHILD_EVENT_TOKEN` branch (`crates/vt/src/pty/unix.rs:201-219`,
   `crates/local-shell/src/event_loop.rs:377-385`), which is a real defect and a new packet.
3. **One stopwatch is left in the file on purpose.**
   `close_returns_without_joining_the_owner_thread` asserts
   `elapsed < Duration::from_millis(500)` for `close()` itself (`:213`). That one *is* a
   latency assertion and is the whole point of the test (CORR-10: `close()` must not join
   the owner thread), so it cannot be widened without deleting what it proves. It is a
   candidate for a future flake on a loaded runner, with no fix that keeps the property.
   Recorded, not changed.
4. **`US-0083` gap 6 is still open.** The `TerminalHandle::lock_for_render` lost-demand race
   is untouched and owned by `crates/terminal`. It does not affect child-exit delivery,
   which never takes that path.

## Harness Record

This packet was written in an isolated worktree, so `harness.db` in the main checkout was
not touched. Run this from the repository root to insert the row (Python 3, standard
library only):

```python
import sqlite3

with sqlite3.connect("harness.db") as db:
    db.execute(
        """INSERT OR REPLACE INTO story (
            id, title, created_at, risk_lane, contract_doc, packet_doc, status,
            unit_proof, integration_proof, e2e_proof, platform_proof, evidence,
            verify_command, last_verified_at, last_verified_result, notes, intake_id
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
        (
            "BUG-0066",
            "The real-shell round-trip bounds are stopwatches and a loaded CI runner trips them",
            "2026-09-16",
            "normal",
            "docs/spec-intakes/IN-0029-vt-engine/US-0083-local-shell-native.md",
            "docs/spec-intakes/IN-0029-vt-engine/BUG-0066-shell-exit-detection-bound-fails-on-ci.md",
            "implemented",
            1, 1, 0, 0,
            "Four real-shell waits in session_tests.rs moved to one SHELL_ROUND_TRIP = 15 s constant; spawned_shell_exit_is_detected now reports elapsed, alive() and the trimmed snapshot. Measured pinned to two logical CPUs at --test-threads=8: worst waits 405/437/415/169 ms against old bounds of 4/2/2/6 s, so the two 2 s echo tests had less headroom than the one that failed. The flood test is not the starver (median 259 ms with it, 212 ms without, 37 ms at --test-threads=2), so the shared-guard option was rejected on measurement. 10/10 runs green after. pwsh scripts/ci-local.ps1 passed.",
            "pwsh scripts/ci-local.ps1",
            "2026-09-16",
            "pass",
            "Test-only change, no production code. Not reproduced on a two-vCPU Linux runner; the owner push is the proof. The 500 ms close() latency assertion is a deliberate remaining stopwatch. US-0083 gap 6 still open and owned by crates/terminal.",
            34,
        ),
    )
```

## Handoff

Next action is the owner's: push, and read the ubuntu-latest job. A repeat failure now
arrives with elapsed time, `alive()` and the snapshot in the message: an empty snapshot is a
shell that never started, a populated snapshot with `alive=true` is an exit that never came
back through the reaper, and neither is this packet's ceiling -- both are new packets against
`crates/vt` or `crates/local-shell`.
