//! Per-line and multi-line URL masks for always-on highlighting.

use oneterm_core::terminal::IndexedCell;

use super::{PREFIXES, is_trailing_punct};

/// Build a per-column mask marking which columns are part of a URL.
///
/// Detects both OSC 8 hyperlinks (`cell.hyperlink()`) and plain-text URLs
/// (`http://`, `https://`, `ftp://`, `www.`). Used for always-on URL
/// highlighting — every URL in the terminal is underlined regardless of
/// hover state.
///
/// Returns `is_url[col]` = true if column `col` is part of a URL.
pub fn url_column_mask(line_cells: &[&IndexedCell]) -> Vec<bool> {
    use alacritty_terminal::term::cell::Flags;

    // Determine the grid width from the highest column index.
    let max_col = line_cells
        .iter()
        .filter(|ic| !ic.cell.flags.contains(Flags::WIDE_CHAR_SPACER))
        .map(|ic| ic.point.column.0 as usize)
        .max()
        .unwrap_or(0);
    let n = max_col + 1;

    let mut chars: Vec<char> = vec![' '; n];
    let mut is_url: Vec<bool> = vec![false; n];

    for ic in line_cells {
        if ic.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let col = ic.point.column.0 as usize;
        if col >= n {
            continue;
        }
        chars[col] = match ic.cell.c {
            '\0' | '\t' => ' ',
            c => c,
        };
        // OSC 8 hyperlink
        if ic.cell.hyperlink().is_some() {
            is_url[col] = true;
        }
    }

    // Plain-text URLs: scan for prefixes.
    let mut i = 0;
    while i < n {
        // Skip columns already marked (OSC 8 hyperlink).
        if is_url[i] {
            i += 1;
            continue;
        }
        let mut found_url = false;
        for prefix in PREFIXES {
            let plen = prefix.len();
            if i + plen > n {
                continue;
            }
            let matches = prefix.iter().zip(&chars[i..i + plen]).all(|(a, b)| *a == *b);
            if !matches {
                continue;
            }
            // Found prefix at i. Extend to whitespace or end.
            let start = i;
            let mut end = i + plen;
            while end < n && !chars[end].is_whitespace() && chars[end] != '\0' {
                end += 1;
            }
            // Strip trailing punctuation.
            while end > start + plen && is_trailing_punct(chars[end - 1]) {
                end -= 1;
            }
            if end > start + plen {
                for col in start..end {
                    is_url[col] = true;
                }
                i = end;
                found_url = true;
                break;
            }
        }
        if !found_url {
            i += 1;
        }
    }

    is_url
}

/// Compute URL masks for all display lines, extending URLs across wrapped
/// line boundaries.
///
/// When a URL reaches the last column of a line with `WRAPLINE` set, the URL
/// continues on the next display line. This function marks those continuation
/// columns so the entire wrapped URL is highlighted.
///
/// Trailing punctuation is stripped **after** wrap extension, so a `.` at the
/// end of a wrapped line (e.g. `x.` in `x.com`) is not incorrectly stripped.
pub fn url_masks_wrapped(
    cells: &[IndexedCell],
    num_lines: usize,
    num_cols: usize,
) -> Vec<Vec<bool>> {
    use alacritty_terminal::term::cell::Flags;

    let mut masks: Vec<Vec<bool>> = Vec::with_capacity(num_lines);
    let mut wrap_flags: Vec<bool> = Vec::with_capacity(num_lines);

    // Step 1: Per-line URL detection **without** trailing-punctuation stripping.
    // We strip after wrap extension so that a `.` at the end of a wrapped line
    // (part of a domain like `x.com`) is not removed prematurely.
    for line in 0..num_lines {
        let line_start = line * num_cols;
        let line_end = (line_start + num_cols).min(cells.len());
        if line_start >= cells.len() {
            masks.push(Vec::new());
            wrap_flags.push(false);
            continue;
        }
        let n = line_end - line_start;
        let mut chars: Vec<char> = vec![' '; n];
        let mut is_url: Vec<bool> = vec![false; n];

        for col in 0..n {
            let cell = &cells[line_start + col];
            if cell.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            chars[col] = match cell.cell.c {
                '\0' | '\t' => ' ',
                c => c,
            };
            // OSC 8 hyperlink
            if cell.cell.hyperlink().is_some() {
                is_url[col] = true;
            }
        }

        // Plain-text URLs (no trailing-punctuation stripping yet).
        let mut i = 0;
        while i < n {
            if is_url[i] {
                i += 1;
                continue;
            }
            for prefix in PREFIXES {
                let plen = prefix.len();
                if i + plen > n {
                    continue;
                }
                let matches =
                    prefix.iter().zip(&chars[i..i + plen]).all(|(a, b)| *a == *b);
                if !matches {
                    continue;
                }
                let start = i;
                let mut end = i + plen;
                while end < n && !chars[end].is_whitespace() && chars[end] != '\0' {
                    end += 1;
                }
                if end > start + plen {
                    for col in start..end {
                        is_url[col] = true;
                    }
                    i = end;
                    break;
                }
            }
            i += 1;
        }

        masks.push(is_url);
        let wraps = cells[line_end - 1].cell.flags.contains(Flags::WRAPLINE);
        wrap_flags.push(wraps);
    }

    // Step 2: Extend URLs across wrapped lines.
    for i in 0..num_lines.saturating_sub(1) {
        if !wrap_flags[i] {
            continue;
        }
        let mask = &masks[i];
        if mask.is_empty() || !mask[mask.len() - 1] {
            continue;
        }
        // URL reaches the end of line i → extend to line i+1, i+2, …
        let mut current = i + 1;
        loop {
            if current >= num_lines {
                break;
            }
            let line_start = current * num_cols;
            let line_end = (line_start + num_cols).min(cells.len());
            if line_start >= cells.len() {
                break;
            }
            let n = line_end - line_start;
            let mut hit_whitespace = false;
            for col in 0..n {
                let cell = &cells[line_start + col];
                if cell.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                let ch = match cell.cell.c {
                    '\0' | '\t' => ' ',
                    c => c,
                };
                if ch.is_whitespace() {
                    hit_whitespace = true;
                    break;
                }
                if col < masks[current].len() {
                    masks[current][col] = true;
                }
            }
            if !hit_whitespace && wrap_flags.get(current).copied().unwrap_or(false) {
                current += 1;
                continue;
            }
            break;
        }
    }

    // Step 3: Strip trailing punctuation from URL ends.
    // Only strip from the actual end of a URL — not from intermediate wrapped
    // lines that continue to the next line.
    for line in 0..num_lines {
        let mask = &mut masks[line];
        let n = mask.len();
        if n == 0 {
            continue;
        }
        let line_start = line * num_cols;
        let mut col = 0;
        while col < n {
            if !mask[col] {
                col += 1;
                continue;
            }
            let start = col;
            while col < n && mask[col] {
                col += 1;
            }
            let end = col; // exclusive

            // Don't strip if URL reaches end of line AND line wraps (continuation).
            if end >= n && wrap_flags.get(line).copied().unwrap_or(false) {
                continue;
            }

            // Strip trailing punctuation backwards from end.
            let mut stripped = end;
            while stripped > start + 1 {
                let cell_idx = line_start + stripped - 1;
                if cell_idx >= cells.len() {
                    break;
                }
                let cell = &cells[cell_idx];
                if cell.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    stripped -= 1;
                    continue;
                }
                let ch = match cell.cell.c {
                    '\0' | '\t' => ' ',
                    c => c,
                };
                if is_trailing_punct(ch) {
                    stripped -= 1;
                } else {
                    break;
                }
            }
            for c in stripped..end {
                mask[c] = false;
            }
        }
    }

    masks
}