//! `TerminalElement` — custom `gpui::Element` that paints the terminal grid from
//! a `TerminalContent` snapshot.
//!
//! This is the orchestrator type; render details live in:
//! - [`prepaint`](super::prepaint) — compute layout state
//! - [`paint`](super::paint) — draw the grid
//! - [`measure`](super::measure) — measure font / cell metrics
//! - [`gutter`](super::gutter) — compute gutter width / entries

use std::collections::VecDeque;
use std::rc::Rc;

use gpui::{
    App, Bounds, Element, ElementId, Entity, Font, GlobalElementId, Hsla, IntoElement, LayoutId,
    Pixels, Window,
};

use oneterm_terminal::TerminalSession;

use super::super::highlight::SemanticOverlay;
use super::super::layout::{self, LayoutState};
use super::super::search::SearchHighlight;
use super::super::theme::TerminalTheme;
use super::super::view::LocalTerminalView;

/// Element that paints the terminal. Holds `Entity<Box<dyn TerminalSession>>` to
/// resize in prepaint (per bounds) + get a fresh snapshot. The View passes a
/// cloned entity + theme + font.
pub(crate) struct TerminalElement {
    pub(crate) session: Entity<Box<dyn TerminalSession>>,
    pub(crate) theme: TerminalTheme,
    pub(crate) font: Font,
    pub(crate) font_size: Pixels,
    pub(crate) line_height_factor: f32,
    pub(crate) focused: bool,
    /// Whether to draw the cursor (blink logic: true = visible, false = hidden mid-blink).
    pub(crate) cursor_visible: bool,
    /// View entity — to register the IME input handler in paint.
    pub(crate) view: Entity<LocalTerminalView>,
    /// Focus handle for `handle_input`.
    pub(crate) focus: gpui::FocusHandle,
    /// Toggle the gutter (timestamp + line number on the left of the terminal).
    pub(crate) show_gutter: bool,
    /// Padding around the terminal content (top/right/bottom/left px).
    pub(crate) padding: oneterm_settings::TerminalPadding,
    /// Cell width override (None = auto from font advance).
    pub(crate) cell_width_override: Option<f32>,
    /// Cursor color override (None = theme caret).
    pub(crate) cursor_color_override: Option<Hsla>,
    /// Cursor shape override from config (Block/Bar/Underline).
    /// Overrides the snapshot shape from the shell (except Hidden) — like Windows Terminal.
    pub(crate) cursor_shape_override: oneterm_settings::TerminalCursorShape,
    /// Per-line timestamps for gutter. `line_times[j]` ↔ line with absolute index
    /// `line_time_base + j`.
    pub(crate) line_times: Rc<VecDeque<String>>,
    /// Absolute index (0-based) of `line_times[0]`.
    pub(crate) line_time_base: usize,
    /// Render cache bundle — row layout, gutter width, grid size, metrics.
    pub(crate) render_cache: layout::types::TerminalRenderCache,
    /// Search highlights to paint (display coordinates, already filtered to the
    /// visible viewport).
    pub(crate) search_highlights: Vec<SearchHighlight>,
    /// Semantic overlay — produces per-cell Class for the visible viewport.
    pub(crate) overlay: SemanticOverlay,
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
        self.prepaint_terminal(bounds, window, cx)
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
        self.paint_terminal(bounds, layout, window, cx);
    }
}

impl IntoElement for TerminalElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}
