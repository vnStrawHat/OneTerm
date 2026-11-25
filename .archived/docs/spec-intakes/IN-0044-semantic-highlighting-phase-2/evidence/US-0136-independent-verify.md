# Independent verification — `US-0136`

Date: 2026-09-22
Subject: `feat/shell-integration-osc133-full-set` @ `c73c7a7d` (main merged in)
Verifier: an independent session, adversarial brief, no access to the implementer's
reasoning beyond the committed packet and `US-0136-verify.md`.
Host: Windows 11 Enterprise 10.0.26200; bash 5.3.15 and dash (Git for Windows);
pwsh 7.6.6; Windows PowerShell 5.1.26100. No Unix host, no zsh, no SSH server.

## Verdict

**FAIL — one major regression. The design is sound; it ships after a one-line fix in each
of two places.**

The packet's headline outcome is real and I reproduced all of it independently: the
`A`/`B`/`C`/`D;<code>` set now reaches OneTerm's router from pwsh, Windows PowerShell,
**and local bash**, in the right order, with the right codes, and with no `D` before the
first local command. The bash behaviour that the implementer could only prove as *strings*
("bash and zsh are unit-tested only") I proved as *behaviour*, twice — in a real
interactive bash and in a real OneTerm bash tab. That gap is now closed and it closes
green.

But the same walk shows what no unit test and no probe in `US-0136-verify.md` could:
**every bash prompt, local and over SSH, now ends with a stray `]` character.** The `B`
marker is appended to `PS1` in a form that bash's own prompt expansion mangles, and the
defect is visible on screen, is new in this packet, and leaks into OneTerm's completion
popup. Acceptance item "A user-supplied `PS1` is preserved" is not met in the sense the
user would mean it.

Two further claims in the packet and in the new `docs/terminal-backend.md` §6.1.2 are
false as written (`D` before the first command over SSH; the integration's opt-out).

| # | Finding | Rank |
|---|---|---|
| MAJ-1 | Every bash prompt (local **and** SSH) prints a stray `]`; the `\[…\]` region is left unclosed | **major** |
| MED-1 | The SSH bootstrap emits `D;0` before the user's first command, in **both** branches; §6.1.2 says it does not | medium |
| MED-2 | The bash integration can no longer be opted out of, and §6.1.2 claims the opposite in its own text | medium |
| MED-3 | The PowerShell init silently **replaces** a user's PSReadLine `Enter` handler | medium |
| MIN-1 | A user `prompt` that throws loses `B` forever: `A` is written, then the wrapper propagates | minor |
| MIN-2 | A multi-value `prompt` return is collapsed with `-join ''`, dropping the host's own separator | minor |
| MIN-3 | The bootstrap line grew 2.7× (319 → 852 bytes) and is still typed with echo on | minor |
| MIN-4 | `US-0136-verify.md` §3/§4 prove the `PS1` *variable*, never the expanded prompt — which is where MAJ-1 lives | minor |

---

## What I reproduced, and what it cost

### The live walks (the strongest evidence, re-run from scratch)

OneTerm `fast-dev` @ `c73c7a7d`, `RUST_LOG=debug`, a fresh `target/terminal.json` per shell
kind, a scratch `HOME`/`USERPROFILE`, posted `WM_CHAR`/`WM_KEYDOWN` into the window of the
pid this session started, and the router's own
`log::debug!("OscRouter: shell mark {mark:?}")` (`crates/terminal/src/backend/osc_router.rs:234`)
read back out of the child's stderr.

```
kind=cmd         A B  A B  A B                                   (no C, no D)
kind=pwsh        A B C D;3  A B C D;0  A B
kind=powershell  A B C D;3  A B C D;0  A B
kind=bash        A B C D;7  A B C D;0  A B      <- NOT in US-0136-verify.md
```

`cmd /c exit 3` for the two PowerShell hosts, `(exit 7)` and `true` for bash. The
implementer's three rows reproduce byte for byte. The fourth row is new: a real Git for
Windows bash spawned by OneTerm through ConPTY, with only `resolve_shell`'s generated
`PROMPT_COMMAND` and `PS0` in its environment, over a `-l` login shell whose
`/etc/profile` sets its own `PS1`. Frames:
`US-0136-verify2-pwsh-tab.png`, `US-0136-verify2-cmd-tab.png`,
`US-0136-verify2-bash-stray-bracket.png`.

### Claim-by-claim

| Acceptance item (packet) | Verdict | Evidence |
|---|---|---|
| bash emits `A`,`B`,`C`,`D;<code>`; `$?` restored before a user hook | **PASS** | live bash tab above; real interactive bash gives `A B C D;7 A B C D;0 A B C`; a hook appended after OneTerm's sees `5` after `(exit 5)` |
| a user `PROMPT_COMMAND` is appended to, not replaced | **PASS** | `crates/core/src/config/shell.rs:442-447`; mutation below |
| a user `PS1` is preserved; `B` appended once and only once | **PARTIAL** | once: PASS (5 expansions → 1 marker). Preserved: **MAJ-1** |
| zsh emits `A`,`B`,`D;<code>` from `PS1`; `C` recorded as a limit | **UNVERIFIED** | no zsh on this host; string-level review only, see §zsh |
| PowerShell/pwsh emit all four; the original `prompt` still runs and shows | **PASS** | live walks; `USERPROMPT> ` survives the wrapper in both hosts |
| `cmd /c exit 3` inside pwsh → `133;D;3` | **PASS** | live walk, and driving the wrapper directly in pwsh 7 and 5.1 |
| `cmd.exe` unchanged, `A`+`B` only | **PASS** | live walk |
| the SSH bootstrap branches on the remote shell and exports `COLORTERM` first | **PASS (structure)** | `crates/ssh/src/session.rs:729`; both branches parse; bash branch run for real |
| **no `D` before the first command of a session** | **FAIL** | **MED-1** — SSH, both branches. Local bash and both PowerShell hosts: PASS |
| every generated string is unit-tested for its exact bytes | **PARTIAL** | `ZSH_OSC133_PS1` and `BASH_OSC133_PS0` are asserted as exact bytes; `BASH_OSC133_PROMPT_COMMAND` and `POWERSHELL_OSC133_PROMPT_INIT` are asserted as substrings, and nothing asserts the *expanded* `PS1` |

---

## MAJ-1 — every bash prompt now prints a stray `]`

`crates/core/src/config/shell.rs:188` ends the generated `PROMPT_COMMAND` with

```sh
case $PS1 in *"$__ot_b"*) ;; *) PS1="$PS1\[$__ot_b\]" ;; esac
```

`__ot_b` is `$'\033]133;B\033\\'` — ESC `]133;B` ESC `\`, i.e. it *already ends in a
literal backslash*. The appended text therefore puts three characters in a row into `PS1`:
the marker's `\`, then the `\` of the `\]` wrapper, then `]`. Bash's prompt expansion reads
the first two as the escape `\\` and emits one backslash, then prints `]` as an ordinary
character — and the `\]` that was supposed to close the non-printing region is gone.

Measured in bash 5.3.15, the same string a user's terminal receives:

```
PS1 variable   :  \ u @ \ h : \ w \ $  SP  \ [ 033 ] 1 3 3 ; B 033  \   \   ]
${PS1@P}       :  trunglt@…$ SP           033 ] 1 3 3 ; B 033  \   ]
                                                                    ^ printed
```

and in a real interactive bash the bytes on the wire before the echoed command are
`] 1 3 3 ; B 033 \ ]  e x i t`. The OSC 133 `B` sequence itself is well formed — this is
why the mark stream above is perfect — but a `]` follows it.

In the OneTerm bash tab (`US-0136-verify2-bash-stray-bracket.png`) the prompt reads

```
trunglt@TrungLT-PC MINGW64 ~
$ ](exit 7)
```

and the completion popup offers `]true` and `](exit` as history entries, because the stray
character lands inside the `Semantic::Input` region `B` opened
(`crates/vt/src/terminal/dispatch.rs:1017`). Readline's width accounting is also wrong: the
`\[` region never closes, so readline believes the `]` is invisible while the terminal
advances the cursor over it.

**The same line exists in the SSH bootstrap** — `crates/ssh/src/session.rs:729`,
`PS1="$PS1\[$(__oneterm_mark B)\]"` — and I measured the identical result there
(`${PS0@P}` is fine; `${PS1@P}` ends `033 \ ]`). So the defect is on the main SSH path as
well as the local one.

It is new in this packet: `main` appended nothing to `PS1` for bash.

**Fix** (both call sites), measured on this host:

```sh
# shipped   -> X 033 ] 1 3 3 ; B 033 \ ]
PS1="$PS1\[$__ot_b\]"
# either of these -> X 033 ] 1 3 3 ; B 033 \
PS1="$PS1"'\[\e]133;B\e\\\]'          # append the prompt-escape form
__ot_b=$'\033]133;B\033\\\\'          # or double the backslash; the `case` guard still matches
```

The second is a two-character change and keeps the idempotence guard working, because the
guard compares `PS1` against the same `$__ot_b` it appended. The SSH bootstrap needs the
equivalent edit at `crates/ssh/src/session.rs:729`.

**Why no test caught it.** `bash_emits_the_full_mark_set`
(`crates/core/src/config/shell.rs:538-551`, the assertion on line 548) asserts the
substring `PS1="$PS1\[$__ot_b\]"` is present — it asserts the *bug* is present. Nothing in the
packet, and nothing in `US-0136-verify.md` §3 or §4, expands `PS1`; both print the
variable (`ps1 : \u@\h:\w\$ \[ESC]133;BESC\\]`), and `\\]` in that dump is exactly the
two backslashes that merge. `${PS0@P}` *was* expanded, which is why `PS0` is correct.

A regression test that would have failed, purely at the Rust level: assert that the literal
appended to `PS1` is the prompt-escape form, not a value already carrying a raw backslash.
There is also a ready-made behavioural harness — `assert_powershell_prompt_emits_cwd`
(`crates/local-shell/src/session_tests.rs:134-154`) spawns a real shell through the real
PTY and asserts on `session.snapshot().text()`. A bash-kind sibling asserting the prompt
row carries no `]` would have caught this in `cargo test --workspace`.

---

## MED-1 — `D` before the first command, over SSH, in both branches

`crates/ssh/src/session.rs:729` ends with `…; __oneterm_precmd; stty echo 2>/dev/null`.
That direct call is what consumes the `__oneterm_seen` guard: it finds the flag unset,
skips `D`, and sets it. The **next** prompt — the first one the user sees, before they have
run anything — finds the flag set and emits `D;<status of stty echo>`.

Measured in bash:

```
. boot.sh          -> 033]7;…033\ 033]133;A033\            (no D, correct)
next precmd        -> 033]133;D;0033\ 033]7;…033\ 033]133;A033\
```

The zsh branch has the same shape for the same reason. This is precisely the condition the
packet's own Context says it is avoiding — "a `D` before any `C` would hand `US-0133` a
completed block that never ran" — and `docs/terminal-backend.md` §6.1.2 states flatly:
"No integration emits `D` before the first command of the session, except local zsh." That
sentence is wrong for `bash (SSH)` and `zsh (SSH)`; the packet's acceptance box "No `D` is
emitted before the first command of a session" is ticked while the doc already admits one
exception and reality has three.

Cheap fix: unset the flag after the direct call (`__oneterm_precmd; __oneterm_seen=`), or
call the body that skips `D` without setting the flag.

Impact is small — `SharedState::last_exit_code` gets a spurious `Some(0)` and one prompt
row is closed early — but the claim is load-bearing for `US-0133` and it is stated three
times.

---

## MED-2 — the bash integration can no longer be opted out of

`docs/terminal-backend.md` §6.1.2 says, in the same section, both:

> every generated variable yields to a user-supplied one, **which is the integration's
> opt-out**

and

> a user-supplied `PROMPT_COMMAND` is *appended to*

Before this packet `crates/core/src/config/shell.rs` guarded the bash branch with
`if !env.contains_key("PROMPT_COMMAND")`. It is now an unconditional `env.insert`
(`shell.rs:442-448`). Setting `PROMPT_COMMAND` in `terminal.json` no longer opts out of the
bash integration — it only moves the user's code behind OneTerm's. The same is true of
`PS1`: the doc says "A user-supplied `PROMPT`, `PS1` or `PS0` wins outright", but for bash
`PS1` is mutated at prompt time by `PROMPT_COMMAND` no matter what the user set.

This may be the intended trade (the packet argues for appending, and appending is the right
default). It is the *stated opt-out contract* of §6.1/§6.2 that is now inaccurate, and
there is no other opt-out for a local shell: `shell_integration` is a field of
`SshSessionConfig` (`crates/core/src/ssh_config.rs:211`, consumed at
`crates/ssh/src/session.rs:378`), and `LocalShellConfig` has no counterpart. Either restore
a way out or stop calling the yield rule an opt-out.

---

## MED-3 — a user's PSReadLine `Enter` handler is silently replaced

`crates/core/src/config/shell.rs:222-227` runs, unconditionally when the cmdlet resolves:

```powershell
Set-PSReadLineKeyHandler -Key Enter -ScriptBlock{
  [Microsoft.PowerShell.PSConsoleReadLine]::AcceptLine(); …
}
```

PowerShell's documented startup order runs `$PROFILE` before the `-Command` string (the
profile on this box is empty, so I could not measure the order without writing outside the
worktree), so this binding lands on top of whatever the profile bound. Measured in
pwsh 7.6.6, binding first and running the init second:

```
before init: Function=ValidateAndAcceptLine   Desc=Accept the input or move to the next line…
after  init: Function=CustomAction            Desc=User defined action
```

`ValidateAndAcceptLine` is a common profile setting; a user who had it loses parse
validation on Enter and gets plain `AcceptLine` instead, with no notice. The handler does
not chain to the previous binding (`Get-PSReadLineKeyHandler` would give it). This is the
one place the packet's "nothing replaces what the user has" rule is broken silently rather
than by design, and it is not in the packet's gap list.

The already-recorded early-`C`-on-an-incomplete-line cost is real and I confirm it is
unavoidable through this hook: `AcceptLine()` returns nothing.

---

## Minor findings

**MIN-1 — a throwing `prompt` loses `B` permanently.** The wrapper writes OSC 7 and `A`
with `[Console]::Write`, then evaluates `(& $global:__OneTermOriginalPrompt)` in the
*returned expression*. If the user's prompt throws, the exception propagates out of
`prompt`; the host falls back to `PS>` and the session keeps working, but `A` has already
gone out and `B` never does — on every prompt, forever. Measured in pwsh 7 and 5.1:

```
]7;…\]133;A\   PROMPT THREW: boom      (no ]133;B)
```

`docs/agents/error-policy.md` puts a failed optional UI refresh in the "log and continue"
row; a `try { … } catch { '' }` around the delegation would keep the region closed.

**MIN-2 — `-join ''` collapses a multi-value prompt.** `shell.rs:220` changed the returned
expression from `main`'s `& $original` (pipeline output, passed to the host as-is) to
`((& $original) -join '')`.
A `prompt` returning two objects — `function prompt { "top"; "bot> " }` — reaches the
wrapper as two items and leaves it as `topbot> `. Whatever separator the host would have
used is gone. I could not render a multi-value prompt in a real console host here, so the
user-visible size of this is unconfirmed; the collapse itself is measured.

**MIN-3 — the bootstrap line is 2.7× longer and still typed with echo on.**
319 → 852 bytes (the `\r` makes 853 on the wire). Comfortably under `MAX_CANON` (4096), so
no truncation risk, and `dash -n` and `bash -n` both parse it (exit 0) — the `eval` around
the zsh array assignments does its job, and switching `printf '\x1b…'` to `printf '\033…'`
is a portability improvement on paper, since `\xHH` is not POSIX (this dash honours it
anyway: the old line emits ESC under dash too, so nothing was broken before).
But there is no `stty -echo` before it, only `stty echo` after, so whatever of the line the
remote line discipline echoes is now 2.7× more noise. A remote `fish`, `csh` or `tcsh`
reaches no branch and cannot parse the *outer* POSIX syntax either — it gets a burst of
parse errors, and nothing in OneTerm notices, because
`send_shell_integration_bootstrap` (`crates/ssh/src/session.rs:732-742`) only reports a
channel-write failure. The session stays usable, which satisfies the error policy; the
"branches on the detected shell" wording in the packet oversells what the detection covers.

**MIN-4 — the implementer's own evidence measures the wrong thing for `PS1`.**
`US-0136-verify.md` §3 prints `ps1 : \u@\h:\w\$ \[ESC]133;BESC\\]` and §4 prints
`PS1 = \[ESC]133;BESC\\]`. Both are the variable. `PS0` in the same block *is* expanded
(`${PS0@P}`) and is correct. One more `${PS1@P}` would have found MAJ-1.

---

## Things I attacked that held

- **Injection through a user's `PROMPT_COMMAND`.** `format!("{OURS}; {user}")` with
  `my_hook`, `my_hook;` and `my_hook &` all parse (`bash -n`). Ours ends in a complete
  command, so no value of the user's can be swallowed by an unterminated construct.
- **`$?` restoration.** `( exit $__ot )` works: with a user hook appended, `(exit 5)` then
  the full `PROMPT_COMMAND` leaves the hook seeing `5`. Dropping the subshell is caught by
  `bash_preserves_the_exit_status_and_skips_the_first_prompt` (mutation below).
- **An adversarial `PS1`.** `PS1='$(touch /tmp/OWNED_PS1)`id`%s"x"'"'"'y'"'"'\'` — the
  `case $PS1 in` word is expanded once (no field splitting, no pathname expansion, and the
  *value* is not re-expanded), so nothing in it runs and the append is unaffected. The
  guard still matched and appended exactly once.
- **A cwd full of metacharacters.** `/tmp/us136/a$b` + backtick + `'` + `;` + `e` comes out
  of `printf '…%s%s…' "$HOSTNAME" "$PWD"` verbatim inside the OSC 7 payload, with no shell
  interpretation. (The payload is not percent-encoded, so a directory name containing a
  literal ESC or `;` would still break OSC 7 — unchanged by this packet.)
- **Idempotence.** Five `PROMPT_COMMAND` expansions leave exactly one `B` in `PS1`.
- **An array-valued `PROMPT_COMMAND` (bash ≥ 5.1).** Not reachable through the injection
  route: an environment variable is always a scalar, and `resolve_shell` only ever
  concatenates two strings. What *is* reachable is a `.bashrc` that assigns
  `PROMPT_COMMAND=(…)` after startup — that discards OneTerm's scalar outright and the
  integration silently stops. Not a defect in this packet's code; an unlisted ceiling of
  the env-only route, and §6.1.2 does not mention it.
- **`%` in a generated string.** Neither `CMD_OSC7_PROMPT` nor `ZSH_OSC133_PS1` embeds any
  user-controlled value, so a cwd or username containing `%`, `$`, backtick or a quote has
  nothing to break out of; both are compile-time constants and the shell substitutes the
  variable parts itself (`$P` for cmd, `%~`/`%n`/`%m` for zsh).
- **`set -e` in a user's rc.** OneTerm's hook ends on a non-zero subshell, which looked
  like a session-killer. It is not OneTerm's: plain interactive bash with `set -e` also
  exits on `false`, with and without the hook. Withdrawn.
- **`set -u`.** `${__ot_seen-}` is guarded. `case $PS1` is not, but interactive bash always
  has `PS1` set; the only way to trip it is `unset PS1` under `set -u`.
- **A user `PS1` that already uses `\[ \]`.** The user's own pairs survive; only the
  appended one is broken (MAJ-1).
- **`PS0`.** `\e]133;C\e\\` expands to exactly ESC `]133;C` ESC `\` — the trailing lone
  backslash the SSH branch stores raw survives prompt expansion too. A user `PS0` wins
  outright (`or_insert_with`), tested.
- **Engine acceptance.** Every producer terminates with ST (ESC `\`), never BEL, and every
  `D` uses the `133;D;<code>` form `osc_shell_mark`
  (`crates/vt/src/terminal/dispatch.rs:2104-2121`) parses as `i32`. cmd's `$E\`, zsh's
  `\x1b\\`, bash's `printf '…\033\\'`, `PS0`'s `\e\\`, PowerShell's `$e+'\'` — all one
  backslash on the wire. Checked each against the parser; no mismatch.
- **PowerShell's exit-code rule.** `$?` first, `$LASTEXITCODE` only as the number:
  `cmd /c exit 3` → `D;3`; a successful `Get-Item` with `$LASTEXITCODE` still 3 → `D;0`.
  Both hosts. A negative `$LASTEXITCODE` (`-1073741819`, a native crash) reports `D;1` —
  non-zero, which is all the tint reads, but the number is lost; worth one line in the
  packet's gap list.
- **A `prompt` defined before the init.** The wrapper captures it and still calls it:
  `USERPROMPT> ` renders between `A` and `B` in both hosts. (Whether `$PROFILE` really runs
  before `-Command` is documented rather than measured here — see MED-3.)
- **`dash`.** Reaches neither branch, emits exactly one OSC 7 followed by `ESC]133;A ESC\`
  and nothing more, and parses (`dash -n` exit 0). Matches §6.1.2's `sh`/`dash` row.

## Mutations (as briefed)

| Mutation | Expected | Result |
|---|---|---|
| bash `PROMPT_COMMAND` replaces instead of appends (`shell.rs:442-447` → `let value = BASH_OSC133_PROMPT_COMMAND.to_string();`) | `bash_appends_to_a_user_prompt_command` fails | **fails, and only it** — 18 passed, 1 failed |
| drop `; ( exit $__ot )` from the end of `BASH_OSC133_PROMPT_COMMAND` | the status test fails | `bash_preserves_the_exit_status_and_skips_the_first_prompt` **fails, and only it** — 18 passed, 1 failed |

Both restored; `git status` clean before the gate.

A third mutation is implied by MAJ-1 and is the point of it: **removing the `\[ \]` wrapper
entirely changes no test**, because no test expands `PS1`.

## Commands

```powershell
git reset --hard c73c7a7d
cargo build -p oneterm-app --profile fast-dev            # CARGO_BUILD_JOBS=4
pwsh walk.ps1 -Kind pwsh       'cmd /c exit 3' 'cmd /c exit 0'
pwsh walk.ps1 -Kind powershell 'cmd /c exit 3' 'cmd /c exit 0'
pwsh walk.ps1 -Kind cmd        'cmd /c exit 3' 'cmd /c exit 0'
pwsh walk.ps1 -Kind bash -Program 'C:\Program Files\Git\bin\bash.exe' '(exit 7)' 'true'
cargo test -p oneterm-core -p oneterm-ssh -p oneterm-terminal
pwsh scripts/ci-local.ps1
```

```bash
# the generated strings driven the way bash drives them
bash --norc --noprofile drive.sh       # PROMPT_COMMAND / PS0 / ${PS1@P} / idempotence
bash --norc --noprofile boot-drive.sh  # the SSH bootstrap, bash branch
printf '(exit 7)\ntrue\nexit 0\n' | PS1='p ' PS0='\e]133;C\e\\' \
  PROMPT_COMMAND="$PC" bash --norc -i     # A B C D;7 A B C D;0 A B C
dash -n boot.sh; bash -n boot.sh; dash boot.sh
pwsh -NoProfile -File ps-drive.ps1 ; powershell -NoProfile -File ps-drive.ps1
```

Scripts are in this session's scratch directory, not committed; each is a few lines and the
transcript above quotes their output verbatim.

## Gate

`cargo test -p oneterm-core -p oneterm-ssh -p oneterm-terminal` — green
(86 / 102 / 218 plus the single-test binaries).

`pwsh scripts/ci-local.ps1` — every step green, ending:

```
ci-local: all checks passed.
```

The gate passing is not evidence against MAJ-1: no check in it renders a bash prompt, and
the one unit test that touches the `PS1` append asserts the defective literal.

## Gaps in this verification

- **zsh is still unproven**, local and remote. No zsh and no WSL on this host. I reviewed
  `ZSH_OSC133_PS1` (`shell.rs:172`) and the bootstrap's zsh branch as strings only:
  `%{…%}` opens and closes once per group, `%?` is zsh's documented last-status escape, the
  content of each group carries no `%`, and the ST is one backslash. The local `PS1` has no
  `__ot_seen` equivalent, so local zsh emits `D` on its first prompt — which §6.1.2 does
  record. Two things I could not check: whether a `.zshrc` that sets `PROMPT` (most themes
  do) drops the markers outright, and whether `precmd_functions=(…)` inside the bootstrap's
  `eval` survives a theme that rewrites the array.
- **No SSH server was reachable**, so MED-1 is proven by running the bootstrap's own code
  in a real bash rather than through a real `russh` channel. The line itself is identical;
  what is untested is the echo behaviour and the timing against `request_shell`.
- **MIN-2's user impact is unconfirmed** — I could not obtain a real PowerShell console
  host under a pty to compare the native rendering of a multi-value prompt.
- **`cfg(unix)` paths are unverifiable on this host**, as always here.
- I did not re-run the `US-0133` consumer; this verification is about the producer only.

---

# Re-verification of `56524fb1`

Date: 2026-09-22
Subject: `feat/shell-integration-osc133-full-set` @ `56524fb1`, one commit on `6045dfe1`
(the FAIL above). Same host, same method; only the findings above were re-checked, plus
whatever the fixes themselves introduce.

## Verdict

**FAIL — the four findings are fixed, and the fix for MED-3 introduces a new major.**

MAJ-1 and MED-1 are closed conclusively, by measurement and by a mutation that brings the
old symptom straight back. MED-2, MIN-1 and MIN-2 are closed. MED-3's *chaining* half works
in both hosts. But its *decline* half rests on `Get-PSReadLineKeyHandler` reporting the
string `CustomAction` for a script-block binding, and it only does that when the block was
bound **without** a `-BriefDescription`. Bind one with a description — the idiomatic form,
and the one PSReadLine's own examples use — and `Function` is that description, the guard
passes, and OneTerm installs a handler whose body calls a static method that does not
exist. That user's `Enter` throws instead of submitting.

Before the rework that user lost `ValidateAndAcceptLine` and kept a working terminal. After
it they keep nothing: the fix made their case strictly worse. That is what the FAIL is for,
and it is a one-expression change.

| # | Finding | Status |
|---|---|---|
| MAJ-1 | stray `]` on every bash prompt | **FIXED** — measured, mutation-proved, visible in the tab and the popup |
| MED-1 | `D` before the first command over SSH | **FIXED** — measured against the old line as a control |
| MED-2 | no opt-out for the bash integration | **FIXED** — with two residual ceilings, below |
| MED-3 | PSReadLine `Enter` replaced silently | **PARTIAL, and now worse.** Chaining works; the decline guard misses the common case (**NEW-MAJ-1**) |
| MIN-1 | a throwing `prompt` loses `B` | **FIXED** — both hosts |
| MIN-2 | `-join ''` collapses a multi-value prompt | **FIXED** — `[Environment]::NewLine` |
| MIN-3 | bootstrap length and echo | unchanged by design; now 862 bytes |
| MIN-4 | the evidence measured the variable, not the expansion | **FIXED** — `US-0136-verify.md` now expands |
| NEW-MAJ-1 | a profile `Enter` script block **with** a `-BriefDescription` makes `Enter` throw | **new, major** |
| NEW-MIN-1 | a user `PS1` ending in a lone backslash swallows the mark's `\[` | new, minor |

---

## NEW-MAJ-1 — the `CustomAction` guard misses the common case, and `Enter` then throws

`crates/core/src/config/shell.rs:262-266`:

```powershell
$b=Get-PSReadLineKeyHandler -Bound|Where-Object{$_.Key -eq 'Enter'}|Select-Object -First 1;
$f=if($b){$b.Function}else{'AcceptLine'};
if($f -ne 'CustomAction'){ $global:__OneTermEnter=$f; Set-PSReadLineKeyHandler -Key Enter -ScriptBlock{
  $m=$global:__OneTermEnter;[Microsoft.PowerShell.PSConsoleReadLine]::$m(); ... }}
```

What `-Bound` actually reports for `Enter`, measured in pwsh 7.6.6 / PSReadLine 2.4.5 and in
Windows PowerShell 5.1.26100 / PSReadLine 2.0.0 — identical in both:

| How `Enter` was bound | `.Function` | shipped guard installs? | a real static method? |
| --- | --- | :-: | :-: |
| `-Function AcceptLine` (the default) | `AcceptLine` | yes | yes |
| `-Function ValidateAndAcceptLine` | `ValidateAndAcceptLine` | yes | yes |
| `-ScriptBlock { }` | `CustomAction` | **no** | no |
| `-ScriptBlock { } -BriefDescription 'SmartEnter'` | `SmartEnter` | **yes** | **no** |

The last row is the hole. `Function` carries the brief description when there is one, so the
literal comparison against `CustomAction` never fires, OneTerm stores `SmartEnter` in
`$global:__OneTermEnter`, and every `Enter` press evaluates
`[Microsoft.PowerShell.PSConsoleReadLine]::SmartEnter()`:

```
ENTER WOULD THROW: RuntimeException: Method invocation failed because
[Microsoft.PowerShell.PSConsoleReadLine] does not contain a method named 'SmartEnter'.
```

Measured in both hosts. The accept never happens, so the line is not submitted — the tab
cannot run a command. (What I measured is the invocation throwing; whether PSReadLine
swallows that exception or surfaces it needs a real console host, which I do not have here.
Either way nothing accepts the line.)

`-BriefDescription` is not an exotic option: it is how PSReadLine's own documentation binds
script blocks, and a described `Enter` block is exactly the "smart enter" pattern the MED-3
fix exists to protect.

**Fix**, measured on this host across all four rows and both hosts — replace the string
comparison with the question it was trying to ask, "can I call this by name afterwards":

```powershell
if([Microsoft.PowerShell.PSConsoleReadLine].GetMethod($f)){ ... }
```

| case | shipped guard | `GetMethod` guard |
| --- | :-: | :-: |
| `AcceptLine` | installs | installs |
| `ValidateAndAcceptLine` | installs | installs |
| script block, no description | declines | declines |
| script block, **with** description | **installs, and breaks Enter** | **declines** |

It contains no `"`, so the one-argument `-Command` rule still holds. OneTerm's own handler is
bound without a description, so it still reports `CustomAction`: a second run of the init
still declines, and that re-entrancy guard survives the change.

The unit test asserts the defective literal (`shell.rs:740`,
`assert!(init.contains("if($f -ne 'CustomAction'){"))`), so it locks the hole in rather than
catching it. The invariant it wants — reject any name that is not a public static of
`PSConsoleReadLine` — cannot be evaluated from Rust; this one needs the corrected expression
plus a line in the packet's gaps.

---

## MAJ-1 — fixed

The mark is now `\[\e]133;B\a\]`: prompt escapes, BEL-terminated, so no backslash ever
touches the closing `\]`. Both call sites carry the same literal
(`crates/core/src/config/shell.rs:188`, `crates/ssh/src/session.rs:729`).

**The expansion, measured in bash 5.3.15, old against new.** "Trailing" is everything left
after the `B` sequence and its terminator:

```
new   expanded = X 033 ] 1 3 3 ; B \a           trailing = []     len 0
old   expanded = X 033 ] 1 3 3 ; B 033 \ ]      trailing = []]    len 1
```

- **Idempotence.** Five expansions leave exactly one copy of the literal; `PS1` grows from 1
  to 15 characters and stops. The `case` pattern quotes the mark, so its `[` and `]` stay
  literal instead of acting as a glob bracket expression.
- **Region balance.** The literal holds exactly one `\[` and one `\]`, in that order, with
  `\a` — not a backslash — before the close. A user `PS1` carrying its own `\[...\]` pairs
  expands cleanly: `ESC[32m trunglt ESC[0m $ ESC]133;B BEL`.
- **Real interactive bash** under the generated environment: `A B C D;7 A B C D;0 A B C`,
  and the bytes drawn on the prompt row are `033 ] 1 3 3 ; B \a` followed immediately by the
  echoed command, with nothing in between.
- **The SSH bootstrap's `PS1`** expands to `u$ 033 ] 1 3 3 ; B \a`; `PS0` still expands to
  `033 ] 1 3 3 ; C 033 \`.
- **BEL reaches the engine.** The live bash tab reports `PromptEnd`, so the BEL-terminated
  `B` parses; `crates/vt/src/terminal/terminal_tests.rs:1781` already feeds `\x07` marks.

**In the app.** A real Git for Windows bash tab, `US-0136-verify3-bash-tab.png`:

```
trunglt@TrungLT-PC MINGW64 ~
$ (exit 7)
```

against the previous round's `$ ](exit 7)`. And in `US-0136-verify3-bash-completion.png` the
history rows now read `true` and `(exit`, not `]true` and `](exit`.

**The mutation.** Reverting `BASH_OSC133_PROMPT_COMMAND` to the shipped MAJ-1 form makes
`bash_prompt_draws_no_stray_bracket` fail with the reported symptom, quoting the grid:

```
the generated bash prompt must print no bracket (`US-0136` MAJ-1); grid:
"... trunglt@TrungLT-PC MSYS ~   $ \n]echo oneterm-prompt-drawn ..."
```

The test really runs here — it printed no skip message and spawned a real bash through the
real PTY in 0.58 s. Restored afterwards; `git status` clean before the gate.

---

## MED-1 — fixed

`crates/ssh/src/session.rs:729` now ends `__oneterm_precmd; __oneterm_seen=; stty echo`.
Driven in a real bash with no subshell between the calls — a pipeline is a subshell and
loses the flag, which is what made my first pass of this measurement read as a false
negative:

```
new bootstrap   prompt1                = 033 ] 7 ; file://...              <- no D
                prompt2 after (exit 7) = 033 ] 1 3 3 ; D ; 7 033 \ 033 ] 7 ; f...
                prompt3 after true     = 033 ] 1 3 3 ; D ; 0 033 \ 033 ] 7 ; f...
                $? after the hook      = 5
old bootstrap   prompt1                = 033 ] 1 3 3 ; D ; 0 033 \ 033 ] 7 ; f...   <- the defect
```

`dash -n` and `bash -n` both exit 0 on the new line; it is 862 bytes. The local bash
`PROMPT_COMMAND` behaves the same way: prompt 1 has no `D`, prompt 2 carries `D;7`.

---

## MED-2 — fixed, with two ceilings worth recording

`crates/core/src/config/shell.rs:497-512`: a user `PROMPT_COMMAND` whose value contains
`133;` is left untouched, and `PS0` with it; any other value is appended to; an absent one
gets OneTerm's. `bash_yields_to_a_prompt_command_that_already_emits_osc133` and
`bash_appends_to_a_user_prompt_command` both pass and pin the two halves. Section 6.1.2 now
states the three rules and no longer claims a general "every generated variable yields"
opt-out.

Two ceilings the section does not mention:

- The opt-out is a substring test on the value in `terminal.json`'s `shell.env`. A user
  whose own integration is installed by `.bashrc` — `eval "$(starship init bash)"`, a sourced
  `vte.sh` — sets nothing there and so cannot express it.
- `133;` is a loose match. A `PROMPT_COMMAND` that emits an unrelated sequence containing it
  (`printf '\033[133;1H'`, a cursor move to row 133) silently disables the whole
  integration. Contrived, but impossible to diagnose from the UI.

---

## MED-3's chaining half, MIN-1 and MIN-2 — fixed

Measured in pwsh 7.6.6 and Windows PowerShell 5.1, identical in both:

- **Chaining.** Default `Enter` gives `__OneTermEnter=AcceptLine`. A profile's
  `ValidateAndAcceptLine` gives `__OneTermEnter=ValidateAndAcceptLine`, and
  `[Microsoft.PowerShell.PSConsoleReadLine].GetMethod(...)` resolves it, so that user keeps
  validating. Dynamic `::$m()` works on both hosts.
- **MIN-1.** A `prompt` that throws now returns the fallback `PS <path>> ` plus
  `ESC]133;B ESC\`: the region closes, and `A` is no longer left open forever.
- **MIN-2.** `function prompt { "top"; "bot> " }` returns `top<NL>bot> ` plus `B`, so a
  two-line prompt stays two lines.
- **The exit-code rule still holds.** `cmd /c exit 3` gives `D;3`; a successful `Get-Item`
  with `$LASTEXITCODE` still 3 gives `D;0`.

---

## NEW-MIN-1 — a user `PS1` ending in a lone backslash swallows the mark's `\[`

The same class as MAJ-1, from the other side. The mark is now safe at its *end*; its
*beginning* is still a backslash, so a user `PS1` whose last character is an odd trailing
backslash merges with it:

```
user PS1 = x\    expanded = x \ [ 033 ] 1 3 3 ; B \a
```

The `[` prints and the non-printing region never opens, so readline counts the mark's nine
bytes as visible width. A trailing lone backslash in `PS1` is pathological, and this is not a
regression against `main`, which appended nothing. It is simply what
`bash_ps1_mark_survives_prompt_expansion` cannot see: that test reads the mark in isolation,
never concatenated to an arbitrary user prompt. `x\\`, `x%`, `x]`, `x\$ ` and a `PS1` full of
proper `\[...\]` pairs all expand cleanly.

---

## Live mark log, re-run

Fresh `target/terminal.json`, `docks.json` and `ui_config.json` per run; scratch `HOME`; own
pid; `RUST_LOG=debug`; the router's own `shell mark` line read back from the child's stderr.

```
kind=cmd         A B  A B  A B
kind=pwsh        A B C D;3  A B C D;0  A B
kind=powershell  A B C D;3  A B C D;0  A B
kind=bash        A B C D;7  A B C D;0  A B
```

All four match the packet. One incidental observation from a misconfigured run of my own
driver: when `shell.program` names something unrunnable the panel logs
`Failed to spawn local terminal session: ...` and creates an empty tree rather than
panicking, and an invalid `terminal.json` is quarantined — the error policy's "user input"
and "persistence" rows, behaving.

## Commands

```powershell
git reset --hard 56524fb1
cargo build -p oneterm-app --profile fast-dev
pwsh walk.ps1 -Kind cmd|pwsh|powershell 'cmd /c exit 3' 'cmd /c exit 0'
pwsh walk.ps1 -Kind bash '(exit 7)' 'true'          # WALK_PROGRAM = Git bash
cargo test -p oneterm-local-shell bash_prompt_draws_no_stray_bracket -- --nocapture
cargo test -p oneterm-core -p oneterm-ssh -p oneterm-local-shell -p oneterm-terminal
pwsh scripts/ci-local.ps1
```

```bash
bash --norc --noprofile r1.sh   # ${PS1@P}, five expansions, interactive marks, raw bytes
bash --norc --noprofile r2.sh   # old against new trailing bytes; bootstrap parse and expansion
bash --norc --noprofile r3.sh   # the MED-1 prompt sequence, with the old line as control
bash --norc --noprofile r4.sh   # adversarial user PS1 values
pwsh / powershell -NoProfile -File ps-drive3.ps1 , ps-med3c.ps1 , ps-fix.ps1
```

## Tests and gate

`cargo test -p oneterm-core -p oneterm-ssh -p oneterm-local-shell -p oneterm-terminal` —
green: 88, 102, 34 with 2 ignored, and 218.

`pwsh scripts/ci-local.ps1` — every step green, ending:

```
ci-local: all checks passed.
```

The gate passing is not evidence against NEW-MAJ-1: nothing in it binds a PSReadLine key
handler, and the one unit test that touches the guard asserts the defective literal.

## Gaps unchanged

zsh remains unproven, local and remote. No SSH server was reachable, so MED-1 is proved by
running the bootstrap's own text in a real bash rather than through a `russh` channel.
`cfg(unix)` is unverifiable here. And PSReadLine's handling of an exception thrown inside a
key handler was not observed in a real console host.

---

# Re-verification of `9e83e07d`

Date: 2026-09-22
Subject: `feat/shell-integration-osc133-full-set` @ `9e83e07d`, one commit on `100bf423`
after merging `main` @ `b5d83702`. Same host, same method; only `NEW-MAJ-1`, `NEW-MIN-1`,
the recorded `AmbiguousMatchException` gap, and the live mark logs were re-checked.

## Verdict

**PASS.**

Both findings are closed, each by a change that removes the *class* of defect rather than
the instance. The `Enter` guard now asks whether the bound name is callable instead of what
it is called, which is the question that was always being asked; a new test runs the real
init in both real hosts over the four bindings, and reverting the guard makes it fail on the
described row and only that row. The bash `B` mark is now four raw bytes with no backslash
anywhere, so neither end of the concatenation has anything prompt expansion can act on —
the seam that produced MAJ-1 and then NEW-MIN-1 no longer exists.

The `AmbiguousMatchException` gap is real but unreachable through the binding path and safe
through the only route that remains.

| # | Finding | Status |
|---|---|---|
| NEW-MAJ-1 | described `Enter` script block made `Enter` throw | **FIXED** — guard, test, mutation, and a live tab with a real `$PROFILE` |
| NEW-MIN-1 | a user `PS1` ending in a lone backslash swallowed the mark's `\[` | **FIXED** — raw `\001 ESC ]133;B BEL \002`, no `0x5c` at all |
| `AmbiguousMatchException` | overloaded names such as `Insert` | **safe** — unreachable via `-Function`, and the one remaining route declines correctly |
| MAJ-1, MED-1, MED-2, MIN-1, MIN-2 | earlier rounds | still fixed; re-measured where the change could have touched them |

---

## NEW-MAJ-1 — fixed

`crates/core/src/config/shell.rs:277-279` now reads

```powershell
$b=Get-PSReadLineKeyHandler -Bound|Where-Object{$_.Key -eq 'Enter'}|Select-Object -First 1;
$f=if($b){$b.Function}else{'AcceptLine'};
if([Microsoft.PowerShell.PSConsoleReadLine].GetMethod($f)){ ... }
```

**The five bindings, run through the real init extracted from the source, in both hosts.**
Identical results in pwsh 7.6.6 / PSReadLine 2.4.5 and Windows PowerShell 5.1.26100 /
PSReadLine 2.0.0:

| `Enter` bound to | init error | prompt wrapped | `Enter` before → after | chained to |
| --- | --- | :-: | --- | --- |
| `-Function AcceptLine` | none | yes | `AcceptLine` → `CustomAction` (OneTerm's) | `AcceptLine` |
| `-Function ValidateAndAcceptLine` | none | yes | `ValidateAndAcceptLine` → `CustomAction` | `ValidateAndAcceptLine` |
| `-ScriptBlock { }` | none | yes | `CustomAction` → **unchanged** | none, so no `C` |
| `-ScriptBlock { } -BriefDescription 'SmartEnter'` | none | yes | `SmartEnter` → **unchanged** | none, so no `C` |
| `-ScriptBlock { } -BriefDescription 'Insert'` | `AmbiguousMatchException` | **yes** | `Insert` → **unchanged** | none, so no `C` |

The fourth row is the one that broke before: the guard now declines it, the user keeps their
handler, and nothing calls a method that does not exist.

**The test.** `powershell_enter_guard_decides_the_four_bindings`
(`crates/core/src/config/shell.rs:868-903`) writes the generated init to a temp script, runs
it in `powershell.exe` and `pwsh.exe` with each binding already applied, and reads back
`INSTALLED:<name>` or `DECLINED`. It passes here in 2.92 s — eight real host launches, so it
is not silently skipping — and asserts `hosts > 0` so an all-skip cannot read as a pass.

**The mutation.** Putting `if($f -ne 'CustomAction'){` back makes it fail on the described
row, and only there; the three rows before it passed in the same run:

```
powershell.exe with described: Set-PSReadLineKeyHandler -Key Enter -ScriptBlock
  { [Microsoft.PowerShell.PSConsoleReadLine]::AcceptLine() } -BriefDescription 'SmartEnter'
  left: "INSTALLED:SmartEnter"
 right: "DECLINED"
```

Restored; `git status` clean apart from this evidence.

**Live, with a real profile.** A `$PROFILE` in the walk's scratch home — `$PROFILE` follows
the redirected `USERPROFILE`, so nothing outside the worktree was touched — binding `Enter`
to a described script block, then a normal pwsh tab:

```
marks: PromptStart PromptEnd  PromptStart PromptEnd  PromptStart PromptEnd
```

and `US-0136-verify5-pwsh-described-enter.png` shows `cmd /c exit 3` and `cmd /c exit 0`
both submitted and both followed by a fresh prompt. `A`/`B` only, no `C`, no `D`, no error
on screen. The implementer's own `US-0136-verify4-pwsh-described-enter.png` shows the same
thing with `Write-Output oneterm-accepted` printing its output — both frames support the
claim.

`D` is absent as well as `C`, because `$global:__OneTermRan` is only ever set inside the
handler. Section 6.1.2 says "that tab reports `A`/`B` only", which is exactly right; the
wording from the round before it ("`A`/`B`/`D` only") would not have been.

---

## The `AmbiguousMatchException` gap — safe, twice over

`[Type].GetMethod(name)` throws when the name is overloaded. Enumerated on this host, the
same seven in both PowerShell versions:

```
GetBufferState, GetKeyHandlers, Insert, ReadLine, SetKeyHandler,
ViDeleteToBeforeChar, ViDeleteToChar
```

**Through `-Function` the gap is unreachable.** Five of the seven are not PSReadLine
function names at all. The two that are — `ViDeleteToChar` and `ViDeleteToBeforeChar` —
cannot be bound, because PSReadLine resolves them by the same reflection and refuses first:

```
Set-PSReadLineKeyHandler -Key Enter -Function ViDeleteToChar
  -> Ambiguous match found for 'Microsoft.PowerShell.PSConsoleReadLine
     Void ViDeleteToChar(System.Nullable`1[System.ConsoleKeyInfo], System.Object)'.
```

**The one route that remains is a described script block whose description happens to name
an overloaded static** — `-BriefDescription 'Insert'`. Measured end to end in both hosts
(the last row of the table above): the exception is raised by the `if` condition, which is
the **last statement of the init**, so everything before it has already run. The prompt
wrapper is installed and works (`prompt` returns `user> ESC]133;B ESC\`), the user's `Enter`
is untouched, `$global:__OneTermEnter` stays empty, and PowerShell continues — an uncaught
run of the init prints one error record and then reaches the next statement:

```
MethodInvocationException: Exception calling "GetMethod" with "1" argument(s):
  "Ambiguous match found for 'Microsoft.PowerShell.PSConsoleReadLine Void Insert(Char)'."
STILL-RUNNING
```

So the failure mode is the documented decline — `A`/`B`, no `C`, the user's `Enter` kept —
plus one error blob printed once at startup. **Safe**, as recorded. The blob is the only
cost and it needs a profile that names its `Enter` block after one of seven methods; a
`try{...}catch{}` around the guard would remove even that, if anyone ever sees it.

---

## NEW-MIN-1 — fixed

The mark is now the value of `$'\001\033]133;B\007\002'` (local,
`crates/core/src/config/shell.rs:215`) and of `$(printf '\001\033]133;B\007\002')` (SSH,
`crates/ssh/src/session.rs:746`) — `\001` and `\002` are `RL_PROMPT_START_IGNORE` and
`RL_PROMPT_END_IGNORE`, which is what `\[` and `\]` expand to, so the region is marked
without writing a backslash anywhere.

```
mark bytes : 001 033 ] 1 3 3 ; B \a 002      len 10      0x5c count 0
```

**Five `PS1` shapes, variable and `${PS1@P}`, measured in bash 5.3.15:**

| user `PS1` | expanded | SOH / STX | `0x5c` in the whole prompt |
| --- | --- | :-: | :-: |
| `x` | `x 001 033 ]133;B \a 002` | 1 / 1 | 0 |
| `x\` | `x \ 001 033 ]133;B \a 002` | 1 / 1 | 1 (the user's own) |
| `x\\` | `x \ 001 033 ]133;B \a 002` | 1 / 1 | 1 (the user's own) |
| `\u@\h:\w\$ ` | `trunglt@...$ 001 033 ]133;B \a 002` | 1 / 1 | 0 |
| `\[\e[32m\]\u\[\e[0m\]\$ ` | `033[32m trunglt 033[0m $ 001 033 ]133;B \a 002` | 1 / 1 | 0 |

`x\` is NEW-MIN-1's case: the user's backslash arrives intact and the mark still opens and
closes its region exactly once. Idempotence holds — five expansions leave one SOH, one STX,
`PS1` length 11 — and readline strips both control bytes before writing, so **none reach the
terminal** (measured: zero `001`/`002` on the wire from a real interactive bash).

A real interactive bash under the generated environment still gives
`A B C D;7 A B C D;0 A B C`, with `033 ] 1 3 3 ; B \a` running straight into the echoed
command.

**SSH bootstrap.** `dash -n` and `bash -n` both exit 0; 880 bytes; `printf` rather than
`$'…'` because a POSIX `sh` must parse the line. `PS1` expands to
`u$ 001 033 ]133;B \a 002` (SOH 1, STX 1), `PS0` still to `033 ]133;C 033 \`, and MED-1 still
holds: prompt 1 carries no `D`, prompt 2 after `(exit 7)` carries `D;7`, `$?` comes back as
5. `dash` still reaches neither branch and emits one OSC 7 plus `133;A`.

**Test extensions present.** `bash_ps1_mark_survives_prompt_expansion`
(`crates/core/src/config/shell.rs:692-721`) now decodes the shell literal with a small
`ansi_c_bytes` helper and asserts the exact four-byte shape, one `\001`, one `\002`, and
`!mark.contains(&b'\\')` — the invariant, not the instance — then concatenates the mark
after six `PS1` tails including `x\` and re-counts. `crates/ssh` asserts its own copy and
that neither earlier form survives anywhere.

---

## Live mark logs — unchanged

```
kind=cmd         A B  A B  A B
kind=pwsh        A B C D;3  A B C D;0  A B
kind=powershell  A B C D;3  A B C D;0  A B
kind=bash        A B C D;7  A B C D;0  A B
```

Frame `US-0136-verify5-bash-tab.png`: the prompt still reads `$ (exit 7)`, no stray
character.

## Commands

```powershell
git reset --hard 9e83e07d
cargo build -p oneterm-app --profile fast-dev
pwsh walk.ps1 -Kind cmd|pwsh|powershell 'cmd /c exit 3' 'cmd /c exit 0'
pwsh walk.ps1 -Kind bash '(exit 7)' 'true'
pwsh walk-profile.ps1 -Kind pwsh 'cmd /c exit 3' 'cmd /c exit 0'   # seeds a described-Enter $PROFILE
cargo test -p oneterm-core powershell_enter_guard_decides_the_four_bindings -- --nocapture
cargo test -p oneterm-core -p oneterm-ssh -p oneterm-local-shell -p oneterm-terminal
pwsh scripts/ci-local.ps1
```

```bash
bash --norc --noprofile f1.sh   # five PS1 shapes, the mark's bytes, idempotence, the wire
bash --norc --noprofile f2.sh   # the bootstrap: parse, PS1/PS0, MED-1, dash
pwsh / powershell -NoProfile -File ps-amb.ps1 , ps-amb2.ps1   # overloaded names; five bindings
```

## Tests and gate

`cargo test -p oneterm-core -p oneterm-ssh -p oneterm-local-shell -p oneterm-terminal` —
green: 89, 102, 34 with 2 ignored, and 218.

`pwsh scripts/ci-local.ps1` — every step green, ending:

```
ci-local: all checks passed.
```

## Gaps unchanged

zsh remains unproven, local and remote. No SSH server was reachable, so the bootstrap is
proved by running its own text in a real bash rather than through a `russh` channel.
`cfg(unix)` is unverifiable here. PSReadLine's behaviour when a key handler throws was not
observed in a real console host — it did not need to be, because no handler that can throw
is now installed.
