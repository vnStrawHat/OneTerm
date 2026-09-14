# US-0083 — independent verification

Verifier: independent agent, 2026-09-13. Branch `worktree-agent-a20da8012abddb9c7`
(3 commits off `feat/vt-engine` @ `d3c537b`). **Nothing was fixed and nothing was
committed.** Every scratch edit below was reverted; `git status --porcelain` is empty
and `git diff d3c537b...HEAD --stat` is unchanged at 6 files / 634 insertions.

Environment: `Get-PSDrive D` free **43.5 GiB** (> 15 GiB gate). `quser` reports the only
session (`trunglt`, id 1) **Disc**, idle 11:43 — no Active desktop, so no GUI walk (see
§7). The owner's `oneterm.exe` was never enumerated, never signalled, never stopped; the
only processes killed were my own `oneterm_local_shell-*.exe` test binaries and `cargo`.

## Verdict

**Merge after a documentation fix.** The behaviour is correct, the handshake works, the
stall the packet documents is real and the shipped loop avoids it, CI is green and
reproducible, and on the transport this crate actually ships on the worst frame wait is
**52–59 µs**, not 86–125 ms. The one change I would require before merge is MAJOR‑1: the
"handshake, measured" table records a fixture artifact as a property of the loop, and
`US-0084` is explicitly pointed at that table.

## Pass/fail table

| # | Check | Result |
|---|---|---|
| 1 | Scope: `crates/local-shell` + two docs only | **PASS** |
| 1 | Trailer exact on all three commits | **PASS** |
| 2 | `pwsh scripts/ci-local.ps1` green | **PASS** |
| 2 | Local-shell suite × 5, real ConPTY shells, no flake | **PASS** |
| 2 | Handshake negative control (`if false &&`) fails as claimed | **PASS** |
| 3 | 86–125 ms worst frame wait attributed | **PASS (and refuted as a loop property)** |
| 3 | Cheapest improvement measured | **PASS** — 10× better, proposed not committed |
| 4 | `break` stall reproduced | **PASS** |
| 4 | Shipped loop cannot stall (every exit path proven) | **PASS** |
| 4 | Ring wake semantics judged | **PASS** — latent `oneterm-pty` bug, own packet |
| 5 | Child exit, resize (idle), input FIFO, colour reply, shutdown covered | **PASS** |
| 5 | OSC colour queries answered **in order**, outside the lock | **PASS** (scratch test) |
| 5 | Ctrl-C delivery under a real flood | **PASS** (scratch test) |
| 5 | Resize during a sustained flood | **FAIL on a fast transport** (MINOR‑4) |
| 5 | `chcp` / UTF-8 across chunks | **NOT ESTABLISHED** — probe unreliable (§7) |
| 6 | Throughput trade confirmed on my own run | **PASS** |
| 6 | Ring never fills / child never back-pressured | **PASS** |
| 7 | Gaps 1–2 recorded as `US-0085`'s | **PASS** |
| 7 | GUI walk | **N/A** — session Disc, recorded |
| 8 | No `unwrap`/`expect` on runtime paths, no dead code | **PASS** |
| 8 | Docs accurate | **PASS** (one nit, MINOR‑5) |
| 8 | Harness DB row `US-0083` | **PASS** |

---

## 1. Scope and trailers

```
$ git diff d3c537b...HEAD --name-only
crates/local-shell/Cargo.toml
crates/local-shell/src/event_loop.rs
crates/local-shell/src/event_loop_tests.rs
crates/local-shell/src/session_terminal.rs
docs/spec-intakes/IN-0029-vt-engine/US-0083-local-shell-native.md
docs/terminal-backend.md
```

All three commits (`7e7787a`, `06fa6ab`, `5f9cc1e`) end with exactly:

```
Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01C1Vip1PKvU8ayf3PZ4jPM9
```

Nothing outside `crates/local-shell` and the two named docs. **PASS.**

## 2. The gate

`pwsh scripts/ci-local.ps1` → `ci-local: all checks passed.` **EXIT=0**, all ten steps.

Raw totals recomputed from the log, per step:

```
workspace:   sections=58 passed=1550 failed=0 ignored=11
vt-paranoid: sections=4  passed=369  failed=0 ignored=3
total:       62 sections, 1919 passed / 0 failed / 14 ignored
```

Identical to the packet's recorded totals. No `FAILED` line anywhere in the log.

**Five consecutive local-shell runs** (these spawn real `cmd.exe` through ConPTY):

```
run 1 : test result: ok. 31 passed; 0 failed; 1 ignored
run 2 : test result: ok. 31 passed; 0 failed; 1 ignored
run 3 : test result: ok. 31 passed; 0 failed; 1 ignored
run 4 : test result: ok. 31 passed; 0 failed; 1 ignored
run 5 : test result: ok. 31 passed; 0 failed; 1 ignored
```

**No flake.** 5/5 identical.

**Negative control.** Scratch edit `if false && self.term.take_render_demand()`, reverted:

```
test event_loop::event_loop_tests::a_flooding_loop_hands_the_engine_to_a_waiting_frame ... FAILED
panicked at crates\local-shell\src\event_loop_tests.rs:586:9:
no frame reached the engine within 4.75s of a flooding pump
test result: FAILED. 0 passed; 1 failed; finished in 4.92s
```

Exactly the packet's claim (it recorded 4.99 s). **PASS.**

---

## 3. The latency question

### 3.1 What the loop actually feeds per lock hold

`event_loop.rs:392` reads into `buf[unprocessed..]` where `buf` is `READ_BUFFER_SIZE`
= **1 MiB**, and `:432` makes exactly one `pump.advance(engine, &buf[..unprocessed])`
per read. So the bytes fed per lock hold = whatever that one `read` returned, which is
`min(transport chunk, 1 MiB)`. The demand is checked once **after** that feed returns
(`:442`), not inside it. The guard is dropped at `:456` and the colour replies are
written at `:457`, i.e. **after** the unlock — the reply write adds nothing to the hold;
only `color_replies(&guard, …)` runs under it, and only when queries exist.

So: **worst frame wait ≈ (bytes per lock hold) ÷ (parse rate).** Everything below follows
from that one identity.

### 3.2 Measured — real ConPTY (the transport this crate ships on)

Scratch instrumentation (all reverted): `SCRATCH_RING_{HIGH,FULL,BYTES}` in
`crates/pty/src/windows/pipe.rs`, `SCRATCH_{FEEDS,YIELDS,TRYLOCK_MISS,BLOCKING_LOCK}` in
`event_loop.rs`, and a renderer thread calling `lock_for_render()` every 16 ms — which
raises the demand and then blocks, so the sample *is* "demand raised → lock obtained".
`--profile fast-dev`, 5 s window, UI event queue drained on its own thread.

```
== type (9.6 MB file) ==
ring bytes      9165368 (8.74 MiB, 1.7 MiB/s)
ring high-water 1517 / 1048576 bytes capacity   ring-full blocks 0
fed bytes       9163916 over 99782 advances   p50=82 p90=132 p99=132 max=1517 bytes/hold
yields          309   try_lock misses 143   blocking locks 0
frame waits     n=308 min=0us p50=0us p90=10us p99=38us MAX=52us

== type (repeat) ==
fed bytes       8960776 over 97711 advances   p50=82 p90=132 p99=132 max=9644 bytes/hold
frame waits     n=307 min=0us p50=0us p90=9us  p99=29us MAX=59us

== yes (cmd `for /l ... @echo yes`) ==
ring high-water 170 / 1048576 bytes capacity   ring-full blocks 0
fed bytes       335169 over 64394 advances   p50=5 p90=5 p99=5 max=170 bytes/hold
frame waits     n=308 p50=0us p90=0us p99=15us MAX=54us
```

**Worst frame wait on real ConPTY: 52–59 µs.** ConPTY does not deliver 32 KiB WriteFile
chunks for these workloads — it delivers **82 bytes** at the median (one 80-column line
plus CRLF) and 9.6 KB at the very worst. The 1 MiB read buffer is never filled, the ring
never exceeds 9.6 KB of its 1 MiB, and the limiter is conhost (0.07–1.7 MiB/s), not the
pump.

### 3.3 Measured — the loopback fixture (where the packet's number comes from)

Same loop, same code, TCP-socket PTY:

| profile | bytes/hold p50 | bytes/hold max | throughput | frame wait p50 | MAX |
|---|---|---|---|---|---|
| `fast-dev` | 618 KB | 868 KB | 51.3 MiB/s | 5.4 ms | 18.6 ms |
| `fast-dev` (repeat) | 659 KB | 1048576 | 41.4 MiB/s | 9.6 ms | 22.6 ms |
| `test` (opt-level 0) | 634 KB | 876 KB | 5.1 MiB/s | **99.5 ms** | **161 ms** |

The `test`-profile row **reproduces the packet's 86 / 108 / 125 ms exactly**, and the
identity closes: 634 KB ÷ 5.1 MiB/s ≈ 118 ms.

### 3.4 Attribution

The 86–125 ms is **not** a property of the shipped loop. It is
`bytes-per-lock-hold × parse-rate`, and the only variable that moved is the transport:
a TCP socket hands the loop 600 KB per `read`, ConPTY hands it 82 bytes. The debug
profile multiplies the parse side by ~10 on top. The shipped Windows local shell is
**~1500× faster** than the recorded figure.

Is 86 ms acceptable for a 60 Hz renderer? **No** — a frame waits 5+ frame periods, and
161 ms is ten. But that number belongs to a socket transport, which is precisely what
`US-0084` is about to write this same `if` against.

### 3.5 Cheapest improvement — measured, proposed, not committed

Two lines, `READ_BUFFER_SIZE` untouched so accumulation and back-pressure are unchanged:

```rust
/// Most bytes handed to one `pump.advance`, i.e. to one lock hold.
pub(crate) const MAX_LOCKED_READ: usize = 0x1_0000; // matches oneterm-pty's pipe CHUNK

let read_end = unprocessed.saturating_add(MAX_LOCKED_READ).min(READ_BUFFER_SIZE);
match self.pty.reader().read(&mut buf[unprocessed..read_end]) {
```

Loopback, 5 s:

| arm | profile | bytes/hold p50 | throughput | frames | wait p50 | MAX |
|---|---|---|---|---|---|---|
| shipped | `fast-dev` | 659 KB | 41.4 MiB/s | 200 | 9.6 ms | 22.6 ms |
| **capped read** | `fast-dev` | 64 KB | **54.2 MiB/s** | **298** | **0.53 ms** | **2.25 ms** |
| shipped | `test` | 634 KB | 5.1 MiB/s | 44 | 99.5 ms | 161 ms |
| **capped read** | `test` | 64 KB | 5.5 MiB/s | 191 | **6.3 ms** | 132 ms |

**~10× better worst-case wait and throughput slightly up**, suite green
(`31 passed; 0 failed`), clippy clean, process exits cleanly. The residual 132 ms tail at
opt-level 0 comes from the contended path (`None => continue` piles up to `READ_BUFFER_SIZE`
before one feed); closing that needs a feed-side cap too.

Two cheaper-looking variants **measured and rejected**:

- `READ_BUFFER_SIZE = 64 KiB` — similar latency, but left a test binary spinning at 100 %
  CPU that never exited (690 s CPU accumulated before I killed it).
- capped read **plus** a feed-side cap with `copy_within` — hung the 5 s measurement on
  both profiles (200 s timeout, twice).

So the recommendation is the capped **read** only. **Propose to `US-0084` or a follow-up
packet; do not hold `US-0083` for it**, because the local shell's real number is 59 µs.

---

## 4. The `break` stall

### 4.1 Reproduced

Scratch edit — `break;` after `self.pump.write_color_replies(replies);` — reverted:

```
test session::session_tests::e2e_echo_output_rendered_in_snapshot ... FAILED
  `echo oneterm_e2e` did not appear in the snapshot after 6s
test session::session_tests::mouse_drag_updates_selection_not_mouse_move ... FAILED
test session::session_tests::selection_text_and_clear ... FAILED
test result: FAILED. 0 passed; 3 failed; finished in 10.08s
```

Exactly the three tests the packet names. The finding is real and correctly recorded.

### 4.2 The shipped loop cannot stall

Every exit from the inner read loop, and why the ring is either drained or re-armed:

| Exit | Condition | Why it is safe |
|---|---|---|
| `Ok(0) if unprocessed == 0` (`:395`) | ring empty | `PipeReader::read` sets `caller_waiting = true` in the **same** lock acquisition that returned 0 (`pipe.rs:223-226`). Armed. |
| `Err(Interrupted\|WouldBlock)` with `unprocessed == 0` | — | Unreachable on Windows: `PipeReader::read` never returns `Err`. On Unix `polling` is genuinely level-triggered, so the loop re-polls. |
| `Err(other)` (`:404`) | read error | Same: unreachable on Windows; level-triggered on Unix. |
| the yield (`:442-458`) | demand raised | **Does not exit.** It drops the guard and keeps reading; `terminal` is `None` on the next pass and re-locks. |
| `None => continue` (`:415`) | engine held | Does not exit; keeps draining into `buf`, then blocks in `lock_unfair` at `READ_BUFFER_SIZE`. |

Race check: a `push` landing between the arming read and `poll.wait` takes the flag and
posts the packet (`pipe.rs:210-213`), so the wait returns immediately; the worst case is
one spurious wake, which converges on the next `Ok(0)`.

Measured corroboration: with the UI queue drained, `fed bytes` equals `ring bytes`
**exactly** in both workloads (335169/335169 then 335258/335258; 3554886/3554886), and
ring high-water stayed at 170 B / 9.6 KB. Nothing is ever left behind.

> One run *did* show a 219 KB ring backlog — that was my own harness not draining the UI
> event queue, parking the pump in `finish_batch_blocking`. Exactly the hazard the
> packet's *Context* section warns about, independently confirmed.

### 4.3 Judgement on the ring's wake semantics

**This is a latent bug in `oneterm-pty` (US-0071), not a caller-side rule.**
`PipeReader` is registered `PollMode::Level` (`event_loop.rs:322`) and `Ring::wake`
explicitly honours Level by keeping the interest registered (`pipe.rs:110-113`) — but
delivery is edge-triggered-once: `push` posts only when `caller_waiting` is set, and
`caller_waiting` is set only by a read that finds the ring **empty**. A caller that obeys
the documented `PollMode::Level` contract — read once per readiness, return to the poller
— deadlocks, with no diagnostic. The obligation ("drain to empty before returning to
`poll.wait`") is invisible in the type, undocumented on `PipeReader`, and the packet's
comment sits in the wrong crate to protect the next caller.

Proposed fix shape (three lines, `PipeReader::read`, `pipe.rs:219-235`) — make Level mean
Level by re-posting when the ring is left non-empty:

```rust
let left = bytes.queue.len();           // after the drain
drop(bytes);
if was_full { self.ring.changed.notify_all(); }
if left > 0 { self.ring.wake(); }       // still readable → re-arm
```

The caller's "drain to empty" then becomes an optimisation instead of a correctness
requirement, and `US-0084`/any future consumer cannot repeat the stall. **Worth its own
packet against `US-0071`.**

---

## 5. Behaviour

Covered by tests already on the branch: child exit → `alive=false` + exit code
(`child_exit_with_status_records_code_and_closes`,
`loop_child_exit_ends_the_session_and_stops_the_thread`), idle resize
(`loop_applies_latest_resize_to_the_pty`), input FIFO
(`loop_writes_queued_input_to_the_pty_in_order`), one colour query
(`loop_answers_color_queries_through_the_pty`), shutdown.

Gaps I wrote scratch tests for (all reverted):

**OSC colour queries answered in order (R-37) — PASS.** Interleaved
`11;? 10;? 11;?` after setting both colours:

```
FIRST TWO REPLIES: "\u{1b}]11;rgb:1111/2222/3333\u{7}\u{1b}]10;rgb:aaaa/bbbb/cccc\u{7}\u{1b}]"
```

`11`, `10`, `11` — the arrival order, 72 bytes, all three present, written after the
guard is dropped. (My assertion then over-read by 3 bytes; the property is confirmed.)

**Ctrl-C under a real ConPTY flood — PASS.** `type` a 9.6 MB file, then `\x03`:

```
Ctrl-C under flood: output stopped after Some(100.9894ms) (ring grew 603 bytes after ctrl-c)
```

**Resize during a sustained flood — FAIL on a fast transport.** On the loopback fixture
the resize never reached the PTY in **12 s**. Root cause: `pending_resize`, the input
queue and the shutdown flag are read only at the top of the **outer** loop
(`event_loop.rs:~296-345`); the inner read loop runs until the transport is dry. On real
ConPTY the pipe runs dry ~35 000–100 000 times in 5 s (see the `advances` counts in §3.2),
so this never bites — which is why Ctrl-C above landed in 101 ms. On a socket it will.
Same root cause and same fix family as §3.5. See MINOR‑4.

**`chcp` / UTF-8 across chunk boundaries — NOT ESTABLISHED.** My probe rendered mojibake
(`h├⌐lloΓåÆ Σ╕û τòî`) on the loopback fixture **and** on a real `cmd.exe` session — but the
control (the identical text in a single write) failed identically, so the probe cannot
distinguish "split across chunks" from anything else and is not evidence of a defect.
Against it: `pump.advance` feeds straight into `term.feed`, and the engine has a dedicated
passing test for exactly this property
(`crates/vt/src/parser/parser_tests.rs:747 utf8_partial_codepoint_survives_the_chunk_boundary`),
inside a workspace suite of 1919 green tests. I therefore claim **no finding** — but the
probe result is unexplained and someone who owns the adapter's snapshot path should spend
ten minutes on it. It is outside this packet's diff either way.

---

## 6. The throughput trade

My own 5 s loopback runs, `--profile fast-dev`, throughput **byte-counted at the feed**
rather than inferred from the line count:

| arm | throughput | frames | frame wait p50 | MAX |
|---|---|---|---|---|
| yield wired | 41.4 / 51.3 / 54.2 MiB/s | 200 / 234 / 298 | 5.4–9.6 ms | 18.6–22.6 ms |
| yield disabled | 51.1 MiB/s | **39** | **51 ms** | **805 ms** |

Confirmed: ~6–7× the frames. On the cost side my numbers are **better than the packet's** —
the two arms sit inside each other's spread, so the honest reading is "no measurable
throughput cost", which is also the reading the packet itself lands on after its caveat.
The packet's "roughly a third of peak flood throughput" is pessimistic and I would soften
it. Its absolute MiB/s figures are corroborated by my independent byte counter.

**Is the feed ever throttled below what ConPTY delivers?** **No.** Across every real-shell
run: ring high-water **170 B** (`yes`) and **1517–9644 B** (`type`) against a **1 MiB**
`PIPE_CAPACITY`, and `push` blocked on a full ring **0 times**. The child is never
back-pressured by the ring; conhost is the limiter.

---

## 7. Gaps and the GUI walk

Packet gaps 1 (`ResizePolicy`) and 2 (the `alacritty_terminal` manifest line) both name
`US-0085` as the owner and both are proved with quoted compiler errors — confirmed, and
`grep -rn alacritty crates/local-shell/src` returns exactly the one hit at
`session_tests.rs:5` the packet predicts.

**GUI walk: not performed.** `quser` → `trunglt  1  Disc  idle 11:43`. No Active desktop,
so the app was not launched, matching the packet's own gap 3 and its unticked E2E box.

---

## 8. Code quality

- `grep -n "unwrap()\|expect(\|panic!\|unreachable!"` over `event_loop.rs`, `session.rs`,
  `transport.rs`, `session_terminal.rs` → **no hits**. The only recovery forms are
  `unwrap_or_else(PoisonError::into_inner)`, which is the codebase's poison policy.
- No dead code: the `Engine::exit()` call site is gone as claimed.
- `docs/terminal-backend.md` §5.1 and §6.2 both updated, and both correctly state the
  no-`break` constraint and which fixture cannot see it. Accurate.
- Harness DB row `US-0083`: present, `status=implemented`, `unit/integration/platform=1`,
  `e2e_proof=0` (consistent with the unticked box), `last_verified_result=pass`.

---

## Findings

### MAJOR‑1 — the headline latency number is a fixture artifact presented as a loop property
`docs/spec-intakes/IN-0029-vt-engine/US-0083-local-shell-native.md`, "The handshake,
measured". The table records **86.2 / 107.9 / 125.3 ms** worst frame wait. On the
transport this crate ships on, the same loop measures **52–59 µs** (§3.2) — three orders
of magnitude apart. The packet does note the units differ, but it never says the shipped
Windows path is that much faster, and the Handoff section sends `US-0084` to this table.
**Fix:** add the real-ConPTY column and state the attribution — worst frame wait =
bytes-per-lock-hold × parse rate; ConPTY delivers 82 B/read, a socket 600 KB/read.
*This is the one change I would require before merge.*

### MAJOR‑2 — the lock hold is unbounded in bytes
`crates/local-shell/src/event_loop.rs:392` (read into a 1 MiB `buf`) with `:432` (one
`advance` per read) and `:442` (demand checked only after it). Harmless on ConPTY, 99.5 ms
p50 on a socket. **Fix:** the two-line capped read in §3.5 — measured 10× better worst
wait, throughput up, back-pressure unchanged, suite green. Hand to `US-0084` or a
follow-up packet.

### MAJOR‑3 — `PipeReader` claims `PollMode::Level` and delivers edge-once
`crates/pty/src/windows/pipe.rs:219-227` (arming) with `:205-213` (the conditional wake)
and `:110-113` (Level keeping the interest). The caller obligation is real, unenforced and
undocumented on the type; the packet's comment protects only this one caller. **Deserves
its own packet against `US-0071`**, fix shape in §4.3.

### MINOR‑4 — control messages starve under a sustained flood
`crates/local-shell/src/event_loop.rs:~296-345`: resize, queued input and the shutdown
flag are read only at the top of the outer loop, and the inner read loop runs until the
transport is dry. Unreachable on ConPTY (measured: Ctrl-C lands in 101 ms), reachable on
any fast transport (measured: resize lost for 12 s on the loopback fixture). Same fix
family as MAJOR‑2; `US-0084` should test for it.

### MINOR‑5 — wrong gap number in a source comment
`crates/local-shell/src/session_terminal.rs:14` — "see the `US-0083` packet's gap 2" for
the `ResizePolicy` problem, which is **gap 1** in the packet (gap 2 is the manifest line).

### MINOR‑6 — unbacked-off spin on the contended path
`crates/local-shell/src/event_loop.rs:~415`, `None => continue`: with `unprocessed > 0`
and the engine held, the loop re-reads and re-tries `try_lock` with no pause. Bounded
(it stops at `READ_BUFFER_SIZE`) and cold in practice (0–160 misses per 5 s run), so it is
a note, not a defect — `std::hint::spin_loop()` or a `ponytail:` comment naming the ceiling.

### OBSERVATION‑7 — unexplained UTF-8 probe result
See §5. No defect claimed; the engine's own test covers the property. Worth ten minutes
from whoever owns the adapter snapshot path.

---

## Reproduction notes

All measurements ran in this worktree's own `target/` (`cargo metadata` confirmed
`…\agent-a20da8012abddb9c7\target`; `CARGO_TARGET_DIR` unset). Scratch instrumentation and
every experimental edit were reverted with `git checkout -- crates/`; the tree is clean.
