//! `impl Render + Focusable for LocalTerminalView` — tách từ
//! `terminal_view.rs` để giảm độ dài file.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, Context, FocusHandle, Focusable, InteractiveElement as _, ParentElement as _, Render,
    SharedString, Styled as _, Window, div, px,
};
use gpui_component::ActiveTheme as _;

use super::terminal_element::TerminalElement;
use super::terminal_view::LocalTerminalView;
use super::theme::{TerminalTheme, build_terminal_theme};
use crate::state::TerminalSettings;

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
        // Đọc settings + extract dữ liệu cần thiết trước khi mutate session.
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
        ) = {
            let settings = settings_entity.read(cx);
            let gpui_theme = cx.theme();
            // Effective font family: settings override → theme mono font.
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
            )
        };
        let metrics = self.metrics.clone();
        let view = cx.entity();

        // Cập nhật scroll handle từ terminal_info (KHÔNG clear damage —
        // prepaint cần damage để biết rows nào dirty để recompute colors).
        let info = session.read(cx).terminal_info();
        let m = *metrics.borrow();
        self.scroll_handle.update(
            info.total_lines,
            info.num_lines,
            info.display_offset,
            f32::from(m.line_height),
        );

        // Áp dụng future_display_offset từ scrollbar drag.
        if let Some(new_offset) = self.scroll_handle.take_future_display_offset() {
            let delta = new_offset as i32 - info.display_offset as i32;
            if delta != 0 {
                session.update(cx, |s, _| s.scroll(delta));
                // Re-info để cập nhật scroll handle với display_offset MỚI
                // (trong cùng frame — tránh lag 1 frame).
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

        // Safety sync: đảm bảo line_times.len() == info.total_lines.
        // Output có thể đến giữa lần update cuối cùng của event handler và
        // frame render này → line_times bị thiếu → gutter hiện --:--:--.
        {
            let total = info.total_lines;
            if self.line_times.len() != total {
                let now = chrono::Local::now().format("%H:%M:%S").to_string();
                while self.line_times.len() < total {
                    self.line_times.push(now.clone());
                }
                while self.line_times.len() > total {
                    self.line_times.pop();
                }
                self.prev_total_lines = total;
            }
        }

        // Apply color overrides từ config → theme.
        let theme = {
            let mut t = theme;
            let co = &color_overrides;
            if let Some(fg) = co.foreground {
                t.fg = fg;
                t.palette.foreground = super::theme::vte_from_rgba(fg.to_rgb());
            }
            if let Some(bg) = co.background {
                t.bg = bg;
                t.palette.background = super::theme::vte_from_rgba(bg.to_rgb());
            }
            if let Some(c) = co.cursor {
                t.palette.cursor = super::theme::vte_from_rgba(c.to_rgb());
            }
            if let Some(sel) = co.selection {
                t.selection = sel;
            }
            if let Some(gf) = co.gutter_fg {
                t.gutter_fg = gf;
                t.clock_fg = gf;
                t.line_number_fg = gf;
            }
            if let Some(gb) = co.gutter_bg {
                t.gutter_bg = gb;
            }
            if let Some(cf) = co.clock_fg {
                t.clock_fg = cf;
            }
            if let Some(lnf) = co.line_number_fg {
                t.line_number_fg = lnf;
            }
            t.min_contrast = co.min_contrast;
            for (i, &color) in co.ansi.iter().enumerate() {
                if i < 16 {
                    t.palette.ansi[i] = super::theme::vte_from_rgba(color.to_rgb());
                }
            }
            t
        };

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
                cell_width_override,
                cursor_color,
                cursor_shape,
                self.row_cache.clone(),
            ))
            // Bell indicator overlay (góc trên-phải).
            .children(if has_bell && bell_enabled {
                Some(
                    div()
                        .id("terminal-bell")
                        .absolute()
                        .top_1()
                        .right_2()
                        .px_1()
                        .py_0()
                        .text_xs()
                        .text_color(theme_ref.warning)
                        .child("🔔"),
                )
            } else {
                None
            })
            // ── Vi mode indicator (góc trên-trái) ──
            .children(if self.vi_mode {
                Some(
                    div()
                        .id("terminal-vi-mode")
                        .absolute()
                        .top_1()
                        .left_2()
                        .px_2()
                        .py_0p5()
                        .text_xs()
                        .rounded_sm()
                        .bg(theme_ref.accent.opacity(0.8))
                        .text_color(theme_ref.foreground)
                        .child(if self.vi_selecting {
                            "-- VISUAL --"
                        } else {
                            "-- NORMAL --"
                        }),
                )
            } else {
                None
            })
            // ── Vi mode cursor overlay ──
            .children(if self.vi_mode {
                let m = *metrics.borrow();
                let cw = f32::from(m.cell_width);
                let lh = f32::from(m.line_height);
                if cw > 0.0 && lh > 0.0 {
                    if let Some(bounds) = m.bounds {
                        let x = f32::from(bounds.origin.x + m.gutter_width)
                            + self.vi_cursor.1 as f32 * cw;
                        let y = f32::from(bounds.origin.y) + self.vi_cursor.0 as f32 * lh;
                        Some(
                            div()
                                .id("vi-cursor")
                                .absolute()
                                .left(px(x))
                                .top(px(y))
                                .w(px(cw))
                                .h(px(lh))
                                .border_1()
                                .border_color(theme_ref.accent)
                                .rounded_sm(),
                        )
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            })
            // ── Custom scrollbar ──
            .children(self.render_scrollbar(&theme, &metrics, cx))
            // ── Breadcrumb bar (bottom) — cwd path từ OSC 7 ──
            .children({
                let breadcrumb = session.read(cx).breadcrumb_text();
                let fg_process = session.read(cx).foreground_process();
                if let Some(bc) = breadcrumb {
                    let label = if let Some(proc) = &fg_process {
                        format!("{} — {}", proc, bc)
                    } else {
                        bc
                    };
                    Some(
                        div()
                            .id("terminal-breadcrumb")
                            .absolute()
                            .bottom_0()
                            .left_0()
                            .right_0()
                            .h(px(20.0))
                            .flex()
                            .items_center()
                            .px_2()
                            .text_xs()
                            .text_color(theme_ref.border)
                            .bg(theme_ref.background.opacity(0.9))
                            .child(label),
                    )
                } else {
                    None
                }
            });

        super::terminal_handlers::attach(terminal_div, session, metrics, view, self.focus.clone())
    }
}
