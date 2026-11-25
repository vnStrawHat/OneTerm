//! How signals look (specs/oneterm, "Signals"): a tab's signal at the end of its row, its icon, the
//! seam bar of its progress, the second line of its row, a space header's signal.

use quadra::{Align, AnyView, Display, Length, Role, View, div, dynamic, row, text};
use quadra_design::{fonts, icon, icons, spinner, token};
use quadra_term::ProgramState;
use quadra_term::vt::Progress;

use crate::model::Tab;
use crate::signals::{percent, state_word};

/// A state's icon, outlined in its colour, and what a reader hears of it; none for idle.
fn state_look(state: ProgramState) -> Option<(&'static str, quadra::Token, &'static str)> {
    match state {
        ProgramState::Blocked => Some((icons::CIRCLE_HELP, token::WARN, "Blocked, needs permission")),
        ProgramState::Done => Some((icons::CIRCLE_CHECK, token::OK, "Done, result not yet seen")),
        ProgramState::Error => Some((icons::CIRCLE_X, token::ERROR, "Error")),
        ProgramState::Working => Some((icons::LOADER_CIRCLE, token::ACCENT, "Working")),
        ProgramState::Idle => None,
    }
}

/// The state's icon of `px`, named for a reader; working's turns.
pub(crate) fn state_icon(state: ProgramState, px: f32) -> Option<quadra::Div> {
    let (body, color, said) = state_look(state)?;
    if state == ProgramState::Working {
        return Some(div().shrink(0.0).text_color(color).child(spinner().size(px).label(said)));
    }
    Some(div().shrink(0.0).role(Role::Image).label(said).child(icon(body).size(px).color(color)))
}

/// The bell that rang unseen, in the error colour.
fn bell(px: f32) -> quadra::Div {
    div()
        .shrink(0.0)
        .role(Role::Image)
        .label("Bell rang")
        .child(icon(icons::BELL).size(px).color(token::ERROR))
}

/// The part done of a tab's progress, when the settings show it on tabs.
pub(crate) fn part_of(tab: Tab) -> Option<u8> {
    tab.progress().filter(|_| tab.prefs.progress_tab.get()).and_then(percent)
}

/// What a tab says at its end: blocked, error and done as their icons, else its progress in
/// the accent, else working's spinner, else a bell when one rang unseen, else nothing: a
/// program that said nothing (or is idle) shows nothing.
pub(crate) fn tab_mark(tab: Tab) -> impl View {
    dynamic(move || {
        let state = tab.status();
        let shown = !matches!(state, ProgramState::Working | ProgramState::Idle);
        match state_icon(state, 14.0).filter(|_| shown) {
            Some(face) => AnyView::new(face),
            None if part_of(tab).is_some() => AnyView::new(
                div()
                    .font_family(fonts::MONO)
                    .text_size(12.0)
                    .text_color(token::ACCENT)
                    .child(text(move || part_of(tab).map(|n| format!("{n}%")).unwrap_or_default())),
            ),
            None => match state_icon(state, 14.0) {
                Some(face) => AnyView::new(face),
                None if tab.bells() > 0 => AnyView::new(bell(14.0)),
                None => AnyView::new(div()),
            },
        }
    })
}

/// What a reader hears of a tab in the folded rail: its title, then its signal if it has one.
pub(crate) fn tab_said(tab: Tab) -> String {
    let state = tab.status();
    let signal = match state_look(state) {
        Some((.., said)) if state != ProgramState::Working => said.to_lowercase(),
        _ => match (part_of(tab), state, tab.bells()) {
            (Some(n), ..) => format!("working, {n} percent"),
            (_, ProgramState::Working, _) => "working".into(),
            (_, _, 1..) => "bell rang".into(),
            _ => return tab.title(),
        },
    };
    format!("{}, {signal}", tab.title())
}

/// The first line of a folded tab's tooltip: the focused space's working directory, empty
/// while its shell said none.
pub(crate) fn tab_dir(tab: Tab) -> String {
    tab.focused().map(|space| space.session.cwd().get()).unwrap_or_default()
}

/// The program the focused space's shell was started as (`cmd.exe`), `ssh deploy@db` for a
/// remote one: the first line of a folded tab's tooltip while there is no directory.
pub(crate) fn tab_program(tab: Tab) -> String {
    let Some(space) = tab.focused() else { return String::new() };
    match (&space.remote, &space.edit) {
        (_, Some(_)) => None,
        (Some(remote), None) => Some(format!("ssh {}", remote.target.label())),
        (None, None) => crate::complete::program(&space.session),
    }
    .unwrap_or_default()
}

/// The second line of a folded tab's tooltip, read when it shows: what the focused space's
/// shell runs (`pwsh.exe · npm run dev`), or its full path; `ssh deploy@db` for a remote one.
pub(crate) fn tab_runs(tab: Tab) -> String {
    let Some(space) = tab.focused() else { return String::new() };
    match (&space.remote, &space.edit) {
        (_, Some(_)) => None,
        (Some(remote), None) => Some(format!("ssh {}", remote.target.label())),
        (None, None) => crate::complete::running(&space.session),
    }
    .unwrap_or_default()
}

/// The tab's icon of `px`: a server for a tab with a remote space, a terminal else; in the
/// accent while it shows, `off` else.
pub(crate) fn tab_icon(tab: Tab, px: f32, on: impl Fn() -> bool + 'static, off: quadra::Token) -> impl View {
    dynamic(move || {
        let remote = tab.spaces.with(|spaces| spaces.iter().any(|it| it.remote.is_some()));
        // The focused space's shell's icon; a remote tab's is a server.
        let shell = tab.focused().and_then(|it| it.session.kept::<crate::shells::ShellName>());
        let body = match (remote, shell) {
            (true, _) => icons::SERVER,
            (false, Some(shell)) => crate::shell_icons::of(&shell.0),
            (false, None) => icons::SQUARE_TERMINAL,
        };
        AnyView::new(icon(body).size(px).color(if on() { token::ACCENT } else { off }))
    })
}

/// What a tab's second line says, while it shows: the kinds of its spaces and their count.
pub(crate) fn tab_line(tab: Tab) -> String {
    // An editor's space is not counted.
    let (n, ssh) = tab.spaces.with(|spaces| {
        let terminals = || spaces.iter().filter(|it| it.edit.is_none());
        (terminals().count(), terminals().filter(|it| it.remote.is_some()).count())
    });
    let local = n - ssh;
    let kinds = match (ssh, local) {
        (0, _) => format!("{local} local"),
        (_, 0) => format!("{ssh} ssh"),
        _ => format!("{ssh} ssh \u{B7} {local} local"),
    };
    format!("{kinds} \u{B7} {n} {}", if n == 1 { "space" } else { "spaces" })
}

/// A bar of 2 px along a tab's lower seam, `bottom` px from its bottom edge (-1: over the
/// line under it), its fill as long as its progress.
pub(crate) fn progress_bar(tab: Tab, bottom: f32) -> impl View {
    let fill =
        div().height(2.0).width(move || Length::Pct(f32::from(part_of(tab).unwrap_or(0)))).bg(move || {
            if matches!(tab.progress(), Some(Progress::Error(_))) { token::ERROR } else { token::ACCENT }
        });
    div()
        .absolute()
        .left(0.0)
        .right(0.0)
        .bottom(bottom)
        .height(2.0)
        .bg(token::LINE_STRONG)
        .display(move || if part_of(tab).is_some() { Display::Flex } else { Display::None })
        .child(fill)
}

/// What a space's header says at its right: its progress in the accent, its state's icon and
/// label in the state's colour (`deploy blocked`), then the link's note (a remote one's
/// latency); with no state and no note, the program's title.
pub(crate) fn header_signal<F: Fn() -> String + 'static>(
    s: &quadra_term::Session,
    prefs: crate::prefs::Prefs,
    title: F,
) -> impl View + use<F> {
    let (status, progress, note, name) = (s.status(), s.progress(), s.link_note(), s.status_name());
    // The settings say whether the header shows states.
    let status =
        quadra::memo(move || if prefs.status_shown.get() == 0 { status.get() } else { ProgramState::Idle });
    let mark = dynamic(move || {
        let (Some(face), Some(word)) = (state_icon(status.get(), 12.0), state_word(status.get())) else {
            return AnyView::new(div());
        };
        let color = state_look(status.get()).map_or(token::TEXT_2, |it| it.1);
        let label = move || match name.get() {
            name if name.is_empty() => word.to_string(),
            name => format!("{name} {word}"),
        };
        AnyView::new(
            row()
                .align_items(Align::Center)
                .gap(token::SPACE_XS)
                .text_color(color)
                .child(face)
                .child(text(label)),
        )
    });
    let named = div().child(text(move || {
        let quiet = state_word(status.get()).is_none() && note.with(String::is_empty);
        if quiet { title() } else { String::new() }
    }));
    let note = div().font_family(fonts::MONO).child(text(move || note.get()));
    let part = div()
        .font_family(fonts::MONO)
        .text_color(token::ACCENT)
        .child(text(move || progress.get().and_then(percent).map(|n| format!("{n}%")).unwrap_or_default()));
    quadra_design::hstack().shrink(0.0).child(part).child(mark).child(note).child(named)
}
