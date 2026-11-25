# Independent verification: BUG-0063 (`SignalMask` / `Options` derives)

Verifier: adversarial review agent, own worktree, no implementer context.
Subject: `5a0106f6` "fix(ci): unbreak the Linux build and the flood hand-over gate",
parent `c8055a71` (main).
Host: Windows 11, `x86_64-pc-windows-msvc` only, toolchain 1.96.0, `CARGO_BUILD_JOBS=4`.

## Verdict: **PASS WITH NOTES**

The fix is correct, minimal and complete for what it claims. Nothing in the workspace
needs `SignalMask` or `pty::Options` to be `PartialEq`, `Eq` or `Hash`, no gate can see
the removal, and the remaining derives are all satisfiable. The notes below are two
public-documentation defects in the same file the packet changed. One the packet already
found and reported (its finding 4); **one it missed**, and that one is a direct
self-contradiction between two documents the same `cargo doc` run renders.

Like the packet, this verification **cannot compile `cfg(unix)` code**: only the msvc
target is installed and adding one is out of scope. The Linux claim stays unproven until
the owner pushes.

---

## Attack 1 - completeness: does anything still require `SignalMask: PartialEq`?

**Result: no. The removal is complete.**

```text
grep -rn "SignalMask" crates/ docs/ examples
grep -rn "pty::Options|PtyOptions|Options::default()|vt::pty::|use oneterm_vt::pty" crates/ --include=*.rs
```

Every `SignalMask` hit in `crates/` is a definition, a re-export, a field declaration, a
doc mention or a snapshot line: `crates/vt/src/pty/unix.rs:35,37,74,76`,
`crates/vt/src/pty/mod.rs:38,61,133,136`, `crates/vt/docs/guide/13-pty.md:214`,
`crates/vt/public-api.unix.txt:864`, `crates/vt/CHANGELOG.md:122,224-227`. No `==`, no
`assert_eq!`, no `matches!`, no map or set key.

`Options` construction sites, all by value, none compared:
`crates/local-shell/src/session.rs:12,57`, `crates/tools/src/bin/pty-throughput.rs:21`,
`crates/tools/src/bin/vt-esctest.rs:55,105`, `crates/vt/tests/pty_contract.rs:18-40`,
`crates/vt/src/pty/unix.rs:426`, `crates/vt/src/pty/windows.rs:191,244`,
`crates/vt/src/pty/windows/pseudo_console_tests.rs:24`.

The one `assert_eq!` near an `Options` is `crates/vt/src/pty/windows.rs:244`,
`assert_eq!(cmdline(&Options::default()), "powershell")` - it compares the returned
`String`, not the `Options`. `crates/vt/src/pty/loopback_tests.rs:252` compares a
`ChildEvent`, whose `PartialEq, Eq` derives are untouched and rest on
`Option<ExitStatus>`, which compares on both platforms.

Transitive containment checked: no other public or private type in `crates/vt` embeds
`Options` or `SignalMask`. The `cfg(unix)` test module in `unix.rs:404-529` builds
`Options` with `..Options::default()` and asserts on exit codes and byte buffers only.

Doctests: `crates/vt/src/guide.rs` pulls fifteen guide chapters in with
`#[doc = include_str!]`, so their code fences are compiled. `crates/vt/docs/guide/13-pty.md`
is one of them, and `grep -n "PartialEq|Eq|derive"` over it and over `12-versioning.md`
and `crates/vt/README.md` returns nothing. Its two Rust fences (`:31`, `:72`) construct an
`Options`; neither compares one.

Remaining derives all still hold on Unix after the change:

| Derive on `Options` | Needs from `SignalMask` | Present? |
| --- | --- | --- |
| `Clone` | `Clone` | yes, `unix.rs:34` |
| `Debug` | `Debug` | yes, hand-written at `unix.rs:74-78` |
| `Default` | nothing (`Option::default`) | yes |

`Copy` is load-bearing beyond the derives and was correctly kept: `spawn` does
`let signal_mask = options.child_signal_mask;` (`unix.rs:120`) out of a `&Options`, and
the `pre_exec` closure is `FnMut`, so `if let Some(mask) = signal_mask` inside it
(`unix.rs:142`) only compiles because `Option<SignalMask>` is `Copy`. Dropping `Copy`
alongside `PartialEq` would have broken the build; the implementer did not.

`Send`/`Sync` for the `pre_exec` bound are auto traits over `libc::sigset_t`
(plain data on every Unix) and are unaffected by removing derives.

**No defect.**

## Attack 2 - was removing the derives from `Options` necessary?

**Result: defensible, and better supported than the packet's own public wording.**

The smaller-looking alternative is a hand-written `impl PartialEq for SignalMask`
comparing membership rather than bytes, which would have let `Options` keep its derives
and made the public API change zero. I checked whether that alternative is actually
available, against the vendored `libc 0.2.186` the lockfile pins
(`Cargo.lock:4281-4283`), read-only:

- `libc::sigismember` exists on every Unix (`src/unix/mod.rs:1589`).
- **`libc::NSIG` does not exist on linux-gnu or macOS.** `grep -rn "pub const NSIG"
  src/unix/` in libc 0.2.186 returns only `hurd`, `newlib/espidf`, `newlib/horizon` and
  `redox`. So `1..libc::NSIG` does not compile on either platform CI builds, and a
  portable membership walk needs a per-target bound (`SIGRTMAX()` on glibc, a literal on
  macOS) behind `cfg`.
- The byte-comparison variant is genuinely unsound in general: `sigset_t` on glibc b64 is
  `s! { pub struct sigset_t { __val: [u64; 16] } }`
  (`src/unix/linux_like/linux/gnu/b64/mod.rs:32-36`), and reading bytes the kernel never
  wrote is UB regardless of the `sigemptyset` zeroing convention holding in practice.

So the alternative is not two lines; it is `cfg`-per-platform `unsafe` code on a file this
host cannot compile, to support a comparison nothing performs. **Rejecting it is right.**

I also confirmed the root diagnosis rather than taking it on trust: libc's `s!` macro
derives `Clone, Copy, Debug` unconditionally and `PartialEq, Eq, Hash` only
`#[cfg_attr(feature = "extra_traits", ...)]` (`src/macros.rs:157-171`), and
`extra_traits` is a non-default feature (`Cargo.toml:194`). On macOS `sigset_t` is
`pub type sigset_t = u32` (`src/unix/bsd/apple/mod.rs:21`). The commit message's "compiled
on one Unix and not on the other" is exactly right.

`CHANGELOG` check: `crates/vt/CHANGELOG.md:222-231` is a single `### Removed` section
under `## [Unreleased]` (the only `Removed` heading in the file) and it names **both**
types in its first sentence, and calls out that Windows is where the `Options` half is a
real removal. The requirement in the brief - that the CHANGELOG record the `Options`
removal too, not only `SignalMask` - is met.

**Defect D3 (LOW, documentation accuracy).** The rustdoc the change adds to `Options`
(`crates/vt/src/pty/mod.rs:112-116`) justifies the removal with *"a hand-written one would
compare padding"*. That is true only of the byte variant; a `sigismember` walk compares no
padding. The actual decisive reasons - nothing needs the comparison, and libc offers no
portable signal-count bound - are correctly recorded in the packet's Context section but
not in the public doc comment, which is the text an embedder reads. Suggest replacing the
padding clause with "nothing needs it, and a portable hand-written one needs per-target
signal-count knowledge".

## Attack 3 - read-through of the never-compiled half

I read `crates/vt/src/pty/unix.rs` (529 lines) and every `cfg(unix)` path of
`crates/vt/src/pty/mod.rs` (226 lines) in full.

### Compile / lint hazards for the Linux job: none found

Checked against the lint configuration that actually applies. Root `Cargo.toml`
`[workspace.lints.clippy]` sets `style = allow` with individual overrides and enables **no
pedantic or nursery group**; `crates/vt/Cargo.toml` has `[lints] workspace = true`;
`crates/vt/src/lib.rs:56` is the only crate-level attribute, `#![warn(missing_docs)]`.

- **Imports** (`unix.rs:10-24`): all sixteen are reachable on Unix - `fmt`, `File`,
  `io::{self, Read}`, `MaybeUninit`, `AsRawFd`/`FromRawFd`/`OwnedFd`/`RawFd`, `OsStrExt`
  (`as_os_str().as_bytes()`), `UnixStream`, `CommandExt` (`pre_exec`), `Command`/`Stdio`,
  `ptr`, `Arc`/`mpsc`, `polling::{Event, PollMode, Poller}`, and the seven `crate::pty`
  items. No unused import.
- **`missing_docs`**: `SignalMask` and `SignalMask::current` are both documented;
  `apply` and the tuple field are private; `Options::child_signal_mask` is documented.
  Nothing public in the Unix half is undocumented.
- **Intra-doc links**: the only link in the Unix-gated prose is
  ``[`Options::child_signal_mask`]`` (`unix.rs:33`) - `Options` is in scope via the `use`,
  and the field is `cfg(unix)`, so it resolves on the platform that renders it.
  ``[`SignalMask::current`]`` (`mod.rs:133`) sits inside the `cfg(unix)` field doc.
  `cargo doc -p oneterm-vt --no-deps` and `--all-features` both passed in the gate run
  below, which is the Windows half; the Unix half has no link the Windows half lacks.
- **libc item names against 0.2.186** (vendored source, read-only, scoped greps):
  `openpty` `src/unix/linux_like/mod.rs:2161` (declared for every non-uclibc linux_like),
  `ioctl(fd, Ioctl, ...)` `:1731`, `TIOCSCTTY: Ioctl`
  `src/unix/linux_like/linux/arch/generic/mod.rs:185`, `IUTF8`
  `src/unix/linux_like/android/mod.rs:1860` and the glibc equivalent, `sigemptyset`
  `src/unix/mod.rs:1581`, `sigprocmask` `:1592`, `setsid` `:1106`, `pthread_sigmask` in the
  per-family modules. Every item the file names exists on linux-gnu and on macOS.
- `libc::TIOCSCTTY as _` is a same-type cast on linux (`Ioctl`), which `clippy::unnecessary_cast`
  does not fire on for an inferred `as _` target; and it is a real widening on the BSDs.
  The `#[allow(clippy::cast_lossless)]` at `unix.rs:349` and the `&winsize as *const _`
  at `:372` are both governed by **pedantic** lints (`cast_lossless`, `borrow_as_ptr`)
  that this workspace does not enable, so neither is a `-D warnings` hazard. Same
  conclusion as the packet's findings 6-8; independently reached.

Which Linux steps would have caught the original defect: **both**. The ubuntu
`workspace-quality` job runs `cargo clippy --workspace --all-targets -- -D warnings`
(`.github/workflows/ci.yml:117`), which type-checks `crates/vt` with the default `pty`
feature, and the `vt-package` job compiles it again. The packet quotes the package job;
the clippy step would have failed identically. Not a defect - just worth knowing the fix
has to satisfy both.

### Defect D1 (MEDIUM, pre-existing, public documentation) - **missed by the packet**

`crates/vt/src/pty/mod.rs:41-43` states:

```text
//! Two threads exist inside the transport on Windows (a pipe reader and a pipe
//! writer) and one on Unix (the reaper). All are internal, all are joined on
//! drop, and none calls into embedder code.
```

**"all are joined on drop" is false on both platforms**, and the crate says so itself in
two other places that `cargo doc` renders in the same page set:

- `crates/vt/docs/guide/13-pty.md:158` (pulled into the rustdoc by
  `crates/vt/src/guide.rs:80`): *"**None of them is joined, and drop does not wait for
  them.** Be exact about this, because shutdown ordering gets built on it"*, then names
  both halves - the Windows pipe threads are parked in a blocking read/write with the
  handle dropped at spawn, and *"the Unix reaper owns the child handle and deliberately
  outlives the drop"*.
- `crates/vt/src/pty/windows/pipe.rs:357`: *"A pipe thread that cannot be joined: it is
  parked in a blocking `ReadFile` or `WriteFile`"*.
- `crates/vt/src/pty/unix.rs:194-207`: `reap_in_background` spawns the reaper and
  `.map(drop)`s the `JoinHandle` - there is nothing left to join.

So the module-level rustdoc and the embedder guide give an embedder opposite answers to
"may I assume this crate's threads are gone when `drop` returns?", and the guide is the
one that is right. MEDIUM rather than LOW because the guide itself flags this exact
sentence as load-bearing for shutdown ordering. Pre-existing, not introduced by `5a0106f6`
- but it is in the file this packet changed, in the paragraph immediately below the one
the packet's own finding 4 quotes, and the packet's claimed read-through of the
`cfg(unix)` half of `mod.rs` did not report it.

### Defect D2 (LOW, pre-existing, public documentation) - packet finding 4 **confirmed**, and wider

The brief asked me to confirm or refute the packet's reading of `mod.rs:17-20`.
**Confirmed.** The prose reads:

```text
//! Passive while it lives - **dropping** a pseudo-console is an action with an
//! external side effect. It closes the console, waits a bounded grace period for
//! the child to exit, and terminates that child if it never does. The drop
//! therefore blocks, and belongs on an owner thread rather than on a UI thread.
```

That is the Windows contract exactly: `crates/vt/src/pty/windows/child.rs:53`
`const CHILD_EXIT_GRACE: Duration = Duration::from_secs(2)`, `:196`
`WaitForSingleObject(handle, CHILD_EXIT_GRACE...)`, `:208` `TerminateProcess(handle, 1)`.

The Unix `PseudoConsole` has **no `Drop` impl at all** - `grep -n "impl Drop" crates/vt/src/pty/unix.rs`
returns nothing - and the file states the design in a block comment at `unix.rs:209-217`:
no drop sends a signal, the reaper's `wait()` reaps the zombie, and closing `master` lets
the line discipline `SIGHUP` the foreground group. Unix drop neither waits nor terminates,
and does not block.

Two additions the packet did not record:

- The **same false claim appears a second time**, in the embedder guide at
  `crates/vt/docs/guide/13-pty.md:172-175`, in almost the same words. A fix has to touch
  both, not just `mod.rs`.
- `mod.rs:208-211`, the `EventedPty` trait rustdoc, says child exit must be observable
  without reading, *"on Unix that is race-free `SIGCHLD` handling"*. `unix.rs:3-8` opens
  by rejecting that design explicitly: *"Child exit is **not** delivered through a
  process-global `SIGCHLD` handler... instead one thread per session owns the `Child`,
  blocks in `wait`"*. Third contradiction, same family.

All three are documentation-only: they do not affect any compile, any test, or
`RUSTDOCFLAGS='-D warnings' cargo doc` (they are prose, not links). They need a transport
owner's decision, as the packet says, and should be one follow-up covering `mod.rs:17-20`,
`mod.rs:41-43`, `mod.rs:208-211` and `13-pty.md:158,172-175` together.

## Attack 4 - see `BUG-0064-verify.md`

The flood hand-over attacks are in
`docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0064-verify.md`.

## Attack 5 - packet honesty

**Result: honest.** Checked item by item.

- **Owning docs reviewed**: eight are listed with line references. I spot-checked every
  "no change needed" claim by grepping for `PartialEq` across
  `docs/spec-intakes/IN-0029-vt-engine/`, `docs/spec-intakes/IN-0038-embeddable-vt-core/`
  and `crates/vt/`. No owning doc states a trait set for either type. The `Options` sketch
  at `docs/spec-intakes/IN-0029-vt-engine/low-level-design/pty.md:35-43` lists fields and
  no derives, as claimed. `crates/vt/docs/guide/13-pty.md:204-217` names both Unix items
  and says nothing about comparing them, as claimed.
- **Public API snapshots**: the claim that neither snapshot records trait impls is
  **true**. `grep -n "PartialEq|Eq" crates/vt/public-api.unix.txt` returns nothing; the
  file is item-level only (`struct ... SignalMask` + `method current` at `:864-865`,
  `struct ... Options` + five `structfield` lines at `:851-856`). `scripts/vt-public-api.py`
  reads rustdoc HTML for items, fields and variants and says so in its own docstring
  (`:47-51`). Worth stating plainly, because it means **no gate in this repository can see
  this API removal** - the CHANGELOG entry is the only record, which is why attack 2
  checked it.
- **Harness snippet**: parses. 17 column names, 17 `?` placeholders, 17 tuple values, and
  the column list and order match the established schema used at
  `docs/spec-intakes/IN-0036-russh-0-63/US-0095-russh-0-63-bump.md:716-719` exactly
  (`id, title, created_at, risk_lane, contract_doc, packet_doc, status, unit_proof,
  integration_proof, e2e_proof, platform_proof, evidence, verify_command,
  last_verified_at, last_verified_result, notes, intake_id`). Proof flags are all `0`,
  which is honest: no platform proof is claimed.
- **ci-local result**: reproduced. See attack 6 - my numbers are identical to the
  packet's, to the test.

**Defect D4 (LOW, harness hygiene).** `intake_id` is a hardcoded `43` and the packet also
says `harness.db` was never opened. The value is therefore unverified: if IN-0038 is not
row 43, the snippet silently attaches the story to the wrong intake, and it uses
`INSERT OR REPLACE`, so it would overwrite rather than fail. Two prior packets handled
this better - `docs/spec-intakes/IN-0029-vt-engine/BUG-0061-adapter-never-sets-cell-pixels.md:376`
leaves `<IN-0029 row id, coordinator fills>` as an explicit placeholder, and
`US-0095` resolves it with `SELECT last_insert_rowid()`. Recommend a `SELECT id FROM
intake WHERE ...` lookup or a placeholder. Same defect in BUG-0064 (`34`).

## Attack 6 - `pwsh scripts/ci-local.ps1`

Run to completion by me, in this worktree, on `5a0106f6`, no `--full`,
`CARGO_BUILD_JOBS=4`, cold target directory.

```text
pwsh scripts/ci-local.ps1
...
==> python scripts/third-party-notices.py --check
THIRD-PARTY-NOTICES.md is up to date.

ci-local: all checks passed.
[exited with code 0]
```

All **25** steps green, in the order the packet lists them. Test totals, aggregated from
the log by summing every `test result:` line per step:

| step | sections | passed | failed | ignored |
| --- | --- | --- | --- | --- |
| `cargo test --workspace` | 71 | 2046 | 0 | 12 |
| `cargo test -p oneterm-vt --features vt-paranoid` | 20 | 834 | 0 | 4 |
| `cargo test -p oneterm-vt --features regex` | 20 | 847 | 0 | 4 |
| `cargo test -p oneterm-vt --no-default-features` | 20 | 806 | 0 | 4 |
| **total** | **131** | **4533** | **0** | **24** |

**Identical to the packet's table, section for section and test for test.** The packet's
claim is reproducible, not decorative.

What it does *not* prove, and the packet is right to say so: this run compiled **zero**
lines of `crates/vt/src/pty/unix.rs`. `rustc --print target-list` availability aside, only
`x86_64-pc-windows-msvc` is installed here and `rustup target add` is out of scope.

## Defects

| # | Severity | Where | Finding |
| --- | --- | --- | --- |
| D1 | **MEDIUM** | `crates/vt/src/pty/mod.rs:41-43` | "all are joined on drop" is false on both platforms and is directly contradicted by `crates/vt/docs/guide/13-pty.md:158` and `crates/vt/src/pty/windows/pipe.rs:357`. Both texts are rendered by the same `cargo doc`. Pre-existing; **missed** by the packet's read-through. Documentation only. |
| D2 | LOW | `crates/vt/src/pty/mod.rs:17-20`, `:208-211`; `crates/vt/docs/guide/13-pty.md:172-175` | The Windows drop contract (2 s grace + `TerminateProcess`) is stated as if it were both platforms; Unix has no `Drop` at all. Packet finding 4 **confirmed**, plus a second copy in the guide and a third instance (`EventedPty` claiming Unix uses `SIGCHLD`, which `unix.rs:3-8` explicitly rejects). Pre-existing, documentation only. |
| D3 | LOW | `crates/vt/src/pty/mod.rs:112-116` | The public reason given for dropping the derives ("a hand-written one would compare padding") is imprecise; a `sigismember` walk compares no padding. The decisive reasons are in the packet but not in the rustdoc. |
| D4 | LOW | packet `:299-322` | `intake_id` hardcoded to `43` without reading `harness.db`, under `INSERT OR REPLACE`. Use a lookup or a placeholder. |

**None of these blocks the fix.** D1-D3 are pre-existing documentation defects in the
`pty` module's public prose and belong in one follow-up packet owned by the transport;
D4 is a one-line change to a snippet nobody has run yet.

## Gaps in this verification

1. **No Linux or macOS compile.** Same limit the packet declares. Everything in attacks 1
   and 3 is reasoning over source plus the vendored `libc 0.2.186`, not a build. The
   ubuntu `workspace-quality` and `vt-package` jobs remain the proof.
2. **`extra_traits` feature unification not exhaustively traced.** I confirmed libc's
   `s!` macro gates `PartialEq` behind `extra_traits` and that the feature is non-default,
   but I did not enumerate the ubuntu dependency graph to prove no transitive crate turns
   it on. Moot for the fix - removing the derives is correct whether or not some crate
   would have unified the feature on - but it means I cannot independently confirm *why*
   the glibc build failed while it did, only that it must.
3. **`assert_eq!` search was textual.** A comparison routed through a generic bound
   (`fn f<T: PartialEq>(...)` called with `Options`) would not be caught by grep. I found
   no generic function in `crates/` taking a `pty::Options`, so the risk is theoretical.
