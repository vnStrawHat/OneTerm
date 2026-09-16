# Work: the `pty` rustdoc describes Windows-only behaviour as if it held on both platforms

ID: BUG-0065
Intake: IN-0038
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

- Change type: bug (public documentation accuracy; no code change)
- Risk lane: normal
- Spec Intake, when required: IN-0038 (`crates/pty` became `crates/vt/src/pty`, `US-0104`)

## Outcome

Every sentence in the `pty` module's public documentation is true on the platform it
names, and the three texts that `cargo doc` renders into the same page set -- the module
rustdoc, guide chapter 3 and guide chapter 13 -- agree with each other and with the code.

The defects were found by the independent verifier of `BUG-0063`
(`evidence/BUG-0063-verify.md`, D1-D3). All are pre-existing and documentation-only:

- **D1 (MEDIUM)** `crates/vt/src/pty/mod.rs:41-43`: *"all are joined on drop"*. False on
  both platforms, and contradicted by `crates/vt/docs/guide/13-pty.md:158` (*"None of
  them is joined"*), by `crates/vt/docs/guide/03-threading.md:130` and by
  `crates/vt/src/pty/windows/pipe.rs:357`. An embedder builds shutdown ordering on this.
- **D2 (LOW)** `crates/vt/src/pty/mod.rs:17-20` and `crates/vt/docs/guide/13-pty.md:172-175`:
  the Windows drop contract (bounded grace, then `TerminateProcess`) stated as if it were
  both platforms. The Unix `PseudoConsole` has no `Drop` impl at all. Third instance: the
  `EventedPty` rustdoc (`mod.rs:210`) and `13-pty.md:133` claim Unix uses `SIGCHLD`, which
  `unix.rs:3-8` explicitly rejects.
- **D3 (LOW)** `crates/vt/src/pty/mod.rs:112-116`: the stated reason `Options` is not
  comparable (*"a hand-written one would compare padding"*) is imprecise -- a
  `sigismember` walk compares no padding. The decisive reasons are that `libc::sigset_t`
  has no `PartialEq` without libc's `extra_traits`, that a hand-written walk needs a
  per-target signal-count bound libc does not export, and that nothing needs the
  comparison.

## Scope

- [x] In scope: the `pty` module rustdoc (`crates/vt/src/pty/mod.rs`), guide chapter 13
      (`crates/vt/docs/guide/13-pty.md`), the one clause in `crates/vt/CHANGELOG.md` that
      repeats D3's imprecise reason, the owning low-level design
      (`low-level-design/pty.md`), which carries the same three false claims, and -- added
      in the acceptance rework -- the `unix.rs` block comment those sentences were derived
      from and the unlabelled headline of `docs/terminal-backend.md` §6.2.
- [x] Out of scope: any code, signature, field order or behaviour change. Guide chapter 3
      (already correct -- see Documentation). No new
      `CHANGELOG` entry: nothing an embedder compiles against changed, which is that
      file's own stated bar for an entry.

## The per-platform truth, read from the code

`PseudoConsole` is a different type on each platform and neither type has a `Drop` impl of
its own (`grep -n "impl Drop" crates/vt/src/pty/unix.rs crates/vt/src/pty/windows.rs`
returns nothing). On Windows the behaviour comes from the drop of its fields, in
declaration order: `Conpty`, `PipeReader`, `PipeWriter`, `ChildExitWatcher`.

| | Windows (ConPTY) | Unix (`openpty`) |
| --- | --- | --- |
| What drop does | `Conpty::drop` -> `ClosePseudoConsole` (`conpty.rs:198-206`), which blocks until conout is drained; then `ChildExitWatcher::drop` (`child.rs:218-229`) waits `CHILD_EXIT_GRACE` = 2 s on the child handle | closes `master`, `exit_signal` and the event receiver. Nothing else; there is no `Drop` impl (`unix.rs:222-229` states this as the invariant) |
| Child fate | asked to exit by the host; if it is still running after the 2 s grace, `TerminateProcess(handle, 1)` -- only this process's own child, only through its own handle, logged at `warn` first (`child.rs:178-215`, `DEC-0016`) | the parent holds no slave descriptor after the spawn -- the `slave` opened at `unix.rs:93` is never stored in `Self`, so it drops at the end of `spawn`, and the child closes its own copies at `:144` -- so closing `master` is the last descriptor to go: it hangs up the slave, which is the controlling terminal the child took with `setsid` + `TIOCSCTTY` (`unix.rs:131, 141`), and the child, as session leader, receives `SIGHUP`. This crate never signals the child: the reaper's `wait()` has already reaped the pid, so a later `kill(pid)` could hit whatever the kernel handed that pid to next |
| Does drop block? | **yes**, up to the 2 s grace (plus the conout drain). Belongs on an owner thread, not a UI thread | **no**. Nothing waits |
| Thread fate on drop | neither pipe thread is joined; the `JoinHandle` is dropped at spawn (`pipe.rs:363-368`). The conout reader is parked in a blocking `ReadFile` and returns when the pipe breaks; the conin writer waits on its ring condvar (`pipe.rs:297-304`) and returns once `PipeWriter::drop` closes it (`pipe.rs:348-352`) | the reaper is not joined either (`unix.rs:219`, `.map(drop)`); it deliberately outlives the drop, blocked in `child.wait()`, and returns when the child exits |
| Exit-event delivery | `RegisterWaitForSingleObject` thread-pool callback -> `mpsc` + an IOCP completion packet posted to the embedder's poller (`child.rs:1-22, 86-98`) | one thread per session blocked in `Child::wait` -> `mpsc` + one byte on a `UnixStream` the poller already watches (`unix.rs:1-8, 200-218`). **No** process-global `SIGCHLD` handler: a library crate that installs one fights every other runtime in the process |

Neither `DEC-0016` nor the low-level design ever asked Unix to terminate its child:
`DEC-0016` is written entirely in ConPTY terms (`ClosePseudoConsole`, `CreateProcessW`,
`conhost.exe`) and names no other platform. The code is right and the prose is wrong, so
this stays a documentation packet.

## Acceptance

- [x] No sentence in the `pty` rustdoc or in guide chapter 13 states a platform-specific
      drop, thread or child-exit fact without naming its platform.
- [x] The module rustdoc, guide chapter 3 and guide chapter 13 give the same answer to
      "are this crate's threads gone when `drop` returns?" -- no.
- [x] The `EventedPty` rustdoc and guide chapter 13 no longer attribute `SIGCHLD` to the
      Unix implementation.
- [x] The `Options` rustdoc gives the decisive reason it is not comparable.
- [x] The per-platform facts are stated once, in the module's `Platforms` section, and
      cross-linked from the paragraphs that used to repeat them.
- [x] No code, signature or behaviour change: `scripts/vt-public-api.py --check` is
      unchanged.
- [x] No `US-`/`BUG-`/`DEC-`/`IN-` citation and no bare `crates/` or `docs/` path is added
      to the crate's rustdoc (the package gate greps for both).

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0038-embeddable-vt-core/low-level-design/pty.md` -- the owning
  low-level design. Its "Two threads exist inside the transport" and "Drop blocks" bullets
  (lines 417-427) are the source the rustdoc was written from and carry all three false
  claims: *"both are joined on drop"*, Unix *"turns `SIGCHLD` into a pollable event"*, and
  a platform-neutral *"Dropping a `PseudoConsole` ... waits up to `CHILD_EXIT_GRACE`"*.
  **Updated.**
- `docs/decisions/DEC-0016-terminate-a-shell-that-outlives-its-pseudo-console.md` --
  the grace period and the escalation. Reviewed, **no change**: it is stated in ConPTY
  terms throughout and never claims a Unix half, so the decision is already correct and it
  is the rustdoc that over-generalised it.
- `docs/terminal-backend.md` §6.2-6.3 -- OneTerm's own use of the transport. **Updated**
  (acceptance rework, verifier defect D4): §6.3 is correctly labelled "Windows-specific"
  and §6.2's body defers to it by reference, but §6.2's headline -- "closing a local
  session is guaranteed to leave no process behind" -- carried no platform label while
  resting entirely on the Windows escalation. It is the same class of over-generalisation
  this packet exists to remove, so it is labelled rather than argued away.
- `crates/vt/docs/guide/03-threading.md:126-131` -- reviewed, **no change**: already says
  the transport threads are "none of which is joined on drop" and points at chapter 13.
  It was one of the texts contradicting the module rustdoc, and it was the correct one.
- `crates/vt/CHANGELOG.md` "Removed" -- **updated**: one clause repeated D3's imprecise
  "padding bytes" reason. No new entry added; nothing an embedder compiles against
  changed.

### Documentation Action

Update required, and the change *is* the documentation: the module rustdoc and guide
chapter 13 are the deliverable, and the low-level design they were written from is fixed
in the same commit so the next writer does not reintroduce the claims.

Reason: the code is correct on both platforms and is not touched.

### Reconciliation

Docs changed: `crates/vt/src/pty/mod.rs` (module rustdoc, `Options`, `EventedPty`),
`crates/vt/src/pty/unix.rs` (the block comment the wrong sentence was derived from),
`docs/terminal-backend.md` §6.2,
`crates/vt/docs/guide/13-pty.md`, `crates/vt/CHANGELOG.md`,
`docs/spec-intakes/IN-0038-embeddable-vt-core/low-level-design/pty.md`.
`docs/terminal-backend.md` moved from no-change to changed in the rework; the no-change
reasons for `DEC-0016` and guide chapter 3 remain valid.

## Plan

- [x] Establish the per-platform truth from the code, not from the prose.
- [x] State each platform-specific fact once in the module's `Platforms` section; make the
      paragraphs that used to repeat it point there instead.
- [x] Fix guide chapter 13's drop paragraph and its `SIGCHLD` sentence.
- [x] Fix the `Options` and `EventedPty` rustdoc.
- [x] Fix the low-level design and the `CHANGELOG` clause.
- [x] Run the full gate; confirm the public API surface is byte-identical.

## Decisions

None. `DEC-0016` already scopes the escalation to Windows; nothing new is inherited.

## Verification Plan

Documentation-only, so the proof is that the docs build clean, that the doctests in the
changed guide chapter still run, and that nothing in the public surface moved.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

Integration and E2E proof are marked not applicable: no code path changed, so there is no
behaviour to integrate or drive end to end. Platform proof is
`scripts/vt-public-api.py --diff-platforms`, which re-derives the Unix surface on a
Windows host and is the only per-platform check this packet can run here.

## Evidence and Gaps

Windows 11, `CARGO_BUILD_JOBS=3`, in an isolated worktree off `main` at `f315bf5e`.

Focused checks, run first:

```text
$ RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps --all-features
    Finished `dev` profile ... Generated target/doc/oneterm_vt/index.html

$ cargo test -p oneterm-vt --doc
44 passed (1 suite, 0.40s)

$ python scripts/vt-public-api.py --check --no-doc
public API surface unchanged (public-api.windows.txt)

$ python scripts/check-english.py
English contributor-text check passed for 918 files.

$ cargo package -p oneterm-vt --allow-dirty --list | python scripts/verify-dependency-graph.py --package-list -
Dependency graph policy passed for 20 workspace packages and 20 explicit members ...
Package set passed: the oneterm-vt package carries CHANGELOG.md, LICENSE, NOTICE,
README.md, examples/headless.rs, and reaches nothing outside crates/vt.
```

Full gate, on the final tree:

```text
$ pwsh scripts/ci-local.ps1
... 25 steps, 131 test sections, 4533 passed, 0 failed
==> python scripts/vt-public-api.py --check --no-doc
    public API surface unchanged (public-api.windows.txt)
==> python scripts/vt-public-api.py --diff-platforms          OK
==> rustdoc self-containment (crates/vt/src)                  OK
==> rustdoc self-containment (crates/vt/docs/guide)           OK
==> python scripts/check-english.py
    English contributor-text check passed for 918 files.
ci-local: all checks passed.
```

The two self-containment steps are the ones that matter most here: they are the greps
that fail on a `US-`/`BUG-`/`DEC-`/`IN-` citation or a bare `crates/` or `docs/` path in
the crate's rustdoc and in the guide, and both halves of this change are rustdoc.

### Acceptance rework, after the independent verification

`evidence/BUG-0065-verify.md` returned PASS-WITH-NOTES on the first commit. Reworked here
rather than opened as a new bug, because the defects are in this packet's own unaccepted
output:

- **D1 (MEDIUM), the Unix hang-up was mis-attributed.** The replacement sentence said the
  *master* is the controlling terminal and that the *line discipline* signals the
  *foreground process group*. The slave is the controlling terminal (`setsid` +
  `TIOCSCTTY`, `unix.rs:131, 141`); the hang-up is the tty layer's when the last master
  descriptor closes; and the signal the crate can defend is `SIGHUP` to the session
  leader, which is the child. All three copies (module rustdoc, guide chapter 13, the
  low-level design) now read the same, and so does the block comment in `unix.rs:222-229`
  that the wrong sentence was derived from -- left unfixed it would have been re-derived.
- **D2 (MEDIUM), risk lane.** `standard` is not in the repository's vocabulary and the DB
  `CHECK` rejects it. Now `normal`, here and in the harness snippet.
- **D3 (LOW), the Windows pipe threads do not end the same way.** Only the conout reader
  returns when the pipe breaks; the conin writer waits on its ring condvar and returns when
  `PipeWriter::drop` closes it. Split in the module rustdoc, in guide chapter 13's thread
  list and in the truth table above.
- **D4 (LOW), `docs/terminal-backend.md` §6.2.** Labelled Windows -- see the
  documentation review above.
- **D6 (LOW), citations.** The no-`Drop` invariant is `unix.rs:222-229` and the reaper's
  `.map(drop)` is `unix.rs:219`.
- **D5**: no action, `intake_id` 43 is correct.

Second gate run, after the rework:

```text
$ RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps                 clean
$ RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps --all-features  clean
$ cargo test -p oneterm-vt --doc                                 44 passed, 0 failed
$ python scripts/vt-public-api.py --check --no-doc    public API surface unchanged
$ rustdoc self-containment greps (crates/vt/src, crates/vt/docs/guide)         clean
$ python scripts/check-english.py           passed for 918 files
```

Gaps:

1. **Windows host only.** Nothing here compiled or ran a line of `cfg(unix)` code, so the
   Unix half of the truth table above was established by reading `unix.rs` and by the
   absence of a `Drop` impl, not by running a Unix build. That is sufficient for a
   documentation packet -- and `--diff-platforms` does re-derive the Unix public surface
   on this host, which is why the platform proof is checked -- but it is not a Linux
   build.
2. **No behaviour was verified**, because none was changed. The claim this packet makes is
   about what the prose says, not about what the transport does.

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
            "BUG-0065",
            "The pty rustdoc describes Windows-only behaviour as if it held on both platforms",
            "2026-09-16",
            "normal",
            "docs/spec-intakes/IN-0038-embeddable-vt-core/low-level-design/pty.md",
            "docs/spec-intakes/IN-0038-embeddable-vt-core/BUG-0065-pty-rustdoc-describes-windows-as-both-platforms.md",
            "implemented",
            1, 0, 0, 1,
            "pwsh scripts/ci-local.ps1 passed on Windows (25 steps, 131 test sections, 4533 passed, 0 failed), including both rustdoc self-containment greps, cargo test -p oneterm-vt --doc (44 passed) and vt-public-api.py --check (surface unchanged). Documentation only; no Unix build ran. After the verifier's PASS-WITH-NOTES the MEDIUM/LOW defects were reworked and the doc, doctest, public-API, self-containment and English checks were re-run clean on the reworked tree.",
            "pwsh scripts/ci-local.ps1",
            "2026-09-16",
            "pass",
            "Closes D1-D3 from evidence/BUG-0063-verify.md, then D1-D4 and D6 from evidence/BUG-0065-verify.md as acceptance rework. No code changed: the Unix PseudoConsole has no Drop impl by design and DEC-0016 is Windows-only, so the prose was wrong and the code was right. Also corrected the same three claims in the owning low-level design and one clause in crates/vt/CHANGELOG.md.",
            43,
        ),
    )
```

## Handoff

None required.
