//! SSH Client right-dock panel for [`oneterm_core::RightDockMode::SshClient`].
//!
//! The right dock contains one tab-group leaf wrapping this [`SshClientPanel`].
//! The workspace [`gpui_component::dock::DockSkin`] suppresses that group's
//! outer tab bar, while this panel hosts [`SessionPanel`] and [`SftpPanel`] in a
//! vertical resizable split with their own header bars.
//!
//! Keeping both sections inside one composite panel preserves OneTerm's
//! chrome-free right dock while the GPUI Base layout tree remains a normal,
//! serializable tab-group layout.
//!
//! This crate (`oneterm-app`) is the only crate allowed to depend on more than
//! one feature (R9 in `docs/agents/crate-dependency-rules.md`), so the composite
//! lives here rather than in a new feature crate (which would violate R5 —
//! features must not cross-depend, except `session-ui → terminal-view`).

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div,
};

use gpui_component::dock::{
    DockArea, Panel, PanelEvent, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_component::{
    ActiveTheme as _, h_flex,
    resizable::{resizable_panel, v_resizable},
    v_flex,
};
use oneterm_session_ui::SessionPanel;
use oneterm_sftp_ui::SftpPanel;
use oneterm_state::panel_names;

/// Combined right-dock panel for SSH Client Mode: a vertical resizable split of
/// [`SessionPanel`] (top) + [`SftpPanel`] (bottom), each with its own header.
///
/// Registered with the gpui-component `PanelRegistry` as
/// [`panel_names::SSH_CLIENT`]; the feature-agnostic shell builds it *by name*
/// and saved layouts deserialize by that name too. The workspace skin hides
/// the containing single-panel tab bar, so this panel draws the two section
/// headers and the resize split itself.
pub(crate) struct SshClientPanel {
    /// Focus proxy owned by the containing dock tab group.
    dock_focus_handle: FocusHandle,
    _dock_focus_subscription: Subscription,
    session: Entity<SessionPanel>,
    sftp: Entity<SftpPanel>,
}

impl SshClientPanel {
    /// Create a new SSH Client panel.
    pub(crate) fn new(
        dock_area: gpui::WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let session = SessionPanel::new_entity(window, cx);
        let sftp = SftpPanel::new_entity_in_workspace(dock_area.entity_id(), window, cx);
        let dock_focus_handle = cx.focus_handle();
        let dock_focus_subscription =
            cx.on_focus(&dock_focus_handle, window, |this, window, cx| {
                this.session
                    .read(cx)
                    .content_focus_handle()
                    .focus(window, cx);
            });

        Self {
            dock_focus_handle,
            _dock_focus_subscription: dock_focus_subscription,
            session,
            sftp,
        }
    }

    /// Helper to create an `Entity<Self>`.
    pub(crate) fn new_entity(
        dock_area: gpui::WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new(dock_area, window, cx))
    }

    /// Render a section header: just the title text. The background uses the
    /// theme's `tab_bar` token so the headers visually match the dock tab bars.
    fn render_header(&self, title: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let (bg, border, foreground) = {
            let theme = cx.theme();
            (theme.tokens.tab_bar, theme.border, theme.foreground)
        };
        h_flex()
            .w_full()
            .h_8()
            .flex_shrink_0()
            .items_center()
            .px_2()
            .bg(bg)
            .border_b_1()
            .border_color(border)
            .child(div().text_sm().text_color(foreground).child(title))
    }
}

impl EventEmitter<PanelEvent> for SshClientPanel {}

impl Focusable for SshClientPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.dock_focus_handle.clone()
    }
}

impl gpui_base::dock::Panel for SshClientPanel {
    fn panel_name(&self) -> &'static str {
        panel_names::SSH_CLIENT
    }

    fn closable(&self, _: &App) -> bool {
        // The panel itself is the whole right dock; closing is handled via the
        // dock's toggle (collapsing the right dock), not via a panel close.
        false
    }

    fn zoomable(&self, _: &App) -> bool {
        false
    }

    fn dump(&self, _cx: &App) -> PanelState {
        // Persist only this leaf's state. The surrounding TabGroup is owned by
        // the dock layout and is reconstructed from the persisted container.
        PanelState {
            panel_name: panel_names::SSH_CLIENT.to_string(),
            children: Vec::new(),
            info: PanelInfo::panel(serde_json::Value::Null),
        }
    }
}

impl Panel for SshClientPanel {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "SSH Client"
    }

    fn zoom_control(&self, _: &App) -> Option<gpui_component::dock::PanelControl> {
        None
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

impl Render for SshClientPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = cx.theme().background;

        // Vertical resizable split of the two sections. Each section is a
        // header (title) stacked above its panel content. The
        // `ResizablePanelGroup` manages its own `ResizableState` internally when
        // none is bound via `with_state`.
        let group = v_resizable("ssh-client-panel-split")
            .child(
                resizable_panel().child(
                    v_flex()
                        .size_full()
                        .child(self.render_header("Session", cx))
                        .child(self.session.clone()),
                ),
            )
            .child(
                resizable_panel().child(
                    v_flex()
                        .size_full()
                        .child(self.render_header("SFTP Browser", cx))
                        .child(self.sftp.clone()),
                ),
            );

        div()
            .id("ssh-client-panel")
            .role(Role::Pane)
            .aria_label("SSH client")
            .size_full()
            .bg(bg)
            .child(group)
            .into_any_element()
    }
}

/// Initialize the SSH Client panel: register the [`panel_names::SSH_CLIENT`] dock
/// panel with the gpui-component `PanelRegistry` so the shell can build it by
/// name and saved layouts can deserialize it. Called by the app aggregator
/// ([`crate::init::init`]).
pub(crate) fn init(cx: &mut App) {
    register_panel(cx, panel_names::SSH_CLIENT, |context, window, cx| {
        panel_handle(SshClientPanel::new_entity(context.dock_area(), window, cx))
    });
}
