//! The session's history and the settings (specs/completion, "History", "Config", "Families").

use oneterm_completion::{Config, Engine, Family};

fn lines(engine: &Engine, family: Family) -> Vec<&str> {
    engine.history(family).collect()
}

fn keeping(max_history: usize) -> Engine {
    Engine::new(Config { max_history, ..Config::default() })
}

#[test]
fn a_line_is_kept_under_its_family() {
    let mut e = keeping(10);
    e.record("  git commit -m msg ", Family::Unix);
    e.record("dir /Q", Family::Cmd);
    e.record("   ", Family::Unix);
    assert_eq!(lines(&e, Family::Unix), ["git commit -m msg"]);
    assert_eq!(lines(&e, Family::Cmd), ["dir /Q"]);
    assert!(lines(&e, Family::PowerShell).is_empty());
}

#[test]
fn a_line_run_again_is_counted_not_added() {
    let mut e = keeping(10);
    e.record("docker ps", Family::Unix);
    e.record("docker ps", Family::Unix);
    assert_eq!(lines(&e, Family::Unix), ["docker ps"]);
}

#[test]
fn the_least_recently_used_goes_first() {
    let mut e = keeping(2);
    e.record("a", Family::Unix);
    e.record("b", Family::Unix);
    e.record("a", Family::Unix);
    e.record("c", Family::Unix);
    assert_eq!(lines(&e, Family::Unix), ["a", "c"]);
    e.set_config(Config { max_history: 1, ..Config::default() });
    assert_eq!(lines(&e, Family::Unix), ["c"]);
}

#[test]
fn no_room_keeps_nothing() {
    let mut e = keeping(10);
    e.record("ls", Family::Unix);
    e.set_config(Config { max_history: 0, ..Config::default() });
    assert!(lines(&e, Family::Unix).is_empty());
    e.record("ls", Family::Unix);
    assert!(lines(&e, Family::Unix).is_empty());
    let mut e = keeping(10);
    e.record("ls", Family::Unix);
    e.clear_history();
    assert!(lines(&e, Family::Unix).is_empty());
}

#[test]
fn recent_and_frequent_lines_rank_higher() {
    let mut e = keeping(500);
    e.record("make old", Family::Unix);
    e.record("make new", Family::Unix);
    let list = e.suggest("make ", 5, Family::Unix);
    assert_eq!(list[0].text, "make new");
    for _ in 0..3 {
        e.record("make old", Family::Unix);
    }
    e.record("make new", Family::Unix);
    assert_eq!(e.suggest("make ", 5, Family::Unix)[0].text, "make old");
}

#[test]
fn secrets_are_taken_out_before_a_line_is_kept() {
    let mut e = keeping(10);
    e.record("mysql -pSECRET db", Family::Unix);
    e.record("deploy --token ghp_0123456789abcdefghij", Family::Unix);
    assert_eq!(lines(&e, Family::Unix), ["mysql -p db", "deploy --token"]);
    assert!(e.suggest("deploy ", 7, Family::Unix).iter().all(|s| !s.text.contains("ghp_")));
}

#[test]
fn a_secret_kept_unredacted_is_never_suggested() {
    let mut e = Engine::new(Config { redact_sensitive: false, ..Config::default() });
    e.record("deploy --token ghp_0123456789abcdefghij", Family::Unix);
    e.record("deploy --target prod", Family::Unix);
    e.set_config(Config::default());
    let list = e.suggest("deploy ", 7, Family::Unix);
    assert_eq!(list.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["deploy --target prod"]);
}

#[test]
fn the_defaults_are_on_and_safe() {
    let c = Config::default();
    assert!(c.enabled && c.accept_tab && c.fuzzy && c.inherit_ancestor_options && c.redact_sensitive);
    assert_eq!((c.max_history, c.min_prefix_len, c.max_visible_items), (500, 1, 8));
    assert!(c.sources.memory && c.sources.manual && c.sources.external);
    assert!(c.disable_in_alt_screen && c.require_prompt_region && !c.windows_allow_coreutils);
    assert_eq!(c.force_family, None);
}

#[test]
fn the_family_is_read_from_a_program_or_a_title() {
    let cases = [
        (r"C:\Windows\System32\cmd.exe", Family::Cmd),
        ("Command Prompt", Family::Cmd),
        ("Administrator: Command Prompt", Family::Cmd),
        (r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe", Family::PowerShell),
        ("Windows PowerShell", Family::PowerShell),
        (r"C:\Program Files\PowerShell\7\pwsh.exe", Family::PowerShell),
        ("PowerShell 7", Family::PowerShell),
        (r"C:\Program Files\Git\bin\bash.exe", Family::Unix),
        ("Git Bash", Family::Unix),
        (r"C:\Windows\System32\wsl.exe", Family::Unix),
        ("cmdlet-runner", Family::Unix),
        (r"D:\cmd\tools\bash.exe", Family::Unix),
        ("MINGW64:/c/src/powershell", Family::PowerShell),
        ("", Family::Unix),
    ];
    for (text, family) in cases {
        assert_eq!(Family::detect(text), family, "{text}");
    }
    let forced = Config { force_family: Family::parse("PWSH"), ..Config::default() };
    assert_eq!(forced.family("bash"), Family::PowerShell);
    assert_eq!(Config::default().family("cmd.exe"), Family::Cmd);
    assert_eq!(Family::parse("zsh"), Some(Family::Unix));
    assert_eq!(Family::parse("nope"), None);
}

#[test]
fn what_starts_an_option_follows_the_family() {
    assert!(Family::Cmd.is_option("/Q") && Family::Cmd.is_option("-x"));
    assert!(!Family::PowerShell.is_option("/Q") && Family::PowerShell.is_option("-Force"));
    assert!(!Family::Unix.is_option("/tmp") && Family::Unix.is_option("--all"));
}
