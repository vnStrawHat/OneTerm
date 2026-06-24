//! `TerminalElement` — custom `gpui::Element` paint grid terminal từ
//! `TerminalContent` snapshot.
//!
//! Module này là orchestrator; chi tiết render nằm trong:
//! - `element::prepaint` — tính layout state
//! - `element::paint` — vẽ grid
//! - `element::measure` — đo font / cell metrics
//! - `element::gutter` — tính gutter width / entries

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    App, Bounds, Element, ElementId, Entity, Font, GlobalElementId, Hsla, IntoElement, LayoutId,
    Pixels, Window,
};

use myterm2_core::TerminalSession;

pub(crate) use super::layout::{GridMetrics, LayoutState, RowLayoutCache};
use super::terminal_view::LocalTerminalView;
use super::theme::TerminalTheme;

pub(crate) mod gutter;
pub(crate) mod measure;
pub(crate) mod paint;
pub(crate) mod prepaint;

/// Element paint terminal. Giữ `Entity<Box<dyn TerminalSession>>` để resize
/// trong prepaint (theo bounds) + snapshot tươi. View truyền entity
/// clone + theme + font.
pub(crate) struct TerminalElement {
    session: Entity<Box<dyn TerminalSession>>,
    theme: TerminalTheme,
    font: Font,
    font_size: Pixels,
    line_height_factor: f32,
    focused: bool,
    /// Có vẽ cursor không (blink logic: true = hiện, false = ẩn giữa blink).
    cursor_visible: bool,
    /// Lần resize gần nhất (tránh resize lặp).
    last_size: Option<(u16, u16)>,
    /// Sink layout metrics cho View (mouse/wheel).
    metrics: Rc<RefCell<GridMetrics>>,
    /// View entity — để đăng ký IME input handler ở paint.
    view: Entity<LocalTerminalView>,
    /// Focus handle cho `handle_input`.
    focus: gpui::FocusHandle,
    /// URL đang hover (Ctrl held) — highlight cells trong range.
    hovered_url: Option<super::url::DetectedUrl>,
    /// Ctrl đang held.
    ctrl_held: bool,
    /// Padding quanh terminal content (top/right/bottom/left px).
    padding: crate::state::TerminalPadding,
    /// Cell width override (None = auto từ font advance).
    cell_width_override: Option<f32>,
    /// Cursor color override (None = theme caret).
    cursor_color_override: Option<Hsla>,
    /// Cursor shape override từ config (Block/Bar/Underline).
    /// Override snapshot shape từ shell (trừ Hidden) — giống Windows Terminal.
    cursor_shape_override: crate::state::TerminalCursorShape,
    /// Per-line timestamps for gutter (0 = oldest line).
    line_times: Vec<String>,
    /// Per-row layout cache — skip recompute cho non-dirty rows.
    row_cache: Rc<RefCell<RowLayoutCache>>,
}

impl TerminalElement {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        session: Entity<Box<dyn TerminalSession>>,
        theme: TerminalTheme,
        font: Font,
        font_size: Pixels,
        line_height_factor: f32,
        focused: bool,
        cursor_visible: bool,
        metrics: Rc<RefCell<GridMetrics>>,
        view: Entity<LocalTerminalView>,
        focus: gpui::FocusHandle,
        hovered_url: Option<super::url::DetectedUrl>,
        ctrl_held: bool,
        line_times: Vec<String>,
        padding: crate::state::TerminalPadding,
        cell_width_override: Option<f32>,
        cursor_color_override: Option<Hsla>,
        cursor_shape_override: crate::state::TerminalCursorShape,
        row_cache: Rc<RefCell<RowLayoutCache>>,
    ) -> Self {
        Self {
            session,
            theme,
            font,
            font_size,
            line_height_factor,
            focused,
            cursor_visible,
            last_size: None,
            metrics,
            view,
            focus,
            hovered_url,
            ctrl_held,
            padding,
            cell_width_override,
            cursor_color_override,
            cursor_shape_override,
            line_times,
            row_cache,
        }
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = LayoutState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = gpui::Style::default();
        style.size.width = gpui::relative(1.).into();
        style.size.height = gpui::relative(1.).into();
        let id = window.request_layout(style, None, cx);
        (id, ())
    }

    fn prepaint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        prepaint::prepaint_terminal(
            &self.session,
            &self.view,
            &self.theme,
            &self.font,
            self.font_size,
            self.line_height_factor,
            self.cell_width_override,
            self.cursor_color_override,
            self.cursor_shape_override,
            self.padding,
            &self.line_times,
            self.hovered_url.as_ref(),
            self.ctrl_held,
            &mut self.last_size,
            &self.metrics,
            &self.row_cache,
            bounds,
            window,
            cx,
        )
    }

    fn paint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        paint::paint_terminal(
            &self.session,
            &self.view,
            &self.focus,
            &self.theme,
            &self.font,
            self.font_size,
            self.focused,
            self.cursor_visible,
            &self.row_cache,
            bounds,
            layout,
            window,
            cx,
        );
    }
}

impl IntoElement for TerminalElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}
