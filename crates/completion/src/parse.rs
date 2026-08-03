//! Input-line parsing (docs/auto-completion/04 §2).
//!
//! Turns `(line, cursor_col)` into a [`ParsedLine`]: the command `head`, the
//! `token` under the cursor, its byte offset, and the tokens to the left of the
//! cursor (used to walk the subcommand tree — docs 10 §3). Tokenization is
//! quote-aware so `"C:\Program Files"` is a single token.

/// A single parsed token: its unquoted text and its byte offset in the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    pub start: usize,
}

/// Tokenize a string, honoring single/double quotes. Quote characters are
/// stripped from the token text; `start` is the byte offset of the token's first
/// character (the opening quote, if quoted) in `input`.
pub fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut start = 0usize;
    let mut in_token = false;
    let mut quote: Option<char> = None;

    for (i, ch) in input.char_indices() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else {
                    cur.push(ch);
                }
            }
            None => {
                if ch == '"' || ch == '\'' {
                    if !in_token {
                        in_token = true;
                        start = i;
                    }
                    quote = Some(ch);
                } else if ch.is_whitespace() {
                    if in_token {
                        tokens.push(Token {
                            text: std::mem::take(&mut cur),
                            start,
                        });
                        in_token = false;
                    }
                } else {
                    if !in_token {
                        in_token = true;
                        start = i;
                    }
                    cur.push(ch);
                }
            }
        }
    }
    if in_token {
        tokens.push(Token { text: cur, start });
    }
    tokens
}

/// The parsed prompt line, relative to the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLine {
    /// The command name (first token), if any.
    pub head: Option<String>,
    /// The token currently under the cursor (may be empty on a trailing space).
    pub token: String,
    /// Byte offset where `token` begins in the line.
    pub token_start: usize,
    /// Whether the cursor is editing the command name itself.
    pub is_first_token: bool,
    /// Tokens strictly to the left of the cursor token (includes `head`).
    pub prior_tokens: Vec<String>,
}

impl ParsedLine {
    /// Parse `line` given the cursor byte offset `cursor_col`.
    pub fn parse(line: &str, cursor_col: usize) -> Self {
        let cursor_col = cursor_col.min(line.len());
        let prefix = &line[..cursor_col];
        let tokens = tokenize(prefix);

        // A trailing space (or empty prefix) means the cursor starts a new,
        // empty token.
        let trailing_ws = prefix.is_empty() || prefix.ends_with(char::is_whitespace);

        if trailing_ws {
            let prior_tokens: Vec<String> = tokens.iter().map(|t| t.text.clone()).collect();
            ParsedLine {
                head: prior_tokens.first().cloned(),
                token: String::new(),
                token_start: cursor_col,
                is_first_token: prior_tokens.is_empty(),
                prior_tokens,
            }
        } else {
            let last = tokens.last().cloned().unwrap_or(Token {
                text: String::new(),
                start: cursor_col,
            });
            let is_first_token = tokens.len() <= 1;
            let prior_tokens: Vec<String> = tokens[..tokens.len().saturating_sub(1)]
                .iter()
                .map(|t| t.text.clone())
                .collect();
            ParsedLine {
                head: prior_tokens.first().cloned(),
                token: last.text,
                token_start: last.start,
                is_first_token,
                prior_tokens,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_simple() {
        let t = tokenize("dir /Q");
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].text, "dir");
        assert_eq!(t[0].start, 0);
        assert_eq!(t[1].text, "/Q");
        assert_eq!(t[1].start, 4);
    }

    #[test]
    fn tokenize_respects_quotes() {
        let t = tokenize(r#"cd "C:\Program Files""#);
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].text, r"C:\Program Files");
        assert_eq!(t[1].start, 3);
    }

    #[test]
    fn tokenize_single_quotes() {
        let t = tokenize("echo 'a b c'");
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].text, "a b c");
    }

    #[test]
    fn command_context_first_token() {
        let p = ParsedLine::parse("di", 2);
        assert_eq!(p.token, "di");
        assert_eq!(p.token_start, 0);
        assert!(p.is_first_token);
        assert_eq!(p.head, None);
        assert!(p.prior_tokens.is_empty());
    }

    #[test]
    fn trailing_space_after_command_is_subcommand_context() {
        let p = ParsedLine::parse("git ", 4);
        assert_eq!(p.token, "");
        assert_eq!(p.token_start, 4);
        assert!(!p.is_first_token);
        assert_eq!(p.head.as_deref(), Some("git"));
        assert_eq!(p.prior_tokens, vec!["git"]);
    }

    #[test]
    fn option_context_head_and_token() {
        let p = ParsedLine::parse("dir /", 5);
        assert_eq!(p.token, "/");
        assert_eq!(p.head.as_deref(), Some("dir"));
        assert!(!p.is_first_token);
        assert_eq!(p.prior_tokens, vec!["dir"]);
    }

    #[test]
    fn nested_prior_tokens() {
        let p = ParsedLine::parse("git remote ", 11);
        assert_eq!(p.token, "");
        assert_eq!(p.prior_tokens, vec!["git", "remote"]);
        assert_eq!(p.head.as_deref(), Some("git"));
    }

    #[test]
    fn cursor_mid_line_ignores_text_after_cursor() {
        // Cursor after "com" in "git commit"; only left side is parsed.
        let p = ParsedLine::parse("git commit", 7);
        assert_eq!(p.token, "com");
        assert_eq!(p.prior_tokens, vec!["git"]);
    }
}
