//! The editor space (specs/oneterm, "Spaces"): its header of 18 px (the full remote path, the type,
//! whether it has changes, Diff and Save), the editor or the diff in its place, and the
//! question when it closes with changes.

use std::rc::Rc;

use quadra::{Align, AnyView, MountCx, NodeId, View, by_text, column, div, effect, row, show, text};
use quadra_code::{EditorCommand, EditorWidget, editor, line_colours};
use quadra_design::{MenuItem, Size, Variant, button, context_menu, dialog, diff, fonts, token};
use quadra_tree::Tree;

use super::{CONTEXT, Content, FileEdit, Save, label};
use crate::locked::shown;
use crate::model::{Model, Space, Tab};

/// The view of an editor space; the editor takes the focus whenever the space is the tab's
/// focused one.
pub(crate) fn view(model: &Model, tab: Tab, space: Space) -> impl View + use<> {
    EditView { model: model.clone(), tab, space }
}

struct EditView {
    model: Model,
    tab: Tab,
    space: Space,
}

/// Whether the space is its tab's focused one.
fn focused(tab: Tab, id: u64) -> impl Fn() -> bool + Copy {
    move || tab.spaces.with(|spaces| spaces.iter().position(|it| it.id == id)) == Some(tab.focus.get())
}

/// The editor's widget under a node.
fn widget(tree: &mut Tree, at: NodeId) -> Option<NodeId> {
    let mut stack = vec![at];
    while let Some(id) = stack.pop() {
        if tree.widget_mut::<EditorWidget>(id).is_some() {
            return Some(id);
        }
        stack.extend_from_slice(tree.children(id));
    }
    None
}

impl View for EditView {
    fn mount(self, cx: &mut MountCx) -> NodeId {
        let EditView { model, tab, space } = self;
        let Some(edit) = space.edit.clone() else { return div().mount(cx) };
        let id = body(&model, tab, &space, &edit).mount(cx);
        let found = widget(cx.tree, id);
        if let Some(w) = found {
            *edit.widget.borrow_mut() = Some((cx.handle.clone(), w));
            // Gone with its tab: its id is no editor's any more.
            let gone = edit.clone();
            quadra::on_cleanup(move || {
                let mut at = gone.widget.borrow_mut();
                if at.as_ref().is_some_and(|it| it.1 == w) {
                    *at = None;
                }
            });
        }
        let (shows, handle, locked) = (focused(tab, space.id), cx.handle.clone(), model.lock().locked);
        let asking = edit.asking;
        effect(move || {
            // As a terminal's space does: once the writes of this pass are done.
            if shows() && !locked.get() {
                let later = handle.clone();
                handle.with(move |_| {
                    later.with(move |tree| {
                        // Its question keeps the focus.
                        if quadra::untrack(|| asking.try_get()).unwrap_or(false) {
                            return;
                        }
                        let mut at = tree.focused();
                        while let Some(node) = at {
                            if node == id {
                                return;
                            }
                            at = tree.parent(node);
                        }
                        match found {
                            Some(w) => tree.focus(w),
                            None => _ = tree.focus_first(id),
                        }
                    })
                });
            }
        });
        id
    }
}

fn body(model: &Model, tab: Tab, space: &Space, edit: &Rc<FileEdit>) -> quadra::Div {
    let (on, id) = (focused(tab, space.id), space.id);
    let index = move || tab.spaces.with(|spaces| spaces.iter().position(|it| it.id == id).unwrap_or(0));
    let ring = move || match on() && tab.spaces.with(Vec::len) > 1 {
        true => token::ACCENT,
        false => token::CANVAS,
    };
    let main = match &edit.content {
        Content::Binary(size) => AnyView::new(binary(*size)),
        Content::Text(text) => AnyView::new(editing(edit, text, model.prefs().editor_wrap)),
    };
    let main = context_menu(main, menu(model, edit, id));
    let (saves, model_) = (edit.clone(), model.clone());
    column()
        .grow(1.0)
        .min_width(0.0)
        .min_height(0.0)
        .bg(token::CANVAS)
        .border(1.0)
        .border_color(ring)
        .key_context(CONTEXT)
        .on_action::<Save>(move |_| saves.save(&model_, || {}))
        .on_mount(move |tree, id| {
            tree.on_pointer_capture(id, quadra::PointerKind::Down, move |_| tab.focus.set(index()))
        })
        .child(header(model, edit, on))
        .child(main)
        .child(ask(model, edit))
}

fn header(model: &Model, edit: &Rc<FileEdit>, on: impl Fn() -> bool + 'static) -> quadra::Div {
    let (modified, saving) = (edit.modified, edit.saving);
    // A closed space's signals may go before its view does.
    let state = move || match (saving.try_get().unwrap_or(false), modified.try_get().unwrap_or(false)) {
        (true, _) => "Saving\u{2026}",
        (false, true) => "\u{25CF} Modified",
        (false, false) => "",
    };
    let kind = match edit.content {
        Content::Binary(_) => "binary",
        Content::Text(_) => label(&edit.kind),
    };
    let head = quadra_design::hstack()
        .padding_x(token::SPACE_MD)
        .height(by_text(18.0))
        .shrink(0.0)
        .bg(token::PANEL)
        .border_bottom(1.0) // inside the 18 px
        .border_color(token::LINE)
        .text_size(12.0)
        // As a terminal's header: a line of 16 over one pixel in the 17 the border leaves.
        .padding_bottom(1.0)
        .line_height(16.0 / 12.0)
        .text_color(token::TEXT_2)
        .child(place(&edit.place, on))
        .child(div().text_color(token::TEXT_3).child(text(kind)))
        .child(div().text_color(token::WARN).child(text(state)));
    if matches!(edit.content, Content::Binary(_)) {
        return head;
    }
    let (diffs, saves, model) = (edit.clone(), edit.clone(), model.clone());
    let small = |b: quadra_design::Button| b.size(Size::Xs).variant(Variant::Ghost);
    let (wraps, wrapped) = (model.prefs().editor_wrap, edit.clone());
    let wrap = move || {
        wraps.update(|on| *on = !*on);
        // The editor has the focus again, unless the diff shows in its place.
        if !wrapped.diff.get_untracked() {
            wrapped.focus();
        }
    };
    head.child(small(button("Wrap")).toggled(wraps).on_click(wrap))
        .child(small(button("Diff")).toggled(edit.diff).on_click(move || diffs.toggle_diff()))
        .child(
            small(button("Save"))
                .disabled(move || !modified.try_get().unwrap_or(false))
                .on_click(move || saves.save(&model, || {})),
        )
}

/// `user@host:/full/path`: too long, its folder is cut and the file's name stays whole.
fn place(place: &str, on: impl Fn() -> bool + 'static) -> quadra::Div {
    let at = place.rfind('/').unwrap_or(0);
    let (folder, name) = place.split_at(at);
    row()
        .grow(1.0)
        .min_width(0.0)
        .cut_to_parent()
        .font_family(fonts::MONO)
        .text_color(move || if on() { token::TEXT } else { token::TEXT_2 })
        .role(quadra::Role::Text)
        .label(place.to_string())
        .child(div().min_width(0.0).child(text(folder.to_string()).ellipsis()))
        .child(div().shrink(0.0).child(text(name.to_string())))
}

/// The editor, coloured by the file's type, its gutter marking the changes against the base;
/// while the diff shows, the diff in its place (the editor stays, with its undo).
fn editing(edit: &Rc<FileEdit>, source: &str, wraps: quadra::Signal<bool>) -> quadra::Div {
    let shows = edit.diff;
    let code = editor(source.to_string())
        .path(edit.kind.clone())
        .font(13.0, 20.0)
        .wrap_when(wraps)
        .base(edit.base.get_untracked())
        .changed(edit.modified)
        .keep(edit.doc.clone());
    let edit = edit.clone();
    let compared = move || {
        let (old, new) = (edit.base.get_untracked(), edit.now.get_untracked());
        let view = match old == new {
            true => AnyView::new(
                div().padding(token::SPACE_LG).text_color(token::TEXT_3).child(text("No changes.")),
            ),
            false => AnyView::new(diff(old, new).colours(line_colours(&edit.kind)).context(3)),
        };
        column().grow(1.0).min_height(0.0).child(view)
    };
    column()
        .grow(1.0)
        .min_height(0.0)
        .child(column().grow(1.0).min_height(0.0).display(shown(move || !shows.get())).child(code))
        .child(show(move || shows.get(), compared))
}

/// The menu of a right click: Copy, Paste and Find run the editor's own commands, as its keys
/// do (the window's clipboard, its find bar), then Close; a binary file's, or one while the
/// diff shows, has Close alone.
fn menu(model: &Model, edit: &Rc<FileEdit>, id: u64) -> impl Fn() -> Vec<MenuItem> + 'static {
    let (model, edit) = (model.clone(), edit.clone());
    move || {
        if !model.prefs().context_menu.get_untracked() {
            return Vec::new();
        }
        let closes = model.clone();
        let close = MenuItem::new("Close").keys("Ctrl+W").danger().on_select(move || {
            _ = closes.close_space(id);
        });
        if matches!(edit.content, Content::Binary(_)) || edit.diff.get_untracked() {
            return vec![close];
        }
        let at = edit.widget.borrow().clone();
        let command = |label: &str, keys: &str, command: EditorCommand| {
            let at = at.clone();
            MenuItem::new(label).keys(keys).on_select(move || {
                if let Some((handle, w)) = at.clone() {
                    handle.with(move |tree| {
                        tree.focus(w);
                        tree.dispatch(&command);
                    });
                }
            })
        };
        vec![
            command("Copy", "Ctrl+C", EditorCommand::Copy),
            command("Paste", "Ctrl+V", EditorCommand::Paste),
            command("Find", "Ctrl+F", EditorCommand::Find),
            MenuItem::separator(),
            close,
        ]
    }
}

/// What stands for a binary file.
fn binary(size: u64) -> quadra::Div {
    let said = format!("Binary file, {}: not shown, and not edited here.", crate::panes::size(size));
    column()
        .grow(1.0)
        .align_items(Align::Center)
        .justify(quadra::Justify::Center)
        .text_color(token::TEXT_3)
        .child(text(said))
}

/// Closing with changes: Save (then it closes), Don't save, or Cancel.
fn ask(model: &Model, edit: &Rc<FileEdit>) -> impl View + use<> {
    let said = format!("{} has changes that are not on the server.", edit.name);
    let (drops, saves, watched, (model, keeps)) =
        (edit.clone(), edit.clone(), edit.clone(), (model.clone(), model.clone()));
    // Given up without an answer that goes on (Cancel, the cross, Escape): the closing stops.
    effect(move || {
        if !watched.asking.get() && watched.then.borrow().is_some() {
            keeps.keep_edits();
        }
    });
    dialog(edit.asking, "Save changes?", move || text(said.clone()))
        .action("Cancel", Variant::Secondary, || {})
        .action("Don't save", Variant::Danger, move || drops.let_go())
        .action("Save", Variant::Primary, move || {
            // The closing waits for the save, not for the dialog.
            let (after, then) = (saves.clone(), saves.then.take());
            saves.save(&model, move || {
                *after.then.borrow_mut() = then;
                after.let_go();
            });
        })
}
