//! App menu bar — builds the native menu (OneTerm).
//!
//! Mirrors `reference/.../story/src/app_menus.rs`, keeping Appearance (Light/Dark)
//! and the Theme submenu, plus the checked Show FPS Monitor item (`US-0151`). The Edit / View / Help menus were removed; their
//! actions remain reachable via key bindings and the in-app UI.

use gpui::{App, Entity, Menu, MenuItem, OwnedMenu, SharedString};
use gpui_component::{ActiveTheme as _, GlobalState, Theme, ThemeRegistry, menu::AppMenuBar};

use oneterm_actions::{
    About, OpenSettings, Quit, SwitchTheme, SwitchThemeMode, ToggleFpsMonitor, ToggleGutter,
};
use oneterm_settings::{TerminalSettings, UiConfig};

/// Initialize the `AppMenuBar` and wire up theme observation to refresh check states.
pub fn init(title: impl Into<SharedString>, cx: &mut App) -> Entity<AppMenuBar> {
    let app_menu_bar = AppMenuBar::new(cx);
    let title: SharedString = title.into();
    update_app_menu(title.clone(), app_menu_bar.clone(), cx);

    // Observe theme changes to refresh the Light/Dark + Theme + Font Size check states.
    cx.observe_global::<Theme>({
        let title = title.clone();
        let app_menu_bar = app_menu_bar.clone();
        move |cx| {
            update_app_menu(title.clone(), app_menu_bar.clone(), cx);
        }
    })
    .detach();

    // Observe terminal settings to refresh the Gutter check state.
    cx.observe(&TerminalSettings::global(cx), {
        let title = title.clone();
        let app_menu_bar = app_menu_bar.clone();
        move |_, cx| {
            update_app_menu(title.clone(), app_menu_bar.clone(), cx);
        }
    })
    .detach();

    // Observe the UI config to refresh the Show FPS Monitor check state.
    cx.observe(&UiConfig::global(cx), {
        let title = title.clone();
        let app_menu_bar = app_menu_bar.clone();
        move |_, cx| {
            update_app_menu(title.clone(), app_menu_bar.clone(), cx);
        }
    })
    .detach();
    cx.on_action(toggle_fps_monitor);

    // Gutter — toggle the timestamp + line number column (kept for key-binding reachability).
    cx.on_action(|_: &ToggleGutter, cx| {
        let new_val = !TerminalSettings::global(cx).read(cx).show_gutter;
        TerminalSettings::global(cx).update(cx, |st, cx| {
            st.show_gutter = new_val;
            cx.notify();
        });
        // Persist a snapshot off the UI thread so the preference survives restarts.
        TerminalSettings::persist_global(cx);
        cx.refresh_windows();
    });
    app_menu_bar
}

/// Flip the FPS HUD (`US-0151`).
fn toggle_fps_monitor(_: &ToggleFpsMonitor, cx: &mut App) {
    let show = !UiConfig::global(cx).read(cx).show_fps;
    UiConfig::set_show_fps(show, cx);
}

fn update_app_menu(title: impl Into<SharedString>, app_menu_bar: Entity<AppMenuBar>, cx: &mut App) {
    let title: SharedString = title.into();

    // Build the tree once; the platform menu and the in-window menu bar both
    // need a copy (`Menu` is not `Clone`, `Menu::owned` consumes it).
    let menus = build_menus(title, cx);
    let owned: Vec<OwnedMenu> = menus.iter().map(clone_menu).map(Menu::owned).collect();
    cx.set_menus(menus);
    GlobalState::global_mut(cx).set_app_menus(owned);

    app_menu_bar.update(cx, |menu_bar, cx| {
        menu_bar.reload(cx);
    });
}

/// Deep-copy a menu tree (actions via `Action::boxed_clone`).
fn clone_menu(menu: &Menu) -> Menu {
    Menu {
        name: menu.name.clone(),
        items: menu.items.iter().map(clone_menu_item).collect(),
        disabled: menu.disabled,
    }
}

fn clone_menu_item(item: &MenuItem) -> MenuItem {
    match item {
        MenuItem::Separator => MenuItem::Separator,
        MenuItem::Submenu(menu) => MenuItem::Submenu(clone_menu(menu)),
        MenuItem::SystemMenu(os_menu) => MenuItem::SystemMenu(gpui::OsMenu {
            name: os_menu.name.clone(),
            menu_type: os_menu.menu_type,
        }),
        MenuItem::Action {
            name,
            action,
            os_action,
            checked,
            disabled,
        } => MenuItem::Action {
            name: name.clone(),
            action: action.boxed_clone(),
            os_action: *os_action,
            checked: *checked,
            disabled: *disabled,
        },
    }
}

fn build_menus(title: impl Into<SharedString>, cx: &App) -> Vec<Menu> {
    vec![Menu {
        name: title.into(),
        items: vec![
            MenuItem::action("About", About),
            MenuItem::Separator,
            MenuItem::Submenu(Menu {
                name: "Appearance".into(),
                items: vec![
                    MenuItem::action("Light", SwitchThemeMode(gpui_component::ThemeMode::Light))
                        .checked(!cx.theme().mode.is_dark()),
                    MenuItem::action("Dark", SwitchThemeMode(gpui_component::ThemeMode::Dark))
                        .checked(cx.theme().mode.is_dark()),
                ],
                disabled: false,
            }),
            theme_menu(cx),
            MenuItem::action("Show FPS Monitor", ToggleFpsMonitor)
                .checked(UiConfig::global(cx).read(cx).show_fps),
            MenuItem::Separator,
            MenuItem::action("Settings...", OpenSettings),
            MenuItem::Separator,
            MenuItem::action("Quit", Quit),
        ],
        disabled: false,
    }]
}

fn theme_menu(cx: &App) -> MenuItem {
    let themes = ThemeRegistry::global(cx).sorted_themes();
    let current_name = cx.theme().theme_name();
    MenuItem::Submenu(Menu {
        name: "Theme".into(),
        items: themes
            .iter()
            .map(|theme| {
                let checked = current_name == &theme.name;
                MenuItem::action(theme.name.clone(), SwitchTheme(theme.name.clone()))
                    .checked(checked)
            })
            .collect(),
        disabled: false,
    })
}

#[cfg(test)]
mod tests {
    use gpui::AppContext as _;
    use oneterm_settings::ui_config::UiConfigGlobal;

    use super::*;

    #[gpui::test]
    fn the_toggle_action_flips_show_fps(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            // `persist_blocked` keeps the test from writing a real `ui_config.json`.
            let config = cx.new(|_| UiConfig {
                persist_blocked: true,
                ..UiConfig::default()
            });
            cx.set_global(UiConfigGlobal(config.clone()));
            cx.on_action(toggle_fps_monitor);

            assert!(!config.read(cx).show_fps, "off by default");
            cx.dispatch_action(&ToggleFpsMonitor);
            assert!(config.read(cx).show_fps);
            cx.dispatch_action(&ToggleFpsMonitor);
            assert!(!config.read(cx).show_fps);
        });
    }
}
