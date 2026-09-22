# Evidence: US-0136 — the shell integrations emit the full OSC 133 set

Date: 2026-09-22
Intake: `IN-0044`
Packet: `US-0136-shell-integrations-emit-the-full-osc-133-set.md`
Host: Windows 11 Enterprise 10.0.26200, `bash` 5.3.15 (Git for Windows), `dash` (Git for
Windows), pwsh 7, Windows PowerShell 5.1. No Unix host, no WSL, no zsh, no SSH server.

## 1. What each shell emits, after the change

| Shell | A | B | C | D | How it was proven |
| --- | :-: | :-: | :-: | :-: | --- |
| `cmd.exe` | ✓ | ✓ | — | — | §2, live tab |
| pwsh 7 | ✓ | ✓ | ✓ | ✓ | §2, live tab |
| Windows PowerShell 5.1 | ✓ | ✓ | ✓ | ✓ | §2, live tab |
| bash (local) | ✓ | ✓ | ✓ | ✓ | §3, the generated strings run through a real bash |
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

Frame: `US-0136-verify-pwsh-tab.png` — the PowerShell 7 tab after both commands. The point
of the frame is that nothing shows: the prompt renders exactly as before, so the markers
are zero-width on the wire and the `%{…%}` / `\[…\]` accounting is right. (`US-0133` is
what will make the marks visible; it is not on this branch.)

Windows PowerShell 5.1 reaches the same result, so the `-Command` init works on both hosts.

## 3. Local bash — the generated strings through a real bash

`BASH_OSC133_PROMPT_COMMAND` and `BASH_OSC133_PS0` were read out of
`crates/core/src/config/shell.rs` and evaluated in the current shell of a real
bash 5.3.15, so `__ot_seen` persists exactly as it does when bash expands
`PROMPT_COMMAND` itself:

```
wire  : ESC]7;file://…ESC\ ESC]133;AESC\                 <- first prompt: no D
        ESC]133;D;7ESC\ ESC]7;file://…ESC\ ESC]133;AESC\ <- after (exit 7)
        ESC]133;D;0ESC\ ESC]7;file://…ESC\ ESC]133;AESC\ <- after true
status-after=9                 <- $? restored for whatever runs next
ps1   : \u@\h:\w\$ \[ESC]133;BESC\\]   <- B appended once, after four expansions
ps0   : ESC]133;CESC\                  <- ${PS0@P}, i.e. what bash will print
user-sees=5                    <- a user PROMPT_COMMAND appended after ours
                                  still sees its own command's exit status
```

## 4. The SSH bootstrap — the typed line through a real bash and a real dash

```
bash -n boot.sh   -> exit 0        (parses)
dash -n boot.sh   -> exit 0        (parses; the zsh array assignments are inside
                                    an `eval`, so a POSIX sh does not choke on
                                    `name=(...)` at parse time)
dash  boot.sh     -> emits one OSC 7 + 133;A and nothing more, as before
bash  boot.sh + two commands ->
        ESC]133;D;7ESC\ ESC]7;…ESC\ ESC]133;AESC\
        ESC]133;D;0ESC\ ESC]7;…ESC\ ESC]133;AESC\
        PROMPT_COMMAND = __oneterm_precmd
        PS0            = ESC]133;CESC\
        PS1            = \[ESC]133;BESC\\]
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
- `oneterm-ssh` `session::tests`: `shell_integration_bootstrap_installs_the_full_mark_set`,
  `shell_integration_bootstrap_branches_on_the_remote_shell`, and the existing
  `shell_integration_bootstrap_exports_colorterm_first`.

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

## 7. Gate

`pwsh scripts/ci-local.ps1` — see the packet's Evidence section for the final line.

## 8. Gaps

- **zsh is unproven as behaviour.** No zsh on this host and no WSL, so
  `ZSH_OSC133_PS1` is proven as bytes only. It is one `PS1` string using zsh's own
  documented `%?` and `%{…%}`, so the risk is low, but it is untested.
- **The SSH bootstrap's zsh branch is unproven as behaviour**, for the same reason, and no
  SSH server was reachable to run either branch end to end. Both branches parse, and the
  bash branch was run in a real bash.
- **Local bash and the bootstrap are not proven in an *interactive* shell under a PTY.**
  `PROMPT_COMMAND`, `PS0` and the `PS1` append were exercised by evaluating them the way
  bash does, not by bash's own prompt loop.
- **PowerShell's `C` fires on `Enter` even when the line is incomplete** (a `{` left open
  continues on the next line, and the handler has already written `C`). PSReadLine gives
  the handler no way to ask whether `AcceptLine` accepted. The cost is one early `C`; the
  real `C` for the finished command follows.
- **A failing *cmdlet*** reports whatever non-zero `$LASTEXITCODE` last held, which may
  belong to an older native command. It is non-zero either way, which is all the tint
  reads.
- **A prompt redrawn without a command** (Ctrl+C on an empty line) emits another `D`.
