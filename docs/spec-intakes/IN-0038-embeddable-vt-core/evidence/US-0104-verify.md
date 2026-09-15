# Independent verification: US-0104, the PTY moves into `oneterm-vt`

Verifier: an adversarial session with no stake in the packet.
Date: 2026-09-15
Branch under test: `feat/vt-pty` @ `e83907f`, merge-base `a13002a`.
Worktree: `.claude/worktrees/agent-ad7de65e8741a2b0b` (nothing committed, nothing pushed).

## Verdict

**PASS-WITH-NOTES.**

The engineering claim holds under every check that can be run on this host. The move is a real
rename, the feature gate is real and provable from outside the workspace, the transport behaves
identically, and the full CI gate is green including `cargo deny`. Thirteen findings follow. None
of them is a defect in the transport or in the feature; eleven are record, documentation or
hygiene, one is a merge-integration risk created by `main` moving underneath the branch, and one is
a platform gap that only a Linux or macOS host can close.

## What was actually verified

| Claim | Result |
| --- | --- |
| `37ad3a5` is a pure rename, nine files, zero content delta | **True.** `git diff --raw -M` prints `R100` for all nine with identical blob ids on both sides. The commit also removes `crates/pty/Cargo.toml` (-24) and three lines from the root manifest (-6/+1), which its own message states. |
| No syscall / flag / handle-lifetime / wait change inside the moved code | **True.** Only `8206ad5` touches `crates/vt/src/pty`. Every hunk is one of: `crate::` -> `crate::pty::`, a log prefix `oneterm-pty` -> `oneterm-vt-pty`, a thread name, a doc comment, a test-only string marker, or a `missing_docs` line. No `unsafe` block, no `CreateProcessW` / `LoadLibraryW` / `TerminateProcess` / `WaitForSingleObject` argument, no handle order, no `CHILD_EXIT_GRACE` value changed. The two shortened markers stay self-consistent (`b"vt-pty-o"` is 8 bytes and `windows(8)` is asked for; `b"vt-pty-typed"` is 12 and `windows(12)`). |
| `pty` is default-on; `polling` public; `windows-sys` / `libc` optional under it | **True.** `crates/vt/Cargo.toml:19-31,42-51`. |
| `--no-default-features` tree = the six leaf deps | **True**, verbatim. |
| Default tree: 8 direct / 16 distinct on windows-msvc, 8 / 11 on linux-gnu | **True**, both measured (`--target x86_64-unknown-linux-gnu` needs no installed target for `cargo tree`). No `gpui*` and no `oneterm-*` in either. |
| 30 `#[test]` moved verbatim | **True.** 30 across the nine files; 24 of them compile on Windows, which is exactly `398 - 374`. |
| 398 default / 374 `--no-default-features` / 398 `vt-paranoid` | **True**, all three measured. |
| local-shell 33 passed incl. `session_orphan_tests` | **True** (35 collected, 2 ignored; both orphan tests green). |
| `oneterm-tools` switched to `oneterm_vt::pty`, no behaviour change | **True**, and `oneterm-vt.workspace = true` was already there (`crates/tools/Cargo.toml:60`). |
| No `oneterm-pty` reference outside the allowed places | **True.** The packet's own grep returns nothing; `crates/pty` is gone; `cargo metadata` has no such package. |
| OpenConsole bundle untouched in `crates/app/assets` | **True** for the assets. `crates/app/build.rs:10` did change - one doc-comment word, `oneterm-pty` -> `oneterm_vt::pty`. Benign, and required by the hygiene grep, but the claim "crates/app untouched" is not literally true. |
| The loader resolves the DLL from the running executable's directory | **True, and proved directly** - see "The bundled host" below. |
| Public API snapshot split, `--diff-platforms` shows exactly 6 pty-only lines | **True**, byte-for-byte the six lines the packet pastes. Negative test performed. |
| Package gate passes with `src/pty/**` in the list | **True**, and `cargo package -p oneterm-vt` (build form) exits 0. |
| Rustdoc clean under `-D warnings`, both platforms in prose | **True** for `--all-features`, default, and `--no-default-features`. |
| `pwsh scripts/ci-local.ps1 -Full` | **Green**, exit 0, including `cargo deny check licenses bans advisories` ("advisories ok, bans ok, licenses ok"). |
| Ten-launch GUI probe | Not run, as declared. Substantially replaced - see below. |

## Findings

### F1 (Medium, repository hygiene) `firecap.bin` was committed by accident

`firecap.bin` at the repository root, zero bytes, added by `339ac9c` ("docs(vt): US-0104 evidence,
and the three gaps the move turned up"). It is unrelated to the packet, it is now tracked, and it is
not in `.gitignore`.

```
$ git diff --raw a13002a..HEAD -- firecap.bin
:000000 100644 0000000 e69de29 A  firecap.bin
```

Delete it before the merge.

### F2 (Medium, record) The status block claims states the packet was never in

`docs/spec-intakes/IN-0038-embeddable-vt-core/US-0104-pty-in-core.md:12-18` ticks all six boxes,
including `Changed`, `Reopened (acceptance rework)` and `Retired`. The packet was never reopened and
is certainly not retired. Every sibling in this intake (`US-0097:10-15`, `BUG-0058:10-15`,
`US-0103:12-17`) ticks only `Planned` / `In progress` / `Implemented`. The block is a harness-synced
region, so this is not cosmetic.

### F3 (Medium, record) `E2E proof` is ticked for an E2E check the packet says it did not run

`US-0104-pty-in-core.md:338` ticks `[x] E2E proof`. The packet's only E2E acceptance criterion - the
ten-launch probe at `:152-157` - is `[ ]`, and its own "Gaps found while doing it" section at
`:559-571` states plainly that it was not run and that the `conpty: bundled` log line and the orphan
count are **unverified**. The two sibling packets that also could not run an E2E leave the box
unticked (`US-0097:238`, `BUG-0058:200`). This is the one place the record claims unverified
behaviour as working, and it contradicts the same document three hundred lines later.

### F4 (Medium, integration) The branch is six commits behind `main`, and one README sentence dies on merge

`main` is now `491dff8` ("Merge US-0100: scrollback search is the engine's, regex behind a
feature"); this branch forked at `a13002a`. `US-0100` and `US-0104` collide in six files:

- `crates/vt/README.md:155` on this branch says "**`pty` is the only feature that adds a
  dependency**". `main` already carries `6ef8ae3` ("fix(vt): the README claimed no feature adds a
  dependency; one does") for `regex`. After the merge that sentence is false.
- `crates/vt/Cargo.toml` `[features]`: `main` has `regex = ["dep:regex"]` and no `default` key; this
  branch adds `default = ["pty"]` and `pty = [...]`.
- `crates/vt/public-api.txt`: `main` regenerated it with the `search` surface; this branch renames it
  to `public-api.windows.txt` and hand-derives `public-api.unix.txt` from it. **Both snapshots have
  to be regenerated and the unix one re-derived after the rebase** - the hand derivation is against
  a surface that no longer exists.
- `crates/vt/CHANGELOG.md`, `crates/vt/src/lib.rs`, `scripts/ci-local.{ps1,sh}` and
  `.github/workflows/ci.yml` each carry edits from both packets.

Nothing here is wrong on the branch as it stands; it is a rebase that must be done with the README
sentence and the two snapshot files in hand, not a `-X ours`.

### F5 (Medium, documentation) `structure.md`'s `vt/` tree was not updated, against the LLD's own instruction

`low-level-design/pty.md` § "Rule changes" item 5 requires: "Directory tree: delete the `pty/` block;
extend the `vt/` entry with `src/pty/` ... noting the `pty` feature." The `pty/` block was deleted.
The `vt/` entry was not extended. `docs/agents/structure.md:181-201`:

- no `src/pty/` line anywhere in the tree;
- `:185` still lists `public-api.txt`, a file that no longer exists (it is now
  `public-api.windows.txt` plus `public-api.unix.txt`);
- `:188-189` still says `grid`, `intern` and `parser` are "the only modules another crate names by
  path", while `crates/vt/src/lib.rs:24-41` was changed in this very packet to say **four**, `pty`
  included.

The responsibility table rows (`:246`, `:248`, `:249`) *were* updated and are accurate, which is
presumably why the packet's acceptance line ("structure.md names `oneterm_vt::pty`") reads as
satisfied. `python scripts/check-doc-paths.py` does not look at this tree, so nothing caught it.

### F6 (Low, documentation) `scripts/README.md:14` still names `crates/vt/public-api.txt`

One row, one filename, now two files. Same omission as F5, different file.

### F7 (Low, evidence arithmetic) The LOC delta is `-73`, not `-97`

`US-0104-pty-in-core.md:508` says "the delta is **+110 / -97**, net **+37**". 110 - 97 is 13.
Measured:

```
$ git diff --numstat 8206ad5~1 8206ad5 -- crates/vt/src/pty
61 22  mod.rs        4 4 unix.rs      1 1 windows.rs
13 12  windows/child.rs   17 18 windows/conpty.rs   5 5 windows/pipe.rs
1  1   windows/pipe_tests.rs   8 10 windows/pseudo_console_tests.rs
-> +110 / -73, net +37
```

The net figure the packet also states is right; the removed count is not.

### F8 (Low, evidence) The by-suite test table is one test over, and contradicts its own total

`US-0104-pty-in-core.md:495-498` lists `pty::windows::conpty::tests` as **8**. There are **7**
(`crates/vt/src/pty/windows/conpty.rs:486,497,508,514,521,526,551`). With 8 the Windows-side suites
sum to 25, while the same paragraph's measured total two lines earlier is 24 - which is the correct
number, confirmed by `cargo test -p oneterm-vt --lib -- --list | grep -c '^pty::'` = 24. The
headline "30 `#[test]` before and after" is correct.

### F9 (Low, evidence, stale) The Rustdoc paragraph still describes the pre-split snapshot

`US-0104-pty-in-core.md:501` says the check passes "against a regenerated `crates/vt/public-api.txt`
whose diff is 28 added lines". `e83907f` replaced that file with two, and the same document's "The
platform split" section (`:513-545`) says so. The earlier paragraph was not updated.

### F10 (Low, rustdoc self-containment) The move carries two repository-path citations into the published crate

The packet's claim that the self-containment grep returns 0 is true. The grep's pattern is
`US-0\d{3}|BUG-0\d{3}|DEC-0\d{3}|IN-0\d{3}|docs/spec-intakes` (`scripts/ci-local.ps1:102`), so it
does not see a plain `docs/` path. Two survived the move into `crates/vt/src`:

- `crates/vt/src/pty/mod.rs:215` - on `OnResize::on_resize`, a **public trait method**, so it is in
  the rendered rustdoc an embedder reads: "(`docs/agents/error-policy.md`)".
- `crates/vt/src/pty/windows/pipe.rs:361`.

Neither resolves for a reader who does not have this repository, which is the rule's stated purpose.
The fix is the same one the packet applied thirteen times elsewhere: drop to `//`, or make it an
absolute repository URL. (The third hit, `crates/vt/src/terminal/terminal_tests.rs:1426`, is
pre-existing and inside `#[cfg(test)]`.)

### F11 (Low, external contract) The packaged crate inherits the workspace's `windows-sys` feature union

`target/package/oneterm-vt-0.5.2/Cargo.toml` flattens `windows-sys.workspace = true` into nine
features, two of which the transport does not use: `Win32_System_Diagnostics_ToolHelp` (that is
`crates/update`'s process sweep) and `Win32_Storage_FileSystem`. Harmless today - it is a superset -
but it means an embeddable crate's dependency feature set is owned by the workspace table rather
than by `crates/vt`, and a future crate leaving the workspace could silently change what
`oneterm-vt` ships. Worth one sentence in `packaging.md`, not a change here.

### F12 (Low, informational) The `polling` leak into `oneterm-terminal` is not merely feature unification

The LLD frames this as workspace feature unification. It is stronger than that:
`crates/terminal/Cargo.toml` depends on `oneterm-vt` with default features, so `pty` is on for
`oneterm-terminal` unconditionally, standalone, and every build of the adapter compiles the ConPTY
module:

```
$ cargo tree -p oneterm-terminal -e normal
oneterm-terminal -> oneterm-vt -> polling -> windows-sys 0.61.2
                              `-> windows-sys 0.59.0
```

R7's text already says only the explicit `--no-default-features` invocation proves the claim, so the
rule is honest. Recorded because a reader of the LLD's risk list would expect `-p oneterm-terminal`
alone to be clean.

### F13 (Low, unverifiable here) The Unix snapshot cannot be executed on this host

`crates/vt/public-api.unix.txt` is hand-derived and `rustup` here holds only
`x86_64-pc-windows-msvc`. Static review supports the derivation completely:

- `crates/vt/src/pty/unix.rs` has exactly three public inherent items -
  `SignalMask::current` (`:39`), `PseudoConsole::spawn` (`:88`), `PseudoConsole::child_pid` (`:193`).
  `SignalMask::apply` (`:61`) is private and the tuple field at `:35` is private, so no
  `structfield` line is owed; `impl fmt::Debug` (`:71`) is a trait impl and the script cuts the page
  at the first trait-impl heading.
- `mod.rs:60-61` re-exports exactly `PseudoConsole` and `SignalMask` under `cfg(unix)`;
  `Options::child_signal_mask` (`:139`) is the only cfg(unix) field.
- `SignalMask` derives `Clone, Copy, PartialEq, Eq` (`:34`), so `Options`'s
  `Clone, Debug, PartialEq, Eq, Default` derives all hold on Unix. This was the most likely way for
  the move to break a platform nobody compiled; it does not.

So the six-line delta is the right six lines. The residual risk is not the derivation but the rest
of the file, which is a byte copy of a Windows surface that `US-0100` has already changed on `main`
(F4). Mitigation already in place and worth stating: `.github/workflows/ci.yml`'s `vt-package` job
runs on `ubuntu-latest` (`:143`) and runs `vt-public-api.py --check --no-doc` against real Linux
rustdoc (`:197`), so the first CI run is a genuine check. It has not run - nothing is pushed.

## The bundled host: the ten-launch probe's gap is smaller than the packet thinks

The packet records that it could not observe which ConPTY host was resolved, because
`pty-throughput` installs no logger. It can be observed, from outside, without touching the running
application and without matching any process by name - inspect the loaded modules of a process you
spawned yourself:

```
$ cd target/debug            # where crates/app/build.rs already staged the pair
$ Start-Process .\pty-throughput.exe -ArgumentList 'cmd','/c','ping -n 6 127.0.0.1' -PassThru
spawned pid=25364
conpty.dll loaded from: ...\agent-ad7de65e8741a2b0b\target\debug\conpty.dll
exit=0
```

That is direct proof that the loader moved into `crates/vt` still resolves `conpty.dll` against the
**running executable's** directory and still prefers the bundled host over `kernel32`. It closes the
substantive half of the ten-launch probe. What remains open is only the orphan count across ten
open/close cycles *in the GPUI application*, and the `conpty: bundled` log line itself. The two unit
tests that pin the preference order are green:

```
test pty::windows::conpty::tests::conpty_api_prefers_the_bundled_host ... ok
test pty::windows::conpty::tests::conpty_api_falls_back_to_the_system_host ... ok
```

## The verifier's own tests

Written outside the OneTerm workspace on purpose (each carries its own `[workspace]` table), so they
consume `oneterm-vt` exactly as an external embedder does - public API only, no `super::`, no
`crate::`, no workspace feature unification.

- `verify-us0104/Cargo.toml`, `verify-us0104/tests/pty_contract.rs` - default features.
- `verify-us0104-nopty/Cargo.toml`, `verify-us0104-nopty/src/bin/engine.rs`,
  `verify-us0104-nopty/src/bin/usepty.rs` - `default-features = false`.

Results:

```
$ cargo test            # verify-us0104
test a_public_api_child_runs_reports_and_leaves_nothing_behind ...
    child pid 25860 exited: Some(ExitStatus(ExitStatus(0)))          ok
test dropping_a_live_console_ends_its_child_within_the_grace ...
    drop of a live console took 9.4258ms (pid 19132)                 ok
test resize_is_safe_on_both_sides_of_the_child_exit ...
    resize after child exit -> Ok(())                                ok
test result: ok. 3 passed; 0 failed
```

- **(a)** `cmd /c echo verify-us0104` spawned through `oneterm_vt::pty::PseudoConsole`, marker read
  back through `EventedReadWrite::reader`, `ChildEvent::Exited(Some(0))` observed through
  `EventedPty::next_child_event` without reading, and the pid gone after the drop. Every liveness
  question is asked with `tasklist /FI "PID eq <pid>"` - by pid, never by image name.
- **(b)** `on_resize` before the child exits returns `Ok(())`; after it exits it also returns
  `Ok(())` on this host. Either way it is an `io::Result` and not a panic, which is the contract in
  `crates/vt/src/pty/mod.rs:210-217`.
- **(c)** DEC-0016: a `PseudoConsole` whose child (`cmd /c ping -n 30`) is running and has already
  produced output was dropped. The drop returned in **9.4 ms** and the pid was gone - the "normal
  path" row of DEC-0016's table (~20 ms), two orders of magnitude inside the 2 s
  `CHILD_EXIT_GRACE`, so the escalation was not needed and the grace was not served. The
  never-started case (the one that *does* need `TerminateProcess`) is covered in-tree by
  `local-shell`'s `a_session_dropped_before_its_shell_starts_leaves_no_orphan`, which is green.
- **(d)** `Options { shell: Some(..), ..Options::default() }` compiles in an external crate with no
  cfg-gated field named. Portable embedder code is possible.
- **(e)** With `default-features = false`:
  - `cargo build --bin usepty` fails as required, and the diagnostic is the good one:
    `error[E0433]: could not find 'pty' in 'oneterm_vt'` ... `note: found an item that was
    configured out ... the item is gated behind the 'pty' feature`.
  - `cargo run --bin engine` prints
    `no-default-features engine fed and rendered: "hello no-pty                    "` - the engine
    builds, feeds and renders with no transport compiled.
  - `cargo tree -e normal` on that crate shows `oneterm-vt` over exactly the six leaves, from an
    external consumer's position rather than from inside the workspace.

## Commands run

```
git reset --hard feat/vt-pty                                  # e83907f
git show --stat -M 37ad3a5 ; git diff --raw -M 37ad3a5~1 37ad3a5
git diff --numstat -M 8206ad5~1 8206ad5 -- crates/vt/src/pty
git show -M 8206ad5 -- crates/vt/src/pty crates/vt/Cargo.toml crates/local-shell crates/tools
git diff --stat -M a13002a..HEAD ; git merge-base main HEAD
cargo tree -p oneterm-vt -e normal [--no-default-features] [--target x86_64-unknown-linux-gnu]
cargo tree -p oneterm-terminal -e normal
cargo test -p oneterm-vt --no-default-features                # 374 passed
cargo test -p oneterm-vt --lib -- --list | grep -c '^pty::'   # 24
cargo test -p oneterm-vt --lib -- conpty_api                  # 2 passed
cargo test -p oneterm-local-shell -- session_orphan           # 2 passed, 1 ignored
cargo test -p oneterm-tools                                   # green
cargo package -p oneterm-vt                                   # exit 0
cargo package -p oneterm-vt --list | grep -i -e conpty -e openconsole
RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps [--no-default-features|--all-features]
python scripts/vt-public-api.py --check --no-doc
python scripts/vt-public-api.py --diff-platforms              # 6 lines, exit 0
  + negative test: a bogus `struct oneterm_vt::BogusVerifierItem` appended to the unix file makes it
    exit 1 with "these are outside it"; file restored, `git status` clean for it afterwards.
pwsh scripts/ci-local.ps1 -Full                               # exit 0, "all checks passed"
target/debug/pty-throughput.exe cmd /c "ping -n 6 127.0.0.1"  # + module inspection, above
```

`ci-local -Full` log: the verifier's scratchpad, `ci-local-full.log` (20 428 lines). Step list:
fmt, clippy x2, `cargo test --workspace` (0 failures; `oneterm-vt` 398/2 ignored, `oneterm-local-shell`
33/2 ignored), `vt-paranoid` 398, the two feature-matrix builds, the six-leaf `cargo tree`
assertion, the headless example, `cargo doc --all-features`, `--check` (`public-api.windows.txt`
unchanged), `--diff-platforms`, the package-list gate ("reaches nothing outside crates/vt"), the
rustdoc self-containment grep, `verify-dependency-graph.py`, `check-doc-paths.py`, the
`check-english` unit tests and `check-english.py`, `completion-catalog.py validate`,
`third-party-notices.py --check` (**no regeneration**), and `cargo deny check licenses bans
advisories`.

## Processes spawned, and their fate

Only processes this session started were ever touched. Nothing was matched, listed or signalled by
image name; the running `oneterm.exe` and its console hosts were never enumerated, inspected or
contacted.

| What | Fate |
| --- | --- |
| `pwsh` running `ci-local.ps1 -Full` and its `cargo` / `rustc` / test children | exited, code 0 |
| `cargo` / `rustc` / `rustdoc` for the tree, test, package, doc and probe invocations | exited |
| `cmd.exe` pid **25860** (`/c echo verify-us0104`, my probe) | exited 0, pid confirmed gone |
| `cmd.exe` pid **19132** (`/c ping -n 30`, my probe, ended by the drop) | gone, confirmed |
| `cmd.exe` for `resize_is_safe_on_both_sides_of_the_child_exit` (pid not printed) | the test asserts the pid is gone; it passed |
| `tasklist.exe` children of the probe binary (`/FI "PID eq N"`) | synchronous, exited |
| `pty-throughput.exe` pid **25364** and its `cmd` / `ping` grandchildren and console host | exited 0, pid confirmed gone |

Post-run check, by pid: `25364 gone`, `25860 gone`, `19132 gone`.

## What could not be verified

1. **Anything on a Unix host.** No Linux or macOS target is installed and installing one writes
   outside the repository, so `crates/vt/src/pty/unix.rs` was read, not compiled; the unix snapshot
   was derived-checked by reading, not generated. F13.
2. **The ten-launch GUI probe**, in the sense of an orphan count across ten open/close cycles in the
   application, and the `conpty: bundled` log line. The owner runs this session inside OneTerm, so a
   second instance was not driven. The loader's runtime path resolution - the part that actually
   depended on the move - is proved above by a different route.
3. **The first CI run on this branch.** Nothing is pushed, so the `ubuntu-latest` `vt-package` job
   that would check `public-api.unix.txt` against real rustdoc has not executed.
4. **`harness.db`.** Not read and not written (no harness binary here, and the task forbids edits).
   Unlike `BUG-0058`, `US-0097`, `IN-0038` and `packaging.md`, **`US-0104` carries no harness
   `INSERT` snippet at all** - a `Handoff` section stands where the siblings put the row. Whether
   that is deliberate is a question for the owner; recorded here because the sibling pattern is
   consistent and this packet breaks it. `intake_id = 43` and the `*_proof` 0/1 columns could
   therefore not be checked against anything.

## Recommendation

Merge after: deleting `firecap.bin` (F1); correcting the status and proof blocks (F2, F3); the
rebase onto `main` with the README sentence and both snapshot files regenerated (F4); and the
`structure.md` tree (F5). F6-F12 are one-line fixes that can ride along or follow. F13 closes itself
on the first CI run.

---

## Final re-check at `a2e9e4c`

Branch rebased onto `491dff8`; `main` now `072560a`, one docs commit ahead. Same rules: pid-tracked
spawns only, nothing written outside the repository and the scratchpad, no waiter left behind.

**Verdict: PASS.** All thirteen findings are closed. Nothing new was found.

| | Re-check | Result |
| --- | --- | --- |
| F1 | `firecap.bin` | Gone from the tree; `.gitignore:34` carries `/firecap.bin`. |
| F2 | status block | Only `Planned` / `In progress` / `Implemented` ticked (`:12-17`). |
| F3 | proof block + harness row | `E2E proof` unticked (`:338`). The SQL row is present (`:681-692`): `intake_id 43`, proof columns `1, 1, 0, 1, 1` - unit, integration, **e2e 0**, platform, verify - matching the block exactly. |
| F4 | README + snapshots | README `:155-157` names **two** dependency-adding features, `pty` and `regex`. Both snapshots regenerated post-`US-0100` (5 `oneterm_vt::search` lines in each). `--diff-platforms` exits 0 with exactly the same six `pty` lines. Unix derivation re-spot-checked against `crates/vt/src/pty/unix.rs`: still exactly `SignalMask` (`:35`), `SignalMask::current` (`:39`), `PseudoConsole` (`:78`), `spawn` (`:89`), `child_pid` (`:193`) - nothing public was missed. |
| F5 | `structure.md` tree | `src/pty/` at `:203`, both snapshot files at `:185-186`, and the module-count line corrected to five (`:189-190`). |
| F6-F9 | docs and arithmetic | `scripts/README.md` row rewritten; `-73`; conpty `7` and the suites sum to 24; the rustdoc paragraph points at "The platform split". |
| F10 | widened grep | `grep -rnE '^\s*//[/!].*(crates/\|docs/)'` over `crates/vt/src`, minus `https://github.com/`, returns **zero**. The widened pattern is live in `scripts/ci-local.ps1:106`, `scripts/ci-local.sh:80`, `.github/workflows/ci.yml:222` and `AGENTS.md:124`. |
| F11 | `windows-sys` union | Recorded at `:618-622` with the trigger. The correction is right and mine was wrong: `Win32_Storage_FileSystem` **is** used (`ReadFile` / `WriteFile`, `pty/windows/pipe.rs:34`); the unused pair is `Win32_System_Diagnostics_ToolHelp` and `Win32_System_IO`. Verified by enumerating every `windows_sys::Win32::*` path in `crates/vt/src`. |
| F12 | fixed, not just recorded | Root `Cargo.toml:69` `oneterm-vt = { path = "crates/vt", default-features = false }`; `crates/local-shell/Cargo.toml:22` and `crates/tools/Cargo.toml:65` take `features = ["pty"]`. `cargo tree -p oneterm-terminal -e normal` shows `oneterm-vt` over the six leaves only - no `polling`, no `windows-sys`. An external embedder is unaffected: a crate outside the workspace taking `oneterm-vt` with default features still gets the module, spawned a real ConPTY child (pid 25904) and exited 0, and its own tree still shows `polling` and `windows-sys 0.59` under `oneterm-vt`. `cargo tree -p oneterm-vt -e normal` is still 16 distinct. |
| F13 | Unix snapshot | Still derived, still labelled so, still first checked by the `ubuntu-latest` `vt-package` job. Unchanged risk, unchanged mitigation. |

Adopted tests, in-workspace: `crates/vt/tests/pty_contract.rs` (`#![cfg(all(windows, feature =
"pty"))]`) **3 passed** inside `cargo test --workspace`; `crates/vt/tests/engine_without_pty.rs`
(`#![cfg(not(feature = "pty"))]`) collects 0 in the default build and **1 passed** under
`cargo test -p oneterm-vt --no-default-features --test engine_without_pty`. The compile-fail half
was dropped on purpose rather than pulling `trybuild` into an embeddable crate; the expectation it
encoded is still true and is recorded above (`E0433 ... gated behind the 'pty' feature`).

One observation, not a finding: no CI entry point runs `cargo test -p oneterm-vt
--no-default-features`, so `engine_without_pty.rs` is compiled by the `--no-default-features` build
step but never *executed* by the gate. It is a two-second step if anyone wants it to be.

`cargo test -p oneterm-local-shell`: **33 passed, 0 failed, 2 ignored**, `session_orphan_tests`
included. `pwsh scripts/ci-local.ps1 -Full`: **exit 0, "ci-local: all checks passed"**, zero
`test result: FAILED` lines, `cargo deny` "advisories ok, bans ok, licenses ok".

Processes spawned in this pass: the `ci-local` `pwsh` and its toolchain children (exited); the
external probe's `cmd.exe` pid **25904** and the three `pty_contract` children, each asserted gone
by the test that spawned it; `tasklist /FI "PID eq N"` helpers (synchronous). Nothing was matched or
listed by image name, and the running `oneterm.exe` was never touched. The scratch external crate
was deleted afterwards; the only file this session leaves in the worktree is this evidence file,
uncommitted.
