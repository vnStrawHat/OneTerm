//! Default environment variables for local shell sessions.

use std::collections::HashMap;

#[cfg(any(windows, test))]
const WSLENV_HINTS: [&str; 2] = ["COLORTERM", "TERM_PROGRAM"];

#[cfg(any(windows, test))]
fn wslenv_with_terminal_hints(existing: Option<&str>) -> String {
    let existing = existing.unwrap_or("");
    if existing.is_empty() {
        return WSLENV_HINTS
            .iter()
            .map(|hint| format!("{hint}/u"))
            .collect::<Vec<_>>()
            .join(":");
    }

    let mut seen = [false; 2];
    let entries = existing
        .split(':')
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (name, flags) = entry.split_once('/').unwrap_or((entry, ""));
            for (idx, hint) in WSLENV_HINTS.iter().enumerate() {
                if name.eq_ignore_ascii_case(hint) {
                    seen[idx] = true;
                    if flags.contains('u') {
                        return entry.to_string();
                    }
                    return if flags.is_empty() {
                        format!("{name}/u")
                    } else {
                        format!("{name}/{flags}u")
                    };
                }
            }
            entry.to_string()
        })
        .collect::<Vec<_>>();

    let mut result = entries.join(":");
    for (idx, hint) in WSLENV_HINTS.iter().enumerate() {
        if !seen[idx] {
            if !result.is_empty() {
                result.push(':');
            }
            result.push_str(&format!("{hint}/u"));
        }
    }
    result
}

/// Default env for every shell.
pub(super) fn base_env() -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert("TERM".into(), "xterm-256color".into());
    env.insert("COLORTERM".into(), "truecolor".into());
    env.insert("TERM_PROGRAM".into(), "OneTerm".into());
    #[cfg(windows)]
    {
        let wslenv = wslenv_with_terminal_hints(std::env::var("WSLENV").ok().as_deref());
        env.insert("WSLENV".into(), wslenv);
    }
    // LANG: prefer the current env, fall back to en_US.UTF-8.
    let lang = std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into());
    env.insert("LANG".into(), lang);
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_env_advertises_oneterm_truecolor() {
        let env = base_env();
        assert_eq!(env.get("TERM").map(String::as_str), Some("xterm-256color"));
        assert_eq!(env.get("COLORTERM").map(String::as_str), Some("truecolor"));
        assert_eq!(env.get("TERM_PROGRAM").map(String::as_str), Some("OneTerm"));
    }

    #[test]
    fn wslenv_with_terminal_hints_preserves_existing_entries() {
        assert_eq!(
            wslenv_with_terminal_hints(None),
            "COLORTERM/u:TERM_PROGRAM/u"
        );
        assert_eq!(
            wslenv_with_terminal_hints(Some("PATH/l:USERPROFILE/p")),
            "PATH/l:USERPROFILE/p:COLORTERM/u:TERM_PROGRAM/u"
        );
        assert_eq!(
            wslenv_with_terminal_hints(Some("PATH/l:COLORTERM/w:TERM_PROGRAM/w")),
            "PATH/l:COLORTERM/wu:TERM_PROGRAM/wu"
        );
        assert_eq!(
            wslenv_with_terminal_hints(Some("PATH/l:COLORTERM/u:TERM_PROGRAM/u")),
            "PATH/l:COLORTERM/u:TERM_PROGRAM/u"
        );
    }
}
