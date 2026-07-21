use std::rc::Rc;

use gpui::{
    App, AppContext as _, ClickEvent, Context, Entity, Focusable as _, ParentElement as _,
    Styled as _, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogButtonProps, DialogFooter},
    input::{Input, InputState},
    notification::NotificationType,
};

use oneterm_state::{AgentRegistry, notif_ext::notify};

use super::TerminalPanel;

impl TerminalPanel {
    /// Return the effective tab label, with a manual override taking priority
    /// over the live OSC 0/2 title and the fallback shell label.
    pub(super) fn effective_tab_label(&self, live_title: Option<&str>) -> String {
        if let Some(title) = &self.tab_title_override {
            return title.clone();
        }
        super::resolve_tab_label(live_title, &self.tab_title)
    }

    /// Update the manual tab-title override and mirror the change to the agent
    /// registry so tab groups refresh immediately.
    pub(super) fn set_custom_tab_title(&mut self, title: String, cx: &mut Context<Self>) {
        self.tab_title_override = Some(title.clone());
        let tab_key = cx.entity_id();
        if let Some(registry) = AgentRegistry::try_global(cx) {
            registry.update(cx, |reg, cx| {
                reg.rename_tab_title(tab_key, title.clone(), cx)
            });
        }
        cx.notify();
    }
}

/// Open the rename-tab dialog for a terminal panel.
pub(crate) fn open_tab_title_dialog(
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
    let title_ok = title_state.clone();
    let panel_ok = panel.clone();

    let save_logic: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
        let title_ok = title_ok.clone();
        let panel_ok = panel_ok.clone();
        move |_, window, cx| {
            let new_title = title_ok.read(cx).value().trim().to_string();
            if new_title.is_empty() {
                window.push_notification(
                    notify(NotificationType::Warning, "Tab title cannot be empty.", cx),
                    cx,
                );
                return false;
            }
            panel_ok.update(cx, |panel, cx| {
                panel.set_custom_tab_title(new_title.clone(), cx)
            });
            true
        }
    });

    window.open_dialog(cx, move |dialog, window, cx| {
        let save_for_click = save_logic.clone();
        let save_for_kb = save_logic.clone();
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
            .footer({
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
                    ))
            })
            .button_props(
                DialogButtonProps::default()
                    .on_cancel(|_, _, _| true)
                    .on_ok(move |_, window, cx| save_for_kb(&ClickEvent::default(), window, cx)),
            )
    });
}
