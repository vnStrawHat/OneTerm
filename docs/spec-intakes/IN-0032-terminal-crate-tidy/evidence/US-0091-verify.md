# US-0091 — independent verification

Verifier: a second agent, in a worktree of its own
(`D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-a1eb678638e10bf81`).
Date: 2026-09-14.
Head verified: `5d2e03a`, four commits on `4bb088d`.
Not committed, not pushed; nothing outside this worktree was touched.

```text
5d2e03a docs(terminal): record PtySession, and that US-0091 misses its line-delta gate
aa54276 refactor(ssh): SshSession becomes a PtyOwner
70c7831 refactor(local-shell): LocalSession becomes a PtyOwner
b48537c refactor(terminal): one concrete PtySession replaces the 286-line macro
4bb088d docs(terminal): record the independent verification of US-0090
```

## Verdict

**Correctness: PASS-WITH-NOTES.** `PtySession<O: PtyOwner>` is behaviourally identical to the
deleted `impl_pty_terminal_session!` expansion — every method, every early return, every log
string, the `close()`/`Drop` split and the resize policies all match, line for line. Every
number the packet reports reproduces exactly. Six notes below; none is a behaviour change, and
the one that matters (**D1**) is a *pre-existing* test-coverage hole this packet's mitigation
strategy silently relies on.

**Line-delta gate: correctly FAILED, and it is unreachable.** The packet's own measurement
(production −19, total 0) reproduces exactly. My independent slimming pass finds **≈ 25–30
lines** available, which would land the net at roughly **−45 to −49** — still short of −80. The
gate cannot be met without changing the four-trait partition, which both the audit and this
packet put out of scope. The implementer's diagnosis ("ownership of `marked_text` / `event_rx`
did move; the audit under-counted the irreducible forwarding") is **correct** — see
§ Slimming judgement for the measured breakdown.

## 1. Behavioural identity — macro expansion vs the four trait impls

The macro was at `4bb088d:crates/terminal/src/session.rs:443-728`. Compared method by method
against `5d2e03a:crates/terminal/src/session.rs:445-767` plus the two
`session_terminal.rs`. Result: **no behavioural difference**. Every checked point:

| Concern | Macro (`4bb088d`) | `PtySession` (`5d2e03a`) | Same? |
| --- | --- | --- | --- |
| `pty_write` alive guard | `if !self.state.alive() { return Err(Closed) }` then `PtyTransport::pty_write(self.listener.transport(), b)` (`:479-484`) | identical guard at `session.rs:561-566`, then `self.owner.pty_write(b)` → `self.transport().pty_write(b)` → `self.listener.transport()` in **both** backends | yes |
| `resize` | `needs_resize` → `pty_resize` → `resize_grid` (`:578-588`) | same order, `session.rs:644-650` | yes |
| `query_state` liveness arg | `TerminalLifecycle::alive(self)` = `self.state.alive()` | `self.state.alive()` (`:579`) | yes |
| `set_default_colors` | `self.state.set_default_colors(DefaultColors::new(..))` | identical (`:591-593`) | yes |
| `flush_pty` / `send_ctrl_c` log text | `concat!($label, ": PTY flush query failed: {}")` → `"LocalSession: PTY flush query failed: …"` | `"{}: PTY flush query failed: {error}", self.label()` — `label()` is `"LocalSession"`/`"SshSession"` (`:544-550`) | byte-identical |
| generated-input reports (5 mouse sites, `clear`, `commit_text`) | `report_generated_input(concat!($label, " mouse input"), …)` → `"LocalSession mouse input delivery failed: …"` | `self.report("mouse input", …)` → `"{} {operation} delivery failed: {error}"` (`:552-558`) | byte-identical |
| `clear()` ordering | `pty_write(b"clear\r")` then `clear_selection` | identical (`:710-713`) | yes |
| IME `marked_text` | `Mutex<Option<String>>` on the backend; `commit_text` clears then writes | `Mutex<Option<String>>` on `PtySession` (`:487`); same order (`:724-730`) | yes |
| `event_rx` single receiver | `self.event_rx.lock().unwrap().take()` | identical (`:737`) | yes |
| `close()` vs liveness | `self.$close()` **then** `state.set_alive(false)` | `self.owner.close()` **then** `state.set_alive(false)` (`:744-748`) | yes |
| `capabilities()` | `impl TerminalSession for {Local,Ssh}Session` | `PtyOwner::capabilities` (same bodies, incl. the SSH poisoned-lock `into_inner`), forwarded by `PtySession` (`:763-767`) | yes |
| `owner_join` / CORR-10 | `LocalSession::drop` + `shutdown_owner` | unchanged, still in `crates/local-shell/src/session.rs:223-247` and `:143-154`; `PtySession` owns the `LocalSession`, so its `Drop` still runs | yes |
| SSH `close_channel` | `pty_close()` then `close_sftp()` (ARCH-28) | same body, now `PtyOwner::close` (`crates/ssh/src/session_terminal.rs:22-26`) | yes |
| `SharedState` identity | one `Arc` | `state.clone()` into `PtySession`, `state` into `SshSession` — `SharedState = Arc<SharedSessionState>` (`backend/state.rs:82`), so the same object | yes |

Only deviation found, and it is benign — **N1** below (drop order of `event_rx` relative to the
backend's `Drop`).

## 2. Resize policy (DEC-0008)

- `4bb088d:crates/terminal/src/model.rs` `From<ResizePolicy> for oneterm_vt::ResizePolicy`
  mapped `Default → BottomAnchor` and `KeepViewportTop → KeepViewportTop`. So the renames in
  `local_resize_policy()`, `model_tests.rs` and `local_session_grow_policy_matches_conpty` are
  **value-preserving**. Verified against the deleted `impl` in the diff, not from memory.
- Local: `KeepViewportTop` on Windows, `BottomAnchor` elsewhere
  (`crates/local-shell/src/session_terminal.rs:13-19`). Matches DEC-0008 ("For `SessionKind::Local`
  sessions **on Windows** … SSH sessions keep alacritty's default behaviour").
- SSH: `const SSH_RESIZE_POLICY: ResizePolicy = ResizePolicy::BottomAnchor`
  (`crates/ssh/src/session.rs:56`), used by `connect` **and** the test helper. New test
  `ssh_session_grow_policy_is_bottom_anchored` pins it; the pre-existing behavioural test
  `ssh_grow_resize_pulls_scrollback_into_the_viewport_top` still passes untouched.
- `DEC-0008-local-conpty-grow-resize-keeps-viewport-top.md` does not name the adapter enum —
  the packet's "no re-word needed" is correct (grepped).

**No mismatch. No MAJOR here.**

## 3. Public surface

`US-0090`'s own proxy command, re-run in this worktree:

```text
vt 392
terminal 272
```

The +5 decomposes as **+7 / −2** (computed by diffing the two proxy lists):

| added | removed |
| --- | --- |
| `pub trait PtyOwner` | `pub use model::ResizePolicy` (lib.rs) |
| `pub struct PtySession<O: PtyOwner>` | `pub enum ResizePolicy` (model.rs) |
| `pub fn new` | |
| `pub fn owner` | |
| `pub fn term` | |
| `pub fn state` | |
| `pub fn resize_policy` | |

Needed cross-crate?

| item | who needs it | `pub` justified? |
| --- | --- | --- |
| `PtyOwner` + its 4 methods | implemented by both backend crates | **yes**, unavoidable |
| `PtySession` | returned by `LocalSession::spawn` / `ssh::connect` | **yes** |
| `new` | both backends construct it | **yes** |
| `owner()` | `crates/local-shell/src/session_tests.rs:186` only (`s.owner().owner_join`) | test-only |
| `term()` | `session_tests.rs:176`, `crates/ssh/src/session.rs:847` — **tests only** | test-only |
| `state()` | `crates/ssh/src/session.rs:871` — **one test**, which already has its own `state` clone three lines above the constructor (`:809`) | **removable** |
| `resize_policy()` | `session_tests.rs:146`, `crates/ssh/src/session.rs:830` — tests only | test-only |

Grep confirming no production user:
`grep -rn "\.owner()\|\.term()\|\.state()\|\.resize_policy()" crates/` returns only the four
test sites above plus unrelated `logging.state()` / `pump.state()` matches.

The struct's fields are all private ✓. `PtyOwner` is object-safe but nothing boxes it ✓.
`PtySession` is a concrete generic struct — object-safety does not apply to it; what matters is
that `TerminalSession` stays object-safe, and both call sites still do
`Box::new(session) as Box<dyn TerminalSession>` (`crates/app/src/session_factory.rs:25`,
`crates/ssh/src/session.rs:484`) ✓.

`DefaultColors` root re-export: `grep -rn "oneterm_terminal::DefaultColors" docs/ crates/`
returns **only** the US-0091 packet's own prose describing the removal. No stale doc ✓.

## 4. Slimming judgement

Measured composition of the 323 new lines (`session.rs:445-767`): **224 code, 51 doc, 48 blank**.

| block | lines | code | doc | verdict |
| --- | ---: | ---: | ---: | --- |
| `PtyOwner` (`:445-468`) | 24 | 8 | 16 | code irreducible; doc is 2× the code |
| `PtySession` struct (`:470-492`) | 23 | 9 | 14 | same |
| inherent `impl` (`:494-567`) | 74 | 52 | 14 | one dead accessor; rest earns its keep |
| `TerminalRender` (`:569-619`) | 51 | 40 | 0 | **irreducible** |
| `TerminalInput` (`:621-714`) | 94 | 73 | 7 | **irreducible** |
| `TerminalIme` (`:716-733`) | 18 | 15 | 0 | **irreducible** |
| `TerminalLifecycle` (`:735-761`) | 27 | 22 | 0 | **irreducible** |
| `TerminalSession` (`:763-767`) | 5 | 5 | 0 | **irreducible** |

The five `impl` blocks are **195 lines (155 code)** for 45 methods — 3.4 lines each, which is
what a forwarding method costs in `rustfmt`'s output. Nothing is available there. The packet's
"about 205 and irreducible" is honest (measured: 195).

**Estimate: ≈ 25–30 lines.** Production net would move from **−19 to about −45 … −49**, still
~30 short of the −80 gate.

The three biggest cuts, in order:

1. **≈ 22 lines — rustdoc that restates the traits and `docs/terminal-backend.md` §9.**
   `session.rs:450-452` (three lines explaining why `pty_write`/`pty_resize` are restated — that
   is packet rationale, not API doc), `:475-479` (a near-verbatim restatement of the §9
   blockquote at `docs/terminal-backend.md:789-798`), and the four one-line docs on trivial
   getters (`:518`, `:523`, `:528`, `:533`). A concrete impl can point at the trait rather than
   repeat it; here it mostly repeats the *design doc*.
2. **−4 lines and −1 `pub` item — delete `pub fn state()` (`session.rs:528-531`).** Its single
   user (`crates/ssh/src/session.rs:871`) is a test whose helper already binds `state` and passes
   `state.clone()` into the constructor at `:809`; returning it alongside `listener` (which the
   helper already does) removes the accessor. That takes the proxy from **272 to 271** — one
   short of `US-0090`'s ≤ 270, not enough on its own.
3. **0 lines, better shape — `#[doc(hidden)]` on `owner()` / `term()` / `resize_policy()`.**
   All three are test-only; advertising them as `oneterm-terminal` API invites a real consumer.

Considered and **rejected** (worth recording so nobody re-litigates it):

- **Move `kind` and `resize_policy` onto `PtyOwner` as defaulted methods.** Removes 2 fields,
  2 constructor args and 2 assignments in `crates/terminal` (≈ −8) but adds ≈ +6 in the two
  backends. Workspace net ≈ −2, and it scatters DEC-0008 across three files. No.
- **A builder, or public fields + struct literal, instead of the 6-arg `new`.** Both are
  *longer*: a builder is +15, a struct literal duplicates 7 field names in two crates and leaks
  the fields. Six positional args of six distinct types is the right call.
- **`impl Deref<Target = TerminalModel>`** to collapse the 13 `TerminalRender` forwards.
  Impossible: `model()` returns by value (it clones an `Arc`), so there is nothing to borrow.
- **Merging `label()`/`report()` into `report_generated_input`.** `report()` *saves* lines
  (5 call sites × 3), and `report_generated_input` still has two external users
  (`crates/state/src/input_channel_registry.rs:177`, `crates/terminal-view/src/input/keys.rs:309`).
- **Collapsing the four traits.** Out of scope by the packet and the audit; it is also the only
  thing that would reach −80, which is the finding the owner needs.

**Conclusion for the owner:** the −80…−200 band was set from the audit's "~230 new lines"
estimate, which under-counted the forwarding cost by ~90 lines. The band is not reachable by
tidying this type. The decision is between accepting a decoupling that is a line-count wash
(the compile-time and coupling wins are real: `macro_export` is gone from
`crates/terminal/src`, so neither backend expands 286 lines of another crate's source any more)
and changing the gate.

## 5. Crate graph

- No manifest changed (`git diff --stat 4bb088d..5d2e03a` lists 12 files, no `Cargo.toml`).
- `python scripts/verify-dependency-graph.py` → `Dependency graph policy passed for 21 workspace
  packages and 21 explicit members.` (exit 0)
- `grep -rn "macro_export\|macro_rules" crates/terminal/src/` → one hit,
  `osc_agent/mod.rs:245` (`macro_rules! payload!`, test-local, never exported). **No
  `#[macro_export]` left** ✓ — neither backend compiles `crates/terminal` source any more.
- `grep -rn "impl_pty_terminal_session" crates/ docs/` → zero hits under `crates/`; docs hits are
  historical records only ✓ (the packet's Acceptance claim holds).

## 6. Tests

```text
cargo test -p oneterm-terminal -p oneterm-local-shell -p oneterm-ssh
  oneterm-local-shell  33 passed; 0 failed; 2 ignored
  oneterm-ssh          68 passed; 0 failed; 0 ignored
  oneterm-terminal    271 passed; 0 failed; 0 ignored
  exit 0
```

Matches the packet exactly (271 / 33+2 / 68).

### Tamper — **this is D1, the one finding that matters**

Removed `self.owner.pty_resize(rows, cols)?;` from `PtySession::resize`
(`crates/terminal/src/session.rs:646`), leaving only `resize_grid` — i.e. the local shell never
tells ConPTY the new size and SSH never sends `window_change`:

```text
test result: ok. 33 passed; 0 failed; 2 ignored   (oneterm-local-shell)
test result: ok. 68 passed; 0 failed; 0 ignored   (oneterm-ssh)
test result: ok. 271 passed; 0 failed; 0 ignored  (oneterm-terminal)
```

**All 372 tests still pass.** Restored and re-verified.

A second, contrasting tamper was not needed: `close()`, the alive guard, IME, mouse and
`take_events` *are* covered — `dropping_a_closed_session_is_idempotent`,
`close_returns_without_joining_the_owner_thread`,
`dead_session_rejects_input_without_touching_the_transport`,
`trait_ime_commit_writes_and_clears_marked`, `trait_mouse_in_normal_mode_starts_selection` and
`trait_take_events_hands_out_the_receiver_once` all fail if their path is broken. Only the
PTY-resize forward is unguarded. See **D1**.

### `pwsh scripts/ci-local.ps1`

Run in this worktree with `$env:CARGO_BUILD_JOBS = "3"`, **exit 0**:

```text
==> cargo fmt --all -- --check                                          ok
==> cargo clippy --workspace --all-targets -- -D warnings               ok
==> cargo clippy --workspace --all-targets \
      --features oneterm-app/terminal-diagnostics -- -D warnings        ok
==> cargo test --workspace                                              ok
==> cargo test -p oneterm-vt --features vt-paranoid                     ok
==> python scripts/verify-dependency-graph.py
    Dependency graph policy passed for 21 workspace packages and 21 explicit members.
==> python scripts/check-doc-paths.py
    Doc path check passed for 188 current paths in 11 documents.
==> python -m unittest scripts/test_check_english.py                    OK (2 tests)
==> python scripts/check-english.py
    English contributor-text check passed for 785 files.
==> python scripts/completion-catalog.py validate
    [completion-catalog] all catalogs valid
==> python scripts/third-party-notices.py --check
    THIRD-PARTY-NOTICES.md is up to date.

ci-local: all checks passed.
```

Tallied from the log: **60 `test result:` sections, 1941 passed, 0 failed, 14 ignored, 0
FAILED sections.** **Reproduces the packet's 60 / 1941 / 0 / 14 exactly.**

## 7. Docs

| doc | claim | verdict |
| --- | --- | --- |
| `docs/terminal-backend.md` §3 (crate table, `:99-100`) | both rows now say `PtyOwner` + "returns the `PtySession` the UI drives" | accurate |
| §5.3 (`:268`, `:283-288`) | names `local_resize_policy()` and `SSH_RESIZE_POLICY`, records the adapter enum's deletion | accurate; one cosmetic line-wrap artefact at `:268` (`"…BottomAnchor` anchors the bottom\nrow:"`) left by the edit |
| §9 blockquote (`:789-798`) | describes `PtySession<O: PtyOwner>` and what each backend still owns | accurate |
| §11 file layout (`:873`, `:891-892`, `:898-899`) | the three `session.rs`/`session_terminal.rs` entries | accurate |
| `IN-0029/low-level-design/migration.md` `:119`, `:321`, `:429` | still describe the macro as live and assign its removal to `US-0085` | **not annotated** — see **N4** |
| `US-0091` packet Status | `Planned` + `In progress` ticked, `Implemented` **not** ticked, with the gate failure stated up front in Evidence | **honest** ✓ |
| `IN-0032.md` `:190-193` | records the unticked packet and why | accurate |
| `docs/PROJECT.md` Important Boundaries | confirmed no edit needed — `TerminalSession` has the same methods and signatures | correct |

Line counts independently recomputed with the packet's stated method (non-blank lines, cut at
`#[cfg(test)]`, `_tests.rs` excluded from "production"), over the same nine files:

```text
4bb088d prod 2062 total 3112
5d2e03a prod 2043 total 3112
```

**Reproduces the packet's −19 / 0 exactly.**

## 8. Merge-ability with `US-0092` (`5d3f9eb`)

```text
git merge-tree --write-tree 5d2e03a 5d3f9eb
c2a813d9d95fce463744cdb45838944faf87ebf3
CONFLICT (content): Merge conflict in docs/spec-intakes/IN-0032-terminal-crate-tidy/IN-0032.md
```

**One conflict, docs only, trivial.** Both sides edit the same region of `IN-0032.md`: `US-0092`
flips its own checkbox `- [ ]` → `- [x]` at `:190`; `US-0091` inserts four lines immediately
above it. **Prefer both sides** — keep `US-0091`'s four inserted lines *and* `US-0092`'s ticked
checkbox. No source file conflicts; the two packets share no `crates/` file.

## 9. Commit trailers

All four commits carry both required trailers, verified with
`git log --format='%h %s%n%(trailers)' 4bb088d..5d2e03a`:

```text
Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Q6xr5jX29B2b6L4MGsoNdW
```

✓ on `b48537c`, `70c7831`, `aa54276`, `5d2e03a`.

## Defects and notes

### D1 — MAJOR (coverage, pre-existing): nothing tests that a session resize reaches the PTY

- **Where:** `crates/terminal/src/session.rs:644-650` (`TerminalInput::resize`).
- **Repro:** delete line 646 (`self.owner.pty_resize(rows, cols)?;`), rebuild, run
  `cargo test -p oneterm-terminal -p oneterm-local-shell -p oneterm-ssh` → **372 passed, 0
  failed**. A user would see the grid resize while the child process kept the old `WINSIZE`:
  every full-screen TUI mis-renders.
- **Why it matters here:** it is *not* a regression — the macro had the same hole, and
  `crates/ssh/src/transport_tests.rs:117-120` tests `SshTransport::pty_resize` in isolation, never
  the session→owner→transport hop. But this packet's entire risk mitigation is "move bodies, do
  not rewrite them, and let the test counts and the unchanged assertions prove it"
  (packet § Context), and for this one path the tests prove nothing. The claim "behaviourally
  identical" rests on my line-by-line reading above, not on the suite.
- **Fix, ~12 lines, and the packet has just made it cheap:** `PtySession` is now a concrete
  generic type, so a `struct FakeOwner { resizes: Mutex<Vec<(u16,u16)>> , writes: … }` in
  `crates/terminal/src/session.rs`'s `mod tests` gives `crates/terminal` its first direct unit
  test of the forwarding it now owns. Under the macro this was impossible without a backend.
  Recommend as a follow-up on this packet (or a one-line addition before it merges).

### D2 — MINOR: `crates/terminal` gained a public type with no test of its own

`cargo test -p oneterm-terminal` is **271 → 271**. The five `mod tests` cases in `session.rs`
all drive `FakeTerminalSession`, which implements the four traits directly and never touches
`PtySession`. The crate that now *owns* 195 lines of forwarding has zero tests for them; the
proof lives entirely in the two backend crates. Same fix as **D1**.

### N1 — NOTE: `event_rx` now drops before the backend's `Drop` body

Field order in `PtySession` (`:483-491`) puts `owner` last, so on drop the order is
`term, state, marked_text, event_rx, owner` → then `LocalSession::drop` / `SshSession::drop`.
Previously the backend's `Drop` body ran **before** `event_rx`. So the event receiver now closes
slightly *earlier* relative to `pty_close`. Benign: a closed event channel is counted, not fatal
(`backend_tests.rs:486-491`, `a_closed_channel_is_counted_not_panicked`), and in production the
UI has already taken the receiver via `take_events()`, leaving `None` to drop. Recording it
because no test covers the ordering and the packet does not mention it.

### N2 — NOTE: four of the five new `pub` items exist only for tests in other crates

`owner()`, `term()`, `state()`, `resize_policy()` — see §3. The packet's Gaps states this
plainly and honestly. `state()` is removable outright (§4, cut 2).

### N3 — NOTE: `LocalSession::config()` is dead

`crates/local-shell/src/session.rs:124-127` plus the `config` field it reads. `grep -rn
"\.config()" crates/` finds no caller anywhere in the workspace. Pre-existing (dead at
`4bb088d` too, so not this packet's defect), but it is exactly the residue `IN-0032` exists to
remove, and this packet had `session.rs` open. −6 lines including the field. Suggest folding it
into `US-0093` or a follow-up rather than reopening this packet.

### N4 — NOTE: `migration.md` still describes the macro as live

`docs/spec-intakes/IN-0029-vt-engine/low-level-design/migration.md:321` says the macro's return
type "changes first, in `crates/terminal`, at `US-0085`" and `:429` says the fork manifest lines
"survive only because `impl_pty_terminal_session!` expands …". Both are now false. The packet
consciously declines to rewrite the record ("history, not rewritten", Reconciliation) and that
is defensible for a *completed* intake's LLD — but `:321` reads as an **open plan**, not a
record. One trailing sentence ("Closed by `US-0091`, which deleted the macro instead.") would
cost a line and stop the next reader chasing `US-0085`. Not blocking.

### N5 — NOTE: `docs/terminal-backend.md` §6.2's sketch is now doubly stale

`:407-418` still shows `pub fn spawn(cfg, initial) -> core::Result<Self>`; `spawn` now returns
`PtySession<Self>`. The block is already fenced as a "historical design sketch" whose shipped
code "differs", so this is pre-existing staleness that got one notch worse. Not blocking.

### N6 — NOTE (cosmetic): a stranded line wrap in §5.3

`docs/terminal-backend.md:268` now reads
`` `ResizePolicy::BottomAnchor` anchors the bottom `` / `row:` after the parenthetical was
removed. Re-flow when the file is next touched.

## Raw outputs

```text
$ git log --oneline -5
5d2e03a docs(terminal): record PtySession, and that US-0091 misses its line-delta gate
aa54276 refactor(ssh): SshSession becomes a PtyOwner
70c7831 refactor(local-shell): LocalSession becomes a PtyOwner
b48537c refactor(terminal): one concrete PtySession replaces the 286-line macro
4bb088d docs(terminal): record the independent verification of US-0090

$ git diff --stat 4bb088d..5d2e03a
 crates/local-shell/src/session.rs           |  41 +-
 crates/local-shell/src/session_terminal.rs  |  38 +-
 crates/local-shell/src/session_tests.rs     |  18 +-
 crates/ssh/src/session.rs                   |  95 ++--
 crates/ssh/src/session_terminal.rs          |  36 +-
 crates/terminal/src/lib.rs                  |  15 +-
 crates/terminal/src/model.rs                |  53 +-
 crates/terminal/src/model_tests.rs          |  16 +-
 crates/terminal/src/session.rs              | 579 ++++++++++++---------
 .../IN-0032-terminal-crate-tidy/IN-0032.md  |   4 +
 .../US-0091-concrete-pty-session.md         | 225 ++++++--
 docs/terminal-backend.md                    |  33 +-
 12 files changed, 665 insertions(+), 488 deletions(-)

$ for c in vt terminal; do grep -rn '^[[:space:]]*pub \(fn\|struct\|enum\|const\|static\|mod\|trait\|type\|use\)' \
    crates/$c/src --include='*.rs' | grep -v '_tests\.rs\|_props\.rs\|_bench\.rs\|test_support\.rs' | wc -l; done
392
272

$ python scripts/verify-dependency-graph.py
Dependency graph policy passed for 21 workspace packages and 21 explicit members.

$ grep -rn "macro_export\|macro_rules" crates/terminal/src/
crates/terminal/src/osc_agent/mod.rs:245:    macro_rules! payload {

$ cargo test -p oneterm-terminal -p oneterm-local-shell -p oneterm-ssh
test result: ok. 33 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 10.21s
test result: ok. 68 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.78s
test result: ok. 271 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s

$ # TAMPER: PtySession::resize with `self.owner.pty_resize(rows, cols)?;` deleted
$ cargo test -p oneterm-terminal -p oneterm-local-shell -p oneterm-ssh
test result: ok. 33 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 10.20s
test result: ok. 68 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.71s
test result: ok. 271 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
$ # restored

$ git merge-tree --write-tree 5d2e03a 5d3f9eb   # exit 1
c2a813d9d95fce463744cdb45838944faf87ebf3
CONFLICT (content): Merge conflict in docs/spec-intakes/IN-0032-terminal-crate-tidy/IN-0032.md
```
