//! OneTerm in a real window:
//!
//!     scripts/cargo.sh run -p oneterm-app
//!
//! With `ONETERM_HOME=.tmp/oneterm` its settings and log go there instead of the user's folders.

#![windows_subsystem = "windows"]

use std::rc::Rc;

use oneterm::{NewSession, OneTerm, keymap, start, theme};
use quadra::{AppDirs, WindowOptions};

fn main() -> quadra::Result<()> {
    // After an update: what the version before left moved aside goes (Quadra's specs/22).
    quadra_updater::started();
    let mut app = quadra::app()?;
    // ONETERM_HOME puts the settings and the log in one folder (a trial run); else the user's.
    let home = std::env::var_os("ONETERM_HOME").map(|home| AppDirs::under(std::path::Path::new(&home)));
    if home.is_none() {
        let roots = ["APPDATA", "LOCALAPPDATA"].map(|it| std::env::var_os(it).unwrap_or_default().into());
        oneterm::adopt(
            &roots.into_iter().filter(|it: &std::path::PathBuf| it.is_absolute()).collect::<Vec<_>>(),
        );
    }
    if let Some(dirs) = home.or_else(|| AppDirs::for_app("OneTerm")) {
        // A crash that is no panic is reported beside the panics' reports.
        quadra_shell::crash_reports(&dirs.data.join("crashes"));
        app = app.dirs(dirs);
    }
    let crashed = app.last_crash();
    // The theme the settings name, the system's by default.
    let chosen = app.settings().value("appearance.theme", "System".to_string());
    let named = app.settings().value("appearance.colour_theme", oneterm::KIT_THEME.to_string());
    let first = quadra_design::system_theme(&app.appearance());
    let app =
        quadra_design::install(app, &first)?.theme_by(move |seen| theme(&chosen.get(), &named.get(), seen));
    // The system's dialogs and what opens files, for SFTP's Edit.
    let app = quadra_shell::install(app);
    let options = WindowOptions::new("OneTerm")
        .size(1440.0, 900.0)
        .fit(0.85)
        .min_size(720.0, 480.0)
        .centered()
        .custom_frame()
        .remember("main");
    let shells: NewSession = Rc::new(start);
    app.keymap(keymap())
        .window(options, move || {
            let term = OneTerm::new(shells);
            // The user's own run takes over from OneTerm 0.6 once and asks for a newer version;
            // a trial run takes nothing and asks nothing.
            if std::env::var_os("ONETERM_HOME").is_none() {
                term.model.take_over_once();
                term.model.check_for_updates_at_start();
            }
            if let Some(report) = crashed.clone() {
                term.crashed(&report);
            }
            term.view()
        })?
        .run()
}
