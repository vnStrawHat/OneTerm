//! `impl Render + Focusable for LocalTerminalView` — builds the per-frame
//! terminal element tree (grid element + overlays + scrollbar + search bar),
//! plus the render-time helpers (font, cursor blink, bell / progress overlays).
//!
//! The per-frame theme is assembled in clearly separated steps:
//! `build_terminal_theme` (gpui theme → palette) → `apply_color_overrides`
//! (settings) → `apply_dynamic_colors` (OSC). All three are plain struct
//! builds — the semantic class styles they carry are a process-wide static
//! (PERF-01) — and the result is shared with the element behind an `Rc`.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, Context, FocusHandle, Focusable, Font, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, Styled as _, Window, div, px, relative,
};
use gpui_component::{ActiveTheme as _, WindowExt as _, notification::NotificationType};

use oneterm_core::config::ShellKind;
use oneterm_highlight::ShellProfile;
use oneterm_settings::{SemanticHighlightingMode, TerminalBlink, TerminalSettings};
use oneterm_terminal::{SessionKind, TerminalLogState, TerminalProgress};
use oneterm_theme::notif_ext::notify;

use super::LocalTerminalView;
use super::scrollbar::ScrollGeometry;
use crate::element::TerminalElement;
use crate::theme::{apply_color_overrides, apply_dynamic_colors, build_terminal_theme};

/// The terminal `Font` plus the settings it was built from, so `render` only
/// rebuilds it (a fresh `Arc<Vec<(String, u32)>>` of font features) when one
/// of those inputs changes (PERF-05).
pub(crate) struct CachedFont {
    family: SharedString,
    weight: FontWeight,
    features: Vec<SharedString>,
    pub(crate) font: Font,
}

impl CachedFont {
    /// Whether the cached font was built from exactly these inputs.
    fn matches(&self, family: &SharedString, settings: &TerminalSettings) -> bool {
        self.family == *family
            && self.weight == settings.font_weight
            && self.features == settings.font_features
    }
}

/// The few gpui theme colours the overlays need — read by value instead of
/// cloning the whole `Theme` per frame (PERF-05).
#[derive(Clone, Copy)]
struct OverlayColors {
    blue: Hsla,
    danger: Hsla,
    warning: Hsla,
    muted: Hsla,
}

impl Focusable for LocalTerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for LocalTerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(logging) = self.session.read(cx).capabilities().logging
            && let Some(message) = logging.take_error()
        {
            window.push_notification(notify(NotificationType::Error, message, cx), cx);
        }

        // Drain OSC 9 notifications here (needs a `Window`, unavailable in the
        // async subscribe task where they are queued).
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
        if !self.pending_notifications.is_empty() {
            for msg in std::mem::take(&mut self.pending_notifications) {
                window.push_notification(notify(NotificationType::Info, msg, cx), cx);
            }
        }

        // Auto-completion: sync settings, feed gating + the live input line, and
        // recompute suggestions for this frame (docs/auto-completion/06 §3).
        self.update_completion(cx);

        // Output since the last frame invalidated the search match coordinates
        // — refresh once per frame (PERF-04 / CORR-42).
        self.refresh_search_if_dirty(cx);

        let focused = self.focus.is_focused(window);
        let session = self.session.clone();
        let is_logging = session
            .read(cx)
            .capabilities()
            .logging
            .is_some_and(|logging| matches!(logging.state(), TerminalLogState::Running { .. }));
        let is_multi_space = self
            .split_ctx
            .as_ref()
            .and_then(|split| split.panel.upgrade())
            .is_some_and(|panel| panel.read(cx).leaf_count() > 1);
        let settings_entity = self.deps.settings.clone();
        let overlay_colors = {
            let t = cx.theme();
            OverlayColors {
                blue: t.blue,
                danger: t.danger,
                warning: t.warning,
                muted: t.muted,
            }
        };
        let (
            font,
            font_size,
            line_height_factor,
            cursor_visible,
            bell_enabled,
            has_bell,
            cursor_color,
            padding,
            cell_width_override,
            theme,
            cursor_shape,
            show_gutter,
            semantic_highlighting,
            show_context_menu,
        ) = {
            let settings = settings_entity.read(cx);
            let gpui_theme = cx.theme();
            let effective_family: SharedString = settings
                .font_family
                .clone()
                .unwrap_or_else(|| gpui_theme.mono_font_family.clone());
            let effective_size = settings
                .font_size
                .map(gpui::px)
                .unwrap_or(gpui_theme.mono_font_size);
            let font = self.cached_font(&effective_family, settings);
            // The settings colour overrides are applied while the settings are
            // borrowed — no per-frame clone of `ColorOverrides` (PERF-05).
            let theme =
                apply_color_overrides(build_terminal_theme(gpui_theme), &settings.color_overrides);
            (
                font,
                effective_size,
                settings.line_height_factor,
                self.should_show_cursor(focused, settings),
                settings.bell_enabled,
                self.has_bell,
                settings.cursor_color,
                settings.padding,
                settings.cell_width,
                theme,
                settings.cursor_shape,
                settings.show_gutter,
                settings.semantic_highlighting,
                settings.show_context_menu,
            )
        };

        let render_cache = self.render_cache.clone();
        let view = cx.entity();

        // The one `terminal_info()` of this frame (a viewport scan under the
        // `Term` lock — PERF-03); the element receives it instead of asking
        // again in prepaint.
        let info_start = std::time::Instant::now();
        let mut info = session.read(cx).terminal_info();
        render_cache.borrow_mut().rows.stats.terminal_info_us = info_start.elapsed().as_micros();
        let line_height = f32::from(render_cache.borrow().metrics.line_height);

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

        // Push the effective default fg/bg/cursor + ANSI palette to the backend
        // so OSC 10/11/12 and OSC 4 *queries* can be answered, then apply OSC-set
        // dynamic colors on top so OSC *sets* (OSC 4/10/11/12) and *resets*
        // (OSC 104/110/111/112) take effect.
        //
        // Skip the push when the palette hasn't changed since the last one.
        if self.last_pushed_palette != Some(theme.palette) {
            let session_ref = session.read(cx);
            session_ref.set_default_colors(
                theme.palette.foreground,
                theme.palette.background,
                theme.palette.cursor,
                theme.palette.ansi,
            );
            self.last_pushed_palette = Some(theme.palette);
        }
        let dynamic_colors = session.read(cx).dynamic_colors();
        let theme = Rc::new(apply_dynamic_colors(theme, &dynamic_colors));

        // ── Search highlights (display coordinates, visible only) ──
        let search_highlights =
            self.search
                .visible_highlights(info.display_offset, info.num_lines, info.num_cols);

        // Semantic overlay (Layer 2) -- Auto/On = enabled; Off = disabled.
        // Update the persisted overlay instead of recreating it every frame.
        let semantic_enabled = !matches!(semantic_highlighting, SemanticHighlightingMode::Off);
        // Select the shell profile for semantic highlighting:
        // - Local session: use the configured ShellKind (Cmd/PowerShell/Unix/...)
        // - SSH session: always Unix (remote hosts are virtually always Unix)
        let profile = match session.read(cx).kind() {
            SessionKind::Local => shell_kind_to_profile(settings_entity.read(cx).shell.kind),
            SessionKind::Ssh => ShellProfile::Unix,
        };
        self.semantic_overlay.set_enabled(semantic_enabled);
        self.semantic_overlay.set_profile(profile);
        let overlay = self.semantic_overlay.clone();

        let terminal_div = div()
            .id("local-terminal-view")
            .size_full()
            .relative()
            .track_focus(&self.focus)
            .key_context("Terminal")
            .when(self.url_hover.is_hovering(), |d| d.cursor_pointer())
            .child(TerminalElement {
                session: session.clone(),
                theme,
                terminal_info: info,
                font,
                font_size,
                line_height_factor,
                focused,
                cursor_visible,
                view: cx.entity(),
                focus: self.focus.clone(),
                line_times: self.gutter_times.times(),
                line_time_base: self.gutter_times.base(),
                padding,
                show_gutter,
                cell_width_override,
                cursor_color_override: cursor_color,
                cursor_shape_override: cursor_shape,
                render_cache: render_cache.clone(),
                search_highlights,
                overlay,
            })
            .children(bell_overlay(has_bell, bell_enabled, overlay_colors))
            .children(recording_overlay(
                is_logging && is_multi_space,
                overlay_colors,
            ))
            .children(progress_overlay(self.progress, overlay_colors))
            .children(self.completion_overlay_element())
            .children(self.render_scrollbar(&render_cache, cx))
            .children(self.render_search_bar(cx));

        let split_ctx = self.split_ctx.clone();
        crate::handlers::attach(
            terminal_div,
            session,
            render_cache,
            view,
            self.focus.clone(),
            split_ctx,
            show_context_menu,
        )
    }
}

impl LocalTerminalView {
    /// Decide whether to draw the cursor (blink logic).
    /// - Not focused → always draw.
    /// - Focused + blink off → always draw.
    /// - Focused + blink on → draw when `cursor_blink_visible`.
    pub(crate) fn should_show_cursor(&self, focused: bool, settings: &TerminalSettings) -> bool {
        if !focused {
            return true;
        }
        match settings.cursor_blink {
            TerminalBlink::Off => true,
            TerminalBlink::On => self.cursor_blink_visible,
        }
    }

    /// The terminal font for `family` + `settings`, rebuilt only when the
    /// family, weight, or feature list changed since the last frame.
    fn cached_font(&mut self, family: &SharedString, settings: &TerminalSettings) -> Font {
        if let Some(cached) = self.font_cache.as_ref() {
            if cached.matches(family, settings) {
                return cached.font.clone();
            }
        }
        let font = terminal_font(settings, family);
        self.font_cache = Some(CachedFont {
            family: family.clone(),
            weight: settings.font_weight,
            features: settings.font_features.clone(),
            font: font.clone(),
        });
        font
    }
}

/// Build the terminal GPUI font from settings: `calt` (ligatures) off unless
/// listed in `font_features`; every listed feature is enabled.
pub(crate) fn terminal_font(settings: &TerminalSettings, font_family: &SharedString) -> gpui::Font {
    let mut features: Vec<(String, u32)> = vec![("calt".to_string(), 0)];
    for f in &settings.font_features {
        features.retain(|(tag, _)| tag != f);
        features.push((f.to_string(), 1u32));
    }
    gpui::Font {
        family: font_family.clone().into(),
        weight: settings.font_weight,
        style: gpui::FontStyle::Normal,
        fallbacks: None,
        features: gpui::FontFeatures(std::sync::Arc::new(features)),
    }
}

/// Taskbar progress overlay (OSC 9;4) — a thin bar along the top edge.
/// The fill width follows the reported percent; the color reflects the
/// state (normal/error/paused). Indeterminate shows a full-width bar.
fn progress_overlay(
    progress: Option<TerminalProgress>,
    colors: OverlayColors,
) -> Option<impl IntoElement> {
    let (fraction, color) = match progress? {
        TerminalProgress::Remove => return None,
        TerminalProgress::Set(pct) => (pct as f32 / 100.0, colors.blue),
        TerminalProgress::Error(pct) => (pct as f32 / 100.0, colors.danger),
        TerminalProgress::Paused(pct) => (pct as f32 / 100.0, colors.warning),
        // Indeterminate: no known percent → fill the whole track.
        TerminalProgress::Indeterminate => (1.0, colors.blue),
    };
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
                    .w(relative(fraction.clamp(0.0, 1.0)))
                    .bg(color),
            ),
    )
}

/// Small recording indicator overlay for split Spaces.
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

/// Bell indicator overlay (top-right corner).
fn bell_overlay(
    has_bell: bool,
    bell_enabled: bool,
    colors: OverlayColors,
) -> Option<impl IntoElement> {
    (has_bell && bell_enabled).then(|| {
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
/// OSC 133 row roles are absent). A mismatch causes the scanner to treat
/// prompt+command lines as plain output — losing command/option highlighting.
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
    use gpui::SharedString;
    use oneterm_settings::TerminalSettings;

    use super::{CachedFont, terminal_font};

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
}
