//! Grid-reading helpers for completion: extract the command line under the
//! cursor, strip the shell prompt, and compute the visible scroll window.

use oneterm_terminal::TerminalContent;

/// Compute the scroll window `(offset, count)` into a suggestion list of length
/// `n`, showing at most `max_visible` rows and keeping `selected` (if any) in
/// view. When nothing is selected the window starts at the top.
pub(crate) fn visible_window(
    n: usize,
    selected: Option<usize>,
    max_visible: usize,
) -> (usize, usize) {
    let max_v = max_visible.max(1);
    if n <= max_v {
        return (0, n);
    }
    let offset = match selected {
        None => 0,
        Some(i) if i < max_v => 0,
        // Keep the selected row at the bottom edge of the window as it scrolls.
        Some(i) => (i + 1 - max_v).min(n - max_v),
    };
    (offset, max_v)
}

/// Extract the command-input text on the cursor's row (up to the cursor column),
/// stripped of the shell prompt prefix. Returns `(command, prompt_found,
/// (cursor_line, cursor_col))`.
pub(crate) fn extract_cursor_command(content: &TerminalContent) -> (String, bool, (i32, usize)) {
    let cur = content.cursor.point;
    let cursor_line = cur.line.0;
    let cursor_col = cur.column.0;

    let mut row: Vec<char> = Vec::new();
    for ic in &content.cells {
        if ic.point.line.0 != cursor_line {
            continue;
        }
        let c = ic.point.column.0;
        if c >= cursor_col {
            continue;
        }
        while row.len() <= c {
            row.push(' ');
        }
        row[c] = ic.cell.c;
    }
    let row_str: String = row.into_iter().collect();
    let (command, found, strip_cols) = strip_prompt(&row_str);
    // The cursor column relative to the grid stays `cursor_col`; the token-start
    // anchor is computed by the caller from `typed_len`.
    let _ = strip_cols;
    (command, found, (cursor_line, cursor_col))
}

/// Strip a shell prompt prefix from a row. Returns `(command_after_prompt,
/// found, prompt_end_col)`. Best-effort: matches the first `>` (cmd/PowerShell)
/// or `$`/`#`/`❯`/`➜`/`λ` sign followed by a space (POSIX), skipping trailing
/// prompt spaces.
fn strip_prompt(row: &str) -> (String, bool, usize) {
    let chars: Vec<char> = row.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        let is_sign = ch == '>'
            || (matches!(ch, '$' | '#' | '❯' | '➜' | 'λ')
                && chars.get(i + 1).map_or(true, |n| *n == ' '));
        if is_sign {
            let mut j = i + 1;
            while chars.get(j) == Some(&' ') {
                j += 1;
            }
            let command: String = chars[j..].iter().collect();
            return (command, true, j);
        }
    }
    (row.trim_start().to_string(), false, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_cmd_prompt() {
        let (cmd, found, _) = strip_prompt(r"C:\Users\Trung>d");
        assert!(found);
        assert_eq!(cmd, "d");
    }

    #[test]
    fn strip_unix_prompt() {
        let (cmd, found, _) = strip_prompt("trung@pc:~/proj$ git c");
        assert!(found);
        assert_eq!(cmd, "git c");
    }

    #[test]
    fn no_prompt_falls_back_to_row() {
        let (cmd, found, _) = strip_prompt("just some text");
        assert!(!found);
        assert_eq!(cmd, "just some text");
    }

    #[test]
    fn powershell_prompt() {
        let (cmd, found, _) = strip_prompt(r"PS C:\Users\Trung> Get-Ch");
        assert!(found);
        assert_eq!(cmd, "Get-Ch");
    }

    #[test]
    fn window_shows_all_when_short() {
        assert_eq!(visible_window(5, None, 8), (0, 5));
        assert_eq!(visible_window(5, Some(4), 8), (0, 5));
    }

    #[test]
    fn window_caps_and_starts_at_top_without_selection() {
        assert_eq!(visible_window(32, None, 8), (0, 8));
    }

    #[test]
    fn window_scrolls_to_keep_selection_visible() {
        // Selection within the first window → no scroll.
        assert_eq!(visible_window(32, Some(7), 8), (0, 8));
        // Selecting row 8 scrolls down by one so row 8 sits at the bottom edge.
        assert_eq!(visible_window(32, Some(8), 8), (1, 8));
        // Last row → window clamped to the end.
        assert_eq!(visible_window(32, Some(31), 8), (24, 8));
    }
}
