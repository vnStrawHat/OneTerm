# US-0090 — independent verification

Verifier: a different agent from the implementer. Worktree
`D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-a3f9ede906d75ba18`, hard-reset to
`18d7b4c`. Diff under review `4e83f31..18d7b4c` (58 files). Not committed, not pushed.

## Verdict

**PASS-WITH-NOTES.**

Every hard gate the packet claims reproduces exactly on an independent run: the five deletions are
genuinely zero-user, `Demand` moved without a semantic change, `crates/vt` holds no atomic and no
other interior mutability, both public-surface proxy counts land on the numbers claimed, exactly one
test moved and none was dropped, and `pwsh scripts/ci-local.ps1` exits 0 at **60 / 1934 / 0 / 14**.

The defects are all in the half of the packet that is *about* documentation accuracy. Three live
prose citations of methods this packet deleted survive in two owning docs, in one case contradicting
the packet's own Reconciliation table, which states those exact lines were fixed. That matters more
than usual here: re-pointing stale prose is the packet's stated outcome, not a side effect of it.

One finding runs the other way — the single acceptance clause the packet marks **NOT MET** appears
to be **met** (defect 9); the packet under-credited itself.

---

## Defects

### D-1 — `docs/terminal-backend.md:453` still describes two deleted methods. **Medium**

`docs/terminal-backend.md:449-453`, in § 6.2 "Spawn via `oneterm-pty`" → "Current implementation":

```
It reads with a heap-allocated 1 MiB buffer into `TerminalPump::advance` under a
`try_lock_unfair` guard (falling back to `lock_unfair` only when the buffer is
full), answers colour queries with the same guard, then calls
`finish_batch_blocking`.
```

`TerminalHandle::try_lock_unfair` and `lock_unfair` do not exist after this packet. The code the
sentence describes now reads `self.term.try_lock()` / `self.term.lock()`
(`crates/local-shell/src/event_loop.rs:436-437`).

Repro:

```text
$ grep -n 'lock_unfair' docs/terminal-backend.md
453:`try_lock_unfair` guard (falling back to `lock_unfair` only when the buffer is

$ grep -rn 'lock_unfair' crates/
(no output)
```

This is a false claim, not only an omission. The packet's Reconciliation table
(`US-0090-…md:619`) states for `docs/terminal-backend.md`: *"the `lock_unfair` / `try_lock_unfair`
sentence deleted"*. `git diff 4e83f31..18d7b4c -- docs/terminal-backend.md` touches only the header
block (`:15-24`), § 5.1 (`:144-172`) and the file-layout tree (`:866-870`). It never reaches `:453`.

### D-2 — `docs/terminal-backend.md:457` and `:615` still cite `take_render_demand()`. **Medium**

```text
$ grep -n 'take_render_demand' docs/terminal-backend.md
170:  on). (It was called `take_render_demand()` until `US-0090`; the verb was residue from   <- correct, historical
457:`take_render_demand()` and, when a frame is waiting, answers that batch's colour           <- live prose, § 6.2
615:  the loop asks `SharedTerminal::take_render_demand()` and yields the task when a frame is <- live prose, § 7
```

Both `:457` and `:615` describe what the two pump loops call *today*; both now call
`render_demand_raised()` (`crates/local-shell/src/event_loop.rs:475`, `crates/ssh/src/task.rs:87`).
Line 170 is the one the packet actually wrote, and it is correct.

Same false claim as D-1: Reconciliation says *"§5.2's and §6's `take_render_demand()` citations"*
were fixed. They were not — only § 5.1's were.

This is precisely the `R`/`D` residue class the packet exists to remove, re-created in the same
commit by a partial pass over the file the packet lists as **Must change**.

### D-3 — the sibling IN-0029 LLD was not reviewed and now states a deleted contract. **Medium**

`docs/spec-intakes/IN-0029-vt-engine/low-level-design/migration.md:233-249`, § "The adapter
contract, as `US-0082` shipped it", introduced by *"these four shapes are what `US-0083`, `US-0084`
and `US-0085` build against"*:

```
:236  Arc<TerminalHandle>` wraps `FairMutex<Engine>` plus one `Demand`
:244  - `take_render_demand() -> bool` is the **pump's** yield check: …
:247    demand survives more than one ask … `render_demand_raised()` is the same read, for diagnostics.
:249  - `lock()`, `lock_unfair()` and `try_lock_unfair()` still compile at today's call sites.
```

Four separate statements, each false after this packet: the `Engine` newtype is deleted
(`TerminalHandle.engine` is `FairMutex<Terminal>`, `handle.rs:86`); `take_render_demand` is deleted;
`render_demand_raised` is the *only* one left, not "the same read, for diagnostics"; and
`lock_unfair` / `try_lock_unfair` no longer compile anywhere.

The packet's Documentation Action reviews `damage-and-render-state.md` (correctly updated — `:222`
now reads `// crates/terminal/src/handle.rs (crates/vt/src/render/demand.rs until US-0090)` and
`:231` explains the rename) and `events-and-api.md` (an accurate "As shipped (`US-0090`)" note
added). `migration.md` is the third LLD in the same directory, it owns the same surface, and the
packet never names it. `docs/HARNESS.md`'s rule is to locate and review the owning docs; this one
was missed.

Repro: `grep -rn 'lock_unfair\|take_render_demand\|FairMutex<Engine>' docs/spec-intakes/IN-0029-vt-engine/low-level-design/`.
(Hits in `docs/spec-intakes/IN-0029-vt-engine/US-00*.md` and `.../evidence/` are shipped packet
records and correctly left alone; `research/` likewise. Only `low-level-design/` is living
contract.)

### D-4 — two undeclared deletions, with no zero-user grep recorded. **Low**

`crates/vt/src/terminal/mode.rs`, base `:400-407`:

```rust
-    pub fn len(&self) -> usize {
-        self.len as usize
-    }
-
-    pub fn is_empty(&self) -> bool {
-        self.len == 0
-    }
```

`FlagStack::len` and `FlagStack::is_empty` are deleted. Neither is in the packet's Scope (which
names exactly `Engine`, `Engine::exit`, `lock_unfair`, `try_lock_unfair` and one of the demand
twins), and neither appears in the Evidence table "The five zero-user greps", against the acceptance
clause *"Zero users proved before deletion … the packet records the grep output"*.

I re-proved them dead myself, so this is bookkeeping, not risk:

```text
$ git grep -n -E 'FlagStack|\.(len|is_empty)\(\)' 4e83f31 -- crates/vt/src/terminal/
4e83f31:crates/vt/src/terminal/mode.rs:480:        if self.stack.len() >= TITLE_STACK_MAX {   # Vec::len, TitleState
4e83f31:crates/vt/src/terminal/mode.rs:492:        self.stack.len()                            # Vec::len, TitleState
```

No caller. (The cause is mechanical: narrowing them to `pub(crate)` makes them `dead_code`, which
`-D warnings` rejects, so the pass had to delete rather than narrow. That is the right call — it
just needed a line in the table.)

Full enumeration of genuinely removed functions across the diff, for the record — everything else is
a visibility narrowing, not a deletion:

```text
crates/terminal/src/handle.rs   -> exit, deref, deref_mut, take_render_demand, lock_unfair, try_lock_unfair
crates/vt/src/render/demand.rs  -> FILE DELETED (new, raise, release, is_raised — moved, not lost)
crates/vt/src/render/render_tests.rs -> pump_yields_to_the_render_demand_within_a_bounded_number_of_chunks (moved)
crates/vt/src/terminal/mode.rs  -> len, is_empty          <- D-4, the only undeclared pair
```

### D-5 — `Demand`'s public API is now asymmetric, and its own doc is unfulfillable. **Low**

`crates/terminal/src/handle.rs:49` publishes `pub struct Demand`, re-exported at
`crates/terminal/src/lib.rs:46`. After the narrowing:

| item | line | visibility |
| --- | --- | --- |
| `Demand::new` | `:52` | `pub(crate)` |
| `Demand::raise` | `:58` | **`pub`** |
| `Demand::release` | `:64` | `pub(crate)` |
| `Demand::is_raised` | `:78` | `pub(crate)` |

`raise`'s own doc at `:56-57` reads *"Call it **before** blocking on the lock, and pair it with
`Demand::release` once the lock is held."* No caller outside `crates/terminal` can obey that
instruction. The one external caller does exactly what the asymmetry permits and nothing else —
`crates/local-shell/src/event_loop_tests.rs:570` raises on a 16 ms watchdog loop and never releases:

```rust
while !stop.load(Ordering::Relaxed) {
    term.demand().raise();
    std::thread::sleep(Duration::from_millis(16));
}
```

That is deliberate in the test, but it is also the only shape the public API now allows, and an
unpaired `raise` pins the pump into yielding forever. The tell is in the packet's own `cargo doc`
list: `` [`Demand::release`] `` had to be downgraded to a plain code span *twice* because rustdoc
rejects a public doc linking a private item — that downgrade is the compiler reporting this, and it
was recorded as a formatting cost rather than read as a signal.

Either narrow `raise` to `pub(crate)` and give the test a crate-local helper, or keep `release`
`pub`. Not blocking; no current build is wrong.

---

## Notes (not defects)

### N-6 — 9 `unreachable_pub` remain, all test-harness, production is clean

```text
$ RUSTFLAGS=-Wunreachable_pub cargo check -p oneterm-vt -p oneterm-terminal --all-targets
warning: `oneterm-vt` (lib test) generated 9 warnings
    Finished `dev` profile in 14.75s
```

All nine are methods of the test harness `Engine` in `crates/vt/src/render/render_tests.rs`
(`:34, :47, :62, :67, :72, :76, :80, :87, :93`). **Zero** in the production code of either crate,
and zero in `oneterm-terminal` at all. The packet's proxy count excludes `*_tests.rs` by
construction, so this is consistent with how the pass was scoped; recording it because the packet
describes the method as compiler-driven and these are what the compiler still says.

Worth stating plainly, because it bounds what this lint proved: `unreachable_pub` detects *pub items
no path can reach*, not *pub items nobody uses*. It therefore says nothing about `RowRef`,
`cluster_width`, `FeedStats`, `DefaultColors` or `EventQueueDiagnostics` — all reachable, all
deliberately kept. Those I re-proved by grep instead; see below.

### N-7 — `TerminalLogError`'s disposition is unrecorded

Scope says to drop `DefaultColors`, `EventQueueDiagnostics` **and `TerminalLogError`** from the
`pub use` block. The Evidence table records the first two being kept, with reasons. `TerminalLogError`
is also still exported (`crates/terminal/src/lib.rs:50`) and is also correct to keep — it is the
error type of the public `TerminalLogController::start` / `stop` (`logging.rs:157, :179`), so
dropping it leaves a public method returning an unnameable type, the same argument as
`EventQueueDiagnostics`. Only the record is missing.

### N-8 — `check-doc-paths.py` still does not cover `docs/spec-intakes/**`

`DOCUMENTS` (`scripts/check-doc-paths.py:37-44`) is `docs/architecture.md`, `docs/README.md`,
`docs/terminal-backend.md`, `README.md`, `AGENTS.md`, `docs/agents/*.md` — 11 documents. The
spec-intake tree is not in it. The packet never claimed it would be, so this is not a defect;
recording it because D-3 lives in that tree and would still not be caught if it were a dead *path*
rather than a dead *method name* (which the checker cannot catch either way — it validates paths
only).

### N-9 — the one acceptance clause marked NOT MET is, on a direct measure, MET

The packet marks *"Net at least −80 production lines"* unmet at **−79**, reaching that figure by
`git diff --numstat` (+397 / −401, net −4) minus a hand adjustment of 75 lines for the ported test,
because a filename filter cannot see an inline `#[cfg(test)] mod tests`.

I reproduced the numstat exactly:

```text
$ git diff --numstat 4e83f31..18d7b4c -- crates/vt/src crates/terminal/src   # excl. *_tests/_props/_bench/test_support
+397 / -401     net -4
```

Counting production lines directly at each commit — everything above the first top-level
`#[cfg(test)]` in each changed file, applied identically to both sides — gives:

```text
production lines, crates/vt/src + crates/terminal/src
  base (4e83f31) = 10288
  head (18d7b4c) = 10196
  net            = -92          # gate: at least -80
  of which handle.rs 225 -> 209 (-16), render/demand.rs -62
```

**−92, not −79.** The hand adjustment under-counts because more of `handle.rs`'s `mod tests` grew
than the 75 lines attributed to the ported test. The packet's Gaps § 1 and the unticked acceptance
box should be revisited; nothing was padded, and the honest accounting simply cost the packet a
clause it had earned.

---

## Claim-by-claim results

### 1. Deletions — every one re-proved zero-user, everything builds

```text
$ grep -rn 'lock_unfair' crates/              (no output)
$ grep -rn 'take_render_demand' crates/       (no output)
$ grep -rn 'Engine::exit\|\.exit()' crates/   (no output)
$ grep -rnw 'Engine' crates/ --include='*.rs' | grep -v 'EngineView\|engine_\|completion::Engine\|base64::Engine'
  -> only oneterm_completion::Engine (8), crates/vt/src/render/render_tests.rs's harness struct,
     and one prose mention at crates/terminal/src/backend/osc_router.rs:39. No `oneterm_terminal::Engine`.
```

`cargo clippy --workspace --all-targets -- -D warnings` — **pass** (this is `cargo build
--workspace --all-targets` plus lints; it compiles `crates/tools`, every test target and every
bench, and `cargo test --workspace` then links them all). `cargo test --workspace --no-run` is
subsumed by the `cargo test --workspace` run below, which passed.

**Fuzz target** (`crates/vt/fuzz`): deliberately not a workspace member — its manifest says
libFuzzer is unusable on `x86_64-pc-windows-msvc` and it needs nightly + `cargo-fuzz`, so I did not
build it here. I verified instead that the whole API it names survives:
`crates/vt/fuzz/fuzz_targets/parser.rs:10` imports `oneterm_vt::parser::{Dispatch, OscParams,
Params, Parser, StringTerm}`; at `18d7b4c` `parser` is still `pub mod` (`crates/vt/src/lib.rs:31`)
and all five are still `pub` (`parser/mod.rs:28, :44, :112`, `parser/osc.rs:23, :32`,
`parser/params.rs:34`). It compiles.

### 2. The `Demand` move — semantically identical

Old `crates/vt/src/render/demand.rs` vs new `crates/terminal/src/handle.rs:49-80`: same
`Arc<AtomicUsize>` newtype; `raise` = `fetch_add(1, AcqRel)`; `release` = `fetch_update(AcqRel,
Acquire, checked_sub(1))` — the same saturating, non-wrapping decrement; `is_raised` = `load(Acquire)
> 0`. Waiter count, raise/release ordering and memory orderings are byte-identical. Only visibility
changed (see D-5).

Ported test passes (it is inside the green `cargo test --workspace`). It keeps all three assertions
(`waited < 2 s`, `chunks_waited <= 8`, `!demand.is_raised()`) and is now run against an unfair
`std::sync::Mutex`, which is a *stronger* subject for the flag than the `FairMutex` it left behind.

Consumers of the old path in the living design docs: `damage-and-render-state.md` **correct**
(`:222` names `crates/terminal/src/handle.rs` with the old path in parentheses; the whole
"Placement: the engine crate … a stated exception" block is gone). `events-and-api.md` **correct**
(no `Demand` reference; the added "As shipped" note is accurate). `migration.md` **wrong** → D-3.
`docs/terminal-backend.md:147` **correct**. Hits under `IN-0029/US-00*.md` and
`IN-0029/evidence/` are shipped packet records — historical, correctly untouched.

### 3. Visibility

| check | result |
| --- | --- |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `RUSTFLAGS=-Wunreachable_pub cargo check -p oneterm-vt -p oneterm-terminal --all-targets` | 9 warnings, all in `render_tests.rs`; production 0 — see N-6 |
| `crates/tools` (vt-corpus, vt-bench, grep-deviations), `crates/vt/tests/*`, `crates/vt/benches/*` | all built by `--all-targets`; pass |
| fuzz target | API verified present, see claim 1 |
| `RowRef::is_allocated` / `occ` / `cells` still `pub` | **yes** — `crates/vt/src/grid/row.rs:307, :292, :296`; `RowRef` re-exported at `crates/vt/src/lib.rs:42` |
| `grid` / `intern` / `parser` still `pub mod` | **yes** — `crates/vt/src/lib.rs:30-32` |

Public-surface proxy, reproduced independently (`git grep` at the base, plain `grep` at head):

```text
             base(4e83f31)   head(18d7b4c)   gate
oneterm-vt        535            392         <= 400   PASS
oneterm-terminal  301            269         <= 270   PASS
```

`crates/vt` interior mutability — the claim behind `lib.rs:4`:

```text
$ grep -rn 'Atomic\|atomic::' crates/vt/src                                     (no output)
$ grep -rn 'RefCell\|UnsafeCell\|OnceLock\|OnceCell\|Mutex\|RwLock\|thread_local' crates/vt/src
  -> only crates/vt/src/intern_tests.rs:1,8,12,38 (std::sync::Mutex in a test)
```

True as written, and true in the stronger form the crate doc now claims ("There is no atomic here at
all"). Also no `unsafe` block in the crate — the four hits are prose saying `GlobalAlloc` is an
unsafe trait this crate has none of.

The "deliberately kept" exports, re-proved by grep rather than inherited:

| export | hits outside `crates/vt/` | disposition |
| --- | --- | --- |
| `RowRef` | 0 | correctly kept — `US-0092` reads it from `crates/terminal/src/content.rs` (confirmed in `f963a94`, see claim 8) |
| `cluster_width` | 0 | correctly kept — mode-2027 entry point, reason recorded at the re-export |
| `FeedStats` | 0 | correctly kept — return type of the public `Terminal::feed` |
| `DefaultColors` | `crates/terminal/src/session.rs:520` as `$crate::DefaultColors::new(` | **the packet's best catch.** Confirmed: an identifier grep in `crates/local-shell` / `crates/ssh` cannot see a `$crate::` path expanded from `impl_pty_terminal_session!`. Dropping the re-export would have broken both backends |
| `EventQueueDiagnostics` | return type of public `SessionEventSink::diagnostics` (`backend/event_sink.rs:57`) | correctly kept |
| `TerminalLogError` | error type of public `TerminalLogController::start`/`stop` | correctly kept, **unrecorded** → N-7 |

### 4. `terminal-diagnostics` gate

The new step, run on its own from this worktree:

```text
==> cargo clippy --workspace --all-targets --features oneterm-app/terminal-diagnostics -- -D warnings
    (pass — step 3 of the ci-local run below)
```

Step lists agree — same eleven commands, same order, in all three places:

| | `ci.yml` | `ci-local.sh` | `ci-local.ps1` | `AGENTS.md` § 4 |
| --- | --- | --- | --- | --- |
| `cargo fmt --all -- --check` | L117 (Linux job) | L26 | L38 | yes |
| `cargo clippy … -D warnings` | L119 / L160 | L27 | L39 | yes |
| `cargo clippy … --features oneterm-app/terminal-diagnostics` | **L125 / L162** | **L29** | **L41** | **yes** |
| `cargo test --workspace` | L128 / L164 | L30 | L42 | yes |
| `cargo test -p oneterm-vt --features vt-paranoid` | L135 / L170 | L35 | L47 | yes |
| the six `python` policy checks | L62-74 (policy job) | L36-41 | L48-53 | yes |

Both quality-gate jobs carry the new step (Linux `:125`, Windows `:162`), each directly after the
plain clippy step. `AGENTS.md` § 4's fenced list matches `ci-local` line for line.

The `#[cfg(test)]` re-gating removes nothing a UI exposes:

```text
$ grep -rn 'diagnostics' crates/app/src crates/settings-ui/src crates/terminal-view/src
  crates/app/src/crash_report.rs:1     -> crash diagnostics, unrelated
  crates/terminal-view/src/render/…    -> FrameStats / DiagnosticsLog, a different, untouched feature site
  crates/settings-ui/src               -> no hits at all
```

`SshCommandDiagnostics` was already `pub(crate)`, so no public API changed, and the two re-gated
items (`crates/ssh/src/transport.rs:49, :103`) have no caller outside `crates/ssh`'s tests. The
counters themselves (`CommandCounters`, `record_failure`) correctly keep `any(test, feature)`. Clean.

### 5. `check-doc-paths.py`

```text
$ python scripts/check-doc-paths.py
Doc path check passed for 188 current paths in 11 documents.      (packet claims 188 / 11 — exact)
exit=0
```

Tamper A — a dead path in a newly covered document:

```text
$ printf '\nTamper probe: crates/terminal/src/no_such_file.rs\n' >> docs/terminal-backend.md
$ python scripts/check-doc-paths.py
error: docs/terminal-backend.md: path does not exist: crates/terminal/src/no_such_file.rs
exit=1                                                            # caught. restored with git checkout.
```

Tamper B — the documented blind spot, a tree-diagram leaf, run in isolation:

```text
$ sed -i 's/├── handle.rs/├── zzz_ghost.rs/' docs/terminal-backend.md   # first occurrence only
$ python scripts/check-doc-paths.py
Doc path check passed for 188 current paths in 11 documents.
exit=0                                                            # NOT caught — blind spot is real
```

D1 is genuinely not detectable, and the limit is written into the script's module doc
(`scripts/check-doc-paths.py:20-22`) as the packet says. Both tampers reverted; `git status` clean.

`DOCUMENTS` covers spec-intakes? **No** — see N-8.

### 6. Test-count accounting — exactly one moved, none dropped

Rather than two expensive `--list` runs, I diffed every `#[test]`-annotated function name between
the two commits:

```text
oneterm-vt        371 -> 370     removed: pump_yields_to_the_render_demand_within_a_bounded_number_of_chunks
                                 added:   (none)
oneterm-terminal  270 -> 271     removed: (none)
                                 added:   a_pump_yields_to_the_demand_within_a_bounded_number_of_chunks
```

One test, one rename, net zero. The `-1` in the ci-local total is fully explained: `cargo test -p
oneterm-vt --features vt-paranoid` runs the `vt` suite a second time, so the moved test was counted
twice at baseline and once now. Accounting confirmed.

### 7. `pwsh scripts/ci-local.ps1` — from this worktree, `CARGO_BUILD_JOBS=3`

```text
==> cargo fmt --all -- --check
==> cargo clippy --workspace --all-targets -- -D warnings
==> cargo clippy --workspace --all-targets --features oneterm-app/terminal-diagnostics -- -D warnings
==> cargo test --workspace
==> cargo test -p oneterm-vt --features vt-paranoid
==> python scripts/verify-dependency-graph.py
==> python scripts/check-doc-paths.py
Doc path check passed for 188 current paths in 11 documents.
==> python -m unittest scripts/test_check_english.py
==> python scripts/check-english.py
==> python scripts/completion-catalog.py validate
==> python scripts/third-party-notices.py --check

ci-local: all checks passed.
EXITCODE=0
```

| | sections | passed | failed | ignored |
| --- | ---: | ---: | ---: | ---: |
| packet claims | 60 | 1934 | 0 | 14 |
| this run | **60** | **1934** | **0** | **14** |

Exact match.

### 8. Merge-ability against `US-0092` (`f963a94`)

```text
$ git merge-base 18d7b4c f963a94
4e83f31836050569e579447475236e3cc73a13ae          # same base, clean three-way

$ git merge-tree --write-tree 18d7b4c f963a94
c94a0a6b207c4eb8c19898900953e1142d48343b
100644 20bafc1… 1   crates/terminal/src/handle.rs
100644 6ae7cef… 2   crates/terminal/src/handle.rs
100644 995d847… 3   crates/terminal/src/handle.rs

Auto-merging crates/terminal/src/content.rs
Auto-merging crates/terminal/src/handle.rs
CONFLICT (content): Merge conflict in crates/terminal/src/handle.rs
Auto-merging docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md
Auto-merging docs/spec-intakes/IN-0032-terminal-crate-tidy/IN-0032.md
exit 1
```

**One conflicting file: `crates/terminal/src/handle.rs`. One hunk. Take both sides.**

`US-0092`'s entire change to that file is five doc-comment lines appended to `lock_for_render`'s
doc block:

```
+    /// That premise held everywhere except `terminal_info`'s `last_content_row`,
+    /// which scanned the whole viewport on an idle screen; `US-0092` made it
+    /// cost the content instead. So the premise is true again, and nothing here
+    /// needs to move onto `lock_for_render`.
     pub fn lock_for_render(&self) -> FairMutexGuard<'_, Engine> {
```

`US-0090` rewrote the surrounding doc block and changed the signature to
`FairMutexGuard<'_, Terminal>`. **Prefer `US-0090`'s side for the code and the existing doc lines
(the `Engine` newtype is gone, so `f963a94`'s signature line must not survive), then append
`US-0092`'s four-line paragraph at the end of the doc block.** Nothing else collides.

The two files both branches touch that auto-merged are also semantically compatible:

- `crates/terminal/src/content.rs` — `US-0090` rewrites the import block at `:18-24`; `US-0092`
  edits `last_content_row`'s body at `:47-84`. Disjoint. `US-0092`'s new calls
  `row.occ()`, `row.is_allocated()`, `row.cells()` all remain `pub` after `US-0090`'s narrowing
  (`crates/vt/src/grid/row.rs:292, :307, :296`), and `RowRef` stays re-exported — the packet's
  hand-off promise holds.
- `docs/…/damage-and-render-state.md` and `IN-0032.md` — disjoint sections.

### 9. Trailers

All three commits, verified with `git log --format='%(trailers)'`:

```text
18d7b4c docs(harness): complete US-0090 and settle the terminal-diagnostics decision
4bba4e2 docs(terminal): re-point the doc rot the engine migration left, and widen the checker
2c5a712 refactor(terminal): delete the migration residue and narrow both terminal crates

each ending:
  Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01Q6xr5jX29B2b6L4MGsoNdW
```

**Correct on all three.** Each also carries `Refs: US-0090, IN-0032` and a Conventional Commits
subject.

---

## What I would require before accepting

1. Fix D-1 / D-2: `docs/terminal-backend.md:449-457` and `:612-616`, and correct the Reconciliation
   table so it stops claiming work that was not done.
2. Fix D-3: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/migration.md:236-249`, and add it
   to Documentation Action § Owning Docs Reviewed.
3. Record D-4's two deletions in the Evidence table with the grep above.
4. Revisit N-9 — tick the line-delta clause at −92, or say why the numstat measure is the one that
   binds.

D-5, N-6, N-7 and N-8 are worth a line in Gaps; none of them blocks.

The code half of this packet is clean work, and the `DefaultColors` catch in particular is the kind
of finding a mechanical pass does not make. The gap is that the doc half stopped one section short
of its own claim, twice, in the packet whose subject is exactly that.
