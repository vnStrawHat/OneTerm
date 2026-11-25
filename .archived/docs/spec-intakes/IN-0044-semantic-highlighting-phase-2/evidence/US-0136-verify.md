# Evidence: US-0136 — the shell integrations emit the full OSC 133 set

> **Rework, 2026-09-22.** Two rounds. An independent verification
> (`US-0136-independent-verify.md`) returned FAIL on one major and three medium
> findings; the re-verification of that fix returned FAIL again on a new major
> the MED-3 fix had introduced, plus one new minor. Everything below is
> re-measured against the current code; §0 records every finding and how it was
> closed.

Date: 2026-09-22
Intake: `IN-0044`
Packet: `US-0136-shell-integrations-emit-the-full-osc-133-set.md`
Host: Windows 11 Enterprise 10.0.26200, `bash` 5.3.15 (Git for Windows), `dash` (Git for
Windows), pwsh 7, Windows PowerShell 5.1. No Unix host, no WSL, no zsh, no SSH server.

## 0. The rework: what the verification found, and what changed

| # | Finding | Fix | Proof |
|---|---|---|---|
| MAJ-1 | Every bash prompt, local **and** SSH, printed a stray `]`: `\[` + `ESC \` + `\]` merges the mark's trailing backslash with the wrapper's, so bash printed `]` and never closed the non-printing region | One constant, `BASH_OSC133_PS1_MARK` = `\[\e]133;B\a\]`, used by both call sites: bash **prompt escapes**, terminated by **BEL** so no backslash touches `\]`. The append is now a plain concatenation and the idempotence guard compares the same text | §3, §4 (`${PS1@P}`, `strays: 0`), §6 (the live bash tab and its completion popup), and two new tests — one of which reproduces the old defect when reverted |
| MED-1 | The SSH bootstrap emitted `D;0` before the user's first command in both branches: the closing `__oneterm_precmd` consumed the seen-flag | The closing call clears the flag again (`__oneterm_precmd; __oneterm_seen=;`) | §4 — the first prompt now carries OSC 7 + `A` and no `D` |
| MED-2 | `env.insert("PROMPT_COMMAND", …)` was unconditional, so the documented opt-out was gone | A precise rule: a user `PROMPT_COMMAND` that **already emits OSC 133** (contains `133;`) is left untouched, `PS0` with it — that is the opt-out; any other value is still appended to | `bash_yields_to_a_prompt_command_that_already_emits_osc133`, and §6.1.2 rewritten to state the three rules |
| MED-3 | The PowerShell init silently replaced a user's PSReadLine `Enter` handler | The handler **chains**: it reads the current binding and calls it, and installs only when that name is one it can call — see NEW-MAJ-1 for how "can call" is decided | §6b, §6c |
| MIN-1 | A user `prompt` that threw lost `B` forever | The delegation is in a `try`; the `catch` falls back to a default prompt so the region still closes | §7 |
| MIN-2 | `-join ''` collapsed a multi-value prompt | Joined with `[Environment]::NewLine` | §7 |
| MIN-3 | Bootstrap size, and fish/csh cannot parse it | Stated, not worked around: §6.1.2 gains a `fish`/`csh`/`tcsh` row and the paragraph explaining it. Size re-measured (862 bytes) | §4 |
| MIN-4 | §3/§4 measured the `PS1` variable, never the expansion — where MAJ-1 lived | Every prompt reading in §3 and §4 is now `${PS1@P}` | §3, §4 |
| **NEW-MAJ-1** | MED-3's decline half tested `$f -ne 'CustomAction'`. `Get-PSReadLineKeyHandler` reports that name only for a script block bound **without** a `-BriefDescription`; with one — the idiomatic form — `Function` *is* the description, the guard passed, and every `Enter` called a static method that does not exist, so the line was never submitted and the tab could not run anything | The guard asks the question it meant to ask: `if([Microsoft.PowerShell.PSConsoleReadLine].GetMethod($f))`. Named function → chain; anything else → do not install | §6c — the four bindings decided correctly in both hosts, as a **test**, plus a live tab whose `Enter` was a described block before the init |
| **NEW-MIN-1** | The mark was safe at its end but still *began* with `\[`, so a user `PS1` ending in a lone `\` swallowed it and the region never opened | The mark is four **raw bytes**, `\001 ESC ]133;B BEL \002` — `\001`/`\002` are what `\[`/`\]` expand to, so there is no backslash at either end to merge with anything | §3 — `${PS1@P}` over five `PS1` shapes including `x\`, each with one start marker and one end marker |

## 1. What each shell emits, after the change

| Shell | A | B | C | D | How it was proven |
| --- | :-: | :-: | :-: | :-: | --- |
| `cmd.exe` | ✓ | ✓ | — | — | §2, live tab |
| pwsh 7 | ✓ | ✓ | ✓ | ✓ | §2, live tab |
| Windows PowerShell 5.1 | ✓ | ✓ | ✓ | ✓ | §2, live tab |
| bash (local) | ✓ | ✓ | ✓ | ✓ | §2 live tab, §3 the generated strings **expanded** in a real bash |
| zsh (local) | ✓ | ✓ | — | ✓ | §5, string only — no zsh on this host |
| bash / zsh (SSH) | ✓ | ✓ | ✓ | ✓ | §4, the bash branch run through a real bash; zsh branch string only |

## 2. The live tab (the strongest evidence): OneTerm `fast-dev`, `RUST_LOG=debug`

One run per shell kind. Each run writes `target/terminal.json` with that `shell.kind`,
starts `target/fast-dev/oneterm.exe`, types `cmd /c exit 3` then `cmd /c exit 0` into the
tab with posted `WM_CHAR` / `WM_KEYDOWN`, and reads back the `OscRouter: shell mark …`
lines the router logs. This is the whole chain — real ConPTY, real shell, OneTerm's own
engine and router — not a simulation of it.

```
kind          : cmd
marks in order: PromptStart PromptEnd PromptStart PromptEnd PromptStart PromptEnd
distinct      : PromptStart PromptEnd
OutputEnd     : (none)

kind          : bash            <- (exit 7) and true, not cmd /c exit
marks in order: PromptStart PromptEnd OutputStart OutputEnd PromptStart PromptEnd OutputStart OutputEnd PromptStart PromptEnd
distinct      : PromptStart PromptEnd OutputStart OutputEnd
OutputEnd     : OutputEnd { exit_code: Some(7) } | OutputEnd { exit_code: Some(0) }

kind          : pwsh
marks in order: PromptStart PromptEnd OutputStart OutputEnd PromptStart PromptEnd OutputStart OutputEnd PromptStart PromptEnd
distinct      : PromptStart PromptEnd OutputStart OutputEnd
OutputEnd     : OutputEnd { exit_code: Some(3) } | OutputEnd { exit_code: Some(0) }

kind          : powershell
marks in order: PromptStart PromptEnd OutputStart OutputEnd PromptStart PromptEnd OutputStart OutputEnd PromptStart PromptEnd
distinct      : PromptStart PromptEnd OutputStart OutputEnd
OutputEnd     : OutputEnd { exit_code: Some(3) } | OutputEnd { exit_code: Some(0) }
```

Four things are in that output at once:

- **`cmd /c exit 3` inside pwsh produces `D;3`** — `OutputEnd { exit_code: Some(3) }` — and
  `cmd /c exit 0` produces `D;0`, so the code is the command's and not a stale
  `$LASTEXITCODE`.
- **The order is `A B C D`**, once per command, which is what the OSC 133 proposal asks for
  and what the `US-0133` fast path reads.
- **No `D` before the first command**: the first pair is `A B`, not `D A B`.
- **`cmd.exe` is unchanged** and still emits exactly `A` and `B`.

Frames. The point of each is that nothing shows: the prompt renders exactly as it would
without the integration, so the markers really are zero-width and the `%{…%}` / `\[…\]`
accounting is right. (`US-0133` is what will make the marks visible; it is not on this
branch.)

- `US-0136-verify-pwsh-tab.png` — the PowerShell 7 tab after both commands.
- **`US-0136-rework-bash-tab.png`** — the bash tab after `(exit 7)` and `true`. Three
  prompts, each `$ ` and nothing else. Compare the verifier's
  `US-0136-verify2-bash-stray-bracket.png`, where the same rows read `$ ](exit 7)`.
- **`US-0136-rework-bash-completion.png`** — the other half of MAJ-1. The stray `]` landed
  inside the `Semantic::Input` region `B` opens, so OneTerm's completion popup offered
  `]true` and `](exit`. After three commands and the prefix `tru`, the popup now offers
  `true` and `truncate`.

Windows PowerShell 5.1 reaches the same result as pwsh 7, so the `-Command` init works on
both hosts.

## 3. Local bash — the generated strings through a real bash, **expanded**

`BASH_OSC133_PROMPT_COMMAND` and `BASH_OSC133_PS0` are read out of
`crates/core/src/config/shell.rs` and evaluated in the current shell of a real
bash 5.3.15, so `__ot_seen` persists exactly as it does when bash expands
`PROMPT_COMMAND` itself.

The first version of this section printed the `PS1` **variable** and missed `MAJ-1`
entirely — the defect lived in the difference between the variable and its expansion.
Every prompt reading below is `${PS1@P}`, byte for byte through `od -c`.

```
wire     : ESC]7;file://…ESC\ ESC]133;AESC\                 <- first prompt: no D
           ESC]133;D;7ESC\ ESC]7;file://…ESC\ ESC]133;AESC\ <- after (exit 7)
           ESC]133;D;0ESC\ ESC]7;file://…ESC\ ESC]133;AESC\ <- after true
PS0 EXP  : 033 ] 1 3 3 ; C 033 \
status   : 9        <- $? restored for whatever runs next
user-sees=5         <- a user PROMPT_COMMAND appended after ours still sees its
                       own command's exit status
```

**The mark concatenated onto five `PS1` shapes** (NEW-MIN-1). `001`/`002` are
`RL_PROMPT_START_IGNORE` / `RL_PROMPT_END_IGNORE`; the count of each is the region
balance, and the user's own bytes are unchanged in every row:

```
user PS1            ${PS1@P}                                          markers
x                   x 001 033 ] 1 3 3 ; B \a 002                      1 / 1
x\                  x \ 001 033 ] 1 3 3 ; B \a 002                    1 / 1   <- NEW-MIN-1
x\\                 x \ 001 033 ] 1 3 3 ; B \a 002                    1 / 1
\u@\h:\w\$          trunglt@TrungLT-PC:/…$ 001 033 ] 1 3 3 ; B \a 002   1 / 1
\[\e[32m\]u\[\e[0m\]$  033 [ 3 2 m u 033 [ 0 m $ 001 033 ] 1 3 3 ; B \a 002   1 / 1
```

Five `PROMPT_COMMAND` expansions still leave exactly one copy: the `case` guard compares
the same bytes it appends.

## 4. The SSH bootstrap — the typed line through a real bash and a real dash

```
bash -n boot.sh   -> exit 0        (parses)
dash -n boot.sh   -> exit 0        (parses; the zsh array assignments are inside
                                    an `eval`, so a POSIX sh does not choke on
                                    `name=(...)` at parse time)
dash  boot.sh     -> emits one OSC 7 + 133;A and nothing more, as before
bash  boot.sh + two commands ->
        ESC]7;…ESC\ ESC]133;AESC\                    <- the FIRST prompt: no D
        ESC]133;D;7ESC\ ESC]7;…ESC\ ESC]133;AESC\
        ESC]133;D;0ESC\ ESC]7;…ESC\ ESC]133;AESC\
        PROMPT_COMMAND = __oneterm_precmd
        PS0 EXP        = ESC]133;CESC\
        PS1 EXP        = x \ 001 033 ] 1 3 3 ; B \a 002
                         (with a user PS1 of `x\`: the mark arrives whole)
line size = 880 bytes + CR, well under MAX_CANON (4096)
```

## 5. Unit tests

`cargo test -p oneterm-core -p oneterm-ssh -p oneterm-terminal` — all green. The tests this
packet added or changed:

- `oneterm-core` `config::shell::tests`: `cmd_prompt_emits_the_prompt_region_only`,
  `zsh_ps1_carries_the_exit_code_and_one_backslash_after_esc`,
  `bash_emits_the_full_mark_set`,
  `bash_preserves_the_exit_status_and_skips_the_first_prompt`,
  `bash_appends_to_a_user_prompt_command`, `bash_keeps_a_user_ps0`,
  `powershell_init_emits_the_full_mark_set`, and the existing
  `generated_windows_prompts_emit_osc_7` / `zsh_kind_injects_ps1_verbatim`.
  **New in the rework:** `bash_ps1_mark_survives_prompt_expansion` (MAJ-1) and
  `bash_yields_to_a_prompt_command_that_already_emits_osc133` (MED-2).
- `oneterm-ssh` `session::tests`: `shell_integration_bootstrap_installs_the_full_mark_set`
  (now also asserting the MED-1 flag reset), `shell_integration_bootstrap_branches_on_the
  _remote_shell` (now also asserting the MAJ-1 prompt-escape form and that the old byte
  form is gone), and the existing `shell_integration_bootstrap_exports_colorterm_first`.
- `oneterm-local-shell` `session::session_tests`: **`bash_prompt_draws_no_stray_bracket`**,
  the behavioural half of MAJ-1. It spawns a real bash through the real PTY, runs a command
  so a whole prompt is known to have been drawn, and asserts the grid holds no `]`. It
  skips rather than fails where there is no bash.

**The MAJ-1 mutation, run both ways.** Reverting `BASH_OSC133_PROMPT_COMMAND` to the exact
shipped append (`__ot_b=$'\033]133;B\033\\'; __ot_b="\[$__ot_b\]"`) makes
`bash_prompt_draws_no_stray_bracket` fail with the verifier's own symptom, reproduced
independently:

```
the generated bash prompt must print no bracket (`US-0136` MAJ-1); grid:
"trunglt@TrungLT-PC MSYS ~   $ ]echo oneterm-prompt-drawn   oneterm-prompt-drawn"
                               ^
```

Restored, it passes. The first draft of the test did **not** catch it — it keyed on
`cwd()`, which arrives from inside `PROMPT_COMMAND` before the prompt row is written, and
then on the first `$`, which appears before the marker at the end of the row. Only keying
on the shell finishing a command makes the wait deterministic. That is worth recording,
because it is the same class of mistake as MIN-4: measuring something adjacent to the
thing that breaks.

## 6. The PowerShell exit-code rule, measured

The rule is `$?` first and `$LASTEXITCODE` only as the number for a failure. Both halves
were measured rather than assumed:

```
pwsh 7 :       cmd /c exit 3  -> $? = False, $LASTEXITCODE = 3
               Get-Item .     -> $? = True,  $LASTEXITCODE = 3   (stale)
powershell 5.1: cmd /c exit 3 -> $? = False, $LASTEXITCODE = 3
```

`$LASTEXITCODE` alone would report 3 for the successful `Get-Item`; `$?` alone carries no
number. Driving the wrapped `prompt` function directly in both hosts gives `D;3` after
`cmd /c exit 3` and `D;0` after the successful cmdlet with `$LASTEXITCODE` still 3.

## 6b. The reworked PowerShell init, driven in both hosts

The `-Command` string is read out of `crates/core/src/config/shell.rs`, a profile-style
`Set-PSReadLineKeyHandler -Key Enter -Function ValidateAndAcceptLine` is bound *before* it
runs, and the wrapped `prompt` is then called the way the host calls it:

```
Enter before  : ValidateAndAcceptLine
Enter after   : CustomAction              <- OneTerm's script block
chained to    : ValidateAndAcceptLine     <- MED-3: it calls what was bound
first         : ESC]7;…ESC\ ESC]133;AESC\ PS …> ESC]133;BESC\      (no D)
after exit 3  : ESC]133;D;3ESC\ ESC]7;…ESC\ ESC]133;AESC\ PS …> ESC]133;BESC\
throwing      : ESC]7;…ESC\ ESC]133;AESC\ PS …> ESC]133;BESC\     <- MIN-1: B survives
two-value     : top<NL>bot> ESC]133;BESC\                        <- MIN-2: line kept
```

Identical in pwsh 7 and Windows PowerShell 5.1. With `Enter` already bound to a *script
block* before the init runs:

```
Enter before  : CustomAction
Enter after   : CustomAction
chained to    :                 <- empty: OneTerm did not install, and that
                                   user's Enter is untouched
```

## 6c. NEW-MAJ-1: the `Enter` guard, as a test and in a live tab

**The four bindings, in both hosts.** `powershell_enter_guard_decides_the_four_bindings`
(`crates/core`) writes the real init into a temp script behind each binding, runs it in
`powershell.exe` and `pwsh.exe`, and reads back whether the guard installed and what it
chained to. This is a `cargo test`, not a probe: the previous round's string assertion
could not have seen this and locked the defect in instead.

| `Enter` bound as | `.Function` reports | `GetMethod` | verdict |
| --- | --- | :-: | --- |
| nothing (default) | `AcceptLine` | yes | `INSTALLED:AcceptLine` |
| `-Function ValidateAndAcceptLine` | `ValidateAndAcceptLine` | yes | `INSTALLED:ValidateAndAcceptLine` |
| `-ScriptBlock { … }` | `CustomAction` | no | `DECLINED` |
| `-ScriptBlock { … } -BriefDescription 'SmartEnter'` | `SmartEnter` | no | `DECLINED` |

**The mutation.** Putting `if($f -ne 'CustomAction')` back makes that test fail on exactly
the row the re-verification named, and only that row:

```
assertion `left == right` failed: powershell.exe with described: … -BriefDescription 'SmartEnter'
  left: "INSTALLED:SmartEnter"
 right: "DECLINED"
```

**Live, in a real tab.** A `custom` shell running the very text of
`POWERSHELL_OSC133_PROMPT_INIT` behind
`Set-PSReadLineKeyHandler -Key Enter -ScriptBlock {…} -BriefDescription 'SmartEnter'` — the
binding has to precede the init, and OneTerm always puts its own args first, so the tab
runs the same script from a file. `cmd /c exit 3` then `Write-Output oneterm-accepted`,
typed in:

```
host/binding : pwsh-described / powershell-described
marks        : PromptStart PromptEnd  x3        <- A and B only, no C, no D
```

and the frame `US-0136-verify4-pwsh-described-enter.png` shows both commands accepted and
`oneterm-accepted` printed. With the old guard this tab could not have run either command.
The undescribed block declines the same way (`pwsh-plain`: three prompts, no `C`).

`D` is absent along with `C` by construction, not by accident: `D` is gated on the flag
only the `C` handler sets, so a tab without `C` reports no completed command rather than a
wrong one.

## 7. Gate

`pwsh scripts/ci-local.ps1` — see the packet's Evidence section for the final line.

## 8. Gaps

- **zsh is unproven as behaviour.** No zsh on this host and no WSL, so
  `ZSH_OSC133_PS1` is proven as bytes only. It is one `PS1` string using zsh's own
  documented `%?` and `%{…%}`, so the risk is low, but it is untested.
- **The SSH bootstrap's zsh branch is unproven as behaviour**, for the same reason, and no
  SSH server was reachable to run either branch end to end. Both branches parse, and the
  bash branch was run in a real bash.
- **The SSH bootstrap is not proven in an *interactive* shell under a PTY.** Its code was
  run in a real bash the way bash runs it, not by bash's own prompt loop over a real
  `russh` channel. Local bash no longer has this gap: `bash_prompt_draws_no_stray_bracket`
  and the `kind=bash` walk both drive a real interactive bash through ConPTY.
- **`fish`, `csh` and `tcsh` over SSH get a burst of parse errors** and no integration at
  all: the bootstrap is one POSIX line and reaches no branch in them. OneTerm cannot see
  this — the channel write succeeded. Stated in `docs/terminal-backend.md` §6.1.2 rather
  than fixed; detecting the remote shell before typing would need a round trip the connect
  path does not have.
- **A negative `$LASTEXITCODE`** (a native crash, e.g. `-1073741819`) reports `D;1`.
  Non-zero, which is all the tint reads, but the number is lost.
- **A `.bashrc` that assigns an array `PROMPT_COMMAND=(…)`** (bash >= 5.1) discards
  OneTerm's scalar and the integration stops; a `.zshrc` that sets `PROMPT` replaces the
  generated `PS1` and takes the marks with it. Both are ceilings of the env-only route,
  now recorded in §6.1.2.
- **A `GetMethod` that throws.** `Type.GetMethod(name)` raises `AmbiguousMatchException`
  on an overloaded name, and `PSConsoleReadLine` has seven overloaded public statics
  (`Insert`, `ReadLine`, `SetKeyHandler`, …). None is a plausible `Enter` binding, and the
  failure is safe if it ever happened: the exception aborts the rest of the init, which has
  already installed the prompt wrapper, so the tab keeps the user's `Enter` and reports no
  `C`.
- **PowerShell's `C` fires on `Enter` even when the line is incomplete** (a `{` left open
  continues on the next line, and the handler has already written `C`). PSReadLine gives
  the handler no way to ask whether `AcceptLine` accepted. The cost is one early `C`; the
  real `C` for the finished command follows.
- **A failing *cmdlet*** reports whatever non-zero `$LASTEXITCODE` last held, which may
  belong to an older native command. It is non-zero either way, which is all the tint
  reads.
- **A prompt redrawn without a command** (Ctrl+C on an empty line) emits another `D`.
