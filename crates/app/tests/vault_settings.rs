//! Settings written before the vault was of OneTerm 0.6's format (specs/oneterm, "Saved passwords"):
//! the hash of the master password, the first vault and its secrets are forgotten when they
//! are read; a vault of today's format stays, and is a master password.

use std::rc::Rc;

use oneterm::OneTerm;
use quadra::{Settings, provide};
use quadra_term::echo;
use quadra_testing::Harness;

const VAULT: &str = include_str!("../../vault/tests/fixtures/vault.json");

#[test]
fn settings_of_the_first_format_are_forgotten_and_a_vault_of_today_s_stays() {
    let dir = std::path::absolute(".tmp/lock-migrate").unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("settings.json");
    let old = r#"{"security.password":"pbkdf2-sha256$600000$00$00","ssh.vault":"v1$600000$00$00",
        "ssh.secrets":[["a","sv2:00"],["b","otv1:AAAA"]]}"#;
    std::fs::write(&file, old).unwrap();
    let load = |file: &std::path::Path| {
        let settings = Settings::open(file);
        let (mut h, given) = (Harness::new(), settings.clone());
        h.mount(move || {
            provide(given.clone());
            OneTerm::new(Rc::new(|_| echo())).view()
        });
        h.settle();
        settings.save_now();
        (h.has_text("OneTerm is locked"), std::fs::read_to_string(file).unwrap())
    };
    let (locked, kept) = load(&file);
    assert!(!locked, "no master password is left");
    assert!(!kept.contains("pbkdf2") && !kept.contains("v1$") && !kept.contains("sv2:"), "{kept}");
    assert!(kept.contains("otv1:AAAA"), "{kept}");
    // A vault of today's format is a master password: it stays, and the start is locked.
    let with = serde_free(&kept, VAULT);
    std::fs::write(&file, with).unwrap();
    let (locked, kept) = load(&file);
    assert!(locked && kept.contains("3f9c0a4e8b1d4c27a6e2f0b9c1d2e3f4"), "{kept}");
}

/// The settings' text with a vault put in: the document as a JSON string, written by hand.
fn serde_free(settings: &str, vault: &str) -> String {
    let quoted: String =
        vault.chars().filter(|c| !c.is_whitespace()).collect::<String>().replace('"', "\\\"");
    let at = settings.find('{').unwrap() + 1;
    format!("{}\"ssh.vault\":\"{quoted}\",{}", &settings[..at], &settings[at..])
}
