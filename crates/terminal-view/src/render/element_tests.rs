//! End-to-end tests of `TerminalElement` on a headless window driven by
//! `FakeTerminalSession`, plus the module-boundary check for `render/`.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, Font, FontFeatures, FontStyle, FontWeight, IntoElement,
    Render, TestAppContext, VisualTestContext, Window, px, size,
};
use oneterm_terminal::TerminalSession;
use oneterm_terminal::test_support::{FakeSessionProbe, FakeTerminalSession};

use super::{CellAnchor, TerminalElement, TerminalElementSpec};
use crate::render::diagnostics::FrameStats;
use crate::render::frame::GridSize;
use crate::render::state::{RenderInputs, RenderState};
use crate::theme::build_terminal_theme;

/// Run text `=>e\u{301}` over three cells: the `=>` ligature is one glyph for
/// bytes 0..2 (cells 0 and 1), `e` starts cell 2 at byte 2, the mark (byte 3)
/// shares its cell. GPUI's `force_width` pass put `e` at 8 px (glyph number 1)
/// instead of 16 px.
#[test]
fn glyphs_are_anchored_at_their_cell() {
    let cells = [0u32, 1, 2];
    let w = px(8.0);
    let mut anchor = CellAnchor::default();
    assert_eq!(anchor.x(&cells, w, 0, px(0.0)), px(0.0));
    assert_eq!(
        anchor.x(&cells, w, 2, px(8.0)),
        px(16.0),
        "`e` moves to cell 2"
    );
    assert_eq!(
        anchor.x(&cells, w, 3, px(10.0)),
        px(18.0),
        "the mark keeps its 2 px offset from `e`"
    );
    // No cell map (gutter labels): the shaped position is used as-is.
    assert_eq!(CellAnchor::default().x(&[], w, 5, px(40.0)), px(40.0));
}

/// Counts heap allocations on the current thread between `start` and `stop`.
/// The counters are `const`-initialised, `Drop`-free thread locals, so reading
/// them inside the allocator never allocates or registers a destructor.
mod alloc_counter {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static ENABLED: Cell<bool> = const { Cell::new(false) };
        static COUNT: Cell<usize> = const { Cell::new(0) };
    }

    struct Counting;

    fn record() {
        if ENABLED.with(|e| e.get()) {
            COUNT.with(|c| c.set(c.get() + 1));
        }
    }

    // SAFETY: every method forwards to `System` with the same arguments, so
    // the allocator contract (layout/pointer pairing) is exactly `System`'s;
    // the counters are plain `Cell`s that cannot allocate or re-enter.
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record();
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            record();
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            record();
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static GLOBAL: Counting = Counting;

    pub(super) fn start() {
        COUNT.with(|c| c.set(0));
        ENABLED.with(|e| e.set(true));
    }

    pub(super) fn stop() -> usize {
        ENABLED.with(|e| e.set(false));
        COUNT.with(|c| c.get())
    }
}

struct Host {
    session: Entity<Box<dyn TerminalSession>>,
    state: Rc<RefCell<RenderState>>,
}

impl Render for Host {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        TerminalElement::new(TerminalElementSpec {
            id: "terminal".into(),
            session: self.session.clone(),
            state: self.state.clone(),
            ime: None,
        })
    }
}

fn font() -> Font {
    Font {
        family: "Test Mono".into(),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    }
}

fn inputs() -> RenderInputs {
    let theme = Rc::new(build_terminal_theme(&gpui_component::Theme::default()));
    RenderInputs::new(theme, font(), px(13.0))
}

/// A focused terminal in its blink-off phase paints no cursor, which keeps
/// the layer count at one for the grid assertions.
fn inputs_without_cursor() -> RenderInputs {
    let mut inputs = inputs();
    inputs.cursor.focused = true;
    inputs.cursor.blink_visible = false;
    inputs
}

struct Harness<'a> {
    host: Entity<Host>,
    probe: FakeSessionProbe,
    state: Rc<RefCell<RenderState>>,
    cx: &'a mut VisualTestContext,
}

impl<'a> Harness<'a> {
    fn open(
        cx: &'a mut TestAppContext,
        rows: usize,
        cols: usize,
        text: &str,
        inputs: RenderInputs,
    ) -> Harness<'a> {
        let (session, probe) = FakeTerminalSession::boxed(rows, cols, text);
        let state = Rc::new(RefCell::new(RenderState::new(inputs)));
        let shared = state.clone();
        let (host, cx) = cx.add_window_view(move |_, cx| Host {
            session: cx.new(|_| session),
            state: shared,
        });
        Harness {
            host,
            probe,
            state,
            cx,
        }
    }

    /// Stats of the first frame: opening the window already draws once; a
    /// platform that does not is drawn explicitly.
    fn first_frame(&mut self) -> FrameStats {
        if self.probe.snapshot_calls() == 0 {
            return self.draw();
        }
        self.state.borrow().stats
    }

    fn draw(&mut self) -> FrameStats {
        self.cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        self.state.borrow().stats
    }

    /// Resize the window and return the stats of the frame that follows.
    fn resize(&mut self, width: f32, height: f32) -> FrameStats {
        let before = self.probe.snapshot_calls();
        self.cx.simulate_resize(size(px(width), px(height)));
        if self.probe.snapshot_calls() == before {
            return self.draw();
        }
        self.state.borrow().stats
    }

    fn session_state(&mut self) -> oneterm_terminal::TerminalQueryState {
        self.host
            .read_with(self.cx, |host, cx| host.session.read(cx).query_state())
    }

    fn grid(&self) -> GridSize {
        self.state
            .borrow()
            .geometry
            .map(|g| g.size)
            .unwrap_or(GridSize { rows: 0, cols: 0 })
    }
}

#[gpui::test]
fn dirty_frame_plans_rows_and_shapes(cx: &mut TestAppContext) {
    let mut h = Harness::open(
        cx,
        24,
        80,
        "hello world\n████████████\n────────────\nhttps://example.com/x",
        inputs_without_cursor(),
    );
    let first = h.first_frame();
    assert_eq!(first.snapshot_calls, 1, "{first:?}");
    assert!(first.rows_planned > 0, "{first:?}");
    assert!(first.rows_planned <= first.rows_candidate, "{first:?}");
    assert!(first.shape_calls > 0, "{first:?}");
    assert!(first.glyphs > 0, "{first:?}");
    assert!(first.quads > 0, "{first:?}");
    assert_eq!(first.layers, 1, "{first:?}");
    assert_eq!(first.url_scans, 1, "{first:?}");
    assert_eq!(first.glyph_errors, 0, "{first:?}");

    // Only the rows whose content changed are rebuilt.
    h.probe.set_text("hello there");
    let dirty = h.draw();
    assert_eq!(dirty.snapshot_calls, 1, "{dirty:?}");
    assert!(dirty.rows_planned >= 1, "{dirty:?}");
    assert!(dirty.rows_planned < dirty.rows_total, "{dirty:?}");
    assert!(dirty.rows_planned <= dirty.rows_candidate, "{dirty:?}");
    eprintln!("dirty_frame_plans_rows_and_shapes first={first:?} dirty={dirty:?}");
}

#[gpui::test]
fn idle_frame_plans_nothing_but_paints(cx: &mut TestAppContext) {
    let mut h = Harness::open(
        cx,
        24,
        80,
        "idle ████ text\n──────",
        inputs_without_cursor(),
    );
    h.first_frame();
    let idle = h.draw();
    assert_eq!(idle.snapshot_calls, 1, "{idle:?}");
    assert_eq!(idle.rows_planned, 0, "{idle:?}");
    assert_eq!(idle.shape_calls, 0, "{idle:?}");
    assert_eq!(idle.url_scans, 0, "{idle:?}");
    assert!(idle.quads > 0, "{idle:?}");
    assert!(idle.glyphs > 0, "{idle:?}");
    assert_eq!(idle.layers, 1, "{idle:?}");
    eprintln!("idle_frame_plans_nothing_but_paints idle={idle:?}");
}

#[gpui::test]
fn idle_frame_allocates_nothing(cx: &mut TestAppContext) {
    let mut h = Harness::open(
        cx,
        24,
        80,
        "steady state\n████ ──── ╭──╮\nhttps://example.com/path",
        inputs(),
    );
    h.first_frame();
    h.draw();
    let session = h.host.read_with(h.cx, |host, _| host.session.clone());
    let state = h.state.clone();
    h.cx.update(|window, cx| {
        let state = &mut *state.borrow_mut();
        // The fake's `snapshot_into` allocates a fresh content; that is the
        // engine's side, so it stays outside the measured region.
        state.frame.snapshot(&**session.read(cx));
        let metrics = state.metrics(window, cx);
        let geometry = state.geometry.expect("drawn");
        state.glyphs.begin_frame();
        state.stats = FrameStats::default();
        // The counter must see a deliberate allocation before it is trusted
        // to report zero.
        alloc_counter::start();
        let probe = vec![0u8; 64];
        assert!(alloc_counter::stop() > 0, "allocation counter is inert");
        drop(probe);
        alloc_counter::start();
        state.update_plans(&metrics, window);
        state.compute_overlays();
        let cursor = state.resolve_cursor(&geometry, window);
        let allocations = alloc_counter::stop();
        assert!(
            cursor.is_some(),
            "unfocused terminals paint a hollow cursor"
        );
        assert_eq!(state.stats.rows_planned, 0, "{:?}", state.stats);
        assert_eq!(
            allocations, 0,
            "idle plan update must not allocate ({:?})",
            state.stats
        );
    });
}

#[gpui::test]
fn glyph_cache_hits_across_rows(cx: &mut TestAppContext) {
    let mut h = Harness::open(
        cx,
        24,
        80,
        "same text\nsame text\nsame text",
        inputs_without_cursor(),
    );
    let first = h.first_frame();
    assert_eq!(
        first.shape_calls, 2,
        "'same' and 'text' shape once: {first:?}"
    );
    assert!(first.glyph_hits >= 4, "{first:?}");
    let idle = h.draw();
    assert_eq!(idle.shape_calls, 0, "{idle:?}");
    assert!(h.state.borrow().glyphs.len() >= 2);
}

#[gpui::test]
fn resize_replans_all_and_resizes_session(cx: &mut TestAppContext) {
    let mut h = Harness::open(cx, 24, 80, "resize me", inputs_without_cursor());
    let first = h.first_frame();
    let grid = h.grid();
    assert_ne!(
        grid,
        GridSize { rows: 24, cols: 80 },
        "window differs from the fake's size"
    );
    let state = h.session_state();
    assert_eq!(
        (state.rows, state.cols),
        (usize::from(grid.rows), usize::from(grid.cols))
    );
    assert_eq!(first.rows_total, u32::from(grid.rows));

    let resized = h.resize(400.0, 300.0);
    let smaller = h.grid();
    assert!(
        smaller.rows < grid.rows && smaller.cols < grid.cols,
        "{smaller:?}"
    );
    assert_eq!(resized.rows_planned, u32::from(smaller.rows), "{resized:?}");
    let state = h.session_state();
    assert_eq!(
        (state.rows, state.cols),
        (usize::from(smaller.rows), usize::from(smaller.cols))
    );
}

#[gpui::test]
fn cursor_layer_and_gutter_paint(cx: &mut TestAppContext) {
    let mut inputs = inputs();
    inputs.cursor.focused = true;
    inputs.cursor.blink_visible = true;
    inputs.show_gutter = true;
    inputs.gutter.absolute_line_count = 3;
    inputs.gutter.times = Rc::new([36_000u32, 36_001, 36_002].into_iter().collect());
    let mut h = Harness::open(cx, 24, 80, "x", inputs);
    h.probe.set_cursor(0, 0);
    h.first_frame();
    let s = h.draw();
    assert_eq!(s.layers, 2, "grid + cursor: {s:?}");
    let state = h.state.borrow();
    let geometry = state.geometry.expect("drawn");
    assert!(geometry.gutter_width > px(0.0));
    assert_eq!(state.gutter.labels.len(), usize::from(geometry.size.rows));
    assert!(geometry.origin.x > geometry.bounds.origin.x + geometry.gutter_width - px(1.0));
}

#[test]
fn render_has_single_alacritty_file() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/render");
    let needle = concat!("alacritty", "_terminal");
    let mut offenders = Vec::new();
    let entries = std::fs::read_dir(&dir).expect("render dir");
    for entry in entries {
        let path = entry.expect("dir entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if !name.ends_with(".rs") || name == "frame.rs" {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("read source");
        if source.contains(needle) {
            offenders.push(name.to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "only frame.rs may name the engine crate: {offenders:?}"
    );
}

/// A 2 x 2 cell Sixel image: uploaded once, painted once per frame from its
/// first visible cell, gone when its cells lose their references.
#[gpui::test]
fn sixel_image_paints_once_per_frame(cx: &mut TestAppContext) {
    use oneterm_terminal::{GraphicCell, GraphicData, GraphicId};
    let mut h = Harness::open(cx, 6, 12, "", inputs_without_cursor());
    let _ = h.first_frame();
    let id = GraphicId(7);
    h.probe.push_graphic(std::sync::Arc::new(GraphicData {
        id,
        width: 16,
        height: 16,
        rgba: vec![255; 16 * 16 * 4],
    }));
    for (line, col, row, column) in [(1, 2, 0, 0), (1, 3, 0, 1), (2, 2, 1, 0), (2, 3, 1, 1)] {
        h.probe.set_graphic(
            line,
            col,
            GraphicCell {
                id,
                col: column,
                row,
            },
        );
    }
    let stats = h.draw();
    assert_eq!((stats.images_uploaded, stats.images), (1, 1), "{stats:?}");
    let stats = h.draw();
    assert_eq!(
        (stats.images_uploaded, stats.images),
        (0, 1),
        "known id is not re-uploaded: {stats:?}"
    );
    h.probe.clear_graphics();
    let stats = h.draw();
    assert_eq!(stats.images, 0, "{stats:?}");
}
