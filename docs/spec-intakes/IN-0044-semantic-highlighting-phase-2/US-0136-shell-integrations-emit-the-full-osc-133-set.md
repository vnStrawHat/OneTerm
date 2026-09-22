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
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change
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

## Scope

- [ ] In scope:
  - `crates/core/src/config/shell.rs` — the generated `PROMPT` (cmd), `PROMPT_COMMAND` +
    `PS0` (bash), `PS1` (zsh) and the PowerShell/pwsh `-Command` init string.
  - `crates/ssh/src/session.rs` — `SHELL_INTEGRATION_BOOTSTRAP`, the line typed into the
    remote shell after `request_shell`.
  - Unit tests over the generated strings, per shell.
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
      first and the user's follows it, separated by `;`.
- [x] A user-supplied `PS1` is preserved, not replaced: `B` is appended to whatever `PS1`
      holds at prompt time (after every rc file has run), once and only once.
- [x] zsh emits `A`, `B` and `D;<code>` from the generated `PS1`; the code comes from zsh's
      own `%?`. `C` is recorded as a limit, not claimed.
- [x] PowerShell and pwsh emit `A`, `B`, `C` and `D;<code>`; the original `prompt` function
      is still called and its output still appears.
- [x] `cmd /c exit 3` inside pwsh produces `133;D;3`.
- [x] `cmd.exe` is unchanged and still emits `A` + `B`; the doc states that `C`/`D` are not
      reachable from `PROMPT`.
- [x] The SSH bootstrap installs the bash set or the zsh set according to the remote shell
      it detects, and still exports `COLORTERM` first (`BUG-0038`).
- [x] No `D` is emitted before the first command of a session.
- [x] Every generated string is unit-tested for its exact bytes: one backslash after ESC,
      the `133;D;<code>` form the engine parses, and no `"` in the PowerShell `-Command`
      argument.

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

Full record: `evidence/US-0136-verify.md`, with the frame
`evidence/US-0136-verify-pwsh-tab.png`.

The live Windows walk (OneTerm `fast-dev`, `RUST_LOG=debug`, one run per shell kind, two
commands typed into the tab) is the load-bearing proof:

```
cmd        : PromptStart PromptEnd  x3                     -- no C, no D
pwsh       : PromptStart PromptEnd OutputStart OutputEnd
             PromptStart PromptEnd OutputStart OutputEnd
             PromptStart PromptEnd
             OutputEnd { exit_code: Some(3) } | OutputEnd { exit_code: Some(0) }
powershell : identical to pwsh
```

`cargo test -p oneterm-core -p oneterm-ssh -p oneterm-terminal` green;
`pwsh scripts/ci-local.ps1` ends with `ci-local: all checks passed`.

Gaps:

- **bash and zsh are unit-tested only.** No Unix host is available in this session, so the
  generated `PROMPT_COMMAND` / `PS0` / `PS1` are proven as strings, not as behaviour in a
  running bash or zsh. Same for the SSH bootstrap, both branches: no remote host was
  reachable, so the typed line is proven as a string only.
- **Local zsh cannot emit `C`.** `preexec` is a function and no environment variable
  carries zsh code; a sourced file is out of scope under `DEC-0001`. Remote zsh (the SSH
  bootstrap) does emit `C`, because there OneTerm types a line and can define functions.
- **`cmd.exe` cannot emit `C` or `D`.** `PROMPT` is the only hook and it has no error-level
  code. Unchanged by design.
- **PowerShell `C` needs PSReadLine.** Without it the `Enter` handler is not installed and
  the tab reports `A`/`B`/`D` only. Every shipping PowerShell 5.1 and 7 has it.
- **PowerShell's exit code after a failing *cmdlet*** is whatever `$LASTEXITCODE` last
  held, if that is non-zero — non-zero, so the failure reads as a failure, but the number
  may belong to an older native command. Distinguishing them needs `Get-History` per
  prompt; not worth a history lookup on every prompt for a number nothing reads beyond
  zero / non-zero.
- **A prompt redrawn without a command** (Ctrl+C on an empty line) emits another `D`. Every
  OSC 133 implementation that hooks the prompt has this; harmless, since the consumer keeps
  only the last code.

## Handoff

None. `US-0133` consumes what this packet produces; the two are independent and can land in
either order.
