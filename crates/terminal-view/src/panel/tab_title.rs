//! Everything about a Terminal Tab's label: resolving it from the live OSC 0/2
//! title, the manual rename override and its dialog, and the tab-strip element
//! (recording dot, active bar, drag source, middle-click / × close,
//! double-click rename).

use std::rc::Rc;

use gpui::{
    App, AppContext as _, ClickEvent, Context, Div, Entity, Focusable as _,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogButtonProps, DialogFooter},
    input::{Input, InputState},
    notification::NotificationType,
};
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

/// Shorten a title that is just an absolute path to its last path component.
fn trim_path_title(title: &str) -> &str {
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
    let title_label_id = SharedString::from(format!("tab-title-label-{panel_entity:?}"));

    gpui_component::h_flex()
        .id("tab-title")
        .relative()
        .h_full()
        .w_full()
        .min_w(px(100.))
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

    use super::{resolve_tab_label, tab_title_label, trim_path_title};

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
