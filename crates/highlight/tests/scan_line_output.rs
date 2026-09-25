//! The scanner's output, pinned: every class of a fixed corpus of ASCII, CJK,
//! Vietnamese, emoji and mixed lines, under every role and profile, must equal
//! what the scanner produced when the corpus was recorded (`US-0144`, which
//! moved the per-line buffers into a caller-owned scratch and gave ASCII lines
//! an identity byte-to-char map).
//!
//! `scan_line_output.golden` holds one hash per (base line, variant). To
//! re-record after an intended class change, run with
//! `ONETERM_RECORD_SCAN_GOLDEN=1` and say why in the commit.

use oneterm_highlight::{RowRole, RuleSet, ShellProfile, scan_line};

const GOLDEN: &str = "tests/scan_line_output.golden";

/// Lines that reach every matcher: keywords, IPs, MAC, dates, paths, numbers,
/// options, strings, permissions, operators, prompts, commands. The last four
/// are `highlight-bench`'s fixture heads and bodies.
const BASES: &[&str] = &[
    "error: something failed",
    "warning: unused variable `x` at src/main.rs:12:5",
    "ok 3 passed; 0 failed; finished in 0.42s",
    "ping 192.168.1.1 from 10.0.0.254 ttl=64 time=1.5ms",
    "Resolved example.com -> 2607:f8b0:4004:80a::200e",
    "link/ether 00:1a:2b:3c:4d:5e brd ff:ff:ff:ff:ff:ff",
    "2026-09-22 10:00:00 INFO server started on :8080",
    "drwxr-xr-x  2 user group 4096 Sep 22 10:00 /var/log/app.log",
    "-rwsr-sr-t 1 root root 0 Jan 1 C:\\Windows\\System32\\cmd.exe",
    "curl --url URL -x=type --verbose -H 'Accept: */*' \"quoted text\"",
    "value = 0x1F + -42 * 1.5e3 / 89% (a[b]) {c} | d && e || f; g",
    "$ ls -la ~/projects | grep rs",
    "user@host:~/src$ cargo build --release",
    "PS C:\\Program Files\\App> Get-ChildItem -Recurse",
    "C:\\Users\\John Doe\\ws>dir /s /b > out.txt",
    "[user@host ~]$ echo 'unclosed",
    "> quoted mail line with no prompt",
    "",
    "   ",
    r"C:\Users\John Doe\customer\acme\backend\gateway>git commit -m src/main.rs ",
    "the quick brown fox jumps over the lazy dog and keeps on running",
    "2026-09-22 10:00:00 error connect 192.168.1.10 failed warn retry /var/log/app.log 42 info ok 10.0.0.1 debug 7 /usr/bin/thing",
    "\u{65e5}\u{672c}\u{8a9e} error at /etc/hosts \u{4e2d}\u{6587}\u{6d4b}\u{8bd5} 2026-09-22 \u{4e2d}\u{6587} error 192.168.0.1",
    // A word char glued to a date, a month and a MAC: Unicode `\b` sees no boundary
    // there, an ASCII `(?-u:\b)` would (US-0144 kept the Unicode one).
    "\u{65e5}\u{672c}\u{8a9e}2026-09-22 caf\u{e9}Mon \u{e9}00:1a:2b:3c:4d:5e",
];

/// Each base line as written, and with non-ASCII text before, around and
/// inside it.
fn variants(base: &str) -> [String; 6] {
    let mid = base
        .char_indices()
        .nth(base.chars().count() / 2)
        .map_or(base.len(), |(i, _)| i);
    [
        base.to_owned(),
        format!("\u{65e5}\u{672c}\u{8a9e} {base}"),
        format!("Ti\u{1ebf}ng Vi\u{1ec7}t c\u{f3} d\u{1ea5}u: {base} l\u{1ed7}i"),
        format!("\u{1f680} {base} \u{2705} \u{1f469}\u{200d}\u{1f4bb}"),
        format!(
            "{} \u{4e2d}\u{6587} l\u{1ed7}i \u{1f389} {}",
            &base[..mid],
            &base[mid..]
        ),
        format!("/home/ng\u{1b0}\u{1edd}i/t\u{1ec7}p.rs {base} \u{e9}t\u{e9} 200\u{a0}ms"),
    ]
}

fn hash_case(line: &str) -> u64 {
    let rules = RuleSet::global();
    let sign = line
        .find(['$', '>', '#'])
        .map(|b| line[..b].chars().count() + 2);
    let roles = [
        (None, None),
        (Some(RowRole::Output), None),
        (Some(RowRole::Command), None),
        (Some(RowRole::Prompt), None),
        (Some(RowRole::Prompt), sign),
    ];
    // FNV-1a over every (profile, role) output in order.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |byte: u8| {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };
    for profile in [
        ShellProfile::Unix,
        ShellProfile::Cmd,
        ShellProfile::PowerShell,
        ShellProfile::Dumb,
    ] {
        for (role, input_at) in roles {
            let classes = scan_line(line, rules, &profile, role, input_at);
            assert_eq!(classes.len(), line.chars().count(), "{line:?}");
            eat(0xff);
            classes.into_iter().for_each(&mut eat);
        }
    }
    hash
}

fn render() -> String {
    let mut out = String::new();
    for (b, base) in BASES.iter().enumerate() {
        for (v, line) in variants(base).iter().enumerate() {
            out.push_str(&format!("{b} {v} {:016x}\n", hash_case(line)));
        }
    }
    out
}

#[test]
fn classes_match_the_recorded_corpus() {
    let actual = render();
    if std::env::var_os("ONETERM_RECORD_SCAN_GOLDEN").is_some() {
        std::fs::write(GOLDEN, &actual).expect("write the golden file");
        return;
    }
    let expected = std::fs::read_to_string(GOLDEN).expect("read the golden file");
    for (a, e) in actual.lines().zip(expected.lines()) {
        let mut ids = a.split(' ');
        let base: usize = ids.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let variant: usize = ids.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        assert_eq!(
            a,
            e,
            "classes changed for {:?}",
            variants(BASES[base])[variant]
        );
    }
    assert_eq!(actual.lines().count(), expected.lines().count());
}
