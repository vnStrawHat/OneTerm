# Independent verification: BUG-0066

Packet: `docs/spec-intakes/IN-0029-vt-engine/BUG-0066-shell-exit-detection-bound-fails-on-ci.md`
Commit under test: `d96b44b0` (parent `a6a72ac3`, branch `worktree-agent-afebdf4e8c427072c`)
Verifier: independent adversarial pass, separate worktree, no shared state with the author's run.
Host: Windows 11, 8 logical CPUs. Every timed run pinned to two logical CPUs
(`ProcessorAffinity = 3`) on a process this session started itself. No process was
enumerated by name and none was terminated.

## Verdict

**PASS-WITH-NOTES.**

The fix is correct, complete for its declared scope, and not a weakening: both negative
controls reproduce verbatim, every real-shell bound in the file is accounted for, and the
`close()`-before-return claim that justifies the two untouched waits is true in the source.
The independent measurement reproduces the packet's conclusion and, on the flood-test
question, reproduces it *more strongly* than the packet's own numbers do.

Three defects, none blocking: one MEDIUM factual error in the stated starvation mechanism
(contradicted by `Drop for LocalSession`, and by this verifier's process census), one
MEDIUM omission of a standing review item that proposes a different remedy for exactly this
problem, and three LOW items (a 47 ms delta that does not reproduce, an off-by-one
citation, an eager `snapshot()` on a success path).

| # | Attack | Result |
| --- | --- | --- |
| 1 | Reproduce the measurement independently | PASS — conclusion confirmed, one sub-claim not reproducible (D3) |
| 2 | Is the fix complete? | PASS — inventory exhaustive; one unmeasured bound measured here and cleared |
| 3 | Is anything weakened? | PASS — both negative controls reproduce |
| 4 | Does the enriched message help, and is `snapshot()` safe there? | PASS — all three fields print; safe (D5 is cosmetic) |
| 5 | Packet honesty | PASS-WITH-NOTES — D1, D2, D4 |
| 6 | `pwsh scripts/ci-local.ps1` | PASS |

## Defects

| ID | Grade | Where | Finding |
| --- | --- | --- | --- |
| D1 | MEDIUM | packet `:170-172`, commit message ¶2 | "Every test here spawns a real shell and **most never close it**, so by the time a late test runs there are around twenty live shells" — false as stated. `Drop for LocalSession` (`crates/local-shell/src/session.rs:216-232`) calls `pty_close()` synchronously on every drop, so a session torn down at the end of its test does not survive the binary. A live process census (below) shows the population **peaks mid-run and drains to 2**, and scales with `--test-threads` (23 processes at 8, 12 at 2) rather than accumulating. The remedy is unaffected and the discriminating experiment (2 vs 8 threads) still holds; only the stated mechanism is wrong. It is repeated in the commit message, so a future reader inherits it. |
| D2 | MEDIUM | packet "Owning Docs Reviewed" / "Reconciliation" | `docs/review-refresh-2026-08/06-testing.md:23-27` (**TEST-02**, still unchecked) is not listed, although it matches the packet's own stated scan key (`session_tests.rs`) and this verifier's identical scan found it. It is the one repo document that both records this class as "already flaky (commit c7a757b)" **and** prescribes a different remedy — "make `run` generic over `EventedReadWrite` and drive it with a pipe/fake; keep one real-shell smoke test behind `#[ignore]`". The packet weighs hypotheses (a)/(b)/(c) from the brief but never this standing one, and "Every hit was read" is therefore not accurate. No contract moves (TEST-02 quotes no bound), so this is a completeness defect, not a correctness one. |
| D3 | LOW | packet `:157-165`, commit message ¶2 | "the exit wait medians 259 ms with the flood test and 212 ms without it" — the 47 ms flood-test penalty does not reproduce. Five runs each here: median **206 ms with** the flood test, **239 ms without** it, worst 320 ms vs 382 ms. The delta is inside run-to-run noise and here points the other way. The *conclusion* ("the flood test is not the starver") is strengthened, not damaged; only the quantification is unreproducible, and the packet presents it as a measured quantity rather than a null result. |
| D4 | LOW | packet `:106-108` | The `US-0062` citation is `:184-187`; the flake record is actually at `:185-188` (`:184` is the preceding clippy line). The quoted substance is inside the range. The `BUG-0051:268-270` citation is exact. |
| D5 | LOW | `crates/local-shell/src/session_tests.rs:406` | `let lines = snapshot_lines(&s);` in `e2e_echo_output_rendered_in_snapshot` is **eager**: it runs on the success path too, and it is the only one of the four that is not inside the lazily formatted `assert!` argument list. It is harmless (it only consumes render damage, and no renderer runs in tests — the file already documents that at `:140`), but it clones the grid on every green run for a message that is almost never printed. It is written that way because it has to precede `s.close()`; a `let lines = (!found).then(|| snapshot_lines(&s));` would keep the ordering without the cost. |

No HIGH defects.

---

## Attack 1 — independent reproduction of the measurement

**Method.** Temporary `eprintln!` probes around all four waits (`exit`, `hello`,
`hello_world`, `e2e`), built, measured, then the file restored from a byte-for-byte copy
of `HEAD`'s version; `git status --porcelain` is empty and `git diff --stat` shows nothing
after the restore. Each run:

```powershell
$p = Start-Process -PassThru -NoNewWindow -FilePath target\debug\deps\oneterm_local_shell-f95f1dbcacce61ac.exe `
     -ArgumentList $args -RedirectStandardError $err -RedirectStandardOutput $out
$p.ProcessorAffinity = 3      # two logical CPUs
$p.WaitForExit()
```

Five runs per configuration, fifteen runs total, **every run exit code 0**.

### Per-wait results (ms)

| cfg | probe | n | median | worst | all |
| --- | --- | --- | --- | --- | --- |
| `--test-threads=2` | exit | 5 | **43** | 47 | 36, 38, 43, 45, 47 |
| `--test-threads=2` | hello | 5 | 54 | 70 | 49, 52, 54, 58, 70 |
| `--test-threads=2` | hello_world | 5 | 51 | 66 | 50, 50, 51, 64, 66 |
| `--test-threads=2` | e2e | 5 | 44 | 51 | 28, 40, 44, 48, 51 |
| `--test-threads=8` | exit | 5 | **206** | 320 | 141, 195, 206, 284, 320 |
| `--test-threads=8` | hello | 5 | 277 | 296 | 171, 245, 277, 285, 296 |
| `--test-threads=8` | hello_world | 5 | 104 | 169 | 91, 103, 104, 105, 169 |
| `--test-threads=8` | e2e | 5 | 107 | 137 | 83, 92, 107, 125, 137 |
| `--test-threads=8 --skip a_flooding_loop` | exit | 5 | **239** | 382 | 219, 230, 239, 308, 382 |
| `--test-threads=8 --skip a_flooding_loop` | hello | 5 | 213 | 252 | 93, 116, 213, 222, 252 |
| `--test-threads=8 --skip a_flooding_loop` | hello_world | 5 | 69 | 97 | 61, 63, 69, 72, 97 |
| `--test-threads=8 --skip a_flooding_loop` | e2e | 5 | 71 | 90 | 65, 70, 71, 76, 90 |

### Does the data support the packet's two claims?

**"Concurrent live shells are the starver" — CONFIRMED.** Exit wait median 43 ms at two
test threads against 206 ms at eight: a **4.8x** jump from thread count alone, on the same
pin and the same binary. The packet reports 37 → 259 ms (7.0x). Same effect, same order of
magnitude; the difference between 4.8x and 7x is ten-run versus five-run sampling of a
wide distribution.

**"The flood test is not the starver" — CONFIRMED, more strongly than the packet claims.**
Removing `a_flooding_loop_hands_the_engine_to_a_waiting_frame` did not reduce the exit wait
at all here — the median rose from 206 ms to 239 ms and the worst from 320 ms to 382 ms.
The packet's own framing ("removing the flood test moves the median by about 47 ms out of
259") therefore overstates a difference this verifier cannot find at all. The decision it
supports — reject hypothesis (b), the shared quiet-machine lock — is correct either way,
and is better supported by these numbers than by the packet's. Recorded as D3 because a
null result presented as a measured 47 ms penalty is still a misstatement.

### Where my numbers and the packet's disagree on magnitude

My worst case across fifteen pinned runs is below the packet's worst across twenty, for
every wait. The packet's table is therefore **not inflated** — it is conservative, which is
the safe direction for a ceiling argument.

| wait | packet worst | this run's worst | old bound | headroom (this run) |
| --- | --- | --- | --- | --- |
| `hello` | 437 ms | 296 ms | 2 s | 6.8x |
| `hello_world` | 415 ms | 169 ms | 2 s | 11.8x |
| exit | 405 ms | 382 ms | 4 s | 10.5x |
| `oneterm_e2e` | 169 ms | 137 ms | 6 s | 43.8x |

One ordering claim does not fully reproduce. The packet says the two 2 s echo waits are
both tighter than the 4 s exit wait. Here `hello` is indeed the tightest (6.8x), but
`hello_world` is not tight at all (11.8x) and the exit wait sits between them. The claim
"the test that failed on CI is not the one with the least margin" survives on both
datasets, because `hello` is the least-margin wait in both. The per-test ranking below that
is noise-limited and should not have been stated as firmly as it was.

### Process census — testing D1 directly

Because the packet's causal sentence is about *how many shells are alive*, this verifier
measured that instead of assuming it. Direct children of the test pid only
(`Get-CimInstance Win32_Process -Filter "ParentProcessId = <own spawned pid>"`), sampled
every 150 ms, read-only, nothing terminated:

| `--test-threads` | peak direct children | composition at peak | tail of run |
| --- | --- | --- | --- |
| 8 | **23** | `cmd.exe x10, conhost.exe x12, pwsh.exe x1` | drains to 2 for the last ~3 s |
| 2 | **12** | `cmd.exe x5, conhost.exe x6, powershell.exe x1` | drains to 2 for the last ~5 s |

So: the packet's "~twenty" matches the **process** count at eight threads (23), not the
shell count (~11). The population **scales with test-threads and drains** rather than
accumulating, which is what `Drop for LocalSession` predicts and what the packet's own
sentence denies. The real mechanism is *concurrent tests plus teardown lag* — at eight
threads ~11 shells are alive for eight running tests, so teardown lag adds roughly three.
This does not change the fix, and it does not change the rejection of hypothesis (b).

---

## Attack 2 — is the fix complete?

**Inventory method.** Python walk of `crates/local-shell/src` (this worktree has no `rg`)
for `wait_until(`, `from_secs(`, `from_millis(`, `Instant::now`, `.elapsed()`, cross-checked
against the same scan of the parent commit's file.

Every bound in the before-state (`a6a72ac3:crates/local-shell/src/session_tests.rs`):
`:108` 15 s, `:165` 2 s, `:182` 500 ms, `:189` 2 s, `:273` 2 s, `:294` 2 s, `:351` 4 s,
`:362` 6 s. The packet's "Every real-shell wait" table lists exactly these eight and
nothing else. **The inventory is exhaustive.**

| after `:` | bound | disposition | verified |
| --- | --- | --- | --- |
| `:139` | `SHELL_ROUND_TRIP` | was 15 s, value unchanged | yes |
| `:196` | 2 s, `!alive()` after `close()` | left | **claim verified in source** (below) |
| `:213` | 500 ms, `close()` latency | left (packet gap 3) | **measured here**, see below |
| `:220` | 2 s, `!alive()` after `close()` | left | same as `:196` |
| `:305` | `SHELL_ROUND_TRIP` | was 2 s | yes |
| `:328` | `SHELL_ROUND_TRIP` | was 2 s | yes |
| `:388` | `SHELL_ROUND_TRIP` | was 4 s | yes |
| `:405` | `SHELL_ROUND_TRIP` | was 6 s | yes |

**The `close()` claim is true.** `crates/terminal/src/session.rs:742-745`:

```rust
fn close(&self) -> Result<(), TerminalError> {
    let result = self.owner.close();
    self.state.set_alive(false);
    result
}
```

`set_alive(false)` runs before the function returns, so `wait_until(_, || !s.alive())`
after a local `close()` is satisfied at its first poll and cannot time out under any load.
Leaving `:196` and `:220` at 2 s is correct, and widening them would have been noise, as
the packet says. (If `self.owner.close()` itself hung, the test would hang regardless of
the bound — and `:213` is the assertion that covers that.)

**`event_loop_tests.rs` — checked, correctly out of scope.** Its waits (`:403` 5 s, `:408`
2 s, `:457` 2 s, `:468` 5 s, `:559` 10 s, `HANDOVER_BOUND` 1 s from `BUG-0064`) all drive a
**loopback-socket peer**, not a real shell: `start_loop()` builds the pump over a socket
pair. None is a real-shell round trip and none belongs in this packet.

**`session_orphan_tests.rs` — out of scope, and the packet's reason is weaker than the real
one.** It does contain one genuine real-shell round trip, `:175`, waiting 10 s for first
output. The packet dismisses both its bounds as "a different instrument". The stronger
reason it does not give: the module is `#[cfg(all(test, windows))]`
(`crates/local-shell/src/session.rs:240`), so it never runs on the ubuntu runner that
failed, and 10 s against a 296 ms worst case is 34x anyway. `LIVENESS_BOUND = 5 s` is the
orphan property itself and cannot be widened without deleting what it proves. Out of scope
is the right call.

**The one remaining bound with plausible risk, measured.** Packet gap 3 correctly names
`close_returns_without_joining_the_owner_thread:213` (`elapsed < 500 ms` for `close()`
itself) as a deliberate remaining stopwatch and a future flake candidate — but never puts a
number on it. This verifier did, since the brief asks for anything under 5x headroom over
the measured worst:

| cfg | `close()` elapsed, 5 runs | worst | headroom vs 500 ms |
| --- | --- | --- | --- |
| `--test-threads=8`, pinned to 2 CPUs | 35, 42, 34, 38, 40 **µs** | **42 µs** | **~11,900x** |

So gap 3 is honest in kind but hugely over-cautious in degree: `close()` under the held
`Term` lock costs tens of *microseconds*, four orders of magnitude under its 500 ms bound,
because `close()` does not wait for anything — it posts the shutdown, hands the join handle
to the reaper (`reap_owner_thread`, `crates/local-shell/src/session.rs`) and returns. It is
not a realistic flake candidate on any runner, and the packet was right to leave it alone.
Worth adding to the record so the next person does not widen it on suspicion.

No other bound in the touched file is under 5x headroom over this verifier's worst
measurement.

---

## Attack 3 — is anything weakened?

A 15 s wait must still fail for a shell that never delivers its exit. Both of the packet's
negative controls were reproduced from scratch (temporary edits, each reverted; neither is
in any commit).

**Control A — bound cut to 1 ms.** `const SHELL_ROUND_TRIP: Duration = Duration::from_millis(1);`

```text
thread 'session::session_tests::spawned_shell_exit_is_detected' (25396) panicked at
crates\local-shell\src\session_tests.rs:389:5:
shell exit not detected in 5.1309ms (bound 1ms); alive=true at the end; terminal snapshot,
blank lines dropped: []
test result: FAILED. 0 passed; 1 failed
```

Packet's recorded control: `... in 6.2763ms (bound 1ms); alive=true at the end; ... []`.
Identical but for the elapsed reading. This is the "shell never started" signature the
packet advertises, and it proves the assertion still fires.

**Control B — unsatisfiable predicate at the shipped 15 s bound.**
`wait_until(SHELL_ROUND_TRIP, || !s.alive() && false)`

```text
thread 'session::session_tests::spawned_shell_exit_is_detected' (13056) panicked at
crates\local-shell\src\session_tests.rs:389:5:
shell exit not detected in 15.0160211s (bound 15s); alive=false at the end; terminal
snapshot, blank lines dropped: ["C:\\Users\\trunglt>"]
test result: FAILED. 0 passed; 1 failed; finished in 15.03s
```

Packet's recorded control: `... in 15.0120911s (bound 15s); alive=false at the end; ...
["C:\\Users\\trunglt>"]`. Byte-identical except the elapsed reading.

The full bound elapses and the test still fails, so nothing the test was written to catch
survives the change — the property is "detected at all", and `wait_until` re-evaluates the
predicate once more after the deadline (`:51`), so a shell that exits at 14.9 s still
passes and one that never exits still fails. Not weakened.

After both controls the file was restored from a pre-edit copy; `git status --porcelain`
empty.

---

## Attack 4 — does the enriched message help, and is `snapshot()` safe there?

**All three data points print.** Both controls above show `elapsed`, the `alive()` re-read
at message time, and the trimmed snapshot, and the two controls **disagree usefully**, which
is the whole point: `alive=true` + `[]` is a shell that never started; `alive=false` +
a populated snapshot is an exit that arrived and the wait missed. The packet's attribution
story (empty snapshot ⇒ never started; populated + `alive=true` ⇒ exit never came back
through the reaper) is supported by the two observed signatures.

**`snapshot()` is safe there.** It consumes and resets render damage
(`TerminalRender::snapshot` rustdoc, `crates/terminal/src/session.rs`), which matters only
to a renderer, and no renderer runs in this binary — the file already states and relies on
this at `:140` ("`snapshot()` consumes render damage, which is fine here: no renderer
runs"). It cannot block any differently from the wait loop itself, because
`snapshot_contains` (`:56-58`) already calls `session.snapshot().text()` on every 5 ms
poll of three of these four waits; the failure-path call is the same call, once more.
`PtySession::snapshot` delegates straight to `self.model().snapshot()`.

Three of the four call sites are inside `assert!`'s format arguments and so are evaluated
only on failure. The fourth (`:406`) is eager — see D5.

One nit that is not a defect: `started` in `spawned_shell_exit_is_detected` is taken
*before* `s.write(b"exit\r")` (`:386-388`), so the printed elapsed includes the write. That
is the more useful number for a CI log (it is the whole round trip) and it is what the
message says it is.

---

## Attack 5 — packet honesty

| Claim | Check | Result |
| --- | --- | --- |
| CI panic at `session_tests.rs:350:5` | `a6a72ac3:crates/local-shell/src/session_tests.rs:350` is `assert!(` at column 5, with `from_secs(4)` on `:351` and `"shell exit not detected after 4s"` on `:352` | **exact** |
| `BUG-0051:268-270` records a flake of `mouse_drag_updates_selection_not_mouse_move` on its 2 s wait | read; the sentence spans exactly `:268-270`, "concurrent with the GUI captures … two isolated runs passed; the crate is untouched" | **exact** |
| `US-0062:184-187` records a flake of `selection_text_and_clear` on a 2 s wait | read; the record is at `:185-188` (`:186-187` carry the test name and "2 s wait") | **off by one** (D4) |
| Test↔packet mapping of the two flakes | `BUG-0051` → `mouse_drag_…`, `US-0062` → `selection_text_and_clear`; the packet's Context table maps them that way round | **correct** |
| `US-0083:193-194` names three of these tests as "failed immediately", quotes no bound | read `:190-196` | **correct**, no-change reason holds |
| `US-0083` gap 6 at `:349-372`, `lock_for_render` lost-demand race, unrelated to child exit | read; gap 6 starts at `:349`; it is a render-path race | **correct** |
| `docs/terminal-backend.md:525` — "real-shell tests in `session_tests.rs` are what cover it", no bound | read | **correct** |
| `15 s was already this file's PowerShell prompt bound` | parent `:108` is `wait_until(Duration::from_secs(15), …)` | **correct** |
| Reconciliation: only two documents quote a moved bound | **independent** Python scan of `docs/`, `crates/`, `scripts/`, `.github/` for the four test names, `SHELL_ROUND_TRIP`, `session_tests.rs`, `assert_powershell_prompt_emits_cwd` | **two quoting hits reproduced**, plus one unlisted hit → D2 |
| "Most never close it … around twenty live shells" | `Drop for LocalSession` + live process census | **false as stated** → D1 |
| Flood test costs ~47 ms of the median | five runs per configuration | **does not reproduce** → D3 |
| Harness snippet shape | 17 column names, 17 `?` placeholders, 17 values; `status="implemented"`, `risk_lane="normal"`, `last_verified_result="pass"`, proofs `(1,1,0,0)` matching the `HARNESS:PROOF` block, `intake_id=34` matching sibling `BUG-0064`, `contract_doc`→`US-0083`, `packet_doc`→itself | **consistent** |
| `risk_lane` matches the packet's own Classification | Classification `:23` says `normal`; snippet says `"normal"`; `normal` is the dominant lane across 44 packets (the sibling `BUG-0064`'s `"standard"` is the outlier) | **consistent** |

The packet's `harness.db` note ("written in an isolated worktree, so `harness.db` in the
main checkout was not touched") is correct — there is no `harness.db` in this worktree
either, and this verifier did not touch the main checkout's.

Scope discipline is clean: `git show d96b44b0 --stat` is two files,
`crates/local-shell/src/session_tests.rs` (+68/−13) and the packet. No production code.

---

## Attack 6 — `pwsh scripts/ci-local.ps1`

Run in this worktree on the commit under test, Windows host, no `--full`,
`CARGO_BUILD_JOBS=4`, with this evidence file already present in the tree. **Exit code 0,
all 25 steps passed**, final line `ci-local: all checks passed.`

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

Totals over the four test steps, **131 sections, 4533 passed, 0 failed, 24 ignored** —
identical, step for step, to the packet's recorded run:

| step | sections | passed | failed | ignored |
| --- | --- | --- | --- | --- |
| `cargo test --workspace` | 71 | 2046 | 0 | 12 |
| `cargo test -p oneterm-vt --features vt-paranoid` | 20 | 834 | 0 | 4 |
| `cargo test -p oneterm-vt --features regex` | 20 | 847 | 0 | 4 |
| `cargo test -p oneterm-vt --no-default-features` | 20 | 806 | 0 | 4 |

The integration proof — the `oneterm-local-shell` section under whole-workspace load, which
is the condition that produced the CI failure:

```text
running 35 tests
test session::session_tests::close_returns_without_joining_the_owner_thread ... ok
test session::session_tests::e2e_echo_output_rendered_in_snapshot ... ok
test session::session_tests::mouse_drag_updates_selection_not_mouse_move ... ok
test session::session_tests::selection_text_and_clear ... ok
test session::session_tests::trait_alive_is_local_close ... ok
test session::session_tests::spawned_shell_exit_is_detected ... ok
test session::session_tests::pwsh_prompt_emits_cwd_without_parser_errors ... ok
test session::session_tests::windows_powershell_prompt_emits_cwd_without_parser_errors ... ok
test result: ok. 33 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 10.31s
```

All four moved tests, both untouched `!alive()` waits and the 500 ms `close()` latency
assertion pass together. `python scripts/check-doc-paths.py` and
`python scripts/check-english.py` were also run directly against this evidence file before
the gate: "Doc path check passed for 199 current paths in 11 documents" and "English
contributor-text check passed for 926 files" (the packet's run saw 925; the extra file is
this one).

### Run hygiene

Every timed run and both negative controls used a temporary edit that was reverted from a
pre-edit copy of `HEAD`'s file. `git status --porcelain` after all measurement shows exactly
one entry — this evidence file. No probe, no bound change and no `eprintln!` is committed.
Across this verification, 27 test-binary processes were started by this session and waited
on with `WaitForExit()`; none was terminated and none was matched by name.

---

## Summary of what this verification adds to the record

1. The packet's central decision (reject the shared quiet-machine lock; raise one shared
   ceiling) is **independently reproduced** and, on the flood-test question, better
   supported by this verifier's data than by the packet's own.
2. The packet's *mechanism* sentence is wrong and its supporting sub-number does not
   reproduce (D1, D3). Neither changes the fix; both would mislead the next reader.
3. `docs/review-refresh-2026-08/06-testing.md` TEST-02 is a standing, unchecked review item
   proposing a fake-transport remedy for exactly this class of flake. It should be named in
   the packet's Owning Docs Reviewed, either adopted or explicitly declined (D2).
4. The one bound the packet flagged as an unmeasured future flake risk (`:213`, the 500 ms
   `close()` latency assertion) now has a number: **42 µs worst of five pinned runs, about
   11,900x headroom**. Gap 3 can be downgraded from "a candidate for a future flake" to
   "measured, not at risk".
5. The gate was re-run end to end and reproduces the packet's recorded totals exactly
   (25 steps, 131 sections, 4533 passed, 0 failed, 24 ignored), so the packet's Evidence
   section is not a transcription of a different run.
