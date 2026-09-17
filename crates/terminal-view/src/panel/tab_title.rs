//! Everything about a Terminal Tab's label: resolving it from the live OSC 0/2
//! title, the manual rename override and its dialog, and the tab-strip element
//! (recording dot, active bar, drag source, middle-click / × close,
//! double-click rename).

use std::path::Path;
use std::rc::Rc;

use gpui::{
    App, AppContext as _, ClickEvent, Context, Div, Entity, EntityId, Focusable as _,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::{DialogButtonProps, DialogFooter},
    input::{Input, InputState},
    menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem},
    notification::NotificationType,
};
use oneterm_core::ShellKind;
use oneterm_terminal::TerminalLogState;
use oneterm_theme::notif_ext::notify;

use super::TerminalPanel;
use crate::space::DragTerminalTab;
use crate::theme::channel_chip;

impl TerminalPanel {
    /// Return the effective tab label, with a manual override taking priority
    /// over the live OSC 0/2 title and the fallback shell label.
    pub(super) fn effective_tab_label(&self, live_title: Option<&str>) -> String {
        if let Some(title) = &self.tab_title_override {
            return title.clone();
        }
        resolve_tab_label(live_title, &self.tab_title)
    }

    /// Update the manual tab-title override and mirror the change to the agent
    /// registry so tab groups refresh immediately.
    fn set_custom_tab_title(&mut self, title: String, cx: &mut Context<Self>) {
        self.tab_title_override = Some(title.clone());
        let tab_key = cx.entity_id();
        if let Some(registry) = self.deps.agent_registry.clone() {
            registry.update(cx, |reg, cx| {
                reg.rename_tab_title(tab_key, title.clone(), cx)
            });
        }
        cx.notify();
    }

    /// Whether the tab shows the recording dot: only an unsplit tab whose
    /// terminal is currently logging (a split tab has no room to say which
    /// Space is recording).
    fn shows_recording_dot(&self, cx: &App) -> bool {
        self.tree.is_single()
            && self.tree.active_terminal().is_some_and(|view| {
                view.read(cx)
                    .session
                    .read(cx)
                    .capabilities()
                    .logging
                    .is_some_and(|logging| {
                        matches!(logging.state(), TerminalLogState::Running { .. })
                    })
            })
    }
}

/// Resolve the tab label from the live OSC 0/2 title and the static fallback.
fn resolve_tab_label(live: Option<&str>, fallback: &str) -> String {
    match live.filter(|s| !s.is_empty()).map(trim_path_title) {
        Some(t) => t.to_string(),
        None => fallback.to_string(),
    }
}

/// The static tab label of a local-shell tab: the program's file stem when the
/// settings name one, otherwise the shell kind's display name (`US-0114`). A
/// live OSC 0/2 title still wins over this — see [`resolve_tab_label`].
///
/// An explicit `program` wins for **every** kind, not only `Custom`, because
/// `resolve_shell` honours it for every kind: `kind: cmd` with
/// `program: nu.exe` runs nushell, and labelling that tab "Command Prompt"
/// would be the same untruth `F1` is about, in a rarer configuration
/// (`F-114.2`). The "+" menu clears `program` for an explicit kind, so only the
/// default-shell path can reach this.
pub(super) fn shell_tab_title(kind: ShellKind, program: Option<&Path>) -> String {
    if let Some(stem) = program.and_then(|p| p.file_stem()).and_then(|s| s.to_str()) {
        return stem.to_string();
    }
    if kind == ShellKind::Custom {
        // A custom shell with no program has nothing to be named after.
        return super::terminal_panel::DEFAULT_TAB_TITLE.to_string();
    }
    kind.display_name().to_string()
}

/// Shorten a title that is just an absolute path to its last path component.
pub(crate) fn trim_path_title(title: &str) -> &str {
    let t = title.trim();
    let bytes = t.as_bytes();
    let is_abs = t.starts_with('/')
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/'));
    if !is_abs {
        return title;
    }
    match t.rsplit(|c| c == '\\' || c == '/').next() {
        Some(last) if !last.is_empty() => last,
        _ => title,
    }
}

fn tab_title_label() -> Div {
    // GPUI creates a rectangular content mask when either overflow axis is
    // hidden, which clips glyph descenders to this label's line box.
    div().flex_1().min_w_0().text_ellipsis().whitespace_nowrap()
}

/// Build the tab-strip row for `panel`: active bar, drag source, middle-click
/// close, recording dot, the (double-click to rename) label, and the × button.
pub(super) fn render_tab_strip(
    panel: &mut TerminalPanel,
    _window: &mut Window,
    cx: &mut Context<TerminalPanel>,
) -> impl IntoElement {
    let tab_panel = panel.tab_panel.clone();
    let panel_entity = cx.entity().clone();
    let panel_weak = cx.entity().downgrade();
    let muted = cx.theme().muted_foreground;
    let highlight = cx.theme().table_active_border;
    let recording_color = cx.theme().danger;
    let is_active = panel.is_active;
    let show_recording = panel.shows_recording_dot(cx);
    // One chip per distinct channel of this tab's Spaces, in A..E order.
    let chips: Vec<_> = panel
        .tab_channels(cx)
        .into_iter()
        .enumerate()
        .map(|(index, channel)| channel_chip(channel, ("tab-channel", index), cx))
        .collect();
    let tab_label = panel.tab_label(cx);
    let drag_title: SharedString = tab_label.clone().into();
    let rename_title = tab_label.clone();
    let menu_title = tab_label.clone();
    let menu_panel = panel_entity.clone();
    let title_label_id = SharedString::from(format!("tab-title-label-{panel_entity:?}"));
    // Per-panel, like the label's id below it: the context menu derives its own
    // element id from this one, and a shared id would give every tab in the
    // strip the same menu state.
    let title_row_id = SharedString::from(format!("tab-title-{panel_entity:?}"));

    gpui_component::h_flex()
        .id(title_row_id)
        .relative()
        .h_full()
        // No `w_full()`: the kit's `Tab` is content-sized and `flex_shrink_0`
        // (`component/src/tab/tab.rs:715-718,794-800`), so a percentage width
        // against it contributes nothing to intrinsic sizing and every tab
        // collapsed to `min_w` whatever its label said — which is how the
        // leftmost tab ended up clipped to a bare `×` with free space beside
        // the strip (`US-0116`, `F11`). Sized by the label between a floor and
        // a ceiling instead: no tab loses its name, and one long OSC title
        // elides rather than eating the strip.
        .min_w(px(100.))
        .max_w(px(220.))
        .items_center()
        .gap_1()
        .when(is_active, |this| {
            this.child(
                div()
                    .absolute()
                    .top_0()
                    .left(-px(20.))
                    .right(-px(20.))
                    .h(px(2.))
                    .bg(highlight),
            )
        })
        .mr(-px(5.))
        // Drag the tab into an empty Space with the terminal-specific
        // payload consumed by Space-tree drop targets.
        .when_some(tab_panel.clone(), |this, _| {
            this.on_drag(
                DragTerminalTab {
                    panel: panel_weak.clone(),
                    title: drag_title.clone(),
                },
                |drag, _pos, _win, cx| {
                    cx.stop_propagation();
                    cx.new(|_| drag.clone())
                },
            )
        })
        // Middle-click on a tab → close that tab.
        .on_mouse_down(MouseButton::Middle, {
            let panel = panel_entity.clone();
            move |_, window, cx| {
                cx.stop_propagation();
                panel.update(cx, |panel, cx| panel.close_tab(window, cx));
            }
        })
        .children(chips)
        .when(show_recording, |this| {
            this.child(
                div()
                    .id("tab-recording")
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(recording_color)
                    .child("●"),
            )
        })
        .child(
            tab_title_label()
                .id(title_label_id)
                .on_click({
                    let panel = panel_entity.clone();
                    move |event, window, cx| {
                        if event.click_count() != 2 {
                            return;
                        }
                        cx.stop_propagation();
                        open_tab_title_dialog(panel.clone(), rename_title.clone(), window, cx);
                    }
                })
                .child(tab_label),
        )
        .when_some(tab_panel, |this, _| {
            this.child(
                div()
                    .id("tab-close")
                    .flex_shrink_0()
                    .cursor_pointer()
                    .size_4()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(3.))
                    .hover(move |this| this.bg(muted.opacity(0.15)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        panel_entity.update(cx, |panel, cx| panel.close_tab(window, cx));
                    })
                    .child(Icon::new(IconName::Close).xsmall().text_color(muted)),
            )
        })
        // The kit attaches no right-click handler to a tab and stops only left
        // mouse-down (`base/src/tabs.rs:172`), so the tab's own menu lives here,
        // on the content OneTerm renders (`US-0116`, `F12`).
        .context_menu(move |menu, _window, cx| {
            tab_context_menu(menu, menu_panel.clone(), menu_title.clone(), cx)
        })
}

/// Which sibling tabs a bulk close acts on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum CloseScope {
    /// Every tab but the one the menu was opened on.
    Others,
    /// Every tab after the one the menu was opened on.
    ToTheRight,
}

/// The tabs a bulk close removes, as positions in the tab list, in the order
/// they must be closed: right to left, so removing one never shifts a position
/// still to come.
pub(super) fn tabs_to_close(total: usize, target: usize, scope: CloseScope) -> Vec<usize> {
    if target >= total {
        return Vec::new();
    }
    let mut victims: Vec<usize> = match scope {
        CloseScope::Others => (0..total).filter(|ix| *ix != target).collect(),
        CloseScope::ToTheRight => (target + 1..total).collect(),
    };
    victims.reverse();
    victims
}

/// The terminal tabs of `panel`'s tab group, each with its index in the group,
/// plus `panel`'s own position in that list.
///
/// The group can in principle hold panels of other kinds; those are dropped, so
/// a position in the returned list is not always the group index — which is why
/// both are carried.
fn sibling_tabs(
    panel: &Entity<TerminalPanel>,
    cx: &App,
) -> Option<(Vec<(usize, Entity<TerminalPanel>)>, usize)> {
    tabs_of(&panel.read(cx), panel.entity_id(), cx)
}

/// Same, for a caller that already holds the panel borrowed — reading it back
/// through its entity there is a double lease and panics.
fn tabs_of(
    panel: &TerminalPanel,
    panel_id: EntityId,
    cx: &App,
) -> Option<(Vec<(usize, Entity<TerminalPanel>)>, usize)> {
    let group = panel.tab_panel.as_ref()?.upgrade()?;
    let tabs: Vec<(usize, Entity<TerminalPanel>)> = group
        .read(cx)
        .panels()
        .iter()
        .enumerate()
        .filter_map(|(ix, p)| Some((ix, p.view().downcast::<TerminalPanel>().ok()?)))
        .collect();
    let position = tabs
        .iter()
        .position(|(_, tab)| tab.entity_id() == panel_id)?;
    Some((tabs, position))
}

/// Close every tab in `victims`, after a confirmation when more than one **tab**
/// would go — tabs, not live shells: a tab whose shell already exited counts the
/// same, which is what the dialog's own wording says. The `×`, middle-click and
/// Close all destroy exactly one tab and are unconfirmed, but a single menu row
/// that ends nine of them is the case the application confirms elsewhere too
/// (`CORR-34`).
fn close_tabs(victims: Vec<Entity<TerminalPanel>>, window: &mut Window, cx: &mut App) {
    fn close_all(victims: &[Entity<TerminalPanel>], window: &mut Window, cx: &mut App) {
        for tab in victims {
            tab.update(cx, |tab, cx| tab.close_tab(window, cx));
        }
    }

    match victims.len() {
        0 => {}
        1 => close_all(&victims, window, cx),
        n => {
            let description = format!("Close {n} terminal tabs? Their sessions end.");
            window.open_alert_dialog(cx, move |alert, _, _| {
                let victims = victims.clone();
                alert
                    .confirm()
                    .title("Close Tabs")
                    .description(description.clone())
                    .button_props(
                        DialogButtonProps::default()
                            .ok_text("Close")
                            .ok_variant(ButtonVariant::Danger)
                            .cancel_text("Cancel")
                            .show_cancel(true),
                    )
                    .on_ok(move |_, window, cx| {
                        close_all(&victims, window, cx);
                        true
                    })
            });
        }
    }
}

/// A bulk-close row, disabled when it would close nothing.
fn close_scope_row(
    label: &'static str,
    panel: Entity<TerminalPanel>,
    scope: CloseScope,
    cx: &App,
) -> PopupMenuItem {
    let victims: Vec<Entity<TerminalPanel>> = sibling_tabs(&panel, cx)
        .map(|(tabs, position)| {
            tabs_to_close(tabs.len(), position, scope)
                .into_iter()
                .map(|ix| tabs[ix].1.clone())
                .collect()
        })
        .unwrap_or_default();
    let empty = victims.is_empty();
    PopupMenuItem::new(label)
        .disabled(empty)
        .on_click(move |_, window, cx| close_tabs(victims.clone(), window, cx))
}

/// The tab's right-click menu (`US-0116`). Every row acts on `panel` — the tab
/// that was right-clicked — and not on whichever tab happens to be active.
fn tab_context_menu(
    menu: PopupMenu,
    panel: Entity<TerminalPanel>,
    label: String,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    menu.item(PopupMenuItem::new("Rename...").on_click({
        let panel = panel.clone();
        move |_, window, cx| open_tab_title_dialog(panel.clone(), label.clone(), window, cx)
    }))
    .item(PopupMenuItem::new("Duplicate").on_click({
        let panel = panel.clone();
        move |_, window, cx| {
            panel.update(cx, |panel, cx| {
                let space = panel.tree.active();
                panel.duplicate_session(space, window, cx);
            });
        }
    }))
    .separator()
    .item(PopupMenuItem::new("Close").on_click({
        let panel = panel.clone();
        move |_, window, cx| {
            panel.update(cx, |panel, cx| panel.close_tab(window, cx));
        }
    }))
    .item(close_scope_row(
        "Close Others",
        panel.clone(),
        CloseScope::Others,
        cx,
    ))
    .item(close_scope_row(
        "Close to the Right",
        panel,
        CloseScope::ToTheRight,
        cx,
    ))
}

/// The tab list the `...` menu opens with: one row per terminal tab, the active
/// one marked, clicking a row shows that tab (`US-0116`, `F11`/`F13`).
///
/// It lives in the `...` menu because that is the one menu a panel can add rows
/// to, above the kit's own separator (`component/src/dock/tab_panel.rs:333-337`),
/// and because the menu
/// otherwise held a single row duplicating the zoom button beside it.
///
/// `panel` is the group's active panel and is already borrowed by the caller
/// (`Panel::dropdown_menu` takes `&mut self`), so its own row must not read it
/// back through its entity — that is a double lease and panics.
pub(super) fn tab_list_menu(
    menu: PopupMenu,
    panel: &TerminalPanel,
    cx: &mut Context<TerminalPanel>,
) -> PopupMenu {
    let panel_id = cx.entity_id();
    let Some((tabs, _)) = tabs_of(panel, panel_id, cx) else {
        return menu;
    };
    let Some(group) = panel.tab_panel.as_ref().and_then(|g| g.upgrade()) else {
        return menu;
    };
    let active = group.read(cx).active_ix();
    let mut menu = menu.label("Terminal Tabs");
    for (ix, tab) in tabs {
        let label = if tab.entity_id() == panel_id {
            panel.tab_label(cx)
        } else {
            tab.read(cx).tab_label(cx)
        };
        let group = group.downgrade();
        menu = menu.item(PopupMenuItem::new(label).checked(ix == active).on_click(
            move |_, window, cx| {
                let _ = group.update(cx, |group, cx| group.select_tab(ix, window, cx));
            },
        ));
    }
    menu.separator()
}

/// Open the rename-tab dialog for a terminal panel. An empty title keeps the
/// dialog open with a warning; Cancel never saves.
fn open_tab_title_dialog(
    panel: Entity<TerminalPanel>,
    current_title: String,
    window: &mut Window,
    cx: &mut App,
) {
    let title_state = cx.new(|cx| {
        let mut st = InputState::new(window, cx).placeholder("Tab title");
        st.set_value(current_title.clone(), window, cx);
        st
    });

    // Shared by the Save button and the dialog's keyboard OK; returns whether
    // the dialog may close.
    let save: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
        let title_state = title_state.clone();
        move |_, window, cx| {
            let new_title = title_state.read(cx).value().trim().to_string();
            if new_title.is_empty() {
                window.push_notification(
                    notify(NotificationType::Warning, "Tab title cannot be empty.", cx),
                    cx,
                );
                return false;
            }
            panel.update(cx, |panel, cx| {
                panel.set_custom_tab_title(new_title.clone(), cx)
            });
            true
        }
    });

    window.open_dialog(cx, move |dialog, window, cx| {
        let save_for_click = save.clone();
        let save_for_kb = save.clone();
        let focus_handle = title_state.read(cx).focus_handle(cx);
        focus_handle.focus(window, cx);

        dialog
            .title("Rename Tab")
            .w(px(440.))
            .content({
                let title_state = title_state.clone();
                move |content, _window, cx| {
                    content.child(
                        div()
                            .gap_1()
                            .w_full()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().foreground)
                                    .child("Tab title"),
                            )
                            .child(Input::new(&title_state)),
                    )
                }
            })
            .footer(
                DialogFooter::new()
                    .child(Button::new("cancel").label("Cancel").outline().on_click(
                        |_, window, cx| {
                            window.close_dialog(cx);
                        },
                    ))
                    .child(Button::new("save").label("Save").primary().on_click(
                        move |_, window, cx| {
                            if save_for_click(&ClickEvent::default(), window, cx) {
                                window.close_dialog(cx);
                            }
                        },
                    )),
            )
            .button_props(
                DialogButtonProps::default()
                    .on_cancel(|_, _, _| true)
                    .on_ok(move |_, window, cx| save_for_kb(&ClickEvent::default(), window, cx)),
            )
    });
}

#[cfg(test)]
mod tests {
    use gpui::Styled as _;

    use std::path::Path;

    use oneterm_core::ShellKind;

    use super::{
        CloseScope, resolve_tab_label, shell_tab_title, tab_title_label, tabs_to_close,
        trim_path_title,
    };

    #[test]
    fn close_others_keeps_the_right_clicked_tab_and_closes_right_to_left() {
        // Right-clicked the third of five. Descending order matters: closing
        // left to right would shift every position still to come.
        assert_eq!(
            tabs_to_close(5, 2, CloseScope::Others),
            vec![4, 3, 1, 0],
            "must never contain the target, and must descend"
        );
        // A lone tab has no others.
        assert_eq!(tabs_to_close(1, 0, CloseScope::Others), Vec::<usize>::new());
    }

    #[test]
    fn close_to_the_right_stops_at_the_right_clicked_tab() {
        assert_eq!(tabs_to_close(5, 2, CloseScope::ToTheRight), vec![4, 3]);
        // The last tab has nothing to its right, so the row closes nothing.
        assert_eq!(
            tabs_to_close(5, 4, CloseScope::ToTheRight),
            Vec::<usize>::new()
        );
        assert_eq!(tabs_to_close(3, 0, CloseScope::ToTheRight), vec![2, 1]);
    }

    #[test]
    fn a_target_outside_the_list_closes_nothing() {
        // A tab removed between the menu opening and the row being clicked.
        assert_eq!(tabs_to_close(3, 3, CloseScope::Others), Vec::<usize>::new());
        assert_eq!(tabs_to_close(0, 0, CloseScope::Others), Vec::<usize>::new());
        assert_eq!(
            tabs_to_close(3, 9, CloseScope::ToTheRight),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn each_shell_kind_gets_its_own_tab_label() {
        let kinds = [
            ShellKind::Cmd,
            ShellKind::PowerShell,
            ShellKind::Pwsh,
            ShellKind::Bash,
            ShellKind::Zsh,
            ShellKind::Sh,
        ];
        let labels: Vec<String> = kinds.iter().map(|k| shell_tab_title(*k, None)).collect();

        assert_eq!(labels[0], "Command Prompt");
        assert_eq!(labels[1], "PowerShell");
        assert_eq!(labels[2], "PowerShell 7");
        // F1 is precisely that ten local tabs read alike: no two kinds may
        // share a label.
        let mut unique = labels.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(
            unique.len(),
            labels.len(),
            "duplicate shell labels: {labels:?}"
        );
        assert!(!labels.iter().any(|l| l == "Terminal"));
    }

    #[test]
    fn custom_shell_is_named_after_its_program() {
        assert_eq!(
            shell_tab_title(
                ShellKind::Custom,
                Some(Path::new("C:\\Program Files\\Git\\bin\\bash.exe"))
            ),
            "bash"
        );
        assert_eq!(
            shell_tab_title(ShellKind::Custom, Some(Path::new("/usr/bin/fish"))),
            "fish"
        );
        // Nothing to name it after — the reset-tab fallback.
        assert_eq!(shell_tab_title(ShellKind::Custom, None), "Terminal");
    }

    #[test]
    fn an_explicit_program_wins_over_the_kinds_name() {
        // `resolve_shell` honours `program` for every kind, so `kind: cmd` with
        // `program: nu.exe` runs nushell — and must not read "Command Prompt".
        assert_eq!(
            shell_tab_title(ShellKind::Cmd, Some(Path::new("C:\\tools\\nu.exe"))),
            "nu"
        );
        assert_eq!(
            shell_tab_title(ShellKind::Bash, Some(Path::new("/usr/bin/fish"))),
            "fish"
        );
        // ...and with no program the kind still names the tab.
        assert_eq!(shell_tab_title(ShellKind::Cmd, None), "Command Prompt");
    }

    #[test]
    fn a_live_osc_title_still_wins_over_the_shell_name() {
        let fallback = shell_tab_title(ShellKind::PowerShell, None);

        assert_eq!(
            resolve_tab_label(Some("vim - main.rs"), &fallback),
            "vim - main.rs"
        );
        // ...and with no live title the shell name is what shows.
        assert_eq!(resolve_tab_label(None, &fallback), "PowerShell");
        assert_eq!(resolve_tab_label(Some(""), &fallback), "PowerShell");
    }

    #[test]
    fn tab_title_label_does_not_create_a_content_mask() {
        let mut label = tab_title_label();
        let overflow = &label.style().overflow;

        assert_eq!(overflow.x, None);
        assert_eq!(overflow.y, None);
    }

    #[test]
    fn live_title_is_used() {
        assert_eq!(
            resolve_tab_label(Some("vim — main.rs"), "Terminal"),
            "vim — main.rs"
        );
        assert_eq!(
            resolve_tab_label(Some("user@host: ~/repo"), "user@host:24"),
            "user@host: ~/repo"
        );
        assert_eq!(resolve_tab_label(Some("cmd.exe"), "Terminal"), "cmd.exe");
    }

    #[test]
    fn none_falls_back_to_static_label() {
        assert_eq!(resolve_tab_label(None, "Terminal"), "Terminal");
        assert_eq!(resolve_tab_label(None, "prod-server"), "prod-server");
    }

    #[test]
    fn empty_title_falls_back_to_static_label() {
        assert_eq!(resolve_tab_label(Some(""), "Terminal"), "Terminal");
    }

    #[test]
    fn fallback_is_returned_by_value() {
        let label = resolve_tab_label(None, "Terminal");
        assert_eq!(label, "Terminal");
    }

    #[test]
    fn windows_drive_path_shortened_to_basename() {
        assert_eq!(
            resolve_tab_label(Some("C:\\Windows\\system32\\cmd.exe"), "Terminal"),
            "cmd.exe"
        );
        assert_eq!(
            resolve_tab_label(
                Some("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"),
                "Terminal"
            ),
            "powershell.exe"
        );
    }

    #[test]
    fn posix_path_shortened_to_basename() {
        assert_eq!(resolve_tab_label(Some("/usr/bin/bash"), "Terminal"), "bash");
        assert_eq!(resolve_tab_label(Some("/bin/sh"), "Terminal"), "sh");
    }

    #[test]
    fn relative_or_descriptive_titles_not_trimmed() {
        assert_eq!(resolve_tab_label(Some("~/repo"), "Terminal"), "~/repo");
        assert_eq!(
            resolve_tab_label(Some("user@host: ~/repo"), "Terminal"),
            "user@host: ~/repo"
        );
        assert_eq!(
            resolve_tab_label(Some("vim — main.rs"), "Terminal"),
            "vim — main.rs"
        );
    }

    #[test]
    fn trim_path_title_helper_directly() {
        assert_eq!(trim_path_title("C:\\Windows\\system32\\cmd.exe"), "cmd.exe");
        assert_eq!(trim_path_title("/usr/bin/bash"), "bash");
        assert_eq!(trim_path_title("cmd.exe"), "cmd.exe");
        assert_eq!(trim_path_title("user@host: ~/repo"), "user@host: ~/repo");
        assert_eq!(trim_path_title("  /usr/bin/zsh  "), "zsh");
    }

    #[test]
    fn phase1_terminal_titles_are_sanitized_by_policy() {
        use oneterm_terminal::security_policy::TerminalSecurityPolicy;

        let policy = TerminalSecurityPolicy::default();

        // Control characters stripped.
        let controlled = "safe\u{0007}\u{001b}[31m\u{202e}txt.exe";
        let sanitized = policy.sanitize_title(controlled).unwrap();
        assert_eq!(sanitized, "safe[31mtxt.exe");

        // Oversized title truncated.
        let oversized = "x".repeat(256 * 1024);
        let sanitized = policy.sanitize_title(&oversized).unwrap();
        assert!(sanitized.len() <= 4 * 1024);

        // resolve_tab_label still passes through what it receives.
        let clean = "vim — main.rs";
        assert_eq!(resolve_tab_label(Some(clean), "Terminal"), clean);
    }
}
