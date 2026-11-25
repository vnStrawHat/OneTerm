# Work: One concrete PTY session type replaces the macro

ID: US-0091
Intake: IN-0032
Created: 2026-09-14

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: maintenance
- Risk lane: normal
- Spec Intake, when required: `IN-0032`

## Outcome

The shared half of `TerminalSession` lives in one concrete type in `crates/terminal` instead of a
286-line macro that expands about 45 forwarding methods into `crates/local-shell` and
`crates/ssh`. Each backend owns one of these and implements `capabilities()` only.

With the macro gone, `model::ResizePolicy` has no namer left: both backends name it today **only
because the macro's signature forces them to**, so it and its two `From` impls are deleted and
each backend passes an `oneterm_vt::ResizePolicy` directly, which `TerminalModel::new` has
accepted since `US-0082`. The two gaps the migration recorded against itself — `US-0083`'s gap 1
and `US-0084`'s gap 2, both of which say the engine's enum "cannot be named here yet" — close with
it.

The four composed traits (`TerminalRender`, `TerminalInput`, `TerminalIme`, `TerminalLifecycle`)
**stay exactly as they are**. The macro is what this packet removes, not the partition: the audit
is explicit that the traits earn their keep and that `test_support.rs`'s `FakeTerminalSession`
implements them separately.

**No behaviour changes.** Every method the macro generates keeps its body; the bodies move from a
macro expansion into one struct's `impl` blocks.

## Scope

### In scope

- [x] **Delete `impl_pty_terminal_session!`** — `crates/terminal/src/session.rs:443-728`
  (286 lines) and its `#[macro_export]`.
- [x] **Add the concrete type it specifies** in `crates/terminal`. The macro's own doc
  (`session.rs:446-453`) already names the design: *"The local shell and SSH sessions differ only
  in their `TerminalCapabilities`, their `SessionKind`, their grow-resize `ResizePolicy` and how
  the channel is torn down."* So one `PtySession` holding the standard fields the macro requires
  today (`term`, `listener`, `state`, `marked_text`, `event_rx`), plus the transport,
  `SessionKind`, an `oneterm_vt::ResizePolicy` and a close hook. The four traits are implemented
  once, on it.
- [x] **Rewrite both backends onto it**: `crates/local-shell/src/session_terminal.rs` (42 lines)
  and `crates/ssh/src/session_terminal.rs` (55 lines) lose their macro invocation; each keeps its
  `capabilities()` impl and its close function (`shutdown_owner`, `close_channel`), and
  `crates/local-shell/src/session.rs` / `crates/ssh/src/session.rs` change to own a `PtySession`.
- [x] **Delete `model::ResizePolicy` and its two `From` impls** —
  `crates/terminal/src/model.rs:28-70`, 43 lines — and its re-export. `local_resize_policy()` in
  `crates/local-shell/src/session_terminal.rs` returns `oneterm_vt::ResizePolicy`
  (`KeepViewportTop` on Windows, `BottomAnchor` elsewhere), and `crates/ssh` passes
  `oneterm_vt::ResizePolicy::BottomAnchor`. The `DEC-0008` rationale comments on both stay; only
  the enum they name changes.
- [x] **Retire the two recorded gaps**: the comment block at
  `crates/local-shell/src/session_terminal.rs:14-18` ("Naming the engine's enum here needs an API
  `crates/terminal` does not offer yet; see the `US-0083` packet's gap 1") and at
  `crates/ssh/src/session_terminal.rs:21-24` ("The engine value cannot be named here yet"). Both
  become false the moment this lands and must go with the code they describe.
- [ ] **`TerminalPump::pending`** — **not taken.** Not a one-line fall-out; see Gaps. If and only if it falls out for free. It is a
  `Mutex<Vec<SessionEvent>>` (`crates/terminal/src/backend/pump.rs:69`) whose own comment says it
  exists because the backends hold the pump by `&` at the lifecycle call sites; the audit notes
  (§4 H4) that B1 would let it become a plain field. Take it only if the concrete type makes it a
  one-line change — otherwise leave it and say so in Gaps. It is not this outcome.
- [x] The `crates/terminal` manifest line the fork era left in both backend crates — **already gone**; neither backend manifest names the fork (checked before touching), if the macro's
  removal makes it dead (`migration.md` records it surviving "long after no backend source named
  it"). Verify before touching.

### Out of scope

- [ ] Changing the `TerminalSession` method surface: no method added, removed, renamed, or given a
  different signature. A consumer in `crates/terminal-view`, `crates/session-ui` or `crates/state`
  must not be able to tell this packet happened.
- [ ] The four composed traits and their partition. Keep the split.
- [ ] `FakeTerminalSession` in `crates/terminal/src/test_support.rs` — it implements the traits
  directly and never used the macro. If it needs an edit, the trait surface moved and this packet
  is out of scope.
- [ ] Either read loop. `crates/local-shell/src/event_loop.rs` and `crates/ssh/src/task.rs` are
  structurally different for good reasons (the local loop holds the engine guard across reads
  under `MAX_LOCKED_READ`; the SSH loop cannot) and are not unified here or anywhere.
- [ ] The byte-budget duplication — `US-0093`, deliberately after this packet, because it adds a
  type to `crates/terminal/src/backend/` and would otherwise collide.
- [ ] Merging `local-shell` or `ssh` into `terminal`. Option C is rejected; see the High-Level
  Design.
- [ ] `SessionFactory` (`crates/terminal/src/factory.rs`). One implementation, and it is the
  dependency inversion that makes R3 possible. The audit names it as explicitly not
  over-engineering.

## Acceptance

- [x] **The macro is gone.** `grep -rn "impl_pty_terminal_session" crates/ docs/` returns only
  historical references in `docs/spec-intakes/` and `docs/decisions/`, and none in `crates/`.
- [x] **`model::ResizePolicy` is gone.** `grep -rn "\bResizePolicy\b" crates/` names only
  `oneterm_vt::ResizePolicy` and its own definition in `crates/vt`.
- [x] **Line delta recorded and honest — FAILS the −80 band; ACCEPTED by the owner (2026-09-15, "Merge"): the coupling removal is the outcome, the band was the audit's under-count.** 383 lines are deleted (286 macro + 42 + 55) plus 43 for
  the enum. The audit estimates the replacement at about 230 lines and is explicit (§(d).2) that
  the net "could land anywhere between −80 and −200". Record the measured net. **A net worse than
  −80 fails this packet**: if the concrete type comes out bigger than that, the shape is wrong —
  most likely because ownership of `marked_text` / `event_rx` / `owner_join` did not move — and
  the packet stops and reports rather than shipping a wash.
- [x] **No test lost, no test assertion edited.** Baselines recorded at the branch point for
  `cargo test -p oneterm-terminal` (`backend_tests.rs` alone is 1 280 lines),
  `-p oneterm-local-shell` (1 395 test lines) and `-p oneterm-ssh` (2 807). Each after-count is
  greater than or equal to its baseline. A test body may move between files; an assertion that
  changes value is a behaviour change and fails this packet.
- [x] **The backend contract suite passes untouched**:
  `cargo test -p oneterm-core -p oneterm-terminal -p oneterm-local-shell -p oneterm-ssh`, the
  portable set CI runs on ubuntu, macos and windows.
- [x] **Both `ResizePolicy` behaviours are still pinned by a test.** Windows local grows must keep
  `KeepViewportTop` and SSH must keep `BottomAnchor` (`DEC-0008`). Name the tests that prove it; if
  none exists, add one — this is the one place where deleting the adapter enum could silently
  change behaviour, because both old variants convert and a wrong constant would compile.
- [x] **`cargo test -p oneterm-terminal` stops recompiling a macro that expands into two other
  crates.** Record the observation qualitatively; no stopwatch claim is required, and none should
  be made without `cargo build --timings`.
- [x] **`pwsh scripts/ci-local.ps1` exits 0**, with totals recorded.

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md` §9 — the section the macro's doc comment points at ("See
  `docs/terminal-backend.md` §9"). It describes the shared session implementation as a macro.
  **Must change.**
- `docs/terminal-backend.md` §6.2 and the `TerminalSession` description — the session lifecycle,
  spawn, and teardown. The surface does not move; who implements it does. Review and update
  wherever it names the macro.
- `IN-0029/low-level-design/migration.md` — records `impl_pty_terminal_session!` blocking
  `US-0083`'s and `US-0084`'s resize-policy change, and forcing the fork's manifest line to
  survive in both backend crates. This packet closes that record; it is history, so it is not
  rewritten, but this packet's evidence must reference it.
- `IN-0029/low-level-design/events-and-api.md` — the engine-side API the session forwards to.
  Unchanged by this packet; reviewed to confirm no forwarding method quietly changes what it calls.
- `docs/agents/crate-dependency-rules.md` — R8 ("backends implement traits only") and R10 ("new
  shared types go in the lowest crate that needs them"). `PtySession` lands in `crates/terminal`,
  which both backends already depend on, so both hold. R3 is untouched: no UI crate gains an edge.
- `docs/decisions/` — the `DEC-0008` record behind the two resize policies. The decision does not
  change; only the enum naming it does. Confirm the decision text does not name
  `oneterm_terminal::model::ResizePolicy` specifically; if it does, that sentence needs the
  engine's name instead.
- `crates/terminal/src/session.rs:443-456` — the macro's own doc comment, which is the
  specification this packet implements and then deletes.

### Documentation Action

Update required:

- `docs/terminal-backend.md` §9 — describe one concrete session type instead of a macro, and state
  what each backend still owns (`capabilities()`, its close function, its resize policy). This is
  the owning contract for the backend design and it would otherwise describe a macro that no
  longer exists.
- `docs/terminal-backend.md` wherever `ResizePolicy` is named — it must name the engine's enum.
- `docs/decisions/` `DEC-0008` — only if it names the adapter enum.

Reason: the behavioural contract does not change at all — `TerminalSession` keeps every method —
but `docs/terminal-backend.md` documents the *mechanism* by name, and the mechanism is exactly
what this packet replaces. Leaving §9 describing a deleted macro would repeat the failure
`US-0090` is cleaning up elsewhere in the same intake.

### Reconciliation

Before completion, list the docs changed, and confirm explicitly that `docs/PROJECT.md`
§ Important Boundaries ("Public contracts: `TerminalSession`, `SessionFactory` …") needed no edit
because the contract's shape is unchanged.

## Context

Why this is worth a packet rather than tolerating the macro: `migration.md` records it as the
single piece of coupling the `IN-0029` migration kept tripping over. It blocked `US-0083`'s and
`US-0084`'s resize-policy change, and both backends still carry an apology comment about it. The
two backend files say so in their own words today:

```text
crates/local-shell/src/session_terminal.rs:14  "Naming the engine's enum *here* needs an API
                                                crates/terminal does not offer yet; see the
                                                US-0083 packet's gap 1."
crates/ssh/src/session_terminal.rs:21          "The engine value cannot be named here yet —
                                                impl_pty_terminal_session! declares
                                                resize_policy() as returning the adapter enum;
                                                see US-0084's packet, gap 2."
```

The macro already contains the hedge that shows the enum is vestigial
(`session.rs` on `resize_policy()`): *"The macro argument may be either this crate's
`ResizePolicy` or the engine's `oneterm_vt::ResizePolicy`: both convert, so a backend can move to
the engine's name without an edit here."* The backends cannot take that offer because the
generated `resize_policy()` returns the adapter enum.

What genuinely differs between the two backends, and therefore stays with each:

| | `LocalSession` | `SshSession` |
| --- | --- | --- |
| `SessionKind` | `Local` | `Ssh` |
| Resize policy | `KeepViewportTop` on Windows, `BottomAnchor` elsewhere (`DEC-0008`: conhost keeps its viewport top on a grow) | `BottomAnchor` (`DEC-0008`: the remote PTY reflows on its side) |
| Close | `shutdown_owner` | `close_channel` — also closes SFTP, which shares the connection (`ARCH-28`) |
| `capabilities()` | `logging` only | `network_stats`, `sftp`, `cwd_source`, `logging` |

Everything else is the same code twice, once per expansion.

**Risk, stated plainly.** The audit rates B1 **medium** — the highest in this intake. It rewrites
both backends' whole `TerminalSession` surface. What makes it acceptable is the test mass already
in place: `backend_tests.rs` is 1 280 lines, `crates/local-shell` has 1 395 test lines and
`crates/ssh` has 2 807, and the portable backend contract suite runs on three platforms in CI.
The mitigation is procedural: move bodies, do not rewrite them, and let the test counts and the
unchanged assertions prove it.

**Do this after `US-0090`.** Not for correctness — for a settled target. `US-0090` narrows
`crates/terminal`'s public surface, including `SharedSessionState`'s 13 externally-unused methods
in `backend/state.rs`, which is exactly the surface the new concrete type reaches through. Doing
the visibility pass second would mean narrowing an API that had just been rewritten against its
wider form.

## Plan

- [x] Record branch-point test counts for `oneterm-terminal`, `oneterm-local-shell`,
  `oneterm-ssh`, and the line counts of the four files involved.
- [x] Write `PtySession` in `crates/terminal/src/session.rs` next to the still-present macro,
  moving each generated method body across unchanged. Compile with both present.
- [x] Move `crates/local-shell` onto it: the struct owns a `PtySession`, keeps `capabilities()`
  and `shutdown_owner`, and `local_resize_policy()` returns `oneterm_vt::ResizePolicy`. Run
  `cargo test -p oneterm-local-shell` against its baseline.
- [x] Move `crates/ssh` the same way. Run `cargo test -p oneterm-ssh`.
- [x] Delete the macro and `model::ResizePolicy` with its two `From` impls. Compile.
- [x] Confirm or add the two resize-policy behaviour tests.
- [x] Update `docs/terminal-backend.md` §9 and any `ResizePolicy` naming.
- [x] Measure the net line delta. **It is worse than −80** (production −19, total 0), so this packet stops and reports; see Evidence.
- [x] `cargo test --workspace`, then `pwsh scripts/ci-local.ps1`.

## Decisions

None expected. The target shape is specified by the macro's own doc comment and recorded in the
intake's High-Level Design § Data Flow. If the concrete type cannot take ownership of
`marked_text` / `event_rx` / `owner_join` and the net delta lands outside the accepted range, that
is a **finding to report**, not a decision to make unilaterally: bring it back to the owner with
the measured numbers.

## Verification Plan

Focused proof:

- `cargo test -p oneterm-terminal` — `backend_tests.rs` (1 280 lines) is the primary net.
- `cargo test -p oneterm-local-shell` and `cargo test -p oneterm-ssh` against their baselines.
- The two resize-policy tests named in Acceptance, run explicitly by name and their output
  recorded. This is the one silent-behaviour-change risk in the packet.

Integration:

- `cargo test -p oneterm-core -p oneterm-terminal -p oneterm-local-shell -p oneterm-ssh` — the
  portable backend contracts, the same set CI runs on three platforms.

Regression:

- `cargo clippy --workspace --all-targets -- -D warnings` — proves every consumer of
  `TerminalSession` across `terminal-view`, `session-ui`, `state` and `app` still compiles against
  an unchanged surface.
- `cargo test --workspace`.

Platform:

- `pwsh scripts/ci-local.ps1`, exit 0, totals recorded.

No E2E and no GUI walk. The session surface is unchanged by construction, and the backend contract
suite plus `-D warnings` across every consumer is stronger proof than a manual walk. If a manual
check ever seems necessary here, the surface moved and the packet is out of scope.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Branch point: `4bb088d`; `main` merged in at `684074a` (`US-0092` + `US-0094`).
Independent verification: `evidence/US-0091-verify.md`, written in the verifier's own worktree
and landed separately — **correctness PASS-WITH-NOTES**, every number reproduced, one MAJOR
coverage finding and six notes, all addressed below.

### FINDING — the line-delta gate fails, and it is unreachable

The measured net is **production −36 lines** (−19 before the post-verification slimming pass),
against the accepted band of **−80 to −200**. Per Acceptance ("a net worse than −80 fails this
packet") the work is complete, green and behaviourally identical, but it is **not marked
Implemented**: the numbers go to the owner, who decides whether to accept a decoupling that is a
line-count wash or to revert.

The verifier reached the same conclusion independently and measured the headroom: **≈ 25–30
lines available by tidying, landing at about −45 to −49**, still ~30 short. This packet then took
that tidying (see below) and landed at **−36**; the remaining 9 is `#[doc(hidden)]`, three
attribute lines the estimate had credited as removals. Either way the gate is out of reach.

Why, measured by the verifier over the 323 added lines (224 code, 51 doc, 48 blank):

| block | lines | verdict |
| --- | ---: | --- |
| the five `impl` blocks — 45 forwarding methods | 195 (155 code) | **irreducible**: 3.4 lines per method is what `rustfmt` costs |
| `PtyOwner` + `PtySession` + the inherent `impl` | 121 | the trait, the struct, the constructor, the helpers, the rustdoc |

The 45 forwarding methods exist whether they are written once or expanded twice; the audit's
"about 230 lines" for the replacement under-counted them by about 90. The diagnosis Acceptance
offers — that ownership of `marked_text` / `event_rx` / `owner_join` did not move — does **not**
apply: `marked_text` and `event_rx` moved into `PtySession` exactly as specified, and
`owner_join` is correctly the local backend's own (its `Drop` needs it). The verifier considered
and rejected four other shapes (moving `kind`/`resize_policy` onto `PtyOwner`: workspace net −2
and DEC-0008 scattered over three files; a builder or public fields: both longer;
`Deref<Target = TerminalModel>`: impossible, `model()` returns by value; folding `report()` back
into `report_generated_input`: `report()` *saves* lines). The only thing that reaches −80 is
collapsing the four traits, which the audit and this packet both put out of scope.

What the line count does not show: `#[macro_export]` is gone from `crates/terminal/src`, so
neither backend crate compiles 286 lines of another crate's source; the shared body is compiled
once; the `US-0083` / `US-0084` gaps are closed; and `crates/terminal` now has its first direct
test of the forwarding it owns (below).

### Defects from the verification, and what was done

| id | severity | finding | resolution |
| --- | --- | --- | --- |
| D1 | MAJOR | Deleting `self.owner.pty_resize(rows, cols)?` from `PtySession::resize` left **all 372 tests green** — the local shell would stop telling ConPTY the new size and SSH would stop sending `window_change`. Pre-existing (the macro had the same hole) but this packet's whole mitigation is "move bodies, let the tests prove it", and for this path they proved nothing. | **Fixed.** `FakeOwner` in `session.rs`'s `mod tests` plus six tests. Re-tampered: with the forward deleted, `resize_tells_the_owner_and_then_grows_the_grid` **fails**; restored and re-verified. |
| D2 | MINOR | `crates/terminal` gained 195 lines of forwarding with no test of its own; the proof lived entirely in the two backend crates. | **Fixed** by the same six tests: `cargo test -p oneterm-terminal` 271 → 277 on this branch. |
| N1 | note | `owner` is the last field, so `event_rx` now drops *before* the backend's `Drop` body, where it used to drop after. | **Recorded, not changed.** Benign: a closed event channel is counted, not fatal (`backend_tests.rs`, `a_closed_channel_is_counted_not_panicked`), and in production the UI has already taken the receiver through `take_events()`, so `None` is what drops. Moving `owner` first would change nothing observable and would put the least interesting field at the top. |
| N2 | note | Four of the five new `pub` items exist only for tests in the backend crates. | **Partly fixed.** `state()` deleted — its one caller is an SSH test whose helper already holds the clone. `owner()` / `term()` / `resize_policy()` are `#[doc(hidden)]`: the backend crates' tests need them across a crate boundary, so `pub(crate)` is not available. Proxy 272 → **271**. |
| N3 | note | `LocalSession::config()` and its field are dead workspace-wide. | **Fixed**, −6 lines. Dead at `4bb088d` too, but this packet had the file open and `IN-0032` exists to remove exactly this. |
| N4 | note | `migration.md:321` reads as an open plan assigning the macro's removal to `US-0085`. | **Fixed**: a closing note records that `US-0091` deleted the macro instead, and that the fork-era manifest lines were already gone. |
| N5 | note | `terminal-backend.md` §6.2's historical sketch got one notch staler. | **Fixed**: the sketch's preamble now says `spawn` returns `PtySession<LocalSession>` and that the shell config is not retained. |
| N6 | note | Stranded line wrap at `terminal-backend.md` §5.3. | **Fixed**, re-flowed. |

### Measurements

| | before (`4bb088d`) | after (this branch) |
| --- | --- | --- |
| `cargo test -p oneterm-terminal` | 271 passed, 0 ignored | **285 passed, 0 ignored** (+6 here, +8 from the merged `US-0092`) |
| `cargo test -p oneterm-local-shell` | 33 passed, 2 ignored | **33 passed, 2 ignored** |
| `cargo test -p oneterm-ssh` | 67 passed, 0 ignored | **68 passed, 0 ignored** (+1, the SSH policy test) |
| line count, the nine touched files, production only | 2062 | **2026 (−36)** |
| line count, the nine touched files, including tests | 3112 | **3201 (+89)** — the new unit tests |
| `pub` proxy count, `crates/terminal` | 267 | **271 (+4)** |

Line counts are non-blank lines; "production only" cuts each file at its `#[cfg(test)]` and
excludes the `_tests.rs` files. The `pub` proxy is `US-0090`'s own command. The nine files are
identical at `4bb088d` and at `684074a`, so both baselines give the same numbers.

`pwsh scripts/ci-local.ps1` (`CARGO_BUILD_JOBS=4`): **all checks passed** — 60 test sections,
**1971 passed, 0 failed, 14 ignored**, against `main`'s 60 / 1964 / 0 / 14. The +7 is exactly
this branch's seven new tests.

### Merging `main`

One conflict, `IN-0032.md`, docs only, resolved by keeping both sides (`US-0092`'s ticked
checkbox and this packet's unticked-with-reason note), as the verification predicted.

One **semantic** conflict the merge could not see, caught by `-D warnings`: `US-0092` added three
`oneterm_terminal::ResizePolicy::Default.into()` call sites in
`crates/terminal-view/src/render/plan_cache.rs` tests. With the adapter enum deleted that name
resolves to `oneterm_vt::ResizePolicy`, whose equivalent variant is `BottomAnchor`; the `.into()`
is now an identity conversion and goes with it. **Value-preserving** — the deleted
`From<ResizePolicy>` mapped `Default → BottomAnchor` — and it is the same rename this packet
already applied in `model_tests.rs` and `local_session_grow_policy_matches_conpty`.

### The resize-policy tests, run by name

```text
test session::session_tests::local_session_grow_policy_matches_conpty ... ok
test session::tests::ssh_session_grow_policy_is_bottom_anchored ... ok
test session::tests::ssh_grow_resize_pulls_scrollback_into_the_viewport_top ... ok
```

`local_session_grow_policy_matches_conpty` is pre-existing; only the enum it names changed
(`::Default` is now `::BottomAnchor`, the same value — verified against the deleted `From` in the
diff, not from memory). `ssh_session_grow_policy_is_bottom_anchored` is **new**: with the policy
no longer a single macro argument it is named in two places (`connect` and the test helper), so
`SSH_RESIZE_POLICY` holds it and this test pins the constant.

### The new `crates/terminal` unit tests (D1, D2)

`FakeOwner` records what the session forwards. Six tests: the resize hop reaches the owner as
`(rows, cols)` exactly once and *then* grows the grid, and a no-op resize asks it nothing; a
write reaches the owner while alive and is refused with `Closed` once shut; `close()` runs the
owner's teardown **before** liveness flips; capabilities, kind and policy read back; the IME
marked text round-trips and `commit_text` writes to the owner; the event receiver is handed out
once. The macro could not be tested this way — exercising it needed a real backend — which is
why the hole survived.

### Gaps

- **`TerminalPump::pending` was left alone.** Not a fall-out of this packet: the `Mutex` is there
  because the two read loops hold the pump by `&` at their lifecycle call sites. A read-loop
  change, not a session change.
- **`crates/terminal`'s public surface is +4** (`PtyOwner`, `PtySession`, `new`, and the three
  `#[doc(hidden)]` test accessors, against the deleted `ResizePolicy` enum and its re-export):
  267 → 271, one over `US-0090`'s "at most 270". A concrete type has a public API where a macro
  had none.
- **`PtyOwner` restates `pty_write` / `pty_resize`.** `PtyTransport: Clone` is not object safe
  and both backends' transport types are crate-private, so `PtySession` cannot be generic over
  the transport without leaking a private type through a public signature. Making
  `LocalTransport` / `SshTransport` public would widen two backend crates' API to save about 26
  lines — the wrong trade for this intake.
- **No generated method body was rewritten.** Every one moved across unchanged except the five
  generated-input reports, where `concat!($label, " mouse input")` became
  `self.report("mouse input", ...)`; the verifier confirmed the logged text is byte-identical and
  the mouse path still allocates nothing per event.
- **`DefaultColors`'s root re-export is deleted**, which is `US-0090`'s deferred item. The type
  stays at `oneterm_terminal::backend::DefaultColors`, because the public
  `SharedState::set_default_colors` takes it.
- **The fork-era manifest line was already gone** from both backend manifests.
- **No E2E and no GUI walk**, as the Verification Plan states.

### Harness story row

`harness.db` is not edited from this worktree. The row update, against the real schema:

```python
import sqlite3

db = sqlite3.connect("harness.db")
db.execute(
    "UPDATE story SET status = ?, unit_proof = ?, integration_proof = ?, e2e_proof = ?, "
    "platform_proof = ?, evidence = ?, last_verified_at = ?, last_verified_result = ? "
    "WHERE id = ?",
    (
        "in_progress",
        1,
        1,
        0,
        1,
        "ci-local.ps1 green (60 sections, 1971 passed, 0 failed, 14 ignored); "
        "terminal 271->285, local-shell 33=33, ssh 67->68; independent verification "
        "PASS-WITH-NOTES, D1/D2 fixed with six new unit tests; line delta production "
        "-36 - fails the -80 gate, which the verifier confirms is unreachable; "
        "returned to the owner",
        "2026-09-14",
        "fail",
        "US-0091",
    ),
)
db.commit()
```

## Reconciliation

Docs changed by this packet:

| document | change |
| --- | --- |
| `docs/terminal-backend.md` section 3 (crate table) | the `local-shell` / `ssh` rows now say each implements `PtyOwner` and returns a `PtySession` |
| `docs/terminal-backend.md` section 5.3 (resize policy) | names `local_resize_policy()` and `SSH_RESIZE_POLICY`; records that the adapter enum is deleted and both backends name `oneterm_vt::ResizePolicy`; re-flowed (verification note `N6`) |
| `docs/terminal-backend.md` section 6.2 (the historical spawn sketch) | says what the shipped `spawn` now returns, and that the shell config is no longer retained (verification note `N5`) |
| `docs/terminal-backend.md` section 9 (the `TerminalSession` blockquote) | describes `PtySession<O: PtyOwner>` and what each backend still owns, instead of a macro |
| `docs/terminal-backend.md` section 11 (file layout) | the three `session.rs` / `session_terminal.rs` entries |
| `docs/spec-intakes/IN-0029-vt-engine/low-level-design/migration.md` | a closing note on the resize-policy paragraph, which read as an open plan assigned to `US-0085` (verification note `N4`) |
| `docs/spec-intakes/IN-0032-terminal-crate-tidy/IN-0032.md` | the `US-0091` line: built, independently verified, left unticked pending the owner's ruling on the gate |
| `docs/spec-intakes/IN-0032-terminal-crate-tidy/US-0091-concrete-pty-session.md` | this packet |
| `docs/spec-intakes/IN-0032-terminal-crate-tidy/evidence/US-0091-verify.md` | the independent verification — written in the verifier's worktree and landed from there, not by this branch |

Reviewed, no change needed:

- `docs/PROJECT.md` Important Boundaries ("Public contracts: `TerminalSession`,
  `SessionFactory` ...") — **confirmed no edit needed**: the contract's shape is unchanged.
  Not one method was added, removed, renamed or re-signed; only who implements it moved. The
  verifier re-checked this independently.
- `IN-0029/low-level-design/migration.md` — the history is not rewritten; only the one paragraph
  that read as an **open plan** now carries a closing note, so the next reader does not chase
  `US-0085` for work that `US-0091` did.
- `IN-0029/low-level-design/events-and-api.md` — reviewed; no forwarding method changed what it
  calls on the engine.
- `docs/decisions/` `DEC-0008` — reviewed; it does not name
  `oneterm_terminal::model::ResizePolicy`, so nothing to re-word (grepped, and re-grepped by the
  verifier).
- `docs/agents/crate-dependency-rules.md` — R8 and R10 hold: `PtySession` and `PtyOwner` land in
  `crates/terminal`, which both backends already depend on; no new crate edge, no new
  dependency, R3 untouched. `python scripts/verify-dependency-graph.py` passes.

## Handoff

`US-0093` starts from this packet's head: its `ByteBudget` lands in
`crates/terminal/src/backend/`, the layer this packet rewrites. `US-0092` is independent of both
and shares no file with either.

### Owner ruling (2026-09-15)

The owner accepted the line-delta wash and ordered the merge: production −36, total +89 with the seven new tests, `pub` proxy 271. The independent verifier had shown the −80 band unreachable by tidying (the audit under-counted the irreducible forwarding by ~90 lines) and the coupling between the backends and `crates/terminal` is gone, which was the audit's reason for B1.
