//! [`BreadcrumbIndicator`] — displays the breadcrumb (cwd path + foreground
//! process) of the active terminal session in the StatusBar.
//!
//! Like `DateTimeClock` / `NetSpeedIndicator`: `Entity` + `Render` + `Focusable`,
//! updated via a 500ms timer. The timer spawns on the window context
//! (`cx.spawn_in`) to fire reliably.
//!
//! Each tick:
//! 1. Find the active terminal panel in the DockArea (via `collect_tab_panels`).
//! 2. Downcast `AnyView` → `Entity<TerminalPanel>`.
//! 3. Read `breadcrumb_text()` + `foreground_process()` from the session.
//! 4. Format the label: `"<process> — <cwd>"` (or just the cwd when no process).
//!
//! Hidden when no active terminal panel has a breadcrumb (e.g. no cwd yet).

use gpui::prelude::FluentBuilder as _;
use std::time::Duration;

use gpui::{
    App, AppContext as _, ClickEvent, ClipboardItem, Context, Entity, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Task, WeakEntity, Window, div,
};
use gpui_component::{ActiveTheme as _, dock::DockArea, tooltip::Tooltip};

use crate::layout::workspace::zoom::collect_tab_panels;
use crate::views::terminal::panel::TerminalPanel;

/// Indicator showing the breadcrumb (cwd path + foreground process) of the
/// active terminal session in the StatusBar.
///
/// Refreshes every 500ms — the cwd (OSC 7) and foreground process update
/// asynchronously from the PTY listener.
pub struct BreadcrumbIndicator {
    focus_handle: FocusHandle,
    dock_area: WeakEntity<DockArea>,
    /// Current formatted label — `None` when no active terminal has a breadcrumb.
    label: Option<String>,
    _timer: Task<()>,
}

impl BreadcrumbIndicator {
    /// Create a new indicator and start the 500ms timer.
    pub fn new(
        dock_area: WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let timer = cx.spawn_in(window, async move |this, window| {
            loop {
                window
                    .background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                if let Some(this) = this.upgrade() {
                    let _ = this.update_in(window, |this, _window, cx| {
                        this.tick(cx);
                    });
                }
            }
        });
        Self {
            focus_handle,
            dock_area,
            label: None,
            _timer: timer,
        }
    }

    /// Helper to create an `Entity<Self>`.
    pub fn new_entity(
        dock_area: WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new(dock_area, window, cx))
    }

    /// Sample the breadcrumb from the active terminal panel.
    fn tick(&mut self, cx: &mut Context<Self>) {
        let dock_area = match self.dock_area.upgrade() {
            Some(da) => da,
            None => {
                if self.label.take().is_some() {
                    cx.notify();
                }
                return;
            }
        };

        let label = active_terminal_breadcrumb(&dock_area, cx);
        if label != self.label {
            self.label = label;
            cx.notify();
        }
    }
}

/// Find the active terminal panel in the DockArea and read its breadcrumb label.
///
/// Walk all TabPanels → find the active panel with `panel_name == "terminal"` →
/// downcast `AnyView` → `Entity<TerminalPanel>` → read `breadcrumb_text()` +
/// `foreground_process()` from the session.
fn active_terminal_breadcrumb(dock_area: &Entity<DockArea>, cx: &App) -> Option<String> {
    let tab_panels = collect_tab_panels(dock_area.read(cx), cx);
    for tp in tab_panels {
        if let Some(panel) = tp.read(cx).active_panel(cx) {
            if panel.panel_name(cx) == "terminal" {
                let any_view = panel.view();
                if let Ok(entity) = any_view.downcast::<TerminalPanel>() {
                    return entity.read(cx).breadcrumb_label(cx);
                }
            }
        }
    }
    None
}

impl Focusable for BreadcrumbIndicator {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BreadcrumbIndicator {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label = self.label.clone();

        div()
            .id("breadcrumb-indicator")
            .track_focus(&self.focus_handle)
            .text_color(cx.theme().muted_foreground)
            .when_some(label, |this, label| {
                this.child(label.clone())
                    .cursor_pointer()
                    .tooltip(move |window, cx| Tooltip::new("Click to copy").build(window, cx))
                    .on_click(move |_: &ClickEvent, _window, cx: &mut App| {
                        cx.write_to_clipboard(ClipboardItem::new_string(label.clone()));
                    })
            })
    }
}
