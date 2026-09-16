# Low-Level Design: PTY in the core

Intake: IN-0038
HLD: [`../high-level-design.md`](../high-level-design.md)
Topic: pty
Date: 2026-09-15

## Concern

The owner reversed intake decision (f) for the PTY on 2026-09-15:

> I still want a PTY in the core: provided by default, and overridable if wanted.

This document is what that sentence means in files, features, dependencies and rules. It covers
where `crates/pty` goes, the cargo feature that carries it, what "overridable" is and is not, the
dependency arithmetic in both feature modes, the effect on R1-R12, what happens to the bundled
Windows console host, and what a hostile verifier runs to check all of it.

The reversal is **PTY only**. Clipboard backends, `TerminalSecurityPolicy`, URL policy and GPUI stay
outside the core exactly as decision (f) says, and nothing in this document weakens the HLD's line:
*the core reports, the embedder decides*. A pseudo-console reports bytes and a child exit; it
decides nothing.

## What moves

`crates/pty` is deleted as a crate and becomes a module of `crates/vt`. The move is a file move plus
a feature gate; no function body changes.

| From | To |
| --- | --- |
| `crates/pty/src/lib.rs` | `crates/vt/src/pty/mod.rs` |
| `crates/pty/src/unix.rs` | `crates/vt/src/pty/unix.rs` |
| `crates/pty/src/windows.rs` | `crates/vt/src/pty/windows.rs` |
| `crates/pty/src/windows/child.rs` | `crates/vt/src/pty/windows/child.rs` |
| `crates/pty/src/windows/conpty.rs` | `crates/vt/src/pty/windows/conpty.rs` |
| `crates/pty/src/windows/pipe.rs` | `crates/vt/src/pty/windows/pipe.rs` |
| `crates/pty/src/windows/pipe_tests.rs` | `crates/vt/src/pty/windows/pipe_tests.rs` |
| `crates/pty/src/windows/pseudo_console_tests.rs` | `crates/vt/src/pty/windows/pseudo_console_tests.rs` |
| `crates/pty/src/loopback_tests.rs` | `crates/vt/src/pty/loopback_tests.rs` |
| `crates/pty/Cargo.toml` | deleted; its four dependency lines become optional entries in `crates/vt/Cargo.toml` |

Measured on `main` @ `92ae9a6`: **2 759 lines** across nine files, of which 492 are the three
dedicated test files. Nothing is rewritten, so the intake's LOC delta for this packet is **net zero**
plus about 30 lines of manifest, module header and `missing_docs` prose.

`crates/vt/src/lib.rs` gains exactly one line:

```rust
#[cfg(feature = "pty")]
pub mod pty;
```

`pub mod`, not root re-exports. Same reason the HLD gives for `input` and `search`: it is a
self-contained namespace an embedder wants to `use` wholesale, and `PseudoConsole` at the crate root
would read as if the crate were a process launcher. An embedder writes
`use oneterm_vt::pty::{PseudoConsole, Options, EventedPty};`.

### What does not move

`crates/local-shell` keeps every line it has. `ShellEventLoop`, the `Poller` it owns, the
`MAX_LOCKED_READ` cap, the `ByteBudget` write cap, the render-demand handover and the
`LocalSession::shutdown_owner` reaper thread are OneTerm's adapter policy, not terminal semantics.
`ChildExitWatcher` and its `DEC-0016` two-second grace **do** move, because they live in
`crates/pty/src/windows/child.rs` and are part of the pseudo-console's own drop contract, not the
adapter's. Their behaviour is unchanged by the move and `DEC-0016` stays accepted and in force; only
the file path in that decision's text needs updating.

## The feature

rio-vt's shape, copied deliberately:

```toml
[features]
# The pseudo-console transport (ConPTY on Windows, openpty on Unix). On by
# default: a terminal core that cannot open a terminal is a surprise. VT-only
# embedders that already own their transport turn it off.
default = ["pty"]
pty = ["dep:polling", "dep:windows-sys", "dep:libc"]
vt-paranoid = []
# `regex` arrives with US-0100. There is no `serde` feature (Open Decision 3,
# ruled against). Before this packet there was no `default` line at all.

[dependencies]
# PUBLIC dependency, and only under `pty`: Poller, Event and PollMode appear in
# the EventedReadWrite signatures, so a `polling` major bump is a breaking
# change to this crate's API.
polling = { workspace = true, optional = true }

[target.'cfg(windows)'.dependencies]
windows-sys = { workspace = true, optional = true }

[target.'cfg(unix)'.dependencies]
libc = { workspace = true, optional = true }
```

Naming a target-specific optional dependency in a feature list is legal and is what `rio-vt` does
(`pty = ["dep:corcovado", "dep:teletypewriter"]`, both declared under
`[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`). On a target where the table does not
apply, the `dep:` entry is a no-op.

`log` is already unconditional in `crates/vt` and stays there; the pty module's single `info` line
naming the resolved ConPTY host costs nothing new.

**This breaks the HLD's old rule that "default features add nothing".** That rule is replaced, not
bent: see [Dependency budget](#dependency-budget) and the replacement sentence in
[Rule changes](#rule-changes-r1-r12-and-structuremd).

## What "overridable" means

This is the part of the owner ruling that needs a decision rather than a transcription, and the
decision is not the obvious one.

### The seam that exists at every feature setting

`Terminal` has never known what a transport is. Its input is `feed(&[u8], &mut EventBatch, Instant)`
and its output is `snapshot_update(&mut SnapshotState)`; its only size input is
`resize(Size, ResizePolicy)`. Bytes from a socket, a file, a test vector or a pseudo-console are
indistinguishable to it. **That is the real "bring your own transport" seam, it is feature
independent, and it is the one an embedder with an existing process model uses.** An embedder who
builds with `--no-default-features` does not lose the ability to drive the engine from their own
transport; they lose a pseudo-console implementation they did not want.

### The seam inside the feature

`EventedReadWrite`, `EventedPty` and `OnResize` are the swap point for the *transport
implementation*: `crates/local-shell`'s whole event loop is generic over `EventedPty + OnResize`, and
`crates/vt/src/pty/loopback_tests.rs` already drives it against a pair of TCP sockets with no
pseudo-console anywhere. That file is the proof the traits describe a shape a caller can implement,
and it moves with the code.

### Decision: the traits are gated with the feature, not feature independent

The packet brief asks whether the traits should live in a feature-independent module so an embedder
can implement them without the platform code compiled. **They should not, and with today's
signatures they cannot.**

```rust
unsafe fn register(&mut self, poller: &Arc<Poller>, interest: Event, mode: PollMode) -> io::Result<()>;
```

`polling::Poller`, `polling::Event` and `polling::PollMode` are in the trait's own signature. A
trait that names types from an optional dependency cannot compile when that dependency is absent, so
"feature-independent traits" would force `polling` to be unconditional -- which costs the
`--no-default-features` build its six-dependency headline in exchange for three traits nobody can
usefully implement without a poller anyway. A trait set you cannot compile without the dependency it
names is not feature independent in any sense a user benefits from.

So: `mod pty` carries the traits and the implementations together, and both disappear under
`--no-default-features`. The crate then offers exactly one transport contract, `feed`, which is the
honest state of affairs and is what the guide chapter says.

**Considered and rejected: strip `polling` from the trait signatures.** Make `register` /
`reregister` / `deregister` inherent methods on the concrete `PseudoConsole` types and reduce the
traits to `reader` / `writer` / `next_child_event` / `on_resize`. The traits would then be pure
`std` and could live outside the feature, and `polling` would stop being a public dependency -- which
also removes a semver hazard from a crate other projects now pin by git tag. It is rejected **for this packet**
because it is an API redesign of the thing being moved, it breaks `crates/local-shell`'s event loop,
and it turns a net-zero move into a rewrite; this intake's rule is that a packet changes crate or
size, never both. Revisit trigger: **the first `polling` major bump after an external project pins
`oneterm-vt` by tag**,
or a second embedder asking for the traits without the feature. Either makes the redesign cheaper
than the alternative, and it is then a packet of its own.

## Dependency budget

Measured on `main` @ `92ae9a6` with `cargo tree -e normal`.

| Build | Direct | Distinct crates in the tree | Tree lines |
| --- | --- | --- | --- |
| `--no-default-features` (any target) | **6** | **6** | 7 |
| default, `x86_64-pc-windows-msvc` | 8 | 16 | 17 |
| default, `x86_64-unknown-linux-gnu` | 8 | 11 | 12 |

`--no-default-features` is bit-for-bit today's `oneterm-vt`: `bitflags`, `log`, `memchr`,
`rustc-hash`, `unicode-segmentation`, `unicode-width`.

Default adds, on Windows: `polling 3.11.0` and `windows-sys 0.59.0` directly, and through them
`cfg-if`, `concurrent-queue`, `crossbeam-utils`, `pin-project-lite`, `windows-sys 0.61.2`,
`windows-link`, `windows-targets`, `windows_x86_64_msvc`. Ten new crates. Two `windows-sys` majors
coexist -- `0.59` is the workspace pin this code uses, `0.61.2` is `polling`'s own -- and that is
already true of OneTerm's graph today, so the move adds no duplicate that was not there.

Default adds, on Linux: `polling 3.11.0` and `libc 0.2.186` directly, and through them `cfg-if`,
`rustix`, `linux-raw-sys`. Five new crates; `bitflags` is shared with the engine and is not counted
twice.

For OneTerm's own graph the delta is **zero in both directions**: `polling`, `windows-sys` and
`libc` are already in the app's tree through `crates/local-shell` and `crates/tools`, and
`THIRD-PARTY-NOTICES.md` already lists all three. `python scripts/third-party-notices.py --check`
must still pass with no regeneration; the packet asserts that rather than assuming it.

The claim the crate is still entitled to make, and the one the README must make instead of the old
one: **`oneterm-vt` with `--no-default-features` is six leaf dependencies, and that build is a CI
target.** `alacritty_terminal` has no way to reach that number and `rio-vt`'s `default = ["pty"]`
sits on fifteen unconditional dependencies before its own feature set opens.

## Rule changes: R1-R12 and structure.md

Three pieces of repository policy stop being true the moment `pty` is a feature of `vt`. Each
replacement sentence below is the exact text the packet writes.

### 1. The L0 layer line and paragraph, `crate-dependency-rules.md`

Replace the L0 row of the layer diagram:

```
L0  core · terminal · completion · highlight · vt   (domain + engine + leaf; core = pure domain)
```

and replace the paragraph that follows it with:

> Within L0, `terminal` and `completion` depend on `core` (the pure-domain leaf); `highlight` is a
> leaf, and so is `vt` (`oneterm-vt`, `IN-0029`, OneTerm's own VT engine): it depends on **no**
> OneTerm crate at all, which is stricter than R2 requires. Since `US-0104` `vt` also carries the
> pseudo-console transport, in `src/pty/`, behind its **default-on `pty` cargo feature**; there is no
> separate `oneterm-pty` crate. `crates/terminal` depends on `vt` since `US-0081` and on nothing else
> for its engine: the vendored fork went at `US-0087`. `crates/tools` (`oneterm-tools`, developer
> diagnostics: DOOM-fire workload, raw PTY throughput probe, VT parity and bench harness) is a
> workspace member **outside** the layering: nothing depends on it, and it may only reach down to the
> L0 leaf `vt`.

### 2. R7, `crate-dependency-rules.md`

Replace the whole R7 row with:

> | **R7** | **The engines are gpui-free, and `vt` is OneTerm-free in every feature combination.**
> `terminal` is coupled to `oneterm-vt` but MUST NOT depend on `gpui` / `gpui-component`;
> `completion` and `highlight` depend on no terminal engine at all. `vt` depends on no OneTerm crate
> under any feature set. `vt` is **not** unconditionally platform-free: its default-on `pty` feature
> adds `polling` plus `windows-sys` (Windows) or `libc` (Unix), and those three are the **only**
> platform dependencies any `vt` feature may add. `cargo build -p oneterm-vt --no-default-features`
> must stay at the six leaf dependencies. | The engines are reusable and unit-testable without a UI;
> an embedder who owns its own process model must be able to take the engine without the transport. |
> `cargo tree -p oneterm-vt -e normal` shows no `gpui*` and no `oneterm-*`;
> `cargo tree -p oneterm-vt -e normal --no-default-features` lists exactly `bitflags`, `log`,
> `memchr`, `rustc-hash`, `unicode-segmentation`, `unicode-width`. |

### 3. R6 and R8, `crate-dependency-rules.md`

R6's verification clause loses a crate that no longer exists. Replace `No gpui, no gpui-component, no
oneterm-pty, no oneterm-vt.` with `No gpui, no gpui-component, no oneterm-vt.`, and its "How to
verify" cell with `cargo tree -p oneterm-core -e normal shows no gpui* and no oneterm-vt.`

R8 names the backends' allowed dependency set. Replace `depend on **only** `core` + `terminal` +
`pty` (+ their protocol crates)` with `depend on **only** `core` + `terminal` + `vt` (+ their
protocol crates)`.

### 4. The full-graph verification block

Delete the `cargo tree -p oneterm-pty` line and replace the `vt` line with two:

```bash
cargo tree -p oneterm-vt -e normal                        # R7: no gpui*, no oneterm-*
cargo tree -p oneterm-vt -e normal --no-default-features  # R7: exactly six leaf deps
```

### 5. `structure.md`

- Directory tree: delete the `pty/` block; extend the `vt/` entry with `src/pty/` (`mod.rs` =
  `Options` / `Shell` / `WindowSize` / `GlyphWidth` plus the three traits; `windows.rs` +
  `windows/{conpty,pipe,child}.rs`; `unix.rs`), noting the `pty` feature.
- Responsibility table: delete the `pty` row; fold its text into the `vt` row and add the feature
  list to the dependency column. Change the `local-shell` row's `Depends on` from `core`,
  `terminal`, `pty` to `core`, `terminal`, `vt`, and the `tools` row's from `oneterm-vt`,
  `oneterm-pty`, ... to `oneterm-vt`, ... .

### 6. Machine-checked policy

`scripts/dependency-graph-policy.json`: drop `"crates/pty"` from `workspace_members`, drop the
`"oneterm-pty": []` entry, set `"oneterm-local-shell": ["oneterm-core", "oneterm-terminal",
"oneterm-vt"]` and `"oneterm-tools": ["oneterm-vt"]`. `python scripts/verify-dependency-graph.py`
is the gate; it also enforces workspace version inheritance, which is unaffected.

## Consumers

### `crates/local-shell`

`oneterm-pty.workspace = true` becomes `oneterm-vt.workspace = true` (default features, so `pty` is
on). Three files change their `use` line and nothing else:

| File | Change |
| --- | --- |
| `src/event_loop.rs:26` | `use oneterm_pty::{...}` -> `use oneterm_vt::pty::{...}` |
| `src/event_loop_tests.rs:9` | same |
| `src/session.rs:12,57` | same, including `oneterm_pty::GlyphWidth::WcsWidth` |
| `src/transport.rs:11` | same |

This adds an L3 -> L0 edge `local-shell -> vt`, which R2 allows and which the graph already has via
`local-shell -> terminal -> vt`; it becomes direct. `polling` stays a direct dependency of
`local-shell` because the adapter owns the `Poller`, and `oneterm-terminal`'s dependency on
`oneterm-vt` is untouched.

### `crates/tools`

Already depends on `oneterm-vt`. Drop `oneterm-pty.workspace = true`, rewrite
`src/bin/pty-throughput.rs:21`, and correct the comment at `Cargo.toml:52`. `polling` stays: the
throughput probe drives its own poller for the same reason `local-shell` does.

### Workspace-level

- Root `Cargo.toml`: drop `"crates/pty"` from `members`, drop `oneterm-pty = { path = "crates/pty" }`
  from `[workspace.dependencies]`, drop `oneterm-pty = { opt-level = 3 }` from
  `[profile.fast-dev.package]`. The transport keeps its optimization for free: `oneterm-vt` is
  already listed at `opt-level = 3` in the same table, and the pty code is now part of it.
- `deny.toml:95`: the `portable-pty` ban reason names `oneterm-pty`; retarget it to
  `oneterm_vt::pty`.
- `docs/agents/dependencies.md`: the "Local shell PTY" row, the `oneterm-vt` row, the "Windows FFI"
  row and the `### oneterm-pty's direct dependencies (US-0071)` heading all name the crate. The
  section becomes `### oneterm-vt's pty feature dependencies (US-0071, US-0104)` with the same four
  rows and the added sentence that all three are optional and gated.
- `docs/architecture.md`, `docs/PROJECT.md`, `docs/terminal-backend.md`: one row or one sentence
  each.

## The bundled Windows console host

**The packet brief's premise is wrong and the correction matters**: there is no `crates/pty/windows/`
asset directory. `crates/pty` contains no binary at all. The bundled pair lives in the **application**
crate:

| File | Size | Owner |
| --- | --- | --- |
| `crates/app/assets/conpty.dll` | 107.3 KB | `crates/app` |
| `crates/app/assets/x64/OpenConsole.exe` | 1.0 MB | `crates/app` |
| `crates/app/assets/conpty-manifest.json` | 804 B | `crates/app` |

About **1.1 MB** total, matching the figure `DEC-0013` records.

What `crates/pty` holds is the *loader*, not the payload: `windows/conpty.rs` calls `LoadLibraryW`
on a `conpty.dll` sitting next to the **running executable** and falls back to
`kernel32!CreatePseudoConsole` when it is absent (`DEC-0013` clause 2). That is runtime path
resolution against the host process's directory. It is entirely independent of which crate the
source file is compiled into.

So the move changes **nothing** about the bundle:

- `crates/app/build.rs` still copies `conpty.dll` and `x64/OpenConsole.exe` into
  `target/<profile>/`, still keyed off `crates/app/assets/`. Not touched.
- `scripts/bump-conpty.ps1`, `crates/app/assets/conpty-manifest.json` and `DEC-0013`'s bump
  procedure are untouched.
- `scripts/build-release.ps1` still stages the pair. `crates/update`'s installer still terminates
  only `OpenConsole.exe` images inside OneTerm's own install directory (`DEC-0005`). Untouched.

### Packaging decision: the bundle stays out of the packaged crate

`US-0097` shipped the gate this has to satisfy, and it is one command:

```bash
cargo package -p oneterm-vt --list | python scripts/verify-dependency-graph.py --package-list -
```

It asserts the packaged list carries `README.md`, `CHANGELOG.md`, `LICENSE`, `NOTICE` and
`examples/headless.rs`, reaches nothing outside `crates/vt`, and keeps every path inside 150
characters. **`US-0104` passes it unchanged and excludes nothing to do so.** Everything the packet
adds is ordinary source inside `crates/vt/src/pty/`; the longest new path,
`crates/vt/src/pty/windows/pseudo_console_tests.rs`, is 48 characters; and none of the required
files is touched.

There is nothing to exclude because there is nothing to exclude *from*: `crates/pty` holds no
binary, only the loader, and `cargo package` never walks `crates/app/assets/`. The packet still
greps the list for `conpty` and `openconsole` and requires no Microsoft binary, because "it happens
to be true" and "it is asserted" are different things, and the cheap assertion is what survives
somebody helpfully copying the assets into `crates/vt/assets/` one day.

Three reasons the bundle must stay out if anyone proposes moving it, in order:

1. **Licence surface.** Redistributing 1.1 MB of Microsoft binaries is a promise about provenance,
   Authenticode signatures, hashes and an MIT notice. `DEC-0013` makes that promise for OneTerm's
   *releases*, where `scripts/bump-conpty.ps1` and `python scripts/third-party-notices.py --check`
   police it. None of that machinery travels with a packaged crate or a git tag, and a crate that
   silently ships signed vendor binaries is a supply-chain surprise.
2. **Platform.** A consumer vendors the crate on whatever platform they build on. Two Windows
   x86-64 binaries in a crate an embedder builds on Linux or arm64 are 1.1 MB of dead weight in
   every vendored copy, forever, on every tag.
3. **It would not work anyway.** The loader resolves `conpty.dll` next to the *running executable*,
   not next to the crate source or the `OUT_DIR`. A file inside the package is not on that path
   unless the embedder's own build script copies it -- which is exactly the twenty lines
   `crates/app/build.rs` already is, and which the embedder must own because only they know where
   their executable lands.

The consequence is stated plainly rather than hidden: **an embedder using `oneterm-vt`'s `pty`
feature on Windows gets the inbox `conhost.exe` unless they bundle their own ConPTY pair.** The
inbox host swallows Sixel DCS payloads on Windows 11 10.0.26100.1 (`DEC-0013`, Context), so an
embedder who wants Sixel in a local shell must do what OneTerm does. The module rustdoc, the README
feature table and guide chapter 13 all say so, and each points at `DEC-0013` by absolute repository
URL (the self-containment rule's allowed form). Since the crate is consumed as a git dependency on
this repository (owner ruling 2026-09-15), that reader can at least open
`crates/app/build.rs` in the same checkout and copy the twenty lines.

### Licence verification

Verified on `main` @ `92ae9a6`, not assumed:

- `THIRD-PARTY-NOTICES.md` section 1 lists both files with version `1.24.2607.10001`, the NuGet
  source URL, per-file SHA-256, and the **full MIT licence text** of Windows Terminal (lines 13-42).
- `NOTICE` line 12 carries the matching summary clause.
- Both are generated from / checked against `crates/app/assets/conpty-manifest.json` by
  `python scripts/third-party-notices.py --check`, which is already in CI and in
  `scripts/ci-local.{sh,ps1}`.

None of those three files moves, so the notice obligation is already discharged and stays
discharged. The packet re-runs the check as evidence.

## Threading and IO model

The text guide chapter 13 and the module rustdoc must both carry, because it is the thing an
embedder gets wrong first:

- **Evented, not async.** The transport is a passive pollable object. It exposes registration into a
  `polling::Poller` the **embedder** owns, and it never runs a read loop. There is no async runtime,
  no executor, no `Future`, no spawned task on the hot path. `oneterm-vt` with `--all-features`
  still has no runtime dependency.
- **The embedder drives.** The loop is: `poller.wait()`, then read from `reader()` when
  `PTY_READ_WRITE_TOKEN` is readable, then `terminal.feed(bytes, &mut batch, now)` under whatever
  lock the embedder chose, then drain the batch, then `next_child_event()` when
  `PTY_CHILD_EVENT_TOKEN` fires. `crates/local-shell/src/event_loop.rs` is the worked example and
  the guide cites it.
- **Two threads exist inside the transport, and they are not the embedder's.** Windows uses a
  reader thread and a writer thread over the ConPTY pipes (`pty/windows/pipe.rs`); Unix uses one
  reaper thread, blocked in `Child::wait`, that turns child exit into a pollable event
  (`pty/unix.rs`) -- **not** a `SIGCHLD` handler, which is process-global and would fight every
  other runtime in the process. Both are internal and neither calls into embedder code, and
  **neither is joined**: the `JoinHandle` is dropped at spawn on both platforms, the Windows pair
  is parked in a blocking pipe read or write until the pipe breaks, and the Unix reaper
  deliberately outlives the drop because owning the child is what keeps its exit observable. This
  is the only place the crate spawns a thread, and it is behind the `pty` feature -- so
  `--no-default-features` keeps the engine's "no threads, no locks, no interior mutability, no
  callbacks" guarantee literally true.
- **Drop is an external side effect, and on Windows it blocks.** Dropping a `PseudoConsole` on
  Windows closes the console, then waits up to `CHILD_EXIT_GRACE` (2 s, `DEC-0016`) for the child
  and terminates it if it never exits: a bounded stall, so it must not run on a UI thread. OneTerm
  hands that drop to a detached owner thread (`LocalSession::shutdown_owner`); an embedder must do
  something equivalent. On Unix there is no `Drop` impl at all -- closing the master side lets the
  line discipline `SIGHUP` the child's foreground process group, the crate never signals the child
  itself (the reaper's `wait` reaps the pid the moment the child exits, so a signal from the drop
  could reach whatever the kernel handed that pid to next), and nothing waits. The rustdoc and guide must
  state both halves per platform, never one as if it were both.
- **`FeedStats`, not a frame clock.** Nothing about the transport changes the pull-side read model.
  The engine still never pushes.

## Guide and documentation

`US-0103` grows from twelve chapters to **thirteen**: `crates/vt/docs/guide/13-pty.md`, module
`ch13_pty`, gated on this packet.

Appended rather than inserted after chapter 3, where the subject logically sits, because rustdoc
orders the chapter list alphabetically by `chNN_` module name and inserting would renumber ten files
and ten module names for a reading-order gain that costs a churn diff across the whole guide. The
cost is that the optional feature is read last; that is acceptable precisely **because** it is
optional -- chapters 1 to 12 are true at every feature setting, and a reader who never enables `pty`
never needs chapter 13.

Chapter 13's job: the feature and why it is on by default; the threading and IO model above; the
`EventedReadWrite` / `EventedPty` / `OnResize` contract and the loopback implementation as the
worked override; what `--no-default-features` gives you instead (`feed` is the seam); the Windows
ConPTY host situation and the missing bundle; the blocking drop and `CHILD_EXIT_GRACE`; and the
`polling` public-dependency semver hazard.

Chapter 1 gains one row: the comparison table's "PTY in the core" line changes from "no -- separate
`oneterm-pty`" to "yes, default-on feature, and the only one of the three where turning it off
leaves a six-dependency crate".

## Acceptance

Every line below is a command. Full checklist with results lives in
[`../US-0104-pty-in-core.md`](../US-0104-pty-in-core.md).

```bash
# Both feature modes build.
cargo build -p oneterm-vt                          # default: pty on
cargo build -p oneterm-vt --no-default-features    # engine only
cargo build -p oneterm-vt --all-features

# The dependency headline, verbatim six lines plus the root.
cargo tree -p oneterm-vt -e normal --no-default-features

# The moved tests run where they landed.
cargo test -p oneterm-vt --features pty            # includes loopback_tests
cargo test -p oneterm-vt --no-default-features     # the whole engine suite, no transport

# The crate is gone and nothing names it.
grep -rn 'oneterm.pty\|oneterm_pty' --include='*.rs' --include='*.toml' \
  --include='*.md' --include='*.json' --include='*.ps1' --include='*.sh' \
  --include='*.yml' crates/ docs/ scripts/ .github/ Cargo.toml deny.toml
test ! -d crates/pty

# The package gate US-0097 shipped, with the pty module in and nothing excluded.
cargo package -p oneterm-vt --list | python scripts/verify-dependency-graph.py --package-list -
cargo package -p oneterm-vt
# No Microsoft binary in the list; the only hit is the source file conpty.rs.
cargo package -p oneterm-vt --list | grep -i -e conpty -e openconsole

# OneTerm is unchanged.
cargo test -p oneterm-local-shell -p oneterm-terminal
python scripts/verify-dependency-graph.py
python scripts/third-party-notices.py --check
pwsh scripts/ci-local.ps1
```

`cargo build -p oneterm-vt --no-default-features --examples` is already in CI: `US-0097` added it to
`scripts/ci-local.{sh,ps1}` and `.github/workflows/ci.yml`. This packet **tightens** that step with
the `cargo tree` assertion beside it rather than adding a second build. That assertion is now the
only thing that keeps the six-dependency claim honest, because until `US-0104` the default build and
the `--no-default-features` build were the same configuration and the claim could not drift.

## Risks and gaps

- **`polling` is now a public dependency of a crate other projects pin by git tag.** A `polling` major bump
  is a breaking change to `oneterm-vt`'s API and therefore a minor version bump under the pre-1.0
  promise in [`api-surface.md`](api-surface.md). This is the strongest argument for the rejected
  redesign above and it is why that redesign has a named trigger rather than a shrug.
- **The default build no longer proves "no platform code".** Anyone reading
  `cargo tree -p oneterm-vt` without `--no-default-features` sees `windows-sys` and concludes the
  engine is platform-coupled. The README, the module map and R7 all have to carry the qualifier now,
  and one of them will eventually be missed. The `--no-default-features` CI step is the only thing
  that keeps the claim honest.
- **Feature unification.** Any workspace crate depending on `oneterm-vt` with default features turns
  `pty` on for **every** crate in that build, `oneterm-terminal` included. It is harmless here
  (`oneterm-terminal` is in the same binary as `oneterm-local-shell`, which needs the transport
  anyway) but it means `cargo tree -p oneterm-terminal` inside the workspace will show `polling`,
  and a reviewer checking R7 by eye can be misled. Only the explicit
  `cargo tree -p oneterm-vt -e normal --no-default-features` proves the claim.
- **Windows CI is the only place the ConPTY path runs.** `pseudo_console_tests.rs` and
  `pipe_tests.rs` are Windows-only and spawn real processes; they move unchanged and keep whatever
  coverage they have today. Unix `openpty` coverage stays what it is -- the loopback tests, which
  touch no pseudo-console.
- **`missing_docs`.** `US-0097` turns on `#![warn(missing_docs)]`. The moved module has undocumented
  `pub` items (`Shell::new`, several `Options` and `WindowSize` fields). About 30 doc lines; the
  packet budgets them. If `US-0104` lands before `US-0097`, they are `US-0097`'s work instead, and
  the packet says so rather than leaving the order to chance.
- **The move is unreviewable as a single diff** if the file move and the feature gate are one
  commit. The packet requires two: commit one is `git mv` plus the module wiring with no content
  change (so `git log --follow` and `-M` show a pure rename), commit two is the manifest, the
  feature gate and the consumer edits.
