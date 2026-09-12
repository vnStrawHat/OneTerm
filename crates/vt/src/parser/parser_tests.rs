//! Unit tests for the state machine.
//!
//! Every test that feeds a whole buffer is run a second time one byte at a
//! time: parser state, parameters, the OSC accumulator and the UTF-8 carry all
//! have to survive a chunk boundary, and that is the property the embedder
//! relies on when a sequence straddles two PTY reads.

use super::*;

/// One recorded action. `Print` is per character so the expectations read like
/// the reference's, with the run text kept separately for the batching test.
#[derive(Debug, PartialEq, Eq, Clone)]
enum Action {
    Print(char),
    Execute(u8),
    Esc(Vec<u8>, u8),
    Csi {
        params: Vec<Vec<u16>>,
        intermediates: Vec<u8>,
        ignore: bool,
        byte: u8,
    },
    Osc {
        code: Option<u32>,
        params: Vec<Vec<u8>>,
        term: StringTerm,
        truncated: bool,
    },
    Hook {
        params: Vec<Vec<u16>>,
        intermediates: Vec<u8>,
        byte: u8,
    },
    Put(u8),
    Unhook(bool),
    ApcStart(u8),
    ApcPut(u8),
    ApcEnd(bool),
}

#[derive(Default)]
struct Recorder {
    actions: Vec<Action>,
    runs: Vec<String>,
    /// OSC numbers this recorder plays the embedder for.
    large: Vec<u32>,
}

impl Recorder {
    fn with_large(codes: &[u32]) -> Self {
        Self {
            large: codes.to_vec(),
            ..Self::default()
        }
    }

    fn prints(&self) -> String {
        self.actions
            .iter()
            .filter_map(|action| match action {
                Action::Print(c) => Some(*c),
                _ => None,
            })
            .collect()
    }
}

impl Dispatch for Recorder {
    fn print_str(&mut self, text: &str) {
        self.runs.push(text.to_owned());
        self.actions.extend(text.chars().map(Action::Print));
    }

    fn execute(&mut self, byte: u8) {
        self.actions.push(Action::Execute(byte));
    }

    fn esc(&mut self, intermediates: &[u8], byte: u8) {
        self.actions.push(Action::Esc(intermediates.to_vec(), byte));
    }

    fn csi(&mut self, params: &Params, intermediates: &[u8], ignore: bool, byte: u8) {
        self.actions.push(Action::Csi {
            params: params.groups().map(<[u16]>::to_vec).collect(),
            intermediates: intermediates.to_vec(),
            ignore,
            byte,
        });
    }

    fn osc(
        &mut self,
        code: Option<u32>,
        params: &OscParams<'_>,
        term: StringTerm,
        truncated: bool,
    ) {
        self.actions.push(Action::Osc {
            code,
            params: params.iter().map(<[u8]>::to_vec).collect(),
            term,
            truncated,
        });
    }

    fn osc_allows_large(&self, code: u32) -> bool {
        self.large.contains(&code)
    }

    fn dcs_hook(&mut self, params: &Params, intermediates: &[u8], byte: u8) {
        self.actions.push(Action::Hook {
            params: params.groups().map(<[u16]>::to_vec).collect(),
            intermediates: intermediates.to_vec(),
            byte,
        });
    }

    fn dcs_put(&mut self, byte: u8) {
        self.actions.push(Action::Put(byte));
    }

    fn dcs_unhook(&mut self, aborted: bool) {
        self.actions.push(Action::Unhook(aborted));
    }

    fn apc_start(&mut self, introducer: u8) {
        self.actions.push(Action::ApcStart(introducer));
    }

    fn apc_put(&mut self, byte: u8) {
        self.actions.push(Action::ApcPut(byte));
    }

    fn apc_end(&mut self, aborted: bool) {
        self.actions.push(Action::ApcEnd(aborted));
    }
}

/// A sink that keeps counts instead of a history, for the multi-megabyte caps.
#[derive(Default)]
struct Counter {
    puts: usize,
    unhooks: Vec<bool>,
    osc_len: usize,
    osc_truncated: bool,
    oscs: usize,
    large: Vec<u32>,
}

impl Dispatch for Counter {
    fn print_str(&mut self, _text: &str) {}
    fn execute(&mut self, _byte: u8) {}
    fn esc(&mut self, _intermediates: &[u8], _byte: u8) {}
    fn csi(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _byte: u8) {}

    fn osc(
        &mut self,
        _code: Option<u32>,
        params: &OscParams<'_>,
        _term: StringTerm,
        truncated: bool,
    ) {
        self.oscs += 1;
        self.osc_len = params.iter().map(<[u8]>::len).sum();
        self.osc_truncated = truncated;
    }

    fn osc_allows_large(&self, code: u32) -> bool {
        self.large.contains(&code)
    }

    fn dcs_hook(&mut self, _params: &Params, _intermediates: &[u8], _byte: u8) {}

    fn dcs_put(&mut self, _byte: u8) {
        self.puts += 1;
    }

    fn dcs_unhook(&mut self, aborted: bool) {
        self.unhooks.push(aborted);
    }

    fn apc_start(&mut self, _introducer: u8) {}
    fn apc_put(&mut self, _byte: u8) {}
    fn apc_end(&mut self, _aborted: bool) {}
}

/// Feed `bytes` in one call.
fn feed(bytes: &[u8]) -> Recorder {
    let mut recorder = Recorder::default();
    Parser::new().advance(&mut recorder, bytes);
    recorder
}

/// Feed `bytes` one byte at a time through one parser.
fn feed_split(bytes: &[u8]) -> Recorder {
    let mut recorder = Recorder::default();
    let mut parser = Parser::new();
    for byte in bytes {
        parser.advance(&mut recorder, &[*byte]);
    }
    recorder
}

/// Feed `bytes` whole and byte by byte, asserting both produce the same actions
/// and returning them.
fn feed_both_ways(bytes: &[u8]) -> Vec<Action> {
    let whole = feed(bytes);
    let split = feed_split(bytes);
    assert_eq!(
        whole.actions, split.actions,
        "chunking changed the action sequence"
    );
    whole.actions
}

fn csi(params: &[&[u16]], intermediates: &[u8], ignore: bool, byte: u8) -> Action {
    Action::Csi {
        params: params.iter().map(|group| group.to_vec()).collect(),
        intermediates: intermediates.to_vec(),
        ignore,
        byte,
    }
}

#[test]
fn states_match_williams_table() {
    // One row per (state, byte class) pair that has an observable action.
    let cases: &[(&[u8], Vec<Action>)] = &[
        // Ground.
        (b"ab", vec![Action::Print('a'), Action::Print('b')]),
        (b"\x07", vec![Action::Execute(0x07)]),
        (b"\x7f", vec![Action::Execute(0x7F)]),
        // Escape: C0 executes, ESC is idempotent, CAN/SUB abort.
        // A C0 byte inside `Escape` executes without leaving the state, so the
        // next byte is still read as the escape's final byte.
        (
            b"\x1b\x07A",
            vec![Action::Execute(0x07), Action::Esc(Vec::new(), b'A')],
        ),
        (b"\x1b\x1b\x1b[A", vec![csi(&[&[0]], b"", false, b'A')]),
        (
            b"\x1b\x18A",
            vec![Action::Execute(0x18), Action::Print('A')],
        ),
        (b"\x1b#8", vec![Action::Esc(b"#".to_vec(), b'8')]),
        (b"\x1b(B", vec![Action::Esc(b"(".to_vec(), b'B')]),
        (b"\x1b7", vec![Action::Esc(Vec::new(), b'7')]),
        // EscapeIntermediate drops DEL and dispatches on 0x30..=0x7E.
        (b"\x1b(\x7fB", vec![Action::Esc(b"(".to_vec(), b'B')]),
        // CsiEntry.
        (b"\x1b[m", vec![csi(&[&[0]], b"", false, b'm')]),
        (b"\x1b[ q", vec![csi(&[&[0]], b" ", false, b'q')]),
        (b"\x1b[?25h", vec![csi(&[&[25]], b"?", false, b'h')]),
        // CsiIntermediate: a parameter byte after an intermediate poisons it.
        (b"\x1b[ 1m", vec![]),
        // CsiIgnore consumes its final byte without dispatching, but still
        // executes C0 while it waits.
        (b"\x1b[?1;2?m", vec![]),
        (b"\x1b[?1;2?\x07m", vec![Action::Execute(0x07)]),
        // DcsEntry drops C0 instead of executing it.
        (
            b"\x1bP\x07q\x1b\\",
            vec![
                Action::Hook {
                    params: vec![vec![0]],
                    intermediates: Vec::new(),
                    byte: b'q',
                },
                Action::Unhook(false),
                Action::Esc(Vec::new(), b'\\'),
            ],
        ),
        // DcsPassthrough forwards C0 to the sink.
        (
            b"\x1bPq\x07\x1b\\",
            vec![
                Action::Hook {
                    params: vec![vec![0]],
                    intermediates: Vec::new(),
                    byte: b'q',
                },
                Action::Put(0x07),
                Action::Unhook(false),
                Action::Esc(Vec::new(), b'\\'),
            ],
        ),
        // DcsIgnore: a private marker in DcsParam discards the sequence.
        (b"\x1bP1;2?q\x1b\\", vec![Action::Esc(Vec::new(), b'\\')]),
        // APC, SOS and PM share one state; only the introducer differs.
        (
            b"\x1b_Gi=1\x1b\\",
            vec![
                Action::ApcStart(b'_'),
                Action::ApcPut(b'G'),
                Action::ApcPut(b'i'),
                Action::ApcPut(b'='),
                Action::ApcPut(b'1'),
                Action::ApcEnd(false),
                Action::Esc(Vec::new(), b'\\'),
            ],
        ),
        (
            b"\x1bX!\x18",
            vec![
                Action::ApcStart(b'X'),
                Action::ApcPut(b'!'),
                Action::ApcEnd(true),
                Action::Execute(0x18),
            ],
        ),
        // OscString drops most C0 and keeps everything else as payload.
        (
            b"\x1b]0;hi\x07",
            vec![Action::Osc {
                code: Some(0),
                params: vec![b"0".to_vec(), b"hi".to_vec()],
                term: StringTerm::Bel,
                truncated: false,
            }],
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(&feed_both_ways(input), expected, "input {input:?}");
    }
}

#[test]
fn params_defaults_and_separators() {
    // A separator with nothing before it pushes a zero, on either side.
    assert_eq!(
        feed_both_ways(b"\x1b[;4m"),
        vec![csi(&[&[0], &[4]], b"", false, b'm')]
    );
    assert_eq!(
        feed_both_ways(b"\x1b[4;m"),
        vec![csi(&[&[4], &[0]], b"", false, b'm')]
    );
    // No parameter at all still pushes the pending zero.
    assert_eq!(
        feed_both_ways(b"\x1b[m"),
        vec![csi(&[&[0]], b"", false, b'm')]
    );
    // The separator is structure, not a run length: these two are different.
    assert_eq!(
        feed_both_ways(b"\x1b[38:2:255:0:255;1m"),
        vec![csi(&[&[38, 2, 255, 0, 255], &[1]], b"", false, b'm')]
    );
    assert_eq!(
        feed_both_ways(b"\x1b[38;2;255;0;255;1m"),
        vec![csi(
            &[&[38], &[2], &[255], &[0], &[255], &[1]],
            b"",
            false,
            b'm'
        )]
    );

    // Thirty-two colons fill the whole budget as one parameter.
    let mut input = b"\x1b[".to_vec();
    input.extend(std::iter::repeat_n(b':', MAX_PARAMS));
    input.push(b'x');
    let actions = feed_both_ways(&input);
    let Action::Csi { params, ignore, .. } = &actions[0] else {
        panic!("expected a CSI dispatch, got {actions:?}");
    };
    assert_eq!(params.len(), 1);
    assert_eq!(params[0], vec![0; MAX_PARAMS]);
    assert!(ignore, "a full parameter buffer must report ignore");
}

#[test]
fn params_keep_the_separator_that_preceded_each_value() {
    let mut parser = Parser::new();
    let mut recorder = SepRecorder::default();
    parser.advance(&mut recorder, b"\x1b[4:3;38:2::1m");
    assert_eq!(recorder.values, vec![4, 3, 38, 2, 0, 1]);
    assert_eq!(
        recorder.seps,
        vec![
            ParamSep::Semicolon,
            ParamSep::Colon,
            ParamSep::Semicolon,
            ParamSep::Colon,
            ParamSep::Colon,
            ParamSep::Colon,
        ]
    );
}

#[test]
fn param_overflow_dispatches_with_ignore() {
    let mut input = b"\x1b[".to_vec();
    for index in 0..40 {
        if index > 0 {
            input.push(b';');
        }
        input.push(b'1');
    }
    input.push(b'm');

    let actions = feed_both_ways(&input);
    let Action::Csi {
        params,
        ignore,
        byte,
        ..
    } = &actions[0]
    else {
        panic!("expected a CSI dispatch, got {actions:?}");
    };
    assert_eq!(*byte, b'm');
    assert_eq!(
        params.len(),
        MAX_PARAMS,
        "values past the limit are dropped"
    );
    assert!(
        ignore,
        "an overflowed sequence still dispatches, with ignore set"
    );
}

#[test]
fn intermediate_overflow_dispatches_with_ignore() {
    let actions = feed_both_ways(b"\x1b[!!!p");
    let Action::Csi {
        intermediates,
        ignore,
        ..
    } = &actions[0]
    else {
        panic!("expected a CSI dispatch, got {actions:?}");
    };
    assert_eq!(intermediates.len(), MAX_INTERMEDIATES);
    assert!(ignore);
}

#[test]
fn private_marker_in_csi_param_dispatches_nothing() {
    // Trap 23: this is a different path from an overflow, which does dispatch.
    assert_eq!(feed_both_ways(b"\x1b[?1;2?m"), vec![]);
    // The marker is only accepted as the first byte after the introducer.
    assert_eq!(
        feed_both_ways(b"\x1b[?1;2m"),
        vec![csi(&[&[1], &[2]], b"?", false, b'm')]
    );
}

#[test]
fn param_value_saturates_at_u16_max() {
    let actions = feed_both_ways(b"\x1b[9223372036854775808m");
    assert_eq!(actions, vec![csi(&[&[u16::MAX]], b"", false, b'm')]);
}

#[test]
fn osc_terminators_bel_and_st() {
    assert_eq!(
        feed_both_ways(b"\x1b]0;title\x07"),
        vec![Action::Osc {
            code: Some(0),
            params: vec![b"0".to_vec(), b"title".to_vec()],
            term: StringTerm::Bel,
            truncated: false,
        }]
    );

    // `ESC \` produces the OSC dispatch and then an ESC dispatch the dispatch
    // layer ignores (trap 47).
    assert_eq!(
        feed_both_ways(b"\x1b]0;title\x1b\\"),
        vec![
            Action::Osc {
                code: Some(0),
                params: vec![b"0".to_vec(), b"title".to_vec()],
                term: StringTerm::St,
                truncated: false,
            },
            Action::Esc(Vec::new(), b'\\'),
        ]
    );

    // An empty OSC still dispatches, with one empty parameter.
    assert_eq!(
        feed_both_ways(b"\x1b]\x07"),
        vec![Action::Osc {
            code: None,
            params: vec![Vec::new()],
            term: StringTerm::Bel,
            truncated: false,
        }]
    );

    // An unterminated OSC dispatches nothing and keeps its state.
    assert_eq!(feed_both_ways(b"\x1b]0;title"), vec![]);
}

#[test]
fn osc_keeps_c1_st_as_payload() {
    // Trap 24: 0x9C inside an OSC is payload, so a UTF-8 continuation byte
    // cannot cut a title short.
    assert_eq!(
        feed_both_ways(b"\x1b]2;\xe6\x9c\xab\x1b\\"),
        vec![
            Action::Osc {
                code: Some(2),
                params: vec![b"2".to_vec(), vec![0xE6, 0x9C, 0xAB]],
                term: StringTerm::St,
                truncated: false,
            },
            Action::Esc(Vec::new(), b'\\'),
        ]
    );
}

#[test]
fn osc_truncates_at_inline_cap_and_still_dispatches() {
    let mut input = b"\x1b]1337;".to_vec();
    input.extend(std::iter::repeat_n(b'x', OSC_INLINE * 2));
    input.push(0x07);

    let recorder = feed(&input);
    let Action::Osc {
        code,
        params,
        truncated,
        ..
    } = &recorder.actions[0]
    else {
        panic!("expected an OSC dispatch, got {:?}", recorder.actions);
    };
    assert_eq!(*code, Some(1337));
    assert!(truncated, "an over-long payload is truncated, not dropped");
    // The number and the payload together stop at the cap; a separator is
    // structure, not a stored byte.
    assert_eq!(params.iter().map(Vec::len).sum::<usize>(), OSC_INLINE);
}

#[test]
fn osc_spills_only_for_numbers_claimed_large() {
    let payload = OSC_INLINE * 2;

    let mut claimed = b"\x1b]52;c;".to_vec();
    claimed.extend(std::iter::repeat_n(b'A', payload));
    claimed.push(0x07);
    let mut recorder = Recorder::with_large(&[52]);
    Parser::new().advance(&mut recorder, &claimed);
    let Action::Osc {
        params, truncated, ..
    } = &recorder.actions[0]
    else {
        panic!("expected an OSC dispatch, got {:?}", recorder.actions);
    };
    assert!(
        !truncated,
        "a claimed number may spill past the inline buffer"
    );
    assert_eq!(params[2].len(), payload);

    // The same payload under a number nobody claimed stops at the inline cap.
    let mut unclaimed = b"\x1b]1337;".to_vec();
    unclaimed.extend(std::iter::repeat_n(b'A', payload));
    unclaimed.push(0x07);
    let mut recorder = Recorder::with_large(&[52]);
    Parser::new().advance(&mut recorder, &unclaimed);
    let Action::Osc { truncated, .. } = &recorder.actions[0] else {
        panic!("expected an OSC dispatch, got {:?}", recorder.actions);
    };
    assert!(truncated);
}

#[test]
fn osc_that_never_terminates_stays_bounded() {
    // The defect this replaces: an unterminated `ESC ]` from any SSH session
    // grew an unbounded `Vec<u8>`. Both tiers are bounded here.
    let mut counter = Counter {
        large: vec![52],
        ..Counter::default()
    };
    let mut parser = Parser::new();
    parser.advance(&mut counter, b"\x1b]52;c;");
    for _ in 0..((OSC_LARGE / 4096) + 4) {
        parser.advance(&mut counter, &[b'A'; 4096]);
    }
    assert_eq!(counter.oscs, 0, "nothing dispatches until the terminator");
    parser.advance(&mut counter, &[0x07]);
    assert_eq!(counter.oscs, 1);
    assert!(counter.osc_truncated);
    assert_eq!(
        counter.osc_len, OSC_LARGE,
        "the payload stops exactly at the large ceiling"
    );
}

#[test]
fn osc_params_past_sixteen_join_into_the_last() {
    let mut input = b"\x1b]8".to_vec();
    for index in 0..MAX_OSC_PARAMS + 4 {
        input.push(b';');
        input.push(b'a' + u8::try_from(index % 26).expect("index fits a byte"));
    }
    input.push(0x07);

    let actions = feed_both_ways(&input);
    let Action::Osc { params, .. } = &actions[0] else {
        panic!("expected an OSC dispatch, got {actions:?}");
    };
    assert_eq!(params.len(), MAX_OSC_PARAMS);
    // The bytes of the dropped parameters are still there, joined into the last.
    assert_eq!(params[MAX_OSC_PARAMS - 1], b"op;q;r;s;t".to_vec());
}

#[test]
fn dcs_streams_without_buffering() {
    let actions = feed_both_ways(b"\x1bP0;1q#0;2;0;0;0\x1b\\");
    assert_eq!(
        actions[0],
        Action::Hook {
            params: vec![vec![0], vec![1]],
            intermediates: Vec::new(),
            byte: b'q',
        }
    );
    let payload: Vec<u8> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Put(byte) => Some(*byte),
            _ => None,
        })
        .collect();
    assert_eq!(payload, b"#0;2;0;0;0".to_vec());
    assert_eq!(actions[actions.len() - 2], Action::Unhook(false));
}

#[test]
fn dcs_aborts_past_byte_cap() {
    let mut counter = Counter::default();
    let mut parser = Parser::new();
    parser.advance(&mut counter, b"\x1bPq");
    for _ in 0..((DCS_MAX_BYTES / 4096) + 4) {
        parser.advance(&mut counter, &[b'#'; 4096]);
    }
    assert_eq!(
        counter.puts, DCS_MAX_BYTES,
        "the sink stops receiving at the cap"
    );
    assert_eq!(
        counter.unhooks,
        vec![true],
        "the abort is reported exactly once"
    );

    // The terminator does not report a second end.
    parser.advance(&mut counter, b"\x1b\\");
    assert_eq!(counter.unhooks, vec![true]);
}

#[test]
fn dcs_exit_via_esc_resets_intermediates() {
    // Trap 47: leaving DcsPassthrough through ESC unhooks *and* clears the
    // parameters, so the following escape sees clean intermediates.
    let actions = feed_both_ways(b"\x1bP=1sZZZ\x1b+\x5c");
    assert_eq!(actions.last(), Some(&Action::Esc(b"+".to_vec(), b'\\')));
}

#[test]
fn c1_is_executed_not_an_introducer() {
    // Trap 48: 0x9B, 0x9D and 0x90 are controls, never CSI/OSC/DCS starts.
    let actions = feed_both_ways(b"\x00\x1f\x80\x90\x98\x9b\x9c\x9d\x9e\x9fa");
    assert_eq!(
        actions,
        vec![
            Action::Execute(0x00),
            Action::Execute(0x1F),
            Action::Execute(0x80),
            Action::Execute(0x90),
            Action::Execute(0x98),
            Action::Execute(0x9B),
            Action::Execute(0x9C),
            Action::Execute(0x9D),
            Action::Execute(0x9E),
            Action::Execute(0x9F),
            Action::Print('a'),
        ]
    );

    // The same bytes as valid UTF-8 are executed too, not printed.
    assert_eq!(
        feed_both_ways(b"\xc2\x9b[A"),
        vec![
            Action::Execute(0x9B),
            Action::Print('['),
            Action::Print('A'),
        ]
    );
}

#[test]
fn utf8_multibyte_runs_print_whole() {
    let actions =
        feed_both_ways(b"\xF0\x9F\x8E\x89_\xF0\x9F\xA6\x80\xF0\x9F\xA6\x80_\xF0\x9F\x8E\x89");
    assert_eq!(actions.len(), 6);
    assert_eq!(feed(b"\xF0\x9F\x8E\x89_").prints(), "\u{1f389}_");
}

#[test]
fn utf8_invalid_becomes_one_replacement() {
    let actions = feed_both_ways(b"a\xEF\xBCb");
    assert_eq!(
        actions,
        vec![
            Action::Print('a'),
            Action::Print('\u{fffd}'),
            Action::Print('b')
        ]
    );
}

#[test]
fn utf8_partial_codepoint_survives_the_chunk_boundary() {
    assert_eq!(feed_split(b"\xF0\x9F\x9A\x80").prints(), "\u{1f680}");
}

#[test]
fn utf8_partial_then_another_multibyte_character() {
    // A two-byte codepoint completes while a byte of the next character is
    // already in the carry buffer.
    let mut recorder = Recorder::default();
    let mut parser = Parser::new();
    let input = b"\xC4\xB8\xF0\x9F\x8E\x89";
    parser.advance(&mut recorder, &input[..1]);
    parser.advance(&mut recorder, &input[1..]);
    assert_eq!(recorder.prints(), "\u{138}\u{1f389}");
}

#[test]
fn utf8_partial_invalid_becomes_one_replacement() {
    assert_eq!(feed_split(b"a\xEF\xBCb").prints(), "a\u{fffd}b");
}

#[test]
fn utf8_partial_invalid_split_across_chunks() {
    let mut recorder = Recorder::default();
    let mut parser = Parser::new();
    let input = b"\xE4\xBF\x99\xB5";
    parser.advance(&mut recorder, &input[..2]);
    parser.advance(&mut recorder, &input[2..]);
    assert_eq!(recorder.prints(), "\u{4fd9}\u{fffd}");
}

#[test]
fn utf8_partial_cut_off_by_escape() {
    assert_eq!(
        feed(b"\xD8\x1b012").actions,
        vec![
            Action::Print('\u{fffd}'),
            Action::Esc(Vec::new(), b'0'),
            Action::Print('1'),
            Action::Print('2'),
        ]
    );
}

#[test]
fn print_runs_are_batched() {
    let run = vec![b'x'; 4096];
    let recorder = feed(&run);
    assert_eq!(
        recorder.runs.len(),
        1,
        "a printable run is one call, not one per character"
    );
    assert_eq!(recorder.runs[0].len(), 4096);

    // A line feed ends the run, which is the point of scanning for it.
    let recorder = feed(b"ab\ncd\r\nef");
    assert_eq!(
        recorder.runs,
        vec!["ab".to_owned(), "cd".to_owned(), "ef".to_owned()]
    );

    // So does any other control character inside the run.
    let recorder = feed(b"ab\tcd");
    assert_eq!(recorder.runs, vec!["ab".to_owned(), "cd".to_owned()]);
}

#[test]
fn unhandled_sequence_is_counted_not_echoed() {
    // R-35: an unrecognised sequence produces exactly one dispatch the layer
    // above can drop. Nothing is printed, and no bytes are handed back.
    let recorder = feed(b"\x1b[?9001h");
    assert_eq!(recorder.runs, Vec::<String>::new());
    assert_eq!(recorder.actions, vec![csi(&[&[9001]], b"?", false, b'h')]);
}

#[test]
fn sync_mode_with_leading_param_is_recognised() {
    // Deviation P6: the parser does not detect mode 2026 as an exact byte
    // sequence, so it arrives as an ordinary private mode and a leading
    // parameter no longer hides it.
    assert_eq!(
        feed_both_ways(b"\x1b[?2026h"),
        vec![csi(&[&[2026]], b"?", false, b'h')]
    );
    assert_eq!(
        feed_both_ways(b"\x1b[?1;2026h"),
        vec![csi(&[&[1], &[2026]], b"?", false, b'h')]
    );
    assert_eq!(
        feed_both_ways(b"\x1b[?2026;1l"),
        vec![csi(&[&[2026], &[1]], b"?", false, b'l')]
    );
}

#[test]
fn reset_returns_to_ground() {
    let mut recorder = Recorder::default();
    let mut parser = Parser::new();
    parser.advance(&mut recorder, b"\x1b]0;half a title");
    parser.reset();
    parser.advance(&mut recorder, b"hi");
    assert_eq!(recorder.prints(), "hi");
}

/// Records the flat value list with its separators, which the grouped view
/// cannot show.
#[derive(Default)]
struct SepRecorder {
    values: Vec<u16>,
    seps: Vec<ParamSep>,
}

impl Dispatch for SepRecorder {
    fn print_str(&mut self, _text: &str) {}
    fn execute(&mut self, _byte: u8) {}
    fn esc(&mut self, _intermediates: &[u8], _byte: u8) {}

    fn csi(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, _byte: u8) {
        self.values = params.values().to_vec();
        self.seps = (0..params.len())
            .filter_map(|index| params.sep(index))
            .collect();
    }

    fn osc(
        &mut self,
        _code: Option<u32>,
        _params: &OscParams<'_>,
        _term: StringTerm,
        _truncated: bool,
    ) {
    }

    fn dcs_hook(&mut self, _params: &Params, _intermediates: &[u8], _byte: u8) {}
    fn dcs_put(&mut self, _byte: u8) {}
    fn dcs_unhook(&mut self, _aborted: bool) {}
    fn apc_start(&mut self, _introducer: u8) {}
    fn apc_put(&mut self, _byte: u8) {}
    fn apc_end(&mut self, _aborted: bool) {}
}

/// A deterministic xorshift, so a failure is reproducible from the seed alone.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn fill(&mut self, out: &mut [u8]) {
        for byte in out.iter_mut() {
            *byte = (self.next_u64() >> 24) as u8;
        }
    }
}

mod props {
    use super::*;

    /// Regression for the carry buffer swallowing a second character.
    ///
    /// `C5 93` is `œ`; splitting after the lead byte leaves the carry to
    /// complete it while `40 97` follows, and `97` is invalid, so the buffer
    /// holds two characters plus an error. Consuming the whole valid prefix —
    /// which is what the reference does — would drop the `@`.
    #[test]
    fn utf8_carry_does_not_swallow_the_next_character() {
        let mut recorder = Recorder::default();
        let mut parser = Parser::new();
        parser.advance(&mut recorder, &[0xC0, 0xC5]);
        parser.advance(&mut recorder, &[0x93, 0x40, 0x97]);
        assert_eq!(
            recorder.actions,
            feed(&[0xC0, 0xC5, 0x93, 0x40, 0x97]).actions
        );
        assert_eq!(recorder.prints(), "\u{fffd}\u{153}@");
    }

    /// A million pseudo-random bytes must not panic the parser, and the same
    /// stream must produce one action sequence whatever the chunking.
    ///
    /// This is the in-tree substitute for the `cargo-fuzz` target: libFuzzer is
    /// unusable on `x86_64-pc-windows-msvc`, so `crates/vt/fuzz/` is a
    /// scheduled Linux activity while this runs on every `cargo test`.
    #[test]
    fn arbitrary_bytes_never_panic_and_chunking_is_invariant() {
        const BUDGET: usize = 1_000_000;
        let mut rng = Rng(0x5EED_1234_ABCD_0001);
        let mut buffer = vec![0u8; 8192];

        let mut streamed = Recorder::default();
        let mut parser = Parser::new();
        let mut fed = 0;
        while fed < BUDGET {
            rng.fill(&mut buffer);
            // Bias a third of the bytes towards ESC and the introducers, so the
            // stream spends real time outside the ground state.
            const BIAS: &[u8] = b"\x1b[]P_X;:?0123456789m\x07\\";
            for (index, byte) in buffer.iter_mut().enumerate() {
                if index % 3 == 0 {
                    *byte = BIAS[usize::from(*byte) % BIAS.len()];
                }
            }
            // Keep the recorder from growing without bound over the budget.
            streamed.actions.clear();
            streamed.runs.clear();
            parser.advance(&mut streamed, &buffer);
            fed += buffer.len();
        }

        // Chunk invariance over a smaller slice, where holding both traces is
        // cheap.
        let mut sample = vec![0u8; 64 * 1024];
        rng.fill(&mut sample);
        let whole = feed(&sample).actions;
        for chunk in [1usize, 7, 997, 4096] {
            let mut recorder = Recorder::default();
            let mut parser = Parser::new();
            for piece in sample.chunks(chunk) {
                parser.advance(&mut recorder, piece);
            }
            assert_eq!(
                recorder.actions, whole,
                "chunk size {chunk} changed the actions"
            );
        }
    }
}

/// Tier 1 of the benchmark (`testing-and-bench.md` § 7): the state machine with
/// a sink that does nothing.
///
/// `vt-bench` has no hook for an engine that does not exist yet, so this is the
/// timing note the packet records instead. It measures nothing unless
/// `ONETERM_VT_BENCH_FIXTURES` points at a directory of `*.vt` files — write
/// them with `cargo run --release -p oneterm-tools --bin vt-bench -- fixtures
/// --out <dir>`, which is the same generator the old-engine baseline used. Run
/// it in release; a debug number measures the debug profile.
mod bench_note {
    use super::*;

    #[derive(Default)]
    struct Null;

    impl Dispatch for Null {
        fn print_str(&mut self, _text: &str) {}
        fn execute(&mut self, _byte: u8) {}
        fn esc(&mut self, _intermediates: &[u8], _byte: u8) {}
        fn csi(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
        fn osc(
            &mut self,
            _code: Option<u32>,
            _params: &OscParams<'_>,
            _term: StringTerm,
            _truncated: bool,
        ) {
        }
        fn dcs_hook(&mut self, _params: &Params, _intermediates: &[u8], _byte: u8) {}
        fn dcs_put(&mut self, _byte: u8) {}
        fn dcs_unhook(&mut self, _aborted: bool) {}
        fn apc_start(&mut self, _introducer: u8) {}
        fn apc_put(&mut self, _byte: u8) {}
        fn apc_end(&mut self, _aborted: bool) {}
    }

    #[test]
    fn tier1_parser_throughput() {
        let Ok(dir) = std::env::var("ONETERM_VT_BENCH_FIXTURES") else {
            return;
        };
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("the fixture directory must exist")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "vt"))
            .collect();
        entries.sort();

        println!("| fixture | MiB/s | ns/B |");
        println!("| --- | ---: | ---: |");
        for path in entries {
            let bytes = std::fs::read(&path).expect("the fixture must be readable");
            let mut samples = Vec::new();
            for _ in 0..3 {
                let mut sink = Null;
                let mut parser = Parser::new();
                let start = std::time::Instant::now();
                parser.advance(&mut sink, &bytes);
                samples.push(start.elapsed());
            }
            samples.sort_unstable();
            let elapsed = samples[1];
            let name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let mib = (bytes.len() as f64 / (1024.0 * 1024.0)) / elapsed.as_secs_f64();
            let ns = elapsed.as_nanos() as f64 / bytes.len() as f64;
            println!("| `{name}` | {mib:.1} | {ns:.2} |");
        }
    }
}
