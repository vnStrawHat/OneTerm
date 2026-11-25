//! Independent verification tests for US-0100 (search moves into oneterm-vt).
//!
//! These live in `tests/` on purpose: they exercise `oneterm_vt::search` as an
//! *external* crate would, so they also prove the public surface is usable
//! without any `pub(crate)` help.
//!
//! Run:
//!   cargo test -p oneterm-vt --test verify_us0100 -- --nocapture
//!   cargo test -p oneterm-vt --features regex --test verify_us0100 -- --nocapture

use std::time::Instant;

use oneterm_vt::search::{GridText, SearchOptions, SearchPattern, search_grid_text};
use oneterm_vt::{Config, EventBatch, Size, Terminal};

fn term_with(cols: u16, rows: u16, scrollback: u32, bytes: &[u8]) -> Terminal {
    let mut cfg = Config::default();
    cfg.scrollback_limit = scrollback;
    let mut term = Terminal::new(Size { rows, cols }, cfg);
    term.feed(bytes, &mut EventBatch::new(), Instant::now());
    term
}

fn term(cols: u16, rows: u16, bytes: &[u8]) -> Terminal {
    term_with(cols, rows, 10_000, bytes)
}

fn literal(t: &Terminal, q: &str) -> Vec<oneterm_vt::search::SearchMatch> {
    search_grid_text(
        &GridText::from_terminal(t),
        SearchPattern::Literal(q),
        SearchOptions::default(),
    )
}

// ───────────────────────── literal behaviour ─────────────────────────

/// A string that straddles an auto-wrap boundary is NOT found. Documented
/// limitation; asserted here so a future "fix" cannot land silently.
#[test]
fn literal_does_not_match_across_a_wrapped_row_boundary() {
    // cols = 5 -> "abcde" / "fghij"
    let t = term(5, 3, b"abcdefghij");
    assert_eq!(literal(&t, "abcde").len(), 1, "within row 0");
    assert_eq!(literal(&t, "fghij").len(), 1, "within row 1");
    assert!(
        literal(&t, "efg").is_empty(),
        "spanning the wrap boundary must find nothing"
    );
    // ... and the needle being longer than a row is also nothing.
    assert!(literal(&t, "abcdefg").is_empty());
}

/// Search sees the *active* screen. On the alt screen the primary content is
/// invisible; switching back restores it. (`GridText::from_terminal` is
/// byte-identical to main's, so this records behaviour, it does not change it.)
#[test]
fn literal_follows_the_primary_alt_screen_switch() {
    let mut t = term(20, 3, b"PRIMARYTEXT\r\n");
    assert_eq!(literal(&t, "PRIMARYTEXT").len(), 1);

    // DECSET 1049: alt screen.
    t.feed(
        b"\x1b[?1049hALTTEXT",
        &mut EventBatch::new(),
        Instant::now(),
    );
    let on_alt_alt = literal(&t, "ALTTEXT").len();
    let on_alt_primary = literal(&t, "PRIMARYTEXT").len();

    // Back to the primary screen.
    t.feed(b"\x1b[?1049l", &mut EventBatch::new(), Instant::now());
    let back_primary = literal(&t, "PRIMARYTEXT").len();
    let back_alt = literal(&t, "ALTTEXT").len();

    println!(
        "alt-screen: on_alt(alt={on_alt_alt}, primary={on_alt_primary}) \
         back(primary={back_primary}, alt={back_alt})"
    );
    assert_eq!(
        on_alt_alt, 1,
        "alt content is searchable while on the alt screen"
    );
    assert_eq!(
        on_alt_primary, 0,
        "primary content is not visible from the alt screen"
    );
    assert_eq!(back_primary, 1, "primary content comes back");
    assert_eq!(back_alt, 0, "alt content is gone");
}

/// Case-insensitive matching is ASCII-only folding. Non-ASCII case pairs do NOT
/// fold. This is main's behaviour (`char::eq_ignore_ascii_case`), preserved.
#[test]
fn case_insensitive_default_is_ascii_only() {
    // Turkish dotless i (U+0131) and dotted capital I (U+0130), German sharp s.
    let t = term(
        40,
        2,
        "I \u{131} \u{130} i \u{df} SS Stra\u{df}e".as_bytes(),
    );

    // ASCII pair folds.
    assert_eq!(
        literal(&t, "i").len(),
        2,
        "ASCII 'I' and 'i' both match 'i'"
    );

    // Turkish dotless i does not fold to ASCII I.
    assert_eq!(literal(&t, "\u{131}").len(), 1, "only the dotless i itself");
    let dotless = literal(&t, "\u{131}");
    assert_eq!(dotless[0].start_col, 2);

    // Dotted capital I (U+0130) does not fold to ASCII i either.
    assert_eq!(literal(&t, "\u{130}").len(), 1);

    // Sharp s never folds to "SS" (and lengths differ, so columns could not
    // stay 1:1 if it did).
    assert_eq!(literal(&t, "ss").len(), 1, "only the literal ASCII 'SS'");
    assert_eq!(literal(&t, "\u{df}").len(), 2, "the two sharp s glyphs");
    assert!(literal(&t, "strasse").is_empty());
    assert_eq!(
        literal(&t, "STRA\u{df}E").len(),
        1,
        "ASCII half folds, sharp s is exact"
    );
}

/// Rough linearity check on a deep scrollback. Not a benchmark: it prints the
/// two timings and only fails on a wildly super-linear result.
#[test]
#[ignore = "slow; run explicitly with --ignored"]
fn deep_scrollback_search_is_linear() {
    fn measure(rows: usize) -> (u128, u128, usize) {
        let mut input = Vec::with_capacity(rows * 20);
        for _ in 0..rows {
            input.extend_from_slice(b"alpha beta gamma delta needle\r\n");
        }
        let t = term_with(80, 24, 200_000, &input);
        let copy_start = Instant::now();
        let text = GridText::from_terminal(&t);
        let copy_ms = copy_start.elapsed().as_millis();
        let search_start = Instant::now();
        let hits = search_grid_text(
            &text,
            SearchPattern::Literal("needle"),
            SearchOptions::default(),
        );
        let search_ms = search_start.elapsed().as_millis();
        println!(
            "rows={rows} grid_rows={} copy={copy_ms}ms search={search_ms}ms hits={}",
            text.rows(),
            hits.len()
        );
        (copy_ms, search_ms, hits.len())
    }

    let (copy_small, search_small, hits_small) = measure(25_000);
    let (copy_big, search_big, hits_big) = measure(100_000);
    assert!(hits_small >= 25_000 - 24);
    assert!(hits_big >= 100_000 - 24);
    // 4x the rows must not cost more than 12x the time.
    let ratio = |big: u128, small: u128| (big as f64) / ((small.max(1)) as f64);
    println!(
        "ratios: copy={:.2} search={:.2} (4x rows)",
        ratio(copy_big, copy_small),
        ratio(search_big, search_small)
    );
    assert!(
        ratio(search_big, search_small) < 12.0,
        "search looks super-linear"
    );
}

/// `SearchOptions` is `#[non_exhaustive]`: the only construction form available
/// to an external crate is `default()` plus field assignment. This compiles;
/// the struct-literal form does not (checked separately, see the report).
#[test]
fn search_options_is_constructible_only_through_default() {
    let mut opts = SearchOptions::default();
    opts.case_sensitive = true;
    opts.whole_word = true;
    let t = term(20, 2, b"foo foobar foo");
    let hits = search_grid_text(
        &GridText::from_terminal(&t),
        SearchPattern::Literal("foo"),
        opts,
    );
    assert_eq!(hits.len(), 2);
}

/// `SearchPattern` is `Copy` (used twice without a clone) and `Debug`.
#[test]
fn search_pattern_is_copy_and_debug() {
    let p = SearchPattern::Literal("x");
    let a = p;
    let b = p;
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
    assert!(format!("{p:?}").contains("Literal"));
}

// ───────────────────────── regex behaviour ─────────────────────────

#[cfg(feature = "regex")]
mod regex_checks {
    use super::*;
    use regex::Regex;

    fn re_hits(t: &Terminal, pattern: &str) -> Vec<oneterm_vt::search::SearchMatch> {
        let re = Regex::new(pattern).expect("pattern compiles");
        search_grid_text(
            &GridText::from_terminal(t),
            SearchPattern::Regex(&re),
            SearchOptions::default(),
        )
    }

    /// `^` and `$` anchor to the first and LAST CELL of the padded row, not to
    /// the end of the typed text.
    #[test]
    fn anchors_are_per_row_and_see_the_padding() {
        let t = term(10, 2, b"ab\r\ncd");
        let starts = re_hits(&t, "^..");
        assert_eq!(starts.len(), 2, "one per row");
        assert!(starts.iter().all(|h| h.start_col == 0));

        // `$` right after the text does NOT match: the row is padded to 10.
        assert!(re_hits(&t, "ab$").is_empty(), "text end is not row end");
        // `$` at the last cell does.
        let ends = re_hits(&t, "..$");
        assert_eq!(ends.len(), 2);
        assert!(ends.iter().all(|h| h.end_col == 10));
    }

    /// `\s` matches the trailing blanks; `\s+$` spans text-end to row-end.
    #[test]
    fn whitespace_matches_the_trailing_padding() {
        let t = term(8, 1, b"ab");
        let h = re_hits(&t, r"\s+$");
        assert_eq!(h.len(), 1);
        assert_eq!((h[0].start_col, h[0].end_col), (2, 8));
        // An entirely blank row is all whitespace.
        let blank = term(8, 1, b"");
        let b = re_hits(&blank, r"^\s+$");
        assert_eq!(b.len(), 1);
        assert_eq!((b[0].start_col, b[0].end_col), (0, 8));
    }

    /// A pattern that matches empty at every position terminates on a wide row
    /// and yields exactly `cols + 1` matches, the last one AT `cols` -- i.e.
    /// one column past the last valid column.
    #[test]
    fn empty_matching_pattern_on_a_2048_column_row() {
        let cols: u16 = 2048;
        let t = term(cols, 1, b"");
        let start = Instant::now();
        let h = re_hits(&t, "x*");
        let ms = start.elapsed().as_millis();
        println!("empty-match on {cols} cols: {} matches in {ms}ms", h.len());
        assert_eq!(h.len(), usize::from(cols) + 1);
        assert!(h.iter().all(|m| m.start_col == m.end_col));
        assert_eq!(h[0].start_col, 0);
        assert_eq!(
            h[h.len() - 1].start_col,
            usize::from(cols),
            "the last empty match sits one past the last column"
        );
    }

    /// `.` matches the wide-char spacer, which reads as NUL.
    #[test]
    fn dot_matches_the_wide_char_spacer() {
        let t = term(4, 1, "\u{65e5}".as_bytes()); // one wide char + spacer + 2 blanks
        let dots = re_hits(&t, ".");
        assert_eq!(dots.len(), 4, "every cell, spacer included");
        // The spacer is literally NUL.
        let nul = re_hits(&t, "\u{0}");
        assert_eq!(nul.len(), 1);
        assert_eq!(nul[0].start_col, 1);
        // A wide glyph is one column.
        let wide = re_hits(&t, "\u{65e5}");
        assert_eq!(wide.len(), 1);
        assert_eq!((wide[0].start_col, wide[0].end_col), (0, 1));
    }

    /// An invalid pattern is the caller's error: compilation happens outside
    /// this crate, so the engine never sees it and never panics.
    #[test]
    // The two patterns below are invalid on purpose; `clippy::invalid_regex`
    // would otherwise reject the file for containing them.
    #[allow(clippy::invalid_regex)]
    fn invalid_regex_is_a_caller_error_not_a_panic() {
        assert!(Regex::new("(").is_err());
        assert!(Regex::new("a{2,1}").is_err());
        // The engine's entry point is unreachable without a compiled Regex.
    }

    /// The `regex` crate has no backtracking: a classic catastrophic pattern is
    /// linear here. Timed so a regression shows up as a timeout, not a hang.
    #[test]
    fn no_catastrophic_backtracking() {
        let row = "a".repeat(60) + "b";
        let t = term(80, 1, row.as_bytes());
        let start = Instant::now();
        let h = re_hits(&t, "^(a+)+$");
        let ms = start.elapsed().as_millis();
        println!(
            "catastrophic pattern on 60 a's + b: {} hits in {ms}ms",
            h.len()
        );
        assert!(ms < 2_000, "took {ms}ms -- backtracking?");
        assert!(
            h.is_empty(),
            "the row ends with padding, so ^(a+)+$ cannot match"
        );
    }

    /// A metacharacter-free pattern must agree with the literal scanner to the
    /// column, including on a row with history.
    #[test]
    fn literal_equivalence_holds_with_history() {
        let t = term_with(12, 2, 100, b"needle one\r\nfiller\r\nneedle two\r\nfiller2");
        let lit = search_grid_text(
            &GridText::from_terminal(&t),
            SearchPattern::Literal("needle"),
            SearchOptions::default(),
        );
        assert_eq!(re_hits(&t, "needle"), lit);
        assert_eq!(lit.len(), 2);
    }
}
