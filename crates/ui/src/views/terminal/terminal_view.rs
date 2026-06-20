//! `LocalTerminalView` — GPUI `Render` render `TerminalElement` + wire session
//! events → `cx.notify` + keyboard + mouse/selection/wheel.
//!
//! Giữ `Entity<Box<dyn TerminalSession>>` (không biết local/ssh). #16: render +
//! events + keyboard. #17: mouse/selection/wheel. IME ở #19.
//!
//! Group A: cursor blink (500ms timer), cursor shape config, bell indicator,
//! font features.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use alacritty_terminal::selection::SelectionType;
use gpui::{
    App, ClipboardItem, Context, Entity, FocusHandle, Focusable, InteractiveElement as _,
    KeyBinding, KeyDownEvent, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, NoAction, ParentElement as _, Pixels, Point, Render, ScrollDelta,
    ScrollWheelEvent, SharedString, Styled as _, Window, div, point, px, size,
};
use gpui_component::ActiveTheme as _;
use gpui_component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_component::scroll::Scrollbar;

use myterm2_core::terminal::{KeyMods, KeySpec, NamedKey, TerminalMouseButton, encode_key};
use myterm2_core::{SessionEvent, TerminalSession};

use super::terminal_element::{GridMetrics, TerminalElement};
use super::terminal_scrollbar::TerminalScrollHandle;
use super::theme::{TerminalTheme, build_terminal_theme};
use crate::state::{TerminalBlink, TerminalSettings};

/// Khoảng thời gian nhấp nháy con trỏ (ms) — giống Zed `CURSOR_BLINK_INTERVAL`.
const CURSOR_BLINK_INTERVAL_MS: u64 = 500;

/// View render 1 terminal session (local hoặc ssh — qua `dyn TerminalSession`).
pub struct LocalTerminalView {
    session: Entity<Box<dyn TerminalSession>>,
    focus: FocusHandle,
    font_family: SharedString,
    font_size: Pixels,
    line_height_factor: f32,
    /// Sink layout metrics (Element ghi ở prepaint, mouse handler đọc).
    metrics: Rc<RefCell<GridMetrics>>,
    /// Scrollbar handle — cache scrollback state, apply drag → session.
    scroll_handle: TerminalScrollHandle,
    /// Con trỏ có đang hiện không (blink toggle). True = vẽ, false = ẩn.
    cursor_blink_visible: bool,
    /// Bell indicator — true khi nhận `\x07`, clear khi user gõ phím.
    has_bell: bool,
}

impl LocalTerminalView {
    /// Tạo view từ session entity. Subscribe events → re-render task.
    pub fn new(
        session: Entity<Box<dyn TerminalSession>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let theme = cx.theme().clone();
        let font_family = theme.mono_font_family.clone();
        let font_size = theme.mono_font_size;

        // Tab/Shift+Tab: gpui-component root binds "tab" → focus_next.
        // NoAction trong context "Terminal" (depth cao hơn "Root") → override →
        // Tab rơi vào on_key_down → gửi \t vào PTY (shell auto-complete).
        cx.bind_keys([
            KeyBinding::new("tab", NoAction {}, Some("Terminal")),
            KeyBinding::new("shift-tab", NoAction {}, Some("Terminal")),
        ]);

        // Subscribe session events → cx.notify (re-render) + OSC 52 clipboard.
        // Burst-coalescing: khi output dồn (vd `cat` file lớn), nhiều Wakeup
        // liên tiếp → chỉ notify 1 lần, drain các Output event còn trong queue.
        let rx = session.read(cx).subscribe();
        let session_for_spawn = session.clone();
        cx.spawn(async move |this, cx| {
            while let Ok(ev) = rx.recv().await {
                match ev {
                    SessionEvent::Clipboard(Some(t)) => {
                        let _ = this.update(cx, |_, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(t));
                        });
                    }
                    SessionEvent::Clipboard(None) => {
                        let _ = this.update(cx, |_, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
                        });
                    }
                    SessionEvent::Output => {
                        // Scroll to bottom + Notify 1 lần rồi drain tất cả
                        // Output event đang chờ trong queue → tránh re-render
                        // từng event khi `cat` file lớn (hàng nghìn Wakeup).
                        let _ = this.update(cx, |_, cx| cx.notify());
                        let s = session_for_spawn.clone();
                        let _ = this.update(cx, |_, cx| {
                            s.read(cx).scroll_to_bottom();
                        });
                        loop {
                            match rx.try_recv() {
                                Ok(SessionEvent::Output) => {} // coalesced
                                Ok(SessionEvent::Clipboard(Some(t))) => {
                                    let _ = this.update(cx, |_, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(t));
                                    });
                                }
                                Ok(SessionEvent::Clipboard(None)) => {
                                    let _ = this.update(cx, |_, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            String::new(),
                                        ));
                                    });
                                }
                                Ok(SessionEvent::Bell) => {
                                    let _ = this.update(cx, |view, cx| {
                                        view.has_bell = true;
                                        cx.notify();
                                    });
                                }
                                Ok(_) => {
                                    // Title/Cwd/Exited/Closed → notify.
                                    let _ = this.update(cx, |_, cx| cx.notify());
                                }
                                Err(_) => break,
                            }
                        }
                        // Follow-up renders: ConPTY (Windows) output có thể đến
                        // thành nhiều đợt (bursty). Sau khi drain xong, data mới
                        // có thể vẫn đang được background thread đọc từ ConPTY →
                        // thêm 2 re-render trì hoãn để bắt data đến muộn.
                        let this_a = this.clone();
                        let this_b = this.clone();
                        cx.spawn(async move |cx| {
                            cx.background_executor().timer(Duration::from_millis(50)).await;
                            let _ = this_a.update(cx, |_, cx| cx.notify());
                            cx.background_executor().timer(Duration::from_millis(150)).await;
                            let _ = this_b.update(cx, |_, cx| cx.notify());
                        }).detach();
                    }
                    SessionEvent::Bell => {
                        let _ = this.update(cx, |view, cx| {
                            view.has_bell = true;
                            cx.notify();
                        });
                    }
                    _ => {
                        let _ = this.update(cx, |_, cx| cx.notify());
                    }
                }
            }
        })
        .detach();

        // Cursor blink timer — toggle visible mỗi 500ms.
        // Chỉ nhấp nháy khi focus + blink on. Timer luôn chạy, View quyết định
        // có vẽ không dựa trên focused + cursor_blink_visible.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(CURSOR_BLINK_INTERVAL_MS))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    view.cursor_blink_visible = !view.cursor_blink_visible;
                    cx.notify();
                });
            }
        })
        .detach();

        Self {
            session,
            focus,
            font_family,
            font_size,
            line_height_factor: 1.2,
            metrics: Rc::new(RefCell::new(GridMetrics::default())),
            scroll_handle: TerminalScrollHandle::new(),
            cursor_blink_visible: true,
            has_bell: false,
        }
    }

    /// Map GPUI `Keystroke` → `KeySpec` + `KeyMods`.
    fn map_key(keystroke: &Keystroke) -> Option<(KeySpec, KeyMods)> {
        let mods = keystroke.modifiers;
        let keymods = KeyMods {
            shift: mods.shift,
            ctrl: mods.control,
            alt: mods.alt,
        };
        let named = match keystroke.key.as_str() {
            "enter" | "return" => Some(NamedKey::Enter),
            "backspace" => Some(NamedKey::Backspace),
            "delete" => Some(NamedKey::Delete),
            "tab" => Some(NamedKey::Tab),
            "escape" => Some(NamedKey::Escape),
            // GPUI uses "up"/"down"/"left"/"right" (not "arrowup"/...).
            "up" => Some(NamedKey::ArrowUp),
            "down" => Some(NamedKey::ArrowDown),
            "left" => Some(NamedKey::ArrowLeft),
            "right" => Some(NamedKey::ArrowRight),
            "home" => Some(NamedKey::Home),
            "end" => Some(NamedKey::End),
            "pageup" => Some(NamedKey::PageUp),
            "pagedown" => Some(NamedKey::PageDown),
            "insert" => Some(NamedKey::Insert),
            _ => None,
        };
        let spec = if let Some(n) = named {
            KeySpec::Named(n)
        } else {
            let ch = keystroke
                .key_char
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| keystroke.key.clone());
            KeySpec::Character(ch)
        };
        Some((spec, keymods))
    }

    fn font(&self, settings: &TerminalSettings) -> gpui::Font {
        gpui::Font {
            family: self.font_family.clone().into(),
            weight: gpui::FontWeight::default(),
            style: gpui::FontStyle::Normal,
            fallbacks: None,
            features: gpui::FontFeatures(std::sync::Arc::new(
                settings.font_features.iter().map(|f| (f.to_string(), 1u32)).collect()
            )),
        }
    }

    /// Convert pixel position → (row, col) display (0-based từ top viewport).
    fn pixel_to_grid(metrics: &GridMetrics, pos: Point<Pixels>) -> Option<(f32, f32)> {
        let b = metrics.bounds?;
        if f32::from(metrics.cell_width) == 0.0 || f32::from(metrics.line_height) == 0.0 {
            return None;
        }
        let x = f32::from(pos.x - b.origin.x);
        let y = f32::from(pos.y - b.origin.y);
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let col = x / f32::from(metrics.cell_width);
        let row = y / f32::from(metrics.line_height);
        Some((row, col))
    }

    /// Selection type theo click count + alt.
    fn sel_type(click_count: usize, alt: bool) -> SelectionType {
        if alt {
            SelectionType::Block
        } else {
            match click_count {
                2 => SelectionType::Semantic,
                n if n >= 3 => SelectionType::Lines,
                _ => SelectionType::Simple,
            }
        }
    }

    /// Quyết định có vẽ cursor không (blink logic).
    /// - Không focus → luôn vẽ (để user thấy cursor ở đâu).
    /// - Focus + blink off → luôn vẽ.
    /// - Focus + blink on → vẽ khi cursor_blink_visible.
    fn should_show_cursor(&self, focused: bool, settings: &TerminalSettings) -> bool {
        if !focused {
            return true;
        }
        match settings.cursor_blink {
            TerminalBlink::Off => true,
            TerminalBlink::On => self.cursor_blink_visible,
        }
    }
}

fn map_button(b: MouseButton) -> TerminalMouseButton {
    match b {
        MouseButton::Left => TerminalMouseButton::Left,
        MouseButton::Right => TerminalMouseButton::Right,
        MouseButton::Middle => TerminalMouseButton::Middle,
        MouseButton::Navigate(_) => TerminalMouseButton::Left,
    }
}

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
        let (font, cursor_visible, bell_enabled, has_bell) = {
            let settings = settings_entity.read(cx);
            (
                self.font(settings),
                self.should_show_cursor(focused, settings),
                settings.bell_enabled,
                self.has_bell,
            )
        };
        let metrics = self.metrics.clone();
        let view = cx.entity();

        // Cập nhật scroll handle từ snapshot (frame trước — metrics đã có
        // line_height từ prepaint lần trước).
        let snap = session.read(cx).snapshot();
        let m = *metrics.borrow();
        self.scroll_handle.update(
            snap.total_lines,
            snap.terminal_bounds.num_lines,
            snap.display_offset,
            f32::from(m.line_height),
        );

        // Áp dụng future_display_offset từ scrollbar drag.
        if let Some(new_offset) = self.scroll_handle.take_future_display_offset() {
            let delta = new_offset as i32 - snap.display_offset as i32;
            if delta != 0 {
                session.update(cx, |s, _| s.scroll(delta));
            }
        }

        let theme_ref = cx.theme().clone();

        div()
            .id("local-terminal-view")
            .size_full()
            .relative()
            .track_focus(&self.focus)
            .key_context("Terminal")
            .child(TerminalElement::new(
                session.clone(),
                theme,
                font,
                self.font_size,
                self.line_height_factor,
                focused,
                cursor_visible,
                metrics.clone(),
                cx.entity(),
                self.focus.clone(),
            ))
            // Bell indicator overlay (góc trên-phải).
            .children(if has_bell && bell_enabled {
                Some(div().id("terminal-bell").absolute().top_1().right_2().px_1().py_0().text_xs().text_color(theme_ref.warning).child("🔔"))
            } else {
                None
            })
            // Scrollbar dọc — overlay absolute, tự ẩn khi không có scrollback.
            .child(Scrollbar::vertical(&self.scroll_handle))
            .on_mouse_down(MouseButton::Left, {
                let s = session.clone();
                let m = metrics.clone();
                let view = view.clone();
                move |e: &MouseDownEvent, _w, cx: &mut App| {
                    let (row, col) = match Self::pixel_to_grid(&m.borrow(), e.position) {
                        Some(rc) => rc,
                        None => return,
                    };
                    // Ctrl+click trên cell có hyperlink → mở URL.
                    if e.modifiers.control {
                        let snap = s.read(cx).snapshot();
                        let nc = snap.terminal_bounds.num_cols;
                        let idx = (row as usize) * nc + (col as usize);
                        if idx < snap.cells.len() {
                            if let Some(h) = snap.cells[idx].cell.hyperlink() {
                                cx.open_url(h.uri());
                                return;
                            }
                        }
                    }
                    s.update(cx, |s, _| {
                        s.mouse_down(
                            row,
                            col,
                            map_button(e.button),
                            Self::sel_type(e.click_count, e.modifiers.alt),
                        )
                    });
                    // Trigger re-render để vẽ selection highlight.
                    let _ = view.update(cx, |_, cx| cx.notify());
                }
            })
            .on_mouse_down(MouseButton::Middle, {
                let s = session.clone();
                move |_e: &MouseDownEvent, _w, cx: &mut App| {
                    // Middle-click = paste (X11 PRIMARY/CLIPBOARD).
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            s.update(cx, |s, _| s.write(text.as_bytes()));
                        }
                    }
                }
            })
            // Mouse move — xử lý cả drag (left button held) và hover (no button).
            // Drag: cập nhật selection end point (non-mouse mode) hoặc encode drag (mouse mode).
            // Hover: encode mouse motion cho app mode (vim/less/htop).
            .on_mouse_move({
                let s = session.clone();
                let m = metrics.clone();
                let view = view.clone();
                move |e: &MouseMoveEvent, _w, cx: &mut App| {
                    let (row, col) = match Self::pixel_to_grid(&m.borrow(), e.position) {
                        Some(rc) => rc,
                        None => return,
                    };
                    if e.pressed_button == Some(MouseButton::Left) {
                        // Drag: cập nhật selection.
                        s.update(cx, |s, _| s.mouse_drag(row, col));
                        let _ = view.update(cx, |_, cx| cx.notify());
                    } else {
                        // Hover: encode mouse motion (cho app mode).
                        s.update(cx, |s, _| s.mouse_move(row, col));
                    }
                }
            })
            .on_mouse_up(MouseButton::Left, {
                let s = session.clone();
                let m = metrics.clone();
                let view = view.clone();
                move |e: &MouseUpEvent, _w, cx: &mut App| {
                    let (row, col) = match Self::pixel_to_grid(&m.borrow(), e.position) {
                        Some(rc) => rc,
                        None => return,
                    };
                    s.update(cx, |s, _| s.mouse_up(row, col, map_button(e.button)));
                    // Select-to-copy: có selection → clipboard.
                    if let Some(text) = s.read(cx).selection_text() {
                        if !text.is_empty() {
                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                        }
                    }
                    // Re-render để vẽ selection cuối cùng.
                    let _ = view.update(cx, |_, cx| cx.notify());
                }
            })
            .on_scroll_wheel({
                let s = session.clone();
                let m = metrics.clone();
                let view = view.clone();
                move |e: &ScrollWheelEvent, _w, cx: &mut App| {
                    let (row, col) = match Self::pixel_to_grid(&m.borrow(), e.position) {
                        Some(rc) => rc,
                        None => return,
                    };
                    let line_h = f32::from(m.borrow().line_height);
                    let delta_y = match e.delta {
                        ScrollDelta::Pixels(p) => {
                            if line_h > 0.0 {
                                f32::from(p.y) / line_h
                            } else {
                                0.0
                            }
                        }
                        ScrollDelta::Lines(l) => l.y,
                    };
                    if delta_y.abs() >= 0.001 {
                        s.update(cx, |s, _| s.wheel(delta_y as f64, row, col));
                        // Re-render để cập nhật scroll handle state → scrollbar
                        // hiện (offset thay đổi → scrollbar fade in).
                        let _ = view.update(cx, |_, cx| cx.notify());
                    }
                }
            })
            .on_key_down({
                let s = session.clone();
                let view = view.clone();
                move |e: &KeyDownEvent, _w, cx: &mut App| {
                    let mods = e.keystroke.modifiers;
                    // Ctrl+Shift+C = copy, Ctrl+Shift+V = paste (terminal: Ctrl+C
                    // là SIGINT nên dùng Shift).
                    if mods.control && mods.shift {
                        match e.keystroke.key.as_str() {
                            "c" => {
                                if let Some(text) = s.read(cx).selection_text() {
                                    if !text.is_empty() {
                                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                                    }
                                }
                                return;
                            }
                            "v" => {
                                if let Some(item) = cx.read_from_clipboard() {
                                    if let Some(text) = item.text() {
                                        s.update(cx, |s, _| s.write(text.as_bytes()));
                                    }
                                }
                                return;
                            }
                            _ => {}
                        }
                    }
                    // IME active (không alt-screen): ký tự thường do
                    // replace_text_in_range lo → skip on_key_down để tránh double.
                    // Alt-screen (vim/less): IME tắt → on_key_down lo ký tự thường.
                    if !s.read(cx).is_alt_screen() {
                        let m = e.keystroke.modifiers;
                        if !m.control && !m.alt && !m.platform {
                            if let Some(ch) = e.keystroke.key_char.as_deref() {
                                if !ch.is_empty() && !ch.chars().any(|c| c.is_control()) {
                                    return; // để replace_text_in_range ghi
                                }
                            }
                        }
                    }
                    let Some((spec, mods)) = Self::map_key(&e.keystroke) else {
                        return;
                    };
                    let Some(bytes) = encode_key(&spec, mods) else {
                        return;
                    };
                    s.update(cx, |s, _| s.write(&bytes));
                    // Clear bell indicator khi user gõ phím.
                    let _ = view.update(cx, |view, cx| {
                        if view.has_bell {
                            view.has_bell = false;
                            cx.notify();
                        }
                    });
                    // Ngăn GPUI xử lý tiếp (vd Tab = focus traversal, arrow =
                    // scroll). Nếu không stop_propagation, focus sẽ bị chuyển đi.
                    cx.stop_propagation();
                }
            })
            // Right-click context menu: Copy / Paste / Select All / Clear.
            .context_menu({
                let session = session.clone();
                let focus = self.focus.clone();
                move |menu, _window, cx| {
                    let has_selection = session
                        .read(cx)
                        .selection_text()
                        .map(|t| !t.is_empty())
                        .unwrap_or(false);

                    menu.item(
                        PopupMenuItem::new("Copy")
                            .disabled(!has_selection)
                            .on_click({
                                let s = session.clone();
                                let f = focus.clone();
                                move |_, window, cx| {
                                    if let Some(text) = s.read(cx).selection_text() {
                                        if !text.is_empty() {
                                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                                        }
                                    }
                                    window.focus(&f, cx);
                                }
                            }),
                    )
                    .item(PopupMenuItem::new("Paste").on_click({
                        let s = session.clone();
                        let f = focus.clone();
                        move |_, window, cx| {
                            if let Some(item) = cx.read_from_clipboard() {
                                if let Some(text) = item.text() {
                                    s.update(cx, |s, _| s.write(text.as_bytes()));
                                }
                            }
                            window.focus(&f, cx);
                        }
                    }))
                    .separator()
                    .item(PopupMenuItem::new("Select All").on_click({
                        let s = session.clone();
                        let f = focus.clone();
                        move |_, window, cx| {
                            s.update(cx, |s, _| s.select_all());
                            window.focus(&f, cx);
                        }
                    }))
                    .item(PopupMenuItem::new("Clear").on_click({
                        let s = session.clone();
                        let f = focus.clone();
                        move |_, window, cx| {
                            s.update(cx, |s, _| s.clear());
                            window.focus(&f, cx);
                        }
                    }))
                }
            })
    }
}

// ── IME (#19) ──────────────────────────────────────────────────────────────
use alacritty_terminal::term::TermMode;
use gpui::{EntityInputHandler, UTF16Selection};

impl EntityInputHandler for LocalTerminalView {
    fn text_for_range(
        &mut self,
        _range: std::ops::Range<usize>,
        _adjusted: &mut Option<std::ops::Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        // Alt-screen (vd vim/less): tắt IME.
        let mode = self.session.read(cx).snapshot().mode;
        if mode.contains(TermMode::ALT_SCREEN) {
            None
        } else {
            Some(UTF16Selection {
                range: (0..0),
                reversed: false,
            })
        }
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        self.session
            .read(cx)
            .marked_text()
            .map(|t| 0..t.encode_utf16().count())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.session.update(cx, |s, _| s.clear_marked_text());
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<std::ops::Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Commit IME hoặc ký tự thường (normal mode). Đây là nguồn ghi tin cậy —
        // on_key_down skip ký tự thường khi IME active để tránh double (aa).
        self.session.update(cx, |s, _| s.commit_text(text));
        // Clear bell khi user gõ.
        if self.has_bell {
            self.has_bell = false;
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<std::ops::Range<usize>>,
        new_text: &str,
        _new_selected: Option<std::ops::Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if new_text.is_empty() {
            self.session.update(cx, |s, _| s.clear_marked_text());
        } else {
            self.session
                .update(cx, |s, _| s.set_marked_text(new_text.to_string()));
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        element_bounds: gpui::Bounds<Pixels>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Bounds<Pixels>> {
        let cur = self.session.read(cx).cursor_bounds()?;
        let m = *self.metrics.borrow();
        let cw = f32::from(m.cell_width).max(1.0);
        let lh = f32::from(m.line_height).max(1.0);
        let x = f32::from(element_bounds.origin.x) + cur.x + cw * range_utf16.start as f32;
        let y = f32::from(element_bounds.origin.y) + cur.y;
        Some(gpui::Bounds::new(point(px(x), px(y)), size(px(cw), px(lh))))
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        true
    }
}