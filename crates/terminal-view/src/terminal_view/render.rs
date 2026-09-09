//! `Render` + `Focusable` for [`TerminalView`]: refreshes `RenderInputs` in
//! place from settings and theme, applies the queued scrollbar offset, builds
//! the wrapper div (focus tracking, listeners, context menu) around one
//! `TerminalElement`, and stacks the badges, progress bar, SSH banner, search
//! bar, scrollbar and completion overlay on top.
//!
//! The per-frame theme is assembled in three steps — `build_terminal_theme`
//! (gpui theme → palette) → `apply_color_overrides` (settings) →
//! `apply_dynamic_colors` (OSC) — and rebuilt only when one of those inputs
//! changed; the result is shared with the element behind an `Rc`.

use std::rc::Rc;

use gpui::{
    App, Bounds, Context, Edges, ElementInputHandler, FocusHandle, Focusable, Font, FontWeight,
    Hsla, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Pixels, Render,
    Role, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px, relative,
};
use gpui_component::{
    ActiveTheme as _, Theme, WindowExt as _, alert::Alert, menu::ContextMenuExt as _,
    notification::NotificationType,
};

use oneterm_core::config::ShellKind;
use oneterm_highlight::ShellProfile;
use oneterm_settings::{
    ColorOverrides, SemanticHighlightingMode, TerminalBlink, TerminalCursorShape, TerminalSettings,
};
use oneterm_terminal::{DynamicColors, SessionKind, TerminalLogState, TerminalProgress};
use oneterm_theme::notif_ext::notify;

use super::TerminalView;
use super::scrollbar::ScrollGeometry;
use crate::input::{MenuContext, MenuSplitContext, build_menu};
use crate::render::cursor::CursorConfig;
use crate::render::element::{TerminalElement, TerminalElementSpec};
use crate::render::frame::CursorShape;
use crate::render::state::{GutterInputs, RenderInputs};
use crate::theme::{
    TerminalTheme, apply_color_overrides, apply_dynamic_colors, build_terminal_theme,
};

/// The terminal `Font` plus the settings it was built from, so `render` only
/// rebuilds it (a fresh `Arc<Vec<(String, u32)>>` of font features) when one
/// of those inputs changes.
pub(super) struct CachedFont {
    family: SharedString,
    weight: FontWeight,
    features: Vec<SharedString>,
    font: Font,
}

impl CachedFont {
    /// Whether the cached font was built from exactly these inputs.
    fn matches(&self, family: &SharedString, settings: &TerminalSettings) -> bool {
        self.family == *family
            && self.weight == settings.font_weight
            && self.features == settings.font_features
    }
}

/// Everything the `TerminalTheme` is derived from. The theme is rebuilt (and
/// the palette re-pushed) only when this changes.
#[derive(Clone, PartialEq)]
pub(super) struct ThemeKey {
    foreground: Hsla,
    background: Hsla,
    caret: Hsla,
    overrides: ColorOverrides,
    dynamic: DynamicColors,
}

impl ThemeKey {
    /// Compare against live inputs without cloning `ColorOverrides` (it owns
    /// a `Vec`): the clone happens only on the rebuild path.
    fn matches(&self, theme: &Theme, overrides: &ColorOverrides, dynamic: &DynamicColors) -> bool {
        self.foreground == theme.colors.foreground
            && self.background == theme.colors.background
            && self.caret == theme.colors.caret
            && self.overrides == *overrides
            && self.dynamic == *dynamic
    }
}

/// The few gpui theme colours the overlays need — read by value instead of
/// cloning the whole `Theme` per frame.
#[derive(Clone, Copy)]
pub(super) struct OverlayColors {
    blue: Hsla,
    danger: Hsla,
    warning: Hsla,
    muted: Hsla,
}

/// The settings a frame reads, copied out so the settings borrow ends before
/// the render state is touched.
struct FrameSettings {
    font: Font,
    font_size: Pixels,
    line_height_factor: f32,
    cell_width_override: Option<f32>,
    padding: Edges<Pixels>,
    show_gutter: bool,
    cursor_shape: CursorShape,
    cursor_color: Option<Hsla>,
    blink_on: bool,
    bell_enabled: bool,
    semantic_enabled: bool,
    profile: ShellProfile,
    show_context_menu: bool,
}

/// The `RenderInputs` a fresh view starts with; the first render refreshes
/// every field from the live settings anyway.
pub(super) fn initial_inputs(settings: &TerminalSettings, theme: &Theme) -> RenderInputs {
    let family = settings
        .font_family
        .clone()
        .unwrap_or_else(|| theme.mono_font_family.clone());
    let font_size = settings.font_size.map(px).unwrap_or(theme.mono_font_size);
    RenderInputs::new(
        Rc::new(build_terminal_theme(theme)),
        terminal_font(settings, &family),
        font_size,
    )
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.drain_notifications(window, cx);
        // Auto-completion: sync settings, feed gating + the live input line, and
        // recompute suggestions for this frame.
        self.update_completion(cx);
        // Output since the last frame invalidated the search match coordinates
        // — refresh once per frame.
        self.refresh_search_if_dirty(cx);

        let focused = self.focus.is_focused(window);
        let session = self.session.clone();
        let (kind, is_logging) = {
            let s = session.read(cx);
            let is_logging = s
                .capabilities()
                .logging
                .is_some_and(|logging| matches!(logging.state(), TerminalLogState::Running { .. }));
            (s.kind(), is_logging)
        };
        let is_multi_space = self
            .split_ctx
            .as_ref()
            .and_then(|split| split.panel.upgrade())
            .is_some_and(|panel| panel.read(cx).leaf_count() > 1);
        let overlay_colors = {
            let t = cx.theme();
            OverlayColors {
                blue: t.blue,
                danger: t.danger,
                warning: t.warning,
                muted: t.muted,
            }
        };

        let dynamic = session.read(cx).dynamic_colors();
        let settings_entity = self.deps.settings.clone();
        let frame = {
            let settings = settings_entity.read(cx);
            let gpui_theme = cx.theme();
            let family = settings
                .font_family
                .clone()
                .unwrap_or_else(|| gpui_theme.mono_font_family.clone());
            let font = self.cached_font(&family, settings);
            self.refresh_theme(gpui_theme, settings, dynamic, cx);
            FrameSettings {
                font,
                font_size: settings
                    .font_size
                    .map(px)
                    .unwrap_or(gpui_theme.mono_font_size),
                line_height_factor: settings.line_height_factor,
                cell_width_override: settings.cell_width,
                padding: Edges {
                    top: px(settings.padding.top),
                    right: px(settings.padding.right),
                    bottom: px(settings.padding.bottom),
                    left: px(settings.padding.left),
                },
                show_gutter: settings.show_gutter,
                cursor_shape: cursor_shape_from_setting(settings.cursor_shape),
                cursor_color: settings.cursor_color,
                blink_on: settings.cursor_blink == TerminalBlink::On,
                bell_enabled: settings.bell_enabled,
                semantic_enabled: !matches!(
                    settings.semantic_highlighting,
                    SemanticHighlightingMode::Off
                ),
                // Remote hosts are virtually always Unix; a local session
                // follows the configured shell kind.
                profile: match kind {
                    SessionKind::Local => shell_kind_to_profile(settings.shell.kind),
                    SessionKind::Ssh => ShellProfile::Unix,
                },
                show_context_menu: settings.show_context_menu,
            }
        };

        // The one `terminal_info()` of this frame (a viewport scan under the
        // engine lock); gutter stamping reads it again only on `Output`.
        let mut info = session.read(cx).terminal_info();
        {
            let mut state = self.render_state.borrow_mut();
            let inputs = &mut state.inputs;
            inputs.font = frame.font;
            inputs.font_size = frame.font_size;
            inputs.line_height_factor = frame.line_height_factor;
            inputs.cell_width_override = frame.cell_width_override;
            inputs.padding = frame.padding;
            inputs.show_gutter = frame.show_gutter;
            let line_height = f32::from(state.metrics(window, cx).line_height);

            // Apply a scrollbar drag / track click queued since the last frame.
            if let Some(new_offset) = self.scrollbar.take_pending_offset() {
                let delta = new_offset as i32 - info.display_offset as i32;
                if delta != 0 {
                    session.update(cx, |s, _| s.scroll(delta));
                    info = session.read(cx).terminal_info();
                }
            }
            self.scrollbar.update(ScrollGeometry {
                total_lines: info.total_lines,
                viewport_lines: info.num_lines,
                display_offset: info.display_offset,
                line_height,
            });

            let inputs = &mut state.inputs;
            inputs.cursor = CursorConfig {
                shape: Some(frame.cursor_shape),
                color: frame.cursor_color,
                focused,
                // Blink Off keeps the cursor steady; the element ignores the
                // phase for an unfocused view.
                blink_visible: !frame.blink_on || self.blink_visible,
            };
            inputs.gutter = GutterInputs {
                times: self.gutter_times.times(),
                base: self.gutter_times.base(),
                absolute_line_count: info.absolute_line_count,
            };
            self.search.visible_highlights_into(
                info.display_offset,
                info.num_lines,
                info.num_cols,
                &mut inputs.search,
            );
            inputs.semantic.set_enabled(frame.semantic_enabled);
            inputs.semantic.set_profile(frame.profile);
            inputs.url_hovering = self.url_hover.is_hovering();
        }

        // The IME handler is installed by the element in paint, only while
        // the terminal owns focus.
        let ime = focused.then(|| {
            let focus = self.focus.clone();
            let view = cx.entity();
            Box::new(
                move |bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App| {
                    window.handle_input(&focus, ElementInputHandler::new(bounds, view), cx);
                },
            ) as Box<dyn FnOnce(Bounds<Pixels>, &mut Window, &mut App)>
        });
        let element = TerminalElement::new(TerminalElementSpec {
            id: "terminal-grid".into(),
            session: session.clone(),
            state: self.render_state.clone(),
            ime,
        });

        let mut terminal_div = div()
            .id("terminal-view")
            .role(Role::Pane)
            .aria_label("Terminal")
            .size_full()
            .relative()
            .track_focus(&self.focus)
            .key_context("Terminal")
            .on_key_down(cx.listener(Self::on_key_down))
            .on_modifiers_changed(cx.listener(Self::on_modifiers_changed))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_exit(cx.listener(Self::on_mouse_exit))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel));
        // With the context menu on, the right button belongs to the menu; the
        // press must not also reach the program behind it.
        if !frame.show_context_menu {
            terminal_div = terminal_div
                .on_mouse_down(MouseButton::Right, cx.listener(Self::on_mouse_down))
                .on_mouse_up(MouseButton::Right, cx.listener(Self::on_mouse_up));
        }
        let terminal_div = terminal_div
            .child(element)
            .children(bell_badge(
                self.has_bell,
                frame.bell_enabled,
                overlay_colors,
            ))
            .children(recording_overlay(
                is_logging && is_multi_space,
                overlay_colors,
            ))
            .children(progress_overlay(self.progress, overlay_colors))
            .children(ssh_closed_banner(self.ssh_closed))
            .children(self.completion_overlay_element())
            .children(self.render_scrollbar(window))
            .children(self.render_search_bar(cx));

        if !frame.show_context_menu {
            return terminal_div.into_any_element();
        }
        let focus = self.focus.clone();
        let split_ctx = self.split_ctx.clone();
        terminal_div
            .context_menu(move |menu, window, cx| {
                let split = split_ctx.clone().and_then(|ctx| {
                    let panel = ctx.panel.upgrade()?;
                    let panel = panel.read(cx);
                    Some(MenuSplitContext {
                        empty_destinations: panel.empty_space_destinations(),
                        leaf_count: panel.leaf_count(),
                        ctx,
                    })
                });
                let (has_selection, logging) = {
                    let s = session.read(cx);
                    (s.has_selection(), s.capabilities().logging)
                };
                let menu_ctx = MenuContext {
                    session: session.clone(),
                    focus: focus.clone(),
                    has_selection,
                    split,
                    logging,
                };
                build_menu(menu, &menu_ctx, window, cx)
            })
            .into_any_element()
    }
}

impl TerminalView {
    /// Toasts that need a `Window`, queued by the events task or the session.
    fn drain_notifications(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(logging) = self.session.read(cx).capabilities().logging
            && let Some(message) = logging.take_error()
        {
            window.push_notification(notify(NotificationType::Error, message, cx), cx);
        }
        if std::mem::take(&mut self.pending_ssh_closed_notification) {
            window.push_notification(
                notify(NotificationType::Error, "SSH connection closed.", cx),
                cx,
            );
        }
        if self.dropped_notifications > 0 {
            let dropped = std::mem::take(&mut self.dropped_notifications);
            window.push_notification(
                notify(
                    NotificationType::Warning,
                    format!("{dropped} terminal notifications were dropped while the UI was busy."),
                    cx,
                ),
                cx,
            );
        }
        for msg in std::mem::take(&mut self.pending_notifications) {
            window.push_notification(notify(NotificationType::Info, msg, cx), cx);
        }
    }

    /// The terminal font for `family` + `settings`, rebuilt only when the
    /// family, weight, or feature list changed since the last frame.
    fn cached_font(&mut self, family: &SharedString, settings: &TerminalSettings) -> Font {
        if let Some(cached) = self.cached_font.as_ref()
            && cached.matches(family, settings)
        {
            return cached.font.clone();
        }
        let font = terminal_font(settings, family);
        self.cached_font = Some(CachedFont {
            family: family.clone(),
            weight: settings.font_weight,
            features: settings.font_features.clone(),
            font: font.clone(),
        });
        font
    }

    /// Rebuild `RenderInputs.theme` when the gpui theme colours, the settings
    /// overrides or the OSC dynamic colours changed. The effective default
    /// fg/bg/cursor + ANSI palette is pushed to the backend (so OSC 4/10/11/12
    /// queries answer with the theme) only when that palette itself changed;
    /// dynamic colours are layered on top afterwards so OSC *sets* win.
    fn refresh_theme(
        &mut self,
        gpui_theme: &Theme,
        settings: &TerminalSettings,
        dynamic: DynamicColors,
        cx: &App,
    ) {
        if self
            .theme_key
            .as_ref()
            .is_some_and(|key| key.matches(gpui_theme, &settings.color_overrides, &dynamic))
        {
            return;
        }
        let base =
            apply_color_overrides(build_terminal_theme(gpui_theme), &settings.color_overrides);
        if self.last_pushed_palette != Some(base.palette) {
            self.session.read(cx).set_default_colors(
                base.palette.foreground,
                base.palette.background,
                base.palette.cursor,
                base.palette.ansi,
            );
            self.last_pushed_palette = Some(base.palette);
        }
        let theme: TerminalTheme = apply_dynamic_colors(base, &dynamic);
        self.render_state.borrow_mut().inputs.theme = Rc::new(theme);
        self.theme_key = Some(ThemeKey {
            foreground: gpui_theme.colors.foreground,
            background: gpui_theme.colors.background,
            caret: gpui_theme.colors.caret,
            overrides: settings.color_overrides.clone(),
            dynamic,
        });
    }
}

/// Build the terminal GPUI font from settings: `calt` (ligatures) off unless
/// listed in `font_features`; every listed feature is enabled.
pub(super) fn terminal_font(settings: &TerminalSettings, font_family: &SharedString) -> Font {
    let mut features: Vec<(String, u32)> = vec![("calt".to_string(), 0)];
    for f in &settings.font_features {
        features.retain(|(tag, _)| tag != f);
        features.push((f.to_string(), 1u32));
    }
    Font {
        family: font_family.clone(),
        weight: settings.font_weight,
        style: gpui::FontStyle::Normal,
        fallbacks: None,
        features: gpui::FontFeatures(std::sync::Arc::new(features)),
    }
}

/// The configured cursor shape; `Hidden` only ever comes from the snapshot.
fn cursor_shape_from_setting(shape: TerminalCursorShape) -> CursorShape {
    match shape {
        TerminalCursorShape::Block => CursorShape::Block,
        TerminalCursorShape::Bar => CursorShape::Beam,
        TerminalCursorShape::Underline => CursorShape::Underline,
    }
}

/// Read-only banner shown after a remote SSH close.
fn ssh_closed_banner(closed: bool) -> Option<impl IntoElement> {
    closed.then(|| {
        Alert::warning(
            "ssh-connection-closed",
            "SSH connection closed. Input is disabled.",
        )
        .banner()
        .absolute()
        .bottom_0()
        .left_0()
        .right_0()
    })
}

/// Fill fraction and colour of the OSC 9;4 progress bar: blue for normal
/// progress, red for error, yellow for paused; indeterminate fills the whole
/// track. `None` when no bar is shown.
pub(super) fn progress_style(
    progress: Option<TerminalProgress>,
    colors: OverlayColors,
) -> Option<(f32, Hsla)> {
    let (fraction, color) = match progress? {
        TerminalProgress::Remove => return None,
        TerminalProgress::Set(pct) => (pct as f32 / 100.0, colors.blue),
        TerminalProgress::Error(pct) => (pct as f32 / 100.0, colors.danger),
        TerminalProgress::Paused(pct) => (pct as f32 / 100.0, colors.warning),
        TerminalProgress::Indeterminate => (1.0, colors.blue),
    };
    Some((fraction.clamp(0.0, 1.0), color))
}

/// Taskbar progress overlay (OSC 9;4) — a thin bar along the top edge.
fn progress_overlay(
    progress: Option<TerminalProgress>,
    colors: OverlayColors,
) -> Option<impl IntoElement> {
    let (fraction, color) = progress_style(progress, colors)?;
    Some(
        div()
            .id("terminal-progress")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(5.0))
            .bg(colors.muted.opacity(0.4))
            .child(
                div()
                    .id("terminal-progress-fill")
                    .h_full()
                    .w(relative(fraction))
                    .bg(color),
            ),
    )
}

/// Small recording indicator for split Spaces (a single Space shows it in
/// the tab strip instead).
fn recording_overlay(is_logging: bool, colors: OverlayColors) -> Option<impl IntoElement> {
    is_logging.then(|| {
        div()
            .id("terminal-recording")
            .absolute()
            .top_1()
            .right_2()
            .text_xs()
            .text_color(colors.danger)
            .child("●")
    })
}

/// Whether the bell badge is shown: a received bell and the setting on.
pub(super) fn bell_badge_visible(has_bell: bool, bell_enabled: bool) -> bool {
    has_bell && bell_enabled
}

/// Bell indicator (top-right corner).
fn bell_badge(
    has_bell: bool,
    bell_enabled: bool,
    colors: OverlayColors,
) -> Option<impl IntoElement> {
    bell_badge_visible(has_bell, bell_enabled).then(|| {
        div()
            .id("terminal-bell")
            .absolute()
            .top_1()
            .right_2()
            .px_1()
            .py_0()
            .text_xs()
            .text_color(colors.warning)
            .child("🔔")
    })
}

/// Map the session's [`ShellKind`] to the scanner's [`ShellProfile`].
///
/// The scanner uses the profile's prompt regex to detect prompt lines (when
/// OSC 133 row roles are absent). A mismatch makes prompt+command lines plain
/// output — losing command/option highlighting.
fn shell_kind_to_profile(kind: ShellKind) -> ShellProfile {
    match kind {
        ShellKind::Cmd => ShellProfile::Cmd,
        ShellKind::PowerShell | ShellKind::Pwsh => ShellProfile::PowerShell,
        ShellKind::Bash | ShellKind::Zsh | ShellKind::Sh => ShellProfile::Unix,
        ShellKind::Custom => ShellProfile::Dumb,
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Hsla, SharedString, hsla};
    use oneterm_settings::TerminalSettings;
    use oneterm_terminal::TerminalProgress;

    use super::{
        CachedFont, OverlayColors, bell_badge_visible, progress_style, ssh_closed_banner,
        terminal_font,
    };

    fn colors() -> OverlayColors {
        OverlayColors {
            blue: hsla(0.6, 1.0, 0.5, 1.0),
            danger: hsla(0.0, 1.0, 0.5, 1.0),
            warning: hsla(0.1, 1.0, 0.5, 1.0),
            muted: hsla(0.0, 0.0, 0.5, 1.0),
        }
    }

    #[test]
    fn ssh_closed_banner_is_persistent_only_after_close() {
        assert!(ssh_closed_banner(true).is_some());
        assert!(ssh_closed_banner(false).is_none());
    }

    #[test]
    fn font_disables_ligatures_unless_requested() {
        let settings = TerminalSettings::default();
        let font = terminal_font(&settings, &SharedString::from("Mono"));
        assert!(
            font.features
                .0
                .iter()
                .any(|(tag, on)| tag == "calt" && *on == 0)
        );
    }

    #[test]
    fn font_enables_listed_features_and_overrides_calt() {
        let settings = TerminalSettings {
            font_features: vec!["calt".into(), "ss01".into()],
            ..TerminalSettings::default()
        };
        let font = terminal_font(&settings, &SharedString::from("Mono"));
        let features = &font.features.0;
        assert_eq!(features.iter().filter(|(tag, _)| tag == "calt").count(), 1);
        assert!(features.iter().any(|(tag, on)| tag == "calt" && *on == 1));
        assert!(features.iter().any(|(tag, on)| tag == "ss01" && *on == 1));
    }

    #[test]
    fn cached_font_matches_only_its_own_inputs() {
        let settings = TerminalSettings::default();
        let family = SharedString::from("Mono");
        let cached = CachedFont {
            family: family.clone(),
            weight: settings.font_weight,
            features: settings.font_features.clone(),
            font: terminal_font(&settings, &family),
        };
        assert!(cached.matches(&family, &settings));
        assert!(!cached.matches(&SharedString::from("Other"), &settings));
        let bold = TerminalSettings {
            font_weight: gpui::FontWeight::BOLD,
            ..TerminalSettings::default()
        };
        assert!(!cached.matches(&family, &bold));
        let ligatures = TerminalSettings {
            font_features: vec!["calt".into()],
            ..TerminalSettings::default()
        };
        assert!(!cached.matches(&family, &ligatures));
    }

    #[test]
    fn progress_bar_color_by_variant() {
        let c = colors();
        let style = |p: TerminalProgress| progress_style(Some(p), c);
        assert_eq!(style(TerminalProgress::Set(40)), Some((0.4, c.blue)));
        assert_eq!(style(TerminalProgress::Error(50)), Some((0.5, c.danger)));
        assert_eq!(style(TerminalProgress::Paused(10)), Some((0.1, c.warning)));
        assert_eq!(style(TerminalProgress::Indeterminate), Some((1.0, c.blue)));
        assert_eq!(style(TerminalProgress::Remove), None);
        assert_eq!(progress_style(None, c), None);
        // Percentages above 100 clamp instead of overflowing the track.
        let (fraction, _): (f32, Hsla) = style(TerminalProgress::Set(250)).expect("bar");
        assert_eq!(fraction, 1.0);
    }

    #[test]
    fn bell_badge_requires_setting() {
        assert!(bell_badge_visible(true, true));
        assert!(!bell_badge_visible(true, false));
        assert!(!bell_badge_visible(false, true));
    }
}
