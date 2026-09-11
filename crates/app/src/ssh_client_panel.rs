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
//! Expanding the SFTP Browser (IN-0025) behaves like zooming a terminal tab:
//! this panel zooms its right-dock node so the browser fills the workspace
//! and hides the Session section while expanded; collapsing (or any other
//! zoom-out) restores the split. Both directions stay in sync through
//! [`SftpExpandedChanged`] and the base `Panel::set_zoomed` hook.
//!
//! This crate (`oneterm-app`) is the only crate allowed to depend on more than
//! one feature (R9 in `docs/agents/crate-dependency-rules.md`), so the composite
//! lives here rather than in a new feature crate (which would violate R5 —
//! features must not cross-depend, except `session-ui → terminal-view`).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
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
use oneterm_sftp_ui::{SftpExpandedChanged, SftpPanel};
use oneterm_state::{dock_util, panel_names};

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
    dock_area: gpui::WeakEntity<DockArea>,
    /// `true` while the SFTP Browser is expanded: the dock node is zoomed and
    /// only the SFTP section is rendered.
    sftp_expanded: bool,
    _sftp_subscription: Subscription,
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
        let sftp_subscription = cx.subscribe_in(
            &sftp,
            window,
            |this, _, event: &SftpExpandedChanged, window, cx| {
                this.on_sftp_expanded(event.expanded, window, cx);
            },
        );
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
            dock_area,
            sftp_expanded: false,
            _sftp_subscription: sftp_subscription,
        }
    }

    /// The SFTP Browser expanded or collapsed: zoom this panel's dock node in
    /// step (expanded = zoomed, like a terminal tab) and re-render.
    ///
    /// The dock update is deferred: zooming asks the tab group whether this
    /// panel is `zoomable`, which reads this entity — not allowed while it is
    /// being updated by this very handler.
    fn on_sftp_expanded(&mut self, expanded: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.sftp_expanded = expanded;
        cx.notify();
        let Some(dock_area) = self.dock_area.upgrade() else {
            return;
        };
        window.defer(cx, move |window, cx| {
            let node = dock_util::find_tab_node_by_panel_name(
                dock_area.read(cx),
                panel_names::SSH_CLIENT,
                cx,
            );
            let zoomed = dock_area.read(cx).zoomed_group();
            log::debug!(
                "SshClientPanel: sftp expanded={expanded}, node={node:?}, zoomed={zoomed:?}"
            );
            dock_area.update(cx, |dock_area, cx| match (expanded, node) {
                (true, Some(node)) if zoomed != Some(node) => {
                    dock_area.set_zoomed_in(node, window, cx);
                }
                // Only give the dock back when it is *this* node that fills it; a
                // zoom onto another group is what collapsed the browser.
                (false, Some(node)) if zoomed == Some(node) => {
                    dock_area.set_zoomed_out(window, cx);
                }
                _ => {}
            });
        });
    }
}

impl SshClientPanel {
    /// Helper to create an `Entity<Self>`.
    pub(crate) fn new_entity(
        dock_area: gpui::WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new(dock_area, window, cx))
    }

    /// Render a section header: the title text and, at the trailing end, the
    /// hosted panel's `Panel::title_suffix` (the SFTP Browser's expand/collapse
    /// toggle). The background uses the theme's `tab_bar` token so the headers
    /// visually match the dock tab bars.
    fn render_header(
        &self,
        title: &'static str,
        suffix: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (bg, border, foreground) = {
            let theme = cx.theme();
            (theme.tokens.tab_bar, theme.border, theme.foreground)
        };
        h_flex()
            .w_full()
            .h_8()
            .flex_shrink_0()
            .items_center()
            .bg(bg)
            .border_b_1()
            .border_color(border)
            .child(
                div()
                    .flex_1()
                    .px_2()
                    .text_sm()
                    .text_color(foreground)
                    .child(title),
            )
            // The suffix sits in the same framed control group the terminal
            // tab bar uses for its trailing buttons: full height, a left
            // border, and the tab-bar padding.
            .when_some(suffix, |this, suffix| {
                this.child(
                    h_flex()
                        .h_full()
                        .items_center()
                        .flex_shrink_0()
                        .border_l_1()
                        .border_color(border)
                        .px_2()
                        .gap_1()
                        .child(suffix),
                )
            })
    }

    /// The SFTP Browser header with its expand/collapse toggle.
    fn render_sftp_header(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let suffix = self.sftp.update(cx, |sftp, cx| {
            sftp.title_suffix(window, cx)
                .map(|element| element.into_any_element())
        });
        self.render_header("SFTP Browser", suffix, cx)
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

    /// Zoomable so the SFTP Browser can fill the workspace; the dock skin
    /// hides this panel's tab bar, so the browser's own toggle drives it.
    fn zoomable(&self, _: &App) -> bool {
        true
    }

    /// Any zoom change of this node — the browser's toggle, a `ToggleZoom`
    /// action, a zoom onto another group, or the restored zoom at startup —
    /// keeps the SFTP Browser's expanded state in step.
    fn set_zoomed(&mut self, zoomed: bool, _window: &mut Window, cx: &mut Context<Self>) {
        log::debug!("SshClientPanel: dock zoom = {zoomed}");
        if self.sftp.read(cx).expanded() != zoomed {
            self.sftp
                .update(cx, |sftp, cx| sftp.set_expanded(zoomed, cx));
        }
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = cx.theme().background;

        // Expanded SFTP Browser: the zoomed node shows only that section.
        if self.sftp_expanded {
            return div()
                .id("ssh-client-panel")
                .role(Role::Pane)
                .aria_label("SSH client")
                .size_full()
                .bg(bg)
                .child(
                    v_flex()
                        .size_full()
                        .child(self.render_sftp_header(window, cx))
                        .child(self.sftp.clone()),
                )
                .into_any_element();
        }

        // Vertical resizable split of the two sections. Each section is a
        // header (title) stacked above its panel content. The
        // `ResizablePanelGroup` manages its own `ResizableState` internally when
        // none is bound via `with_state`.
        let group = v_resizable("ssh-client-panel-split")
            .child(
                resizable_panel().child(
                    v_flex()
                        .size_full()
                        .child(self.render_header("Session", None, cx))
                        .child(self.session.clone()),
                ),
            )
            .child(
                resizable_panel().child(
                    v_flex()
                        .size_full()
                        .child(self.render_sftp_header(window, cx))
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
