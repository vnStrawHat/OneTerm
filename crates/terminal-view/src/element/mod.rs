//! `TerminalElement` — custom `gpui::Element` that paints the terminal grid from
//! a `TerminalContent` snapshot.
//!
//! This module is the orchestrator; render details live in:
//! - `element::prepaint` — compute layout state
//! - `element::paint` — draw the grid
//! - `element::measure` — measure font / cell metrics
//! - `element::gutter` — compute gutter width / entries

use std::collections::VecDeque;
use std::rc::Rc;

use gpui::{
    App, Bounds, Element, ElementId, Entity, Font, GlobalElementId, Hsla, IntoElement, LayoutId,
    Pixels, Window,
};

use oneterm_terminal::TerminalSession;

use super::highlight::SemanticOverlay;
pub(crate) use super::layout::{GridMetrics, LayoutState, RowLayoutCache};
use super::theme::TerminalTheme;
use super::view::LocalTerminalView;

pub(crate) mod gutter;
pub(crate) mod measure;
pub(crate) mod paint;
pub(crate) mod prepaint;

/// Element that paints the terminal. Holds `Entity<Box<dyn TerminalSession>>` to
/// resize in prepaint (per bounds) + get a fresh snapshot. The View passes a
/// cloned entity + theme + font.
pub(crate) struct TerminalElement {
    session: Entity<Box<dyn TerminalSession>>,
    theme: TerminalTheme,
    font: Font,
    font_size: Pixels,
    line_height_factor: f32,
    focused: bool,
    /// Whether to draw the cursor (blink logic: true = visible, false = hidden mid-blink).
    cursor_visible: bool,
    /// Sink for layout metrics used by the View (mouse/wheel).

    /// View entity — to register the IME input handler in paint.
    view: Entity<LocalTerminalView>,
    /// Focus handle for `handle_input`.
    focus: gpui::FocusHandle,
    /// URL currently hovered (Ctrl held) — highlight cells in range.
    hovered_url: Option<super::url::DetectedUrl>,
    /// Whether Ctrl is held.
    ctrl_held: bool,
    /// Toggle the gutter (timestamp + line number on the left of the terminal).
    pub show_gutter: bool,
    /// Padding around the terminal content (top/right/bottom/left px).
    padding: oneterm_settings::TerminalPadding,
    /// Cell width override (None = auto from font advance).
    cell_width_override: Option<f32>,
    /// Cursor color override (None = theme caret).
    cursor_color_override: Option<Hsla>,
    /// Cursor shape override from config (Block/Bar/Underline).
    /// Overrides the snapshot shape from the shell (except Hidden) — like Windows Terminal.
    cursor_shape_override: oneterm_settings::TerminalCursorShape,
    /// Per-line timestamps for gutter. `line_times[j]` ↔ line with absolute index
    /// `line_time_base + j`.
    line_times: Rc<VecDeque<String>>,
    /// Absolute index (0-based) of `line_times[0]`.
    line_time_base: usize,
    /// Render cache bundle — row layout, gutter width, grid size, metrics.
    render_cache: super::layout::types::TerminalRenderCache,
    /// Search highlights to paint (display coordinates, already filtered to the
    /// visible viewport).
    search_highlights: Vec<super::search::SearchHighlight>,
    /// Semantic overlay — produces per-cell Class for the visible viewport.
    overlay: SemanticOverlay,
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
        view: Entity<LocalTerminalView>,
        focus: gpui::FocusHandle,
        hovered_url: Option<super::url::DetectedUrl>,
        ctrl_held: bool,
        line_times: Rc<VecDeque<String>>,
        line_time_base: usize,
        padding: oneterm_settings::TerminalPadding,
        show_gutter: bool,
        cell_width_override: Option<f32>,
        cursor_color_override: Option<Hsla>,
        cursor_shape_override: oneterm_settings::TerminalCursorShape,
        render_cache: super::layout::types::TerminalRenderCache,

        search_highlights: Vec<super::search::SearchHighlight>,
        overlay: SemanticOverlay,
    ) -> Self {
        Self {
            session,
            theme,
            font,
            font_size,
            line_height_factor,
            focused,
            cursor_visible,
            render_cache,
            view,
            focus,
            hovered_url,
            ctrl_held,
            padding,
            show_gutter,
            cell_width_override,
            cursor_color_override,
            cursor_shape_override,
            line_times,
            line_time_base,

            search_highlights,
            overlay,
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
            self.show_gutter,
            &self.line_times,
            self.line_time_base,
            self.hovered_url.as_ref(),
            self.ctrl_held,
            &self.render_cache.cached_gutter,
            &self.render_cache.last_grid_size,
            &self.render_cache.metrics,
            &self.render_cache.row_cache,
            &self.search_highlights,
            &self.overlay,
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
            &self.render_cache.row_cache,
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
