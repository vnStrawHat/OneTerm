//! App menu bar — dựng các menu native (OneTerm / Edit / Window / Help).
//!
//! Mirror `reference/.../story/src/app_menus.rs`, giữ Appearance (Light/Dark),
//! Theme submenu, Language, Edit, Window, Help.

use gpui::{App, Entity, Menu, MenuItem, SharedString};
use gpui_component::{ActiveTheme, GlobalState, Theme, ThemeRegistry, menu::AppMenuBar};

use crate::actions::{About, Quit, SelectLocale, SwitchTheme, SwitchThemeMode};

/// Khởi tạo `AppMenuBar` + wire observe theme để refresh check-state.
pub fn init(title: impl Into<SharedString>, cx: &mut App) -> Entity<AppMenuBar> {
    let app_menu_bar = AppMenuBar::new(cx);
    let title: SharedString = title.into();
    update_app_menu(title.clone(), app_menu_bar.clone(), cx);

    cx.on_action({
        let title = title.clone();
        let app_menu_bar = app_menu_bar.clone();
        move |_: &SelectLocale, cx: &mut App| {
            // rust-i18n chưa wire ở skeleton — chỉ refresh menu.
            update_app_menu(title.clone(), app_menu_bar.clone(), cx);
        }
    });

    // Observe theme changes để refresh check-state Light/Dark + Theme.
    cx.observe_global::<Theme>({
        let title = title.clone();
        let app_menu_bar = app_menu_bar.clone();
        move |cx| {
            update_app_menu(title.clone(), app_menu_bar.clone(), cx);
        }
    })
    .detach();

    app_menu_bar
}

fn update_app_menu(title: impl Into<SharedString>, app_menu_bar: Entity<AppMenuBar>, cx: &mut App) {
    let title: SharedString = title.into();

    cx.set_menus(build_menus(title.clone(), cx));
    let menus = build_menus(title, cx)
        .into_iter()
        .map(|menu| menu.owned())
        .collect();
    GlobalState::global_mut(cx).set_app_menus(menus);

    app_menu_bar.update(cx, |menu_bar, cx| {
        menu_bar.reload(cx);
    });
}

fn build_menus(title: impl Into<SharedString>, cx: &App) -> Vec<Menu> {
    vec![
        Menu {
            name: title.into(),
            items: vec![
                MenuItem::action("About", About),
                MenuItem::Separator,
                MenuItem::Submenu(Menu {
                    name: "Appearance".into(),
                    items: vec![
                        MenuItem::action(
                            "Light",
                            SwitchThemeMode(gpui_component::ThemeMode::Light),
                        )
                        .checked(!cx.theme().mode.is_dark()),
                        MenuItem::action("Dark", SwitchThemeMode(gpui_component::ThemeMode::Dark))
                            .checked(cx.theme().mode.is_dark()),
                    ],
                    disabled: false,
                }),
                theme_menu(cx),
                language_menu(),
                MenuItem::Separator,
                MenuItem::action("Quit", Quit),
            ],
            disabled: false,
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::action("Undo", gpui_component::input::Undo),
                MenuItem::action("Redo", gpui_component::input::Redo),
                MenuItem::separator(),
                MenuItem::action("Cut", gpui_component::input::Cut),
                MenuItem::action("Copy", gpui_component::input::Copy),
                MenuItem::action("Paste", gpui_component::input::Paste),
                MenuItem::separator(),
                MenuItem::action("Select All", gpui_component::input::SelectAll),
            ],
            disabled: false,
        },
        Menu {
            name: "Help".into(),
            items: vec![MenuItem::action("About OneTerm", About)],
            disabled: false,
        },
    ]
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
