//! The settings of how terminals look and behave (specs/oneterm, "Settings"): the pages
//! Appearance and Terminal.

use std::rc::Rc;

use quadra::{
    AnyView, MountCx, NodeId, Signal, Theme, View, dynamic, effect, row, signal, untrack, use_context,
};
use quadra_design::{SettingsGroup, color_picker, input, number_input, segmented, select, switch, token};

use super::Back;
use crate::prefs::{self, Prefs};

pub(super) fn appearance(p: Prefs, d: Prefs) -> Vec<SettingsGroup> {
    let theme = SettingsGroup::new("Theme")
        .row("Theme", "System follows the OS's dark or light mode.", segmented(prefs::THEMES, p.theme))
        .back(p.theme, d.theme)
        .row(
            "Colour theme",
            "A colour theme of its own. Default follows the theme above.",
            select(prefs::COLOUR_THEMES.clone(), p.colour_theme).width(240.0),
        )
        .back(p.colour_theme, d.colour_theme)
        .row(
            "Text size",
            "Size of all text in OneTerm, terminals included, in percent.",
            number_input(p.ui_text).stepper().range(75.0, 200.0).step(5.0),
        )
        .back(p.ui_text, d.ui_text);
    let (l, k) = (p.look, d.look);
    let text = SettingsGroup::new("Terminal text")
        .row(
            "Font family",
            "A font installed on this computer. Default is OneTerm's own monospace font.",
            Families(l.font_family),
        )
        .back(l.font_family, k.font_family)
        .row(
            "Fallback fonts",
            "Fonts tried in order, separated by commas, for characters the font lacks, such as icons.",
            input(l.fallbacks).placeholder("Fallback fonts"),
        )
        .back(l.fallbacks, k.fallbacks)
        .row("Font size", "In pixels.", number_input(l.font_size).stepper().range(8.0, 32.0))
        .back(l.font_size, k.font_size)
        .row("Weight", "Used when the font has this weight.", select(prefs::WEIGHTS, l.weight).width(240.0))
        .back(l.weight, k.weight)
        .row(
            "Line height",
            "A multiple of the font size.",
            number_input(l.line_height).stepper().range(1.0, 2.5).step(0.1),
        )
        .back(l.line_height, k.line_height)
        .row(
            "Cell width",
            "In pixels. 0 uses the font's own width.",
            number_input(l.cell_width).stepper().range(0.0, 48.0).step(0.5).test_id("cell-width"),
        )
        .back(l.cell_width, k.cell_width)
        .row("Gutter", "Show each line's number and the time it was written.", switch(l.show_gutter))
        .back(l.show_gutter, k.show_gutter)
        .row("Ligatures", "Draw the font's ligatures (=>, ->, !=) when it has them.", switch(l.ligatures))
        .back(l.ligatures, k.ligatures)
        .row(
            "Grayscale smoothing",
            "Smooth the text by pixel, not by ClearType's subpixels: no coloured fringes.",
            switch(l.grayscale),
        )
        .back(l.grayscale, k.grayscale)
        .row(
            "Semantic highlighting",
            "Colour paths, commands, numbers, errors and URLs in text a program left plain.",
            switch(l.semantic_highlighting),
        )
        .back(l.semantic_highlighting, k.semantic_highlighting);
    let sides = prefs::PADDING.iter().zip(l.padding.into_iter().zip(k.padding));
    let text = sides.fold(text, |group, (&(key, name, _), (side, default))| {
        let id = &key["terminal.".len()..];
        let field = number_input(side).stepper().range(0.0, 64.0).test_id(id);
        group.row(name, "Space around the terminal's text, in pixels.", field).back(side, default)
    });
    let theirs = [
        token::EDITOR_FOREGROUND,
        token::CANVAS,
        token::EDITOR_CURSOR,
        token::EDITOR_SELECTION,
        token::CANVAS,
    ];
    // The gutter's text, clock and line number, unset: the foreground mixed half with the
    // background, as the terminal draws them, in the window's theme.
    let window = use_context::<Signal<Rc<Theme>>>();
    let half = move || {
        window.map(|it| it.with(|t| t.color(token::EDITOR_FOREGROUND).lerp(t.color(token::CANVAS), 0.5)))
    };
    let colors = prefs::COLORS.iter().zip(l.colors.into_iter().zip(k.colors)).enumerate().fold(
        SettingsGroup::new("Terminal colours"),
        |group, (n, ((_, name), (color, default)))| {
            let field = move || color_picker(color).label(name.to_string());
            let field = match n {
                0..=3 => AnyView::new(field().fallback(theirs[n])),
                5 => AnyView::new(field().fallback(theirs[4])),
                _ => AnyView::new(dynamic(move || match half() {
                    Some(mixed) => AnyView::new(field().fallback(mixed)),
                    None => AnyView::new(field()),
                })),
            };
            group.row(name, "A colour as #rrggbb. Leave empty for the theme's.", field).back(color, default)
        },
    );
    let colors = colors
        .row(
            "Minimum contrast",
            "Text is lightened or darkened to stand out this much from its background; 1 is off.",
            number_input(l.min_contrast).stepper().range(1.0, 21.0).step(0.5).test_id("min-contrast"),
        )
        .back(l.min_contrast, k.min_contrast);
    let ansi = (0..8).fold(SettingsGroup::new("ANSI colours"), |group, n| {
        let field = |at: usize| color_picker(l.ansi[at]).label(prefs::ANSI[at].1).fallback(token::ANSI[at]);
        // A row of two colours, the normal and the bright: it differs while one does.
        let pair = [(l.ansi[n], k.ansi[n].get_untracked()), (l.ansi[n + 8], k.ansi[n + 8].get_untracked())];
        let back = pair.clone();
        group
            .row(
                prefs::ANSI[n].1,
                "Normal and bright, as #rrggbb. Leave empty for the theme's.",
                row().gap(token::SPACE_MD).child(field(n)).child(field(n + 8)),
            )
            .resets_with(
                move || pair.iter().any(|(now, was)| now.with(|now| now != was)),
                move || back.iter().for_each(|(now, was)| now.set(was.clone())),
            )
    });
    vec![theme, text, colors, ansi]
}

pub(super) fn terminal(p: Prefs, d: Prefs) -> Vec<SettingsGroup> {
    let cursor = SettingsGroup::new("Cursor & history")
        .row("Cursor shape", "Until a program sets its own.", segmented(prefs::CURSORS, p.cursor))
        .back(p.cursor, d.cursor)
        .row("Cursor blink", "Whether the cursor blinks.", switch(p.blink))
        .back(p.blink, d.blink)
        .row(
            "Scrollback lines",
            "Lines of history each space keeps.",
            number_input(p.scrollback).stepper().range(0.0, 100_000.0).step(1000.0),
        )
        .back(p.scrollback, d.scrollback);
    let mouse = SettingsGroup::new("Mouse & wheel")
        .row("Context menu", "A right click opens a space's menu.", switch(p.context_menu))
        .back(p.context_menu, d.context_menu)
        .row(
            "Copy on select",
            "Copy a selection as soon as the mouse button is released.",
            switch(p.copy_on_select),
        )
        .back(p.copy_on_select, d.copy_on_select)
        .row("Middle-click paste", "The middle button pastes.", switch(p.middle_paste))
        .back(p.middle_paste, d.middle_paste)
        .row(
            "Scroll speed",
            "A multiple of the system's wheel speed.",
            number_input(p.scroll_speed).stepper().range(0.1, 10.0).step(0.5),
        )
        .back(p.scroll_speed, d.scroll_speed)
        .row(
            "Alternate scroll",
            "In full-screen programs the wheel sends arrow keys.",
            switch(p.alternate_scroll),
        )
        .back(p.alternate_scroll, d.alternate_scroll);
    let shell = SettingsGroup::new("Shell")
        .row("Default shell", "The shell a new tab starts.", shell_choice(p.shell))
        .back(p.shell, d.shell)
        .row(
            "Program",
            "The shell's program, when Default shell is Custom program.",
            input(p.shell_program).placeholder("C:\\path\\to\\shell.exe").label("Program"),
        )
        .back(p.shell_program, d.shell_program)
        .row(
            "Arguments",
            "Separated by spaces.",
            input(p.shell_args).placeholder("--login").label("Arguments"),
        )
        .back(p.shell_args, d.shell_args)
        .row(
            "New spaces start in",
            "The folder a new shell starts in.",
            select(prefs::STARTS, p.start).width(240.0),
        )
        .back(p.start, d.start)
        .row(
            "Force links",
            "Make programs that show links only in terminals they know show them here (FORCE_HYPERLINK). Some then write links into pipes too.",
            switch(p.force_links),
        )
        .back(p.force_links, d.force_links)
        .row(
            "Shell integration",
            "Start Command Prompt, PowerShell and bash so they report their folder and where each command begins and ends, and write UTF-8. New tabs, completion and highlighting use it.",
            switch(p.integration),
        )
        .back(p.integration, d.integration);
    vec![cursor, mouse, shell]
}

/// The default shell as a choice of the shells found, written as its name; the first is
/// the OS's own, the last a program typed.
fn shell_choice(shell: Signal<String>) -> quadra_design::Select {
    let found = crate::shells::found().into_iter().map(|it| it.name.to_string());
    let names =
        std::iter::once("System default".to_string()).chain(found).chain([crate::shells::CUSTOM.into()]);
    named_choice(names.collect(), shell)
}

/// A select of `names` that writes the name chosen to `value`, and follows it; the first
/// stands for none (an empty value), and a value no name has shows as the first.
fn named_choice(names: Vec<String>, value: Signal<String>) -> quadra_design::Select {
    let known = names.clone();
    let at = move || known.iter().position(|it| value.with(|now| it == now)).unwrap_or(0);
    let chosen = signal(untrack(&at));
    effect(move || {
        let now = at();
        if untrack(|| chosen.get()) != now {
            chosen.set(now);
        }
    });
    let words = names.clone();
    effect(move || {
        let name = if chosen.get() == 0 { String::new() } else { words[chosen.get()].clone() };
        if untrack(|| value.get()) != name {
            value.set(name);
        }
    });
    select(names, chosen).width(240.0)
}

/// The terminal's family, chosen from the machine's in a select with a search; the first
/// is the kit's mono face.
struct Families(Signal<String>);

impl View for Families {
    fn mount(self, cx: &mut MountCx) -> NodeId {
        let families = cx.tree.fonts().borrow_mut().text.backend().families();
        let names = std::iter::once("Default".to_string()).chain(families).collect();
        named_choice(names, self.0).searchable("Find a family\u{2026}").label("Font family").mount(cx)
    }
}
