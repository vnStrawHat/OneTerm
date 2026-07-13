use gpui::{AppContext as _, TestAppContext, VisualTestContext};
use oneterm_core::terminal::test_support::FakeTerminalSession;

use super::LocalTerminalView;

#[gpui::test]
fn phase0_renderer_baseline_counts_dirty_and_idle_frames(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);

    let (session, probe) = FakeTerminalSession::boxed(
        24,
        80,
        "OneTerm Phase 0 renderer baseline\nhttps://example.com/diagnostics",
    );
    let (view, cx) = cx.add_window_view(move |window, cx| {
        let session = cx.new(|_| session);
        LocalTerminalView::new(session, window, cx)
    });
    let cx: &mut VisualTestContext = cx;

    // Let async tasks (event subscriber, blink) settle, then draw once to warm
    // the cache.
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let warm = view.read_with(cx, |view, _| view.render_diagnostics());
    assert!(warm.frame_count >= 1);
    assert!(warm.total_lines > 0);
    assert!(warm.paint_quad_calls > 0);
    assert!(warm.allocation_buffer_sites > 0);

    // ── Dirty frame: change the session text and immediately redraw ──
    // Don't run_until_parked between set_text and draw — the blink task could
    // fire and consume the damage before our explicit draw.
    probe.set_text("Changed content forces a dirty render\nnew line here");
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let dirty = view.read_with(cx, |view, _| view.render_diagnostics());
    assert!(dirty.frame_count > warm.frame_count);
    assert!(probe.snapshot_calls() > 0);
    assert!(
        dirty.row_layout_calls > 0,
        "dirty frame should re-layout changed rows: {:?}",
        dirty
    );
    assert!(
        dirty.shape_line_calls > 0,
        "dirty frame should shape new text: {:?}",
        dirty
    );
    assert!(dirty.paint_quad_calls > 0);
    assert!(dirty.allocation_buffer_sites > 0);

    // ── Idle frame: no content change, rows should be cached ──
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let idle = view.read_with(cx, |view, _| view.render_diagnostics());
    assert!(idle.frame_count > dirty.frame_count);
    assert_eq!(idle.total_lines, dirty.total_lines);
    assert_eq!(
        idle.dirty_lines, 0,
        "idle frame should have zero dirty rows"
    );
    assert_eq!(idle.row_layout_calls, 0, "idle frame should not re-layout");
    assert_eq!(idle.shape_line_calls, 0, "idle frame should not re-shape");
    assert!(idle.paint_quad_calls > 0, "idle frame still paints quads");
    assert!(idle.allocation_buffer_sites > 0);

    eprintln!("phase0_renderer_dirty={dirty:?}");
    eprintln!("phase0_renderer_idle={idle:?}");
}
