# Work: Child shells inherit another terminal's identity

ID: BUG-0080
Intake: IN-0045
Created: 2026-09-25

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug (shipped behaviour: local shells have always inherited OneTerm's whole
  environment).
- Risk lane: normal. Touches the `oneterm-vt` pty transport, an external contract, but only its
  behaviour: no signature changes and `public-api.*.txt` is unchanged. Recorded in
  `crates/vt/CHANGELOG.md` under Changed.
- Spec Intake, when required: `IN-0045` (phase 3 § 7 and phase 4 § 5 found it).

## Reported by

`IN-0045` research, [`research/agent-load-phase-3.md`](research/agent-load-phase-3.md) § 7 and
[`research/agent-load-phase-4.md`](research/agent-load-phase-4.md) § 5. OneTerm launched from a
Windows Terminal shell hands its local shells `WT_SESSION` and `WT_PROFILE_ID`. `claude` reads
`WT_SESSION` as "this is Windows Terminal" and emits OSC 8 links (the `BUG-0079` load), while
`TERM_PROGRAM=OneTerm` is not in its list. A terminal must not give its children another
terminal's identity.

## Outcome

A local shell never sees a variable through which the terminal that launched OneTerm identifies
itself. `TERM`, `COLORTERM`, `TERM_PROGRAM=OneTerm`, `LANG`, the `WSLENV` hints and every
unrelated inherited variable are unchanged, and OneTerm also sets `TERM_PROGRAM_VERSION` to its
own version.

## Scope

- [x] In scope: the inherited half of the child environment on both platforms
  (`oneterm_vt::pty`: `environment_block` on Windows, the `Command` on Unix), and
  `TERM_PROGRAM_VERSION` in `oneterm_core`'s `base_env()`.
- [x] Out of scope: SSH. The remote shell is started by sshd from a fresh login environment and
  never sees OneTerm's own environment; `REMOTE_SHELL_ENV` only adds `COLORTERM`,
  `TERM_PROGRAM` and (verification F3) `TERM_PROGRAM_VERSION`. Nothing to strip there.
- [x] In scope after verification (F3): `TERM_PROGRAM_VERSION` travels with `TERM_PROGRAM` in
  the `WSLENV` hints and the SSH env requests.
- [x] Out of scope: making `claude` emit links in OneTerm (its list is its own), and `BUG-0079`.

## Where the fix lives

The brief pointed at `crates/core/src/config/env.rs`. That file builds only the **override** map:
`resolve_shell` hands it to `oneterm_vt::pty::Options::env`, which the transport writes ahead of
the parent environment (Windows `environment_block`) or on top of it (Unix `Command::env`).
Neither path can express "remove", so no change in `env.rs` alone can drop an inherited variable.
The inherited half is assembled in exactly one place per platform inside `oneterm_vt::pty`, which
already dropped two inherited variables on Unix (`XDG_ACTIVATION_TOKEN`, `DESKTOP_STARTUP_ID`,
"they belong to the process that was launched, not to a shell it opens"). The fix widens that
existing rule into one platform-independent list, `DROPPED_PARENT_ENV` in
`crates/vt/src/pty/mod.rs`, used by both platforms, so every embedder and every spawn is covered
and no public item changes. An `Options::env` entry of the same name still wins, which is how
OneTerm keeps `TERM_PROGRAM` and sets `TERM_PROGRAM_VERSION`.

Rejected: a new `Options::env_remove` field with the list in `env.rs`. It needs a public-API
change (a minor bump, since `Options` is not `#[non_exhaustive]`) for a policy every embedder
wants anyway.

## The list

Dropped from the **inherited** environment only; each is a claim about the parent's terminal that
is false inside a new pseudo-console:

| Variable | Why |
|---|---|
| `WT_SESSION` | Windows Terminal's session id; programs treat it as "running in Windows Terminal" (`claude` turns OSC 8 links on). The reported defect. |
| `WT_PROFILE_ID` | Windows Terminal's profile for the parent tab; same identity, same harm. |
| `TERM_PROGRAM` | The parent terminal's name. OneTerm sets its own; another embedder sets its own or has none, never the parent's. |
| `TERM_PROGRAM_VERSION` | The version of the parent's `TERM_PROGRAM`; wrong once the name is not the parent's. OneTerm sets its own. |
| `TERMINAL_EMULATOR` | JetBrains' terminal marker (`JetBrains-JediTerm`); tools switch to JetBrains-specific behaviour on it. |
| `ITERM_SESSION_ID` | iTerm2's session id; iTerm2 integration and image tools key on it. |
| `ITERM_PROFILE` | iTerm2's profile name; same identity. |
| `ConEmuPID` | ConEmu's process id; tools talk to the ConEmu GUI through it, and agent emitters use it to detect ConEmu (the `OSC 9;7` guard, `IN-0029` prior art § 6.4). |
| `ConEmuANSI` | ConEmu's "ANSI is on" flag; the other half of the ConEmu detection. |
| `XDG_ACTIVATION_TOKEN` | Already dropped on Unix: a single-use startup token of the launched process. |
| `DESKTOP_STARTUP_ID` | Already dropped on Unix: the X11 form of the same token. |

Considered and kept: `VSCODE_*` (IPC handles and git credential helpers the user may rely on;
none claims the terminal is VS Code's, `TERM_PROGRAM` did that), `SESSIONNAME` (a Windows logon
session, not a terminal), `TERM` and `COLORTERM` (the embedder sets them; on Unix a missing
`TERM` breaks more than a stale one), `KITTY_WINDOW_ID` / `WEZTERM_*` / `ALACRITTY_*` (Unix
nesting, not reported; add a line when one is).

## Acceptance

- [x] Inherited `WT_SESSION` and `WT_PROFILE_ID` are absent from a spawned child's environment
  on Windows, whatever their case; `TERM_PROGRAM` is the embedder's value; an unrelated inherited
  variable survives. Unit test with an injected parent map.
- [x] The same on Unix, on the `Command` the spawn builds. Unit test (runs on the Linux and macOS
  CI jobs).
- [x] `base_env()` sets `TERM_PROGRAM_VERSION` to OneTerm's version next to `TERM_PROGRAM`.
- [x] Manual: the debug app launched with `WT_SESSION=test` in its environment opens a local
  shell whose `$env:WT_SESSION` is empty.
- [x] Elevated instance (`IN-0043`) and the shell integrations (`US-0136`) do not read any
  dropped variable (checked, see Evidence).

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md` § 6.3 — the local-shell env contract (`TERM`, `COLORTERM`); silent
  on what is inherited.
- `docs/ssh-client-connect.md` § 9.7 — the remote shell env; mirrors `base_env()`.
- `crates/vt/src/pty/mod.rs` `Options::env` rustdoc and `crates/vt/docs/guide/13-pty.md` —
  "entries applied on top of the parent environment".
- `crates/vt/CHANGELOG.md` — the promise; a behaviour change with no signature change is recorded
  under Changed.
- `docs/spec-intakes/IN-0043-run-shell-as-administrator/` — the elevated child gets the base env
  and no custom entry; unaffected.

### Documentation Action

Update required: `docs/terminal-backend.md` § 6.3 (what the child inherits and what it does
not), the `Options::env` rustdoc and guide chapter 13 (the dropped list), `crates/vt/CHANGELOG.md`
(Changed), `docs/ssh-client-connect.md` § 9.7 (why SSH needs nothing).

Reason: none of them says the inherited environment is filtered.

### Reconciliation

All five updated with this change.

## Context

- The child environment has two halves: OneTerm's override map (`base_env()` + `shell.env`,
  built in `crates/core/src/config/env.rs` / `shell.rs`) and the inherited environment, which
  only `oneterm_vt::pty` assembles (`conpty.rs` `environment_block`, `unix.rs` `apply_env`).
  The drop list therefore lives in vt (`DROPPED_PARENT_ENV`, `crates/vt/src/pty/mod.rs`); see
  "Where the fix lives".
- The elevated instance (`IN-0043`) spawns with an empty `shell.env`; it goes through the same
  `environment_block`, so it is covered with no change of its own.

## Plan

- [x] One drop list in vt, applied on both platforms; `TERM_PROGRAM_VERSION` in `base_env()`.
- [x] Verification rework: F1 (empty block is a double NUL), F3 (`TERM_PROGRAM_VERSION` in the
  `WSLENV` hints and the SSH env requests), F4/F6 (this Context/Plan/Decisions and the Handoff).

## Decisions

No `DEC`: the choice binds only this code path. Recorded here:

- **Placement in `oneterm_vt::pty`, not `env.rs`.** The override map cannot express a removal;
  the inherited half is built only in vt, which already dropped two Unix startup tokens for the
  same reason. One list there covers every spawn and every embedder.
- **Not a new `Options::env_remove` field.** `Options` is not `#[non_exhaustive]`, so a new
  public field is a minor (breaking) bump under the vt CHANGELOG promise, for a rule every
  embedder wants anyway. Only the inherited value is dropped, so an embedder that wants one of
  these names sets it through `Options::env`.

## Verification Plan

- `cargo test -p oneterm-vt --lib pty` (the Windows `environment_block` test; the Unix test is
  compiled out here and runs in CI), `cargo test -p oneterm-core -p oneterm-local-shell`.
- Manual private-profile launch with `WT_SESSION` set.
- Full `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Unit, Windows: `cargo test -p oneterm-vt --lib conpty` — 8 passed, including
  `another_terminals_identity_is_not_inherited` (injected parent `WT_SESSION`,
  lower-case `wt_profile_id`, `TERM_PROGRAM=vscode`, `ConEmuPID`, `USERPROFILE`; custom
  `TERM_PROGRAM=Embedder`; the block is exactly `TERM_PROGRAM=Embedder`, `USERPROFILE=...`) and
  `an_empty_environment_still_copies_the_parent`. The two older tests now inject the parent
  instead of mutating the test process's environment.
- Unit, core: `cargo test -p oneterm-core -p oneterm-local-shell` — all passed;
  `base_env_advertises_oneterm_truecolor` checks `TERM_PROGRAM_VERSION`.
- Manual, Windows 11, 2026-09-25: debug `oneterm.exe` from this branch launched by
  `Start-Process` with a scratch `USERPROFILE`/`APPDATA`/`LOCALAPPDATA` and `WT_SESSION=test`,
  `WT_PROFILE_ID={test-profile}`, `TERM_PROGRAM_VERSION=9.9.9`, `BUG0080_UNRELATED=kept` in
  its environment. Typed into its startup `cmd.exe` (posted `WM_CHAR` to that pid's window only):

  ```text
  WT_SESSION=[%WT_SESSION%]
  WT_PROFILE_ID=[%WT_PROFILE_ID%]
  TERM_PROGRAM=[OneTerm]
  TERM_PROGRAM_VERSION=[0.6.5]
  BUG0080_UNRELATED=[kept]
  Environment variable WT_ not defined
  ```

  (`cmd` echoes `%NAME%` literally for an undefined variable.) Then
  `powershell -NoProfile -Command "'WT_SESSION=[' + $env:WT_SESSION + '] exists=' + (Test-Path env:WT_SESSION)"`
  in the same shell printed `WT_SESSION=[] exists=False`.
- Elevated instance (`IN-0043`): it spawns with an empty `Options::env` plus `base_env()`; the
  dropped names are read nowhere in `crates/` (searched), and the elevated child now also loses
  them, which is the intent. Shell integrations (`US-0136`): the injected hooks read none of the
  dropped names (searched `crates/` for all eleven).
- Full gate: `pwsh scripts/ci-local.ps1` on Windows 11, 2026-09-25, ended with
  `ci-local: all checks passed.` (`vt-public-api.py --check` unchanged: no public item moved).
- Verification ([`evidence/BUG-0080-verify.md`](evidence/BUG-0080-verify.md)): PASS with
  findings F1-F6. Rework, 2026-09-25:
  - F1: a block with no entries (empty `Options::env`, empty parent) was a single `u16` 0;
    `CreateProcessW` needs two. `environment_block` now pushes the extra NUL when the block is
    empty; test `a_block_with_no_entries_is_a_double_nul` asserts exactly `[0, 0]`.
  - F3: `TERM_PROGRAM_VERSION` added to `WSLENV_HINTS` (`env.rs`, tests updated) and to
    `REMOTE_SHELL_ENV` (`crates/ssh/src/session.rs`; the `BUG-0038` env-request test now
    expects three requests).
  - F4, F6: Context, Plan, Decisions and Handoff sections added. F2 (unsorted block) is
    pre-existing and out of scope; F5 (harness row) is the coordinator's.
  - Re-run: `cargo test -p oneterm-vt --lib conpty`, `cargo test -p oneterm-core -p oneterm-ssh`,
    `cargo fmt --all -- --check`, `cargo clippy -p oneterm-vt -p oneterm-core -p oneterm-ssh
    --all-targets -- -D warnings`, `check-doc-paths`, `check-english`, and the vt rustdoc
    citation grep: all pass.

Gaps:

- The Unix test (`unix.rs` `another_terminals_identity_is_not_inherited`) and the Unix
  `apply_env` path were not compiled here (no Unix target installed); they run in the Linux and
  macOS CI jobs.
- No control run with a pre-fix binary in the same walk (the owner's own instance was not
  touched); the pre-fix behaviour is the phase 3 § 7 finding.
- No `claude` session was run to confirm it stops emitting OSC 8 in OneTerm; per phase 3 § 7 it
  keys on `WT_SESSION` and does not list `TERM_PROGRAM=OneTerm`, so links from `claude` should
  now be off in OneTerm.
- `KITTY_WINDOW_ID`, `WEZTERM_*`, `ALACRITTY_*` are deliberately not in the list (see "The
  list").

## Handoff

- State: implemented and verified (PASS); F1 and F3 fixed, F4/F6 recorded here.
- **Known consequence: `claude` emits no OSC 8 links inside OneTerm after this fix.** The
  `claude` CLI decides hyperlink support from `WT_SESSION` and a `TERM_PROGRAM` allow-list that
  does not contain `OneTerm` (phase 3 § 7). The inherited `WT_SESSION` was the only reason it
  sent links here; it is gone, so its URLs now arrive as plain text.
- Next actions, not done here:
  - (a) Coordinator/owner: ask upstream (`claude` CLI) to honour `TERM_PROGRAM=OneTerm`, or to
    probe the capability instead of keying on terminal names. **Not filed.**
  - (b) Owner: accept losing `claude`'s OSC 8 links in OneTerm. That acceptance is owed. If links
    come back (upstream change, or a user setting `WT_SESSION` in `shell.env`), `BUG-0079`
    removes the memory harm of repainted implicit links.
