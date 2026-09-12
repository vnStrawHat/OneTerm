use proptest::prelude::*;

use super::*;
use crate::intern::{Extras, GraphicId};

fn wide(c: char) -> Cell {
    Cell::EMPTY
        .with_content(CellContent::Scalar(c))
        .with_width(CellWidth::Wide)
}

fn spacer() -> Cell {
    Cell::EMPTY.with_width(CellWidth::WideSpacer)
}

#[test]
fn cell_is_eight_bytes() {
    assert_eq!(size_of::<Cell>(), 8);
    assert_eq!(align_of::<Cell>(), align_of::<u64>());
}

#[test]
fn zero_cell_is_a_blank_space_with_the_default_style() {
    let cell = Cell::default();
    assert_eq!(cell, Cell::EMPTY);
    assert_eq!(cell.to_bits(), 0);
    assert_eq!(cell.content(), CellContent::Scalar(' '));
    assert_eq!(cell.width(), CellWidth::Narrow);
    assert_eq!(cell.semantic(), Semantic::None);
    assert!(!cell.protected());
    assert_eq!(cell.style_id(), StyleId::DEFAULT);
    assert_eq!(cell.extras_id(), ExtrasId::NONE);
    assert!(cell.is_blank());
    assert!(cell.is_erasable(&Interner::default()));
}

#[test]
fn content_roundtrips_for_max_scalar_and_max_grapheme_id() {
    let max_scalar = Cell::EMPTY.with_content(CellContent::Scalar('\u{10FFFF}'));
    assert_eq!(max_scalar.content(), CellContent::Scalar('\u{10FFFF}'));

    let max_id = GraphemeId(CONTENT_LIMIT - 1);
    let grapheme = Cell::EMPTY.with_content(CellContent::Grapheme(max_id));
    assert_eq!(grapheme.content(), CellContent::Grapheme(max_id));

    // The two spaces are disjoint: the same bit pattern means different things.
    let scalar = Cell::EMPTY.with_content(CellContent::Scalar('A'));
    let id = Cell::EMPTY.with_content(CellContent::Grapheme(GraphemeId('A' as u32)));
    assert_ne!(scalar.content(), id.content());
}

#[test]
fn every_packed_field_is_independent() {
    let cell = Cell::EMPTY
        .with_content(CellContent::Scalar('Z'))
        .with_width(CellWidth::LeadingWideSpacer)
        .with_semantic(Semantic::Input)
        .with_protected(true)
        .with_style(StyleId(0xBEEF))
        .with_extras(ExtrasId(0xC0DE));

    assert_eq!(cell.content(), CellContent::Scalar('Z'));
    assert_eq!(cell.width(), CellWidth::LeadingWideSpacer);
    assert_eq!(cell.semantic(), Semantic::Input);
    assert!(cell.protected());
    assert_eq!(cell.style_id(), StyleId(0xBEEF));
    assert_eq!(cell.extras_id(), ExtrasId(0xC0DE));

    // Rewriting one field leaves the rest alone.
    let rewritten = cell.with_content(CellContent::Grapheme(GraphemeId(7)));
    assert_eq!(rewritten.content(), CellContent::Grapheme(GraphemeId(7)));
    assert_eq!(rewritten.width(), CellWidth::LeadingWideSpacer);
    assert_eq!(rewritten.semantic(), Semantic::Input);
    assert!(rewritten.protected());
    assert_eq!(rewritten.style_id(), StyleId(0xBEEF));
    assert_eq!(rewritten.extras_id(), ExtrasId(0xC0DE));
}

#[test]
fn the_bit_layout_is_the_documented_one() {
    assert_eq!(
        Cell::EMPTY
            .with_content(CellContent::Scalar('\u{1}'))
            .to_bits(),
        1
    );
    assert_eq!(
        Cell::EMPTY
            .with_content(CellContent::Grapheme(GraphemeId(0)))
            .to_bits(),
        1 << 21
    );
    assert_eq!(Cell::EMPTY.with_width(CellWidth::Wide).to_bits(), 1 << 22);
    assert_eq!(
        Cell::EMPTY.with_semantic(Semantic::Prompt).to_bits(),
        1 << 24
    );
    assert_eq!(Cell::EMPTY.with_protected(true).to_bits(), 1 << 26);
    assert_eq!(Cell::EMPTY.with_style(StyleId(1)).to_bits(), 1 << 32);
    assert_eq!(Cell::EMPTY.with_extras(ExtrasId(1)).to_bits(), 1 << 48);
    // Bits 27..32 are reserved and nothing this packet writes may touch them.
    let full = Cell::EMPTY
        .with_content(CellContent::Scalar('\u{10FFFF}'))
        .with_width(CellWidth::LeadingWideSpacer)
        .with_semantic(Semantic::Output)
        .with_protected(true)
        .with_style(StyleId(u16::MAX))
        .with_extras(ExtrasId(u16::MAX));
    assert_eq!(full.to_bits() & (0b11111 << 27), 0);
}

#[test]
fn width_enum_covers_every_wide_pair_shape() {
    for width in [
        CellWidth::Narrow,
        CellWidth::Wide,
        CellWidth::WideSpacer,
        CellWidth::LeadingWideSpacer,
    ] {
        assert_eq!(Cell::EMPTY.with_width(width).width(), width);
    }
    assert!(!CellWidth::Narrow.is_spacer());
    assert!(!CellWidth::Wide.is_spacer());
    assert!(CellWidth::WideSpacer.is_spacer());
    assert!(CellWidth::LeadingWideSpacer.is_spacer());
    assert_eq!(CellWidth::default(), CellWidth::Narrow);
}

#[test]
fn wide_pair_repair_on_overwrite() {
    // Overwriting the glyph clears the spacer to its right.
    let mut row = vec![Cell::EMPTY, wide('漢'), spacer(), Cell::EMPTY];
    repair_wide_pair_in_row(&mut row, 1);
    assert_eq!(row[2].width(), CellWidth::Narrow);

    // Overwriting the spacer clears the glyph to its left, which loses the
    // glyph but keeps the cell.
    let mut row = vec![Cell::EMPTY, wide('漢'), spacer(), Cell::EMPTY];
    repair_wide_pair_in_row(&mut row, 2);
    assert_eq!(row[1].width(), CellWidth::Narrow);
    assert_eq!(row[1].content(), CellContent::Scalar(' '));

    // A glyph in the last column has no spacer of its own, so the repair falls
    // back to the cell on its left, exactly as the reference does.
    let mut row = vec![Cell::EMPTY, wide('漢')];
    repair_wide_pair_in_row(&mut row, 1);
    assert_eq!(row[0].content(), CellContent::Scalar(' '));

    // A narrow cell is never repaired, and neither is an out-of-range column.
    let mut row = vec![
        Cell::EMPTY.with_content(CellContent::Scalar('a')),
        wide('漢'),
    ];
    let before = row.clone();
    repair_wide_pair_in_row(&mut row, 0);
    repair_wide_pair_in_row(&mut row, 9);
    assert_eq!(row, before);
}

#[test]
fn blank_and_erasable_predicates_differ_on_a_bold_space() {
    let mut interner = Interner::default();
    let bold = interner.style(&Style {
        attrs: Attrs::BOLD,
        ..Style::DEFAULT
    });
    let bold_space = Cell::EMPTY.with_style(bold);

    // Trap 37: the reference's erase rule ignores BOLD, so a bold space is
    // erasable; the renderer must not take a blank fast path over it.
    assert!(bold_space.is_erasable(&interner));
    assert!(!bold_space.is_blank());

    // INVERSE and any underline are the attributes that rule does look at.
    for attrs in [
        Attrs::INVERSE,
        Attrs::UNDERLINE,
        Attrs::UNDERCURL,
        Attrs::STRIKEOUT,
    ] {
        let id = interner.style(&Style {
            attrs,
            ..Style::DEFAULT
        });
        assert!(
            !Cell::EMPTY.with_style(id).is_erasable(&interner),
            "{attrs:?}"
        );
    }

    // A non-default background is not erasable either (BCE keeps the colour).
    let painted = interner.style(&Style {
        bg: Color::Palette(4),
        ..Style::DEFAULT
    });
    assert!(!Cell::EMPTY.with_style(painted).is_erasable(&interner));

    // A wide spacer is never erasable, whatever its style.
    assert!(!spacer().is_erasable(&interner));
}

#[test]
fn tab_cell_is_erasable_but_not_blank_and_reads_back_as_tab() {
    let interner = Interner::default();
    let tab = Cell::EMPTY.with_content(CellContent::Scalar('\t'));
    assert!(tab.is_erasable(&interner));
    assert!(!tab.is_blank());
    assert_eq!(tab.text_char(&interner.graphemes), '\t');
}

#[test]
fn graphic_cell_is_not_erasable() {
    let mut interner = Interner::default();
    let extras = interner.extras(&Extras {
        graphic: Some(GraphicId(1)),
        ..Extras::NONE
    });
    let covered = Cell::EMPTY.with_extras(extras);
    assert!(!covered.is_erasable(&interner));
    assert!(!covered.is_blank());

    // A hyperlink alone does not stop an erase (trap 37 ignores it).
    let linked = interner.extras(&Extras {
        hyperlink: Some(crate::intern::HyperlinkId(0)),
        ..Extras::NONE
    });
    assert!(Cell::EMPTY.with_extras(linked).is_erasable(&interner));
}

#[test]
fn text_char_reads_the_base_scalar_of_a_grapheme_cell() {
    let mut interner = Interner::default();
    let id = interner.grapheme(&['e', '\u{301}']);
    let cell = Cell::EMPTY.with_content(CellContent::Grapheme(id));
    assert_eq!(cell.text_char(&interner.graphemes), 'e');
    assert_eq!(interner.resolve_grapheme(id), &['e', '\u{301}'][..]);
    // A grapheme cell is neither blank nor erasable.
    assert!(!cell.is_blank());
    assert!(!cell.is_erasable(&interner));
}

#[test]
fn the_default_style_is_the_terminal_foreground_on_the_terminal_background() {
    assert_eq!(Style::default(), Style::DEFAULT);
    assert_eq!(Style::DEFAULT.fg, Color::Named(NamedColor::Foreground));
    assert_eq!(Style::DEFAULT.bg, Color::Named(NamedColor::Background));
    assert_eq!(Style::DEFAULT.underline_color, None);
    assert!(Style::DEFAULT.attrs.is_empty());
    assert!(Attrs::ALL_UNDERLINES.contains(Attrs::UNDERCURL));
    assert!(!Attrs::ALL_UNDERLINES.contains(Attrs::OVERLINE));
}

proptest! {
    #[test]
    fn packed_scalar_cells_round_trip(
        scalar in any::<char>(),
        width in 0u64..4,
        semantic in 0u64..4,
        protected in any::<bool>(),
        style in any::<u16>(),
        extras in any::<u16>(),
    ) {
        let width = CellWidth::from_bits(width);
        let semantic = Semantic::from_bits(semantic);
        let cell = Cell::EMPTY
            .with_content(CellContent::Scalar(scalar))
            .with_width(width)
            .with_semantic(semantic)
            .with_protected(protected)
            .with_style(StyleId(style))
            .with_extras(ExtrasId(extras));

        // NUL is the one scalar the engine never stores; it reads back blank.
        let expected = if scalar == '\0' { ' ' } else { scalar };
        prop_assert_eq!(cell.content(), CellContent::Scalar(expected));
        prop_assert_eq!(cell.width(), width);
        prop_assert_eq!(cell.semantic(), semantic);
        prop_assert_eq!(cell.protected(), protected);
        prop_assert_eq!(cell.style_id(), StyleId(style));
        prop_assert_eq!(cell.extras_id(), ExtrasId(extras));
        prop_assert_eq!(cell.to_bits() & (0b11111 << 27), 0);
    }

    #[test]
    fn packed_grapheme_cells_round_trip(
        id in 0u32..CONTENT_LIMIT,
        style in any::<u16>(),
        extras in any::<u16>(),
    ) {
        let cell = Cell::EMPTY
            .with_content(CellContent::Grapheme(GraphemeId(id)))
            .with_style(StyleId(style))
            .with_extras(ExtrasId(extras));
        prop_assert_eq!(cell.content(), CellContent::Grapheme(GraphemeId(id)));
        prop_assert_eq!(cell.style_id(), StyleId(style));
        prop_assert_eq!(cell.extras_id(), ExtrasId(extras));
    }

    #[test]
    fn writing_one_field_never_disturbs_another(
        scalar in any::<char>(),
        style in any::<u16>(),
        extras in any::<u16>(),
    ) {
        // Start full, then rewrite each field in turn and re-read all of them.
        let mut cell = Cell::EMPTY
            .with_content(CellContent::Scalar(scalar))
            .with_width(CellWidth::Wide)
            .with_semantic(Semantic::Output)
            .with_protected(true)
            .with_style(StyleId(style))
            .with_extras(ExtrasId(extras));

        cell = cell.with_style(StyleId(0));
        prop_assert_eq!(cell.extras_id(), ExtrasId(extras));
        cell = cell.with_extras(ExtrasId(0));
        prop_assert_eq!(cell.width(), CellWidth::Wide);
        prop_assert_eq!(cell.semantic(), Semantic::Output);
        prop_assert!(cell.protected());
        let expected = if scalar == '\0' { ' ' } else { scalar };
        prop_assert_eq!(cell.content(), CellContent::Scalar(expected));
    }
}
