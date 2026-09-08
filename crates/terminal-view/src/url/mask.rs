//! Multi-line URL masks for always-on highlighting.

use oneterm_terminal::IndexedCell;

use super::{PREFIXES, is_trailing_punct};
use crate::render::frame::Frame;

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
                let matches = prefix
                    .iter()
                    .zip(&chars[i..i + plen])
                    .all(|(a, b)| *a == *b);
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

/// [`url_masks_wrapped`] over a [`Frame`], writing into caller-owned buffers so
/// the per-frame recomputation allocates nothing once the grid size settles.
///
/// `masks[row][col]` is true for every column that belongs to a URL; `wraps`
/// is scratch space for the per-row `WRAPLINE` flags. Both are resized to the
/// frame, keeping their inner allocations.
pub(crate) fn url_masks_into(frame: &Frame, masks: &mut Vec<Vec<bool>>, wraps: &mut Vec<bool>) {
    let rows = usize::from(frame.size().rows);
    masks.resize_with(rows, Vec::new);
    wraps.clear();
    wraps.resize(rows, false);

    // Pass 1: per-row detection without trailing-punctuation stripping, which
    // waits until wrap extension so `x.` at the end of a wrapped `x.com` stays.
    for (r, mask) in masks.iter_mut().enumerate() {
        let row = frame.row(r);
        let n = row.len();
        mask.clear();
        mask.resize(n, false);
        wraps[r] = row.wraps();
        for col in 0..n {
            let cell = row.cell(col);
            if !cell.is_spacer() && cell.hyperlink.is_some() {
                mask[col] = true;
            }
        }
        let ch_at = |col: usize| -> char {
            let cell = row.cell(col);
            if cell.is_spacer() {
                return ' ';
            }
            match cell.ch {
                '\0' | '\t' => ' ',
                c => c,
            }
        };
        let mut i = 0;
        while i < n {
            if mask[i] {
                i += 1;
                continue;
            }
            let mut matched = false;
            for prefix in PREFIXES {
                let plen = prefix.len();
                if i + plen > n || !prefix.iter().enumerate().all(|(k, &p)| ch_at(i + k) == p) {
                    continue;
                }
                let mut end = i + plen;
                while end < n && !ch_at(end).is_whitespace() {
                    end += 1;
                }
                if end > i + plen {
                    for m in &mut mask[i..end] {
                        *m = true;
                    }
                    i = end;
                    matched = true;
                    break;
                }
            }
            if !matched {
                i += 1;
            }
        }
    }

    // Pass 2: a URL reaching the end of a wrapped row continues on the next.
    for i in 0..rows.saturating_sub(1) {
        if !wraps[i] || !masks[i].last().copied().unwrap_or(false) {
            continue;
        }
        let mut current = i + 1;
        while current < rows {
            let row = frame.row(current);
            let mut hit_whitespace = false;
            for col in 0..row.len() {
                let cell = row.cell(col);
                if cell.is_spacer() {
                    continue;
                }
                let ch = match cell.ch {
                    '\0' | '\t' => ' ',
                    c => c,
                };
                if ch.is_whitespace() {
                    hit_whitespace = true;
                    break;
                }
                if let Some(m) = masks[current].get_mut(col) {
                    *m = true;
                }
            }
            if hit_whitespace || !wraps[current] {
                break;
            }
            current += 1;
        }
    }

    // Pass 3: strip trailing punctuation from the real end of each URL only.
    for (r, mask) in masks.iter_mut().enumerate() {
        let row = frame.row(r);
        let n = mask.len();
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
            let end = col;
            if end >= n && wraps[r] {
                continue;
            }
            let mut stripped = end;
            while stripped > start + 1 {
                let cell = row.cell(stripped - 1);
                if cell.is_spacer() {
                    stripped -= 1;
                    continue;
                }
                if is_trailing_punct(cell.ch) {
                    stripped -= 1;
                } else {
                    break;
                }
            }
            for m in &mut mask[stripped..end] {
                *m = false;
            }
        }
    }
}

#[cfg(test)]
mod frame_tests {
    use super::url_masks_into;
    use crate::render::frame::CellFlags;
    use crate::render::frame::test_support::FrameBuilder;

    fn masks(frame: &crate::render::frame::Frame) -> Vec<Vec<bool>> {
        let mut out = Vec::new();
        let mut wraps = Vec::new();
        url_masks_into(frame, &mut out, &mut wraps);
        out
    }

    #[test]
    fn url_masks_into_marks_plain_urls_and_strips_punctuation() {
        let frame = FrameBuilder::new(1, 30)
            .text(0, 0, "see https://x.test/a). ok")
            .build();
        let m = masks(&frame);
        let marked: Vec<usize> = (0..30).filter(|&c| m[0][c]).collect();
        assert_eq!(marked, (4..20).collect::<Vec<_>>(), "{marked:?}");
    }

    #[test]
    fn url_masks_into_extends_across_wrapped_rows_and_reuses_buffers() {
        let frame = FrameBuilder::new(3, 10)
            .text(0, 0, "https://x.")
            .flags(0, 9, CellFlags::WRAPLINE)
            .text(1, 0, "com/p. end")
            .hyperlink(2, 3, "https://y.test")
            .build();
        let mut out = Vec::new();
        let mut wraps = Vec::new();
        url_masks_into(&frame, &mut out, &mut wraps);
        assert!(out[0].iter().all(|&m| m), "row 0 is all URL: {:?}", out[0]);
        assert_eq!(&out[1][..5], &[true; 5]);
        assert!(!out[1][5], "trailing '.' stripped");
        assert!(out[2][3] && !out[2][2]);
        let ptrs: Vec<*const bool> = out.iter().map(|v| v.as_ptr()).collect();
        url_masks_into(&frame, &mut out, &mut wraps);
        let again: Vec<*const bool> = out.iter().map(|v| v.as_ptr()).collect();
        assert_eq!(ptrs, again, "inner buffers must be reused");
    }
}
