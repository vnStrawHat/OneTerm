//! `impl Render + Focusable for LocalTerminalView` — split out from
//! `view.rs` to keep file length down.
//!
//! The original `render.rs` module was split into `render/`.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, Context, FocusHandle, Focusable, InteractiveElement as _, ParentElement as _, Render,
    SharedString, Styled as _, Window, div,
};
use gpui_component::{ActiveTheme as _, WindowExt as _, notification::NotificationType};

use super::element::TerminalElement;
use super::highlight::SemanticOverlay;
use super::theme::{TerminalTheme, build_terminal_theme};
use super::view::LocalTerminalView;
use crate::notif_ext::notify;
use crate::state::{SemanticHighlightingMode, TerminalSettings};
use oneterm_core::config::ShellKind;
use oneterm_highlight::ShellProfile;

pub(crate) mod overlays;
pub(crate) mod theme_apply;

impl Focusable for LocalTerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for LocalTerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        // Drain OSC 9 notifications here (needs a `Window`, unavailable in the
        // async subscribe task where they are queued).
        if !self.pending_notifications.is_empty() {
            for msg in std::mem::take(&mut self.pending_notifications) {
                window.push_notification(notify(NotificationType::Info, msg, cx), cx);
            }
        }

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
                self.font(settings, &effective_family),
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
            )
        };
        let metrics = self.metrics.clone();
        let view = cx.entity();

        let info = session.read(cx).terminal_info();
        let m = *metrics.borrow();
        self.scroll_handle.update(
            info.total_lines,
            info.num_lines,
            info.display_offset,
            f32::from(m.line_height),
        );

        if let Some(new_offset) = self.scroll_handle.take_future_display_offset() {
            let delta = new_offset as i32 - info.display_offset as i32;
            if delta != 0 {
                session.update(cx, |s, _| s.scroll(delta));
                let new_info = session.read(cx).terminal_info();
                self.scroll_handle.update(
                    new_info.total_lines,
                    new_info.num_lines,
                    new_info.display_offset,
                    f32::from(m.line_height),
                );
            }
        }

        let theme_ref = cx.theme().clone();

        // The single source that updates line_times — stamp the timestamp for new
        // lines at the exact moment of this render (see `update_line_times`).
        self.update_line_times(&info);

        let theme = self.apply_color_overrides(theme, &color_overrides);

        // Push the effective default fg/bg/cursor + ANSI palette to the backend
        // so OSC 10/11/12 and OSC 4 *queries* can be answered, then apply OSC-set
        // dynamic colors on top so OSC *sets* (OSC 4/10/11/12) and *resets*
        // (OSC 104/110/111/112) take effect.
        {
            let session_ref = session.read(cx);
            session_ref.set_default_colors(
                theme.palette.foreground,
                theme.palette.background,
                theme.palette.cursor,
                theme.palette.ansi,
            );
        }
        let dynamic_colors = session.read(cx).dynamic_colors();
        let theme = theme_apply::apply_dynamic_colors(theme, &dynamic_colors);

        // ── Search highlights (display coordinates, visible only) ──
        let search_highlights =
            self.visible_search_highlights(info.display_offset, info.num_lines, info.num_cols);

        // Semantic overlay (Layer 2) -- Auto/On = enabled; Off = disabled.
        let semantic_enabled = !matches!(semantic_highlighting, SemanticHighlightingMode::Off);
        // Select the shell profile for semantic highlighting:
        // - Local session: use the configured ShellKind (Cmd/PowerShell/Unix/...)
        // - SSH session: always Unix (remote hosts are virtually always Unix)
        let profile = if session.read(cx).is_local() {
            shell_kind_to_profile(settings_entity.read(cx).shell.kind)
        } else {
            ShellProfile::Unix
        };
        let overlay = SemanticOverlay::new(profile, semantic_enabled);

        let terminal_div = div()
            .id("local-terminal-view")
            .size_full()
            .relative()
            .track_focus(&self.focus)
            .key_context("Terminal")
            .when(self.hovered_url.is_some(), |d| d.cursor_pointer())
            .child(TerminalElement::new(
                session.clone(),
                theme.clone(),
                font,
                font_size,
                line_height_factor,
                focused,
                cursor_visible,
                metrics.clone(),
                cx.entity(),
                self.focus.clone(),
                self.hovered_url.clone(),
                self.ctrl_held,
                self.line_times.clone(),
                self.line_time_base,
                padding,
                show_gutter,
                cell_width_override,
                cursor_color,
                cursor_shape,
                self.row_cache.clone(),
                self.cached_gutter.clone(),
                self.last_grid_size.clone(),
                search_highlights,
                overlay,
            ))
            .children(self.bell_overlay(has_bell, bell_enabled, &theme_ref))
            .children(self.progress_overlay(&theme_ref))
            .children(self.render_scrollbar(&theme, &metrics, cx))
            .children(self.render_search_bar(window, cx));

        let split_ctx = self.split_ctx.clone();
        super::handlers::attach(
            terminal_div,
            session,
            metrics,
            view,
            self.focus.clone(),
            split_ctx,
        )
    }
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
