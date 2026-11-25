//! The catalogs: the bundled files, reading one, and which command wins (specs/completion, "Catalogs").

use std::path::Path;

use oneterm_completion::{Catalog, Config, Engine, Family, Group, Kind, Source};

fn json_files(folder: &Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            json_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            out.push(path.display().to_string());
        }
    }
}

#[test]
fn every_file_in_the_assets_is_bundled_and_reads() {
    let mut files = Vec::new();
    json_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("assets"), &mut files);
    assert_eq!(Catalog::bundled().len(), files.len(), "a file is missing from the list in catalog.rs");
    for file in files {
        let json = std::fs::read_to_string(&file).unwrap();
        assert!(Catalog::new().add(&json, Source::External, Group::Unix).is_ok(), "{file}");
    }
}

#[test]
fn an_option_is_a_string_or_an_object() {
    let mut catalog = Catalog::new();
    let json =
        r#"{ "name": "grep", "generated": "x", "options": ["-a", { "flag": "--all", "description": " " }] }"#;
    catalog.add(json, Source::Manual, Group::Unix).unwrap();
    let e = Engine::with_catalog(Config::default(), catalog);
    let list = e.suggest("grep -", 6, Family::Unix);
    assert_eq!(list.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["-a", "--all"]);
    assert!(list.iter().all(|s| s.detail.is_none()));
}

#[test]
fn a_bad_file_is_refused_and_the_rest_stay() {
    let mut catalog = Catalog::new();
    catalog.add(r#"{ "schema": 1, "name": "dir" }"#, Source::External, Group::Windows).unwrap();
    assert!(catalog.add("{ this is not json", Source::External, Group::Windows).is_err());
    assert!(catalog.add(r#"{ "schema": 99, "name": "future" }"#, Source::External, Group::Windows).is_err());
    assert!(catalog.add(r#"{ "options": [] }"#, Source::External, Group::Windows).is_err());
    assert!(catalog.add(r#"{ "name": "x", "options": [3] }"#, Source::External, Group::Windows).is_err());
    assert_eq!(catalog.len(), 1);
    let e = Engine::with_catalog(Config::default(), catalog);
    assert_eq!(
        e.suggest_now("", 0, Family::Cmd).iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
        ["dir"]
    );
}

#[test]
fn manual_wins_over_external_then_the_earlier_group() {
    let mut catalog = Catalog::new();
    catalog.add(r#"{ "name": "COPY", "options": ["/X"] }"#, Source::External, Group::Windows).unwrap();
    catalog.add(r#"{ "name": "copy", "options": ["/Y"] }"#, Source::Manual, Group::Windows).unwrap();
    catalog.add(r#"{ "name": "date", "options": ["/T"] }"#, Source::External, Group::Windows).unwrap();
    catalog.add(r#"{ "name": "date", "options": ["-u"] }"#, Source::External, Group::Unix).unwrap();
    let both = Config { windows_allow_coreutils: true, ..Config::default() };
    let e = Engine::with_catalog(both, catalog);
    let names: Vec<_> = e.suggest("c", 1, Family::Cmd).into_iter().map(|s| (s.text, s.source)).collect();
    assert_eq!(names, [("copy".to_string(), Source::Manual)]);
    assert_eq!(e.suggest("copy /", 6, Family::Cmd)[0].text, "/Y");
    assert!(e.suggest("date -", 6, Family::Cmd).is_empty());
    assert_eq!(e.suggest("date /", 6, Family::Cmd)[0].text, "/T");
    assert_eq!(e.suggest("date -", 6, Family::Unix)[0].text, "-u");
    assert!(e.suggest("c", 1, Family::Unix).is_empty());
}

#[test]
fn the_bundled_catalogs_answer() {
    let e = Engine::new(Config::default());
    let d: Vec<_> = e.suggest("d", 1, Family::Cmd).into_iter().map(|s| s.text).collect();
    assert!(d.contains(&"dir".to_string()) && d.contains(&"del".to_string()), "{d:?}");
    assert!(e.suggest("l", 1, Family::Unix).iter().any(|s| s.text == "ls"));
    assert!(!e.suggest("l", 1, Family::Cmd).iter().any(|s| s.text == "ls" && s.prefix));
    let b = e.suggest("git checkout -", 14, Family::Unix);
    let b = b.iter().find(|s| s.text == "-b").expect("-b of git checkout");
    assert_eq!((b.detail.as_deref(), b.kind, b.source), (Some("new-branch"), Kind::Option, Source::Manual));
    // The manual ping (dashes) is found before the generated one (slashes).
    assert_eq!(e.suggest("ping -", 6, Family::Cmd)[0].text, "-t");
    let unix = Engine::new(Config { windows_allow_coreutils: true, ..Config::default() });
    assert!(unix.suggest("l", 1, Family::Cmd).iter().any(|s| s.text == "ls"));
}
