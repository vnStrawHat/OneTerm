//! App menu bar — builds the native menus (OneTerm / Edit / View / Help).
//!
//! Mirrors `reference/.../story/src/app_menus.rs`, keeping Appearance (Light/Dark),
//! Theme submenu, Language, Edit, View (Font Size + Gutter), and Help.

use gpui::{App, Entity, Menu, MenuItem, SharedString, px};
use gpui_component::{ActiveTheme as _, GlobalState, Theme, ThemeRegistry, menu::AppMenuBar};

use crate::actions::{
    About, Quit, SelectFont, SelectLocale, SwitchTheme, SwitchThemeMode, ToggleGutter,
};
use crate::state::TerminalSettings;

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

    // Font Size — set the global theme font size (View ▸ Font Size).
    cx.on_action(|select: &SelectFont, cx| {
        Theme::global_mut(cx).font_size = px(select.0 as f32);
        cx.refresh_windows();
    });

    // Gutter — toggle the timestamp + line number column (View ▸ Gutter).
    cx.on_action(|_: &ToggleGutter, cx| {
        TerminalSettings::global(cx).update(cx, |st, cx| {
            st.show_gutter = !st.show_gutter;
            cx.notify();
        });
        cx.refresh_windows();
    });
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
            name: "View".into(),
            items: view_menu_items(cx),
            disabled: false,
        },
        Menu {
            name: "Help".into(),
            items: vec![MenuItem::action("About OneTerm", About)],
            disabled: false,
        },
    ]
}

/// Build the items for the "View" menu: Font Size submenu + Gutter toggle.
fn view_menu_items(cx: &App) -> Vec<MenuItem> {
    let font_size = cx.theme().font_size.as_f32() as i32;
    vec![
        MenuItem::Submenu(Menu {
            name: "Font Size".into(),
            items: vec![
                MenuItem::action("Large", SelectFont(18)).checked(font_size == 18),
                MenuItem::action("Medium (default)", SelectFont(16)).checked(font_size == 16),
                MenuItem::action("Small", SelectFont(14)).checked(font_size == 14),
            ],
            disabled: false,
        }),
        MenuItem::Separator,
        // Gutter — toggle the timestamp + line number column on the left of the terminal.
        MenuItem::action("Gutter", ToggleGutter)
            .checked(TerminalSettings::global(cx).read(cx).show_gutter),
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
