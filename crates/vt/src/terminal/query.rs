//! The three sequences that let a program read terminal state back:
//! `DECRQCRA`, `DECRQSS` and `XTGETTCAP`.
//!
//! They are grouped here because they share one property nothing else in the
//! dispatch layer has: the answer is *derived from* the terminal's own state
//! rather than from the request. Everything that decides what those answers
//! look like -- the checksum variant, the SGR serialisation order, the
//! capability table and its ceilings -- lives in this file, so a reader
//! checking the engine against a specification has one place to look.
//!
//! Design:
//! `docs/spec-intakes/IN-0039-vt-gaps-and-publish/low-level-design/conformance-queries.md`.

use crate::cell::{Attrs, Color, NamedColor, Style};

/// How much of a `DECRQSS` / `XTGETTCAP` payload is worth buffering.
///
/// The parser's own `DCS_MAX_BYTES` is 16 MiB, which is the right ceiling for a
/// Sixel image and the wrong one for a query: buffering to it would let one
/// hostile `DCS + q` make the terminal hold 16 MiB for the rest of the session,
/// because the buffer keeps its capacity so the common case allocates once.
///
/// 8 KiB is comfortably above every answerable request. A `DECRQSS` request is
/// two or three bytes; the largest `XTGETTCAP` request that can be answered in
/// full is [`XTGETTCAP_MAX_NAMES`] names of [`XTGETTCAP_MAX_NAME_BYTES`] bytes
/// each, hex-encoded and semicolon-separated: `16 * 128 * 2 + 15` = 4 111 --
/// and a name at that ceiling is dropped rather than answered anyway.
///
/// A payload that reaches this is over-long: it is answered with nothing and
/// counted, the same treatment the ceilings inside `XTGETTCAP` give.
pub(crate) const QUERY_MAX_BYTES: usize = 8 * 1024;

/// Which DCS query `dcs_hook` opened, if any.
///
/// Exactly one can be open at a time, and never together with the Sixel
/// decoder: a DCS cannot nest and the parser is a single state machine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DcsQuery {
    /// `DCS $ q` -- request status string.
    Decrqss,
    /// `DCS + q` -- request terminfo capability.
    Xtgettcap,
}

// ── DECRQCRA ────────────────────────────────────────────────────────────────

/// `DCS Pid ! ~ xxxx ST` -- the reply xterm gives, four upper-case hex digits
/// and the label echoed back unchanged.
///
/// **One checksum variant, never negotiated.** It is what xterm reaches with
/// `checksumExtension: 7` (`csPOSITIVE | csATTRIBS | csNOTRIM`): the positive
/// sum of the cells' first Unicode scalar values, masked to 16 bits, with no
/// attribute contribution, no negation and no trimming of trailing blanks. An
/// unwritten or erased cell counts as `U+0020`. A program written against
/// xterm's *default* (negated, with the video attributes folded in) will
/// disagree; `DECRQCRA` is a harness primitive and no such program is known.
pub(crate) fn decrqcra_reply(id: u16, checksum: u32) -> String {
    format!("\x1bP{id}!~{:04X}\x1b\\", checksum & 0xffff)
}

// ── DECRQSS ─────────────────────────────────────────────────────────────────

/// `DCS 0 $ r ST` -- the request named a setting this engine does not report.
///
/// The list of settings it *does* report is deliberately short: answering for a
/// feature the engine does not have would claim a capability that does not
/// exist, which is the one rule guide chapter 11 never bends.
pub(crate) const DECRQSS_INVALID: &str = "\x1bP0$r\x1b\\";

/// `DCS 1 $ r <value><request> ST` -- the value, then the request's own final
/// bytes, which is what makes the answer round-trippable.
pub(crate) fn decrqss_reply(value: &str, request: &str) -> String {
    format!("\x1bP1$r{value}{request}\x1b\\")
}

/// The `SGR` parameters that reproduce `style` from the default one, in xterm's
/// canonical order, without the trailing `m`.
///
/// Always starts with `0`, so the answer resets whatever the receiver had
/// before replaying it -- that reset is what makes the round trip a property
/// rather than a coincidence, and a fresh terminal correctly answers a plain
/// `0` rather than an empty string.
pub(crate) fn sgr_parameters(style: &Style) -> String {
    let mut out = String::from("0");
    let mut push = |text: &str| {
        out.push(';');
        out.push_str(text);
    };

    for (attr, code) in [
        (Attrs::BOLD, "1"),
        (Attrs::DIM, "2"),
        (Attrs::ITALIC, "3"),
        (Attrs::BLINK_SLOW, "5"),
        (Attrs::BLINK_FAST, "6"),
        (Attrs::INVERSE, "7"),
        (Attrs::HIDDEN, "8"),
        (Attrs::STRIKEOUT, "9"),
        // The underline family is mutually exclusive by construction
        // (`set_underline` clears the others), so listing all five is a table
        // walk rather than a choice.
        (Attrs::UNDERLINE, "4"),
        (Attrs::DOUBLE_UNDERLINE, "4:2"),
        (Attrs::UNDERCURL, "4:3"),
        (Attrs::DOTTED_UNDERLINE, "4:4"),
        (Attrs::DASHED_UNDERLINE, "4:5"),
        (Attrs::OVERLINE, "53"),
    ] {
        if style.attrs.contains(attr) {
            push(code);
        }
    }

    if let Some(text) = color_parameters(style.fg, 30) {
        push(&text);
    }
    if let Some(text) = color_parameters(style.bg, 40) {
        push(&text);
    }
    if let Some(color) = style.underline_color {
        // There is no named form for `SGR 58`, so a named colour cannot be
        // reported and is dropped rather than mis-reported. The parser only
        // ever stores a palette or an RGB colour there.
        if let Some(text) = color_parameters(color, 58) {
            push(&text);
        }
    }
    out
}

/// One colour as `SGR` parameters, or `None` when it is the default for its
/// role and the leading `0` already covers it.
///
/// `base` is 30 for a foreground, 40 for a background and 58 for the underline
/// colour; the extended forms are `base + 8`, except at 58 where the extended
/// form *is* 58.
fn color_parameters(color: Color, base: u16) -> Option<String> {
    let extended = if base == 58 { 58 } else { base + 8 };
    match color {
        Color::Named(NamedColor::Foreground) if base == 30 => None,
        Color::Named(NamedColor::Background) if base == 40 => None,
        Color::Named(named) => ansi_index(named).map(|index| {
            let code = if index < 8 {
                base + index
            } else {
                base + 60 + (index - 8)
            };
            code.to_string()
        }),
        Color::Palette(index) => Some(format!("{extended};5;{index}")),
        Color::Rgb(rgb) => Some(format!("{extended};2;{};{};{}", rgb.r, rgb.g, rgb.b)),
    }
}

/// The 0-15 palette index of a named slot, or `None` for a slot `SGR` cannot
/// name (the dim and bright variants the engine resolves for a renderer, the
/// cursor colour, and the two defaults at the wrong base).
fn ansi_index(named: NamedColor) -> Option<u16> {
    Some(match named {
        NamedColor::Black => 0,
        NamedColor::Red => 1,
        NamedColor::Green => 2,
        NamedColor::Yellow => 3,
        NamedColor::Blue => 4,
        NamedColor::Magenta => 5,
        NamedColor::Cyan => 6,
        NamedColor::White => 7,
        NamedColor::BrightBlack => 8,
        NamedColor::BrightRed => 9,
        NamedColor::BrightGreen => 10,
        NamedColor::BrightYellow => 11,
        NamedColor::BrightBlue => 12,
        NamedColor::BrightMagenta => 13,
        NamedColor::BrightCyan => 14,
        NamedColor::BrightWhite => 15,
        _ => return None,
    })
}

// ── XTGETTCAP ───────────────────────────────────────────────────────────────

/// Requested names past this are dropped and counted. A request is a hex
/// string, and a hostile one can be kilobytes of semicolons.
pub(crate) const XTGETTCAP_MAX_NAMES: usize = 16;

/// A requested name longer than this cannot match any table entry, so it is
/// dropped rather than echoed back truncated -- a truncated echo would be a
/// lie about what was asked.
pub(crate) const XTGETTCAP_MAX_NAME_BYTES: usize = 128;

/// What the engine answers `XTGETTCAP` with. Compiled in, sorted by name, and
/// the whole of it: the engine reads no terminfo database, no environment
/// variable and no file.
///
/// A value is the capability's raw bytes, exactly as a terminfo entry stores
/// them -- so `\x1b` is one byte, while `%p1%d` is five literal ones.
///
/// Grouped by what a capability is for rather than sorted by name: the lookup
/// is a linear scan over about twenty entries either way, and the grouping is
/// what lets a reader check the table against another terminal's without
/// reading every line. `the_capability_table_has_no_duplicate_names` is the
/// only ordering property that matters.
const CAPABILITIES: &[(&str, &str)] = &[
    // Terminal name. `TN` is what a prober asks first; the value is overridden
    // by `Config::product_name` when the embedder set one.
    ("TN", "xterm-256color"),
    // Colours. Both spellings, because tmux sends the short one and a terminfo
    // prober the long one.
    ("Co", "256"),
    ("colors", "256"),
    // Truecolour, in the two de-facto flag spellings plus the two setters a
    // prober falls back to.
    ("RGB", ""),
    ("Tc", ""),
    ("setrgbf", "\x1b[38:2::%p1%d:%p2%d:%p3%dm"),
    ("setrgbb", "\x1b[48:2::%p1%d:%p2%d:%p3%dm"),
    // Styled underlines, which the engine stores and reports.
    ("Su", ""),
    // Clipboard write, `OSC 52`, which the engine routes to the embedder.
    ("Ms", "\x1b]52;%p1%s;%p2%s\x07"),
    // Cursor style, `DECSCUSR`.
    ("Ss", "\x1b[%p1%d q"),
    ("Se", "\x1b[2 q"),
    // The alternate screen.
    ("smcup", "\x1b[?1049h"),
    ("rmcup", "\x1b[?1049l"),
    // The smallest set that lets a prober conclude the terminal is real.
    ("bel", "\x07"),
    ("clear", "\x1b[H\x1b[2J"),
    ("cr", "\r"),
    ("cub1", "\x08"),
    ("cud1", "\n"),
    ("cuf1", "\x1b[C"),
    ("cuu1", "\x1b[A"),
    ("ed", "\x1b[J"),
    ("el", "\x1b[K"),
    ("home", "\x1b[H"),
];

/// The capability's value, or `None` when the engine does not have it.
///
/// `product_name` overrides `TN`: an embedder that tells programs it is
/// `OneTerm(1.2.3)` through `XTVERSION` should not tell them it is
/// `xterm-256color` here. Only the name half is reported, because `TN` is a
/// terminfo entry name and a version in parentheses is not part of one.
pub(crate) fn capability<'a>(name: &[u8], product_name: Option<&'a str>) -> Option<&'a str> {
    if name.len() > XTGETTCAP_MAX_NAME_BYTES {
        return None;
    }
    // The lookup works on bytes: a name that decodes to invalid UTF-8 simply
    // matches nothing, and no `String` is ever built from untrusted hex.
    let (_, value) = CAPABILITIES
        .iter()
        .find(|(candidate, _)| candidate.as_bytes() == name)?;
    if name == b"TN" {
        if let Some(product) = product_name {
            let stem = product.split('(').next().unwrap_or(product).trim();
            if !stem.is_empty() {
                return Some(stem);
            }
        }
    }
    Some(value)
}

/// The requested name, echoed back exactly as it arrived, or `""` when it
/// cannot be echoed safely.
///
/// A reply splices the request's own bytes back into a DCS string, so anything
/// that is not a hex digit is dropped rather than echoed: a requested "name"
/// carrying `ESC \` would otherwise end the reply early and leave the rest of
/// it on the program's input as text. Hex digits are the only bytes a
/// well-formed request can contain and the only ones that cannot do that.
pub(crate) fn hex_echo(request: &[u8]) -> &str {
    if request.iter().all(|&byte| hex_digit(byte).is_some()) {
        // Every byte is an ASCII hex digit, so this cannot fail.
        std::str::from_utf8(request).unwrap_or("")
    } else {
        ""
    }
}

/// Decode a hex-encoded capability name in place, appending to `out`.
///
/// `false` for odd-length or non-hex input, which is answered as unknown
/// rather than partially decoded.
pub(crate) fn hex_decode(input: &[u8], out: &mut Vec<u8>) -> bool {
    if input.is_empty() || input.len() % 2 != 0 {
        return false;
    }
    for pair in input.chunks_exact(2) {
        let (Some(high), Some(low)) = (hex_digit(pair[0]), hex_digit(pair[1])) else {
            return false;
        };
        out.push((high << 4) | low);
    }
    true
}

const fn hex_digit(byte: u8) -> Option<u8> {
    Some(match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => return None,
    })
}

/// Append `bytes` as upper-case hex pairs.
pub(crate) fn hex_encode(bytes: &[u8], out: &mut String) {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    for &byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0xf)] as char);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capability_table_has_no_duplicate_names() {
        // A duplicate would be answered by whichever entry came first, which is
        // the one table property the linear scan cannot make obvious.
        let mut names: Vec<&str> = CAPABILITIES.iter().map(|(name, _)| *name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "duplicate capability name");
    }

    #[test]
    fn every_answerable_request_fits_the_payload_ceiling() {
        // The arithmetic `QUERY_MAX_BYTES`'s documentation states, asserted so
        // that raising a ceiling without raising the buffer fails here.
        let largest = XTGETTCAP_MAX_NAMES * XTGETTCAP_MAX_NAME_BYTES * 2 + XTGETTCAP_MAX_NAMES - 1;
        assert!(largest < QUERY_MAX_BYTES, "{largest} names do not fit");
    }

    #[test]
    fn hex_decode_refuses_odd_and_non_hex_input() {
        let mut out = Vec::new();
        assert!(!hex_decode(b"54E", &mut out));
        assert!(!hex_decode(b"54ZZ", &mut out));
        assert!(!hex_decode(b"", &mut out));
        out.clear();
        assert!(hex_decode(b"544e", &mut out));
        assert_eq!(out, b"TN");
    }

    #[test]
    fn hex_echo_refuses_anything_that_could_end_the_reply() {
        assert_eq!(hex_echo(b"544e"), "544e");
        assert_eq!(hex_echo(b"544"), "544", "odd length is still hex");
        // The whole point: an echo of these would terminate the DCS reply and
        // leave the tail on the program's input as text.
        assert_eq!(hex_echo(b"54\x1b\\4e"), "");
        assert_eq!(hex_echo(b"\x9c"), "");
        assert_eq!(hex_echo("544é".as_bytes()), "");
    }

    #[test]
    fn hex_encode_is_upper_case_pairs() {
        let mut out = String::new();
        hex_encode(b"\x1b[C", &mut out);
        assert_eq!(out, "1B5B43");
    }
}
