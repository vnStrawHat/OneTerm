//! The workspace (specs/oneterm, "Spaces"): the spaces of the tab that shows, where its pane tree
//! puts them, each with its header of 18 px; the focused one has an accent ring (in the colour
//! of its input channel). A space whose program ended says so, and a key then closes it.

use std::time::Instant;

use std::rc::Rc;

use quadra::{
    Align, AnyView, Clipboard, MountCx, NodeId, View, by_text, column, div, dynamic, effect, row, signal,
    text, untrack, use_context,
};
use quadra_design::{MenuItem, context_menu, fonts, token};
use quadra_term::{Link, Session};

use crate::links::Links;
use crate::model::{Model, Side, Space, Tab};

pub(crate) fn workspace(model: &Model) -> impl View + use<> {
    let shown = model.clone();
    let tab = dynamic(move || match shown.active_tab() {
        Some(tab) => AnyView::new(columns(&shown, tab)),
        None => AnyView::new(crate::no_tab()),
    });
    // The expanded SFTP view takes the workspace's place; the spaces run on, not displayed.
    let expanded = model.ui().expanded;
    tab.container(row().grow(1.0).bg(token::CANVAS).display(crate::locked::shown(move || !expanded.get())))
}

/// The tab's pane tree, a space a leaf: a space is built once, and moves as the tree does.
/// While the tab arranges itself it is arranged again when the tabs move between the rail and
/// the strip, and a seam dragged makes the layout the user's.
fn columns(model: &Model, tab: Tab) -> impl View + use<> {
    let model = model.clone();
    let vertical = model.vertical();
    effect(move || {
        vertical.get();
        untrack(|| tab.arrange());
    });
    effect(move || {
        tab.layout.with(|_| ());
        untrack(|| tab.follow_layout());
    });
    let leaf = move |group: &str| {
        let space =
            tab.spaces.with_untracked(|spaces| spaces.iter().find(|it| it.id.to_string() == group).cloned());
        match space {
            Some(space) if space.edit.is_some() => AnyView::new(crate::editing::view(&model, tab, space)),
            Some(space) => AnyView::new(SpaceView { model: model.clone(), tab, space }),
            None => AnyView::new(div()),
        }
    };
    row().grow(1.0).min_width(0.0).child(quadra_design::work::panes(tab.layout, leaf))
}

/// Whether the space is its tab's focused one.
fn focused(tab: Tab, id: u64) -> impl Fn() -> bool + Copy {
    move || tab.spaces.with(|spaces| spaces.iter().position(|it| it.id == id)) == Some(tab.focus.get())
}

struct SpaceView {
    model: Model,
    tab: Tab,
    space: Space,
}

impl View for SpaceView {
    /// The header, the terminal and the ring; the terminal takes the focus whenever the
    /// space is the tab's focused one.
    fn mount(self, cx: &mut MountCx) -> NodeId {
        let SpaceView { model, tab, space } = self;
        let shows = focused(tab, space.id);
        // A remote space's connection being made: the Connect dialogs have the focus.
        let conn = space.session.kept::<crate::remote::Conn>();
        let made = move || conn.as_ref().is_none_or(|it| it.phase.get() == crate::remote::Phase::Done);
        let id = space_view(&model, tab, space).mount(cx);
        let (handle, locked) = (cx.handle.clone(), model.lock().locked);
        effect(move || {
            // Opened again, the focus comes back to the focused space; one pressed keeps what
            // the press focused in it. After the writes of this pass, which close a dialog.
            if shows() && !locked.get() && made() {
                let later = handle.clone();
                handle.with(move |_| {
                    later.with(move |tree| {
                        let mut at = tree.focused();
                        while let Some(node) = at {
                            if node == id {
                                return;
                            }
                            at = tree.parent(node);
                        }
                        tree.focus_first(id);
                    })
                });
            }
        });
        id
    }
}

/// Who a remote space is: `user@` its host's name, else `user@host`; the user the one the
/// connection was answered with when none was given.
pub(crate) fn who(
    model: &Model,
    remote: &crate::shells::Remote,
    conn: Option<&crate::remote::Conn>,
) -> String {
    let label = conn.map_or_else(|| remote.target.label(), |it| it.label.get());
    let name = model.host_name(remote);
    match label.split_once('@') {
        _ if name == remote.target.label() => label,
        Some((user, _)) => format!("{user}@{name}"),
        None => name,
    }
}

/// What the header says of the program: its title shortened, or the shell's name before it
/// gives one (a newer console host gives none at the start).
fn program_name(s: &quadra_term::Session) -> impl Fn() -> String + 'static {
    let (title, shell) = (s.title(), s.kept::<crate::shells::ShellName>());
    move || match title.get() {
        title if !title.is_empty() => crate::tab::short(&title),
        _ => shell.as_ref().map(|it| it.0.clone()).unwrap_or_default(),
    }
}

fn space_view(model: &Model, tab: Tab, space: Space) -> quadra::Div {
    let s = space.session.clone();
    let index = move || tab.spaces.with(|spaces| spaces.iter().position(|it| it.id == space.id).unwrap_or(0));
    let on = focused(tab, space.id);
    let (title, cwd, exited) = (s.title(), s.cwd(), s.exited());
    // Once the program ended it says so, and the next key closes the space.
    let (said, closes, gone) = (s.clone(), model.clone(), space.id);
    effect(move || {
        if let Some(code) = exited.get() {
            let words = format!("\r\n[process exited {code}]\r\n");
            said.feeder().feed(words.as_bytes(), Instant::now());
            let closes = closes.clone();
            said.connect(Link::new(move |_| _ = closes.close_space(gone)));
        }
    });
    // Where it is: the directory (else the title), after a remote space's user and host.
    let (named, remote, conn) = (model.clone(), space.remote.clone(), s.kept::<crate::remote::Conn>());
    let place = move || {
        let dir = cwd.get();
        let dir = if dir.is_empty() { title.get() } else { dir };
        match &remote {
            Some(remote) => {
                let who = who(&named, remote, conn.as_deref());
                // A title in the directory's place; one that says who already is not said twice.
                let sep = if cwd.with(String::is_empty) { " " } else { ":" };
                if sep == " " && dir.starts_with(&who) { dir } else { format!("{who}{sep}{dir}") }
            }
            None => dir,
        }
    };
    let links = Links::here(model);
    // A tab of one space shows no header: the tab says what it would.
    let many = move || tab.spaces.try_with(|it| it.len() > 1).unwrap_or(false);
    let header = row()
        .display(crate::locked::shown(many))
        .align_items(Align::Center)
        .gap(token::SPACE_MD)
        .padding_x(token::SPACE_MD)
        .height(by_text(18.0))
        .shrink(0.0)
        .bg(token::PANEL)
        .border_bottom(1.0) // inside the 18 px
        .border_color(token::LINE)
        .text_size(12.0)
        // The border leaves 17: a line of 16 over one pixel has the words in their middle.
        .padding_bottom(1.0)
        .line_height(16.0 / 12.0)
        .text_color(token::TEXT_2)
        .child(
            div()
                .grow(1.0)
                .cut_to_parent()
                .font_family(fonts::MONO)
                .text_color(move || if on() { token::TEXT } else { token::TEXT_2 })
                .child(text(place).ellipsis()),
        )
        .child(crate::channels::chip(space.channel))
        .child(crate::marks::header_signal(&s, model.prefs(), program_name(&s)))
        .child(text(move || exited.get().map(|code| format!("exited {code}")).unwrap_or_default()));
    // The focused space's ring is its border, so nothing lies over the terminal for the pointer.
    let channel = space.channel;
    // A closed space's signals may go before its view does.
    let ring = move || match (on() && tab.spaces.with(Vec::len) > 1, channel.try_get().flatten()) {
        (true, Some(c)) => crate::channels::colour(c),
        (true, None) => token::ACCENT,
        (false, _) => token::CANVAS,
    };
    column()
        .grow(1.0)
        .min_width(0.0)
        .min_height(0.0)
        .bg(token::CANVAS)
        .border(1.0)
        .border_color(ring)
        // A press anywhere in it makes it the tab's focused space (the focus is the
        // terminal's, a node inside: its own focus handlers do not reach this box).
        .on_mount(move |tree, id| {
            tree.on_pointer_capture(id, quadra::PointerKind::Down, move |_| tab.focus.set(index()))
        })
        .child(header)
        .child(
            column()
                .grow(1.0)
                .min_height(0.0)
                .child(crate::find::around(
                    &s,
                    crate::complete::around(model, &space, |view| menu_of(model, &s, &space, view, &links)),
                ))
                .child(flash(&s)),
        )
        .child(links.dialog())
}

/// The tint over the space while its bell flashes; not displayed otherwise, so the pointer
/// reaches the terminal under it.
fn flash(s: &Session) -> impl quadra::View + use<> {
    let on = s.kept::<crate::signals::Flash>().map(|it| it.0);
    let on = move || on.is_some_and(|on| on.get());
    let shown = move || if on() { quadra::Display::Flex } else { quadra::Display::None };
    div().absolute().top(0.0).left(0.0).right(0.0).bottom(0.0).display(shown).bg(token::SELECTION)
}

/// The space's menu for the other button (specs/oneterm, "Spaces"), when the settings have it; the
/// terminal follows the settings of the pointer and the wheel.
fn menu_of(
    model: &Model,
    s: &Session,
    space: &Space,
    view: quadra_term::Terminal,
    links: &Links,
) -> impl View + use<> {
    let (p, id, folder) = (model.prefs(), space.id, crate::logs::folder(&model.prefs()));
    let name = space.remote.as_ref().map_or_else(|| "local".to_string(), |it| it.target.host.clone());
    let (settings, look) = (signal(untrack(|| p.term())), signal(untrack(|| p.look())));
    effect(move || settings.set(p.term()));
    // A new look is followed by the terminal as it is: its screen stays.
    effect(move || look.set(p.look()));
    let (s, model, clipboard, space) =
        (s.clone(), model.clone(), use_context::<Rc<dyn Clipboard>>(), space.clone());
    let (follows, links) = (links.clone(), links.clone());
    let view = view.settings(settings).look(look).on_link(move |link| follows.follow(link));
    context_menu(view, move || {
        if !p.context_menu.get_untracked() {
            return Vec::new();
        }
        let on_link = s.link_pressed().map(|link| links.menu(link)).unwrap_or_default();
        let chosen = s.selection_text();
        let (copies, pastes, (all, clears)) = (clipboard.clone(), clipboard.clone(), (s.clone(), s.clone()));
        let (pasted, closes) = (s.clone(), model.clone());
        let items = vec![
            MenuItem::new("Copy").keys("Ctrl+Shift+C").disabled(chosen.is_none()).on_select(move || {
                if let (Some(clipboard), Some(text)) = (&copies, &chosen) {
                    clipboard.write_text(text);
                }
            }),
            MenuItem::new("Paste").keys("Ctrl+Shift+V").on_select(move || {
                if let Some(clipboard) = &pastes {
                    pasted.paste(&clipboard.read_text().unwrap_or_default());
                }
            }),
            MenuItem::new("Select all").on_select(move || all.select_all()),
            MenuItem::new("Clear scrollback").on_select(move || clears.clear_history()),
            MenuItem::separator(),
            logging(&model, &s, &name, folder.clone()),
            MenuItem::separator(),
            split(&model, "Split right", Side::Right, "Ctrl+Shift+Right"),
            split(&model, "Split left", Side::Left, "Ctrl+Shift+Left"),
            split(&model, "Split up", Side::Up, "Ctrl+Shift+Up"),
            split(&model, "Split down", Side::Down, "Ctrl+Shift+Down"),
            MenuItem::separator(),
            crate::channels::menu(model.tabs(), &space),
            MenuItem::separator(),
            MenuItem::new("Close space").keys("Ctrl+W").danger().on_select(move || {
                _ = closes.close_space(id);
            }),
        ];
        on_link.into_iter().chain(items).collect()
    })
}

/// A row that splits the space to a side.
fn split(model: &Model, name: &str, side: Side, keys: &str) -> MenuItem {
    let model = model.clone();
    MenuItem::new(name).keys(keys).on_select(move || model.split_to(side))
}

/// Start logging, or Stop logging while a log is open.
fn logging(model: &Model, s: &Session, name: &str, folder: Option<std::path::PathBuf>) -> MenuItem {
    let (model, s, name) = (model.clone(), s.clone(), name.to_string());
    match s.logging() {
        true => MenuItem::new("Stop logging").on_select(move || s.stop_log()),
        false => MenuItem::new("Start logging").on_select(move || model.start_log(&s, &name, folder.clone())),
    }
}
