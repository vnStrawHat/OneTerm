//! `LocalTerminalView` — GPUI `Render` render `TerminalElement` + wire session
//! events → `cx.notify` + keyboard + mouse/selection/wheel.
//!
//! Giữ `Entity<Box<dyn TerminalSession>>` (không biết local/ssh). #16: render +
//! events + keyboard. #17: mouse/selection/wheel. IME ở #19.

use std::cell::RefCell;
use std::rc::Rc;

use alacritty_terminal::selection::SelectionType;
use gpui::{
    App, ClipboardItem, Context, Entity, FocusHandle, Focusable, InteractiveElement as _,
    KeyDownEvent, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement as _, Pixels, Point, Render, ScrollDelta, ScrollWheelEvent, SharedString,
    Styled as _, Window, div, point, px, size,
};
use gpui_component::ActiveTheme as _;

use myterm2_core::terminal::{encode_key, KeyMods, KeySpec, NamedKey, TerminalMouseButton};
use myterm2_core::{SessionEvent, TerminalSession};

use super::terminal_element::{GridMetrics, TerminalElement};
use super::theme::{build_terminal_theme, TerminalTheme};

/// View render 1 terminal session (local hoặc ssh — qua `dyn TerminalSession`).
pub struct LocalTerminalView {
    session: Entity<Box<dyn TerminalSession>>,
    focus: FocusHandle,
    font_family: SharedString,
    font_size: Pixels,
    line_height_factor: f32,
    /// Sink layout metrics (Element ghi ở prepaint, mouse handler đọc).
    metrics: Rc<RefCell<GridMetrics>>,
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

        // Subscribe session events → cx.notify (re-render) + OSC 52 clipboard.
        let rx = session.read(cx).subscribe();
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
                    _ => {
                        let _ = this.update(cx, |_, cx| cx.notify());
                    }
                }
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
            "arrowup" => Some(NamedKey::ArrowUp),
            "arrowdown" => Some(NamedKey::ArrowDown),
            "arrowleft" => Some(NamedKey::ArrowLeft),
            "arrowright" => Some(NamedKey::ArrowRight),
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

    fn font(&self) -> gpui::Font {
        gpui::Font {
            family: self.font_family.clone().into(),
            weight: gpui::FontWeight::default(),
            style: gpui::FontStyle::Normal,
            fallbacks: None,
            features: Default::default(),
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
        let font = self.font();
        let metrics = self.metrics.clone();

        div()
            .id("local-terminal-view")
            .size_full()
            .track_focus(&self.focus)
            .child(TerminalElement::new(
                session.clone(),
                theme,
                font,
                self.font_size,
                self.line_height_factor,
                focused,
                metrics.clone(),
                cx.entity(),
                self.focus.clone(),
            ))
            .on_mouse_down(MouseButton::Left, {
                let s = session.clone();
                let m = metrics.clone();
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
                        s.mouse_down(row, col, map_button(e.button), Self::sel_type(e.click_count, e.modifiers.alt))
                    });
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
            .on_mouse_move({
                let s = session.clone();
                let m = metrics.clone();
                move |e: &MouseMoveEvent, _w, cx: &mut App| {
                    let (row, col) = match Self::pixel_to_grid(&m.borrow(), e.position) {
                        Some(rc) => rc,
                        None => return,
                    };
                    s.update(cx, |s, _| s.mouse_move(row, col));
                }
            })
            .on_mouse_up(MouseButton::Left, {
                let s = session.clone();
                let m = metrics.clone();
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
                }
            })
            .on_scroll_wheel({
                let s = session.clone();
                let m = metrics.clone();
                move |e: &ScrollWheelEvent, _w, cx: &mut App| {
                    let (row, col) = match Self::pixel_to_grid(&m.borrow(), e.position) {
                        Some(rc) => rc,
                        None => return,
                    };
                    let line_h = f32::from(m.borrow().line_height);
                    let delta_y = match e.delta {
                        ScrollDelta::Pixels(p) => {
                            if line_h > 0.0 { -f32::from(p.y) / line_h } else { 0.0 }
                        }
                        ScrollDelta::Lines(l) => -l.y,
                    };
                    if delta_y.abs() >= 0.001 {
                        s.update(cx, |s, _| s.wheel(delta_y as f64, row, col));
                    }
                }
            })
            .on_key_down({
                let s = session.clone();
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
            Some(UTF16Selection { range: (0..0), reversed: false })
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

    fn accepts_text_input(&self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> bool {
        true
    }
}