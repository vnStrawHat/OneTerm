//! The [`TerminalPanel`] type: [`PanelSpec`], the [`TerminalPanel::open`]
//! constructor, the session-spawn helpers, the read-only accessors used by the
//! status bar / Agent Panel, and the dock trait implementations (including the
//! [`Render`] impl with the panel's action handlers).
//!
//! Space operations live in [`super::spaces`], the duplicate-session flows in
//! [`super::duplicate`], and tab-label resolution + the tab-strip element in
//! [`super::tab_title`].

use gpui::{
    Anchor, App, AppContext as _, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable,
    Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, Subscription, WeakEntity, Window, div, px,
};
use gpui_component::dock::{ClosePanel, Panel, PanelControl, PanelEvent, TabGroup};
use gpui_component::{
    ActiveTheme as _, Colorize as _, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    menu::{DropdownMenu as _, PopupMenuItem},
};

use oneterm_actions::{
    AddPanelWithShell, CloseInputChannel, CloseSpace, DuplicateSession, JoinInputChannel,
    LeaveInputChannel, NewSession, SplitDown, SplitLeft, SplitRight, SplitUp, TerminalClear,
    TerminalCopy, TerminalPaste, TerminalSelectAll,
};
use oneterm_core::{InputChannel, LocalShellConfig, SessionDuplicateConfig, ShellKind};
use oneterm_settings::TabTitleMode;
use oneterm_state::AppServices;
use oneterm_terminal::{PtySize, SessionKind, TerminalSession};
use std::path::PathBuf;

use super::tab_title::shell_tab_title;
use crate::input::edit;
use crate::security::security_policy_from_settings;
use crate::space::{SpaceId, SpaceTree, SplitContext, SplitDir};
use crate::terminal_view::{TerminalDeps, TerminalView, TerminalViewEvent};

/// How a [`labelled_separator`]'s rule is drawn: the "SSH Sessions" heading is
/// solid, a session group's heading dashed (`IN-0033`, 2026-09-15 rework).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SeparatorRule {
    Solid,
    Dashed,
}

/// A menu separator that carries a label, centred between two rules.
///
/// `PopupMenu` has a plain `Separator` and a plain `Label` but nothing that is
/// both, so the row is composed from a disabled element item: a rule, the label
/// text, another rule. Disabled keeps it out of hover and keyboard navigation,
/// like the separator it stands in for. Colours come from the theme; the rules
/// are the kit separator's own 2px, which is also what makes the dashes read as
/// dashes.
fn labelled_separator(label: impl Into<SharedString>, rule: SeparatorRule) -> PopupMenuItem {
    let label = label.into();
    PopupMenuItem::element(move |_, cx| {
        let line = || {
            let line = div()
                .flex_1()
                .h(px(0.))
                .border_t(px(2.))
                .border_color(cx.theme().border);
            match rule {
                SeparatorRule::Solid => line,
                SeparatorRule::Dashed => line.border_dashed(),
            }
        };
        gpui_component::h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(line())
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(label.clone()),
            )
            .child(line())
    })
    .disabled(true)
}

/// One saved SSH session in the "+" menu: the session's colour square, then its
/// title — the same pair the right dock's session tree draws for a leaf, so the
/// two surfaces read as one list (`US-0110`).
///
/// The colour arrives as a hex string the session feature has already resolved
/// against the same rule the tree uses (see `SavedSshSessionSections`), so it
/// always parses and the theme accent below is a formality, not a second
/// fallback policy — the fallback lives in one place, and not here. An element
/// item rather than a plain
/// one because a plain item renders text alone — the kit gives both the same
/// padding, height, hover and selection styling, and treats both as clickable
/// for the mouse and the arrow keys.
fn saved_session_row(title: String, color_hex: String) -> PopupMenuItem {
    PopupMenuItem::element(move |_, cx| {
        let color = Hsla::parse_hex(&color_hex).unwrap_or_else(|_| cx.theme().accent);
        gpui_component::h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(div().w(px(8.)).h(px(8.)).bg(color).flex_shrink_0())
            .child(div().truncate().child(title.clone()))
    })
}

/// How many rows the "+" menu emits, so the scroll estimate below stays honest
/// in both modes rather than being a literal that goes stale.
///
/// Unelevated: the three shells, the "SSH Sessions" heading, `session_rows`, the
/// closing separator and the two closing rows ("Quick Connect...", "New Saved
/// Session..."). Elevated: the three shells and nothing else — M1 (`DEC-0019`)
/// leaves an elevated window with local shells only.
pub(super) fn menu_rows(elevated: bool, session_rows: usize) -> usize {
    const SHELL_ROWS: usize = 3;
    if elevated {
        return SHELL_ROWS;
    }
    SHELL_ROWS + ELEVATED_SUBMENU_ROWS + 1 + session_rows + 1 + 2
}

/// The `Run as administrator >` row, which exists on Windows only and never in
/// an elevated window (`US-0132`).
#[cfg(windows)]
const ELEVATED_SUBMENU_ROWS: usize = 1;
#[cfg(not(windows))]
const ELEVATED_SUBMENU_ROWS: usize = 0;

/// Whether the popup needs the kit's scrolling container for `rows`.
///
/// A saved list can be longer than the window, and the kit applies its height
/// cap (half the window, at most 450px) only when `scrollable` is set: without
/// it a long list runs off the bottom, unreachable by mouse *and* by keyboard
/// (`scroll_to_item` is a no-op outside a scrolling container). But a scrollable
/// menu also carries a scrollbar, which this app's theme keeps permanently
/// visible, so scrolling is turned on only when the rows really cannot fit: the
/// everyday menu stays free of the bar and a long one stays reachable.
///
/// ponytail: the row height is the kit's own 26px item plus its 2px gap,
/// estimated rather than measured, so a menu within a row of the cap can guess
/// wrong by one row. Measuring would need the popup's laid-out bounds, which do
/// not exist while it is being built.
fn menu_scrolls(rows: usize, window: &Window) -> bool {
    const ROW_HEIGHT: f32 = 28.;
    let cap = (window.window_bounds().get_bounds().size.height * 0.5).min(px(450.));
    px(rows as f32 * ROW_HEIGHT) > cap
}

/// Initial PTY size for a freshly spawned session; the element resizes it to
/// the real grid on the first prepaint.
pub(super) const INITIAL_PTY_SIZE: PtySize = PtySize::INITIAL;

/// The label a reset tab, or a custom shell with no program, falls back to.
/// A local-shell tab is named after its shell — see `shell_tab_title`.
pub(super) const DEFAULT_TAB_TITLE: &str = "Terminal";

/// Panel displaying a Terminal Tab (a tree of Spaces).
pub struct TerminalPanel {
    /// Focus target owned by the dock's tab-group frame. When it receives focus,
    /// `_dock_focus_subscription` forwards to the active Space in the next frame.
    pub(super) dock_focus_handle: FocusHandle,
    pub(super) _dock_focus_subscription: Subscription,
    /// DockArea identity used to publish active state only to this workspace.
    pub(super) workspace_id: Option<EntityId>,
    /// The pane tree — leaves are terminals or empty placeholders.
    pub(super) tree: SpaceTree,
    /// Reference to the containing `TabGroup`, used by terminal-tab close policy
    /// and Agent Panel navigation.
    pub(super) tab_panel: Option<WeakEntity<TabGroup>>,
    /// Whether this panel is the currently selected tab in the `TabGroup`.
    pub(super) is_active: bool,
    /// Tab title fallback — the shell's display name for local, the session
    /// label for SSH. A live OSC 0/2 title wins over it.
    pub(super) tab_title: String,
    /// Manual tab title override selected by the user (wins over OSC 0/2).
    pub(super) tab_title_override: Option<String>,
    /// Subscriptions to every terminal leaf's `TitleChanged` — rebuilt whenever
    /// the set of terminals changes (split / close / drop / fill).
    pub(super) _title_subs: Vec<Subscription>,
    /// Subscription to global `TerminalSettings` changes.
    pub(super) _settings_sub: Subscription,
    /// Subscription to the broadcast input channel registry, so a join or a
    /// `Close Channel` performed in another tab repaints this tab's chips and
    /// Space frames.
    pub(super) _channels_sub: Option<Subscription>,
    /// The services this panel's terminal views receive (ARCH-20).
    pub(super) deps: TerminalDeps,
}

/// What a new terminal tab hosts. Passed to [`TerminalPanel::open`].
pub enum PanelSpec {
    /// The default local shell from settings, bound to the dock area
    /// `workspace` (the primary workspace when `None`).
    DefaultShell { workspace: Option<EntityId> },
    /// A local shell of a specific kind (other shell settings unchanged).
    Shell(ShellKind),
    /// An already-connected session (SSH connect, duplicate) with its tab
    /// title and optional non-secret duplication metadata.
    Session {
        session: Box<dyn TerminalSession>,
        title: String,
        duplicate_config: Option<SessionDuplicateConfig>,
    },
}

impl TerminalPanel {
    /// Create a panel entity for `spec`. The single public constructor.
    pub fn open(spec: PanelSpec, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::from_spec(spec, window, cx))
    }

    /// Build the panel for `spec`: spawn/wrap the session view, put it in a
    /// single-leaf Space tree, focus it, and wire the title + settings
    /// subscriptions. When a local shell fails to spawn the tree starts empty
    /// (the user can retry from the placeholder).
    pub(crate) fn from_spec(spec: PanelSpec, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let primary = oneterm_state::AppState::primary_workspace_id(cx);
        // The one place the terminal feature reads its process globals; the
        // views get them handed down (ARCH-20).
        let deps = TerminalDeps::from_globals(cx);
        let (view, tab_title, workspace_id) = match spec {
            PanelSpec::DefaultShell { workspace } => {
                // `--elevated-shell <kind>` *replaces* the configured default
                // shell rather than adding a tab beside it, so `DEC-0016`'s
                // "exactly one initial shell" still holds (`IN-0043`). The
                // global is `None` for every ordinary start.
                let requested = oneterm_core::elevation::initial_shell();
                // Name the tab after the shell settings actually spawn, so ten
                // local tabs are ten distinguishable tabs (`US-0114`).
                let title = match requested {
                    // `spawn_local_view` clears `program` for an explicit kind.
                    Some(kind) => shell_tab_title(kind, None),
                    None => {
                        let shell = &deps.settings.read(cx).shell;
                        shell_tab_title(shell.kind, shell.program.as_deref())
                    }
                };
                (
                    Self::spawn_local_view(&deps, requested, window, cx),
                    title,
                    workspace.or(primary),
                )
            }
            PanelSpec::Shell(kind) => (
                Self::spawn_local_view(&deps, Some(kind), window, cx),
                // `spawn_local_view` clears `program` for an explicit kind, so
                // the label is the kind's own name.
                shell_tab_title(kind, None),
                primary,
            ),
            PanelSpec::Session {
                session,
                title,
                duplicate_config,
            } => {
                let session = cx.new(|_| session);
                let view = Self::new_view(&deps, session, duplicate_config, window, cx);
                (Some(view), title, primary)
            }
        };

        let focus = cx.focus_handle();
        let tree = match &view {
            Some(view) => {
                // Focus the terminal view right after creation.
                view.read(cx).focus_handle(cx).focus(window, cx);
                SpaceTree::new_terminal(view.clone(), focus)
            }
            None => {
                log::warn!("TerminalPanel: spawn failed, creating empty tree");
                SpaceTree::new_empty(focus)
            }
        };
        let active = tree.active();
        let dock_focus_handle = cx.focus_handle();
        let dock_focus_subscription =
            cx.on_focus(&dock_focus_handle, window, |this, window, cx| {
                this.focus_active_space(window, cx);
            });

        let _settings_sub = cx.observe(&deps.settings, |_this, _settings, cx| {
            cx.notify();
        });
        let _channels_sub = deps
            .input_channels
            .as_ref()
            .map(|registry| cx.observe(registry, |_this, _registry, cx| cx.notify()));

        let mut this = Self {
            dock_focus_handle,
            _dock_focus_subscription: dock_focus_subscription,
            workspace_id,
            tree,
            tab_panel: None,
            is_active: false,
            tab_title,
            tab_title_override: None,
            _title_subs: Vec::new(),
            _settings_sub,
            _channels_sub,
            deps,
        };
        if let Some(view) = &view {
            this.attach_split_ctx(view, active, cx);
        }
        this.rebuild_title_subs(cx);
        this
    }

    /// Spawn a fresh local session + view from the current settings, optionally
    /// overriding the shell kind. `None` when the session could not be spawned
    /// (e.g. missing shell); the caller shows an empty Space instead.
    pub(super) fn spawn_local_view(
        deps: &TerminalDeps,
        shell_kind_override: Option<ShellKind>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<TerminalView>> {
        let shell = {
            let settings = deps.settings.read(cx);
            match shell_kind_override {
                Some(kind) => {
                    // Override the shell kind but keep other settings (utf8, cwd, env, args).
                    let mut shell = settings.shell.clone();
                    shell.kind = kind;
                    // Clear program so resolve_shell auto-detects for the new kind.
                    shell.program = None;
                    shell
                }
                None => settings.shell.clone(),
            }
        };
        let session = match Self::spawn_local_session(deps, shell.clone(), cx) {
            Ok(session) => session,
            Err(error) => {
                log::error!("Failed to spawn local terminal session: {error}");
                return None;
            }
        };
        let session = cx.new(|_| session);
        Some(Self::new_view(
            deps,
            session,
            Some(SessionDuplicateConfig::Local(shell)),
            window,
            cx,
        ))
    }

    /// Spawn a local session from an exact shell configuration, applying the
    /// settings-derived scrollback / OSC security / logging policy (SEC-08).
    pub(super) fn spawn_local_session(
        deps: &TerminalDeps,
        shell: LocalShellConfig,
        cx: &mut Context<Self>,
    ) -> oneterm_core::Result<Box<dyn TerminalSession>> {
        let (scrollback_history, security, logging) = {
            let settings = deps.settings.read(cx);
            (
                settings.scrollback_history,
                security_policy_from_settings(settings),
                settings.logging.local_config(),
            )
        };
        AppServices::session_factory(cx).spawn_local(
            shell,
            INITIAL_PTY_SIZE,
            scrollback_history,
            security,
            logging,
        )
    }

    /// Build the view for an already-created session entity.
    pub(super) fn new_view(
        deps: &TerminalDeps,
        session: Entity<Box<dyn TerminalSession>>,
        duplicate_config: Option<SessionDuplicateConfig>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TerminalView> {
        let deps = deps.clone();
        cx.new(|cx| {
            let mut view = TerminalView::new(session, deps, window, cx);
            view.duplicate_config = duplicate_config;
            view
        })
    }

    /// Point `view`'s context menu at Space `space_id` in this panel.
    pub(super) fn attach_split_ctx(
        &self,
        view: &Entity<TerminalView>,
        space_id: SpaceId,
        cx: &mut Context<Self>,
    ) {
        let panel = cx.entity().downgrade();
        view.update(cx, |v, _| {
            v.split_ctx = Some(SplitContext { panel, space_id });
        });
    }

    /// (Re)subscribe to `TitleChanged` on every terminal leaf so the tab strip
    /// refreshes on OSC 0/2 title changes regardless of which leaf changed.
    pub(super) fn rebuild_title_subs(&mut self, cx: &mut Context<Self>) {
        self._title_subs = self
            .tree
            .terminal_views()
            .into_iter()
            .map(|view| {
                cx.subscribe(&view, |_this, _view, _ev: &TerminalViewEvent, cx| {
                    cx.notify();
                })
            })
            .collect();
    }

    /// Empty Space ids in visual tree order.
    pub(crate) fn empty_space_destinations(&self) -> Vec<SpaceId> {
        self.tree.empty_space_destinations()
    }

    /// The active Space's terminal view (used by Edit ▸ Find). `None` when the
    /// active Space is empty.
    pub(crate) fn active_view(&self) -> Option<Entity<TerminalView>> {
        self.tree.active_terminal()
    }

    /// Session network stats for the active Space (SSH only — `None` for local
    /// or an empty Space). Used by the StatusBar.
    pub(crate) fn network_stats(&self, cx: &App) -> Option<oneterm_terminal::NetStats> {
        self.tree
            .active_terminal()?
            .read(cx)
            .session
            .read(cx)
            .capabilities()
            .network_stats
    }

    /// Breadcrumb label for the active Space's session: the OSC 7 cwd as
    /// displayed text. `None` when the active Space is empty or has no cwd yet.
    pub(crate) fn breadcrumb_label(&self, cx: &App) -> Option<String> {
        let view = self.tree.active_terminal()?;
        let cwd = view.read(cx).session.read(cx).cwd()?;
        Some(cwd.display().to_string())
    }

    /// The OSC 7 cwd of the active Space when its session is a local shell.
    /// `None` for SSH (the path is remote), an empty Space, or no cwd yet.
    pub(crate) fn local_cwd(&self, cx: &App) -> Option<PathBuf> {
        let view = self.tree.active_terminal()?;
        let session = view.read(cx).session.read(cx);
        (session.kind() == SessionKind::Local)
            .then(|| session.cwd())
            .flatten()
    }

    /// Number of Spaces in this tab.
    pub(crate) fn leaf_count(&self) -> usize {
        self.tree.leaf_count()
    }

    /// The resolved tab label (manual override, live OSC 0/2 title, or static
    /// fallback), respecting the `tab_title_mode` setting. Used as the Agent
    /// Panel tab-group title (`docs/agent-panel-display.md` §2.1).
    ///
    /// Reads the active terminal's session title via `v.read(cx)`. Do **not**
    /// call this from inside a `TerminalView::update` closure on the active
    /// terminal — it would re-enter the view's lease and panic
    /// (`entity_map::read` double-lease). Use [`Self::tab_label_with_title`]
    /// instead, passing the title fetched from the already-leased view's own
    /// `session.read(cx).title()`.
    pub(crate) fn tab_label(&self, cx: &App) -> String {
        let session_title = self
            .tree
            .active_terminal()
            .and_then(|v| v.read(cx).session.read(cx).title());
        self.tab_label_with_title(session_title.as_deref(), cx)
    }

    /// Same as [`Self::tab_label`] but takes the live session title as a
    /// parameter instead of reading the active terminal view. Use this from
    /// contexts where the active `TerminalView` is already being updated
    /// (e.g. `push_agent_status` inside the view's `update` closure) — reading
    /// it again would re-enter the view's lease and panic.
    ///
    /// `live_title` is the OSC 0/2 title of the terminal the event came from
    /// (the caller already has the view and can read its own session). It is
    /// only used when `tab_title_mode == Osc` and no manual override exists; in
    /// `Default` mode the static `tab_title` fallback is returned.
    pub(crate) fn tab_label_with_title(&self, live_title: Option<&str>, cx: &App) -> String {
        let live = match self.deps.settings.read(cx).tab_title_mode {
            TabTitleMode::Osc => live_title,
            TabTitleMode::Default => None,
        };
        self.effective_tab_label(live)
    }

    /// The Agent Panel ordering key for `space_id`: its 0-based depth-first
    /// (left-to-right) position in the current tree. User-facing labels use the
    /// stable `SpaceId` instead.
    pub(crate) fn space_order(&self, space_id: SpaceId) -> usize {
        self.tree.leaf_index(space_id).unwrap_or(0)
    }

    /// A weak handle to the containing `TabGroup` (for Agent Panel click-to-focus).
    pub(crate) fn tab_panel_weak(&self) -> Option<WeakEntity<TabGroup>> {
        self.tab_panel.clone()
    }

    /// Whether this tab has no terminal Spaces left (all empty).
    pub(crate) fn has_no_terminals(&self) -> bool {
        self.tree.has_no_terminals()
    }

    /// Shut down all terminal sessions and cancel all tasks.
    /// Called by `Panel::on_removed`, terminal-tab close, and error paths.
    /// Idempotent.
    pub(crate) fn shutdown(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        for view in self.tree.terminal_views() {
            view.update(cx, |v, cx| v.shutdown(cx));
        }
    }

    /// Put every terminal Space of this tab in `channel`.
    pub(crate) fn join_tab_to_channel(&mut self, channel: InputChannel, cx: &mut Context<Self>) {
        for view in self.tree.terminal_views() {
            view.update(cx, |view, cx| view.join_channel(channel, cx));
        }
    }

    /// Take every terminal Space of this tab out of its channel.
    pub(crate) fn leave_tab_channels(&mut self, cx: &mut Context<Self>) {
        for view in self.tree.terminal_views() {
            view.update(cx, |view, cx| view.leave_channel(cx));
        }
    }

    /// Remove every Space, in any tab, from the channel of the active Space.
    fn close_active_channel(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.active_view() else {
            return;
        };
        let Some(channel) = view.update(cx, |view, cx| view.channel(cx)) else {
            return;
        };
        let Some(registry) = self.deps.input_channels.clone() else {
            return;
        };
        registry.update(cx, |registry, cx| {
            registry.close(channel, cx);
        });
    }

    /// Terminal Spaces in this tab (empty Spaces cannot join a channel).
    pub(crate) fn terminal_space_count(&self) -> usize {
        self.tree.terminal_views().len()
    }

    /// The distinct channels of this tab's Spaces, ordered A..E — the chips.
    pub(crate) fn tab_channels(&self, cx: &App) -> Vec<InputChannel> {
        let Some(registry) = &self.deps.input_channels else {
            return Vec::new();
        };
        let ids: Vec<EntityId> = self
            .tree
            .terminal_views()
            .iter()
            .map(|view| view.entity_id())
            .collect();
        registry.read(cx).channels_in(&ids)
    }

    /// Run an edit command (copy/paste/select-all/clear) on the active
    /// terminal's session, if the active Space has one.
    fn edit_active(&self, edit: edit::EditCommand, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = self.active_view() {
            let (session, origin) = view.update(cx, |view, cx| {
                (view.session.clone(), view.broadcast_origin(cx))
            });
            edit(&session, &origin, window, cx);
        }
    }
}

impl EventEmitter<PanelEvent> for TerminalPanel {}

impl Focusable for TerminalPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        // The tab group owns this proxy. Its focus listener forwards to the
        // active Space's independently tracked content handle in the next frame,
        // so accessibility never sees both frames claim one focused handle.
        self.dock_focus_handle.clone()
    }
}

impl Render for TerminalPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = self.tree.render(
            cx.entity().downgrade(),
            self.deps.input_channels.as_ref(),
            window,
            cx,
        );
        div()
            .id("terminal-panel")
            .size_full()
            .bg(cx.theme().background)
            // Terminal context-menu action handlers — also fired by global key bindings.
            .on_action(cx.listener(|this, _: &ClosePanel, w, cx| this.close_tab(w, cx)))
            .on_action(cx.listener(|this, _: &DuplicateSession, w, cx| {
                this.duplicate_session(this.tree.active(), w, cx)
            }))
            .on_action(cx.listener(|this, _: &SplitRight, w, cx| {
                this.split_active_at(this.tree.active(), SplitDir::Right, w, cx)
            }))
            .on_action(cx.listener(|this, _: &SplitLeft, w, cx| {
                this.split_active_at(this.tree.active(), SplitDir::Left, w, cx)
            }))
            .on_action(cx.listener(|this, _: &SplitUp, w, cx| {
                this.split_active_at(this.tree.active(), SplitDir::Up, w, cx)
            }))
            .on_action(cx.listener(|this, _: &SplitDown, w, cx| {
                this.split_active_at(this.tree.active(), SplitDir::Down, w, cx)
            }))
            .on_action(cx.listener(|this, _: &TerminalCopy, w, cx| {
                this.edit_active(edit::copy_selection, w, cx)
            }))
            .on_action(cx.listener(|this, _: &TerminalPaste, w, cx| {
                this.edit_active(edit::paste_clipboard, w, cx)
            }))
            .on_action(cx.listener(|this, _: &TerminalSelectAll, w, cx| {
                this.edit_active(edit::select_all, w, cx)
            }))
            .on_action(cx.listener(|this, _: &TerminalClear, w, cx| {
                this.edit_active(edit::clear_screen, w, cx)
            }))
            .on_action(cx.listener(|this, action: &JoinInputChannel, _w, cx| {
                if let Some(view) = this.active_view() {
                    view.update(cx, |view, cx| view.join_channel(action.0, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &LeaveInputChannel, _w, cx| {
                if let Some(view) = this.active_view() {
                    view.update(cx, |view, cx| view.leave_channel(cx));
                }
            }))
            .on_action(cx.listener(|this, _: &CloseInputChannel, _w, cx| {
                this.close_active_channel(cx);
            }))
            .on_action(cx.listener(|this, _: &CloseSpace, w, cx| {
                // Closing the only Space would empty the tab; that is what
                // "Close Terminal Tab" is for.
                if this.tree.leaf_count() > 1 {
                    this.close_space(this.tree.active(), w, cx);
                }
            }))
            .child(body)
    }
}

impl gpui_base::dock::Panel for TerminalPanel {
    fn panel_name(&self) -> &'static str {
        "terminal"
    }

    fn closable(&self, _: &App) -> bool {
        false
    }

    fn on_added_to(
        &mut self,
        tab_panel: WeakEntity<TabGroup>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.tab_panel = Some(tab_panel);
    }

    fn on_removed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Close all sessions and cancel all tasks when the panel is removed
        // from the TabGroup (tab close button, middle-click, drag removal).
        self.shutdown(window, cx);
    }

    fn set_active(&mut self, active: bool, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_active != active {
            self.is_active = active;
            cx.notify();
        }
        // Becoming the selected tab always republishes, even when the active
        // Space did not change: another tab published over this state.
        if active {
            self.publish_active_session(cx);
        }
    }
}

impl Panel for TerminalPanel {
    fn inner_padding(&self, _: &App) -> bool {
        false
    }

    fn title(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        super::tab_title::render_tab_strip(self, window, cx)
    }

    fn zoom_control(&self, _: &App) -> Option<PanelControl> {
        Some(PanelControl::Both)
    }

    /// Rows this panel adds to the tab bar's `...` menu, above the kit's own
    /// separator: the tab list, so a tab that does not fit the strip is still
    /// reachable (`US-0116`). The kit's zoom and close rows below it are its
    /// own — their wording and their presence are not ours to change (see
    /// `docs/gui-layout.md`).
    fn dropdown_menu(
        &mut self,
        menu: gpui_component::menu::PopupMenu,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_component::menu::PopupMenu {
        super::tab_title::tab_list_menu(menu, self, cx)
    }

    /// "+" button next to the zoom button — dropdown to spawn a new terminal tab
    /// with a specific shell, open the New SSH Session dialog, or open one of
    /// the saved SSH sessions (`IN-0033`).
    fn title_suffix(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let btn = Button::new("add-shell-tab")
            .icon(IconName::Plus)
            .xsmall()
            .ghost()
            .tab_stop(false)
            .tooltip("New Terminal")
            .dropdown_menu(|menu, window, cx| {
                let mut menu = menu;
                // Platform-specific shells.
                // Row labels come from `ShellKind::display_name`, the same list
                // the tab the row opens is named after (`US-0114`).
                #[cfg(windows)]
                const SHELLS: [ShellKind; 3] =
                    [ShellKind::Cmd, ShellKind::PowerShell, ShellKind::Pwsh];
                #[cfg(not(windows))]
                const SHELLS: [ShellKind; 3] = [ShellKind::Bash, ShellKind::Sh, ShellKind::Zsh];
                for kind in SHELLS {
                    menu = menu.menu(kind.display_name(), Box::new(AddPanelWithShell(kind)));
                }
                // M1 (`DEC-0019`): an elevated window runs local shells and
                // nothing else — no "Run as administrator" row either, since a
                // `runas` from an elevated process would only produce a second
                // identical window — no SSH Sessions heading, no saved sessions,
                // no Quick Connect, no New Saved Session. The menu ends here.
                if oneterm_core::elevation::is_elevated() {
                    return menu.scrollable(menu_scrolls(menu_rows(true, 0), window));
                }
                // One submenu row rather than three top-level rows, so the
                // owner-fixed order below the shell block is not disturbed and
                // "open a shell" never shares a hit target with "open an
                // elevated shell" (`US-0132`).
                #[cfg(windows)]
                {
                    menu = menu.submenu(
                        "Run as administrator",
                        window,
                        cx,
                        |mut submenu, _window, _cx| {
                            for kind in SHELLS {
                                submenu =
                                    submenu.item(PopupMenuItem::new(kind.display_name()).on_click(
                                        move |_, window, cx| {
                                            (oneterm_state::commands::commands(cx)
                                                .launch_elevated_shell)(
                                                kind, window, cx
                                            );
                                        },
                                    ));
                            }
                            submenu
                        },
                    );
                }
                // The sessions saved in `ssh_session.json`, so a saved host
                // opens from the same place a local shell does. This closure
                // runs on every open, so the list is never stale.
                let commands = oneterm_state::commands::commands(cx);
                menu = menu.item(labelled_separator("SSH Sessions", SeparatorRule::Solid));
                let saved = (commands.saved_ssh_sessions)(cx);
                let mut session_rows = 0;
                if saved.is_empty() {
                    menu = menu.item(PopupMenuItem::new("No saved sessions").disabled(true));
                    session_rows += 1;
                }
                for (group, rows) in saved {
                    if !group.is_empty() {
                        menu = menu.item(labelled_separator(group, SeparatorRule::Dashed));
                        session_rows += 1;
                    }
                    session_rows += rows.len();
                    for (id, name, color) in rows {
                        let open = commands.open_saved_ssh_session;
                        menu = menu.item(
                            saved_session_row(name, color)
                                .on_click(move |_, window, cx| open(id, window, cx)),
                        );
                    }
                }

                // Each closing row names the dialog it opens (`US-0114`, `F7`):
                // the action row still reaches quick connect, so the
                // `NewSession` key binding does not change meaning, and the new
                // row reaches the full session dialog the session tree opens.
                let open_saved_dialog = commands.open_new_saved_session_dialog;
                menu = menu
                    .separator()
                    .menu("Quick Connect...", Box::new(NewSession))
                    .item(
                        PopupMenuItem::new("New Saved Session...")
                            .on_click(move |_, window, cx| open_saved_dialog(window, cx)),
                    );

                menu.scrollable(menu_scrolls(menu_rows(false, session_rows), window))
            })
            .anchor(Anchor::TopRight);
        Some(btn)
    }
}
