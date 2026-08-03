//! Unit tests for the catalog store + schema parsing.

use super::*;

const RAW: &[(&str, &str, &str, &str)] = &[
    (
        "dir",
        "external",
        "cmd",
        r#"{ "schema": 1, "name": "dir", "options": ["/A", "/B"] }"#,
    ),
    (
        "ls",
        "external",
        "coreutils",
        r#"{ "schema": 1, "name": "ls", "options": ["-a", "--all"] }"#,
    ),
    (
        "git",
        "manual",
        "common",
        r#"{ "schema": 1, "name": "git", "options": ["-C"],
                "subcommands": [ { "name": "commit", "options": ["-m", "--amend"] } ] }"#,
    ),
    ("bad", "external", "cmd", r#"{ this is not json"#),
    (
        "future",
        "external",
        "cmd",
        r#"{ "schema": 99, "name": "future" }"#,
    ),
];

fn cat() -> Catalog {
    Catalog::from_raw(RAW)
}

#[test]
fn parses_string_and_object_option_forms() {
    let node = parse_node(
        r#"{ "schema": 1, "name": "grep",
                "options": ["-a", { "flag": "--all", "description": "x" }] }"#,
    )
    .unwrap();
    assert_eq!(node.options[0].text, "-a");
    assert_eq!(node.options[1].text, "--all");
}

#[test]
fn rejects_unknown_major_schema() {
    assert!(parse_node(r#"{ "schema": 99, "name": "x" }"#).is_err());
}

#[test]
fn lookup_resolves_by_category_path() {
    let c = cat();
    let cmd = ShellFamily::Cmd.categories(false);
    assert!(c.lookup("dir", &cmd, ShellFamily::Cmd).is_some());
    // `ls` (coreutils) is not in the cmd search path.
    assert!(c.lookup("ls", &cmd, ShellFamily::Cmd).is_none());
    let unix = ShellFamily::Unix.categories(false);
    assert!(c.lookup("ls", &unix, ShellFamily::Unix).is_some());
}

#[test]
fn lookup_is_case_insensitive_for_cmd_only() {
    let c = cat();
    let cmd = ShellFamily::Cmd.categories(false);
    assert!(c.lookup("DIR", &cmd, ShellFamily::Cmd).is_some());
    let unix = ShellFamily::Unix.categories(false);
    // Unix is case-sensitive: "LS" must not resolve to "ls".
    assert!(c.lookup("LS", &unix, ShellFamily::Unix).is_none());
}

#[test]
fn subcommand_child_lookup() {
    let c = cat();
    let common = vec![CatalogCategory::Common];
    let git = c.lookup("git", &common, ShellFamily::Unix).unwrap();
    let commit = git.child("commit", ShellFamily::Unix).unwrap();
    assert_eq!(commit.options.len(), 2);
}

#[test]
fn malformed_file_skipped_without_breaking_siblings() {
    let c = cat();
    let cmd = ShellFamily::Cmd.categories(false);
    // "bad" (malformed) and "future" (unknown schema) yield None…
    assert!(c.lookup("bad", &cmd, ShellFamily::Cmd).is_none());
    assert!(c.lookup("future", &cmd, ShellFamily::Cmd).is_none());
    // …but "dir" still resolves.
    assert!(c.lookup("dir", &cmd, ShellFamily::Cmd).is_some());
}

#[test]
fn command_names_deduped_across_categories() {
    let c = cat();
    let unix = ShellFamily::Unix.categories(false);
    let names = c.command_names(&unix, ShellFamily::Unix);
    assert!(names.contains(&"ls"));
    assert!(names.contains(&"git"));
    assert!(!names.contains(&"dir")); // cmd-only, not in unix path
}
