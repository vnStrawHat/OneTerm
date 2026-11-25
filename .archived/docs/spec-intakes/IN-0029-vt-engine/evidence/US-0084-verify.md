# Independent verification: `US-0084` — `crates/ssh` goes native

Intake: IN-0029
Packet: [`../US-0084-ssh-native.md`](../US-0084-ssh-native.md)
Branch: `worktree-agent-a34259a83971f85d8`, four commits off `feat/vt-engine` @ `d3c537b`
Worktree: `.claude/worktrees/agent-a34259a83971f85d8` (built into its own `target/`; `D:` free 52.06 GB)
Date: 2026-09-13
Verdict: **merge after fixes** — three documentation corrections, no code change.

The app was never launched (engine/crate-level packet), no process was enumerated or
signalled, and every experiment below was reverted: both trees end `git status --porcelain`
empty.

---

## 1. Pass / fail table

| # | Claim | Verdict | Evidence |
| --- | --- | --- | --- |
| 1 | Scope: only `crates/ssh` + the two doc files | **PASS** | §2.1 |
| 1b | Commit trailer exact on all four commits | **PASS** | §2.1 |
| 2 | `pwsh scripts/ci-local.ps1` green, 1919 passed | **PASS** | §2.2 |
| 2b | `cargo test -p oneterm-ssh` 67 passed | **PASS** | §2.2 |
| 2c | The handshake test is not flaky (5 runs) | **PASS** | §2.3 |
| 2d | Its negative control fails as the packet claims | **PASS** | §2.3 |
| 3 | Event order: replies → events outside the lock → repaint hint → demand check | **PASS** | §2.4 |
| 3b | The lock is not held across the `await` | **PASS** | §2.4 |
| 3c | A resize during a flood is applied | **PASS** (scratch test) | §2.5 |
| 4 | `BottomAnchor` behaviour, asserted not named | **PASS** (mutation-checked) | §2.6 |
| 5 | Gap 1 — the manifest line cannot go | **PASS**, reproduced | §2.7 |
| 5b | Gap 2 — `resize_policy()`'s return type | **PASS**, confirmed | §2.7 |
| 5c | Both assigned to `US-0085` in the packet | **PASS** | §2.7 |
| 6 | No `unwrap`/`expect` on new runtime paths, no dead code, comments accurate | **PASS** | §2.8 |
| 6b | The two `docs/terminal-backend.md` edits are correct | **PARTIAL** | F1, §2.9 |
| 7 | Packet completeness and the `US-0084` DB row | **PASS** | §2.10 |
| — | Owning docs reconciled | **FAIL** | F2, F3 |

---

## 2. Raw evidence

### 2.1 Scope and trailers

```
$ git diff d3c537b...HEAD --stat
 crates/ssh/Cargo.toml                              |   4 +
 crates/ssh/src/session.rs                          |  32 ++-
 crates/ssh/src/session_terminal.rs                 |   7 +-
 crates/ssh/src/task.rs                             |  39 ++-
 crates/ssh/src/task_tests.rs                       | 173 ++++++++++++
 crates/ssh/src/transport.rs                        |   6 +-
 .../IN-0029-vt-engine/US-0084-ssh-native.md        | 305 +++++++++++++++++++++
 docs/terminal-backend.md                           |  16 +-
 8 files changed, 558 insertions(+), 24 deletions(-)
```

Nothing outside `crates/ssh` + the packet + `docs/terminal-backend.md`. `crates/terminal`,
`crates/local-shell` and `crates/terminal-view` are untouched, as N-04 requires
(`migration.md:116`). `crates/ssh/src/session.rs`'s hunk starts at line 803, inside
`mod tests` (which opens at `:698`), so the connect / auth / host-key / forwarding path of
`docs/ssh-client-connect.md` is not touched — `transport.rs`'s two hunks are doc comments
only.

All four commits carry the identical two-line trailer:

```
$ for c in $(git log d3c537b..HEAD --format=%h); do git log -1 --format=%B $c | tail -4; done
--- 96e24d8 / 831c576 / f1c97d5 / e41e9f9   (all four identical)
Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01C1Vip1PKvU8ayf3PZ4jPM9
```

### 2.2 `ci-local.ps1` and the crate suite

```
$ pwsh scripts/ci-local.ps1
==> cargo fmt --all -- --check
==> cargo clippy --workspace --all-targets -- -D warnings
==> cargo test --workspace
==> cargo test -p oneterm-vt --features vt-paranoid
==> python scripts/verify-dependency-graph.py
==> python scripts/check-doc-paths.py
==> python -m unittest scripts/test_check_english.py
==> python scripts/check-english.py
==> python scripts/completion-catalog.py validate
==> python scripts/third-party-notices.py --check
ci-local: all checks passed.
CI-EXIT=0
```

Totals tallied from the log by script, not by eye:

```
ALL:                 62 sections, 1919 passed, 0 failed, 13 ignored
before vt-paranoid:  58 sections, 1550 passed, 0 failed, 10 ignored
after  vt-paranoid:   4 sections,  369 passed, 0 failed,  3 ignored
```

**Exactly the 1919 / 62 / 58-1550-0-10 / 4-369-0-3 the packet recorded**, and exactly +1 on
`US-0082`'s 1918.

```
$ cargo test -p oneterm-ssh
test result: ok. 67 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.73s
test task::task_tests::the_task_yields_the_engine_to_a_waiting_frame ... ok
test session::tests::ssh_grow_resize_pulls_scrollback_into_the_viewport_top ... ok
```

The diff adds exactly one test function and renames one, so 66 → 67 is arithmetically
consistent with the base.

### 2.3 The handshake test: flakiness and negative control

Five consecutive runs, all green, all ~0.1 s:

```
run 1: ... ok | test result: ok. 1 passed; 0 failed; ... finished in 0.10s
run 2: ... ok | ... 0.09s
run 3: ... ok | ... 0.10s
run 4: ... ok | ... 0.10s
run 5: ... ok | ... 0.10s
```

Negative control — `if term.take_render_demand() { yield_now().await }` commented out in
`crates/ssh/src/task.rs`, run, then `git checkout --`:

```
thread '...the_task_yields_the_engine_to_a_waiting_frame' panicked at crates\ssh\src\task_tests.rs:172:5:
the loop never took the render demand
test result: FAILED. 0 passed; 1 failed; ... finished in 15.11s
```

The packet's claim is reproduced to the second (15.11 s vs "15.1 s") and to the message.
It also reproduces the packet's honest caveat in gap 3: the `waited < 2 s` assertion
**passed** in the failing run, so that half of the test is not discriminating — the `taken`
assertion is the one carrying the proof. The packet says so itself.

### 2.4 Event order and the drain

Traced end to end; every hop confirmed in source, not inferred.

| Step | Where | What it does |
| --- | --- | --- |
| bytes in | `crates/ssh/src/task.rs:74-78` | `state.add_rx_bytes`, then `pump.process_chunk(&term, bytes)` |
| lock taken | `crates/terminal/src/backend/pump.rs:160-168` | `let mut guard = term.lock();` inside a block expression |
| feed + drain | `pump.rs:111-118` (`advance`) | `term.feed(bytes, &mut self.batch, …)` then `self.router.drain(&self.batch, &mut self.lock_pending())` |
| **replies first** | `crates/terminal/src/backend/osc_router.rs:138-149` | one pass emitting only `VtEvent::Reply`, then a second pass for everything else — R-37 |
| lock dropped | `pump.rs:161-170` | the guard's scope ends before `write_color_replies` |
| events out | `task.rs:79` → `pump.rs:191-197` | `finish_batch(true).await`: `publish_line_count()`, `flush().await` (the batch's events), **then** `post_repaint()` |
| demand check | `task.rs:87-89` | `if term.take_render_demand() { tokio::task::yield_now().await; }` |

**The lock is not held across the await.** `process_chunk` is `fn`, not `async fn`, and its
guard dies with the block that produced `replies`; `finish_batch` and `take_render_demand`
take no engine lock (`crates/terminal/src/handle.rs:115-118`). There is no `await` inside a
guard's scope anywhere in `ssh_main_task`.

The order matches the contract verbatim: `migration.md:223-225` — "call it at a chunk
boundary, **after the batch's replies have left** (R-37), and drop the guard when it answers
`true`". SSH has no guard to drop (it locks per chunk), so it yields instead — a strictly
stronger answer, and the packet argues for it in the code comment at `task.rs:80-86`.

The "no deferred sink left" claim checks out: `SessionEventSink` appears in `crates/ssh` only
as a constructor call at `session.rs:231` and `:784` (a test helper) — no `flush_reliable`,
no `deferred`, anywhere in the crate.

### 2.5 A resize during a flood (scratch test, written by the verifier, reverted)

The packet's gap 4 records that no walk could show a resize. I closed that at crate level: a
second `#[tokio::test]` appended to `task_tests.rs`, reusing its loopback harness with a
server that records `window_change_request`, raising `pty_resize(30, 100)` from another task
mid-flood after 256 KiB:

```
SCRATCH: window_change latency under flood = 47.3258ms
test task::task_tests::a_resize_during_a_flood_reaches_the_remote_pty ... ok
```

47 ms, of which 5 ms is the poll granularity. The data arm cannot starve the resize because
`transport.take_pending_resize()` is checked at the top of every loop iteration
(`task.rs:60-66`), before the `select!`. File reverted; tree clean.

### 2.6 `BottomAnchor` through the engine

`reflow-and-resize.md:154` defines it: *"pull `min(history, added)` rows out of scrollback
into the top; the cursor moves **down** by that amount"*. The test feeds 40 lines into a
24-row grid (≥16 rows of history), grows 24 → 30 (6 added), and asserts `cursor_line + 6`
and `total_lines` unchanged.

Both assertions are correct against the definitions:
`total_lines = screen.history_len() + screen.rows()` (`crates/terminal/src/model.rs:131`), so
moving 6 rows from history into the viewport leaves it constant while appending 6 blank rows
would raise it by 6; `cursor_line = row_to_line(cursor.pos.row, screen.screen_top())`
(`model.rs:125`) is viewport-relative, so the cursor's *absolute* row staying put while the
viewport top rises 6 is exactly `+6`.

It also genuinely pins the policy, which is the whole point of the rewrite. **Mutation check**
— `session_terminal.rs` flipped to `ResizePolicy::KeepViewportTop`, run, reverted:

```
panicked at crates\ssh\src\session.rs:828:9:
assertion `left == right` failed: the cursor did not follow the rows pulled out of history
  left: 23
 right: 29
```

And the token really does reach the engine as `BottomAnchor`:
`crates/terminal/src/model.rs:60-64` — `ResizePolicy::Default => oneterm_vt::ResizePolicy::BottomAnchor`,
fed to `term.resize(Size{..}, self.resize_policy)` at `model.rs:236`. The old test asserted
the adapter enum's *name*; the new one asserts the engine's *behaviour*. Strictly better.

### 2.7 Gaps 1 and 2 — both reproduced

**Gap 1**, manifest line removed, `cargo check -p oneterm-ssh --all-targets`:

```
error[E0433]: cannot find `alacritty_terminal` in the crate root
  --> crates\ssh\src\session_terminal.rs:14:1
   | |_^ could not find `alacritty_terminal` in the list of imported crates
error: could not compile `oneterm-ssh` (lib) due to 5 previous errors
error: could not compile `oneterm-ssh` (lib test) due to 5 previous errors
```

E0433, **5 errors**, exactly as measured, and the span is the
`impl_pty_terminal_session!` invocation — the macro, not an import. The source is
`crates/terminal/src/session.rs:515-518` (four `::alacritty_terminal::vte::ansi::Rgb`
parameters of `set_default_colors`) and `:611`
(`::alacritty_terminal::selection::SelectionType` in `mouse_down`). Those are the **trait's**
signatures, so no hand-written impl escapes them. `grep -rn alacritty_terminal crates/ssh`
gives two hits, both `Cargo.toml` (`:21` the comment, `:24` the line) — no source file names
the fork. (Cargo.lock was touched by the experiment and restored.)

**Gap 2** confirmed structurally, which is conclusive here:
`crates/terminal/src/session.rs:474` generates `pub(crate) fn resize_policy(&self) -> $crate::model::ResizePolicy`,
and `:470` passes `self.resize_policy()` into `TerminalModel::new`. So although `new` takes
`impl Into<oneterm_vt::ResizePolicy>` (`model.rs:87`), the `$resize_policy` token is
type-checked against the *adapter* enum first. A backend cannot name the engine value.

Both gaps name the API wanted and the owner. The packet assigns gap 1 to `US-0085`
explicitly and gap 2 to "the next packet that may edit `crates/terminal` — `US-0085`". **It
says so.**

### 2.8 Code quality

- `grep -n "unwrap()\|expect(" crates/ssh/src/{task,transport,session_terminal}.rs` — the only
  hit is `session_terminal.rs:45`, which is **pre-existing** (identical at `d3c537b`) and not
  on a path this packet changed. See F5.
- No `unwrap`/`expect` added to any runtime path; the new ones are all in `task_tests.rs`.
- No dead code, no new `#[allow]` (the one at `task.rs:36` is pre-existing).
- `grep -rni "EventListener|processor.advance|alacritty" crates/ssh/src/*.rs` — **empty**. The
  fork's callback names are gone from the crate's prose; `transport.rs:16` and `:60-61`
  now say "the shared batch drain" and "Arc-shared between the router, the session and the
  tokio task" instead of "Alacritty `EventListener`" / "`Term`".
- The comment at `task.rs:80-86` is accurate: SSH replies really do go out via
  `Cmd::Write` on the channel this same `select!` drains (`transport.rs:156-177`,
  `task.rs:115-122`), so "the replies have left" means "queued", and the comment says that.

### 2.9 The `docs/terminal-backend.md` edits

§ 5.1 (`:160-166`) is correct and better than what it replaced: it names which loop has the
call, why SSH's answer is a yield rather than a guard drop, and correctly leaves
`lock_unfair` / `try_lock_unfair` hanging on `US-0083` — verified, `crates/local-shell/src/event_loop.rs:395`,
`:397`, `:428` are now the only non-test callers.

§ 7 (`:562-567`) is correct on the mechanics but overstates on its last clause. See **F1**.

§ 5.3's `ResizePolicy` prose (`:247-252`, "`ResizePolicy::BottomAnchor` (this crate's
`ResizePolicy::Default`)") is already right, so the packet was right not to touch it.

### 2.10 Packet and DB row

Packet: Outcome / Scope / Acceptance / Documentation / Verification Plan all present and
filled before the code commit (`e41e9f9` precedes `f1c97d5`). Status `Implemented`; proof
block `[x]` unit, integration, platform, verify, `[ ]` E2E — honest, and gap 4 explains the
E2E blank with a concrete reason (`quser` shows the only session `Disc`;
`sftp-dev-server.rs:102-110` answers a shell request with a one-line banner). Every rewritten
test is named with its reason, as acceptance requires. The one acceptance box marked `[~]`
is exactly the one that could not be met, and it is the one the gap explains.

`harness.db` `story` row `US-0084` (read from a copy, the DB was not written): status
`implemented`, unit/integration/platform proof `1`, e2e `0`, `verify_command`
`pwsh scripts/ci-local.ps1`, `last_verified_result` `pass`, `contract_doc` and `packet_doc`
correct, `intake_id` 34. Its evidence text matches the packet, including the two gaps and the
+1 test count.

---

## 3. Findings, ranked

### F1 — medium — `docs/terminal-backend.md:565-566` contradicts the packet's own gap 1

```
  waiting (§5.1). Nothing in `crates/ssh` names the engine or the fork: the shared pump is
  the whole of the terminal side.
```

`crates/ssh/Cargo.toml:24` names the fork, and must, for the reason gap 1 measured. A reader
of § 7 would conclude the manifest line is gone. The packet's own wording elsewhere is the
correct one ("No `crates/ssh` **source file** names the fork").

*Fix:* "No `crates/ssh` source file names the engine or the fork; the `alacritty_terminal`
manifest line survives only because `impl_pty_terminal_session!` expands the name into this
crate (`US-0085`)."

### F2 — medium — `migration.md` keeps three statements this packet measured false, and the packet declines to edit it

The packet's Reconciliation defers them to "the LLD's owner". But `migration.md` is a
document, not `crates/terminal`; nothing in N-04 forbids editing it, and `US-0083` and
`US-0085` will read these lines next:

- `low-level-design/migration.md:116` — the `US-0084` may-touch row still says "the
  `alacritty_terminal` manifest line is deleted". It cannot be, and this packet proved it.
- `:233-235` — "The backends pass `BottomAnchor` or `KeepViewportTop` through the macro
  argument they already pass — **one token each**." Gap 2 shows one token is not enough; the
  macro's return type has to change first.
- `:230-231` — "`take_render_demand` has no non-test caller yet — `US-0083` and `US-0084`
  own calling it". Half of that is now done.

*Fix:* three short edits to `migration.md` in this packet (they are its own measurements), or
an explicit coordinator task before `US-0083` starts — `US-0083` will otherwise budget "one
token" for a change that needs `crates/terminal` first.

### F3 — low — the packet cites a `migration.md` deletion row that does not exist

`US-0084-ssh-native.md`, Documentation → Owning Docs Reviewed: "the deletion list (the
manifest-line row naming `crates/ssh` at `US-0084`)", and gap 1: "The migration LLD's deletion
row … calling the line 'already dead'". The deletion list is `migration.md:302-316` and has
**no** `crates/ssh` manifest row. The claim the packet is rebutting lives in the may-touch
table at `:116` and in `US-0081`'s harness note ("it is dead TODAY … `US-0084` can take it").

*Fix:* cite `migration.md:116` and the `US-0081` DB note; the rebuttal itself is right and
well measured.

### F4 — low — one off-by-one citation

Packet, "The batch drain needed no change": "every `SessionEventSink` use in `crates/ssh`
(`session.rs:232`, the constructor…)". The constructor call is `crates/ssh/src/session.rs:231`.

### F5 — low — pre-existing, out of scope, worth an owner

`crates/ssh/src/session_terminal.rs:45` — `self.sftp.lock().unwrap()` inside
`TerminalSession::capabilities()`, a runtime path called per frame-ish. Identical at
`d3c537b`, so **not this packet's**, but `OscRouter` next door already uses
`unwrap_or_else(PoisonError::into_inner)` (`osc_router.rs:126-129`), which is what
`error-policy.md` wants. Worth a line in `US-0086`.

### F6 — low — `crates/terminal/src/handle.rs:132-135` goes stale on merge

The comment above `lock_unfair` / `try_lock_unfair` still lists "`crates/local-shell/src/event_loop.rs`
**and `crates/ssh/src/task.rs`**, which `US-0083` / `US-0084` rewrite". After this packet,
`crates/ssh` is not a call site. The packet may not edit `crates/terminal`, so this is
correctly `US-0083`'s to fix — noting it so it is not lost.

### F7 — informational, no fix — the test's timing bound is decoration

Confirmed by the negative control: the `waited < 2 s` assertion passes with the feature
removed, so only `assert!(taken)` proves anything. The packet states this plainly in gap 3
and does not claim otherwise, which is the right call — but a future reader should not take
the 2 s bound as a starvation guard. Gap 3's framing ("the yield is insurance, not a fix, in
the SSH shape") is accurate and I could not fault it: SSH locks per chunk
(`pump.rs:160-170`), which is not the shape `US-0082` measured at 354 ms.

---

## 4. Verdict

**Merge after fixes.**

The code is right and the evidence behind it is real. Every substantive claim I could test, I
tested, and all of them held: the totals to the digit (1919 / 62 sections), the negative
control to the message and the second, the E0433 to the error count, and the resize policy to
a mutation check the packet did not run. The one `if` that this packet adds is in the right
place, in the right order, with the lock provably released. The two things it could not do are
blocked in a crate N-04 forbids it to touch, both are measured with a build rather than
asserted, and both name the API and the owner.

Nothing found is a code defect. The three fixes are documentation: **F1** (one sentence in
`docs/terminal-backend.md` that contradicts the packet's own gap), **F2** (three stale
statements in the owning LLD that `US-0083` is about to build on), and **F3** (a citation to a
row that does not exist). F1 and F3 are one-line edits; F2 is the one that matters, because
`US-0083` inherits both walls and will otherwise plan for "one token".

---

*Verifier notes: the app was not launched and no process was enumerated or signalled. Four
experiments were run and all four reverted (`task.rs` demand call, `Cargo.toml` manifest line
+ `Cargo.lock`, `session_terminal.rs` policy token, a scratch test appended to
`task_tests.rs`); the worktree and the main checkout both end with an empty
`git status --porcelain`. Builds went only into this worktree's own `target/`. This report is
written, not committed.*
