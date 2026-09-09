//! Multi-line URL masks for always-on highlighting.

use super::{PREFIXES, is_trailing_punct};
use crate::render::frame::Frame;

/// URL masks for every display row of `frame`, extending URLs across wrapped
/// rows, written into caller-owned buffers so the per-frame recomputation
/// allocates nothing once the grid size settles.
///
/// A URL reaching the last column of a row with `WRAPLINE` continues on the
/// next row. Trailing punctuation is stripped **after** wrap extension, so a
/// `.` at the end of a wrapped row (`x.` in `x.com`) survives.
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
