//! The status bar (specs/oneterm, "Window"): 30 px on the chrome plane, 12 px, its groups parted by
//! rules of 1×14: the rail's fold; the focused space's directory (a click copies it); git; a
//! red dot that blinks while it logs; at the right the progress, a remote space's network,
//! OneTerm's processor and memory
//! (details in a tooltip), the clock and the SFTP panel. Locked, only a lock and "Locked,
//! details hidden", the progress and the clock show.

use std::rc::Rc;
use std::time::Duration;

use quadra::{Align, Clipboard, Length, Role, Signal, View, by_text, div, interval, row, text, use_context};
use quadra_design::{Variant, fonts, icon, icon_button, icons, token};

use crate::locked::shown;
use crate::model::Model;
use crate::signals::percent;
use crate::usage::Clock;

/// What shows when; a rule stands before a group that shows after one that shows.
type When = Rc<dyn Fn() -> bool>;

/// A part of a group: its icon or label and its value, 6 px apart.
fn part() -> quadra::Div {
    row().align_items(Align::Center).gap(token::SPACE_SM)
}

/// A value: mono, in the text colour.
fn value(words: impl Fn() -> String + 'static) -> quadra::Div {
    div().font_family(fonts::MONO).text_color(token::TEXT).child(text(words))
}

/// Groups in a row, a rule before each that shows after one that showed.
fn groups(row_of: quadra::Div, all: Vec<(When, quadra::Div)>) -> quadra::Div {
    let whens: Vec<When> = all.iter().map(|(it, _)| it.clone()).collect();
    let mut line = row_of;
    for (i, (when, group)) in all.into_iter().enumerate() {
        let (before, me, ruled) = (whens[..i].to_vec(), when.clone(), when);
        let rule = div().width(1.0).height(14.0).shrink(0.0).bg(token::LINE_STRONG);
        line = line.child(rule.display(shown(move || ruled() && before.iter().any(|it| it()))));
        line = line.child(group.display(shown(move || me())));
    }
    line
}

fn when(f: impl Fn() -> bool + 'static) -> When {
    Rc::new(f)
}

pub(crate) fn status_bar(model: &Model, clock: Signal<String>, now: Clock) -> impl View + use<> {
    tick(clock, now);
    let (prefs, lock) = (model.prefs(), model.lock());
    let open = move || !lock.locked.get();
    let (vertical, folded) = (model.vertical(), prefs.rail_folded);
    let fold = icon_button(icons::PANEL_LEFT, "Collapse tabs to icons")
        .variant(Variant::Link)
        .label(move || if folded.get() { "Expand tabs" } else { "Collapse tabs to icons" }.to_string())
        .on_click(move || folded.update(|on| *on = !*on));
    let cwd_of = model.clone();
    let cwd = move || {
        let space = cwd_of.active_tab().and_then(|tab| tab.focused());
        space.map(|space| space.session.cwd().get()).unwrap_or_default()
    };
    let (clipboard, said, repo_of) = (use_context::<Rc<dyn Clipboard>>(), cwd.clone(), cwd.clone());
    let placed = cwd.clone();
    let repo = crate::git::follow(repo_of);
    let place = part()
        .role(Role::Button)
        .label("Working directory")
        .on_click(move |_| {
            if let Some(clipboard) = &clipboard {
                clipboard.write_text(&quadra::untrack(&cwd));
            }
        })
        .child(icon(icons::FOLDER).size(14.0).color(token::WARN))
        .child(value(said));
    let counts = move || repo.get().and_then(|it| it.changes).unwrap_or_default();
    let count = |color, sign: &'static str, n: fn(crate::git::Changes) -> usize| {
        div()
            .font_family(fonts::MONO)
            .text_color(color)
            .display(shown(move || n(counts()) > 0))
            .child(text(move || format!("{sign}{}", n(counts()))))
    };
    let git_shown = move || prefs.show_git.get() && repo.with(Option::is_some);
    let git = part()
        .label("Git")
        .child(icon(icons::GIT_BRANCH).size(14.0).color(token::OK))
        .child(value(move || repo.get().map(|it| it.branch).unwrap_or_default()))
        .child(count(token::HUE_GREEN, "\u{2191}", |it| it.ahead))
        .child(count(token::HUE_YELLOW, "M", |it| it.files))
        .child(count(token::HUE_GREEN, "+", |it| it.added))
        .child(count(token::HUE_RED, "-", |it| it.removed));
    let usage = crate::usage::follow();
    let (cpu, ram) = (crate::usage::cpu(usage), crate::usage::ram(usage));
    let meters = part()
        .role(Role::Group)
        .label("CPU and memory")
        .gap(token::SPACE_LG)
        .display(shown(move || prefs.show_usage.get()))
        .child(part().child(text("CPU")).child(value(cpu)))
        .child(part().child(text("RAM")).child(value(ram)));
    let using = part().gap(token::SPACE_LG).label("Usage").child(crate::usage::tip(meters, usage)).child(
        part().display(shown(move || prefs.show_fps.get())).child({
            let said = fps(prefs.show_fps);
            value(move || said.get())
        }),
    );
    let time = value(move || clock.get()).label("Clock").display(shown(move || prefs.show_clock.get()));
    let hidden = part()
        .display(shown(move || !open()))
        .child(icon(icons::LOCK).size(14.0).color(token::HUE_PURPLE))
        .child(text("Locked, details hidden"));
    let shows = move || prefs.show_usage.get() || prefs.show_fps.get();
    let (meter, metered) = meter(model, open);
    let rec = logging(model);
    let left_side = groups(
        part().gap(token::SPACE_LG),
        vec![
            (when(move || open() && vertical.get()), part().child(fold)),
            // A shell that has said no directory has no place to show.
            (when(move || open() && !placed().is_empty()), place),
            (when(move || open() && git_shown()), git),
            (when(move || open() && rec.get().0), recording(rec)),
        ],
    );
    let right_side = groups(
        part().gap(token::SPACE_LG),
        vec![
            (metered, meter),
            (remote_shown(model, open), part().label("Network").child(value(traffic(model)))),
            (when(move || open() && shows()), using),
            (when(move || prefs.show_clock.get()), time),
            (when(open), part().child(sftp(model))),
        ],
    );
    row()
        .align_items(Align::Center)
        .gap(token::SPACE_LG)
        .padding_x(token::SPACE_MD)
        .height(by_text(32.0))
        .shrink(0.0)
        .bg(token::STATUS_BG)
        .border_top(1.0)
        .border_color(token::LINE)
        .text_size(12.0)
        .text_color(token::TEXT_2)
        .test_id("status")
        .child(hidden)
        .child(left_side)
        .child(div().grow(1.0))
        .child(right_side)
}

/// Toggle SFTP panel, in the accent while the dock shows; zoomed in, it zooms out.
fn sftp(model: &Model) -> impl View + use<> {
    let (dock, expanded) = (model.ui().dock, model.ui().expanded);
    icon_button(icons::PANEL_RIGHT, "Toggle SFTP panel")
        .label(move || {
            if expanded.get() { "SFTP is zoomed in; zoom out" } else { "Toggle SFTP panel" }.to_string()
        })
        .toggled(move || dock.get())
        .on_click(move || match expanded.get_untracked() {
            true => expanded.set(false),
            false => dock.update(|on| *on = !*on),
        })
}

/// Whether the network shows: unlocked, a remote space focused.
/// The network group shows while the focused space's connection has said its traffic: one
/// that is not connected yet has nothing to stand between two lines.
fn remote_shown(model: &Model, open: impl Fn() -> bool + 'static) -> When {
    let rate = traffic(model);
    when(move || open() && !rate().is_empty())
}

/// What the focused remote space's connection received and sent in the last second.
fn traffic(model: &Model) -> impl Fn() -> String + 'static {
    let model = model.clone();
    move || {
        let space = model.active_tab().and_then(|tab| tab.focused());
        let conn = space.and_then(|it| it.session.kept::<crate::remote::Conn>());
        conn.map(|it| it.traffic.get()).unwrap_or_default()
    }
}

/// The progress of the tab that shows, else of the first that reports one: the id of what
/// reports it (`build`; not while locked: a detail), a line of 56 px and the percentage; and
/// whether it shows.
fn meter(model: &Model, open: impl Fn() -> bool + 'static) -> (quadra::Div, When) {
    let (model, shows) = (model.clone(), model.prefs().progress_bar);
    // The first space that reports a part done, and the id of what it runs (OSC 7501).
    let found = quadra::memo(move || {
        let tabs = model.tabs().get();
        let mut first = model.active_tab().into_iter().chain(tabs);
        first.find_map(|tab| {
            tab.spaces.with(|all| {
                all.iter().find_map(|it| {
                    let n = it.session.progress().get().and_then(percent)?;
                    Some((n, it.session.status_name().get()))
                })
            })
        })
    });
    let fill = div()
        .height(2.0)
        .bg(token::ACCENT)
        .width(move || Length::Pct(f32::from(found.with(|it| it.as_ref().map_or(0, |it| it.0)))));
    let name = move || found.with(|it| it.as_ref().map(|it| it.1.clone()).unwrap_or_default());
    let meter = part()
        .label("Progress")
        .child(div().max_width(Length::Px(120.0)).display(shown(open)).child(text(name).ellipsis()))
        .child(div().width(56.0).height(2.0).bg(token::LINE_STRONG).child(fill))
        .child(value(move || {
            found.with(|it| it.as_ref().map(|it| format!("{}%", it.0)).unwrap_or_default())
        }));
    (meter, when(move || shows.get() && found.with(Option::is_some)))
}

/// Whether the focused space logs, and whether its dot is lit: looked at each half second,
/// as a log can end on the feeding thread. Written only when it changes.
fn logging(model: &Model) -> Signal<(bool, bool)> {
    let (rec, model) = (quadra::signal((false, false)), model.clone());
    let _ = interval(Duration::from_millis(500), move || {
        let on = model.active_tab().and_then(|tab| tab.focused()).is_some_and(|it| it.session.logging());
        let next = (on, on && !rec.get_untracked().1);
        if next != rec.get_untracked() {
            rec.set(next);
        }
    });
    rec
}

/// The dot of 8 px, red while lit.
fn recording(rec: Signal<(bool, bool)>) -> quadra::Div {
    let lit = move || if rec.get().1 { token::HUE_RED } else { token::STATUS_BG };
    part().role(Role::Image).label("Logging").child(div().size(8.0, 8.0).radius(token::RADIUS_SM).bg(lit))
}

/// The frames the window made in the last second, `60 fps`, counted while shown.
fn fps(shown: Signal<bool>) -> Signal<String> {
    let (said, window) = (quadra::signal(String::new()), quadra::use_context::<quadra::Window>());
    let last = std::cell::Cell::new(0u64);
    let _ = interval(Duration::from_secs(1), move || {
        let made = window.as_ref().map_or(0, quadra::Window::frames);
        let n = made.saturating_sub(last.replace(made));
        if shown.get_untracked() {
            said.set(format!("{n} fps"));
        }
    });
    said
}

/// Keeps `clock` the local time, `2026-11-16 20:45:07`, by `now`: again each second.
fn tick(clock: Signal<String>, now: Clock) {
    let said = move || now().to_string();
    clock.set(said());
    let _ = interval(Duration::from_secs(1), move || clock.set(said()));
}
