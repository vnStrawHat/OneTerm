//! The differential oracle: `oneterm_vt::parser` against the raw `vte` state
//! machine (IN-0029 R-41).
//!
//! **Why the vendored fork is still a pristine oracle.** The workspace
//! `[patch]` redirects `vte` at `vendor/vte` for every dependency kind,
//! dev-dependencies included, so this test cannot ask for the crates.io release.
//! It does not need to: both vendored patches modify only `src/ansi.rs` — the
//! semantic `Handler` layer — and neither touches `src/lib.rs` (the `Parser`
//! state machine, the `Perform` trait and the UTF-8 handling) or
//! `src/params.rs`. The comparison here is at the `Perform` level, which is
//! therefore upstream byte for byte.
//! `oracle_precondition_patches_do_not_touch_the_state_machine` asserts that
//! precondition instead of trusting it, so a future patch that reaches into
//! `lib.rs` fails the oracle's own contract rather than silently weakening it.
//!
//! **The deliberate differences**, each filtered explicitly and counted, and
//! each one a row of `low-level-design/parser.md` § "Deliberate deviations"
//! unless marked otherwise:
//!
//! | # | Difference | How it is filtered |
//! | --- | --- | --- |
//! | P1 | `print_str(&str)` instead of `print(char)` per character | the recorder splits every run back into characters |
//! | P2 | withdrawn — the scan is `memchr(ESC)`, as the reference's is (packet deviation V9) | none |
//! | P3 | the separator is kept per parameter | no filter: `Params::groups()` reproduces the reference's grouping exactly |
//! | P4 | OSC bounded at 2 KiB / 8 MiB with truncation | an OSC the new parser marks `truncated` is accepted against the reference's unbounded one |
//! | P5 | DCS bounded at 16 MiB, and `dcs_unhook` carries an `aborted` flag | the flag has no reference counterpart, so `Unhook` is compared without it. The cap itself cannot fire on inputs this size — 16 MiB against at most a megabyte — and is proven by `parser::parser_tests::dcs_aborts_past_byte_cap` |
//! | P6 | mode 2026 is an ordinary private mode | no filter: the reference's buffering is above the `Perform` level |
//! | P7 | APC is streamed to a sink | the recorder drops `apc_*`; the reference emits nothing for SOS/PM/APC |
//! | P8 | `DEL` is executed, not printed | `Execute(0x7F)` is accepted against `Print('\u{7f}')`. **Not in the LLD's table**: its Ground row says `execute` while its deviation list does not carry the row, so the implementation follows the row and declares the difference here (packet US-0073, deviation V2) |
//! | P9 | parameters past the sixteenth join into the last **with** their separators | only the first fifteen are compared once either side reports sixteen. The reference stops extending the sixteenth at the seventeenth `;`, which loses the tail OSC 8 rejoins |
//!
//! The recorder is fed whole buffers, so the carry-buffer difference the packet
//! records as V6 (the reference consumes a whole valid prefix and can drop a
//! character) is unreachable here; it is covered by
//! `utf8_carry_does_not_swallow_the_next_character` in the unit suite.

use std::path::{Path, PathBuf};

use oneterm_vt::parser::{Dispatch, OscParams, Params, Parser, StringTerm};

/// One action, in the shape both engines can produce.
#[derive(Debug, PartialEq, Eq, Clone)]
enum Action {
    Print(char),
    Execute(u8),
    Esc {
        intermediates: Vec<u8>,
        ignore: bool,
        byte: u8,
    },
    Csi {
        params: Vec<Vec<u16>>,
        intermediates: Vec<u8>,
        ignore: bool,
        byte: u8,
    },
    Osc {
        params: Vec<Vec<u8>>,
        bel: bool,
        truncated: bool,
    },
    Hook {
        params: Vec<Vec<u16>>,
        intermediates: Vec<u8>,
        ignore: bool,
        byte: u8,
    },
    Put(u8),
    /// Without the `aborted` flag: the reference has no such concept, and it
    /// reports a `CAN`/`SUB` abort through the same callback as a clean end.
    Unhook,
}

/// Records the new parser's actions, normalised to the reference's shape.
#[derive(Default)]
struct NewEngine {
    actions: Vec<Action>,
}

impl Dispatch for NewEngine {
    fn print_str(&mut self, text: &str) {
        self.actions.extend(text.chars().map(Action::Print));
    }

    fn execute(&mut self, byte: u8) {
        self.actions.push(Action::Execute(byte));
    }

    fn esc(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        self.actions.push(Action::Esc {
            intermediates: intermediates.to_vec(),
            ignore,
            byte,
        });
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
        _code: Option<u32>,
        params: &OscParams<'_>,
        term: StringTerm,
        truncated: bool,
    ) {
        self.actions.push(Action::Osc {
            params: params.iter().map(<[u8]>::to_vec).collect(),
            bel: term == StringTerm::Bel,
            truncated,
        });
    }

    fn dcs_hook(&mut self, params: &Params, intermediates: &[u8], byte: u8) {
        self.actions.push(Action::Hook {
            params: params.groups().map(<[u16]>::to_vec).collect(),
            intermediates: intermediates.to_vec(),
            ignore: params.ignored(),
            byte,
        });
    }

    fn dcs_put(&mut self, byte: u8) {
        self.actions.push(Action::Put(byte));
    }

    fn dcs_unhook(&mut self, _aborted: bool) {
        self.actions.push(Action::Unhook);
    }

    // P7: the reference discards SOS/PM/APC payload without a callback.
    fn apc_start(&mut self, _introducer: u8) {}
    fn apc_put(&mut self, _byte: u8) {}
    fn apc_end(&mut self, _aborted: bool) {}
}

/// Records the reference state machine's actions.
#[derive(Default)]
struct Oracle {
    actions: Vec<Action>,
}

impl vte::Perform for Oracle {
    fn print(&mut self, c: char) {
        self.actions.push(Action::Print(c));
    }

    fn execute(&mut self, byte: u8) {
        self.actions.push(Action::Execute(byte));
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        self.actions.push(Action::Esc {
            intermediates: intermediates.to_vec(),
            ignore,
            byte,
        });
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        self.actions.push(Action::Csi {
            params: params.iter().map(<[u16]>::to_vec).collect(),
            intermediates: intermediates.to_vec(),
            ignore,
            byte: action as u8,
        });
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        self.actions.push(Action::Osc {
            params: params.iter().map(|param| param.to_vec()).collect(),
            bel: bell_terminated,
            truncated: false,
        });
    }

    fn hook(&mut self, params: &vte::Params, intermediates: &[u8], ignore: bool, action: char) {
        self.actions.push(Action::Hook {
            params: params.iter().map(<[u16]>::to_vec).collect(),
            intermediates: intermediates.to_vec(),
            ignore,
            byte: action as u8,
        });
    }

    fn put(&mut self, byte: u8) {
        self.actions.push(Action::Put(byte));
    }

    fn unhook(&mut self) {
        self.actions.push(Action::Unhook);
    }
}

/// How many times each declared difference was accepted.
#[derive(Default, Debug, PartialEq, Eq)]
struct Filters {
    truncated_osc: usize,
    del_executed: usize,
    joined_osc_params: usize,
}

fn actions_new(bytes: &[u8]) -> Vec<Action> {
    let mut engine = NewEngine::default();
    Parser::new().advance(&mut engine, bytes);
    engine.actions
}

fn actions_oracle(bytes: &[u8]) -> Vec<Action> {
    let mut oracle = Oracle::default();
    vte::Parser::new().advance(&mut oracle, bytes);
    oracle.actions
}

/// Compare two traces, accepting only the declared differences.
///
/// Both traces are walked in step; an accepted pair advances both cursors and
/// moves a counter, and anything else fails with the position and both actions.
fn assert_agree(name: &str, bytes: &[u8], filters: &mut Filters) {
    let new = actions_new(bytes);
    let reference = actions_oracle(bytes);

    let mut index = 0;
    while index < new.len() && index < reference.len() {
        let (mine, theirs) = (&new[index], &reference[index]);
        if mine == theirs {
            index += 1;
            continue;
        }
        if accept(mine, theirs, filters) {
            index += 1;
            continue;
        }
        // A truncated OSC swallows the reference's remaining payload bytes only
        // inside one action, so a length mismatch here is a real divergence.
        panic!(
            "{name}: action {index} diverges\n  new       {mine:?}\n  reference {theirs:?}\n  \
             context {:?}",
            &new[index.saturating_sub(2)..(index + 2).min(new.len())]
        );
    }
    assert_eq!(
        new.len(),
        reference.len(),
        "{name}: the traces have different lengths ({} against {})",
        new.len(),
        reference.len()
    );
}

/// Whether this pair is one of the declared differences.
fn accept(mine: &Action, theirs: &Action, filters: &mut Filters) -> bool {
    match (mine, theirs) {
        // P8: DEL is executed rather than printed.
        (Action::Execute(0x7F), Action::Print('\u{7f}')) => {
            filters.del_executed += 1;
            true
        }
        // P4 and P9: a truncated payload, or parameters joined into the last.
        (
            Action::Osc {
                params: mine,
                truncated,
                ..
            },
            Action::Osc {
                params: theirs,
                bel: _,
                ..
            },
        ) => {
            let joined = mine.len() == oneterm_vt::parser::MAX_OSC_PARAMS
                && theirs.len() == oneterm_vt::parser::MAX_OSC_PARAMS
                && mine[..mine.len() - 1] == theirs[..theirs.len() - 1];
            if joined {
                filters.joined_osc_params += 1;
                return true;
            }
            // A truncated payload must still be a *prefix* of the reference's,
            // parameter for parameter: accepting any pair once `truncated` is
            // set would mask a genuine bug inside a truncated OSC.
            if *truncated && is_prefix_of(mine, theirs) {
                filters.truncated_osc += 1;
                return true;
            }
            false
        }
        _ => false,
    }
}

/// Whether the truncated parameter list is a prefix of the reference's: every
/// parameter before the truncated one identical, and the truncated one a prefix
/// of its counterpart.
fn is_prefix_of(mine: &[Vec<u8>], theirs: &[Vec<u8>]) -> bool {
    if mine.len() > theirs.len() {
        return false;
    }
    let Some((last, head)) = mine.split_last() else {
        return true;
    };
    head == &theirs[..head.len()] && theirs[head.len()].starts_with(last)
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/alacritty-ref")
}

fn recordings() -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(corpus_dir())
        .expect("the vendored corpus must be present")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path().join("recording");
            let name = entry.file_name().to_string_lossy().into_owned();
            std::fs::read(&path).ok().map(|bytes| (name, bytes))
        })
        .collect();
    out.sort_by(|left, right| left.0.cmp(&right.0));
    out
}

#[test]
fn action_traces_agree_on_ref_corpus() {
    let recordings = recordings();
    assert_eq!(
        recordings.len(),
        45,
        "the corpus is the 45 vendored alacritty recordings"
    );

    let mut filters = Filters::default();
    for (name, bytes) in &recordings {
        assert_agree(name, bytes, &mut filters);
    }
    // Real captures exercise none of the caps, so the corpus must agree exactly.
    assert_eq!(
        filters,
        Filters::default(),
        "the recordings needed a declared difference"
    );
}

/// A deterministic xorshift, so a failure is reproducible from its seed.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, limit: usize) -> usize {
        (self.next_u64() % limit as u64) as usize
    }

    fn fill(&mut self, out: &mut [u8]) {
        for byte in out.iter_mut() {
            *byte = (self.next_u64() >> 24) as u8;
        }
    }
}

#[test]
fn action_traces_agree_on_generated_streams() {
    // The LLD names Ghostty's AFL++ seed corpus here; it is not vendored, so
    // the seeds are generated instead: uniform noise, an escape-biased stream,
    // spliced recordings and bit-flipped recordings.
    const BIAS: &[u8] = b"\x1b[]P_X^;:?0123456789<=>!\x20\x07\\mhlqrp\x18\x1a\x7f\xc2\x80\x9c";
    let recordings = recordings();
    let mut rng = Rng(0x0129_0073_C0FF_EE01);
    let mut filters = Filters::default();

    for round in 0..64 {
        let mut noise = vec![0u8; 4096];
        rng.fill(&mut noise);
        assert_agree(&format!("uniform-{round}"), &noise, &mut filters);

        let mut biased = vec![0u8; 4096];
        rng.fill(&mut biased);
        for byte in &mut biased {
            if usize::from(*byte) % 4 != 0 {
                *byte = BIAS[usize::from(*byte) % BIAS.len()];
            }
        }
        assert_agree(&format!("biased-{round}"), &biased, &mut filters);

        // Splice two recordings at random offsets.
        let (left_name, left) = &recordings[rng.below(recordings.len())];
        let (right_name, right) = &recordings[rng.below(recordings.len())];
        let cut = rng.below(left.len().max(1));
        let mut spliced = left[..cut].to_vec();
        spliced.extend_from_slice(&right[rng.below(right.len().max(1))..]);
        assert_agree(
            &format!("splice-{left_name}-{right_name}"),
            &spliced,
            &mut filters,
        );

        // Flip a handful of bits in one recording.
        let (name, source) = &recordings[rng.below(recordings.len())];
        let mut mutated = source.clone();
        for _ in 0..32 {
            if mutated.is_empty() {
                break;
            }
            let at = rng.below(mutated.len());
            mutated[at] ^= 1 << rng.below(8);
        }
        assert_agree(&format!("flip-{name}"), &mutated, &mut filters);
    }

    // The generated streams are where the declared differences actually fire;
    // the count is reported so a change in the filters is visible in review.
    println!("accepted declared differences: {filters:?}");
}

/// The two OSC filters are the only ones a realistic stream never reaches, so
/// they are driven deliberately: a filter nobody exercises is a hole that would
/// hide a regression in the comparison itself.
#[test]
fn declared_osc_differences_are_exercised() {
    let mut filters = Filters::default();

    // P4: past the inline cap the new parser truncates where the reference
    // grows without bound.
    let mut long = b"\x1b]1337;".to_vec();
    long.extend(std::iter::repeat_n(b'x', 8192));
    long.push(0x07);
    assert_agree("truncated-osc", &long, &mut filters);
    assert_eq!(filters.truncated_osc, 1);

    // P9: past the sixteenth parameter the bytes join into the last, with the
    // separators the reference drops.
    let mut many = b"\x1b]8".to_vec();
    for _ in 0..24 {
        many.extend_from_slice(b";p");
    }
    many.push(0x07);
    assert_agree("joined-osc-params", &many, &mut filters);
    assert_eq!(filters.joined_osc_params, 1);
}

#[test]
fn chunk_splitting_is_invariant() {
    for (name, bytes) in recordings() {
        let whole = actions_new(&bytes);
        for chunk in [1usize, 7, 64 * 1024] {
            let mut engine = NewEngine::default();
            let mut parser = Parser::new();
            for piece in bytes.chunks(chunk) {
                parser.advance(&mut engine, piece);
            }
            assert_eq!(
                engine.actions, whole,
                "{name}: chunk size {chunk} changed the actions"
            );
        }
    }
}

#[test]
fn oracle_precondition_patches_do_not_touch_the_state_machine() {
    let patches = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/patches/vte");
    let mut checked = 0;
    for entry in std::fs::read_dir(&patches).expect("the vte patch series must be present") {
        let path = entry.expect("a readable directory entry").path();
        if path.extension().is_none_or(|ext| ext != "patch") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a readable patch");
        for line in text.lines().filter(|line| line.starts_with("+++")) {
            assert!(
                line.ends_with("src/ansi.rs"),
                "{}: patches the state machine, so the differential oracle is no longer \
                 upstream: {line}",
                path.display()
            );
            checked += 1;
        }
        // A patch that touched no file at all would pass vacuously.
        assert!(text.contains("+++"), "{}: no file header", path.display());
    }
    assert!(
        checked >= 2,
        "expected the two vendored vte patches, found {checked} file headers"
    );
}
