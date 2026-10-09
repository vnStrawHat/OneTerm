//! Independent verification of `US-0158` (saved SSH passwords): persistence
//! attacks on `sealed_password` and on the `.bak` scrub, and hop attribution of
//! a rejected saved password. Child of `session_state`, so it reaches the
//! private `save_snapshot` / `SessionDocument`.

use super::*;
use oneterm_vault::{KdfParams, OpenError, SecretPurpose};

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(tag: &str) -> TempDir {
    let dir = std::env::temp_dir().join(format!(
        "oneterm-session-us0158v-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    TempDir(dir)
}

const E1: &str = "otv1:AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4v";
const E2: &str = "otv1:LywrKikoJyYlJCMiISAfHh0cGxoZGBcWFRQTEhEQDw4NDAsKCQgHBgUEAwIBAA==";

fn sealed(text: &str) -> SealedSecret {
    serde_json::from_value(Value::String(text.to_owned())).unwrap()
}

fn session(label: &str, envelope: Option<&str>) -> SshSession {
    SshSession {
        label: label.into(),
        host: format!("{label}.example.test"),
        port: 22,
        username: Some("deploy".into()),
        auth_method: SshAuthPreference::Password,
        key_path: None,
        color: None,
        group: None,
        logging: SshLoggingOverride::Inherit,
        jump_host: None,
        port_forwards: Vec::new(),
        agent_forwarding: false,
        sealed_password: envelope.map(sealed),
    }
}

fn doc(entries: Vec<(u64, SshSession)>) -> SessionDocument {
    let next_id = entries.iter().map(|(id, _)| id + 1).max().unwrap_or(1);
    SessionDocument {
        entries: entries
            .into_iter()
            .map(|(id, session)| SshSessionEntry {
                id: SshSessionId(id),
                session,
            })
            .collect(),
        next_id,
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// Attack (a): deleting the only session that holds an envelope retires it,
/// so `.bak` is scrubbed; deleting one of two copies (Duplicate) retires
/// nothing, so `.bak` keeps the previous generation.
#[test]
fn deleting_a_session_scrubs_only_when_its_envelope_leaves_the_document() {
    let dir = temp_dir("delete");
    let path = dir.0.join("ssh_session.json");
    let bak = dir.0.join("ssh_session.bak");

    // Two copies of one envelope (Duplicate Session), plus an unrelated one.
    SshSessionStore::save_snapshot(
        &doc(vec![
            (1, session("a", Some(E1))),
            (2, session("a-copy", Some(E1))),
            (3, session("b", Some(E2))),
        ]),
        &path,
    );
    SshSessionStore::save_snapshot(
        &doc(vec![
            (1, session("a", Some(E1))),
            (3, session("b", Some(E2))),
        ]),
        &path,
    );
    assert!(
        read(&bak).contains("a-copy"),
        "E1 is still in use: ordinary write"
    );

    // Delete `b`: E2 leaves the document, so no file keeps it.
    SshSessionStore::save_snapshot(&doc(vec![(1, session("a", Some(E1)))]), &path);
    assert!(!read(&bak).contains(E2));
    assert!(!read(&path).contains(E2));
    assert_eq!(read(&bak), read(&path));
}

/// Attack (a): the decision is made against the file actually replaced, so a
/// coalesced queue that skips the intermediate snapshot still scrubs, and an
/// unreadable previous document counts as holding a secret.
#[test]
fn the_scrub_is_decided_against_the_replaced_file_not_the_last_snapshot() {
    let dir = temp_dir("coalesce");
    let path = dir.0.join("ssh_session.json");
    let bak = dir.0.join("ssh_session.bak");
    SshSessionStore::save_snapshot(&doc(vec![(1, session("a", Some(E1)))]), &path);

    // In memory: Replace (E1 -> E2), then a rename; only the rename is written.
    let mut renamed = session("a", Some(E2));
    renamed.group = Some("g".into());
    SshSessionStore::save_snapshot(&doc(vec![(1, renamed)]), &path);
    assert!(!read(&bak).contains(E1));

    // A previous document that does not parse is scrubbed over.
    std::fs::write(&path, format!("{{ not json {E1}")).unwrap();
    SshSessionStore::save_snapshot(&doc(vec![(1, session("a", Some(E2)))]), &path);
    assert!(!read(&bak).contains(E1));

    // Swapping two envelopes between sessions retires none: ordinary write.
    SshSessionStore::save_snapshot(
        &doc(vec![
            (1, session("a", Some(E1))),
            (2, session("b", Some(E2))),
        ]),
        &path,
    );
    let before = read(&path);
    SshSessionStore::save_snapshot(
        &doc(vec![
            (1, session("a", Some(E2))),
            (2, session("b", Some(E1))),
        ]),
        &path,
    );
    assert_eq!(read(&bak), before, "previous generation kept");
}

/// Info finding I1: when `ssh_session.json` is missing (deleted by hand, or
/// quarantined) a stale `.bak` is left as it is by the first write, envelopes
/// included; the second write replaces it. Pinned so a change is noticed.
#[test]
fn a_stale_backup_survives_the_first_write_after_the_document_went_missing() {
    let dir = temp_dir("stale-bak");
    let path = dir.0.join("ssh_session.json");
    let bak = dir.0.join("ssh_session.bak");
    std::fs::write(&bak, format!("{{\"stale\": \"{E1}\"}}")).unwrap();

    SshSessionStore::save_snapshot(&doc(vec![(1, session("a", None))]), &path);
    assert!(read(&bak).contains(E1), "first write: .bak untouched");
    SshSessionStore::save_snapshot(&doc(vec![(1, session("b", None))]), &path);
    assert!(
        !read(&bak).contains(E1),
        "second write: previous generation"
    );
}

/// The session record exactly as on `main` before `US-0158` (v0.6.8 shape):
/// same attributes, **flattened** into the entry like the real
/// `SshSessionEntry`, no `sealed_password`, no `deny_unknown_fields`.
#[derive(Serialize, Deserialize)]
struct OldSession {
    label: String,
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(default, skip_serializing_if = "is_password_auth")]
    auth_method: SshAuthPreference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    group: Option<String>,
    #[serde(default, skip_serializing_if = "is_inherited_logging")]
    logging: SshLoggingOverride,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    jump_host: Option<SshSessionId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    port_forwards: Vec<PortForward>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    agent_forwarding: bool,
}

#[derive(Serialize, Deserialize)]
struct OldEntry {
    id: SshSessionId,
    #[serde(flatten)]
    session: OldSession,
}

/// Attack (f): what the current binary **writes** (not a hand fixture) is
/// read by the old flattened entry, every session kept; and every fixture
/// carrying the field, including the non-string one, loads in the old shape.
#[test]
fn the_old_flattened_entry_reads_what_the_current_binary_writes() {
    let dir = temp_dir("old-binary");
    let path = dir.0.join("ssh_session.json");
    let mut hop = session("bastion", Some(E1));
    hop.port_forwards = vec![PortForward::Local {
        bind: std::net::Ipv4Addr::LOCALHOST.into(),
        bind_port: 8080,
        target_host: "db".into(),
        target_port: 5432,
    }];
    let mut target = session("prod", Some(E2));
    target.jump_host = Some(SshSessionId(1));
    SshSessionStore::save_snapshot(&doc(vec![(1, hop), (2, target)]), &path);

    let value: Value = serde_json::from_str(&read(&path)).unwrap();
    assert_eq!(value["schema_version"], 2, "no schema bump");
    let old: Vec<OldEntry> = serde_json::from_value(value[SESSIONS_FIELD].clone()).unwrap();
    assert_eq!(old.len(), 2);
    assert_eq!(old[1].session.jump_host, Some(SshSessionId(1)));
    assert_eq!(old[0].session.port_forwards.len(), 1);
    let rewritten = serde_json::to_value(&old).unwrap();
    assert!(!rewritten.to_string().contains("sealed_password"));

    for fixture in [
        include_str!("../tests/fixtures/persistence/ssh-session-v2-sealed.json"),
        include_str!("../tests/fixtures/persistence/ssh-session-invalid-sealed-type.json"),
    ] {
        let value: Value = serde_json::from_str(fixture).unwrap();
        let old: Vec<OldEntry> = serde_json::from_value(value[SESSIONS_FIELD].clone()).unwrap();
        assert!(!old.is_empty());
    }
}

/// Attack (f): every non-string shape loads as `None` without failing the
/// document; a plain-text string is kept opaque (never treated as a
/// password: it opens as `Malformed`), and `otv2:` opens as `NewerFormat`.
#[test]
fn odd_sealed_values_load_leniently_and_never_open_as_a_password() {
    for odd in ["null", "42", "true", "{\"a\":1}", "[\"otv1:x\"]"] {
        let raw = format!(
            r#"{{"schema_version":2,"next_session_id":2,"sessions":[{{"id":1,"label":"a","host":"h","username":"u","group":null,"sealed_password":{odd}}}]}}"#
        );
        let loaded = parse_document(&raw).unwrap_or_else(|e| panic!("{odd}: {e}"));
        assert_eq!(loaded.document.entries.len(), 1, "{odd}");
        assert_eq!(loaded.document.entries[0].session.sealed_password, None);
    }

    let raw = r#"{"schema_version":2,"next_session_id":2,"sessions":[{"id":1,"label":"a","host":"h","username":"u","group":null,"sealed_password":"hunter2"}]}"#;
    let loaded = parse_document(raw).unwrap();
    let kept = loaded.document.entries[0]
        .session
        .sealed_password
        .clone()
        .unwrap();
    assert_eq!(format!("{kept:?}"), "<sealed>", "Debug never shows it");

    let dir = temp_dir("open");
    let vault = oneterm_vault::init_with_params(
        &dir.0,
        &oneterm_core::SecretString::new("master password"),
        KdfParams::FLOOR,
    )
    .unwrap();
    let context = SealContext {
        host: "h",
        port: 22,
        username: "u",
    };
    assert_eq!(
        vault
            .open(SecretPurpose::SshPassword, &context, &kept)
            .err(),
        Some(OpenError::Malformed)
    );
    assert_eq!(
        vault
            .open(
                SecretPurpose::SshPassword,
                &context,
                &sealed("otv2:a-format-this-binary-does-not-know")
            )
            .err(),
        Some(OpenError::NewerFormat)
    );
}

mod hop_attribution {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{Context, IntoElement, Render, TestAppContext, Window};

    use crate::jump_hops::{HopSpec, JumpHopForms};
    use crate::session_state::SshAuthPreference;

    struct Empty;

    impl Render for Empty {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            gpui::div()
        }
    }

    fn spec(host: &str, port: u16) -> HopSpec {
        HopSpec {
            label: host.into(),
            host: host.into(),
            port,
            username: "ops".into(),
            auth_method: SshAuthPreference::Password,
            key_path: None,
            saved: None,
        }
    }

    fn hops(specs: Vec<HopSpec>, cx: &mut TestAppContext) -> JumpHopForms {
        let probe = Rc::new(RefCell::new(None));
        let probe_in = probe.clone();
        let (_view, _cx) = cx.add_window_view(move |window, cx| {
            *probe_in.borrow_mut() = Some(JumpHopForms::new(specs, window, cx));
            Empty
        });
        probe.borrow().clone().unwrap()
    }

    fn index_of(forms: &JumpHopForms, message: &str) -> Option<usize> {
        let picked = forms.form_for_error(message)?;
        forms.forms().position(|form| std::ptr::eq(form, picked))
    }

    /// The message `oneterm-ssh` `hop_error` builds for a hop's failure.
    fn rejected(label: &str) -> String {
        format!("jump host {label}: rejected by the server")
    }

    /// Attack (b): same user and host, ports 22 / 2222 / 222; an IPv6 literal
    /// with colons. The trailing `:` after the port keeps these apart.
    #[gpui::test]
    fn ports_sharing_a_prefix_and_ipv6_hosts_pick_their_own_hop(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let forms = hops(
            vec![
                spec("10.0.0.2", 22),
                spec("10.0.0.2", 2222),
                spec("10.0.0.2", 222),
                spec("::1", 22),
            ],
            cx,
        );
        assert_eq!(index_of(&forms, &rejected("ops@10.0.0.2:22")), Some(0));
        assert_eq!(index_of(&forms, &rejected("ops@10.0.0.2:2222")), Some(1));
        assert_eq!(index_of(&forms, &rejected("ops@10.0.0.2:222")), Some(2));
        assert_eq!(index_of(&forms, &rejected("ops@::1:22")), Some(3));
        // The target's own failure carries no hop prefix.
        assert_eq!(index_of(&forms, "rejected by the server"), None);
    }

    /// Defect D1 (fixed in the rework): two IPv6 hops where one address is the
    /// other followed by `:<group>` (`2001:db8::1` and `2001:db8::1:22`, both
    /// port 22). The second hop's failure `jump host ops@2001:db8::1:22:22: ...`
    /// also starts with the first hop's prefix `jump host ops@2001:db8::1:22:`;
    /// the longest matching prefix picks the second hop, and the first hop's
    /// own failure still picks the first.
    #[gpui::test]
    fn d1_an_ipv6_hop_whose_address_extends_another_picks_the_longest_prefix(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_component::init);
        let forms = hops(
            vec![spec("2001:db8::1", 22), spec("2001:db8::1:22", 22)],
            cx,
        );
        assert_eq!(
            index_of(&forms, &rejected("ops@2001:db8::1:22:22")),
            Some(1)
        );
        assert_eq!(index_of(&forms, &rejected("ops@2001:db8::1:22")), Some(0));
        // Listed the other way round, the same messages pick the same hops.
        let reversed = hops(
            vec![spec("2001:db8::1:22", 22), spec("2001:db8::1", 22)],
            cx,
        );
        assert_eq!(
            index_of(&reversed, &rejected("ops@2001:db8::1:22:22")),
            Some(0)
        );
        assert_eq!(
            index_of(&reversed, &rejected("ops@2001:db8::1:22")),
            Some(1)
        );
    }

    /// Re-verification of the D1 rework: two hops with the same address tie
    /// and the first is taken; hop text that is not at the start of the
    /// message (a target error quoting it) matches no hop; and the prefix is
    /// exactly what `oneterm-ssh` builds from `SshHop::label()`.
    #[gpui::test]
    fn ties_take_the_first_hop_and_only_a_leading_prefix_counts(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let forms = hops(vec![spec("10.0.0.2", 22), spec("10.0.0.2", 22)], cx);
        assert_eq!(index_of(&forms, &rejected("ops@10.0.0.2:22")), Some(0));
        assert_eq!(
            index_of(
                &forms,
                "rejected by the server; banner: jump host ops@10.0.0.2:22: x"
            ),
            None
        );

        let hop = oneterm_core::SshHop {
            host: "2001:db8::1:22".into(),
            port: 22,
            username: "ops".into(),
            auth: oneterm_core::SshAuthMethod::None,
            host_key_policy: oneterm_core::HostKeyPolicy::Strict,
        };
        let forms = hops(
            vec![spec("2001:db8::1", 22), spec("2001:db8::1:22", 22)],
            cx,
        );
        let emitted = format!("jump host {}: rejected by the server", hop.label());
        assert_eq!(index_of(&forms, &emitted), Some(1));
    }
}
