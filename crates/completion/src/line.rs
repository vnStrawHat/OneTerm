//! The line left of the cursor, as tokens (specs/completion, "A suggestion").

/// The line as the engine reads it at the cursor.
#[derive(Debug, PartialEq)]
pub(crate) struct Parsed {
    /// The token under the cursor, its quotes taken off; empty after a blank.
    pub token: String,
    /// Where the token starts in the line, in bytes.
    pub token_start: usize,
    /// The tokens before it; the first is the command.
    pub before: Vec<String>,
}

impl Parsed {
    /// Reads `line[..cursor]`; `cursor` is a byte offset at a character's start.
    pub(crate) fn new(line: &str, cursor: usize) -> Parsed {
        let left = &line[..cursor];
        let mut tokens = tokenize(left);
        let blank_end = left.is_empty() || left.ends_with(char::is_whitespace);
        let (token, token_start) = match tokens.pop() {
            Some(last) if !blank_end => last,
            Some(last) => {
                tokens.push(last);
                (String::new(), cursor)
            }
            None => (String::new(), cursor),
        };
        Parsed { token, token_start, before: tokens.into_iter().map(|(text, _)| text).collect() }
    }

    /// The command, when the cursor is past it.
    pub(crate) fn head(&self) -> Option<&str> {
        self.before.first().map(String::as_str)
    }
}

/// Splits on blanks outside quotes; each token with where it starts (its opening quote).
fn tokenize(input: &str) -> Vec<(String, usize)> {
    let mut tokens = Vec::new();
    let mut current: Option<(String, usize)> = None;
    let mut quote = None;
    for (at, ch) in input.char_indices() {
        if let Some(open) = quote {
            if ch == open {
                quote = None;
            } else if let Some((text, _)) = current.as_mut() {
                text.push(ch);
            }
            continue;
        }
        if ch.is_whitespace() {
            tokens.extend(current.take());
            continue;
        }
        let (text, _) = current.get_or_insert_with(|| (String::new(), at));
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else {
            text.push(ch);
        }
    }
    tokens.extend(current);
    tokens
}

/// The largest character start at or before `at`, within the line.
pub(crate) fn floor(line: &str, at: usize) -> usize {
    let mut at = at.min(line.len());
    while !line.is_char_boundary(at) {
        at -= 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_hold_a_token_together() {
        assert_eq!(tokenize("dir /Q"), [("dir".into(), 0), ("/Q".into(), 4)]);
        assert_eq!(tokenize(r#"cd "C:\Program Files""#)[1], (r"C:\Program Files".into(), 3));
        assert_eq!(tokenize("echo 'a b c'")[1].0, "a b c");
    }

    #[test]
    fn the_token_at_the_cursor() {
        let first = Parsed::new("di", 2);
        assert_eq!((first.token.as_str(), first.token_start, first.head()), ("di", 0, None));
        let after_blank = Parsed::new("git ", 4);
        assert_eq!((after_blank.token.as_str(), after_blank.token_start), ("", 4));
        assert_eq!(after_blank.before, ["git"]);
        let option = Parsed::new("dir /", 5);
        assert_eq!((option.token.as_str(), option.head()), ("/", Some("dir")));
        assert_eq!(Parsed::new("git remote ", 11).before, ["git", "remote"]);
    }

    #[test]
    fn what_is_right_of_the_cursor_is_not_read() {
        let parsed = Parsed::new("git commit", 7);
        assert_eq!((parsed.token.as_str(), parsed.before.as_slice()), ("com", &["git".to_string()][..]));
    }

    #[test]
    fn floor_finds_a_character_start() {
        assert_eq!(floor("日x", 1), 0);
        assert_eq!(floor("日x", 3), 3);
        assert_eq!(floor("ab", 9), 2);
    }
}
