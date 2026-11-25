//! Taking over from OneTerm 0.6 (specs/oneterm, "Taking over from OneTerm 0.6"): its saved sessions become
//! hosts with their colours, its terminal's settings are taken at the first pass alone, and a
//! host OneTerm has is left as it is. The folder is the test's, under `.tmp`.

use std::rc::Rc;

use oneterm::{NewSession, OneTerm};
use quadra::{Role, Settings, provide};
use quadra_term::echo;
use quadra_testing::Harness;

#[test]
fn the_folders_of_the_name_before_become_the_app_s_where_it_has_none() {
    let root = std::path::absolute(".tmp/import/adopt").unwrap();
    let _ = std::fs::remove_dir_all(&root);
    let (moved, kept) = (root.join("moved"), root.join("kept"));
    std::fs::create_dir_all(moved.join("1Term")).unwrap();
    std::fs::write(moved.join("1Term/settings.json"), "{}").unwrap();
    std::fs::create_dir_all(kept.join("1Term")).unwrap();
    std::fs::create_dir_all(kept.join("OneTerm")).unwrap();
    oneterm::adopt(&[moved.clone(), kept.clone(), root.join("none")]);
    assert!(moved.join("OneTerm/settings.json").is_file() && !moved.join("1Term").exists());
    assert!(kept.join("1Term").is_dir(), "the app's own folder is there: nothing is moved over it");
    assert!(!root.join("none").exists());
}

/// OneTerm 0.6's own fixture of a vault, its master password, and a password it sealed for
/// `deploy@10.0.0.1`.
const VAULT: &str = include_str!("../../vault/tests/fixtures/vault.json");
const PW: &str = "correct horse battery staple";
const SEALED: &str = "otv1:P5wKTosdTCdcXFxcXFxcXFxcXFxbpgEB6ylBEMijZRhddr55R0kOe/uq";

/// A window of OneTerm over its settings, and OneTerm 0.6's folder `name` under `.tmp` with two
/// sessions, the first with a saved password.
fn app(name: &str) -> (Harness, Rc<Settings>, oneterm::Model, std::path::PathBuf) {
    let folder = std::path::absolute(".tmp/import").unwrap().join(name).join(".OneTerm");
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let saved = format!(
        r##"{{"schema_version":2,"sessions":[
        {{"id":1,"label":"prod","host":"10.0.0.1","port":22,"username":"deploy","group":"infra",
         "color":"#56B6C2","sealed_password":"{SEALED}"}},
        {{"id":2,"label":"lab","host":"10.0.0.9","port":2222,"username":"me","jump_host":1}}]}}"##
    );
    std::fs::write(folder.join("ssh_session.json"), saved).unwrap();
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    let settings = Settings::memory();
    let (given, sessions): (_, NewSession) = (settings.clone(), Rc::new(|_| echo()));
    let model = Rc::new(std::cell::RefCell::new(None));
    let slot = model.clone();
    h.mount(move || {
        provide(given.clone());
        let app = OneTerm::new(sessions);
        *slot.borrow_mut() = Some(app.model.clone());
        app.view()
    });
    h.settle();
    let model = model.borrow_mut().take().unwrap();
    (h, settings, model, folder)
}

/// Waits for what the pool was given (a vault's key derived) to be told.
fn wait(h: &mut Harness, done: impl Fn() -> bool) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !done() {
        assert!(std::time::Instant::now() < end, "never done");
        h.advance(std::time::Duration::from_millis(10));
        h.settle();
    }
}

#[test]
fn the_vault_and_its_saved_passwords_come_as_they_are() {
    let (mut h, settings, model, folder) = app("vault");
    let secrets = settings.value("ssh.secrets", Vec::<Vec<String>>::new());
    let vault = settings.value("ssh.vault", String::new());
    // Hosts taken while OneTerm 0.6's vault could not be read: the password is not, and it is said.
    assert_eq!(model.take_over(&folder), (2, 0, 1));
    assert!(vault.get().is_empty() && secrets.get().is_empty());
    // With the vault there, the password arrives for the host that is here already.
    std::fs::write(folder.join("vault.json"), VAULT).unwrap();
    assert_eq!(model.take_over(&folder), (0, 1, 0));
    assert_eq!(secrets.get(), [["prod".to_string(), SEALED.to_string()]]);
    assert!(vault.get().contains("3f9c0a4e8b1d4c27a6e2f0b9c1d2e3f4"), "{}", vault.get());
    assert_eq!(model.take_over(&folder), (0, 0, 0), "nothing twice");
    // OneTerm 0.6's master password is OneTerm's: it opens the vault, and another does not.
    let lock = model.lock();
    assert!(lock.can_lock() && lock.key.get_untracked().is_none());
    let told = Rc::new(std::cell::Cell::new(None));
    for (typed, right) in [("another", false), (PW, true)] {
        let tells = told.clone();
        told.set(None);
        lock.open(typed.into(), move |it| tells.set(Some(it)));
        wait(&mut h, || told.get().is_some());
        assert_eq!((told.get(), lock.key.get_untracked().is_some()), (Some(right), right), "{typed}");
    }
    // A host whose target was changed here since keeps no password that would not open.
    secrets.set(Vec::new());
    let hosts = settings.value("ssh.hosts", Vec::<Vec<String>>::new());
    hosts.update(|rows| rows[0][2] = "deploy@10.0.0.2".into());
    h.settle();
    assert_eq!(model.take_over(&folder), (0, 0, 1));
    assert!(secrets.get().is_empty());
}

#[test]
fn a_vault_of_oneterm_s_own_is_kept_and_takes_no_password_of_another() {
    let (mut h, settings, model, folder) = app("own");
    std::fs::write(folder.join("vault.json"), VAULT).unwrap();
    let done = Rc::new(std::cell::Cell::new(false));
    let told = done.clone();
    model.lock().set_password(String::new(), "mine".into(), move |_| told.set(true));
    wait(&mut h, || done.get());
    let own = settings.value("ssh.vault", String::new()).get();
    assert_eq!(model.take_over(&folder), (2, 0, 1));
    assert_eq!(settings.value("ssh.vault", String::new()).get(), own);
    assert!(settings.value("ssh.secrets", Vec::<Vec<String>>::new()).get().is_empty());
    h.settle();
    assert!(h.has_text("prod"), "the hosts come all the same");
}

#[test]
fn hosts_and_the_terminal_s_settings_are_taken_over_and_a_host_there_is_left() {
    let (mut h, settings, model, folder) = app("settings");
    let terminal = r#"{"font":{"family":"Legacy Mono","size":15.0},"scroll":{"scrollback_history":20000}}"#;
    std::fs::write(folder.join("terminal.json"), terminal).unwrap();
    let updates = r#"{"proxy_url":"http://proxy.corp:3128","verify_certificates":false}"#;
    std::fs::write(folder.join("update_config.json"), updates).unwrap();

    assert_eq!(model.take_over(&folder), (2, 0, 1));
    h.settle();
    let rows = settings.value("ssh.hosts", Vec::<Vec<String>>::new()).get();
    assert_eq!(rows[0][..4], ["prod", "infra", "deploy@10.0.0.1", "Password"].map(String::from));
    assert_eq!(rows[0].get(7).map(String::as_str), Some("#56B6C2"), "{rows:?}");
    assert_eq!((rows[1][2].as_str(), rows[1][5].as_str()), ("me@10.0.0.9:2222", "prod"));
    assert_eq!(settings.value("terminal.font_family", String::new()).get(), "Legacy Mono");
    assert_eq!(settings.value("terminal.scrollback", 0.0).get(), 20000.0);
    // The proxy its updates went through comes along; that certificates went unchecked does not.
    let net = model.prefs().net.network();
    assert_eq!((net.proxy.as_deref(), net.verify_tls), (Some("http://proxy.corp:3128"), true));
    // The rail lists them, in their group.
    assert!(h.has_text("prod") && h.has_text("lab"), "{}", h.semantics());

    // Again: no host twice.
    assert_eq!(model.take_over(&folder), (0, 0, 1));
    assert_eq!(settings.value("ssh.hosts", Vec::<Vec<String>>::new()).get().len(), 2);
    // Nothing in the settings imports: taking over is the start's, once.
    h.get_by_role(Role::Button, "Settings").click();
    h.settle();
    h.get_by_role(Role::TreeItem, "SSH & hosts").click();
    h.settle();
    assert!(h.has_text("Saved hosts") && !h.has_text("Import from OneTerm 0.6"), "{}", h.semantics());
}
