# BUG-0080 independent verification

- Packet: [`BUG-0080`](../BUG-0080-child-shells-inherit-another-terminals-identity.md)
- Commit under test: `b883488b` (branch `fix/child-env-terminal-identity`, one commit on main
  `e8b16a74`)
- Date: 2026-09-25, Windows 11 Enterprise 10.0.26200, own worktree and own target dir,
  `CARGO_BUILD_JOBS=2`, no `target/release` present.

## Verdict

**PASS.** The fix is in the right place, behaves as claimed on Windows (unit tests and a real
spawn), the Unix path reads correctly, the vt public API is unchanged and the gate is green.
There are no High findings. Two Low code findings (F1, F2) and three record findings (F4-F6)
are worth fixing before the packet is accepted; none of them blocks the merge.

## 1. Design placement

Judgement: **dropping the variables in `oneterm-vt` is right.**

- `env.rs` only builds the override map (`Options::env`). The transport writes that map ahead
  of the parent (Windows) or on top of it (Unix `Command::env`). Neither can express "remove",
  so a fix limited to `env.rs` could not work without a new public field.
- Precedent: `git log -S XDG_ACTIVATION_TOKEN` shows `f7291075` (US-0071, when the transport
  moved into the crate) already dropped `XDG_ACTIVATION_TOKEN` / `DESKTOP_STARTUP_ID` inside the
  transport. This commit widens the same rule into one list, `DROPPED_PARENT_ENV`, and uses it
  on both platforms.
- Contract: before this commit the `Options::env` rustdoc said "Environment entries applied on
  top of the parent environment", which implies that the child gets the parent environment
  plus `Options::env`. The new rustdoc names every dropped variable. `CHANGELOG.md` records it
  under `[Unreleased]` / Changed as "Behaviour, no signature", with the full list, the Unix
  change for the two startup tokens (an explicit entry now wins, where before it was removed)
  and the always-explicit Windows block. Under the promise (clause 6), only reply bytes, encoder
  bytes and `FeedStats` count as behaviour contracts. The child environment is neither, so this
  needs no version bump of its own. It ships in whatever the next tag is. `vt-public-api --check`
  is unchanged.
- Trade-off: suppose an embedder runs inside Windows Terminal and wants `WT_SESSION` kept. The
  child runs in a new pseudo-console that the embedder renders, not Windows Terminal. So
  `WT_SESSION` would be a false claim there as well. An embedder that still wants it can use
  `options.env.insert("WT_SESSION", std::env::var("WT_SESSION")?)`, because an explicit entry
  wins over the drop. This workaround is one line and documented. Every name on the list is
  "another terminal's identity" (WT, JetBrains, iTerm2, ConEmu, generic `TERM_PROGRAM*`),
  plus the two single-use startup tokens.
- `TERM_PROGRAM` / `TERM_PROGRAM_VERSION`: OneTerm sets both through `base_env()`. The manual
  run shows `TERM_PROGRAM=OneTerm` and `TERM_PROGRAM_VERSION=0.6.5`. An embedder that sets
  neither gives its child no `TERM_PROGRAM`. That is honest, and the rustdoc, the guide
  (ch. 13) and the CHANGELOG all tell embedders to set it.

## 2. Windows

- Block construction (`conpty.rs` `environment_block`): the custom entries come first, deduped
  with `to_ascii_uppercase`. Next, the dropped names are added to `seen`, upper-cased. Then
  each parent entry is added only if its upper-cased key is new. So the drop and the override
  are both case-insensitive for ASCII names, and every dropped name is ASCII. Only inherited
  values are dropped: the dropped names are added to `seen` after the custom entries are
  written. The unit test `another_terminals_identity_is_not_inherited` covers a lower-case
  `wt_profile_id`, a parent `TERM_PROGRAM` that loses to the custom one, `ConEmuPID`, and an
  unrelated variable that survives.
- Terminator: a non-empty block ends in `...\0\0`. The shared `entries()` helper asserts this in
  every test.
- `=C:` hidden entries: `std::env::vars_os()` on Windows yields them (key `=C:`, because the name
  scan starts at index 1). They are not on the list, so they pass through as before.
- Unicode: `encode_wide` is lossless for `OsStr`, unpaired surrogates included.
- Sorting: the block is not sorted (see F2). This is not a regression.
- "Always an explicit block": OneTerm always had a non-empty `Options::env` (`base_env()`). So
  for OneTerm, this commit changes only which inherited entries are copied, not the mechanism.
  For a bare embedder with an empty `Options::env`, the change is from a NULL block to a block
  built from `vars_os()`. Both are snapshots taken at spawn time: with a NULL block,
  `CreateProcessW` copies the parent's current block at the call, not a live link. So later
  parent changes were never seen, and there is no difference in freshness. The one new edge is
  F1.
- Tests: `cargo test -p oneterm-vt --lib conpty` (in the gate's `cargo test --workspace` and the
  vt feature runs): `an_empty_environment_still_copies_the_parent`,
  `another_terminals_identity_is_not_inherited`, `custom_entries_win_over_the_inherited_ones`
  and `custom_environment_deduplicates_case_insensitively` all `ok`. The older tests no longer
  call `set_var` on the test process; they inject the parent map instead, which also removes a
  process-global mutation from the suite.

## 3. Unix (read-only; not compiled here)

- `apply_env` calls `env_remove` for each name on the list, then `env` for each custom entry.
  In std's `CommandEnv`, a later `env` overrides an earlier `env_remove`, so the embedder's
  entry wins, as documented.
- The whole of `unix.rs` sits under `#[cfg(unix)] mod unix;`, and `pty` itself under
  `#[cfg(feature = "pty")]`. `DROPPED_PARENT_ENV` is used on both platforms, so there is no
  dead-code warning on either. No Windows API is imported into `unix.rs`. The import list
  gains only `DROPPED_PARENT_ENV`, which is used.
- Test: `get_envs()` yields only explicitly touched keys, with `None` for removed ones. So
  `get("WT_SESSION") == Some(None)`, `get("TERM_PROGRAM") == Some(Some("Embedder"))` and
  `get("PATH") == None` are the correct expectations. `envs.get(OsStr::new(name))` type-checks
  (`&OsStr: Borrow<OsStr>`), and `.copied()` on `Option<&Option<&OsStr>>` is valid. By
  inspection no default clippy lint fires. Fully qualified `std::collections::HashMap` is
  stylistic only.
- Not compiled: no Linux target is installed, and none was installed. The Linux and macOS CI
  jobs are the first compile.

## 4. Product check (own pid only)

Ran the debug `oneterm.exe` built from `b883488b` with `Process.Start` from a scratch cwd.
Debug builds keep their config in `./target`, so this also gave the run a private config. It
also used a private `USERPROFILE`, `APPDATA` and `LOCALAPPDATA`, and these extra variables:
`WT_SESSION=test`, `WT_PROFILE_ID={test}`, `TERM_PROGRAM_VERSION=9.9.9`, `ConEmuPID=1`,
`BUG0080_KEPT=kept`. To avoid driving the GUI, the private `terminal.json` got
`shell.args = ["&", "set", ">", "<scratch>\env.txt"]`. The spawned child was:

```text
C:\WINDOWS\system32\cmd.exe /K chcp 65001 >nul & set > ...\scratchpad\run\env.txt
```

These are the matching lines of `env.txt` (filter
`^(WT_|TERM|COLORTERM|ConEmu|BUG0080|USERPROFILE|WSLENV|LANG)`):

```text
TERM_PROGRAM=OneTerm
TERM_PROGRAM_VERSION=0.6.5
COLORTERM=truecolor
LANG=en_US.UTF-8
WSLENV=COLORTERM/u:TERM_PROGRAM/u
TERM=xterm-256color
BUG0080_KEPT=kept
USERPROFILE=...\scratchpad\run\prof
```

No `WT_SESSION`, `WT_PROFILE_ID` or `ConEmuPID`. The parent's `TERM_PROGRAM_VERSION=9.9.9` was
replaced by `0.6.5`. The unrelated variable was kept. Only the pids this run started (30808,
then 8008) were stopped.

Elevated instance (IN-0043, code read only, UAC not triggered): `elevation.rs` starts a new
`oneterm.exe` through `ShellExecuteExW` `runas`. That process spawns its shells through the
same `local-shell` `event_loop.rs` -> `PseudoConsole::spawn`, with `base_env()` as
`Options::env`. So it gets the same block and the same drop, and nothing in `crates/` reads any
dropped name (searched all eleven). Other `Command::new` sites in `crates/` (editor launcher,
updater relaunch, `git` for the status bar, the `shell.rs` probe) do not start a
pseudo-console child and are out of scope. The updater relaunch still passes `WT_SESSION` to
the new OneTerm, which then drops it for its own shells.

## Findings

- **F1 (Low, code):** if the block has no entries, it is a single `u16` 0. That happens when
  `Options::env` is empty and the parent environment is empty, or consists only of dropped
  names. With `CREATE_UNICODE_ENVIRONMENT`, Windows reads up to a double NUL (four bytes). So
  `CreateProcessW` would read past the `Vec`. Before this commit the empty-custom case passed
  NULL and could not reach it. std's `make_envp` handles this by pushing an extra 0 when the
  block is empty. The fix is `if block.is_empty() { block.push(0); }` before the final push,
  plus a test with an empty parent. This is not reachable from OneTerm, whose `base_env()` is
  never empty, but it is reachable by an embedder that runs with a cleared environment.
- **F2 (Low, pre-existing, not a regression):** the block is not sorted. The "Changing
  Environment Variables" documentation asks for the block to be sorted by name,
  case-insensitively. std sorts it (a `BTreeMap<EnvKey, _>`). Here the custom entries come
  first (the manual `set` output lists `TERM_PROGRAM` before `COLORTERM`). Shells work, as
  before. Record it as a follow-up; do not fold it into this bug.
- **F3 (Info):** `TERM_PROGRAM_VERSION` is not in `WSLENV_HINTS` (`env.rs`), so a WSL child gets
  `TERM_PROGRAM=OneTerm` without the version. The SSH `REMOTE_SHELL_ENV` sends
  `TERM_PROGRAM` but not the version either. Both are consistent with the packet's scope. They
  are optional one-line additions if symmetry matters.
- **F4 (Medium, records):** the packet records "links from `claude` should now be off in
  OneTerm" only as a Gap. That is a user-visible consequence: `claude` loses clickable OSC 8
  links inside OneTerm, including when OneTerm is started from Windows Terminal, where links
  used to work by accident. This needs an explicit follow-up: an upstream request asking
  `claude` to honour `TERM_PROGRAM=OneTerm`, or a generic OSC 8 capability probe. Ideally it
  is also a line in IN-0045's proposal list, with the owner's call on whether losing the links
  is acceptable while BUG-0079 is open. This is arguably desired, because BUG-0079 is the load
  those links cause, but it must be stated as a decision.
- **F5 (Low, records):** the existing harness row (rowid 159) has
  `packet_doc = .../BUG-0080-child-env-terminal-identity.md`, but the packet file is
  `BUG-0080-child-shells-inherit-another-terminals-identity.md`. Its title and notes also
  still say "Fix in crates/core/src/config/env.rs", which is superseded (see "Where the fix
  lives"). The proposed row below corrects both.
- **F6 (Info, records):** the packet omits the template's optional Context, Plan, Decisions and
  Handoff sections. "Where the fix lives" and "The list" do the job of Context and the
  rejected alternative, which is acceptable. The date (2026-09-25), the owning docs, the
  documentation action, the reconciliation, the evidence and the gaps are all present.

## Gate

- `pwsh scripts/ci-local.ps1` (`CARGO_BUILD_JOBS=2`, fresh target dir): final line
  `ci-local: all checks passed.`
- `python scripts/vt-public-api.py --check --no-doc`:
  `Public API surface unchanged (public-api.windows.txt)`.
- `python scripts/vt-public-api.py --diff-platforms`:
  `the delta is 6 lines, all inside oneterm_vt::pty` (the pre-existing `escape_args`,
  `PipeReader`, `PipeWriter` / `child_signal_mask`, `SignalMask`, `SignalMask::current`).

## Gaps

- The Unix `apply_env` path and its test were not compiled or run here. The Linux and macOS CI
  jobs are the proof.
- No `claude` session was run inside the built app to observe the link change (F4).
- F1 is found by reading the code and the Win32 contract; it was not reproduced (that would need
  a spawn with an empty environment).
- The elevated instance was checked by reading the code only; UAC was not triggered.
