//! Everything about a Terminal Tab's label: resolving it from the live OSC 0/2
//! title, the manual rename override and its dialog, and the tab-strip element
//! (recording dot, OSC 7501 status icon, active bar, drag source, middle-click /
//! × close, double-click rename). The vertical rail's row is the same element on two
//! lines (the title, then the current directory and the status icon) without the ×, or on one
//! centred line when the tab has no directory; the collapsed rail's cell is the shell icon on the
//! status colour, and its hover card draws those same lines.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, App, AppContext as _, Bounds, ClickEvent, Context, Div, Entity,
    EntityId, Focusable as _, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Pixels, Point, Render, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, Transformation, WeakEntity, Window, div,
    percentage, point, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, ElementExt as _, Icon, IconName, Sizable as _, Size, WindowExt as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::{DialogButtonProps, DialogFooter},
    h_flex,
    input::{Input, InputState},
    menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem},
    notification::NotificationType,
    tooltip::Tooltip,
    v_flex,
};
use oneterm_core::{SessionDuplicateConfig, ShellKind};
use oneterm_state::{TabMenuOpen, TabRowLayout};
use oneterm_terminal::{ProgramState, TerminalLogState};
use oneterm_theme::{AppIcon, notif_ext::notify};

use super::TerminalPanel;
use super::channel_menu::channel_submenu;
use crate::space::DragTerminalTab;
use crate::theme::channel_chip;

impl TerminalPanel {
    /// Return the effective tab label, with a manual override taking priority
    /// over the live OSC 0/2 title and the fallback shell label.
    pub(super) fn effective_tab_label(&self, live_title: Option<&str>) -> String {
        if let Some(title) = &self.tab_title_override {
            return title.clone();
        }
        resolve_tab_label(live_title, &self.tab_title)
    }

    /// Update the manual tab-title override.
    fn set_custom_tab_title(&mut self, title: String, cx: &mut Context<Self>) {
        self.tab_title_override = Some(title);
        cx.notify();
    }

    /// The directory line of a vertical tab row: the very string the status bar's breadcrumb
    /// shows for this tab (`None` while the shell has reported no directory).
    pub(super) fn tab_cwd(&self, cx: &App) -> Option<String> {
        self.breadcrumb_label(cx)
    }

    /// The OSC 7501 state the tab's status icon shows: only for an unsplit tab
    /// (a split tab has no room to say which Space reports it, owner ruling
    /// `IN-0049`), and `None` when its terminal reports nothing to show.
    pub(super) fn tab_status(&self, cx: &App) -> Option<ProgramState> {
        if !self.tree.is_single() {
            return None;
        }
        self.tree
            .active_terminal()
            .and_then(|view| view.read(cx).program_status())
    }

    /// The glyph a collapsed cell shows for the shell the tab's active Space runs (see
    /// [`shell_icon`]); the generic terminal glyph when the tab has no terminal.
    pub(super) fn tab_shell_icon(&self, cx: &App) -> AppIcon {
        let view = self.tree.active_terminal();
        shell_icon(
            view.as_ref()
                .and_then(|view| view.read(cx).duplicate_config.as_ref()),
        )
    }

    /// Whether the tab shows the recording dot: only an unsplit tab whose
    /// terminal is currently logging (a split tab has no room to say which
    /// Space is recording).
    fn shows_recording_dot(&self, cx: &App) -> bool {
        self.tree.is_single()
            && self.tree.active_terminal().is_some_and(|view| {
                view.read(cx)
                    .session
                    .read(cx)
                    .capabilities()
                    .logging
                    .is_some_and(|logging| {
                        matches!(logging.state(), TerminalLogState::Running { .. })
                    })
            })
    }
}

/// Resolve the tab label from the live OSC 0/2 title and the static fallback.
fn resolve_tab_label(live: Option<&str>, fallback: &str) -> String {
    match live.filter(|s| !s.is_empty()).map(trim_path_title) {
        Some(t) => t.to_string(),
        None => fallback.to_string(),
    }
}

/// The display name of a shell program path: its last component after either
/// separator — `/` **or** `\`, whatever the host OS thinks — with one trailing
/// `.exe` removed case-insensitively and nothing else. `None` when that leaves
/// nothing to name a tab after (a trailing separator, an empty program).
///
/// [`std::path::Path::file_stem`] cannot do this and used to: on Linux `\` is
/// not a separator, so `C:\Program Files\Git\bin\bash.exe` kept its
/// directories and the label read as the whole path, and it strips *any* last
/// extension, so `python3.11` would come out `python3`.
fn program_display_name(program: &str) -> Option<&str> {
    let last = program
        .rsplit(|c| c == '/' || c == '\\')
        .next()
        .unwrap_or(program);
    // `saturating_sub` + `get` also rejects a non-char-boundary cut.
    let name = match last.get(last.len().saturating_sub(4)..) {
        Some(ext) if ext.eq_ignore_ascii_case(".exe") => &last[..last.len() - 4],
        _ => last,
    };
    (!name.is_empty()).then_some(name)
}

/// The static tab label of a local-shell tab: the program's display name when
/// the settings name one, otherwise the shell kind's display name (`US-0114`).
/// A live OSC 0/2 title still wins over this — see [`resolve_tab_label`].
///
/// An explicit `program` wins for **every** kind, not only `Custom`, because
/// `resolve_shell` honours it for every kind: `kind: cmd` with
/// `program: nu.exe` runs nushell, and labelling that tab "Command Prompt"
/// would be the same untruth `F1` is about, in a rarer configuration
/// (`F-114.2`). The "+" menu clears `program` for an explicit kind, so only the
/// default-shell path can reach this.
pub(super) fn shell_tab_title(kind: ShellKind, program: Option<&Path>) -> String {
    if let Some(name) = program
        .and_then(|p| p.to_str())
        .and_then(program_display_name)
    {
        return name.to_string();
    }
    if kind == ShellKind::Custom {
        // A custom shell with no program has nothing to be named after.
        return super::terminal_panel::DEFAULT_TAB_TITLE.to_string();
    }
    kind.display_name().to_string()
}

/// Shorten a title that is just an absolute path to its last path component.
pub(crate) fn trim_path_title(title: &str) -> &str {
    let t = title.trim();
    let bytes = t.as_bytes();
    let is_abs = t.starts_with('/')
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/'));
    if !is_abs {
        return title;
    }
    match t.rsplit(|c| c == '\\' || c == '/').next() {
        Some(last) if !last.is_empty() => last,
        _ => title,
    }
}

/// The icon of a collapsed cell for the shell a terminal was launched with (`US-0198` rework 6):
/// `cmd` and PowerShell (Windows PowerShell or PowerShell 7) have their own; every other shell,
/// an SSH session and a terminal with no recorded launch data take the generic terminal glyph.
/// An explicit `program` names the shell, as it names the tab label, so `kind: cmd` running
/// `nu.exe` is not a `cmd` icon; with none, the kind decides. The window title is never read.
pub(super) fn shell_icon(config: Option<&SessionDuplicateConfig>) -> AppIcon {
    let Some(SessionDuplicateConfig::Local(shell)) = config else {
        return AppIcon::Terminal;
    };
    let program = shell
        .program
        .as_deref()
        .and_then(Path::to_str)
        .and_then(program_display_name);
    let by_name = |name: &str| match name.to_ascii_lowercase().as_str() {
        "cmd" => AppIcon::Cmd,
        "powershell" | "pwsh" => AppIcon::Powershell,
        _ => AppIcon::Terminal,
    };
    match (program, shell.kind) {
        (Some(name), _) => by_name(name),
        (None, ShellKind::Cmd) => AppIcon::Cmd,
        (None, ShellKind::PowerShell | ShellKind::Pwsh) => AppIcon::Powershell,
        (None, _) => AppIcon::Terminal,
    }
}

/// The background of a collapsed cell and the colour of its glyph for the tab's OSC 7501 state
/// (`US-0198` rework 6): the tokens [`tab_status_icon`] draws the state's icon in. The glyph is
/// whichever of the theme's `background` and `foreground` reads better on the fill (owner
/// ruling, 2026-10-22: the themes' own `*_foreground` tokens were below 3:1 in 23 of 39 variants,
/// so none is used). `None` (idle, or nothing reported) leaves the cell as it is.
pub(super) fn collapsed_fill(
    status: ProgramState,
    theme: &gpui_component::Theme,
) -> Option<(gpui::Hsla, gpui::Hsla)> {
    let fill = match status {
        ProgramState::Working => theme.primary,
        ProgramState::Blocked => theme.warning,
        ProgramState::Done => theme.success,
        ProgramState::Error => theme.danger,
        ProgramState::Idle => return None,
    };
    Some((
        fill,
        oneterm_theme::legible_on(fill, [theme.background, theme.foreground]),
    ))
}

/// The tab's OSC 7501 status icon (owner's icon set, `IN-0049`): a rotating
/// loader while working, a pulsing bell while waiting on the user, a check or a
/// cross for a result. Colours are theme tokens. The animations are gpui's, so a
/// reduced-motion setting draws them still and asks for no frames.
pub(super) fn tab_status_icon(status: ProgramState, panel: EntityId, cx: &App) -> gpui::AnyElement {
    let theme = cx.theme();
    let (icon, color, label) = match status {
        ProgramState::Working => (IconName::LoaderCircle, theme.primary, "Working"),
        ProgramState::Blocked => (IconName::Bell, theme.warning, "Waiting for you"),
        ProgramState::Done => (IconName::CircleCheck, theme.success, "Done"),
        ProgramState::Error => (IconName::CircleX, theme.danger, "Error"),
        // Never stored by the view; nothing to draw.
        ProgramState::Idle => return div().into_any_element(),
    };
    let icon = Icon::new(icon).xsmall().text_color(color);
    let icon = match status {
        ProgramState::Working => icon
            .with_animation(
                ("tab-status-working", panel.as_u64()),
                Animation::new(Duration::from_secs(1)).repeat(),
                |icon, delta| icon.transform(Transformation::rotate(percentage(delta))),
            )
            .into_any_element(),
        ProgramState::Blocked => div()
            .child(icon)
            .with_animation(
                ("tab-status-blocked", panel.as_u64()),
                Animation::new(Duration::from_millis(1200)).repeat(),
                |this, delta| {
                    // 1 -> 0.35 -> 1 over one period.
                    let wave = (delta * std::f32::consts::TAU).cos() * 0.5 + 0.5;
                    this.opacity(0.35 + 0.65 * wave)
                },
            )
            .into_any_element(),
        _ => icon.into_any_element(),
    };
    div()
        .id(("tab-status", panel.as_u64()))
        .debug_selector(|| TAB_STATUS_SELECTOR.to_string())
        .flex_shrink_0()
        .flex()
        .items_center()
        .child(icon)
        .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
        .into_any_element()
}

/// Names the tab row in the debug-bounds map, so a test can ask a really drawn frame where it
/// is. A no-op outside test builds.
pub(super) const TAB_ROW_SELECTOR: &str = "oneterm-tab-row";
/// Same, for the row's close button, status icon and label.
pub(super) const TAB_CLOSE_SELECTOR: &str = "oneterm-tab-close";
pub(super) const TAB_STATUS_SELECTOR: &str = "oneterm-tab-status";
pub(super) const TAB_LABEL_SELECTOR: &str = "oneterm-tab-label";
/// Same, for the current directory on line 2 of a vertical row.
pub(super) const TAB_CWD_SELECTOR: &str = "oneterm-tab-cwd";
/// Same, for the shell icon of a collapsed cell and for its status fill.
pub(super) const TAB_SHELL_ICON_SELECTOR: &str = "oneterm-tab-shell-icon";
pub(super) const TAB_FILL_SELECTOR: &str = "oneterm-tab-fill";
pub(super) const TAB_FILL_PILL_SELECTOR: &str = "oneterm-tab-fill-pill";
/// How far a collapsed cell's recording dot sits from its corner.
const COLLAPSED_DOT_INSET: f32 = 3.;

fn tab_title_label() -> Div {
    // GPUI creates a rectangular content mask when either overflow axis is
    // hidden, which clips glyph descenders to this label's line box.
    div().flex_1().min_w_0().text_ellipsis().whitespace_nowrap()
}

/// The directory on line 2 of a vertical row: muted, one line, and cut at its **start** when
/// the room runs out, so the folder the user is in stays visible (`…\src\panel`).
pub(super) fn tab_cwd_label(cwd: String, color: gpui::Hsla) -> Div {
    div()
        .debug_selector(|| TAB_CWD_SELECTOR.to_string())
        .min_w_0()
        .text_xs()
        .text_color(color)
        .whitespace_nowrap()
        .text_ellipsis_start()
        .child(cwd)
}

/// The widest a collapsed cell's hover card grows; narrower when its content is.
pub(super) const HOVER_CARD_MAX_WIDTH: f32 = 280.;
/// The gap between the strip's outer edge and the hover card.
pub(super) const HOVER_CARD_GAP: f32 = 6.;
/// The card's corner radius.
pub(super) const HOVER_CARD_RADIUS: f32 = 6.;
/// What the card adds around its lines, vertically: a 1 px border and 4 px of padding each side.
const HOVER_CARD_CHROME_Y: f32 = 10.;
/// Names the hover card's frame and title in the debug-bounds map. A no-op outside test builds.
#[cfg(test)]
pub(super) const TAB_CARD_SELECTOR: &str = "oneterm-tab-card";
#[cfg(test)]
pub(super) const TAB_CARD_LABEL_SELECTOR: &str = "oneterm-tab-card-label";

/// The lines of a vertical row, shared by the rail's row and the collapsed cell's hover card so
/// the two cannot drift apart. Line 1: the channel chips, the recording dot, the title. Line 2:
/// the directory (cut at its start when it does not fit) and the status icon at the end. The path
/// is as wide as its text up to the room there is, so it shrinks (and is cut) before the spacer
/// that keeps the status icon at the end. A tab with **no directory has no line 2**: line 1 is
/// the whole content, centred in a row that is taller than it, and the status icon ends it.
fn vertical_row_lines(
    chips: Vec<Stateful<Div>>,
    recording: Option<Stateful<Div>>,
    label: impl IntoElement,
    path: Option<Div>,
    status: Option<gpui::AnyElement>,
) -> Div {
    let title = h_flex()
        .h(px(TabRowLayout::VERTICAL_FIRST_LINE_HEIGHT))
        .gap_1()
        .children(chips)
        .children(recording)
        .child(label);
    match path {
        None => v_flex().justify_center().child(title.children(status)),
        Some(path) => v_flex().child(title).child(
            h_flex()
                .h(px(TabRowLayout::VERTICAL_SECOND_LINE_HEIGHT))
                .gap_1()
                .child(path)
                .child(div().flex_1().min_w_0())
                .children(status),
        ),
    }
}

/// The recording dot of a tab.
fn recording_dot(color: gpui::Hsla) -> Stateful<Div> {
    div()
        .id("tab-recording")
        .flex_shrink_0()
        .text_xs()
        .text_color(color)
        .child("●")
}

/// One chip per distinct channel of the tab's Spaces, in A..E order.
fn tab_chips(panel: &TerminalPanel, cx: &App) -> Vec<Stateful<Div>> {
    panel
        .tab_channels(cx)
        .into_iter()
        .enumerate()
        .map(|(index, channel)| channel_chip(channel, ("tab-channel", index), cx))
        .collect()
}

/// What the collapsed cell shows while the pointer rests on it: the expanded row's lines, read
/// from the live tab, in a popover frame (the kit's tooltip look: `popover` fill and
/// foreground, `border`, `shadow_md`; a 6 px radius) as wide as they are up to 280 px. The title
/// does not grow into spare room, so the card hugs its content, and is cut with an ellipsis
/// when the content is wider than the card may be.
pub(super) fn tab_hover_card(panel: &TerminalPanel, panel_id: EntityId, cx: &App) -> Div {
    let theme = cx.theme();
    let label = div().min_w_0().text_ellipsis().whitespace_nowrap();
    #[cfg(test)]
    let label = label.debug_selector(|| TAB_CARD_LABEL_SELECTOR.to_string());
    let lines = vertical_row_lines(
        tab_chips(panel, cx),
        panel
            .shows_recording_dot(cx)
            .then(|| recording_dot(theme.danger)),
        label.child(panel.tab_label(cx)),
        panel
            .tab_cwd(cx)
            .map(|cwd| tab_cwd_label(cwd, theme.muted_foreground)),
        panel
            .tab_status(cx)
            .map(|status| tab_status_icon(status, panel_id, cx)),
    );
    let card = div()
        .max_w(px(HOVER_CARD_MAX_WIDTH))
        .px_2()
        .py_1()
        .border_1()
        .border_color(theme.border)
        .rounded(px(HOVER_CARD_RADIUS))
        .bg(theme.tokens.popover)
        .text_color(theme.popover_foreground)
        .font_family(theme.font_family.clone())
        .text_sm()
        // As in a row (the kit's `Tab` sets it), so the 20 px line holds its 14 px text.
        .line_height(gpui::relative(1.))
        .shadow_md()
        .child(lines);
    #[cfg(test)]
    let card = card.debug_selector(|| TAB_CARD_SELECTOR.to_string());
    card
}

/// How tall the hover card is: its lines (one, or two with a directory) and its frame.
pub(super) fn hover_card_height(has_cwd: bool) -> Pixels {
    let lines = TabRowLayout::VERTICAL_FIRST_LINE_HEIGHT
        + if has_cwd {
            TabRowLayout::VERTICAL_SECOND_LINE_HEIGHT
        } else {
            0.
        };
    px(lines + HOVER_CARD_CHROME_Y)
}

/// Where the hover card goes relative to where gpui puts a native tooltip, which is 1 px right of
/// and below the pointer: its left edge `HOVER_CARD_GAP` right of the strip's outer edge (the
/// cell's right edge and the rail's own border) and its middle level with the cell's, whatever
/// the pointer's position in the cell. The result is the inset to give the card.
pub(super) fn hover_card_offset(
    cell: Bounds<Pixels>,
    pointer: Point<Pixels>,
    card_height: Pixels,
) -> Point<Pixels> {
    let origin = point(
        cell.right() + px(TabRowLayout::RAIL_BORDER_WIDTH + HOVER_CARD_GAP),
        cell.center().y - card_height / 2.,
    );
    origin - (pointer + point(px(1.), px(1.)))
}

/// The collapsed cell's tooltip, a gpui native one: it draws nothing while a right-click menu is
/// open on the cell, and otherwise the live tab's hover card at the spot [`hover_card_offset`]
/// works out. gpui lays a tooltip out at the pointer and flips it at the window's edge by its own
/// size, so the root is a point and the card is an absolute child, which cannot be flipped
/// the wrong way.
pub(super) struct CellTooltip {
    pub(super) panel: WeakEntity<TerminalPanel>,
    /// The cell's bounds, and the pointer, when the tooltip was built.
    pub(super) cell: Bounds<Pixels>,
    pub(super) pointer: Point<Pixels>,
    pub(super) hidden: Rc<Cell<bool>>,
}

impl Render for CellTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(panel) = self.panel.upgrade().filter(|_| !self.hidden.get()) else {
            return div().into_any_element();
        };
        let tab = panel.read(cx);
        let offset = hover_card_offset(
            self.cell,
            self.pointer,
            hover_card_height(tab.tab_cwd(cx).is_some()),
        );
        div()
            .size_0()
            .child(
                tab_hover_card(tab, panel.entity_id(), cx)
                    .absolute()
                    .left(offset.x)
                    .top(offset.y),
            )
            .into_any_element()
    }
}

/// The compact cell of the collapsed rail (`US-0198`): the tab's shell icon (16 px), centred, on
/// the tab's status colour, and the recording dot in the top-left corner. There is no status
/// badge and nothing animates: the state is the background. The title, directory, channel chips
/// and status icon are the hover card's ([`tab_hover_card`]).
///
/// The icon is plain content: the whole cell, icon included, is the rail's reorder drag
/// (`DragPanel`), which an empty Space accepts too (`US-0198` rework 8).
///
/// The fill is an absolute child over the whole cell, painted over the rail's hover fill (so a
/// filled cell takes no hover tint) and over the rail's accent pill, which a `Working` fill
/// (`primary`) could swallow anyway; a selected filled cell draws its own pill in the glyph's
/// colour instead.
fn collapsed_row(
    icon: AppIcon,
    fill: Option<(gpui::Hsla, gpui::Hsla)>,
    selected: bool,
    recording: Option<gpui::Stateful<Div>>,
) -> Div {
    let glyph = Icon::new(icon).with_size(Size::Medium);
    let glyph = match fill {
        Some((_, foreground)) => glyph.text_color(foreground),
        None => glyph,
    };
    div()
        .relative()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .justify_center()
        .children(fill.map(|(background, _)| {
            div()
                .debug_selector(|| TAB_FILL_SELECTOR.to_string())
                .absolute()
                .inset_0()
                .bg(background)
        }))
        // The rail's accent pill is under the fill, so a selected filled cell draws its own, in
        // the glyph's colour (the better of `background` and `foreground` on the fill), which
        // reads on a fill of any colour, `primary` included.
        .children(fill.filter(|_| selected).map(|(_, glyph)| {
            div()
                .debug_selector(|| TAB_FILL_PILL_SELECTOR.to_string())
                .absolute()
                .left_0()
                .top(px(TabRowLayout::ACCENT_BAR_INSET))
                .bottom(px(TabRowLayout::ACCENT_BAR_INSET))
                .w(px(TabRowLayout::ACCENT_BAR_WIDTH))
                .rounded(px(TabRowLayout::ACCENT_BAR_WIDTH / 2.))
                .bg(glyph)
        }))
        .child(
            div()
                .debug_selector(|| TAB_SHELL_ICON_SELECTOR.to_string())
                .flex()
                .child(glyph),
        )
        .children(recording.map(|dot| {
            div()
                .absolute()
                .top(px(COLLAPSED_DOT_INSET))
                .left(px(COLLAPSED_DOT_INSET))
                .child(dot)
        }))
}

/// The size of the New Terminal `+` button: the strip's 20 px, or 24 px in the rail, whose header
/// has room for the larger target (the rail draws the button under its own layout).
pub(super) fn new_tab_button_size(layout: TabRowLayout) -> Size {
    match layout {
        TabRowLayout::Horizontal => Size::XSmall,
        TabRowLayout::Vertical | TabRowLayout::Collapsed => Size::Small,
    }
}

/// Start dragging a tab: the preview is the title chip, and the payload is what an empty Space
/// accepts.
fn start_tab_drag(
    drag: &DragTerminalTab,
    _: Point<Pixels>,
    _: &mut Window,
    cx: &mut App,
) -> Entity<DragTerminalTab> {
    cx.stop_propagation();
    cx.new(|_| drag.clone())
}

/// Build the tab-strip row for `panel`: active bar, drag source, middle-click
/// close, recording dot, status icon, the (double-click to rename) label, and
/// the × button. The vertical rail's row (`IN-0055`, `US-0196` rework) has two
/// lines and no ×: the label after the chips and the dot, then the current
/// directory with the status icon at the end of the line; the tab closes by
/// middle-click and the context menu. The collapsed rail's cell (`US-0198`) is the shell icon on
/// the status colour, with the expanded row's content as a hover card.
pub(super) fn render_tab_strip(
    panel: &mut TerminalPanel,
    _window: &mut Window,
    cx: &mut Context<TerminalPanel>,
) -> impl IntoElement {
    let layout = TabRowLayout::current(cx);
    let vertical = layout == TabRowLayout::Vertical;
    let collapsed = layout == TabRowLayout::Collapsed;
    let tab_panel = panel.tab_panel.clone();
    let tooltip_hidden = panel.tab_tooltip_hidden.clone();
    let panel_entity = cx.entity().clone();
    let panel_weak = cx.entity().downgrade();
    let muted = cx.theme().muted_foreground;
    let highlight = oneterm_theme::selection_accent(cx);
    let recording_color = cx.theme().danger;
    let is_active = panel.is_active;
    let show_recording = panel.shows_recording_dot(cx);
    let status = panel.tab_status(cx);
    // A collapsed cell shows the state as its background instead of an icon.
    let status_icon = status
        .filter(|_| !collapsed)
        .map(|status| tab_status_icon(status, panel_entity.entity_id(), cx));
    // The directory is shown by the vertical row only.
    let cwd = vertical.then(|| panel.tab_cwd(cx)).flatten();
    let chips = tab_chips(panel, cx);
    let tab_label = panel.tab_label(cx);
    let drag_title: SharedString = tab_label.clone().into();
    let rename_title = tab_label.clone();
    let menu_title = tab_label.clone();
    let menu_panel = panel_entity.clone();
    let title_label_id = SharedString::from(format!("tab-title-label-{panel_entity:?}"));
    // Per-panel, like the label's id below it: the context menu derives its own
    // element id from this one, and a shared id would give every tab in the
    // strip the same menu state.
    let title_row_id = SharedString::from(format!("tab-title-{panel_entity:?}"));

    // The recording dot is `danger`, which a collapsed cell's status fill can be, so on a filled
    // cell it takes the glyph's colour.
    let fill = collapsed
        .then(|| status.and_then(|status| collapsed_fill(status, cx.theme())))
        .flatten();
    let dot_color = fill.map_or(recording_color, |(_, glyph)| glyph);
    let recording_dot = show_recording.then(|| recording_dot(dot_color));
    let label = tab_title_label()
        .id(title_label_id)
        .debug_selector(|| TAB_LABEL_SELECTOR.to_string())
        .on_click({
            let panel = panel_entity.clone();
            move |event, window, cx| {
                if event.click_count() != 2 {
                    return;
                }
                cx.stop_propagation();
                open_tab_title_dialog(panel.clone(), rename_title.clone(), window, cx);
            }
        })
        .child(tab_label);

    // What an empty Space accepts from here. A rail row and a collapsed cell start none (the
    // whole row or cell is the rail's reorder drag, `DragPanel`, which an empty Space accepts
    // too, `US-0196` rework 9, `US-0198` rework 8); the strip's tab starts it from the whole row.
    let row_drag = (layout == TabRowLayout::Horizontal)
        .then(|| {
            tab_panel.as_ref().map(|_| DragTerminalTab {
                panel: panel_weak.clone(),
                title: drag_title.clone(),
            })
        })
        .flatten();

    let row = if collapsed {
        collapsed_row(panel.tab_shell_icon(cx), fill, is_active, recording_dot)
    } else if vertical {
        // The rail draws the selected row's accent pill and starts the row's reorder drag itself,
        // so this row carries neither the top bar, nor the strip's negative margin, nor a drag.
        // The selected row's fill is too dark for `muted.foreground` in some light themes.
        let path_color = if is_active {
            oneterm_theme::selected_row_secondary(cx.theme())
        } else {
            muted
        };
        let path = cwd.map(|cwd| tab_cwd_label(cwd, path_color));
        vertical_row_lines(chips, recording_dot, label, path, status_icon)
            .flex_1()
            .min_w_0()
            .pl(px(TabRowLayout::VERTICAL_ROW_PADDING_LEFT))
            .pr(px(TabRowLayout::VERTICAL_ROW_PADDING_RIGHT))
            .py(px(TabRowLayout::VERTICAL_ROW_PADDING))
    } else {
        // No `w_full()`: the kit's `Tab` is content-sized and `flex_shrink_0`
        // (`component/src/tab/tab.rs:715-718,794-800`), so a percentage width
        // against it contributes nothing to intrinsic sizing and every tab
        // collapsed to `min_w` whatever its label said — which is how the
        // leftmost tab ended up clipped to a bare `×` with free space beside
        // the strip (`US-0116`, `F11`). Sized by the label between a floor and
        // a ceiling instead: no tab loses its name, and one long OSC title
        // elides rather than eating the strip.
        h_flex()
            .min_w(px(100.))
            .max_w(px(220.))
            .mr(-px(5.))
            .gap_1()
            .when(is_active, |this| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .left(-px(20.))
                        .right(-px(20.))
                        .h(px(2.))
                        .bg(highlight),
                )
            })
            .children(chips)
            .children(recording_dot)
            .children(status_icon)
            .child(label)
            .when_some(tab_panel.clone(), |this, _| {
                this.child(
                    div()
                        .id("tab-close")
                        .debug_selector(|| TAB_CLOSE_SELECTOR.to_string())
                        .flex_shrink_0()
                        .cursor_pointer()
                        .size_4()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(3.))
                        .hover(move |this| this.bg(muted.opacity(0.15)))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_click({
                            let panel_entity = panel_entity.clone();
                            move |_, window, cx| {
                                cx.stop_propagation();
                                panel_entity.update(cx, |panel, cx| panel.close_tab(window, cx));
                            }
                        })
                        .child(Icon::new(IconName::Close).xsmall().text_color(muted)),
                )
            })
    };

    row.id(title_row_id)
        .debug_selector(|| TAB_ROW_SELECTOR.to_string())
        .relative()
        .h_full()
        // A collapsed cell has no room for the title, directory or chips, so a hover card with
        // the expanded row's content carries them.
        .when(collapsed, |this| {
            let card_panel = panel_weak.clone();
            let (shown, entered, pressed) = (
                tooltip_hidden.clone(),
                tooltip_hidden.clone(),
                tooltip_hidden.clone(),
            );
            // Where the cell is, for placing the card beside the strip.
            let cell = Rc::new(Cell::new(Bounds::<Pixels>::default()));
            let measured = cell.clone();
            this.on_prepaint(move |bounds, _, _| measured.set(bounds))
                .tooltip(move |window, cx| {
                    // Built from the live panel each time the card is drawn.
                    let tooltip = CellTooltip {
                        panel: card_panel.clone(),
                        cell: cell.get(),
                        pointer: window.mouse_position(),
                        hidden: shown.clone(),
                    };
                    cx.new(|_| tooltip).into()
                })
                // gpui clears a tooltip on a press but shows it again on the next pointer move
                // while the cell is hovered, which put it on top of the menu a right-click opens,
                // so a right-press hides it until the pointer comes back to the cell.
                .on_hover(move |hovered, _, _| {
                    if *hovered {
                        entered.set(false);
                    }
                })
                .on_mouse_down(MouseButton::Right, move |_, window, _| {
                    pressed.set(true);
                    window.refresh();
                })
        })
        // Drag the strip's tab into an empty Space with the terminal-specific
        // payload consumed by Space-tree drop targets.
        .when_some(row_drag, |this, drag| this.on_drag(drag, start_tab_drag))
        // Middle-click on a tab → close that tab.
        .on_mouse_down(MouseButton::Middle, {
            let panel = panel_entity.clone();
            move |_, window, cx| {
                cx.stop_propagation();
                panel.update(cx, |panel, cx| panel.close_tab(window, cx));
            }
        })
        // The kit attaches no right-click handler to a tab and stops only left
        // mouse-down (`base/src/tabs.rs:172`), so the tab's own menu lives here,
        // on the content OneTerm renders (`US-0116`, `F12`).
        .context_menu(move |menu, window, cx| {
            tab_context_menu(
                menu,
                menu_panel.clone(),
                menu_title.clone(),
                layout,
                window,
                cx,
            )
        })
}

/// Which sibling tabs a bulk close acts on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum CloseScope {
    /// Every tab but the one the menu was opened on.
    Others,
    /// Every tab after the one the menu was opened on.
    ToTheRight,
}

/// The tabs a bulk close removes, as positions in the tab list, in the order
/// they must be closed: right to left, so removing one never shifts a position
/// still to come.
pub(super) fn tabs_to_close(total: usize, target: usize, scope: CloseScope) -> Vec<usize> {
    if target >= total {
        return Vec::new();
    }
    let mut victims: Vec<usize> = match scope {
        CloseScope::Others => (0..total).filter(|ix| *ix != target).collect(),
        CloseScope::ToTheRight => (target + 1..total).collect(),
    };
    victims.reverse();
    victims
}

/// The terminal tabs of `panel`'s tab group, each with its index in the group,
/// plus `panel`'s own position in that list.
///
/// The group can in principle hold panels of other kinds; those are dropped, so
/// a position in the returned list is not always the group index — which is why
/// both are carried.
fn sibling_tabs(
    panel: &Entity<TerminalPanel>,
    cx: &App,
) -> Option<(Vec<(usize, Entity<TerminalPanel>)>, usize)> {
    tabs_of(&panel.read(cx), panel.entity_id(), cx)
}

/// Same, for a caller that already holds the panel borrowed — reading it back
/// through its entity there is a double lease and panics.
fn tabs_of(
    panel: &TerminalPanel,
    panel_id: EntityId,
    cx: &App,
) -> Option<(Vec<(usize, Entity<TerminalPanel>)>, usize)> {
    let group = panel.tab_panel.as_ref()?.upgrade()?;
    let tabs: Vec<(usize, Entity<TerminalPanel>)> = group
        .read(cx)
        .panels()
        .iter()
        .enumerate()
        .filter_map(|(ix, p)| Some((ix, p.view().downcast::<TerminalPanel>().ok()?)))
        .collect();
    let position = tabs
        .iter()
        .position(|(_, tab)| tab.entity_id() == panel_id)?;
    Some((tabs, position))
}

/// Close every tab in `victims`, after a confirmation when more than one **tab**
/// would go — tabs, not live shells: a tab whose shell already exited counts the
/// same, which is what the dialog's own wording says. The `×`, middle-click and
/// Close all destroy exactly one tab and are unconfirmed, but a single menu row
/// that ends nine of them is the case the application confirms elsewhere too
/// (`CORR-34`).
fn close_tabs(victims: Vec<Entity<TerminalPanel>>, window: &mut Window, cx: &mut App) {
    fn close_all(victims: &[Entity<TerminalPanel>], window: &mut Window, cx: &mut App) {
        for tab in victims {
            tab.update(cx, |tab, cx| tab.close_tab(window, cx));
        }
    }

    match victims.len() {
        0 => {}
        1 => close_all(&victims, window, cx),
        n => {
            let description = format!("Close {n} terminal tabs? Their sessions end.");
            window.open_alert_dialog(cx, move |alert, _, _| {
                let victims = victims.clone();
                alert
                    .confirm()
                    .title("Close Tabs")
                    .description(description.clone())
                    .button_props(
                        DialogButtonProps::default()
                            .ok_text("Close")
                            .ok_variant(ButtonVariant::Danger)
                            .cancel_text("Cancel")
                            .show_cancel(true),
                    )
                    .on_ok(move |_, window, cx| {
                        close_all(&victims, window, cx);
                        true
                    })
            });
        }
    }
}

/// A bulk-close row, disabled when it would close nothing.
fn close_scope_row(
    label: &'static str,
    panel: Entity<TerminalPanel>,
    scope: CloseScope,
    cx: &App,
) -> PopupMenuItem {
    let victims: Vec<Entity<TerminalPanel>> = sibling_tabs(&panel, cx)
        .map(|(tabs, position)| {
            tabs_to_close(tabs.len(), position, scope)
                .into_iter()
                .map(|ix| tabs[ix].1.clone())
                .collect()
        })
        .unwrap_or_default();
    let empty = victims.is_empty();
    // A disabled row carries no click path: the kit's `confirm` ignores `disabled`.
    PopupMenuItem::new(label)
        .disabled(empty)
        .when(!empty, |item| {
            item.on_click(move |_, window, cx| close_tabs(victims.clone(), window, cx))
        })
}

/// The tab's right-click menu (`US-0116`). Every row acts on `panel` — the tab
/// that was right-clicked — and not on whichever tab happens to be active.
fn tab_context_menu(
    menu: PopupMenu,
    panel: Entity<TerminalPanel>,
    label: String,
    layout: TabRowLayout,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    // The vertical rail keeps this tab's row highlighted while the menu is up: the menu's
    // overlay takes the pointer, so the row stops being hovered (`US-0196` rework 8). The menu
    // builder runs once per opening; the menu says when it is gone (Escape, a click outside, a
    // chosen item), and a late close does not clear a newer menu's record.
    let (tab, menu_id) = (panel.entity_id().as_u64(), cx.entity_id().as_u64());
    TabMenuOpen::opened(tab, menu_id, cx);
    cx.subscribe_self(move |_, _: &gpui::DismissEvent, cx| TabMenuOpen::closed(menu_id, cx))
        .detach();
    // A menu that goes without being dismissed (its window closes, or a newer menu replaces it)
    // is released instead; the record is per menu, so a newer menu's record is left alone.
    cx.on_release(move |_, cx| TabMenuOpen::closed(menu_id, cx))
        .detach();
    let menu = menu
        .item(PopupMenuItem::new("Rename...").on_click({
            let panel = panel.clone();
            move |_, window, cx| open_tab_title_dialog(panel.clone(), label.clone(), window, cx)
        }))
        .item(PopupMenuItem::new("Duplicate").on_click({
            let panel = panel.clone();
            move |_, window, cx| {
                panel.update(cx, |panel, cx| {
                    let space = panel.tree.active();
                    panel.duplicate_session(space, window, cx);
                });
            }
        }));
    // After its own separator; absent for a tab with no terminal (`US-0199`).
    channel_submenu(menu, panel.clone(), window, cx)
        .separator()
        .item(PopupMenuItem::new("Close").on_click({
            let panel = panel.clone();
            move |_, window, cx| {
                panel.update(cx, |panel, cx| panel.close_tab(window, cx));
            }
        }))
        .item(close_scope_row(
            "Close Others",
            panel.clone(),
            CloseScope::Others,
            cx,
        ))
        .item(close_scope_row(
            close_following_label(layout),
            panel,
            CloseScope::ToTheRight,
            cx,
        ))
}

/// The wording of the menu row that closes the tabs after this one: they are to the right in
/// the strip and below in the vertical rail (same command, same tabs).
pub(super) fn close_following_label(layout: TabRowLayout) -> &'static str {
    match layout {
        TabRowLayout::Horizontal => "Close to the Right",
        TabRowLayout::Vertical | TabRowLayout::Collapsed => "Close Tabs Below",
    }
}

/// The tab list the `...` menu opens with: one row per terminal tab, the active
/// one marked, clicking a row shows that tab (`US-0116`, `F11`/`F13`).
///
/// It lives in the `...` menu because that is the one menu a panel can add rows
/// to, above the kit's own separator (`component/src/dock/tab_panel.rs:333-337`),
/// and because the menu
/// otherwise held a single row duplicating the zoom button beside it.
///
/// `panel` is the group's active panel and is already borrowed by the caller
/// (`Panel::dropdown_menu` takes `&mut self`), so its own row must not read it
/// back through its entity — that is a double lease and panics.
pub(super) fn tab_list_menu(
    menu: PopupMenu,
    panel: &TerminalPanel,
    cx: &mut Context<TerminalPanel>,
) -> PopupMenu {
    let panel_id = cx.entity_id();
    let Some((tabs, _)) = tabs_of(panel, panel_id, cx) else {
        return menu;
    };
    let Some(group) = panel.tab_panel.as_ref().and_then(|g| g.upgrade()) else {
        return menu;
    };
    let active = group.read(cx).active_ix();
    let mut menu = menu.label("Terminal Tabs");
    for (ix, tab) in tabs {
        let label = if tab.entity_id() == panel_id {
            panel.tab_label(cx)
        } else {
            tab.read(cx).tab_label(cx)
        };
        let group = group.downgrade();
        menu = menu.item(PopupMenuItem::new(label).checked(ix == active).on_click(
            move |_, window, cx| {
                let _ = group.update(cx, |group, cx| group.select_tab(ix, window, cx));
            },
        ));
    }
    menu.separator()
}

/// Open the rename-tab dialog for a terminal panel. An empty title keeps the
/// dialog open with a warning; Cancel never saves.
fn open_tab_title_dialog(
    panel: Entity<TerminalPanel>,
    current_title: String,
    window: &mut Window,
    cx: &mut App,
) {
    let title_state = cx.new(|cx| {
        let mut st = InputState::new(window, cx).placeholder("Tab title");
        st.set_value(current_title.clone(), window, cx);
        st
    });

    // Shared by the Save button and the dialog's keyboard OK; returns whether
    // the dialog may close.
    let save: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
        let title_state = title_state.clone();
        move |_, window, cx| {
            let new_title = title_state.read(cx).value().trim().to_string();
            if new_title.is_empty() {
                window.push_notification(
                    notify(NotificationType::Warning, "Tab title cannot be empty.", cx),
                    cx,
                );
                return false;
            }
            panel.update(cx, |panel, cx| {
                panel.set_custom_tab_title(new_title.clone(), cx)
            });
            true
        }
    });

    window.open_dialog(cx, move |dialog, window, cx| {
        let save_for_click = save.clone();
        let save_for_kb = save.clone();
        let focus_handle = title_state.read(cx).focus_handle(cx);
        focus_handle.focus(window, cx);

        dialog
            .title("Rename Tab")
            .w(px(440.))
            .content({
                let title_state = title_state.clone();
                move |content, _window, cx| {
                    content.child(
                        div()
                            .gap_1()
                            .w_full()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().foreground)
                                    .child("Tab title"),
                            )
                            .child(Input::new(&title_state)),
                    )
                }
            })
            .footer(
                DialogFooter::new()
                    .child(Button::new("cancel").label("Cancel").outline().on_click(
                        |_, window, cx| {
                            window.close_dialog(cx);
                        },
                    ))
                    .child(Button::new("save").label("Save").primary().on_click(
                        move |_, window, cx| {
                            if save_for_click(&ClickEvent::default(), window, cx) {
                                window.close_dialog(cx);
                            }
                        },
                    )),
            )
            .button_props(
                DialogButtonProps::default()
                    .on_cancel(|_, _, _| true)
                    .on_ok(move |_, window, cx| save_for_kb(&ClickEvent::default(), window, cx)),
            )
    });
}

#[cfg(test)]
mod tests {
    use gpui::Styled as _;

    use std::path::Path;

    use oneterm_core::ShellKind;

    use super::{
        CloseScope, program_display_name, resolve_tab_label, shell_tab_title, tab_title_label,
        tabs_to_close, trim_path_title,
    };

    #[test]
    fn close_others_keeps_the_right_clicked_tab_and_closes_right_to_left() {
        // Right-clicked the third of five. Descending order matters: closing
        // left to right would shift every position still to come.
        assert_eq!(
            tabs_to_close(5, 2, CloseScope::Others),
            vec![4, 3, 1, 0],
            "must never contain the target, and must descend"
        );
        // A lone tab has no others.
        assert_eq!(tabs_to_close(1, 0, CloseScope::Others), Vec::<usize>::new());
    }

    #[test]
    fn close_to_the_right_stops_at_the_right_clicked_tab() {
        assert_eq!(tabs_to_close(5, 2, CloseScope::ToTheRight), vec![4, 3]);
        // The last tab has nothing to its right, so the row closes nothing.
        assert_eq!(
            tabs_to_close(5, 4, CloseScope::ToTheRight),
            Vec::<usize>::new()
        );
        assert_eq!(tabs_to_close(3, 0, CloseScope::ToTheRight), vec![2, 1]);
    }

    #[test]
    fn a_target_outside_the_list_closes_nothing() {
        // A tab removed between the menu opening and the row being clicked.
        assert_eq!(tabs_to_close(3, 3, CloseScope::Others), Vec::<usize>::new());
        assert_eq!(tabs_to_close(0, 0, CloseScope::Others), Vec::<usize>::new());
        assert_eq!(
            tabs_to_close(3, 9, CloseScope::ToTheRight),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn each_shell_kind_gets_its_own_tab_label() {
        let kinds = [
            ShellKind::Cmd,
            ShellKind::PowerShell,
            ShellKind::Pwsh,
            ShellKind::Bash,
            ShellKind::Zsh,
            ShellKind::Sh,
        ];
        let labels: Vec<String> = kinds.iter().map(|k| shell_tab_title(*k, None)).collect();

        assert_eq!(labels[0], "Command Prompt");
        assert_eq!(labels[1], "PowerShell");
        assert_eq!(labels[2], "PowerShell 7");
        // F1 is precisely that ten local tabs read alike: no two kinds may
        // share a label.
        let mut unique = labels.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(
            unique.len(),
            labels.len(),
            "duplicate shell labels: {labels:?}"
        );
        assert!(!labels.iter().any(|l| l == "Terminal"));
    }

    #[test]
    fn a_program_name_is_read_the_same_way_on_every_os() {
        // The two tests below use Windows-style paths, and `Path::file_stem`
        // kept their directories on Linux (CI, "Full workspace quality gate").
        // This table is the OS-independent proof: both separator styles, and
        // only a trailing `.exe` comes off.
        let cases = [
            ("bash", Some("bash")),
            ("cmd", Some("cmd")),
            ("nu", Some("nu")),
            ("pwsh.exe", Some("pwsh")),
            ("PWSH.EXE", Some("PWSH")),
            ("C:\\Program Files\\Git\\bin\\bash.exe", Some("bash")),
            ("C:\\tools\\nu", Some("nu")),
            ("/usr/local/bin/fish", Some("fish")),
            ("/opt/shells/pwsh.exe", Some("pwsh")),
            // Only `.exe`: a version suffix is part of the name.
            ("/usr/bin/python3.11", Some("python3.11")),
            // Nothing left to name the tab after.
            ("C:\\tools\\", None),
            ("/usr/bin/", None),
            (".exe", None),
            ("", None),
        ];

        for (program, expected) in cases {
            assert_eq!(program_display_name(program), expected, "{program:?}");
        }
    }

    #[test]
    fn custom_shell_is_named_after_its_program() {
        assert_eq!(
            shell_tab_title(
                ShellKind::Custom,
                Some(Path::new("C:\\Program Files\\Git\\bin\\bash.exe"))
            ),
            "bash"
        );
        assert_eq!(
            shell_tab_title(ShellKind::Custom, Some(Path::new("/usr/bin/fish"))),
            "fish"
        );
        // Nothing to name it after — the reset-tab fallback.
        assert_eq!(shell_tab_title(ShellKind::Custom, None), "Terminal");
    }

    #[test]
    fn an_explicit_program_wins_over_the_kinds_name() {
        // `resolve_shell` honours `program` for every kind, so `kind: cmd` with
        // `program: nu.exe` runs nushell — and must not read "Command Prompt".
        assert_eq!(
            shell_tab_title(ShellKind::Cmd, Some(Path::new("C:\\tools\\nu.exe"))),
            "nu"
        );
        assert_eq!(
            shell_tab_title(ShellKind::Bash, Some(Path::new("/usr/bin/fish"))),
            "fish"
        );
        // ...and with no program the kind still names the tab.
        assert_eq!(shell_tab_title(ShellKind::Cmd, None), "Command Prompt");
    }

    #[test]
    fn a_live_osc_title_still_wins_over_the_shell_name() {
        let fallback = shell_tab_title(ShellKind::PowerShell, None);

        assert_eq!(
            resolve_tab_label(Some("vim - main.rs"), &fallback),
            "vim - main.rs"
        );
        // ...and with no live title the shell name is what shows.
        assert_eq!(resolve_tab_label(None, &fallback), "PowerShell");
        assert_eq!(resolve_tab_label(Some(""), &fallback), "PowerShell");
    }

    #[test]
    fn tab_title_label_does_not_create_a_content_mask() {
        let mut label = tab_title_label();
        let overflow = &label.style().overflow;

        assert_eq!(overflow.x, None);
        assert_eq!(overflow.y, None);
    }

    #[test]
    fn live_title_is_used() {
        assert_eq!(
            resolve_tab_label(Some("vim — main.rs"), "Terminal"),
            "vim — main.rs"
        );
        assert_eq!(
            resolve_tab_label(Some("user@host: ~/repo"), "user@host:24"),
            "user@host: ~/repo"
        );
        assert_eq!(resolve_tab_label(Some("cmd.exe"), "Terminal"), "cmd.exe");
    }

    #[test]
    fn none_falls_back_to_static_label() {
        assert_eq!(resolve_tab_label(None, "Terminal"), "Terminal");
        assert_eq!(resolve_tab_label(None, "prod-server"), "prod-server");
    }

    #[test]
    fn empty_title_falls_back_to_static_label() {
        assert_eq!(resolve_tab_label(Some(""), "Terminal"), "Terminal");
    }

    #[test]
    fn fallback_is_returned_by_value() {
        let label = resolve_tab_label(None, "Terminal");
        assert_eq!(label, "Terminal");
    }

    #[test]
    fn windows_drive_path_shortened_to_basename() {
        assert_eq!(
            resolve_tab_label(Some("C:\\Windows\\system32\\cmd.exe"), "Terminal"),
            "cmd.exe"
        );
        assert_eq!(
            resolve_tab_label(
                Some("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"),
                "Terminal"
            ),
            "powershell.exe"
        );
    }

    #[test]
    fn posix_path_shortened_to_basename() {
        assert_eq!(resolve_tab_label(Some("/usr/bin/bash"), "Terminal"), "bash");
        assert_eq!(resolve_tab_label(Some("/bin/sh"), "Terminal"), "sh");
    }

    #[test]
    fn relative_or_descriptive_titles_not_trimmed() {
        assert_eq!(resolve_tab_label(Some("~/repo"), "Terminal"), "~/repo");
        assert_eq!(
            resolve_tab_label(Some("user@host: ~/repo"), "Terminal"),
            "user@host: ~/repo"
        );
        assert_eq!(
            resolve_tab_label(Some("vim — main.rs"), "Terminal"),
            "vim — main.rs"
        );
    }

    #[test]
    fn trim_path_title_helper_directly() {
        assert_eq!(trim_path_title("C:\\Windows\\system32\\cmd.exe"), "cmd.exe");
        assert_eq!(trim_path_title("/usr/bin/bash"), "bash");
        assert_eq!(trim_path_title("cmd.exe"), "cmd.exe");
        assert_eq!(trim_path_title("user@host: ~/repo"), "user@host: ~/repo");
        assert_eq!(trim_path_title("  /usr/bin/zsh  "), "zsh");
    }

    #[test]
    fn phase1_terminal_titles_are_sanitized_by_policy() {
        use oneterm_terminal::security_policy::TerminalSecurityPolicy;

        let policy = TerminalSecurityPolicy::default();

        // Control characters stripped.
        let controlled = "safe\u{0007}\u{001b}[31m\u{202e}txt.exe";
        let sanitized = policy.sanitize_title(controlled).unwrap();
        assert_eq!(sanitized, "safe[31mtxt.exe");

        // Oversized title truncated.
        let oversized = "x".repeat(256 * 1024);
        let sanitized = policy.sanitize_title(&oversized).unwrap();
        assert!(sanitized.len() <= 4 * 1024);

        // resolve_tab_label still passes through what it receives.
        let clean = "vim — main.rs";
        assert_eq!(resolve_tab_label(Some(clean), "Terminal"), clean);
    }
}
