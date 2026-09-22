# Work: The shell integrations OneTerm injects emit the full OSC 133 mark set

ID: US-0136
Intake: IN-0044
Created: 2026-09-22

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change (reopened once for acceptance rework — see Rework)
- Risk lane: normal
- Spec Intake, when required: `IN-0044` — `docs/spec-intakes/IN-0044-semantic-highlighting-phase-2/IN-0044.md`

## Outcome

Every shell integration OneTerm injects emits as much of the OSC 133 mark set — `A`
(prompt start), `B` (prompt end / input start), `C` (output start), `D;<code>` (command
done, with the exit code) — as that shell can express through the env-only injection route,
and the per-shell limit is written down where the limit is real.

`US-0133` reads the engine's OSC 133 regions and the exit code. It can only read what the
shell sent. Before this packet the producer side emitted at most `A` and `B`:

| Shell | Before |
| --- | --- |
| `cmd.exe` | `A` + `B` (generated `PROMPT`) |
| zsh (local) | `A` + `B` (generated `PS1`) |
| bash (local) | `A` only (generated `PROMPT_COMMAND`) |
| PowerShell / pwsh | nothing (the `prompt` wrapper emitted OSC 7 only) |
| SSH bootstrap | `A` only |

No integration emitted `C` or `D` anywhere, so `SnapshotCell::semantic` never held
`Semantic::Output` from a mark and `SharedState::last_exit_code` was never written by a
local shell. The exit-code tint of `US-0133` had no input.

## Rework (independent verification, 2026-09-22, two rounds)

`evidence/US-0136-independent-verify.md` returned **FAIL** twice: first one major and three
medium findings plus four minors, then — against the fix for those — a new major the MED-3
fix had itself introduced, plus a new minor. All are closed;
`evidence/US-0136-verify.md` §0 is the finding-by-finding record and every section below is
updated to match the current code.

| # | What was wrong | What it is now |
|---|---|---|
| MAJ-1 | Every bash prompt, local **and** SSH, printed a stray `]`. `\[` + `ESC \` + `\]` merges the mark's trailing backslash into the escape `\\`, so bash printed `]` and never closed the non-printing region — visible on screen and leaking into OneTerm's completion popup | One constant, `BASH_OSC133_PS1_MARK` = `\[\e]133;B\a\]`, used by both call sites: prompt escapes terminated by **BEL**, so no backslash touches `\]`. The append is a plain concatenation and the guard compares the same text |
| MED-1 | The SSH bootstrap emitted `D;0` before the user's first command, in both branches | The closing `__oneterm_precmd` clears the seen-flag again |
| MED-2 | `PROMPT_COMMAND` was inserted unconditionally, so the documented opt-out no longer existed | A stated rule: a user `PROMPT_COMMAND` that already emits OSC 133 is left alone, `PS0` with it; any other value is still appended to |
| MED-3 | The PowerShell init silently replaced a user's PSReadLine `Enter` handler | It chains to the current binding, and declines to install at all when that binding is a script block it cannot call by name |
| MIN-1 / MIN-2 | A throwing `prompt` lost `B` forever; a multi-value prompt was collapsed with `-join ''` | The delegation is in a `try` with a fallback prompt; the join is `[Environment]::NewLine` |
| MIN-3 | Bootstrap size, and `fish`/`csh` cannot parse it | Stated rather than worked around: §6.1.2 gains the row and the reason; size re-measured at 880 bytes, against `MAX_CANON` 4096 |
| MIN-4 | The evidence measured the `PS1` **variable**, never its expansion — exactly where MAJ-1 lived | Every prompt reading is now `${PS1@P}`, and a real-PTY bash test guards the expansion |

**Round two.**

| # | What was wrong | What it is now |
|---|---|---|
| NEW-MAJ-1 | The MED-3 fix declined on `$f -ne 'CustomAction'`, but `Get-PSReadLineKeyHandler` reports that name only for a script block bound **without** a `-BriefDescription`. With one — the idiomatic form — `Function` *is* the description, so the guard passed, OneTerm installed, and every `Enter` called `[PSConsoleReadLine]::<description>()`, which does not exist. The line was never submitted: that user could not run a command at all, where before the rework they merely lost `ValidateAndAcceptLine` | The guard asks what it meant to ask — `if([Microsoft.PowerShell.PSConsoleReadLine].GetMethod($f))`. A name it can call afterwards → chain to it; anything else → do not install, and report no `C` |
| NEW-MIN-1 | The mark was safe at its end but still *began* with `\[`, so a user `PS1` ending in a lone `\` swallowed it and the region never opened | The mark is four **raw bytes** — `\001 ESC ]133;B BEL \002`, the values `\[` and `\]` expand to — so it carries no backslash at either end and nothing can merge with it |

**Why no test caught NEW-MAJ-1.** The same reason as MAJ-1, one level up: the assertion read
the *generated string* rather than what a shell does with it, and a string cannot tell
`CustomAction` from `SmartEnter`. `powershell_enter_guard_decides_the_four_bindings` now
runs the real init in both real hosts behind each of the four bindings and asserts the
verdict; restoring the old comparison fails it on exactly the described-block row.

**Why no test caught MAJ-1, and what now does.** The one assertion that touched the append
asserted the defective literal was present. Two tests replace it:
`bash_ps1_mark_survives_prompt_expansion` holds the invariant a string test *can* hold (the
mark is escapes, its region balances, nothing ends in a backslash beside `\]`), and
`bash_prompt_draws_no_stray_bracket` in `crates/local-shell` spawns a real bash through the
real PTY and reads the drawn grid. Reverting the fix makes the second one fail with the
verifier's exact symptom.

## Scope

- [ ] In scope:
  - `crates/core/src/config/shell.rs` — the generated `PROMPT` (cmd), `PROMPT_COMMAND` +
    `PS0` (bash), `PS1` (zsh) and the PowerShell/pwsh `-Command` init string.
  - `crates/ssh/src/session.rs` — `SHELL_INTEGRATION_BOOTSTRAP`, the line typed into the
    remote shell after `request_shell`.
  - Unit tests over the generated strings, per shell, and — since the rework — two
    behavioural tests, because a string assertion can see neither a prompt escape nor a
    PSReadLine binding: one in `crates/local-shell/src/session_tests.rs` that draws a real
    bash prompt through the real PTY, and one in `crates/core` that runs the generated
    PowerShell init in both real hosts across the four `Enter` bindings.
  - `crates/terminal/src/backend/osc_router.rs` — one `log::debug!` of each routed
    `ShellMark`. Not a behaviour change: it is the only way to observe a mark without a
    renderer, and it is what turned the Windows walk into evidence.
  - `docs/terminal-backend.md` §6.1.1 (rewritten as §6.1.2, "What each shell integration
    emits": the duplicate §6.1.1 numbering it shared with the elevated-shell section of
    `IN-0043` is resolved in that section's favour, since every existing citation of §6.1.1
    means that one) and `docs/terminal-semantic-highlighting.md` §4.2.
- [ ] Out of scope:
  - Any consumer change. The engine already parses `A`/`B`/`C`/`D;<code>`
    (`crates/vt/src/terminal/dispatch.rs:2105-2110`) and the router already routes them
    (`crates/terminal/src/backend/osc_router.rs:229-240`). Nothing downstream changes
    behaviour; the one edit there is a diagnostic log line.
  - Reading the marks in the renderer — `US-0133`.
  - Any new injection route. The integration stays env-based (local) and one typed
    bootstrap line (SSH), opt-out exactly as today. No temp file, no `ZDOTDIR`, no profile
    edit; see Context.
  - Non-Windows platform proof. Only Windows is available here.

## Acceptance

- [x] bash emits `A`, `B`, `C` and `D;<code>`; the `<code>` is the exit status of the user's
      command, and `$?` is restored before any user-supplied `PROMPT_COMMAND` runs.
- [x] A user-supplied `PROMPT_COMMAND` is preserved, not replaced: OneTerm's part runs
      first and the user's follows it, separated by `;` — unless that value already emits
      OSC 133, which is the opt-out and leaves `PROMPT_COMMAND` and `PS0` untouched
      (MED-2).
- [x] A user-supplied `PS1` is preserved, not replaced: `B` is appended to whatever `PS1`
      holds at prompt time (after every rc file has run), once and only once — **and the
      prompt bash then draws is unchanged**, which is the half the first attempt failed
      (MAJ-1).
- [x] zsh emits `A`, `B` and `D;<code>` from the generated `PS1`; the code comes from zsh's
      own `%?`. `C` is recorded as a limit, not claimed.
- [x] PowerShell and pwsh emit `A`, `B`, `C` and `D;<code>`; the original `prompt` function
      is still called and its output still appears.
- [x] `cmd /c exit 3` inside pwsh produces `133;D;3`.
- [x] `cmd.exe` is unchanged and still emits `A` + `B`; the doc states that `C`/`D` are not
      reachable from `PROMPT`.
- [x] The SSH bootstrap installs the bash set or the zsh set according to the remote shell
      it detects, and still exports `COLORTERM` first (`BUG-0038`).
- [x] No `D` is emitted before the first command of a session — including over SSH,
      where the bootstrap's own closing call used to consume the guard (MED-1). Local zsh
      is the one stated exception: its `PS1`-only route has nowhere to hold a flag.
- [x] Every generated string is unit-tested for its exact bytes: one backslash after ESC,
      the `133;D;<code>` form the engine parses, and no `"` in the PowerShell `-Command`
      argument. What a string test cannot see — the **expanded** bash prompt — has a
      real-PTY test of its own.
- [x] OneTerm does not silently take over a PSReadLine `Enter` binding the user already
      has: it chains to it, or leaves it alone and reports no `C` (MED-3) — decided by
      whether the bound name is a real public static it can call, not by its spelling, so
      a described script block is declined rather than turned into a broken `Enter`
      (NEW-MAJ-1).
- [x] The `B` mark appended to a bash `PS1` cannot merge with the user's prompt at
      **either** end, whatever that prompt ends in (NEW-MIN-1).

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md` §6.1.1 (cwd reporting) — the generated-prompt contract per Windows shell, and
  §6.2's note that the elevated instance re-runs `resolve_shell` so `PROMPT`,
  `PROMPT_COMMAND`, `PS1` and `TERM` survive. The new `PS0` joins that list.
- `docs/terminal-semantic-highlighting.md` §4.2 — the OSC 133 fast path, which names the
  four marks and what each one means to the scanner. It describes the consumer; it said
  nothing about which shells produce what.
- `docs/spec-intakes/IN-0044-semantic-highlighting-phase-2/high-level-design.md` — the exit
  code is the one fact not in a cell; it arrives only as `ShellMark::OutputEnd`.
- `docs/terminal-backend.md` §6.1 / §6.2 — the integration route itself: environment
  variables only for a local shell, opt-out because every generated variable yields to a
  user-supplied one, and no file written or profile edited. There is no decision record for
  it; §6.1 and §6.2 are where it is written down. Honoured as-is.

### Documentation Action

Update required:

- `docs/terminal-backend.md` §6.1.1 (cwd reporting) — becomes §6.1.2, the per-shell mark
  table: what each integration emits, by which mechanism, and what it cannot emit, instead
  of only the OSC 7 paragraph.
- `docs/terminal-semantic-highlighting.md` §4.2 — a pointer to that table, so the fast
  path's reader knows which shells actually feed it.

Reason: the behaviour of the producer side changes for four of the five integrations, and
that section is the only place that documents it.

### Reconciliation

Changed: `docs/terminal-backend.md` — the old §6.1.1 "Windows local cwd reporting" is now
§6.1.2 "What each shell integration emits (OSC 7 and OSC 133)", carrying the per-shell mark
table, the reason for each gap and the never-replace rule; and
`docs/terminal-semantic-highlighting.md` §4.2, a paragraph naming which shells feed the
fast path and pointing at that table. No other owning doc needed a change: the engine and
router contracts are unchanged by this packet.

## Context

**Why the mechanism differs per shell.** The integration is env-based for local shells:
OneTerm sets variables in the spawn environment and never writes a file or edits a profile.
That decides what is reachable.

- **bash** reads `PROMPT_COMMAND`, `PS0` and `PS1` from the environment. `PROMPT_COMMAND`
  runs before each prompt with `$?` still holding the last command's status → `D` and `A`.
  `PS0` is expanded after a complete command is read and before it runs (bash ≥ 4.4) → `C`.
  `B` must sit at the *end* of the prompt, and an rc file usually sets `PS1` after the
  environment is read, so the marker is appended to `PS1` from inside `PROMPT_COMMAND`,
  which runs after every rc file; the `case` guard keeps it to one append.
- **zsh** has no `PROMPT_COMMAND`, and `precmd` / `preexec` are *functions* — there is no
  environment variable that carries zsh code. From the environment only `PS1` is reachable,
  which gives `A`, `B` and — through zsh's own `%?` prompt escape — `D;<code>`. `C` needs
  `preexec`, which needs a sourced file. Recorded as a gap, not worked around.
- **PowerShell / pwsh** are started with `-Command <init>`, so code *can* be injected: the
  `prompt` function is wrapped (already the case for OSC 7) and `C` comes from a PSReadLine
  `Enter` handler. The handler is installed only when `Set-PSReadLineKeyHandler` exists, so
  a host without PSReadLine keeps its default `Enter` and simply reports no `C`.
  Overriding `PSConsoleHostReadLine` instead would have been the other route; it replaces
  the line editor's entry point, so a mistake there costs the user their input, and it was
  not taken.
- **cmd.exe** has exactly one hook, `PROMPT`, which is expanded before the prompt and whose
  `$` codes do not include the error level. `A` and `B` are everything it can do.
- **SSH** types one bootstrap line into the remote shell, so it can define functions. It
  branches on `$ZSH_VERSION` and installs the zsh set (`precmd_functions` /
  `preexec_functions`, so the full `A`/`B`/`C`/`D` set is reachable there, unlike local
  zsh) or the bash set (`PROMPT_COMMAND` + `PS0`).

**The exit code.** The engine parses `133;D;<code>` with `<code>` an `i32`
(`dispatch.rs:2109-2114`); anything else yields `OutputEnd { exit_code: None }`. All five
producers emit the `;<code>` form.

**PowerShell's exit code, measured rather than assumed.** Probed on this machine:

```
pwsh 7:       cmd /c exit 3  -> $? = False, $LASTEXITCODE = 3
              Get-Item .     -> $? = True,  $LASTEXITCODE = 3   (stale)
powershell 5: cmd /c exit 3  -> $? = False, $LASTEXITCODE = 3
```

So `$LASTEXITCODE` alone is wrong (it is stale after a cmdlet) and `$?` alone carries no
number. The rule is `$?` first, `$LASTEXITCODE` only as the number for a failure:
`if ($ok) { 0 } elseif ($LASTEXITCODE -gt 0) { $LASTEXITCODE } else { 1 }`.

**No `D` before the first command.** Each producer skips the `D` of its first prompt
(`__oneterm_seen` / `$global:__OneTermRan`); a `D` before any `C` would hand `US-0133` a
completed block that never ran.

## Plan

- [x] bash: `PROMPT_COMMAND` (D + OSC 7 + A, `PS1` append for B, `$?` restored) and `PS0` (C).
- [x] zsh: `PS1` gains `133;D;%?` ahead of `A`.
- [x] PowerShell/pwsh: the `prompt` wrapper emits D + OSC 7 + A and returns the original
      prompt with B appended; a PSReadLine `Enter` handler emits C.
- [x] SSH bootstrap: branch on `$ZSH_VERSION`; bash set or zsh set.
- [x] Unit tests per shell over the generated strings.
- [x] Reconcile `docs/terminal-backend.md` §6.1.2 and `docs/terminal-semantic-highlighting.md` §4.2.

## Decisions

None. The env-based, opt-out integration route documented in `docs/terminal-backend.md`
§6.1 / §6.2 is honoured as-is; this packet only changes what those variables contain. No
choice here is one future work must inherit.

## Verification Plan

- Unit: the generated strings per shell (exact bytes, escaping, the `133;D;` form, a
  user-supplied `PROMPT_COMMAND`/`PS1` preserved).
- Platform (Windows): a `fast-dev` walk with a `cmd` tab (unchanged, A+B) and a PowerShell
  tab, plus a probe that logs the router's `SessionEvent::ShellIntegration` stream for a
  live pwsh session and shows `A`/`B`/`C`/`D;3` for `cmd /c exit 3`.
- Regression: `cargo test -p oneterm-core -p oneterm-ssh -p oneterm-terminal`, then
  `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Full record: `evidence/US-0136-verify.md`, whose §0 is the finding-by-finding rework table.
Frames: `evidence/US-0136-verify-pwsh-tab.png`, `evidence/US-0136-rework-bash-tab.png`,
`evidence/US-0136-rework-bash-completion.png` and
`evidence/US-0136-verify4-pwsh-described-enter.png`.

The live Windows walk (OneTerm `fast-dev`, `RUST_LOG=debug`, one run per shell kind, two
commands typed into the tab) is the load-bearing proof, and since the rework it covers a
real bash tab too:

```
cmd        : PromptStart PromptEnd  x3                     -- no C, no D
bash       : PromptStart PromptEnd OutputStart OutputEnd
             PromptStart PromptEnd OutputStart OutputEnd
             PromptStart PromptEnd
             OutputEnd { exit_code: Some(7) } | OutputEnd { exit_code: Some(0) }
pwsh       : PromptStart PromptEnd OutputStart OutputEnd
             PromptStart PromptEnd OutputStart OutputEnd
             PromptStart PromptEnd
             OutputEnd { exit_code: Some(3) } | OutputEnd { exit_code: Some(0) }
powershell : identical to pwsh
```

`cargo test -p oneterm-core -p oneterm-ssh -p oneterm-local-shell -p oneterm-terminal`
green; `pwsh scripts/ci-local.ps1` ends with `ci-local: all checks passed`.

Gaps:

- **zsh is unproven as behaviour**, local and remote. No zsh and no WSL on this host, so
  `ZSH_OSC133_PS1` and the bootstrap's zsh branch are proven as strings only. Both use
  zsh's own documented `%?` and `%{…%}`, and zsh expands no backslashes in a prompt, so the
  `MAJ-1` class of defect cannot reach it — but it is untested.
- **The SSH bootstrap is unproven over a real channel.** No SSH server was reachable. Both
  branches parse under `bash -n` and `dash -n`, and the bash branch was driven in a real
  bash; what is untested is the echo behaviour and the timing against `request_shell`.
- **`fish`, `csh` and `tcsh` over SSH get a burst of parse errors** and no integration:
  the bootstrap is one POSIX line and reaches no branch in them. OneTerm cannot detect it —
  the channel write succeeded, which is all `send_shell_integration_bootstrap` observes.
  Recorded in `docs/terminal-backend.md` §6.1.2; fixing it needs a round trip the connect
  path does not have.
- **Local zsh cannot emit `C`.** `preexec` is a function and no environment variable
  carries zsh code; a sourced file is outside the env-only route. Remote zsh does emit `C`,
  because there OneTerm types a line and can define functions.
- **`cmd.exe` cannot emit `C` or `D`.** `PROMPT` is its only hook and has no error-level
  code. Unchanged by design.
- **PowerShell's `C` needs PSReadLine, and an `Enter` binding OneTerm can chain to.**
  Without PSReadLine, or when `Enter` is bound to a script block of the user's own — with or
  without a `-BriefDescription`; `Get-PSReadLineKeyHandler` hands out a name, never the
  block — the handler is not installed and the tab reports `A`/`B` only (and so no `D`
  either, since `D` is gated on the flag the `C` handler sets). Leaving the user's editor
  alone is the deliberate choice there.
- **`Type.GetMethod` throws on an overloaded name.** `PSConsoleReadLine` has seven
  overloaded public statics (`Insert`, `ReadLine`, `SetKeyHandler`, …); none is a plausible
  `Enter` binding, and if one ever were, the exception aborts the rest of the init after the
  prompt wrapper is already installed — so the tab keeps the user's `Enter` and reports no
  `C`. Safe by construction rather than by handling.
- **PowerShell's `C` fires on `Enter` even when the line is incomplete** — PSReadLine gives
  the handler no way to ask whether `AcceptLine` accepted. One early `C`; the real one
  follows.
- **PowerShell's exit code after a failing *cmdlet*** is whatever `$LASTEXITCODE` last held,
  if non-zero — the failure still reads as a failure, but the number may belong to an older
  native command. A **negative** `$LASTEXITCODE` (a native crash) reports `D;1` for the same
  reason. Distinguishing them needs a `Get-History` on every prompt, for a number nothing
  reads beyond zero / non-zero.
- **A prompt redrawn without a command** (Ctrl+C on an empty line) emits another `D`. Every
  OSC 133 implementation that hooks the prompt has this; harmless, since the consumer keeps
  only the last code.
- **Two ceilings of the env-only route**, neither detectable from here: a `.bashrc` that
  assigns an array `PROMPT_COMMAND=(…)` (bash >= 5.1) discards OneTerm's scalar and the
  integration stops; a `.zshrc` that sets `PROMPT` replaces the generated `PS1` and takes
  the marks with it. Both are now in §6.1.2.

## Handoff

None. `US-0133` consumes what this packet produces; the two are independent and can land in
either order.
