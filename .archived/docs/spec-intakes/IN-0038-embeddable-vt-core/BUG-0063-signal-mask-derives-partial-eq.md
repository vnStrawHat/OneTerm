# Work: `SignalMask` derives `PartialEq` on a `libc::sigset_t` that has neither

ID: BUG-0063
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

- Change type: bug
- Risk lane: standard (a public trait impl is removed; see Documentation)
- Spec Intake, when required: IN-0038 (`crates/pty` became `crates/vt/src/pty`, `US-0104`)

## Outcome

`crates/vt/src/pty/unix.rs` compiles on Linux. The `pty` module is `cfg(unix)` code that
has never been compiled on the owner's Windows machine -- only the `msvc` target is
installed -- so the defect reached `main` and only the GitHub Actions ubuntu job sees it.

The reported failure, from the "Full workspace quality gate" job (ubuntu-latest), package
step for `oneterm-vt`:

```text
error[E0369]: binary operation `==` cannot be applied to type `sigset_t`
  --> crates/vt/src/pty/unix.rs:35
error[E0277]: the trait bound `sigset_t: Eq` is not satisfied
  --> crates/vt/src/pty/unix.rs:35
```

`#[derive(Clone, Copy, PartialEq, Eq)] pub struct SignalMask(libc::sigset_t);` asks the
compiler for a comparison the inner type cannot provide. On macOS `sigset_t` is a type
alias for `u32`, which *does* compare, which is why the derive was plausible when it was
written and why only the Linux job fails: with libc 0.2 and no `extra_traits` feature,
the glibc `sigset_t` is a struct (`{ __val: [c_ulong; 16] }`) with no `PartialEq` and no
`Eq`.

## Scope

- [x] In scope: drop the two derives from `SignalMask`; drop the same two from
      `pty::Options`, which contains `Option<SignalMask>` under `cfg(unix)` and therefore
      cannot keep them either; correct the now-stale justification comment inside
      `SignalMask::current`; a `CHANGELOG` entry; a read-through of the whole
      `cfg(unix)` half for further never-compiled hazards.
- [x] Out of scope: any change to what the mask *does*, to `PseudoConsole`, or to the
      Windows half. No new trait impl is written by hand: nothing in this repository
      compares a mask or an `Options`, so an unused comparison that breaks a platform
      build is weight, not API.

## Acceptance

- [x] `SignalMask` no longer derives `PartialEq` or `Eq`.
- [x] `pty::Options` no longer derives `PartialEq` or `Eq` (it cannot: the derive expands
      to a comparison of its `Option<SignalMask>` field on Unix).
- [x] `grep -rn` across `crates/` finds no comparison of a `SignalMask` or of a
      `pty::Options`, so nothing in the workspace breaks.
- [x] `crates/vt/public-api.unix.txt` and `crates/vt/public-api.windows.txt` are
      unchanged: neither snapshot records trait impls.
- [x] The zeroing in `SignalMask::current` stays, with a justification that is true
      without `PartialEq`.
- [x] `crates/vt/CHANGELOG.md` `[Unreleased]` records the removal, because a removal is a
      minor bump under promise 1 of that file.
- [x] `pwsh scripts/ci-local.ps1` passes on the Windows host.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0038-embeddable-vt-core/low-level-design/pty.md` -- the file move
  (`crates/pty/src/unix.rs` to `crates/vt/src/pty/unix.rs`), the `pty` feature and its
  dependency arithmetic. It says "no function body changes" for the move and names no
  trait on `SignalMask` or `Options`. **No change needed.**
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/pty.md` -- the original transport
  design. Its `Options` sketch (`:41`) lists fields, not derives. **No change needed.**
- `crates/vt/src/pty/mod.rs` -- the module rustdoc and `Options`. It documents
  `child_signal_mask` and the platform split in prose and claims no trait set.
  **Changed:** the `Options` derive line only.
- `crates/vt/docs/guide/13-pty.md` (`:205-217`) -- the embedder guide's "Platform
  differences" section names `SignalMask` and `Options::child_signal_mask` and says
  nothing about comparing either. **No change needed.**
- `crates/vt/CHANGELOG.md` -- **changed**, a `[Unreleased]` / `Removed` entry.
- `docs/spec-intakes/IN-0038-embeddable-vt-core/low-level-design/api-surface.md`
  (`:49`, `:248`) and `.../packaging.md` (`:315`) -- both enumerate the `pty` items per
  platform, neither states a trait set. **No change needed.**
- `docs/spec-intakes/IN-0038-embeddable-vt-core/evidence/US-0104-verify.md` (`:196`) --
  a verification record that observes the derive as it then was. Evidence is a record of
  a past run and is not rewritten.

### Documentation Action

Update required: `crates/vt/CHANGELOG.md`. Everything else above is a no-change review --
no owning document states the trait set of either type, so removing two derives
contradicts nothing that was written down.

Reason: the crate is consumed by other projects as a git dependency, so a removed public
trait impl belongs in the changelog even though no signature moved and neither public API
snapshot can see it.

### Reconciliation

Docs changed: `crates/vt/CHANGELOG.md` (one `Removed` entry under `[Unreleased]`). The
no-change reasons recorded above remain valid: re-checked after the edit, no owning
document names a derive on `SignalMask` or on `pty::Options`.

## Context

`pty::Options` is the part the report did not name. It derives
`#[derive(Clone, Debug, PartialEq, Eq, Default)]` and carries

```rust
#[cfg(unix)]
pub child_signal_mask: Option<SignalMask>,
```

A `PartialEq` derive expands to a field-by-field `==`, so `Options: PartialEq` on Unix
requires `Option<SignalMask>: PartialEq` requires `SignalMask: PartialEq`. Today the
compiler is satisfied because the *impl* the broken derive emits exists -- only its body
fails to type-check -- which is why the ubuntu job reported one error and not two.
Removing `SignalMask`'s derive without removing `Options`'s would have traded one Linux
compile error for another, one file away, with the Windows host still unable to see
either.

The alternative was to write `PartialEq` for `SignalMask` by hand -- a byte comparison of
the `sigset_t`, or a `sigismember` walk over `1..libc::NSIG`. Both were rejected:
they add `unsafe` (or a constant whose presence varies by target) to a file this host
cannot compile, in order to support a comparison that no code in this repository
performs. The whole point of this packet is to stop shipping unverifiable Unix code, not
to ship more of it.

Removing the derives from `Options` costs Windows an impl that did compile. That is
accepted deliberately: a public type whose trait set differs by platform is worse than a
public type that compares on neither, `--diff-platforms` exists precisely to keep the two
surfaces identical apart from the documented six `pty` lines, and no caller, test or
document uses the comparison.

## Plan

- [x] Grep `crates/` for every `SignalMask` and `pty::Options` use.
- [x] Drop `PartialEq, Eq` from `SignalMask` and from `Options`.
- [x] Rewrite the stale comment in `SignalMask::current`.
- [x] Read the whole of `unix.rs` and the `cfg(unix)` paths of `mod.rs` for further
      never-compiled hazards; report all, fix only what is clearly wrong.
- [x] `CHANGELOG` entry.
- [x] `pwsh scripts/ci-local.ps1`.

## Decisions

None. The choice between "remove the unused derive" and "hand-write the comparison" is
recorded in Context; it binds no future work.

## Verification Plan

**This packet is verified by inspection on the host it was written on, and the owner's
next push is the proof.** The changed code is `cfg(unix)`. This machine has only the
`x86_64-pc-windows-msvc` target installed, adding one is out of scope for an agent, and
the Windows quality gate compiles none of the changed lines. `pwsh scripts/ci-local.ps1`
therefore proves only that nothing on the Windows side regressed. The claim "Linux
compiles" is **unproven here** and becomes proven when the GitHub Actions
"Full workspace quality gate" job runs on ubuntu-latest after the next push.

- Reasoned proof, in place of a compile: the derive is removed, so nothing asks
  `sigset_t` for `==` or for `Eq`; `grep -rn "SignalMask" crates/` and the `Options`
  greps show no other use site; `Options`'s remaining derives (`Clone`, `Debug`,
  `Default`) are satisfied by `Option<T>` for any `T: Clone` plus `SignalMask: Copy`,
  and by `SignalMask`'s hand-written `Debug`.
- Platform proof is therefore **not claimed** below.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Greps

```text
grep -rn "SignalMask" crates/ docs/ scripts/
```

Every hit in `crates/` is a definition, a re-export, a field declaration or a rustdoc
mention: `crates/vt/src/pty/unix.rs` (`:35`, `:37`, `:71`, `:73`),
`crates/vt/src/pty/mod.rs` (`:38`, `:61`, `:128`, `:131`),
`crates/vt/docs/guide/13-pty.md:214`, `crates/vt/public-api.unix.txt:864`,
`crates/vt/CHANGELOG.md:122`. **No `==`, no `assert_eq!`, no `matches!`, no
`HashSet`/`HashMap` key.** The `Options` search is the same story: the four construction
sites (`crates/local-shell/src/session.rs:48`,
`crates/tools/src/bin/pty-throughput.rs:35`, `crates/tools/src/bin/vt-esctest.rs:91`, and
the test helpers in `crates/vt/src/pty/unix.rs`, `crates/vt/src/pty/windows.rs`,
`crates/vt/src/pty/windows/pseudo_console_tests.rs`, `crates/vt/tests/pty_contract.rs`)
build one and pass it by value. Nothing compares one.

### Public API snapshots

`crates/vt/public-api.unix.txt` has 888 lines and records no `impl` line at all: the
`SignalMask` entries are `struct oneterm_vt::pty::SignalMask` (`:864`) and its `current`
method. `Options` is one `struct` line plus its `structfield`s. Confirmed unchanged by
`python scripts/vt-public-api.py --check --no-doc` and `--diff-platforms` in the gate run
below.

### `pwsh scripts/ci-local.ps1`

Run in the worktree, Windows host, no `--full`. All 25 steps passed, exit 0:

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

The two packets share one gate run.

### Read-through of the never-compiled half

The whole of `crates/vt/src/pty/unix.rs` (526 lines, including its six `cfg(test)` tests) and the `cfg(unix)` paths of `crates/vt/src/pty/mod.rs` were read once for the
class of defect this packet is about: a derive on a `libc` type, a `libc` item that does
not exist on the target, an import that `-D warnings` would reject, a trait signature
that does not match. Findings:

| # | Where | Finding | Action |
| --- | --- | --- | --- |
| 1 | `unix.rs:34` | The reported defect. | **Fixed.** |
| 2 | `mod.rs:112` | `Options` derives `PartialEq, Eq` over an `Option<SignalMask>` field -- the second half of the same defect, one file away, not in the report. | **Fixed.** |
| 3 | `unix.rs:41-45` | The comment justifying `sigemptyset` says the padding is zeroed because reading it "in `PartialEq`" would be undefined behaviour. With the derive gone that reason is false, and the *real* reason is stronger: the padding is read by every `Copy` of the struct, not only by a comparison. | **Fixed** (comment only; the `sigemptyset` call stays). |
| 4 | `mod.rs:17-20` | The module rustdoc says dropping a `PseudoConsole` "waits a bounded grace period for the child to exit, and terminates that child if it never does. The drop therefore blocks." That is the Windows `ChildExitWatcher` contract (`DEC-0016`). The Unix type has **no** `Drop` impl at all and says so in a block comment at `unix.rs:219-227`: it closes `master` and lets `SIGHUP` do the work. The public prose therefore describes one platform as if it were both. | **Reported, not fixed.** It is a documentation defect in a public contract, not a compile hazard, and choosing between "make the Unix drop match the prose" and "make the prose describe two platforms" is the transport owner's call. Own follow-up. |
| 5 | `unix.rs:10-24` | Every import is reachable on Unix: `AsRawFd`, `FromRawFd`, `OwnedFd`, `RawFd`, `OsStrExt` (`as_os_str().as_bytes()`), `UnixStream`, `CommandExt` (`pre_exec`), `Read` (`next_child_event`), `Arc` (the `register` signatures), `mpsc`, `fmt`, `File`, `MaybeUninit`, `ptr`, `Command`, `Stdio`. No unused import. | None. |
| 6 | `unix.rs:359-374` | `libc::ioctl(fd, libc::TIOCSWINSZ, &winsize as *const libc::winsize)` casts a borrow with `as`, which `clippy::borrow_as_ptr` would reject -- but that lint is `pedantic` and this workspace enables no pedantic group (root `Cargo.toml` `[workspace.lints.clippy]` sets `style = allow` and lists individual lints). Harmless. The `#[allow(clippy::cast_lossless)]` at `:351` is likewise a no-op under the current configuration. | None. |
| 7 | `unix.rs:302-323` | `libc::openpty` on linux-gnu is declared by libc behind `#[link(name = "util")]`. glibc 2.34 and later fold `libutil` into `libc`, ubuntu-latest ships 2.39, and the same call already built on this project's ubuntu job before the `SignalMask` derive was introduced. | None. |
| 8 | `unix.rs:325-344` | `libc::IUTF8` is reached only under `cfg(any(target_os = "linux", target_os = "macos"))`, where it exists; the `else` arm binds `let _ = master;` so the parameter is used on every other Unix. | None. |
| 9 | `unix.rs:125-163` | The `pre_exec` closure must be `FnMut() -> io::Result<()> + Send + Sync + 'static`. It captures `Option<CString>`, `Option<SignalMask>` and two `RawFd`s. `SignalMask` is a plain-data wrapper, so it stays `Send + Sync` after the derives are removed -- `Send`/`Sync` are auto traits and neither was derived. | None. |
| 10 | `unix.rs:228-299` | The three trait impls match `mod.rs`'s declarations exactly, including `unsafe fn register` and the two associated types (`Reader = File`, `Writer = File`, both satisfying `io::Read` / `io::Write`). | None. |

### Gaps

1. **No Linux compile happened here.** See the Verification Plan. The gate run above is
   Windows-only and compiles none of the changed lines. Platform proof is unchecked, and
   the packet claims inspection, not a build.
2. **Finding 4 is unowned.** The module rustdoc's drop contract is Windows-shaped and
   describes both platforms. Not this packet's outcome; it needs a transport-owner
   decision about which half to change.

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
            "BUG-0063",
            "SignalMask derives PartialEq on a libc::sigset_t that has neither",
            "2026-09-16",
            "standard",
            "docs/spec-intakes/IN-0038-embeddable-vt-core/low-level-design/pty.md",
            "docs/spec-intakes/IN-0038-embeddable-vt-core/BUG-0063-signal-mask-derives-partial-eq.md",
            "implemented",
            0, 0, 0, 0,
            "pwsh scripts/ci-local.ps1 passed on Windows (25 steps, 131 test sections, 4533 passed, 0 failed). The changed lines are cfg(unix) and were NOT compiled: verified by inspection, proof is the next ubuntu-latest CI run.",
            "pwsh scripts/ci-local.ps1",
            "2026-09-16",
            "pass",
            "Also removed PartialEq/Eq from pty::Options, which could not keep them once SignalMask lost them. Open follow-up: the pty module rustdoc describes the Windows drop contract as if it were both platforms (Unix has no Drop impl).",
            43,
        ),
    )
```

## Handoff

Next action is the owner's: push. The ubuntu-latest "Full workspace quality gate" job is
the proof this packet cannot produce. If that job still fails in `crates/vt/src/pty`, the
failure text belongs back on this packet as acceptance rework rather than in a new bug.
