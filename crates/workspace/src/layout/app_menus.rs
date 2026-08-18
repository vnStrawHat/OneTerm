//! App menu bar — builds the native menu (OneTerm).
//!
//! Mirrors `reference/.../story/src/app_menus.rs`, keeping Appearance (Light/Dark),
//! Theme submenu, and Language. The Edit / View / Help menus were removed; their
//! actions remain reachable via key bindings and the in-app UI.

use gpui::{App, Entity, Menu, MenuItem, OwnedMenu, SharedString, px};
use gpui_component::{ActiveTheme as _, GlobalState, Theme, ThemeRegistry, menu::AppMenuBar};

use oneterm_actions::{
    About, OpenSettings, Quit, SelectFont, SelectLocale, SwitchTheme, SwitchThemeMode, ToggleGutter,
};
use oneterm_settings::TerminalSettings;

/// Initialize the `AppMenuBar` and wire up theme observation to refresh check states.
pub fn init(title: impl Into<SharedString>, cx: &mut App) -> Entity<AppMenuBar> {
    let app_menu_bar = AppMenuBar::new(cx);
    let title: SharedString = title.into();
    update_app_menu(title.clone(), app_menu_bar.clone(), cx);

    cx.on_action({
        let title = title.clone();
        let app_menu_bar = app_menu_bar.clone();
        move |_: &SelectLocale, cx: &mut App| {
            // rust-i18n is not wired up in the skeleton yet — just refresh the menu.
            update_app_menu(title.clone(), app_menu_bar.clone(), cx);
        }
    });

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

    // Font Size — set the global theme font size (kept for key-binding reachability).
    cx.on_action(|select: &SelectFont, cx| {
        Theme::global_mut(cx).font_size = px(select.0 as f32);
        cx.refresh_windows();
    });

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
            language_menu(),
            MenuItem::Separator,
            MenuItem::action("Settings...", OpenSettings),
            MenuItem::Separator,
            MenuItem::action("Quit", Quit),
        ],
        disabled: false,
    }]
}

fn language_menu() -> MenuItem {
    MenuItem::Submenu(Menu {
        name: "Language".into(),
        items: vec![
            MenuItem::action("English", SelectLocale("en".into())),
            MenuItem::action("Tiếng Việt", SelectLocale("vi".into())),
        ],
        disabled: false,
    })
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
