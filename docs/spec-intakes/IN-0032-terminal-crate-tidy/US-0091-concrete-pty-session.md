# Work: One concrete PTY session type replaces the macro

ID: US-0091
Intake: IN-0032
Created: 2026-09-14

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
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

- [ ] **Delete `impl_pty_terminal_session!`** — `crates/terminal/src/session.rs:443-728`
  (286 lines) and its `#[macro_export]`.
- [ ] **Add the concrete type it specifies** in `crates/terminal`. The macro's own doc
  (`session.rs:446-453`) already names the design: *"The local shell and SSH sessions differ only
  in their `TerminalCapabilities`, their `SessionKind`, their grow-resize `ResizePolicy` and how
  the channel is torn down."* So one `PtySession` holding the standard fields the macro requires
  today (`term`, `listener`, `state`, `marked_text`, `event_rx`), plus the transport,
  `SessionKind`, an `oneterm_vt::ResizePolicy` and a close hook. The four traits are implemented
  once, on it.
- [ ] **Rewrite both backends onto it**: `crates/local-shell/src/session_terminal.rs` (42 lines)
  and `crates/ssh/src/session_terminal.rs` (55 lines) lose their macro invocation; each keeps its
  `capabilities()` impl and its close function (`shutdown_owner`, `close_channel`), and
  `crates/local-shell/src/session.rs` / `crates/ssh/src/session.rs` change to own a `PtySession`.
- [ ] **Delete `model::ResizePolicy` and its two `From` impls** —
  `crates/terminal/src/model.rs:28-70`, 43 lines — and its re-export. `local_resize_policy()` in
  `crates/local-shell/src/session_terminal.rs` returns `oneterm_vt::ResizePolicy`
  (`KeepViewportTop` on Windows, `BottomAnchor` elsewhere), and `crates/ssh` passes
  `oneterm_vt::ResizePolicy::BottomAnchor`. The `DEC-0008` rationale comments on both stay; only
  the enum they name changes.
- [ ] **Retire the two recorded gaps**: the comment block at
  `crates/local-shell/src/session_terminal.rs:14-18` ("Naming the engine's enum here needs an API
  `crates/terminal` does not offer yet; see the `US-0083` packet's gap 1") and at
  `crates/ssh/src/session_terminal.rs:21-24` ("The engine value cannot be named here yet"). Both
  become false the moment this lands and must go with the code they describe.
- [ ] **`TerminalPump::pending`**, if and only if it falls out for free. It is a
  `Mutex<Vec<SessionEvent>>` (`crates/terminal/src/backend/pump.rs:69`) whose own comment says it
  exists because the backends hold the pump by `&` at the lifecycle call sites; the audit notes
  (§4 H4) that B1 would let it become a plain field. Take it only if the concrete type makes it a
  one-line change — otherwise leave it and say so in Gaps. It is not this outcome.
- [ ] The `crates/terminal` manifest line the fork era left in both backend crates, if the macro's
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

- [ ] **The macro is gone.** `grep -rn "impl_pty_terminal_session" crates/ docs/` returns only
  historical references in `docs/spec-intakes/` and `docs/decisions/`, and none in `crates/`.
- [ ] **`model::ResizePolicy` is gone.** `grep -rn "\bResizePolicy\b" crates/` names only
  `oneterm_vt::ResizePolicy` and its own definition in `crates/vt`.
- [ ] **Line delta recorded and honest.** 383 lines are deleted (286 macro + 42 + 55) plus 43 for
  the enum. The audit estimates the replacement at about 230 lines and is explicit (§(d).2) that
  the net "could land anywhere between −80 and −200". Record the measured net. **A net worse than
  −80 fails this packet**: if the concrete type comes out bigger than that, the shape is wrong —
  most likely because ownership of `marked_text` / `event_rx` / `owner_join` did not move — and
  the packet stops and reports rather than shipping a wash.
- [ ] **No test lost, no test assertion edited.** Baselines recorded at the branch point for
  `cargo test -p oneterm-terminal` (`backend_tests.rs` alone is 1 280 lines),
  `-p oneterm-local-shell` (1 395 test lines) and `-p oneterm-ssh` (2 807). Each after-count is
  greater than or equal to its baseline. A test body may move between files; an assertion that
  changes value is a behaviour change and fails this packet.
- [ ] **The backend contract suite passes untouched**:
  `cargo test -p oneterm-core -p oneterm-terminal -p oneterm-local-shell -p oneterm-ssh`, the
  portable set CI runs on ubuntu, macos and windows.
- [ ] **Both `ResizePolicy` behaviours are still pinned by a test.** Windows local grows must keep
  `KeepViewportTop` and SSH must keep `BottomAnchor` (`DEC-0008`). Name the tests that prove it; if
  none exists, add one — this is the one place where deleting the adapter enum could silently
  change behaviour, because both old variants convert and a wrong constant would compile.
- [ ] **`cargo test -p oneterm-terminal` stops recompiling a macro that expands into two other
  crates.** Record the observation qualitatively; no stopwatch claim is required, and none should
  be made without `cargo build --timings`.
- [ ] **`pwsh scripts/ci-local.ps1` exits 0**, with totals recorded.

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

- [ ] Record branch-point test counts for `oneterm-terminal`, `oneterm-local-shell`,
  `oneterm-ssh`, and the line counts of the four files involved.
- [ ] Write `PtySession` in `crates/terminal/src/session.rs` next to the still-present macro,
  moving each generated method body across unchanged. Compile with both present.
- [ ] Move `crates/local-shell` onto it: the struct owns a `PtySession`, keeps `capabilities()`
  and `shutdown_owner`, and `local_resize_policy()` returns `oneterm_vt::ResizePolicy`. Run
  `cargo test -p oneterm-local-shell` against its baseline.
- [ ] Move `crates/ssh` the same way. Run `cargo test -p oneterm-ssh`.
- [ ] Delete the macro and `model::ResizePolicy` with its two `From` impls. Compile.
- [ ] Confirm or add the two resize-policy behaviour tests.
- [ ] Update `docs/terminal-backend.md` §9 and any `ResizePolicy` naming.
- [ ] Measure the net line delta. If it is worse than −80, stop and report instead of shipping.
- [ ] `cargo test --workspace`, then `pwsh scripts/ci-local.ps1`.

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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record: the branch point and commit; the measured net line delta against the
audit's −80 to −200 estimate, per file; the three test counts before and after; the two
resize-policy test names and their output; whether `TerminalPump::pending` became a plain field or
was left alone and why; whether the fork-era manifest line was dead and removed; and
`ci-local.ps1`'s totals. Record any generated method whose body could not move across unchanged,
with the reason.

## Handoff

`US-0093` starts from this packet's head: its `ByteBudget` lands in
`crates/terminal/src/backend/`, the layer this packet rewrites. `US-0092` is independent of both
and shares no file with either.
