//! Unit tests for the suggestion engine.

use super::*;
use crate::catalog::Catalog;

const RAW: &[(&str, &str, &str, &str)] = &[
    (
        "dir",
        "external",
        "cmd",
        r#"{ "schema": 1, "name": "dir", "options": ["/A", "/B", "/Q", "/S"] }"#,
    ),
    (
        "date",
        "external",
        "cmd",
        r#"{ "schema": 1, "name": "date" }"#,
    ),
    (
        "del",
        "external",
        "cmd",
        r#"{ "schema": 1, "name": "del" }"#,
    ),
    (
        "ls",
        "external",
        "coreutils",
        r#"{ "schema": 1, "name": "ls", "options": ["-a", "-l", "--all", "--color"] }"#,
    ),
    (
        "git",
        "manual",
        "common",
        r#"{ "schema": 1, "name": "git", "options": ["-C", "--version"],
                "subcommands": [
                  { "name": "commit", "options": ["-m", "--amend", "--no-verify"] },
                  { "name": "checkout", "options": ["-b"] },
                  { "name": "remote", "options": ["-v"],
                    "subcommands": [ { "name": "add", "options": ["-t", "-f", "--tags"] } ] }
                ] }"#,
    ),
];

fn engine() -> Engine {
    Engine::with_catalog(Catalog::from_raw(RAW))
}

fn ctx<'a>(family: ShellFamily, line: &'a str) -> CompletionContext<'a> {
    CompletionContext {
        family,
        line,
        cursor_col: line.len(),
        now_ms: 1_000_000,
    }
}

#[test]
fn command_context_lists_matching_commands() {
    let e = engine();
    let h = CompletionHistory::new(10);
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Cmd, "d"),
        &CompletionParams::default(),
    );
    let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
    assert!(texts.contains(&"dir"));
    assert!(texts.contains(&"date"));
    assert!(texts.contains(&"del"));
    assert!(s.iter().all(|x| x.kind == SuggestionKind::Command));
}

#[test]
fn bash_prompt_does_not_show_windows_commands() {
    let e = engine();
    let h = CompletionHistory::new(10);
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Unix, "d"),
        &CompletionParams::default(),
    );
    let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
    assert!(!texts.contains(&"dir"));
    assert!(!texts.contains(&"date"));
}

#[test]
fn option_context_lists_options() {
    let e = engine();
    let h = CompletionHistory::new(10);
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Cmd, "dir /"),
        &CompletionParams::default(),
    );
    let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
    assert!(texts.contains(&"/A"));
    assert!(texts.contains(&"/Q"));
    assert!(s.iter().all(|x| x.kind == SuggestionKind::Option));
}

#[test]
fn option_context_narrows_on_prefix() {
    let e = engine();
    let h = CompletionHistory::new(10);
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Cmd, "dir /Q"),
        &CompletionParams::default(),
    );
    let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
    assert_eq!(texts, vec!["/Q"]);
}

#[test]
fn unix_long_option_narrows() {
    let e = engine();
    let h = CompletionHistory::new(10);
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Unix, "ls --"),
        &CompletionParams::default(),
    );
    let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
    assert!(texts.contains(&"--all"));
    assert!(texts.contains(&"--color"));
    assert!(!texts.contains(&"-a")); // `--` narrows to long options
}

#[test]
fn subcommand_context_lists_children() {
    let e = engine();
    let h = CompletionHistory::new(10);
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Unix, "git "),
        &CompletionParams::default(),
    );
    let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
    assert!(texts.contains(&"commit"));
    assert!(texts.contains(&"checkout"));
    assert!(texts.contains(&"remote"));
}

#[test]
fn nested_subcommand_context() {
    let e = engine();
    let h = CompletionHistory::new(10);
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Unix, "git remote "),
        &CompletionParams::default(),
    );
    let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
    assert!(texts.contains(&"add"));
}

#[test]
fn subcommand_option_context_uses_active_node() {
    let e = engine();
    let h = CompletionHistory::new(10);
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Unix, "git commit --"),
        &CompletionParams::default(),
    );
    let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
    assert!(texts.contains(&"--amend"));
    assert!(texts.contains(&"--no-verify"));
    // git's global --version should not outrank; with inheritance it may
    // appear but commit's own options must be present.
}

#[test]
fn nested_option_context() {
    let e = engine();
    let h = CompletionHistory::new(10);
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Unix, "git remote add -"),
        &CompletionParams::default(),
    );
    let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
    assert!(texts.contains(&"-t"));
    assert!(texts.contains(&"-f"));
}

#[test]
fn ancestor_options_inherited_but_ranked_lower() {
    let e = engine();
    let h = CompletionHistory::new(10);
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Unix, "git remote add -"),
        &CompletionParams::default(),
    );
    // With inheritance, remote's -v appears, ranked below add's own options.
    let pos_own = s.iter().position(|x| x.text == "-t");
    let pos_anc = s.iter().position(|x| x.text == "-v");
    assert!(pos_own.is_some());
    if let Some(anc) = pos_anc {
        assert!(
            pos_own.unwrap() < anc,
            "active-node option must rank above ancestor"
        );
    }
}

#[test]
fn unknown_subcommand_falls_back_to_command_options() {
    let e = engine();
    let h = CompletionHistory::new(10);
    // `git frobnicate -` → walk stops at git; offers git's options.
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Unix, "git frobnicate -"),
        &CompletionParams::default(),
    );
    let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
    assert!(texts.contains(&"-C"));
}

#[test]
fn history_beats_catalog_and_dedups_with_h_tag() {
    let e = engine();
    let mut h = CompletionHistory::new(10);
    // Use `dir` a lot this session.
    for t in [1000u64, 2000, 3000] {
        h.record(ShellFamily::Cmd, "dir", t);
    }
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Cmd, "d"),
        &CompletionParams::default(),
    );
    // `dir` appears once, tagged History, and ranks first.
    let dir_entries: Vec<_> = s.iter().filter(|x| x.text == "dir").collect();
    assert_eq!(dir_entries.len(), 1, "dir must be deduped");
    assert_eq!(dir_entries[0].kind, SuggestionKind::History);
    assert_eq!(s[0].text, "dir", "frecent history ranks first");
}

#[test]
fn prefix_beats_fuzzy() {
    let e = engine();
    let h = CompletionHistory::new(10);
    // token "dt" fuzzy-matches "date"; "d" prefix beats it — use "da".
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Cmd, "da"),
        &CompletionParams::default(),
    );
    assert_eq!(s[0].text, "date");
}

#[test]
fn case_sensitivity_per_family() {
    let e = engine();
    let h = CompletionHistory::new(10);
    // Cmd is case-insensitive: "DI" → dir.
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Cmd, "DI"),
        &CompletionParams::default(),
    );
    assert!(s.iter().any(|x| x.text == "dir"));
    // Unix is case-sensitive: "LS" must not match "ls".
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Unix, "LS"),
        &CompletionParams::default(),
    );
    assert!(!s.iter().any(|x| x.text == "ls"));
}

#[test]
fn remainder_is_append_only() {
    let s = Suggestion {
        text: "dir".into(),
        kind: SuggestionKind::Command,
        description: None,
        match_len: 2,
        score: 0.0,
        replace_from: 0,
    };
    assert_eq!(s.remainder("di"), "r");
    assert_eq!(s.remainder("dir"), "");
    // Non-prefix → empty (fuzzy accept gated off).
    assert_eq!(s.remainder("xyz"), "");
}

#[test]
fn min_prefix_len_suppresses_short_tokens() {
    let e = engine();
    let h = CompletionHistory::new(10);
    let mut cfg = CompletionParams::default();
    cfg.min_prefix_len = 2;
    let s = e.suggest(&h, &ctx(ShellFamily::Cmd, "d"), &cfg);
    assert!(s.is_empty());
}

#[test]
fn recorded_secret_never_suggested() {
    let e = engine();
    let mut h = CompletionHistory::new(10);
    // Even if a raw secret is injected into the ring (bypassing capture),
    // the suggestion-time guard drops it.
    h.record(
        ShellFamily::Unix,
        "deploy --token ghp_0123456789abcdefghij",
        1000,
    );
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Unix, "deploy "),
        &CompletionParams::default(),
    );
    assert!(s.iter().all(|x| !x.text.contains("ghp_")));
}

#[test]
fn sources_toggle_off_history() {
    let e = engine();
    let mut h = CompletionHistory::new(10);
    h.record(ShellFamily::Cmd, "docker ps", 1000);
    let mut cfg = CompletionParams::default();
    cfg.sources.memory = false;
    let s = e.suggest(&h, &ctx(ShellFamily::Cmd, "d"), &cfg);
    assert!(!s.iter().any(|x| x.text == "docker"));
}

#[test]
fn embedded_catalog_suggests_cmd_commands_for_d() {
    // Uses the REAL compile-time embedded catalogs (not the inline fixture),
    // mirroring what the app does via `Engine::from_embedded()`.
    let e = Engine::from_embedded();
    let h = CompletionHistory::new(10);
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Cmd, "d"),
        &CompletionParams::default(),
    );
    let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
    assert!(
        !s.is_empty(),
        "embedded catalog returned no suggestions for 'd'"
    );
    assert!(
        texts.contains(&"dir") || texts.contains(&"date") || texts.contains(&"del"),
        "expected dir/date/del among embedded cmd suggestions, got {texts:?}"
    );
}

#[test]
fn embedded_catalog_suggests_unix_commands_for_l() {
    let e = Engine::from_embedded();
    let h = CompletionHistory::new(10);
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Unix, "l"),
        &CompletionParams::default(),
    );
    assert!(
        !s.is_empty(),
        "embedded catalog returned no unix suggestions for 'l'"
    );
    assert!(
        s.iter().any(|x| x.text == "ls"),
        "expected ls among unix suggestions"
    );
}

#[test]
fn option_description_flows_from_manual_catalog() {
    // `git checkout -b` should surface the `-b` option carrying its
    // `new-branch` argument hint (docs: object flag form with description).
    let e = Engine::from_embedded();
    let h = CompletionHistory::new(10);
    let s = e.suggest(
        &h,
        &ctx(ShellFamily::Unix, "git checkout -b"),
        &CompletionParams::default(),
    );
    let b = s
        .iter()
        .find(|x| x.text == "-b")
        .expect("expected -b option for `git checkout`");
    assert_eq!(b.description.as_deref(), Some("new-branch"));
}
