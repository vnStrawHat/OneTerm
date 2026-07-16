//! Combined side panel — the right dock's single `DockItem::Panel`.
//!
//! OneTerm's right dock used to be a `DockItem::v_split` of two
//! `DockItem::tabs` (Session on top, SFTP on the bottom). It is now a single
//! [`DockItem::Panel`] wrapping this [`SidePanel`], which internally hosts a
//! [`SessionPanel`] and an [`SftpPanel`] in a vertical resizable split, each
//! with its own header bar (title, no close button).
//!
//! Why a composite panel instead of two dock tabs:
//! - `DockItem::Panel` is rendered *raw* by the library (no tab bar / title bar
//!   / close / zoom chrome), so this panel owns its own headers + split.
//! - `DockItem::Panel` cannot be a child of a `v_split`/`h_split`
//!   (`StackPanel::assert_panel_is_valid` only accepts `TabPanel`/`StackPanel`),
//!   so the two sections must live *inside* one panel rather than as two dock
//!   children.
//!
//! This crate (`oneterm-app`) is the only crate allowed to depend on more than
//! one feature (R9 in `docs/agents/crate-dependency-rules.md`), so the composite
//! lives here rather than in a new feature crate (which would violate R5 —
//! features must not cross-depend, except `session-ui → terminal-view`).

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div,
};

use gpui_component::{
    ActiveTheme as _,
    dock::{Panel, PanelControl, PanelEvent, PanelInfo, PanelState, register_panel},
    h_flex,
    resizable::{resizable_panel, v_resizable},
    v_flex,
};
use oneterm_session_ui::SessionPanel;
use oneterm_sftp_ui::SftpPanel;

/// Panel name registered with the gpui-component `PanelRegistry`.
///
/// The feature-agnostic shell builds this panel *by name* via
/// `build_named_panel("side_panel", ...)` — it never depends on the concrete
/// type. Saved layouts deserialize by this name too.
pub const SIDE_PANEL_NAME: &str = "side_panel";

/// Combined right-dock panel: a vertical resizable split of
/// [`SessionPanel`] (top) + [`SftpPanel`] (bottom), each with its own header.
///
/// `panel_name = "side_panel"`. Rendered raw as a `DockItem::Panel`, so it
/// draws its own title bars + the resize split between the two sections.
pub struct SidePanel {
    focus_handle: FocusHandle,
    session: Entity<SessionPanel>,
    sftp: Entity<SftpPanel>,
}

impl SidePanel {
    /// Create a new combined side panel.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let session = SessionPanel::new_entity(window, cx);
        let sftp = SftpPanel::new_entity(window, cx);

        Self {
            focus_handle: cx.focus_handle(),
            session,
            sftp,
        }
    }

    /// Helper to create an `Entity<Self>`.
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
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

impl EventEmitter<PanelEvent> for SidePanel {}

impl Focusable for SidePanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        // Delegate focus to the session tree first (top section), so keyboard
        // input reaches it instead of dying at the panel root.
        self.session.read(cx).focus_handle(cx).clone()
    }
}

impl Panel for SidePanel {
    fn panel_name(&self) -> &'static str {
        SIDE_PANEL_NAME
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "Side"
    }

    fn closable(&self, _: &App) -> bool {
        // The panel itself is the whole right dock; closing is handled via the
        // dock's toggle (collapsing the right dock), not via a panel close.
        false
    }

    fn zoomable(&self, _: &App) -> Option<PanelControl> {
        // Zoom is a TabPanel feature; `DockItem::Panel` is not subscribed to
        // zoom events by the library (see `DockArea::subscribe_item`). Drop
        // zoom for the side panel — per the design decision to switch to
        // `DockItem::Panel`.
        None
    }

    fn dump(&self, _cx: &App) -> PanelState {
        // Persist as `PanelInfo::Panel` so the saved layout records this as a
        // single panel rather than a tab group. (The gpui-component
        // `PanelInfo::Panel` load path rebuilds a `DockItem::tabs` wrapper;
        // the shell always re-applies the right dock fresh on startup — see
        // `reset_center_only` + the `MAIN_DOCK_VERSION` bump — so this dump is
        // only used for the between-session save/restore of dock openness +
        // size, not to reconstruct the exact `DockItem` variant.)
        PanelState {
            panel_name: SIDE_PANEL_NAME.to_string(),
            children: Vec::new(),
            info: PanelInfo::panel(serde_json::Value::Null),
        }
    }
}

impl Render for SidePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = cx.theme().background;

        // Vertical resizable split of the two sections. Each section is a
        // header (title) stacked above its panel content. The
        // `ResizablePanelGroup` manages its own `ResizableState` internally when
        // none is bound via `with_state`.
        let group = v_resizable("side-panel-split")
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
            .id("side-panel")
            .size_full()
            .track_focus(&self.focus_handle)
            .bg(bg)
            .child(group)
            .into_any_element()
    }
}

/// Initialize the side panel feature: register the `"side_panel"` dock panel
/// with the gpui-component `PanelRegistry` so the shell can build it by name
/// and saved layouts can deserialize it. Called by the app aggregator
/// ([`crate::init::init`]).
pub fn init(cx: &mut App) {
    register_panel(cx, SIDE_PANEL_NAME, |_, _, _, window, cx| {
        Box::new(SidePanel::new_entity(window, cx))
    });
}
