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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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

/// The path body of a Windows prompt, between the drive (or `PS`) and the `>`.
///
/// It **may contain spaces**: `C:\Users\John Doe\Documents>` is the ordinary
/// shape of a Windows profile directory, and `PS C:\path>` puts a space right
/// after the `PS` — the old `[^\s>]*` matched neither, so a cwd with a space was
/// never a prompt and a PowerShell prompt was never detected at all
/// (`BUG-0071`). It excludes `< > | " * ?`, which cannot appear in a Windows
/// path, and its last character must not be a space, so an output line like
/// `C:\log size > 3` is not read as a prompt. Because `>` is excluded, the match
/// ends at the **first** `>`, which is the prompt sign: a redirection later on
/// the line (`dir > out.txt`) stays outside the prompt region.
///
/// Two costs of that trade, both bounded and cosmetic (`BUG-0071` N2/N3): a
/// drive-anchored line whose first `>` is not preceded by a space is read as a
/// prompt (`C:\src -> C:\dst`), and a cwd that legally ends in a space
/// (`C:\trailing >`) is not.
const WIN_PATH_BODY: &str = r#"[^<>|"*?\r\n]*[^\s<>|"*?]"#;

/// cmd.exe prompt: `C:\path>`, a UNC path `\\server\share>`, or a bare `>`. The
/// trailing space is optional so the prompt is detected even when the user has
/// typed right after `>` (the blank cell after `>` is replaced by the typed
/// char, removing the trailing space).
pub(crate) static PROMPT_CMD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!("{}|(?:^>[ ]?)", win_path_prompt_pattern()))
        .expect("cmd prompt regex is valid")
});

/// The drive- or UNC-anchored half of the Windows prompt patterns, shared with
/// the scanner's universal fallback.
///
/// It deliberately leaves out the bare `>` / `>>` continuation branch, which
/// belongs to the Windows profiles alone. A line starting with `> ` is `cmd`'s
/// continuation prompt, but it is also a mail quote, a markdown blockquote, a
/// `git log` body and diff context — and those arrive on a Unix or SSH tab,
/// where they are output. Sharing one pattern string briefly made every profile
/// read them as prompts (`BUG-0071` N1).
pub(crate) fn win_path_prompt_pattern() -> String {
    format!(r"^(?:(?:[A-Za-z]:|\\\\){WIN_PATH_BODY}>[ ]?)")
}

/// PowerShell prompt: `PS C:\path>`, the bare `PS>`, or the `>>` continuation.
static PROMPT_PWSH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"^(?:PS(?: {WIN_PATH_BODY})?>[ ]?)|(?:^>+[ ]?)"))
        .expect("PowerShell prompt regex is valid")
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

    /// `BUG-0071` F2: a Windows cwd may contain spaces, and the match must stop
    /// at the prompt's own `>` so a later redirection is not swallowed.
    #[test]
    fn cmd_prompt_accepts_spaces_unc_and_stops_at_the_sign() {
        for (line, want) in [
            (
                r"C:\Users\John Doe\Documents>echo hi",
                Some(r"C:\Users\John Doe\Documents>"),
            ),
            (
                r"C:\Users\John Doe\oneterm workspace\deep> ",
                Some(r"C:\Users\John Doe\oneterm workspace\deep> "),
            ),
            (r"C:\>", Some(r"C:\>")),
            (
                r"\\server\share\My Files>dir",
                Some(r"\\server\share\My Files>"),
            ),
            // The prompt ends at its own sign; `dir > out.txt` is not part of it.
            (r"C:\work>dir > out.txt", Some(r"C:\work>")),
            (r"> dir", Some("> ")),
            // Output, not a prompt: a space right before the sign, and a line
            // that does not start at a drive or a UNC root.
            (r"C:\log size > 3", None),
            ("total 100%", None),
            ("see C:\\x> not a prompt", None),
        ] {
            let got = PROMPT_CMD.find(line).map(|m| m.as_str());
            assert_eq!(got, want, "{line:?}");
        }
    }

    /// `BUG-0071` F1: `PS C:\path>` has a space after `PS`, so the old
    /// `[^\s>]*` never matched a PowerShell prompt at all.
    #[test]
    fn powershell_prompt_matches() {
        for (line, want) in [
            (r"PS C:\Users\me> ", Some(r"PS C:\Users\me> ")),
            (
                r"PS C:\Users\John Doe\Documents> echo hi",
                Some(r"PS C:\Users\John Doe\Documents> "),
            ),
            (
                r"PS \\server\share\My Files> dir",
                Some(r"PS \\server\share\My Files> "),
            ),
            (r"PS>", Some("PS>")),
            (r">> ", Some(">> ")),
            (r"PS C:\work> dir > out.txt", Some(r"PS C:\work> ")),
            // Output.
            ("PS is > 3", None),
            ("PSReadLine loaded", None),
            ("hello world", None),
        ] {
            let got = PROMPT_PWSH.find(line).map(|m| m.as_str());
            assert_eq!(got, want, "{line:?}");
        }
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
