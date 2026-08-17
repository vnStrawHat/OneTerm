//! `impl Render + Focusable for LocalTerminalView` — builds the per-frame
//! terminal element tree (grid element + overlays + scrollbar + search bar),
//! plus the render-time helpers (font, cursor blink, bell / progress overlays).
//!
//! The per-frame theme is assembled in clearly separated steps so it can be
//! cached later: `build_terminal_theme` (gpui theme → palette) →
//! `apply_color_overrides` (settings) → `apply_dynamic_colors` (OSC).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, Context, FocusHandle, Focusable, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Window, div, px, relative,
};
use gpui_component::{ActiveTheme as _, Theme, WindowExt as _, notification::NotificationType};

use oneterm_core::config::ShellKind;
use oneterm_highlight::ShellProfile;
use oneterm_settings::{SemanticHighlightingMode, TerminalBlink, TerminalSettings};
use oneterm_state::notif_ext::notify;
use oneterm_terminal::TerminalProgress;

use super::LocalTerminalView;
use super::scrollbar::ScrollGeometry;
use crate::element::TerminalElement;
use crate::theme::{
    TerminalTheme, apply_color_overrides, apply_dynamic_colors, build_terminal_theme,
};

impl Focusable for LocalTerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for LocalTerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

        let theme: TerminalTheme = build_terminal_theme(cx.theme());
        let focused = self.focus.is_focused(window);
        let session = self.session.clone();
        let settings_entity = TerminalSettings::global(cx);
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
            color_overrides,
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
            (
                terminal_font(settings, &effective_family),
                effective_size,
                settings.line_height_factor,
                self.should_show_cursor(focused, settings),
                settings.bell_enabled,
                self.has_bell,
                settings.cursor_color,
                settings.padding,
                settings.cell_width,
                settings.color_overrides.clone(),
                settings.cursor_shape,
                settings.show_gutter,
                settings.semantic_highlighting,
                settings.show_context_menu,
            )
        };

        let render_cache = self.render_cache.clone();
        let view = cx.entity();

        let info = session.read(cx).terminal_info();
        let line_height = f32::from(render_cache.borrow().metrics.line_height);
        self.scrollbar.update(ScrollGeometry {
            total_lines: info.total_lines,
            viewport_lines: info.num_lines,
            display_offset: info.display_offset,
            line_height,
        });

        // Apply a scrollbar drag / track click queued since the last frame.
        if let Some(new_offset) = self.scrollbar.take_pending_offset() {
            let delta = new_offset as i32 - info.display_offset as i32;
            if delta != 0 {
                session.update(cx, |s, _| s.scroll(delta));
                let new_info = session.read(cx).terminal_info();
                self.scrollbar.update(ScrollGeometry {
                    total_lines: new_info.total_lines,
                    viewport_lines: new_info.num_lines,
                    display_offset: new_info.display_offset,
                    line_height,
                });
            }
        }

        let theme_ref = cx.theme().clone();

        // Stamp the timestamp for new lines at the exact moment of this render
        // (see `GutterTimestamps`). Skip when the gutter is disabled to avoid
        // unnecessary work.
        if show_gutter {
            self.gutter_times.update(&info);
        }

        let theme = apply_color_overrides(theme, &color_overrides);

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
        let theme = apply_dynamic_colors(theme, &dynamic_colors);

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
        let profile = if session.read(cx).is_local() {
            shell_kind_to_profile(settings_entity.read(cx).shell.kind)
        } else {
            ShellProfile::Unix
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
                theme: theme.clone(),
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
            .children(bell_overlay(has_bell, bell_enabled, &theme_ref))
            .children(progress_overlay(self.progress, &theme_ref))
            .children(self.completion_overlay_element(cx))
            .children(self.render_scrollbar(&render_cache, cx))
            .children(self.render_search_bar(window, cx));

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
    theme_ref: &Theme,
) -> Option<impl IntoElement> {
    let (fraction, color) = match progress? {
        TerminalProgress::Remove => return None,
        TerminalProgress::Set(pct) => (pct as f32 / 100.0, theme_ref.blue),
        TerminalProgress::Error(pct) => (pct as f32 / 100.0, theme_ref.danger),
        TerminalProgress::Paused(pct) => (pct as f32 / 100.0, theme_ref.warning),
        // Indeterminate: no known percent → fill the whole track.
        TerminalProgress::Indeterminate => (1.0, theme_ref.blue),
    };
    Some(
        div()
            .id("terminal-progress")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(5.0))
            .bg(theme_ref.muted.opacity(0.4))
            .child(
                div()
                    .id("terminal-progress-fill")
                    .h_full()
                    .w(relative(fraction.clamp(0.0, 1.0)))
                    .bg(color),
            ),
    )
}

/// Bell indicator overlay (top-right corner).
fn bell_overlay(has_bell: bool, bell_enabled: bool, theme_ref: &Theme) -> Option<impl IntoElement> {
    (has_bell && bell_enabled).then(|| {
        div()
            .id("terminal-bell")
            .absolute()
            .top_1()
            .right_2()
            .px_1()
            .py_0()
            .text_xs()
            .text_color(theme_ref.warning)
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

    use super::terminal_font;

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
}
