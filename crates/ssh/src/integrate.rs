//! Shell integration of a remote shell (specs/oneterm, "M7 in detail"): one line typed into the
//! shell once it is open, so that it says where it is (OSC 7) and where its prompts, commands
//! and outputs begin and end (OSC 133), as a local shell does. The PTY is asked for with echo
//! off, so the line is not seen; its last word turns echo on again.

use russh::Pty;

/// The PTY's modes while the line is typed: no echo.
pub(crate) const QUIET: &[(Pty, u32)] = &[(Pty::ECHO, 0)];

/// The line, OneTerm 0.6's. It must read in any POSIX `sh`, which reads a whole line before
/// it runs any of it: zsh's arrays are in an `eval`, and bash's `B` is made by `printf`, not
/// `$'..'`. bash: `PROMPT_COMMAND` (`D` with the status, OSC 7, `A`), `PS0` (`C`), `B` at the
/// end of `PS1` as raw bytes between readline's marks (`\001`, `\002`), nothing in it for
/// prompt expansion to join with the user's `PS1`. zsh: `precmd_functions` and
/// `preexec_functions`, `B` in `%{..%}`. The user's own hooks stay, after OneTerm's, and see
/// their command's status. Another shell (dash, ash) says its directory once. The call at the
/// end says the first prompt's and forgets it was said, so that the first prompt the user
/// sees tells no `D` of a command never run. `COLORTERM` is set here too: an sshd gives a
/// shell only the variables it is told to accept. The space before it keeps the line out of
/// a history that leaves such lines out (`HISTCONTROL=ignorespace`, zsh's `HIST_IGNORE_SPACE`).
pub(crate) const LINE: &str = r#" export COLORTERM=truecolor; __oneterm_mark() { printf '\033]133;%s\033\\' "$1"; }; __oneterm_precmd() { __oneterm_status=$?; if [ -n "${__oneterm_seen-}" ]; then __oneterm_mark "D;$__oneterm_status"; fi; __oneterm_seen=1; printf '\033]7;file://%s%s\033\\' "${HOSTNAME:-$(hostname)}" "$PWD"; __oneterm_mark A; return $__oneterm_status; }; __oneterm_preexec() { __oneterm_mark C; }; if [ -n "${ZSH_VERSION-}" ]; then eval 'precmd_functions=(__oneterm_precmd $precmd_functions); preexec_functions=($preexec_functions __oneterm_preexec)'; PS1="$PS1%{$(__oneterm_mark B)%}"; elif [ -n "${BASH_VERSION-}" ]; then case ";${PROMPT_COMMAND:-};" in *";__oneterm_precmd;"*) ;; *) PROMPT_COMMAND="__oneterm_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;; esac; PS0="$(__oneterm_mark C)"; PS1="$PS1$(printf '\001\033]133;B\007\002')"; fi; __oneterm_precmd; __oneterm_seen=; stty echo 2>/dev/null"#;
