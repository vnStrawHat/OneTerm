//! Jump-host UI shared by the connect, Quick Connect, and session dialogs
//! (IN-0023 / US-0058): the hop picker (a `Select` over the other saved
//! sessions) and the per-hop credential blocks rendered above the target's
//! own authentication form.

use std::path::PathBuf;

use gpui::{
    App, AppContext as _, Entity, FocusHandle, IntoElement, ParentElement as _, Styled, Window, div,
};
use gpui_component::{
    ActiveTheme as _, IndexPath,
    select::{Select, SelectState},
    v_flex,
};
use oneterm_core::{HostKeyPolicy, SshDuplicateAuth, SshDuplicateHop, SshHop};
use oneterm_state::form_dialog::{FieldRequirement, labelled_field};

use crate::auth_form::SshAuthForm;
use crate::session_state::{SshAuthPreference, SshSessionEntry, SshSessionId, SshSessionStore};

/// The non-secret description of one hop, from a saved session or from the
/// metadata a Duplicate Session keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HopSpec {
    pub(crate) label: String,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) username: String,
    pub(crate) auth_method: SshAuthPreference,
    pub(crate) key_path: Option<PathBuf>,
}

impl HopSpec {
    /// A saved session as a hop. A hop must carry its username: there is no
    /// place to ask for one at connect time.
    pub(crate) fn from_entry(entry: &SshSessionEntry) -> Result<Self, String> {
        let session = &entry.session;
        let username = session
            .username
            .clone()
            .filter(|user| !user.is_empty())
            .ok_or_else(|| {
                format!(
                    "Jump host \"{}\" has no username; edit that session first.",
                    session.label
                )
            })?;
        Ok(Self {
            label: session.label.clone(),
            host: session.host.clone(),
            port: session.port,
            username,
            auth_method: session.auth_method,
            key_path: session.key_path.clone(),
        })
    }

    pub(crate) fn from_duplicate(hop: &SshDuplicateHop) -> Self {
        let (auth_method, key_path) = match &hop.auth {
            SshDuplicateAuth::PrivateKey { key_path } => {
                (SshAuthPreference::PrivateKey, Some(key_path.clone()))
            }
            SshDuplicateAuth::Agent => (SshAuthPreference::Agent, None),
            SshDuplicateAuth::None | SshDuplicateAuth::Password => {
                (SshAuthPreference::Password, None)
            }
        };
        Self {
            label: format!("{}@{}:{}", hop.username, hop.host, hop.port),
            host: hop.host.clone(),
            port: hop.port,
            username: hop.username.clone(),
            auth_method,
            key_path,
        }
    }

    fn address(&self) -> String {
        format!("{}@{}:{}", self.username, self.host, self.port)
    }
}

/// One credential block per hop, outermost first.
#[derive(Clone)]
pub(crate) struct JumpHopForms {
    hops: Vec<(HopSpec, SshAuthForm)>,
}

impl JumpHopForms {
    pub(crate) fn new(specs: Vec<HopSpec>, window: &mut Window, cx: &mut App) -> Self {
        let hops = specs
            .into_iter()
            .map(|spec| {
                let form = SshAuthForm::new(spec.auth_method, spec.key_path.as_deref(), window, cx);
                (spec, form)
            })
            .collect();
        Self { hops }
    }

    /// The first hop credential field, for the dialog's initial focus.
    pub(crate) fn first_secret_focus(&self, cx: &App) -> Option<FocusHandle> {
        self.hops
            .iter()
            .find_map(|(_, form)| form.secret_focus_handle(cx))
    }

    /// Render every hop as "Jump host <label> (user@host:port)" followed by
    /// its credential fields; nothing when there is no hop.
    pub(crate) fn render(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .gap_3()
            .w_full()
            .children(self.hops.iter().map(|(spec, form)| {
                v_flex()
                    .gap_2()
                    .w_full()
                    .pb_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_sm()
                            .child(format!("Jump host {}", spec.label))
                            .child(
                                div()
                                    .text_color(theme.muted_foreground)
                                    .child(spec.address()),
                            ),
                    )
                    .child(form.render(true, cx))
            }))
    }

    /// Collect every hop's credential (clearing the fields) into the
    /// backend's hop list, outermost first.
    pub(crate) fn take_hops(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Vec<SshHop>, String> {
        self.hops
            .iter()
            .map(|(spec, form)| {
                let auth = form
                    .take_auth(window, cx)
                    .map_err(|message| format!("Jump host \"{}\": {message}", spec.label))?;
                Ok(SshHop {
                    host: spec.host.clone(),
                    port: spec.port,
                    username: spec.username.clone(),
                    auth,
                    host_key_policy: HostKeyPolicy::Strict,
                })
            })
            .collect()
    }
}

/// "Jump host" field: `None` or one of the other saved sessions.
#[derive(Clone)]
pub(crate) struct JumpHostPicker {
    state: Entity<SelectState<Vec<String>>>,
    ids: Vec<SshSessionId>,
}

impl JumpHostPicker {
    /// `exclude` is the session being edited (it cannot be its own hop);
    /// `selected` is the saved reference, ignored when it no longer exists.
    pub(crate) fn new(
        exclude: Option<SshSessionId>,
        selected: Option<SshSessionId>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let sessions = SshSessionStore::global(cx).read(cx).sessions().to_vec();
        let candidates: Vec<&SshSessionEntry> = sessions
            .iter()
            .filter(|entry| Some(entry.id) != exclude)
            .collect();
        let ids: Vec<SshSessionId> = candidates.iter().map(|entry| entry.id).collect();
        let items: Vec<String> = candidates
            .iter()
            .map(|entry| {
                let session = &entry.session;
                format!(
                    "{}  ({}@{}:{})",
                    session.label,
                    session.username.as_deref().unwrap_or("?"),
                    session.host,
                    session.port
                )
            })
            .collect();
        let selected_index = selected
            .and_then(|id| ids.iter().position(|candidate| *candidate == id))
            .map(|row| IndexPath::default().row(row));
        let state =
            cx.new(|cx| SelectState::new(items, selected_index, window, cx).searchable(true));
        Self { state, ids }
    }

    pub(crate) fn selected(&self, cx: &App) -> Option<SshSessionId> {
        let index = self.state.read(cx).selected_index(cx)?;
        self.ids.get(index.row).copied()
    }

    pub(crate) fn render(&self, cx: &App) -> impl IntoElement {
        labelled_field(
            "Jump host",
            FieldRequirement::Optional,
            Select::new(&self.state)
                .placeholder("None")
                .cleanable(true)
                .w_full(),
            cx,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_state::{SshLoggingOverride, SshSession};

    fn entry(username: Option<&str>) -> SshSessionEntry {
        SshSessionEntry {
            id: SshSessionId::parse("7").unwrap(),
            session: SshSession {
                label: "bastion".into(),
                host: "bastion.example.com".into(),
                port: 2222,
                username: username.map(str::to_string),
                auth_method: SshAuthPreference::PrivateKey,
                key_path: Some(PathBuf::from("/keys/id_ed25519")),
                color: None,
                group: None,
                logging: SshLoggingOverride::Inherit,
                jump_host: None,
            },
        }
    }

    #[test]
    fn a_saved_session_becomes_a_hop_only_with_a_username() {
        let hop = HopSpec::from_entry(&entry(Some("ops"))).unwrap();
        assert_eq!(hop.address(), "ops@bastion.example.com:2222");
        assert_eq!(hop.auth_method, SshAuthPreference::PrivateKey);
        assert_eq!(
            hop.key_path.as_deref(),
            Some(std::path::Path::new("/keys/id_ed25519"))
        );

        let error = HopSpec::from_entry(&entry(None)).unwrap_err();
        assert!(error.contains("has no username"), "{error}");
        assert!(HopSpec::from_entry(&entry(Some(""))).is_err());
    }

    #[test]
    fn duplicate_metadata_prefills_the_method_without_secrets() {
        let hop = HopSpec::from_duplicate(&SshDuplicateHop {
            host: "bastion".into(),
            port: 22,
            username: "ops".into(),
            auth: SshDuplicateAuth::Agent,
        });
        assert_eq!(hop.label, "ops@bastion:22");
        assert_eq!(hop.auth_method, SshAuthPreference::Agent);
        assert_eq!(hop.key_path, None);
        let password = HopSpec::from_duplicate(&SshDuplicateHop {
            host: "bastion".into(),
            port: 22,
            username: "ops".into(),
            auth: SshDuplicateAuth::Password,
        });
        assert_eq!(password.auth_method, SshAuthPreference::Password);
    }
}
