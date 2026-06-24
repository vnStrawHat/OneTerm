//! Layout persistence — load / save dock state.

use anyhow::{Context as _, Result};
use gpui::{Context, Edges, Entity, PromptLevel, Window};
use gpui_component::dock::{DockArea, DockAreaState};

use super::{MAIN_DOCK_VERSION, STATE_FILE};

impl super::MyTermWorkspace {
    /// Load layout từ file — dùng để giữ right dock + settings.
    pub(crate) fn load_layout(
        dock_area: Entity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let json = std::fs::read_to_string(STATE_FILE)?;
        let state = serde_json::from_str::<DockAreaState>(&json)?;

        if state.version != Some(MAIN_DOCK_VERSION) {
            let answer = window.prompt(
                PromptLevel::Info,
                "The default main layout has been updated.\n\
                Do you want to reset the layout to default?",
                None,
                &["Yes", "No"],
                cx,
            );

            let weak_dock_area = dock_area.downgrade();
            cx.spawn_in(window, async move |this, window| {
                if answer.await == Ok(0) {
                    _ = this.update_in(window, |_, window, cx| {
                        super::layout::reset_default_layout(weak_dock_area, window, cx);
                    });
                }
            })
            .detach();
        }

        dock_area.update(cx, |dock_area, cx| {
            dock_area.load(state, window, cx).context("load layout")?;
            dock_area.set_dock_collapsible(
                Edges {
                    right: true,
                    ..Default::default()
                },
                window,
                cx,
            );
            Ok::<(), anyhow::Error>(())
        })
    }
}

/// Save dock state ra file.
pub(crate) fn save_state(state: &DockAreaState) -> Result<()> {
    tracing::info!("Save layout...");
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(STATE_FILE, json)?;
    Ok(())
}
