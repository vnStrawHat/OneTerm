//! `impl Render + Focusable for LocalTerminalView` — split out from
//! `view.rs` to keep file length down.
//!
//! The original `render.rs` module was split into `render/`.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, Context, FocusHandle, Focusable, InteractiveElement as _, ParentElement as _, Render,
    SharedString, Styled as _, Window, div,
};
use gpui_component::ActiveTheme as _;
use gpui_component::WindowExt as _;

use super::element::TerminalElement;
use super::theme::{TerminalTheme, build_terminal_theme};
use super::view::LocalTerminalView;
use crate::state::TerminalSettings;

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
                window.push_notification(msg, cx);
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
            ))
            .children(self.bell_overlay(has_bell, bell_enabled, &theme_ref))
            .children(self.progress_overlay(&theme_ref))
            .children(self.vi_mode_overlay(&theme_ref))
            .children(self.vi_cursor_overlay(&theme_ref))
            .children(self.render_scrollbar(&theme, &metrics, cx))
            .children(self.breadcrumb_overlay(&session, &theme_ref, cx));

        super::handlers::attach(terminal_div, session, metrics, view, self.focus.clone())
    }
}
