//! `impl Render + Focusable for LocalTerminalView` — tách từ
//! `view.rs` để giảm độ dài file.
//!
//! Module gốc `render.rs` đã được tách thành `render/`.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, Context, FocusHandle, Focusable, InteractiveElement as _, ParentElement as _, Render,
    SharedString, Styled as _, Window, div,
};
use gpui_component::ActiveTheme as _;

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

        {
            let total = info.total_lines;
            let absolute = info.absolute_line_count;
            if self.line_times.len() != total {
                let now = chrono::Local::now().format("%H:%M:%S").to_string();
                self.line_times.resize(total, now);
                self.prev_total_lines = total;
            }
            self.prev_absolute_line_count = absolute;
        }

        let theme = self.apply_color_overrides(theme, &color_overrides);

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
                padding,
                show_gutter,
                cell_width_override,
                cursor_color,
                cursor_shape,
                self.row_cache.clone(),
            ))
            .children(self.bell_overlay(has_bell, bell_enabled, &theme_ref))
            .children(self.vi_mode_overlay(&theme_ref))
            .children(self.vi_cursor_overlay(&theme_ref))
            .children(self.render_scrollbar(&theme, &metrics, cx))
            .children(self.breadcrumb_overlay(&session, &theme_ref, cx));

        super::handlers::attach(terminal_div, session, metrics, view, self.focus.clone())
    }
}
