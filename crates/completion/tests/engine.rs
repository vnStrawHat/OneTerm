//! Suggestions over a small catalog (specs/completion, "A suggestion").

use oneterm_completion::{Catalog, Config, Engine, Family, Group, Kind, Source, Sources, Suggestion};

const FIXTURE: &[(Source, Group, &str)] = &[
    (
        Source::External,
        Group::Windows,
        r#"{ "schema": 1, "name": "dir", "options": ["/A", "/B", "/Q", "/S"] }"#,
    ),
    (Source::External, Group::Windows, r#"{ "schema": 1, "name": "date" }"#),
    (Source::External, Group::Windows, r#"{ "schema": 1, "name": "del" }"#),
    (Source::External, Group::Unix, r#"{ "name": "ls", "options": ["-a", "-A", "-l", "--all", "--color"] }"#),
    (
        Source::Manual,
        Group::Common,
        r#"{ "schema": 1, "name": "git", "options": ["-C", "--version"],
             "subcommands": [
               { "name": "commit", "options": ["-m", "--amend", "--no-verify"] },
               { "name": "checkout", "options": [{ "flag": "-b", "description": "new-branch" }] },
               { "name": "remote", "options": ["-v"],
                 "subcommands": [ { "name": "add", "options": ["-t", "-f", "--tags"] } ] }
             ] }"#,
    ),
];

fn engine_with(config: Config) -> Engine {
    let mut catalog = Catalog::new();
    for (source, group, json) in FIXTURE {
        catalog.add(json, *source, *group).unwrap();
    }
    Engine::with_catalog(config, catalog)
}

fn engine() -> Engine {
    engine_with(Config::default())
}

fn at_end(engine: &Engine, line: &str, family: Family) -> Vec<Suggestion> {
    engine.suggest(line, line.len(), family)
}

fn texts(suggestions: &[Suggestion]) -> Vec<&str> {
    suggestions.iter().map(|s| s.text.as_str()).collect()
}

#[test]
fn the_first_token_is_completed_from_the_commands_of_the_family() {
    let e = engine();
    let cmd = at_end(&e, "d", Family::Cmd);
    assert!(["dir", "date", "del"].iter().all(|name| texts(&cmd).contains(name)));
    assert!(cmd.iter().all(|s| s.kind == Kind::Command && s.range == (0..1)));
    assert!(at_end(&e, "d", Family::Unix).iter().all(|s| s.text != "dir" && s.text != "date"));
    assert!(texts(&at_end(&e, "l", Family::Unix)).contains(&"ls"));
}

#[test]
fn options_come_from_the_command_and_narrow_as_they_are_typed() {
    let e = engine();
    let all = at_end(&e, "dir /", Family::Cmd);
    assert!(texts(&all).contains(&"/A") && texts(&all).contains(&"/Q"));
    assert!(all.iter().all(|s| s.kind == Kind::Option && s.range == (4..5)));
    assert_eq!(texts(&at_end(&e, "dir /q", Family::Cmd)), ["/Q"]);
    let long = texts(&at_end(&e, "ls --", Family::Unix)).join(" ");
    assert_eq!(long, "--all --color");
}

#[test]
fn options_keep_the_case_of_unix() {
    let e = engine();
    let short = at_end(&e, "ls -a", Family::Unix);
    assert_eq!(short[0].text, "-a");
    assert!(short.iter().all(|s| s.text != "-A" || !s.prefix));
}

#[test]
fn subcommands_follow_the_tree() {
    let e = engine();
    let git = at_end(&e, "git ", Family::Unix);
    assert_eq!(texts(&git), ["commit", "remote", "checkout"]);
    assert!(git.iter().all(|s| s.kind == Kind::Subcommand && s.source == Source::Manual));
    assert_eq!(texts(&at_end(&e, "git remote ", Family::Unix)), ["add"]);
    let commit = at_end(&e, "git commit --", Family::Unix);
    let commit = texts(&commit);
    assert!(commit.contains(&"--amend") && commit.contains(&"--no-verify"));
    let add = at_end(&e, "git remote -v add -", Family::Unix);
    let add = texts(&add);
    assert!(add.contains(&"-t") && add.contains(&"-f"));
}

#[test]
fn ancestor_options_rank_below_the_nodes_own() {
    let e = engine();
    let add = at_end(&e, "git remote add -", Family::Unix);
    let add = texts(&add);
    let (own, ancestor) = (add.iter().position(|t| *t == "-t"), add.iter().position(|t| *t == "-v"));
    assert!(own.unwrap() < ancestor.expect("remote's -v is inherited"));
    let alone = engine_with(Config { inherit_ancestor_options: false, ..Config::default() });
    assert!(!texts(&at_end(&alone, "git remote add -", Family::Unix)).contains(&"-v"));
}

#[test]
fn an_unknown_subcommand_stops_the_walk() {
    let e = engine();
    assert!(texts(&at_end(&e, "git frobnicate -", Family::Unix)).contains(&"-C"));
}

#[test]
fn an_option_carries_its_hint() {
    let e = engine();
    let b = at_end(&e, "git checkout -", Family::Unix);
    assert_eq!(b[0].detail.as_deref(), Some("new-branch"));
}

#[test]
fn the_history_ranks_first_and_is_kept_once() {
    let mut e = engine();
    for _ in 0..3 {
        e.record("dir", Family::Cmd);
    }
    let list = at_end(&e, "d", Family::Cmd);
    assert_eq!(list[0].text, "dir");
    assert_eq!(list[0].source, Source::History);
    assert_eq!(list.iter().filter(|s| s.text == "dir").count(), 1);
}

#[test]
fn every_prefix_match_ranks_above_a_fuzzy_one() {
    let mut e = engine();
    e.record("dxe", Family::Cmd);
    let list = at_end(&e, "de", Family::Cmd);
    assert_eq!(texts(&list), ["del", "dxe", "date"]);
    assert!(list[0].prefix && !list[1].prefix && list[1].matched == 0);
    let strict = engine_with(Config { fuzzy: false, ..Config::default() });
    assert_eq!(texts(&at_end(&strict, "dt", Family::Cmd)), Vec::<&str>::new());
    assert_eq!(texts(&at_end(&e, "dt", Family::Cmd)), ["date"]);
}

#[test]
fn case_follows_the_family() {
    let e = engine();
    assert!(texts(&at_end(&e, "DI", Family::Cmd)).contains(&"dir"));
    assert!(!texts(&at_end(&e, "LS", Family::Unix)).contains(&"ls"));
}

#[test]
fn whole_lines_of_the_history_complete_what_is_typed() {
    let mut e = engine();
    e.record("deploy prod --force", Family::Unix);
    let list = at_end(&e, "  deploy pr", Family::Unix);
    let line = list.iter().find(|s| s.kind == Kind::Line).unwrap();
    assert_eq!((line.text.as_str(), line.range.clone()), ("deploy prod --force", 2..11));
    assert_eq!(line.edit("  deploy pr"), (0, "od --force"));
}

#[test]
fn a_command_no_catalog_knows_is_offered_the_options_it_was_run_with() {
    let mut e = engine();
    e.record("foo -日x --bar", Family::Cmd);
    let list = at_end(&e, "foo -x", Family::Cmd);
    assert!(texts(&list).contains(&"-日x"));
    assert_eq!(list.iter().find(|s| s.text == "-日x").unwrap().source, Source::History);
}

#[test]
fn a_short_command_token_waits_unless_the_trigger_asks() {
    let e = engine_with(Config { min_prefix_len: 2, ..Config::default() });
    assert!(at_end(&e, "d", Family::Cmd).is_empty());
    assert!(!e.suggest_now("d", 1, Family::Cmd).is_empty());
    assert!(at_end(&engine(), "", Family::Cmd).is_empty());
    assert!(texts(&engine().suggest_now("", 0, Family::Cmd)).contains(&"dir"));
}

#[test]
fn a_list_of_one_that_is_already_typed_is_empty() {
    let e = engine();
    assert!(at_end(&e, "dir /Q", Family::Cmd).is_empty());
    assert_eq!(texts(&at_end(&e, "dir /q", Family::Cmd)), ["/Q"]);
}

#[test]
fn the_list_is_bounded_by_the_rows_shown() {
    let e = engine_with(Config { max_visible_items: 1, ..Config::default() });
    assert_eq!(at_end(&e, "ls -", Family::Unix).len(), 4);
}

#[test]
fn sources_can_be_left_out() {
    let mut e = engine();
    e.record("docker ps", Family::Cmd);
    assert!(texts(&at_end(&e, "d", Family::Cmd)).contains(&"docker"));
    let without = |sources: Sources| Config { sources, ..Config::default() };
    e.set_config(without(Sources { memory: false, ..Sources::default() }));
    assert!(!texts(&at_end(&e, "d", Family::Cmd)).contains(&"docker"));
    e.set_config(without(Sources { external: false, ..Sources::default() }));
    assert!(at_end(&e, "dir /", Family::Cmd).is_empty());
    assert!(!at_end(&e, "git ", Family::Unix).is_empty());
}

#[test]
fn off_is_silent() {
    let mut e = engine_with(Config { enabled: false, ..Config::default() });
    e.record("dir", Family::Cmd);
    assert!(at_end(&e, "d", Family::Cmd).is_empty());
    assert_eq!(e.history(Family::Cmd).count(), 0);
}

#[test]
fn a_cursor_inside_a_character_is_moved_to_its_start() {
    let mut e = engine();
    e.record("日x", Family::Cmd);
    for cursor in 0..=4 {
        let _ = e.suggest("日x", cursor, Family::Cmd);
    }
    let list = e.suggest("x", 1, Family::Cmd);
    let fuzzy = list.iter().find(|s| s.text == "日x").expect("a fuzzy match of the history");
    assert!(!fuzzy.prefix);
    assert_eq!(fuzzy.edit("x"), (1, "日x"));
}

#[test]
fn accepting_fixes_the_case_by_erasing_from_the_first_difference() {
    let e = engine();
    let list = at_end(&e, "dIr", Family::Cmd);
    let dir = list.iter().find(|s| s.text == "dir").unwrap();
    assert!(dir.prefix);
    assert_eq!(dir.edit("dIr"), (2, "ir"));
    let git = at_end(&e, "git co", Family::Unix);
    assert_eq!(git.iter().find(|s| s.text == "commit").unwrap().edit("git co"), (0, "mmit"));
}
