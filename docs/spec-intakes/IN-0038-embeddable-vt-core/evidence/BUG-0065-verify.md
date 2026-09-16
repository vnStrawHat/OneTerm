# Evidence: independent verification of BUG-0065

Packet: `docs/spec-intakes/IN-0038-embeddable-vt-core/BUG-0065-pty-rustdoc-describes-windows-as-both-platforms.md`
Intake: IN-0038
Commit under test: `ba1ba200` ("docs(vt): state the pseudo-console drop contract per platform"),
parent `f315bf5e` (`main`).
Verifier: independent adversarial pass, separate worktree, Windows 11, `CARGO_BUILD_JOBS=3`.
Date: 2026-09-16

## Verdict

**PASS-WITH-NOTES.**

The packet's central claim holds: the code was right and the prose was wrong, and the three
defects the `BUG-0063` verifier raised (D1 "all are joined on drop", D2 the Windows drop
contract stated as both platforms plus the `SIGCHLD` mis-attribution, D3 the "would compare
padding" reason) are each closed, checked against the code rather than against the packet's
own table. The public API surface is byte-identical, both package-gate greps pass, the
rustdoc builds clean under `-D warnings` in both feature sets and its new anchor resolves.

Six defects remain, none of them blocking, none HIGH. The most substantive is that the
**replacement** Unix sentence -- the one the packet exists to get right -- describes the
hang-up mechanism with three wrong attributions, while getting the child's actual fate
right. That is D1 below, MEDIUM.

## Attack 1 -- Windows: every claim against `crates/vt/src/pty/windows/`

Read in full: `windows.rs` (246 lines), `windows/child.rs` (335), `windows/conpty.rs`
(565, drop + spawn regions), `windows/pipe.rs` (416).

| Claim in the new prose | Verdict | Where in the code |
| --- | --- | --- |
| Field drop order gives `ClosePseudoConsole` before the watcher's grace | **confirmed** | `windows.rs:33-43` declares `conpty, conout, conin, child` in that order, with the invariant spelled out at `:34-38`; Rust drops fields in declaration order, so `Conpty::drop` (`conpty.rs:198-206`, `(self.api.close)(self.handle)`, resolved from `ClosePseudoConsole` at `conpty.rs:113`/`:148`) runs before `ChildExitWatcher::drop` (`child.rs:218-229`) |
| The grace is 2 s (`CHILD_EXIT_GRACE`) | **confirmed** | `child.rs:53` `const CHILD_EXIT_GRACE: Duration = Duration::from_secs(2)`; consumed at `child.rs:196` `WaitForSingleObject(handle, CHILD_EXIT_GRACE.as_millis() as u32)`. The "~20 ms normal / still alive at 15 s while initialising" measurement the guide quotes is `child.rs:48-52` |
| `TerminateProcess` targets only the handle the crate spawned | **confirmed** | `child.rs:194` takes `self.process.as_raw_handle()`; `self.process` is the `OwnedHandle` built from `process.hProcess` of this crate's own `CreateProcessW` (`conpty.rs:284`, `:317`) and handed straight to `ChildExitWatcher::new` (`conpty.rs:326`). `child.rs:208` passes that same handle to `TerminateProcess(handle, 1)`. No name match, no enumeration, no pid-based call anywhere in the module |
| Drop blocks the calling thread | **confirmed** | two blocking points: `ClosePseudoConsole` (blocks until conout drains, `conpty.rs:202-204`) and the up-to-2 s `WaitForSingleObject` (`child.rs:196`). `UnregisterWaitEx(..., INVALID_HANDLE_VALUE)` at `child.rs:226` is a third, bounded by the callback |
| Pipe threads are never joined | **confirmed** | `pipe.rs:363-368` `spawn_pipe_thread` ends in `.map(drop)`, discarding the `JoinHandle`. `grep -rn "join()" crates/vt/src/pty/` returns **nothing** on the whole module |
| Exit delivery is `RegisterWaitForSingleObject` -> mpsc + IOCP packet | **confirmed** | `child.rs:125-134` registers `child_exited` with `WT_EXECUTEINWAITTHREAD \| WT_EXECUTEONLYONCE`; the callback sends `ChildEvent::Exited` on the `mpsc` channel (`child.rs:88`) and posts a `CompletionPacket` to the registered poller (`child.rs:90-97`) |

No Windows claim in the new prose is wrong.

## Attack 2 -- Unix: `crates/vt/src/pty/unix.rs`, read in full (529 lines)

### No `Drop` impl

`grep -n "impl Drop" crates/vt/src/pty/unix.rs crates/vt/src/pty/windows.rs` -> **no hits**
(the only `impl Drop`s in the module are `windows/child.rs:218`, `windows/conpty.rs:198`,
`windows/conpty.rs:415`, `windows/pipe.rs:253`, `windows/pipe.rs:348`). The packet's claim
that neither `PseudoConsole` has a `Drop` impl of its own is **confirmed**, and so is
"nothing waits, so the drop does not block": the Unix fields are `File`, `UnixStream`,
`mpsc::Receiver` and a `u32` (`unix.rs:81-88`), none of whose drops block.

### The reaper: not joined, never signals, no `SIGCHLD` handler

- Not joined: `unix.rs:210-220`, `std::thread::Builder::new()...spawn(...).map(drop)` --
  the `JoinHandle` is dropped at spawn, exactly as on Windows. **Confirmed.**
- Never signals the child: there is no `kill`, `killpg` or `raise` anywhere in the file
  (`grep` for each: no hits). The reaper only `child.wait()`s (`unix.rs:213`), sends the
  event and writes one byte to the waker socket (`:216-217`). **Confirmed.**
- No `SIGCHLD` handler is installed: the only `SIGCHLD` in the file is
  `unix.rs:154` inside the `pre_exec` list at `:153-162`, which calls
  `libc::signal(signal, libc::SIG_DFL)` **in the forked child** -- a reset to default, the
  opposite of installing a handler, and it happens after `fork` so it cannot affect the
  parent. The module rustdoc says so itself at `unix.rs:3-8`. **Confirmed**, and the
  `EventedPty` rustdoc (`mod.rs:228-235`) and `13-pty.md:131-136` now agree with it.

### What closing `master` actually does -- the fd trace

This is the claim the task singled out, so here is the descriptor lifetime end to end.

1. `open_pty()` (`unix.rs:305-324`) returns `(master, slave)` as two `OwnedFd`s held by the
   parent.
2. `unix.rs:109-111` gives the child its stdio as **three `try_clone()` duplicates** of the
   slave, wrapped in `Stdio::from(..)`. These duplicates are owned by the `Command`, not by
   the `Child`: `Stdio::Fd` is `dup2`'d over the child's 0/1/2 and the parent's copies are
   released when `command` drops. `Child` keeps no descriptor for a non-`piped()` stdio,
   so the reaper thread -- which owns the `Child` for the rest of the session -- holds **no
   pty descriptor at all**.
3. In the child, `pre_exec` (`unix.rs:129-165`) calls `setsid()` (`:131`), then
   `set_controlling_terminal(slave_fd)` (`:141` -> `ioctl(fd, TIOCSCTTY, 0)` at `:349-360`),
   then closes both the inherited `slave_fd` and `master_fd` (`:144-145`). The child
   therefore keeps the slave only through its own 0/1/2.
4. In the parent, `slave` and `command` are plain locals of `spawn()`. Neither is moved
   into the returned `Self` (`unix.rs:187-192` takes only `master`), so **both drop when
   `spawn` returns** and the parent retains exactly one pty descriptor: the master.
5. `master` is never duplicated: `set_utf8_input(&master)`, `set_window_size(fd, ..)` and
   `set_nonblocking(fd)` all borrow; `reader()`/`writer()` return `&mut self.master`
   (`:274-280`); `polling` registers the raw fd without `dup`.

**So the premise holds**: when `PseudoConsole` drops, the `File` at `unix.rs:83` is the
last master-side descriptor in this process, the parent holds no slave, and the child is a
session leader whose controlling terminal is that pty. Closing the master **does** hang the
child up. The hypothesis the task raised -- "if the parent still holds the slave, closing
the master does not hang up the child" -- does not bite here on either count: the parent
does not hold the slave, and on Linux the master's close runs the pty driver's vhangup
unconditionally, whatever the slave's open count (the child itself keeps the slave open for
its whole life and is still hung up).

**But the sentence that describes this is wrong in three of its attributions.** See D1.

## Attack 3 -- remaining platform over-generalisation

Greps over `crates/vt/src` (rustdoc lines only), `crates/vt/docs/guide`, `crates/vt/README.md`
and `docs/terminal-backend.md` for `drop|terminat|grace|join|SIGCHLD|SIGHUP|reap`, every hit
read in context:

- `crates/vt/src` rustdoc: the only platform-sensitive hits left are `mod.rs:17-20`,
  `:38-43`, `:48-54`, `:56-65`, `:228-235` (all rewritten by this commit and all naming
  their platform; the `# Platforms` heading itself is `mod.rs:22`), `unix.rs:1-8`, `windows/child.rs:1-7`/`:44-53`/`:178-192`,
  `windows/pipe.rs:356-362` and `windows.rs:34-38` -- each inside a `cfg`-gated,
  platform-named module. **No unlabelled platform claim remains.**
- `crates/vt/docs/guide`: chapter 3 (`03-threading.md:126-131`) already said "none of which
  is joined on drop" and points at chapter 13 -- it was the correct text all along and the
  module rustdoc now agrees with it. Chapter 13's drop paragraph (`:175-193`) and its
  child-exit paragraph (`:132-136`) are the rewritten ones, both per platform.
- `crates/vt/README.md`: no hit relates to the transport at all (the two matches are
  `OscRoute::Drop` and "dropped, truncated or degraded"). Clean.
- `docs/terminal-backend.md`: §6.3 is explicitly "Windows-specific" and correct
  (`:558-570`). §6.2 is **not** platform-labelled and states the Windows sequence as the
  general one -- see D4.

## Attack 4 -- package-gate hygiene

```text
$ # the two greps copied verbatim from .github/workflows/ci.yml:235-251
$ grep -rn '^[[:space:]]*//[/!].*\(US-0[0-9]\{3\}\|BUG-0[0-9]\{3\}\|DEC-0[0-9]\{3\}\|IN-0[0-9]\{3\}\|crates/\|docs/\)' \
      crates/vt/src --include='*.rs' | grep -v 'https://github.com/'
GATE A PASS (no hits)

$ grep -rn '\(US-0[0-9]\{3\}\|BUG-0[0-9]\{3\}\|DEC-0[0-9]\{3\}\|IN-0[0-9]\{3\}\|crates/\|docs/\)' \
      crates/vt/docs/guide --include='*.md' | grep -v 'https://github.com/'
GATE B PASS (no hits)
```

Note that the citations the new prose *does* need (`DEC-0016`, `BUG-0055`, the error-policy
path) all sit in **non-doc** `//` comments (`child.rs:44`, `:178`, `pipe.rs:359`), which the
gate's `//[/!]` anchor deliberately does not match. That placement is preserved by this
commit, not broken by it.

Rustdoc, both feature sets, warnings denied:

```text
$ RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps
    Finished `dev` profile ... Generated target/doc/oneterm_vt/index.html          [exit 0]
$ RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps --all-features
    Finished `dev` profile ... Generated target/doc/oneterm_vt/index.html          [exit 0]
```

The new `[Platforms](#platforms)` link at `mod.rs:20` is a plain anchor, not an intra-doc
path, so `-D warnings` cannot prove it resolves. Checked against the rendered HTML instead:

```text
$ grep -o '<a href="[^"]*">Platforms</a>' target/doc/oneterm_vt/pty/index.html
<a href="#platforms">Platforms</a>
$ grep -o 'id="[a-z-]*"' target/doc/oneterm_vt/pty/index.html | sort -u
... id="platforms" ...
```

Link and target both present; the anchor resolves.

```text
$ python scripts/vt-public-api.py --check --no-doc
public API surface unchanged (public-api.windows.txt)                              [exit 0]
```

`git diff --stat f315bf5e ba1ba200 -- crates/vt/public-api.*.txt scripts/` is **empty**: the
snapshots and the checker itself were not touched, so "unchanged" is a real result and not a
re-baselined one.

## Attack 5 -- packet honesty

- **Reviewed-docs list and no-change reasons.** Spot-checked each:
  - `DEC-0016`: the packet says it "is stated in ConPTY terms throughout and never claims a
    Unix half". **Verified** -- `grep -iE "unix|linux|macos|posix|SIGHUP"` over all 114 lines
    of `docs/decisions/DEC-0016-terminate-a-shell-that-outlives-its-pseudo-console.md`
    returns **nothing**. The no-change reason is sound and the packet is right that this
    stays a documentation defect rather than a decision defect.
  - `crates/vt/docs/guide/03-threading.md:126-131`: **verified** already correct.
  - `crates/vt/CHANGELOG.md`: the one amended clause now says a hand-written comparison
    "would mean walking the signal numbers against a per-target upper bound libc does not
    export". **Verified against libc 0.2.186/0.2.189**: `pub const NSIG` exists only for
    hurd, newlib (espidf/horizon), redox and windows -- **not** for `linux-gnu`, `musl` or
    Apple, which are the targets in play. And `sigset_t` on linux-gnu is
    `s! { pub struct sigset_t { __val: [u64; 16] } }` (private field, `PartialEq` only under
    libc's `extra_traits`) while on Apple it is `pub type sigset_t = u32`. Every clause of
    the replacement reason for D3 is accurate -- including the "plain `u32` on some targets"
    half. The packet's reason for adding **no new** entry is also verified: the file's own
    bar is "an entry belongs here when it changes what an embedder compiles against or what
    bytes the terminal replies with" (`crates/vt/CHANGELOG.md:4-5`), and this commit changes
    neither.
  - `docs/terminal-backend.md`: reviewed, no change -- **partially supported only**, see D4.
- **Harness snippet.** Parses. 17 column names, 17 `?` placeholders, 17 tuple values, in the
  established order (`id, title, created_at, risk_lane, contract_doc, packet_doc, status,
  unit_proof, integration_proof, e2e_proof, platform_proof, evidence, verify_command,
  last_verified_at, last_verified_result, notes, intake_id`). The `*_proof` flags
  `1, 0, 0, 1` match the packet's own `HARNESS:PROOF` block (`Unit` + `Platform` ticked)
  exactly. `harness.db` was **not** opened -- it does not exist in this worktree and the
  main checkout's copy is out of bounds -- so the two db-dependent values are judged from
  the repository instead: `intake_id = 43` is corroborated as IN-0038 by four earlier
  verifications (`US-0097-verify.md:384`, `US-0099-verify.md:427`, `US-0100-verify.md:216`,
  `US-0102-verify.md:231`), and `risk_lane = "standard"` is **not** in the vocabulary -- D2.
- **Proof honesty.** Integration/E2E marked not applicable with a stated reason, and the two
  Gaps (Windows host only, no behaviour verified because none changed) are both accurate.
  The packet does not claim any Unix behaviour was executed, which is correct: nothing in
  this gate compiles a line of `unix.rs`.

## Attack 6 -- `pwsh scripts/ci-local.ps1`

Run to completion by me, in this worktree, at `ba1ba200`, no `--full`, `CARGO_BUILD_JOBS=3`.

```text
$ pwsh scripts/ci-local.ps1
==> cargo fmt --all -- --check
... 25 steps ...
==> python scripts/vt-public-api.py --check --no-doc
    public API surface unchanged (public-api.windows.txt)
==> python scripts/vt-public-api.py --check-nameable --no-doc
    every type in a public signature is nameable
==> python scripts/vt-public-api.py --diff-platforms
    windows only: oneterm_vt::pty::Options    structfield escape_args
    windows only: struct oneterm_vt::pty::PipeReader
    ... (the `pty` differences only)
==> rustdoc self-containment (crates/vt/src)                  OK
==> rustdoc self-containment (crates/vt/docs/guide)           OK
==> python scripts/check-doc-paths.py
    Doc path check passed for 199 current paths in 11 documents.
==> python scripts/check-english.py
    English contributor-text check passed for 919 files.
==> python scripts/third-party-notices.py --check
    THIRD-PARTY-NOTICES.md is up to date.

ci-local: all checks passed.
[exited with code 0]
```

All **25** steps green, in the order the packet lists them. Test totals, summed from every
`test result:` line per step:

| step | sections | passed | failed | ignored |
| --- | --- | --- | --- | --- |
| `cargo test --workspace` | 71 | 2046 | 0 | 12 |
| `cargo test -p oneterm-vt --features vt-paranoid` | 20 | 834 | 0 | 4 |
| `cargo test -p oneterm-vt --features regex` | 20 | 847 | 0 | 4 |
| `cargo test -p oneterm-vt --no-default-features` | 20 | 806 | 0 | 4 |
| **total** | **131** | **4533** | **0** | **24** |

**Identical to the packet's claim** ("25 steps, 131 test sections, 4533 passed, 0 failed"),
section for section and test for test. The two steps that matter most for a documentation
packet -- the rustdoc self-containment greps -- are green, as is `cargo doc` in both feature
sets, run here as steps 12 and 13 in addition to my own `-D warnings` runs above.

`check-english.py` reports **919** files against the packet's 918: the extra file is this
evidence document, which was already in the tree when the gate ran, so the run also gates
the text you are reading.

## Defects

| # | Severity | Where | Finding |
| --- | --- | --- | --- |
| D1 | **MEDIUM** | `crates/vt/src/pty/mod.rs:48-50`, `crates/vt/docs/guide/13-pty.md:186-193`, `low-level-design/pty.md:432-433` | The replacement Unix sentence gets the child's fate right and the mechanism wrong three times over. See below. |
| D2 | **MEDIUM** | packet `:23` and `:246` | `risk_lane` is `standard`, which is not one of the lanes this repository uses. |
| D3 | LOW | `crates/vt/src/pty/mod.rs:59-60` | "The Windows pair is parked in a blocking pipe read or write and returns when the pipe breaks" is true of the conout reader and false of the conin writer. |
| D4 | LOW | `docs/terminal-backend.md:527-536` | Recorded as reviewed/no-change, but §6.2's platform-neutral headline is the same class of defect this packet fixes elsewhere. |
| D5 | LOW | packet `:256` (`intake_id`) | `43` is still hardcoded under `INSERT OR REPLACE` without reading `harness.db` -- the `BUG-0063` verifier's D4, not adopted. |
| D6 | LOW | packet `:307`, `:310` (the truth table) | Two of the three `unix.rs` line citations point at the wrong lines. |

### D1 (MEDIUM) -- the Unix hang-up mechanism

> `mod.rs:48-50`: "dropping closes the master side, and that last close of the controlling
> terminal makes the line discipline send `SIGHUP` to the child's foreground process group."

Three attributions in one sentence, none of them right:

1. **"the controlling terminal"** -- the descriptor being closed is the pty **master**. The
   child's controlling terminal is the **slave**: `unix.rs:131` `setsid()`, then `:141`
   `set_controlling_terminal(slave_fd)` -> `ioctl(slave_fd, TIOCSCTTY, 0)`. Calling the
   master "the controlling terminal" inverts the two sides of the pair the paragraph is
   about.
2. **"the line discipline sends"** -- the hang-up on master close is the tty layer's vhangup
   path, which *flushes* the line discipline rather than being performed by it. The line
   discipline is what produces `SIGINT` from `^C`; it is not what produces this `SIGHUP`.
3. **"to the child's foreground process group"** -- the kernel signals the **session
   leader** whose controlling tty this is, not the foreground process group as such. Here
   the two coincide, because the child called `setsid()` and is both, so the child does get
   `SIGHUP` -- but an embedder who reads this sentence and concludes that a foreground
   *grandchild* in its own process group (a `vim`, a `ping`) is hung up by closing the
   master will be wrong. That grandchild is hung up, if at all, by the shell's own job
   control reacting to its `SIGHUP`. The Windows bullet is careful to say exactly this kind
   of thing ("Only this process's own child is touched"); the Unix bullet is not.

All three copies say it: the rustdoc, guide chapter 13 (`:186-188`) and the low-level design
this commit also rewrote (`low-level-design/pty.md:432-433`, "closing the master side lets the
line discipline `SIGHUP` the child's foreground process group"). So the next writer working
from the owning design will reproduce it -- which is the specific failure mode the packet set
out to prevent.

Graded MEDIUM, not HIGH, deliberately: the *child's fate* -- the thing the severity rule
protects -- is stated correctly. The child is hung up when the master closes, and the crate
never signals it. What is wrong is which kernel layer does it and which set of processes it
reaches. Also note the same wording already exists as a private comment at `unix.rs:222-229`,
so this is prose inherited rather than invented; but this commit is what promoted it into
published rustdoc and into the embedder guide, which is why it is worth a line.

Suggested wording, for whoever picks this up: "dropping closes the master side, and the
pty's last master close hangs the slave up: the kernel sends `SIGHUP` to the session leader
-- the child, which called `setsid`. A foreground grandchild in another process group is
reached only by the shell's own job control."

### D2 (MEDIUM) -- `risk_lane: standard` is off-vocabulary

Classification line `:23` reads "Risk lane: standard" and the harness snippet writes
`"standard"` into the `risk_lane` column. Across every packet in `docs/spec-intakes/`, the
lanes in use are `normal` (41), `tiny` (10) and `high_risk`/`high-risk` (15). "standard"
appears **twice in the whole repository**: `BUG-0063` and this packet, the same author in
the same series. A `CHECK` constraint on that column would reject the insert outright; even
without one the row is unqueryable by lane. This packet's lane is `normal` (public
documentation of a public contract, no code). Fixing it in both places is a two-token edit
and should be done before the row is inserted.

### D3 (LOW) -- what actually ends the conin writer thread

`mod.rs:59-60` is right about the conout reader: `pipe.rs:171-186` parks it in `read_pipe`
(a blocking `ReadFile`, `:370-387`) and it returns on `Ok(0)` or an error, i.e. when
`ClosePseudoConsole` breaks the pipe. It is not right about the conin writer. That thread's
loop calls `pull` first (`pipe.rs:271-282`), and `pull` parks on the ring's condvar while
the ring is empty (`:297-304`) -- which is where it sits whenever the embedder is not
typing. It returns `false`, and the thread exits, because `PipeWriter::drop` calls
`Ring::close()` (`:348-352`), which sets `closed` and `notify_all()`s (`:151-155`). So the
writer thread ends **at the drop**, through the ring, not when the pipe breaks.

The error is conservative -- it tells an embedder to expect a lingering thread that in
practice usually goes away at once -- so nobody builds a wrong shutdown on it. Same phrasing
pre-exists at `pipe.rs:356-357` (private) and `13-pty.md:164-166`; this commit copied it
into rustdoc.

### D4 (LOW) -- `docs/terminal-backend.md` §6.2

The packet records this file as reviewed/no-change because "the escalation lives in §6.3
'Windows-specific', §6.2 defers to it by reference and names Windows-only APIs
(`ClosePseudoConsole`) where it describes the bounded wait". That is accurate about the one
sentence at `:532-535`. It does not cover §6.2's headline at `:527-529`: *"Closing a local
session is guaranteed to leave no process behind while OneTerm is running"* -- a guarantee
that rests entirely on the Windows escalation and is not labelled Windows. On Unix there is
no bounded wait and no escalation, and a child that ignores `SIGHUP` (or a `nohup`'d or
detached grandchild) survives the close. This is an internal architecture document rather
than published rustdoc, and it is outside the packet's stated scope, so it does not change
the verdict -- but the no-change reason as written is only partly supported, and the
sentence is the same class of defect BUG-0065 exists to remove.

### D5 (LOW) -- inherited

`intake_id` is still the bare literal `43` under `INSERT OR REPLACE`, with the packet also
stating that `harness.db` was never opened. The `BUG-0063` verifier raised this as its D4
and recommended a `SELECT id FROM intake WHERE ...` lookup or an explicit placeholder (as
`IN-0029/BUG-0061` does). The value is in fact correct for IN-0038 -- four earlier evidence
files confirm it against the live db -- so the risk is latent rather than realized, but the
recommendation was not adopted.

### D6 (LOW) -- stale line citations in the packet's truth table

The packet's per-platform truth table is the artefact a future reader will check the prose
against, and its Windows citations are all exact (`conpty.rs:198-206`, `child.rs:218-229`,
`child.rs:178-215`, `pipe.rs:363-368`). Two of its three `unix.rs` citations are not:

- "there is no `Drop` impl (`unix.rs:209-217` states this as the invariant)" -- lines
  209-217 are the body of `reap_in_background`. The invariant comment is at
  **`unix.rs:222-229`**.
- "the reaper is not joined either (`unix.rs:194-207`, `.map(drop)`)" -- lines 194-207 are
  `child_pid` plus `reap_in_background`'s signature. `.map(drop)` is at **`unix.rs:219`**;
  the function spans **`:205-220`**.

The third (`unix.rs:1-8, 200-218` for the reaper blocked in `Child::wait`) is fine:
`child.wait()` is at `:213`. `unix.rs` is not touched by this commit, so these are not
citations that went stale in it -- they were off when written.

## Observation (not a defect)

`mod.rs:61-63` says "Each holds only what it was given, so a thread still running after
`drop` returns cannot touch embedder memory." The Windows pipe threads do hold a clone of
the embedder's `Arc<Poller>` (`pipe.rs:129-134`), so they hold embedder memory -- but the
`Arc` keeps it alive, and after `Ring::close()` no further `wake()` can run, because both
`push` (`:204-214`) and `pull` (`:297-307`) test `closed` under the lock before waking. The
sentence is true in effect; it is the `Arc` that makes it true, not the absence of the
reference.

## What this verification does not prove

1. **Windows host only.** No line of `cfg(unix)` code was compiled or executed. The whole
   Unix half above -- the fd trace, the absence of a `Drop` impl, the reaper's behaviour and
   D1 -- was established by reading `unix.rs` in full and by reading libc's `sigset_t` and
   `NSIG` definitions from the vendored registry source. `rustup target add` was out of
   scope, so the Linux job's own result is not reproduced here.
2. **No behaviour was exercised**, because none changed. What is verified is that the prose
   matches the code, not that the code is right; `DEC-0016` and the ConPTY measurements it
   rests on are taken as given.
3. `harness.db` was never opened, by instruction. The snippet's shape is checked; its
   effect on the live database is not.

---

# Re-verification: the acceptance rework

Commit under test: `2aaf138b` ("docs(vt): attribute the Unix hang-up to the right
descriptor"), built on the verification commit `21328df6`.
Targeted pass over D1-D4 and D6 only. Date: 2026-09-16.

## Verdict

**PASS.** Every defect this pass covers is closed, and none of the fixes introduced a new
mis-attribution. The rework was correctly scoped as acceptance rework of BUG-0065 rather
than a new bug, and it fixed the block comment in `unix.rs:222-229` the wrong sentence was
derived from -- which is the part that would otherwise have been re-derived by the next
writer.

## D1 -- the Unix hang-up, all four copies

The three banned attributions are gone from the whole tree:

```text
$ grep -rn -i "line discipline\|foreground process group" \
      crates/vt/src crates/vt/docs docs/spec-intakes/.../low-level-design \
      docs/terminal-backend.md crates/vt/README.md
crates/vt/src/pty/unix.rs:327:/// Ask the line discipline to treat input as UTF-8. ...
```

The single survivor is correct and unrelated: `IUTF8` is a `termios` `c_iflag`, which *is*
the line discipline's business. Every "controlling terminal" in the tree now names the
**slave**.

Each of the four sentences judged against the fd lifetimes and the `setsid` + `TIOCSCTTY`
sequence traced in the first pass:

| # | Where | Text | Verdict |
| --- | --- | --- | --- |
| 1 | `crates/vt/src/pty/mod.rs:48-55` | "dropping closes the master side, and nothing else holds a slave descriptor once the child is running, so that close hangs up the slave -- the child's controlling terminal, taken at spawn -- and the child, as session leader, receives `SIGHUP`" | **correct** |
| 2 | `crates/vt/docs/guide/13-pty.md:186-193` | same wording, "That is the hang-up that was wanted" appended | **correct** |
| 3 | `low-level-design/pty.md:432-434` | "closing the master side is the last descriptor to go, so it hangs up the slave (the child's controlling terminal, taken at spawn) and the child, as session leader, receives `SIGHUP`" | **correct** |
| 4 | `crates/vt/src/pty/unix.rs:222-230` | "Closing `master` instead is both safe and sufficient: nothing else holds a slave descriptor once the child is running, so that close hangs up the slave -- the controlling terminal the child took with `setsid` + `TIOCSCTTY` -- and the child, as session leader, receives `SIGHUP`" | **correct** |

Against the code:

- **the slave is the controlling terminal** -- `unix.rs:131` `setsid()`, then `:141`
  `set_controlling_terminal(slave_fd)` -> `ioctl(slave_fd, TIOCSCTTY, 0)`. Copy 4's
  "`setsid` + `TIOCSCTTY`" names the pair in the right order, and `setsid` really is the
  prerequisite (it leaves the new session with no controlling terminal for `TIOCSCTTY` to
  fill).
- **closing the master is what hangs up** -- and it is the last master-side close, because
  `master` is never duplicated (`:180`, `:245`, `:274-280` all borrow). The `slave` opened
  at `:93` is not stored in `Self` (`:187-192`), so it and the `Command` holding the three
  `try_clone()` stdio duplicates (`:109-111`) drop when `spawn` returns; the child closed
  its own inherited copies at `:144`. The parent holds no slave descriptor, exactly as the
  first pass traced.
- **the child, as session leader, receives `SIGHUP`** -- correct on both Linux and BSD: the
  hang-up path signals session leaders whose controlling tty this is, and `setsid()` at
  `:131` makes the child one. This is the claim the previous wording got wrong by naming
  the foreground process group, and it is now the claim the crate can actually defend.

One wording note, not a defect: "nothing else holds a slave descriptor once the child is
running" reads most naturally as "nothing else **in this process**" -- the child of course
holds three, as its stdio, which is what there is to hang up. Copy 3 avoids the ambiguity
("the last descriptor to go"). No reader is misled about the outcome, and the sentence is
true on the reading the paragraph supports.

## D2 -- risk lane

`Risk lane: normal` (packet `:23`) and `"normal"` in the insert snippet (`:289`). The
snippet still parses: 17 column names, 17 `?` placeholders, 17 tuple values, proof flags
`1, 0, 0, 1` still matching the `HARNESS:PROOF` block. **Closed.**

## D3 -- the two Windows pipe threads

Split in all three places, and each half now matches the code:

- `mod.rs:57-63`: "The Windows reader is parked in a blocking pipe read and returns when the
  pipe breaks; the Windows writer waits on its ring and returns once the drop closes it."
- `13-pty.md:164-166`: the same split in the thread list.
- the packet's truth table `:76`, which now cites `pipe.rs:297-304` (the writer's condvar
  wait in `pull`) and `pipe.rs:348-352` (`PipeWriter::drop` -> `Ring::close()`) -- the exact
  lines the first pass identified. **Closed.**

## D4 -- `docs/terminal-backend.md` §6.2

`:527-530` now reads: "**Closing a local session** is guaranteed to leave no process behind
**while OneTerm is running** -- a **Windows** guarantee, because it rests on the escalation
in §6.3. The Unix transport has no escalation: its `PseudoConsole` has no `Drop` at all, and
closing the master hangs up the slave so the child, as session leader, gets `SIGHUP`." The
headline is labelled and the added Unix sentence carries none of the three mis-attributions.
The later `ClosePseudoConsole` sentence (`:535-537`) is still written without a platform word
of its own, but it now sits under an explicit Windows label and names only Windows APIs, so
the over-generalisation is gone. **Closed.**

## D6 -- citations

`unix.rs:222-229` for the no-`Drop` invariant (packet `:73`) and `unix.rs:219` for the
reaper's `.map(drop)` (`:76`). Both verified against the file. The truth table's new Unix
citations are correct too: `unix.rs:93` is `let (master, slave) = open_pty()?`, `:144` is
the child's `libc::close(slave_fd)`, and `:131, 141` are `setsid()` and
`set_controlling_terminal`. **Closed.**

## D5 -- no action, and agreed

`intake_id` stays the literal `43`. The packet says so explicitly and the value is correct
for IN-0038; the recommendation was about method, not about this row.

## Gates

```text
$ # ci.yml:235-251, both stand-alone greps
GATE A PASS (no hits)   -- rustdoc, crates/vt/src
GATE B PASS (no hits)   -- embedder guide, crates/vt/docs/guide

$ RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps --all-features
    Finished `dev` profile ... Generated target/doc/oneterm_vt/index.html      [exit 0]

$ python scripts/vt-public-api.py --check --no-doc
public API surface unchanged (public-api.windows.txt)                          [exit 0]

$ python scripts/check-english.py
English contributor-text check passed for 919 files.                           [exit 0]

$ cargo fmt --all -- --check                                                   [exit 0]
```

`cargo fmt` was run because this commit touches a `.rs` file for the first time in the
packet (`unix.rs`, comment text only -- `git show --stat` reports 7 lines, and the change is
entirely inside the `//` block at `:222-230`). The public API surface being unchanged
confirms the same thing from the other side. The `[Platforms](#platforms)` link still
renders as `<a href="#platforms">Platforms</a>` against an `id="platforms"` heading.

No full `ci-local` was re-run, per instruction, and nothing here failed to require it.

## One observation

The `HARNESS:STATUS` block still shows `Implemented` rather than `Reopened (acceptance
rework)`. That block is machine-owned and mirrored from `harness.db`, which is out of bounds
from this worktree, so it cannot be moved here -- and the packet does record the rework in
prose, under "Acceptance rework, after the independent verification", with the correct
reasoning for why it is rework rather than a new bug. Worth a coordinator's attention when
the row is finally inserted, not a defect in the change.
