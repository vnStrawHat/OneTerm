//! Per-shell profile — only the prompt regex and path/option syntax differ.
//!
//! Everything else (keyword sets, structural regexes, probes) is shared in
//! [`RuleSet`](crate::RuleSet). One `ShellProfile` per view, not 6 duplicate
//! grammars. See §6 of the design doc.

use std::sync::LazyLock;

use regex::Regex;

/// Path separator syntax.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathSep {
    /// Unix — `/` only.
    Unix,
    /// Windows — `\` or `/`.
    Windows,
}

/// Shell profile — the two things that actually differ per shell.
///
/// Selected from session settings (shell kind). Unknown → `Dumb` (most
/// permissive prompt regex). See §6.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ShellProfile {
    /// bash/sh/zsh/fish — Linux/macOS/WSL local + SSH on Linux.
    #[default]
    Unix,
    /// cmd.exe — path `\`/`/`, option `-`/`/`.
    Cmd,
    /// pwsh — path `\`/`/`, option `-`.
    PowerShell,
    /// Unknown / serial / router — most permissive prompt regex.
    Dumb,
}

impl ShellProfile {
    /// The path separator for this shell.
    pub(crate) fn path_sep(&self) -> PathSep {
        match self {
            Self::Unix => PathSep::Unix,
            Self::Cmd | Self::PowerShell | Self::Dumb => PathSep::Windows,
        }
    }

    /// Whether a char is an option prefix (`--flag`, `-x`, `/flag`).
    pub fn is_option_prefix(&self, c: char) -> bool {
        match self {
            Self::Unix | Self::PowerShell => c == '-',
            Self::Cmd => c == '-' || c == '/',
            // Dumb: accept both — most permissive.
            Self::Dumb => c == '-' || c == '/',
        }
    }

    /// The fallback prompt detector regex (used only without OSC 133).
    ///
    /// Matches a line that starts with (optional non-glyph prefix) a prompt
    /// sign glyph followed by a space. The scanner uses this to decide whether
    /// to start in `PromptLine` state.
    pub fn prompt_regex(&self) -> &Regex {
        match self {
            Self::Unix => &PROMPT_UNIX,
            Self::Cmd => &PROMPT_CMD,
            Self::PowerShell => &PROMPT_PWSH,
            Self::Dumb => &PROMPT_DUMB,
        }
    }

    /// Whether a char is a recognized prompt-sign glyph.
    pub fn is_prompt_sign(&self, c: char) -> bool {
        matches!(
            c,
            '$' | '#' | '%' | '>' | '<' | '❯' | '➜' | 'λ' | '→' | '»'
            // Powerline separators (Private Use Area).
            | '\u{E0B0}' | '\u{E0B1}' | '\u{E0B2}' | '\u{E0B3}'
        )
    }
}

// ── Compiled prompt regexes (DFA, ReDoS-safe) ──────────────────────────────

/// Unix prompt: an optional plausible prefix (`user@host:~`, `[user@host ~]`,
/// `~/src`, `/srv`, with an optional `(venv) ` in front) then `$`/`#`/`%`
/// followed by a space or the end of the line. The prefix must be empty or
/// contain one of `@ : ~ / ]`, and the sign must not be glued to text, so
/// `100%`, `#include <stdio.h>` and `$HOME=/root` are not prompts (CORR-48).
static PROMPT_UNIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(UNIX_PROMPT_PATTERN).expect("Unix prompt regex is valid"));

/// Shared Unix prompt pattern (also the universal fallback in the scanner).
pub(crate) const UNIX_PROMPT_PATTERN: &str =
    r"^(?:\([^)\s]*\) )?(?:\[[^\]]*\]|[^\s]*[@:~/\]][^\s]*)?[\$#%](?: |$)";

/// cmd.exe prompt: `C:\path>` or `>`. The trailing space is optional so the
/// prompt is detected even when the user has typed right after `>` (the blank
/// cell after `>` is replaced by the typed char, removing the trailing space).
static PROMPT_CMD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[A-Za-z]:[^\s>]*>[ ]?)|(?:^>[ ]?)").expect("cmd prompt regex is valid")
});

/// PowerShell prompt: `PS C:\path>` or `>>`.
static PROMPT_PWSH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:PS[^\s>]*>[ ]?)|(?:^>+[ ]?)").expect("PowerShell prompt regex is valid")
});

/// Dumb / serial / router — most permissive prefix (`Router#`, `Router>`),
/// but the sign must still be followed by a space or the end of the line.
static PROMPT_DUMB: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[^\s]*[\$#%>»](?: |$)").expect("dumb prompt regex is valid"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_prompt_matches() {
        for line in [
            "user@host:~$ ",
            "user@host:~$ ls",
            "[user@host ~]$ ",
            "# ",
            "$ echo hi",
            "~/src$ ",
            "(venv) user@host:~/p$ ",
            "root@box:/srv# ",
            "user@host:~%",
        ] {
            assert!(PROMPT_UNIX.is_match(line), "{line:?} must be a prompt");
        }
    }

    /// CORR-48: a sign glued to text, or a bare word before the sign, is output.
    #[test]
    fn unix_prompt_no_match_on_output() {
        for line in [
            "hello world",
            "error: something failed",
            "100% done",
            "#include <stdio.h>",
            "$HOME=/root",
            "total 100%",
            "50%",
        ] {
            assert!(!PROMPT_UNIX.is_match(line), "{line:?} must not be a prompt");
        }
    }

    #[test]
    fn dumb_prompt_requires_space_or_eol_after_sign() {
        assert!(PROMPT_DUMB.is_match("Router#"));
        assert!(PROMPT_DUMB.is_match("Router> show ip"));
        assert!(!PROMPT_DUMB.is_match("#include <stdio.h>"));
        assert!(!PROMPT_DUMB.is_match("$HOME=/root"));
    }

    #[test]
    fn cmd_prompt_matches() {
        assert!(PROMPT_CMD.is_match("C:\\Users\\me> "));
        assert!(PROMPT_CMD.is_match("> dir"));
    }

    #[test]
    fn prompt_sign_chars() {
        let p = ShellProfile::Unix;
        assert!(p.is_prompt_sign('$'));
        assert!(p.is_prompt_sign('#'));
        assert!(p.is_prompt_sign('❯'));
        assert!(!p.is_prompt_sign('a'));
    }

    #[test]
    fn option_prefix() {
        assert!(ShellProfile::Unix.is_option_prefix('-'));
        assert!(!ShellProfile::Unix.is_option_prefix('/'));
        assert!(ShellProfile::Cmd.is_option_prefix('/'));
        assert!(ShellProfile::Cmd.is_option_prefix('-'));
    }
}
