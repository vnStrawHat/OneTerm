//! The render state's contract: the tri-state, watermarks, resolved values and
//! the fairness hand-off.
//!
//! Named for the verification list in
//! `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md`.

use std::time::Instant;

use crate::cell::{Cell, CellContent, Color, NamedColor, Style};
use crate::grid::{Pos, PrintMode, RowId, ScrollRegion, Size, TerminalGrid};
use crate::intern::{Extras, GraphicId, Interner};
use crate::reflow::ResizePolicy;
use crate::render::{
    EngineView, ModeSnapshot, Palette, RenderContent, RenderState, RenderUpdate, SyncState,
};
use crate::selection::SelectionRange;

/// The fields `US-0076`'s `Terminal` will own, in the shape `EngineView` wants.
///
/// Shared with the sync tests and the bench, because every one of them needs a
/// grid, an interner and a clock that does not tick on its own.
pub(crate) struct Engine {
    pub grid: TerminalGrid,
    pub interner: Interner,
    pub sync: SyncState,
    pub modes: ModeSnapshot,
    pub generation: u32,
    pub palette_epoch: u32,
    pub selection: Option<SelectionRange>,
    pub now: Instant,
}

impl Engine {
    pub fn new(rows: u16, cols: u16) -> Engine {
        Engine {
            grid: TerminalGrid::new(Size { rows, cols }, 1_000),
            interner: Interner::default(),
            sync: SyncState::new(),
            modes: ModeSnapshot::default(),
            generation: 0,
            palette_epoch: 0,
            selection: None,
            now: Instant::now(),
        }
    }

    pub fn view(&mut self) -> EngineView<'_> {
        EngineView {
            grid: &self.grid,
            interner: &self.interner,
            sync: &mut self.sync,
            modes: self.modes,
            generation: self.generation,
            palette_epoch: self.palette_epoch,
            selection: self.selection,
            // No `Terminal`, so no graphics state: `graphics::tests` owns the
            // placement copy.
            placements: &[],
        }
    }

    pub fn update(&mut self, state: &mut RenderState) -> RenderUpdate {
        let now = self.now;
        self.update_at(state, now)
    }

    pub fn update_at(&mut self, state: &mut RenderState, now: Instant) -> RenderUpdate {
        state.begin_update(self.view(), now)
    }

    /// One `feed`: the batch stamp every row mutated below will carry.
    pub fn batch(&mut self) {
        self.grid.begin_batch();
    }

    pub fn write(&mut self, index: u16, text: &str) {
        self.write_with(index, text, PrintMode::default());
    }

    pub fn write_with(&mut self, index: u16, text: &str, mode: PrintMode) {
        self.grid.screen_mut().goto(index, 0);
        for glyph in text.chars() {
            self.grid.print(glyph, mode, &mut self.interner);
        }
    }

    pub fn row_id(&self, index: u16) -> RowId {
        self.grid.screen().row_of_index(index)
    }

    /// What the terminal does on `RIS`, a resize, a reflow and an
    /// alternate-screen swap: the one signal that invalidates every row.
    pub fn bump_generation(&mut self) {
        self.generation += 1;
    }
}

fn styled(engine: &mut Engine, fg: NamedColor) -> Cell {
    let id = engine.interner.style(&Style {
        fg: Color::Named(fg),
        ..Style::DEFAULT
    });
    Cell::EMPTY.with_style(id)
}

#[test]
fn first_update_is_full() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();

    assert_eq!(engine.update(&mut state), RenderUpdate::Full);
    assert_eq!(state.rows().len(), 10);
    assert_eq!(state.changed().len(), 10);
}

#[test]
fn idle_terminal_returns_unchanged() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);

    assert_eq!(engine.update(&mut state), RenderUpdate::Unchanged);
    assert!(state.changed().is_empty());
}

#[test]
fn unchanged_frame_copies_no_rows() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);
    let copied = state.rows_copied();

    for _ in 0..5 {
        assert_eq!(engine.update(&mut state), RenderUpdate::Unchanged);
    }

    assert_eq!(state.rows_copied(), copied, "an idle frame copied a row");
}

#[test]
fn rows_always_hold_the_full_viewport() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();

    assert_eq!(engine.update(&mut state), RenderUpdate::Full);
    assert_eq!(state.rows().len(), 10);

    assert_eq!(engine.update(&mut state), RenderUpdate::Unchanged);
    assert_eq!(state.rows().len(), 10);

    engine.batch();
    engine.write(4, "partial");
    assert!(matches!(
        engine.update(&mut state),
        RenderUpdate::Partial { .. }
    ));
    assert_eq!(state.rows().len(), 10);
}

#[test]
fn single_row_change_lists_one_changed_index() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);
    let copied = state.rows_copied();

    engine.batch();
    engine.write(3, "hello");

    assert_eq!(
        engine.update(&mut state),
        RenderUpdate::Partial { scrolled: 0 }
    );
    assert_eq!(state.changed(), &[3]);
    assert_eq!(state.rows_copied(), copied + 1);
}

#[test]
fn pure_scroll_reports_a_delta_and_copies_only_the_exposed_row() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();
    engine.batch();
    for index in 0..10 {
        engine.write(index, "row");
    }
    engine.update(&mut state);
    let copied = state.rows_copied();
    let second_row = state.rows()[1].id;

    engine.batch();
    engine.grid.screen_mut().goto(9, 0);
    engine.grid.linefeed();

    assert_eq!(
        engine.update(&mut state),
        RenderUpdate::Partial { scrolled: 1 }
    );
    // The rows that stayed on screen were moved, not rebuilt.
    assert_eq!(state.changed(), &[9]);
    assert_eq!(state.rows_copied(), copied + 1);
    assert_eq!(state.rows()[0].id, second_row);
}

#[test]
fn scroll_larger_than_the_viewport_returns_full() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);

    engine.batch();
    engine.grid.screen_mut().goto(9, 0);
    for _ in 0..15 {
        engine.grid.linefeed();
    }

    assert_eq!(engine.update(&mut state), RenderUpdate::Full);
}

#[test]
fn resize_and_alt_swap_and_ris_each_return_full() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);

    engine.batch();
    engine
        .grid
        .resize(Size { rows: 12, cols: 20 }, ResizePolicy::BottomAnchor);
    engine.bump_generation();
    assert_eq!(engine.update(&mut state), RenderUpdate::Full);
    assert_eq!(state.rows().len(), 12);

    engine.batch();
    engine.grid.swap_alt();
    engine.bump_generation();
    assert_eq!(engine.update(&mut state), RenderUpdate::Full);

    engine.batch();
    engine.grid.reset();
    engine.bump_generation();
    assert_eq!(engine.update(&mut state), RenderUpdate::Full);
}

#[test]
fn generation_mismatch_forces_full() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);
    assert_eq!(engine.update(&mut state), RenderUpdate::Unchanged);

    engine.bump_generation();

    assert_eq!(engine.update(&mut state), RenderUpdate::Full);
}

#[test]
fn invalidate_forces_full() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);

    state.invalidate();

    assert_eq!(engine.update(&mut state), RenderUpdate::Full);
    assert_eq!(engine.update(&mut state), RenderUpdate::Unchanged);
}

#[test]
fn cursor_only_movement_returns_partial_with_no_changed_rows() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);

    engine.batch();
    engine.grid.screen_mut().goto(4, 7);

    assert_eq!(
        engine.update(&mut state),
        RenderUpdate::Partial { scrolled: 0 }
    );
    assert!(state.changed().is_empty());
    assert_eq!(state.cursor().row, Some(4));
    assert_eq!(state.cursor().col, 7);
}

/// `US-0078`: a drag over static content changes no row, so only the selection
/// field can tell the renderer it must paint again.
#[test]
fn selection_change_only_returns_partial_and_refreshes_the_range() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);
    assert_eq!(state.selection(), None);

    let top = engine.grid.screen().screen_top();
    let range = SelectionRange {
        start: Pos { row: top, col: 2 },
        end: Pos { row: top, col: 5 },
        is_block: false,
    };
    engine.selection = Some(range);

    assert_eq!(
        engine.update(&mut state),
        RenderUpdate::Partial { scrolled: 0 }
    );
    assert!(state.changed().is_empty());
    assert_eq!(state.selection(), Some(range));

    // Unchanged again once the renderer has seen it.
    assert_eq!(engine.update(&mut state), RenderUpdate::Unchanged);
}

#[test]
fn mode_change_only_returns_partial_and_refreshes_the_snapshot() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);

    engine.modes.app_cursor = true;

    assert_eq!(
        engine.update(&mut state),
        RenderUpdate::Partial { scrolled: 0 }
    );
    assert!(state.changed().is_empty());
    assert!(state.modes().app_cursor);
}

#[test]
fn insert_mode_does_not_force_full_damage() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();
    engine.update(&mut state);

    engine.modes.insert = true;
    engine.batch();
    engine.write_with(
        2,
        "inserted",
        PrintMode {
            insert: true,
            autowrap: true,
        },
    );

    assert_eq!(
        engine.update(&mut state),
        RenderUpdate::Partial { scrolled: 0 }
    );
    assert_eq!(state.changed(), &[2]);
}

#[test]
fn changes_while_scrolled_back_are_not_copied() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();
    engine.batch();
    engine.grid.screen_mut().goto(9, 0);
    for _ in 0..20 {
        engine.grid.linefeed();
    }
    engine.update(&mut state);

    engine.batch();
    engine.grid.screen_mut().scroll_viewport(-5);
    engine.update(&mut state);

    // A write to the cursor row, which is now five rows below the viewport.
    engine.batch();
    engine.write(9, "offscreen");
    let update = engine.update(&mut state);

    assert!(matches!(update, RenderUpdate::Partial { scrolled: 0 }));
    assert!(
        state.changed().is_empty(),
        "an off-screen change was copied: {:?}",
        state.changed()
    );
}

#[test]
fn watermark_never_moves_backwards() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();
    let mut last = state.watermark();

    for index in 0..10u16 {
        engine.batch();
        engine.write(index, "line");
        engine.update(&mut state);
        assert!(state.watermark() >= last);
        last = state.watermark();
    }

    // An update with nothing to do never rewinds it either.
    engine.update(&mut state);
    assert!(state.watermark() >= last);
}

#[test]
fn two_consumers_keep_independent_watermarks() {
    let mut engine = Engine::new(10, 20);
    let mut view = RenderState::new();
    let mut search = RenderState::new();
    engine.update(&mut view);
    engine.update(&mut search);

    engine.batch();
    engine.write(6, "shared change");

    assert_eq!(
        engine.update(&mut view),
        RenderUpdate::Partial { scrolled: 0 }
    );
    assert_eq!(view.changed(), &[6]);
    // The first consumer cleared nothing: the second still sees the change.
    assert_eq!(
        engine.update(&mut search),
        RenderUpdate::Partial { scrolled: 0 }
    );
    assert_eq!(search.changed(), &[6]);
    // And neither sees it twice.
    assert_eq!(engine.update(&mut view), RenderUpdate::Unchanged);
    assert_eq!(engine.update(&mut search), RenderUpdate::Unchanged);
}

#[test]
fn style_runs_carry_resolved_values() {
    let mut engine = Engine::new(4, 8);
    let mut state = RenderState::new();
    let template = styled(&mut engine, NamedColor::Red);
    engine.batch();
    engine
        .grid
        .screen_mut()
        .set_template(template, &mut engine.interner);
    engine.write(1, "red");
    engine.update(&mut state);

    let row = &state.rows()[1];
    let first = row.style_of(&row.cells[0]);
    assert_eq!(first.fg, Color::Named(NamedColor::Red));
    assert_eq!(row.runs[0].cols, 0..3);
    assert_eq!(row.runs.len(), 2, "styled prefix plus the default tail");
}

#[test]
fn a_copied_row_survives_a_grapheme_sweep() {
    let mut engine = Engine::new(4, 8);
    let mut state = RenderState::new();
    let cluster = ['a', '\u{0301}'];
    let id = engine.interner.grapheme(&cluster);
    engine.batch();
    let row_id = engine.row_id(2);
    engine
        .grid
        .screen_mut()
        .row_mut(row_id)
        .set(0, Cell::EMPTY.with_content(CellContent::Grapheme(id)));
    engine.update(&mut state);

    // The arena renumbers underneath the copy; the copy is values, not ids.
    engine.interner.graphemes.sweep(std::iter::empty());

    let row = &state.rows()[2];
    let RenderContent::Cluster { start, len } = row.cells[0].content else {
        panic!("the grapheme cell was not copied as a cluster");
    };
    assert_eq!(row.cluster(start, len), &cluster);
}

#[test]
fn uniform_row_is_one_style_run() {
    let mut engine = Engine::new(50, 200);
    let mut state = RenderState::new();
    let template = styled(&mut engine, NamedColor::Green);
    engine.batch();
    engine
        .grid
        .screen_mut()
        .set_template(template, &mut engine.interner);
    let ids: Vec<RowId> = (0..50).map(|index| engine.row_id(index)).collect();
    for id in ids {
        engine
            .grid
            .screen_mut()
            .row_mut(id)
            .fill(0..200, template.with_content(CellContent::Scalar(' ')));
    }
    engine.update(&mut state);

    assert_eq!(state.rows().len(), 50);
    for row in state.rows() {
        assert_eq!(row.runs.len(), 1, "row {:?} was not one run", row.id);
        assert_eq!(row.runs[0].cols, 0..200);
    }
}

#[test]
fn hyperlink_strings_are_resolved_under_the_lock() {
    let mut engine = Engine::new(4, 8);
    let mut state = RenderState::new();
    let link = engine
        .interner
        .hyperlinks
        .intern(Some("1"), "https://example.invalid/a")
        .expect("the table is empty");
    let extras = engine.interner.extras(&Extras {
        hyperlink: Some(link),
        graphic: None,
    });
    engine.batch();
    let row_id = engine.row_id(0);
    engine
        .grid
        .screen_mut()
        .row_mut(row_id)
        .set(0, Cell::EMPTY.with_extras(extras));
    engine.update(&mut state);

    assert_eq!(state.rows()[0].cells[0].hyperlink, Some(link));
    let resolved = state.hyperlink(link).expect("the link was not resolved");
    assert_eq!(&*resolved.uri, "https://example.invalid/a");
    assert_eq!(&*resolved.id, "1");
}

#[test]
fn two_render_states_both_see_the_graphic() {
    let mut engine = Engine::new(4, 8);
    let graphic = GraphicId(7);
    let extras = engine.interner.extras(&Extras {
        hyperlink: None,
        graphic: Some(graphic),
    });
    engine.batch();
    let row_id = engine.row_id(1);
    engine
        .grid
        .screen_mut()
        .row_mut(row_id)
        .set(3, Cell::EMPTY.with_extras(extras));

    let mut view = RenderState::new();
    let mut second = RenderState::new();
    engine.update(&mut view);
    engine.update(&mut second);

    assert_eq!(view.rows()[1].cells[3].graphic, Some(graphic));
    assert_eq!(second.rows()[1].cells[3].graphic, Some(graphic));
}

#[test]
fn map_colors_resolves_named_colours_and_is_idempotent() {
    let mut engine = Engine::new(4, 8);
    let mut state = RenderState::new();
    engine.update(&mut state);
    let palette = Palette::new();

    state.map_colors(&palette);
    let mapped = state.rows()[0].runs[0].style;
    assert_eq!(mapped.bg, Color::Rgb(palette.background));
    assert_eq!(mapped.fg, Color::Rgb(palette.foreground));

    state.map_colors(&palette);
    assert_eq!(state.rows()[0].runs[0].style, mapped);
}

#[test]
fn size_reports_the_viewport() {
    let mut engine = Engine::new(10, 20);
    let mut state = RenderState::new();
    assert_eq!(state.size(), Size { rows: 0, cols: 0 });

    engine.update(&mut state);
    assert_eq!(state.size(), Size { rows: 10, cols: 20 });

    engine.batch();
    engine
        .grid
        .resize(Size { rows: 12, cols: 40 }, ResizePolicy::BottomAnchor);
    engine.bump_generation();
    engine.update(&mut state);
    assert_eq!(state.size(), Size { rows: 12, cols: 40 });
}

#[test]
fn dim_colours_match_oneterms_palette() {
    // `crates/terminal/src/palette.rs`: a dim colour is a 50 % mix with the
    // background, and the reference rounds. Pinning the arithmetic here is what
    // keeps the seam at `US-0081` from visibly recolouring every dim cell.
    let mut palette = Palette::new();
    palette.background = crate::cell::Rgb { r: 0, g: 0, b: 0 };
    palette.indexed[1] = crate::cell::Rgb { r: 205, g: 0, b: 0 };

    assert_eq!(
        palette.resolve(Color::Named(NamedColor::DimRed)),
        crate::cell::Rgb { r: 103, g: 0, b: 0 }
    );

    palette.background = crate::cell::Rgb {
        r: 40,
        g: 40,
        b: 60,
    };
    assert_eq!(
        palette.resolve(Color::Named(NamedColor::DimRed)),
        crate::cell::Rgb {
            r: 123,
            g: 20,
            b: 30
        }
    );
    assert_eq!(
        palette.resolve(Color::Named(NamedColor::DimForeground)),
        palette.dim(palette.foreground)
    );
}

#[test]
fn a_palette_epoch_change_rebuilds_and_remaps_every_row() {
    let mut engine = Engine::new(4, 8);
    let mut state = RenderState::new();
    engine.update(&mut state);
    state.map_colors(&Palette::new());

    let mut themed = Palette::new();
    themed.background = crate::cell::Rgb { r: 1, g: 2, b: 3 };
    engine.palette_epoch += 1;

    assert_eq!(engine.update(&mut state), RenderUpdate::Full);
    state.map_colors(&themed);
    for row in state.rows() {
        assert_eq!(row.runs[0].style.bg, Color::Rgb(themed.background));
    }
}

#[test]
fn steady_state_makes_no_allocation() {
    let mut engine = Engine::new(50, 200);
    let mut state = RenderState::new();
    let palette = Palette::new();

    let mut capacities = Vec::new();
    for cycle in 0..600 {
        engine.batch();
        engine.write(u16::try_from(cycle % 50).unwrap_or(0), "steady state line");
        engine.update(&mut state);
        state.map_colors(&palette);

        // Warm up first: the high-water mark is reached within a few frames.
        if cycle == 60 {
            capacities = row_capacities(&state);
        }
    }

    assert_eq!(
        capacities,
        row_capacities(&state),
        "a steady-state frame grew a buffer"
    );
}

fn row_capacities(state: &RenderState) -> Vec<(usize, usize, usize)> {
    state
        .rows()
        .iter()
        .map(|row| {
            (
                row.cells.capacity(),
                row.runs.capacity(),
                row.clusters.capacity(),
            )
        })
        .collect()
}

#[test]
fn scroll_damage_is_consumed_as_a_move_instruction() {
    let mut engine = Engine::new(8, 20);
    let mut state = RenderState::new();
    engine.batch();
    for index in 0..8 {
        engine.write(index, "content");
    }
    engine.update(&mut state);
    let ids: Vec<RowId> = state.rows().iter().map(|row| row.id).collect();

    engine.batch();
    engine.grid.screen_mut().goto(7, 0);
    engine.grid.linefeed();
    engine.grid.screen_mut().goto(7, 0);
    engine.grid.linefeed();
    let update = engine.update(&mut state);

    assert_eq!(update, RenderUpdate::Partial { scrolled: 2 });
    // A consumer applying `scrolled` to its own cache lands where we did.
    for (index, id) in ids.iter().skip(2).enumerate() {
        assert_eq!(state.rows()[index].id, *id);
    }
    assert_eq!(state.changed(), &[6, 7]);
}

/// The `US-0075` rework at the render seam: write row 3, consume it, blank it
/// back to what used to be an unwritten slot, consume again — the row must come
/// back as changed, **from the sequence number alone**.
///
/// The render state used to carry a private `allocated` flag for this, which
/// made `DEC-0015`'s "a second consumer becomes possible without an engine
/// change" false. The plain watermark reader at the end is that second
/// consumer, holding nothing but a `SeqNo` and the public row API.
#[test]
fn a_blanked_row_reaches_a_consumer_holding_only_a_watermark() {
    let mut engine = Engine::new(6, 20);
    let mut state = RenderState::new();

    engine.batch();
    engine.write(3, "row3");
    assert_eq!(engine.update(&mut state), RenderUpdate::Full);
    assert_eq!(state.rows()[3].cells[0].content, RenderContent::Scalar('r'));
    // What a second consumer would have taken away from the same frame.
    let watermark = engine.grid.seq();
    let id = engine.row_id(3);

    // Row 4 was never written; the in-region scroll pulls it over row 3.
    engine.batch();
    engine.grid.scroll_up(ScrollRegion { top: 1, bottom: 5 }, 1);

    assert_eq!(
        engine.update(&mut state),
        RenderUpdate::Partial { scrolled: 0 }
    );
    assert!(
        state.changed().contains(&3),
        "the blanked row was not reported as changed"
    );
    assert!(
        state.rows()[3]
            .cells
            .iter()
            .all(|cell| cell.content == RenderContent::Scalar(' ')),
        "the consumer kept painting the old content"
    );
    assert!(
        engine.grid.screen().row(id).seq() > watermark,
        "a watermark consumer would have missed the blanking"
    );
}

#[test]
fn an_in_region_scroll_moves_content_between_row_ids() {
    let mut engine = Engine::new(8, 20);
    let mut state = RenderState::new();
    engine.batch();
    for index in 0..8 {
        engine.write(index, "content");
    }
    engine.update(&mut state);

    // A bounded region: the rows inside it change content without the viewport
    // moving, which is exactly what `VtEvent::RowsScrolled` reports.
    engine.batch();
    let report = engine.grid.scroll_up(ScrollRegion { top: 1, bottom: 4 }, 1);
    let update = engine.update(&mut state);

    assert!(report.scrolled.is_some());
    assert_eq!(update, RenderUpdate::Partial { scrolled: 0 });
    // Row 3 is the one the scroll blanked. The grid stamps a blanked row
    // (`US-0075`'s rework), so it is copied for the same reason as the other
    // two: its sequence number is above the watermark.
    assert_eq!(state.changed(), &[1, 2, 3]);
    assert!(
        state.rows()[3]
            .cells
            .iter()
            .all(|cell| cell.content == RenderContent::Scalar(' ')),
        "the blanked row was painted with stale content"
    );
}
