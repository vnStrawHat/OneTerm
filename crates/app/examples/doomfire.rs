//! OneTerm in a real window whose one space runs the fire demo through the pseudo console
//! (Quadra's specs/14, "Terminal throughput"); it ends when the demo has written its summary.
//!
//! Quadra's bench runs it (`tools/doomfire.sh [seconds]` in Quadra's repository).
//!
//! `DOOMFIRE` names the demo's program, `DOOMFIRE_OUT` the file of its summary, and
//! `DOOMFIRE_SECONDS` how long it burns. Settings and the log go to `.tmp/oneterm-bench`.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use oneterm::{Launch, NewSession, OneTerm, Shell, keymap, start, theme};
use quadra::{AppDirs, WindowOptions};

fn main() -> quadra::Result<()> {
    let var = |name: &str| std::env::var(name).unwrap_or_default();
    let (program, out, seconds) =
        (PathBuf::from(var("DOOMFIRE")), PathBuf::from(var("DOOMFIRE_OUT")), var("DOOMFIRE_SECONDS"));
    let _ = std::fs::remove_file(&out);
    // The summary is the end: then the app goes too.
    let watched = out.clone();
    std::thread::spawn(move || {
        while !watched.is_file() {
            std::thread::sleep(Duration::from_millis(100));
        }
        std::thread::sleep(Duration::from_millis(200));
        std::process::exit(0);
    });
    let mut app = quadra::app()?.dirs(AppDirs::under(std::path::Path::new(".tmp/oneterm-bench/defaults")));
    let chosen = app.settings().value("appearance.theme", "Dark".to_string());
    let named = app.settings().value("appearance.colour_theme", oneterm::KIT_THEME.to_string());
    let first = quadra_design::system_theme(&app.appearance());
    app = quadra_design::install(app, &first)?.theme_by(move |seen| theme(&chosen.get(), &named.get(), seen));
    let args = vec!["--seconds".into(), seconds, "--out".into(), out.display().to_string()];
    let fire = Shell { name: "doomfire", program, args };
    let shells: NewSession =
        Rc::new(move |launch: Launch| start(Launch { shell: Some(fire.clone()), ..launch }));
    let options = WindowOptions::new("OneTerm").size(1440.0, 900.0).centered().custom_frame();
    app.keymap(keymap()).window(options, move || OneTerm::new(shells.clone()).view())?.run()
}
