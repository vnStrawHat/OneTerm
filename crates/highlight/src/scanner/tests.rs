//! Tests for the line scanner.

use crate::class::Class;
use crate::profile::ShellProfile;
use crate::role::RowRole;
use crate::rules::RuleSet;
use crate::scanner::scan_line;

fn scan(line: &str, role: RowRole) -> Vec<Class> {
    let rules = RuleSet::global();
    let profile = ShellProfile::Unix;
    scan_line(line, rules, &profile, role)
        .into_iter()
        .map(Class::from_u8)
        .collect()
}

#[test]
fn error_keyword_tagged() {
    let c = scan("error: something failed", RowRole::Output);
    // "error" at chars 0..5 → Error
    assert_eq!(c[0], Class::Error);
    assert_eq!(c[4], Class::Error);
    // "failed" should also be tagged
    let failed_pos = "error: something failed".find("failed").unwrap();
    assert_eq!(c[failed_pos], Class::Error);
}

#[test]
fn word_boundary_prevents_false_match() {
    // "node" should NOT match "no" (word boundary).
    let c = scan("node server", RowRole::Output);
    assert_eq!(c[0], Class::Default); // 'n' in "node"
    assert_eq!(c[1], Class::Default); // 'o' in "node"
}

#[test]
fn ipv4_tagged() {
    let c = scan("ping 192.168.1.1", RowRole::Output);
    let ip_pos = "ping 192.168.1.1".find("192").unwrap();
    assert_eq!(c[ip_pos], Class::Ip);
    assert_eq!(c[ip_pos + 10], Class::Ip); // last '1'
}

#[test]
fn ipv6_compressed_full_address_tagged() {
    let line = "Resolved example.com -> 2607:f8b0:4004:80a::200e";
    let c = scan(line, RowRole::Output);
    let pos = line.find("2607").unwrap();
    let addr = "2607:f8b0:4004:80a::200e";
    for (i, _ch) in addr.chars().enumerate() {
        assert_eq!(c[pos + i], Class::Ip, "char {i} of {addr}");
    }
}

#[test]
fn ipv6_zone_id_not_tagged() {
    let line = "IPv6 link-local: fe80::1c2d:3e4f:5a6b%eth0";
    let c = scan(line, RowRole::Output);
    let pos = line.find("fe80").unwrap();
    // Address chars are Ip...
    let addr = "fe80::1c2d:3e4f:5a6b";
    for (i, _ch) in addr.chars().enumerate() {
        assert_eq!(c[pos + i], Class::Ip, "char {i} of {addr}");
    }
    // ...but %eth0 is NOT Ip.
    let zone_start = pos + addr.len();
    assert_ne!(c[zone_start], Class::Ip); // '%'
    assert_ne!(c[zone_start + 1], Class::Ip); // 'e'
}

#[test]
fn ipv6_leading_double_colon_tagged() {
    let line = "localhost ::1";
    let c = scan(line, RowRole::Output);
    let pos = line.find("::1").unwrap();
    assert_eq!(c[pos], Class::Ip); // ':'
    assert_eq!(c[pos + 1], Class::Ip); // ':'
    assert_eq!(c[pos + 2], Class::Ip); // '1'
}

#[test]
fn ipv4_not_partial() {
    // 1.2.3.4.5 should not be tagged as IPv4 (too many octets).
    let c = scan("1.2.3.4.5", RowRole::Output);
    // The first 4 octets should not be tagged because the next char is a digit/dot.
    for cls in &c {
        assert_ne!(*cls, Class::Ip);
    }
}

#[test]
fn path_tagged() {
    let c = scan("cat /usr/bin/env", RowRole::Output);
    let path_pos = "cat /usr/bin/env".find("/usr").unwrap();
    assert_eq!(c[path_pos], Class::Path);
}

#[test]
fn path_relative_multi_segment_tagged() {
    // `src/views/terminal/mod.rs` should highlight `src` too.
    let line = "Found 3 matches in src/views/terminal/mod.rs:128";
    let c = scan(line, RowRole::Output);
    let p = line.find("src").unwrap();
    let end = p + "src/views/terminal/mod.rs".len();
    for i in p..end {
        assert_eq!(c[i], Class::Path, "char {i} of '{line}'");
    }
}

#[test]
fn path_relative_with_extension_tagged() {
    // `src/main.rs` has one separator but a file extension → path.
    let line = "edit src/main.rs now";
    let c = scan(line, RowRole::Output);
    let p = line.find("src").unwrap();
    let end = p + "src/main.rs".len();
    for i in p..end {
        assert_eq!(c[i], Class::Path, "char {i} of '{line}'");
    }
}

#[test]
fn slash_word_not_path() {
    // `link/ether`, `bytes/sec`, `3/5` must NOT be tagged as Path.
    for line in [
        "eth0: link/ether aa:bb:cc:dd:ee:ff",
        "Throughput: 1_048_576 bytes/sec, latency: 2.5ms",
        "DEBUG: retrying connection (attempt 3/5)",
    ] {
        let c = scan(line, RowRole::Output);
        for (i, cls) in c.iter().enumerate() {
            assert_ne!(*cls, Class::Path, "char {i} of '{line}'");
        }
    }
}

#[test]
fn number_tagged() {
    let c = scan("port 8080", RowRole::Output);
    let num_pos = "port 8080".find("8080").unwrap();
    assert_eq!(c[num_pos], Class::Number);
}

#[test]
fn number_with_percent_tagged() {
    let c = scan("usage 89% detected", RowRole::Output);
    let pos = "usage 89% detected".find("89").unwrap();
    assert_eq!(c[pos], Class::Number); // '8'
    assert_eq!(c[pos + 1], Class::Number); // '9'
    assert_eq!(c[pos + 2], Class::Number); // '%'
}

#[test]
fn hex_number_tagged() {
    let c = scan("addr 0x1F", RowRole::Output);
    let hex_pos = "addr 0x1F".find("0x1F").unwrap();
    assert_eq!(c[hex_pos], Class::Number);
}

#[test]
fn string_tagged() {
    let c = scan(r#"echo "hello world""#, RowRole::Output);
    let q_pos = r#"echo "hello world""#.find('"').unwrap();
    assert_eq!(c[q_pos], Class::String);
    assert_eq!(c[q_pos + 5], Class::String); // inside the string
}

#[test]
fn permission_block_tagged() {
    //       - r w - r - - r - -
    // pos: 0 1 2 3 4 5 6 7 8 9
    let c = scan("-rw-r--r--  2 user group", RowRole::Output);
    // Position 0 = type char → PermType.
    assert_eq!(c[0], Class::PermType);
    // r at positions 1, 4, 7 → PermRead.
    assert_eq!(c[1], Class::PermRead);
    assert_eq!(c[4], Class::PermRead);
    assert_eq!(c[7], Class::PermRead);
    // w at position 2 → PermWrite.
    assert_eq!(c[2], Class::PermWrite);
    // - at positions 3, 5, 6, 8, 9 → PermNone.
    assert_eq!(c[3], Class::PermNone);
    assert_eq!(c[5], Class::PermNone);
    assert_eq!(c[6], Class::PermNone);
    assert_eq!(c[8], Class::PermNone);
    assert_eq!(c[9], Class::PermNone);
}

#[test]
fn permission_block_special_bits() {
    let c = scan("drwsr-sr-t  2 user group", RowRole::Output);
    assert_eq!(c[0], Class::PermType); // d
    assert_eq!(c[1], Class::PermRead); // r
    assert_eq!(c[2], Class::PermWrite); // w
    assert_eq!(c[3], Class::PermSpecial); // s
    assert_eq!(c[6], Class::PermSpecial); // s
    assert_eq!(c[9], Class::PermSpecial); // t
}

#[test]
fn operator_and_bracket_tagged() {
    let c = scan("(a | b)", RowRole::Output);
    assert_eq!(c[0], Class::Bracket); // '('
    assert_eq!(c[6], Class::Bracket); // ')'
    // '|' is an operator
    let pipe_pos = "(a | b)".find('|').unwrap();
    assert_eq!(c[pipe_pos], Class::Operator);
}

#[test]
fn prompt_line_tagged() {
    let c = scan("$ ls -la", RowRole::Output);
    assert_eq!(c[0], Class::PromptSign); // '$'
    // "ls" is the command
    let ls_pos = "$ ls -la".find("ls").unwrap();
    assert_eq!(c[ls_pos], Class::Command);
    // "-la" is an option
    let opt_pos = "$ ls -la".find("-la").unwrap();
    assert_eq!(c[opt_pos], Class::Option);
}

#[test]
fn command_role_skips_prompt() {
    // RowRole::Command → first token = Command directly.
    let c = scan("git status", RowRole::Command);
    assert_eq!(c[0], Class::Command); // 'g' in "git"
    assert_eq!(c[2], Class::Command); // 't' in "git"
}

#[test]
fn command_separator_resets() {
    let c = scan("echo hi ; cat file", RowRole::Command);
    let cat_pos = "echo hi ; cat file".find("cat").unwrap();
    assert_eq!(c[cat_pos], Class::Command); // "cat" is a new command after ';'
}

#[test]
fn datetime_tagged() {
    let c = scan("2026-06-23 log entry", RowRole::Output);
    assert_eq!(c[0], Class::DateTime);
}

#[test]
fn datetime_full_patterns_tagged() {
    // Reference-style datetimes should be highlighted as one contiguous DateTime span.
    let cases: [(&str, std::ops::Range<usize>); 5] = [
        ("2024-01-15 09:30:45 INFO  Server started", 0..19),
        ("12/25/2023 11:59 PM - session ended", 0..19),
        ("Mon Jul 13 06:38:56 AM UTC 2026", 0..31),
        ("Last login: Wed Oct 25 10:15:30 UTC 2023", 12..40),
        ("Build finished at 2024-03-01T14:22:08.123Z", 18..42),
    ];
    for (line, span) in cases {
        let c = scan(line, RowRole::Output);
        for i in span.clone() {
            assert_eq!(c[i], Class::DateTime, "char {i} of '{line}'");
        }
        // Char immediately after the span is not DateTime.
        if span.end < c.len() {
            assert_ne!(
                c[span.end],
                Class::DateTime,
                "trailing char {} of '{line}'",
                span.end
            );
        }
    }
}

#[test]
fn clock_time_is_datetime_not_ip() {
    let line = "2024-01-15 09:30:45";
    let c = scan(line, RowRole::Output);
    let time_pos = line.find("09:30").unwrap();
    assert_eq!(c[time_pos], Class::DateTime);
    assert_eq!(c[time_pos + 1], Class::DateTime);
    // Make sure it was not mis-classified as Ip.
    assert_ne!(c[time_pos], Class::Ip);
}

#[test]
fn mac_tagged() {
    let c = scan("mac aa:bb:cc:dd:ee:ff", RowRole::Output);
    let mac_pos = "mac aa:bb:cc:dd:ee:ff".find("aa:").unwrap();
    assert_eq!(c[mac_pos], Class::Mac);
}

#[test]
fn mac_with_digits_tagged_full() {
    // A MAC that contains a digit-only triplet must not be partially
    // stolen by the DateTime time regex.
    let line = "mac aa:bb:cc:11:22:33 ok";
    let c = scan(line, RowRole::Output);
    let mac_start = line.find("aa:").unwrap();
    let mac_end = mac_start + "aa:bb:cc:11:22:33".len();
    for i in mac_start..mac_end {
        assert_eq!(c[i], Class::Mac, "char {i} of '{line}'");
    }
}

#[test]
fn ipv6_not_inside_path() {
    // Compressed IPv6-like short sequences inside paths/identifiers must not be
    // highlighted as Ip.
    let line = "warning: unused import: std::collections::HashMap";
    let c = scan(line, RowRole::Output);
    for (i, cls) in c.iter().enumerate() {
        assert_ne!(*cls, Class::Ip, "char {i} of '{line}'");
    }
}

#[test]
fn empty_line() {
    let c = scan("", RowRole::Output);
    assert!(c.is_empty());
}

#[test]
fn priority_string_over_keyword() {
    // "error" inside a string should be String, not Error.
    let c = scan(r#""error""#, RowRole::Output);
    for cls in &c {
        assert_eq!(*cls, Class::String);
    }
}
fn scan_with_profile(line: &str, role: RowRole, profile: ShellProfile) -> Vec<Class> {
    let rules = RuleSet::global();
    scan_line(line, rules, &profile, role)
        .into_iter()
        .map(Class::from_u8)
        .collect()
}

#[test]
fn prompt_sign_only_is_tagged() {
    // Only the sign char itself should be PromptSign, not the preceding path.
    let line = "user@host:~/dir$ ls";
    let c = scan(line, RowRole::Output);
    let dollar = line.find('$').unwrap();
    for i in 0..dollar {
        assert_ne!(c[i], Class::PromptSign, "char {i} '{line}'");
    }
    assert_eq!(c[dollar], Class::PromptSign);
}

#[test]
fn unix_prompt_path_before_sign_is_highlighted() {
    // The path `~/dir` before `$` should be tagged as Path.
    let line = "user@host:~/dir$ ls";
    let c = scan(line, RowRole::Output);
    let tilde = line.find('~').unwrap();
    let dollar = line.find('$').unwrap();
    assert_eq!(c[tilde], Class::Path, "'~' should be Path");
    assert_eq!(
        c[dollar - 1],
        Class::Path,
        "last path char before '$' should be Path"
    );
}

#[test]
fn windows_prompt_sign_only_is_tagged() {
    let line = r"C:\Users\foo> dir";
    let c = scan_with_profile(line, RowRole::Output, ShellProfile::Cmd);
    let gt = line.find('>').unwrap();
    for i in 0..gt {
        assert_ne!(c[i], Class::PromptSign, "char {i} of '{line}'");
    }
    assert_eq!(c[gt], Class::PromptSign);
}

#[test]
fn windows_prompt_path_before_sign_is_highlighted() {
    // The path before the `>` in a cmd prompt should be tagged as `Path`,
    // not left as `Default` (white). This was the root cause of the bug where
    // the path was white initially but became highlighted when the user typed.
    let line = r"C:\Users\foo> dir";
    let c = scan_with_profile(line, RowRole::Output, ShellProfile::Cmd);
    let gt = line.find('>').unwrap();
    // The path `C:\Users\foo` should be tagged as Path.
    assert_eq!(c[0], Class::Path, "drive letter 'C' should be Path");
    assert_eq!(c[1], Class::Path, "colon ':' should be Path");
    assert_eq!(
        c[gt - 1],
        Class::Path,
        "last path char before '>' should be Path"
    );
}

#[test]
fn windows_prompt_path_highlighted_without_trailing_space() {
    // When the user types right after `>` (no trailing space), the prompt regex
    // should still match and the path should be highlighted as Path, with `>`
    // tagged as PromptSign (not Operator).
    let line = r"D:\TrungKFC-Research\Rust\myTerm2>dir";
    let c = scan_with_profile(line, RowRole::Output, ShellProfile::Cmd);
    let gt = line.find('>').unwrap();
    // Path before `>` should be tagged as Path.
    assert_eq!(c[0], Class::Path, "drive letter should be Path");
    assert_eq!(
        c[gt - 1],
        Class::Path,
        "last path char before '>' should be Path"
    );
    // `>` should be PromptSign, not Operator.
    assert_eq!(c[gt], Class::PromptSign);
    // `dir` should be Command.
    let dir_pos = line.find("dir").unwrap();
    assert_eq!(c[dir_pos], Class::Command);
}

#[test]
fn windows_prompt_path_highlighted_at_end_of_line() {
    // `D:\path>` at end of line (no trailing space, no user input) — the prompt
    // regex should still match so the path is highlighted and `>` is PromptSign.
    let line = r"D:\TrungKFC-Research\Rust\myTerm2>";
    let c = scan_with_profile(line, RowRole::Output, ShellProfile::Cmd);
    let gt = line.find('>').unwrap();
    assert_eq!(c[0], Class::Path, "drive letter should be Path");
    assert_eq!(
        c[gt - 1],
        Class::Path,
        "last path char before '>' should be Path"
    );
    assert_eq!(c[gt], Class::PromptSign);
}

#[test]
fn windows_prompt_path_highlighted_with_trailing_space() {
    // `D:\path> ` (with trailing space — initial state when no user input).
    // The blank cell after `>` is converted to space, so the prompt regex matches.
    let line = r"D:\TrungKFC-Research\Rust\myTerm2> ";
    let c = scan_with_profile(line, RowRole::Output, ShellProfile::Cmd);
    let gt = line.find('>').unwrap();
    assert_eq!(c[0], Class::Path, "drive letter should be Path");
    assert_eq!(
        c[gt - 1],
        Class::Path,
        "last path char before '>' should be Path"
    );
    assert_eq!(c[gt], Class::PromptSign);
}

#[test]
fn windows_prompt_path_highlighted_with_input_after_space() {
    // `D:\path> x` (with trailing space + user input).
    let line = r"D:\TrungKFC-Research\Rust\myTerm2> x";
    let c = scan_with_profile(line, RowRole::Output, ShellProfile::Cmd);
    let gt = line.find('>').unwrap();
    assert_eq!(c[0], Class::Path, "drive letter should be Path");
    assert_eq!(
        c[gt - 1],
        Class::Path,
        "last path char before '>' should be Path"
    );
    assert_eq!(c[gt], Class::PromptSign);
    let x_pos = line.find('x').unwrap();
    assert_eq!(c[x_pos], Class::Command);
}

#[test]
fn output_option_long_and_short() {
    // --help output: both long and short options should be Option.
    let line = " -d, --data <data>  HTTP POST data";
    let c = scan(line, RowRole::Output);
    let d_pos = line.find(" -d").unwrap() + 1;
    let data_pos = line.find("--data").unwrap();
    assert_eq!(c[d_pos], Class::Option);
    assert_eq!(c[d_pos + 1], Class::Option);
    assert_eq!(c[data_pos], Class::Option);
    assert_eq!(c[data_pos + 5], Class::Option);
}

#[test]
fn option_not_tagged_inside_filename() {
    // Filenames with hyphens must not have embedded segments highlighted as options.
    let line = "drwxr-xr-x 2 root root 4.0K May 21 2025 container-diff-linux-amd64";
    let c = scan(line, RowRole::Output);
    for (i, cls) in c.iter().enumerate() {
        assert_ne!(*cls, Class::Option, "char {i} of '{line}'");
    }
}

#[test]
fn number_not_tagged_inside_identifier() {
    // `math-2`, `file_v2`, `step-1` must not highlight the trailing digit.
    for line in ["result: math-2 ok", "read file_v2 now", "stage step-1 done"] {
        let c = scan(line, RowRole::Output);
        for (i, cls) in c.iter().enumerate() {
            assert_ne!(*cls, Class::Number, "char {i} of '{line}'");
        }
    }
}

#[test]
fn standalone_number_still_tagged() {
    let line = "error -1 happened at 89%";
    let c = scan(line, RowRole::Output);
    let minus_one = line.find("-1").unwrap();
    let percent = line.find("89").unwrap();
    assert_eq!(c[minus_one], Class::Number);
    assert_eq!(c[minus_one + 1], Class::Number);
    assert_eq!(c[percent], Class::Number);
    assert_eq!(c[percent + 1], Class::Number);
    assert_eq!(c[percent + 2], Class::Number); // %
}

#[test]
fn cross_shell_prompt_unix_inside_cmd() {
    // Running `wsl` inside cmd.exe: prompt becomes Unix ($) but the terminal
    // profile may still be Cmd. The universal fallback should detect it.
    let line = "user@host:~$ echo hi";
    let c = scan_with_profile(line, RowRole::Output, ShellProfile::Cmd);
    let dollar = line.find('$').unwrap();
    assert_eq!(c[dollar], Class::PromptSign);
    // echo should be command
    let echo_pos = line.find("echo").unwrap();
    assert_eq!(c[echo_pos], Class::Command);
}
