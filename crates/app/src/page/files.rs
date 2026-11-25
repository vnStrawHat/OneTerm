//! The settings' files (specs/oneterm, "Settings"): the settings file and its folder on the
//! Advanced page, and the logging of spaces, a group of the Terminal page.

use quadra::{row, text};
use quadra_design::{SettingsGroup, Tone, Variant, badge, button, input, segmented, switch, token};

use super::Back;
use crate::prefs::Prefs;

/// The settings file, where this run saves it (a trial run's folder too), and its folder
/// shown in the file manager.
pub(super) fn files() -> SettingsGroup {
    let dirs = quadra::use_context::<std::rc::Rc<quadra::AppDirs>>().map(|it| (*it).clone());
    let file = dirs.or_else(|| quadra::AppDirs::for_app("OneTerm")).map(|it| it.config.join("settings.json"));
    let said = file.as_ref().map(|it| it.display().to_string()).unwrap_or_default();
    let opener = quadra::use_context::<std::rc::Rc<dyn quadra_shell::Opener>>();
    let show = button("Show in folder").variant(Variant::Secondary).on_click(move || {
        let (Some(opener), Some(file)) = (&opener, &file) else { return };
        // Nothing is written while every setting is its default: then the folder, made.
        match (file.exists(), file.parent()) {
            (false, Some(folder)) => {
                let _ = std::fs::create_dir_all(folder);
                opener.open(&folder.display().to_string());
            }
            _ => _ = opener.reveal(file),
        }
    });
    SettingsGroup::new("Files")
        .about("Saved as you change them; the file can be edited by hand while OneTerm is closed.")
        .row("Settings file", &said, show)
}

/// How what the app fetches reaches the network: the proxy, and whether a server's
/// certificate is checked, with a warning while it is not.
pub(super) fn network(p: Prefs, d: Prefs) -> SettingsGroup {
    let (n, was) = (p.net, d.net);
    let address = input(n.proxy_address)
        .placeholder("proxy.example:8080")
        .label("Proxy address")
        .disabled(move || n.proxy.get() == 0);
    let unsafe_now = badge("Not checked", Tone::Danger)
        .display(move || if n.verify_tls.get() { quadra::Display::None } else { quadra::Display::Flex });
    let verify = row()
        .align_items(quadra::Align::Center)
        .gap(token::SPACE_MD)
        .child(unsafe_now)
        .child(switch(n.verify_tls));
    SettingsGroup::new("Network")
        .about("For what OneTerm fetches over HTTPS: its updates. SSH connections do not go through it.")
        .row("Proxy", "System is the proxy Windows is set to use.", segmented(crate::prefs::PROXIES, n.proxy))
        .back(n.proxy, was.proxy)
        .row("Proxy address", "The HTTP proxy asked when Proxy is Manual, as host:port.", address)
        .back(n.proxy_address, was.proxy_address)
        .row(
            "Verify certificates",
            "Turn off only on a network that opens TLS on the way: what is fetched is then not known to be GitHub's.",
            verify,
        )
        .back(n.verify_tls, was.verify_tls)
}

/// Which spaces are logged from their start, where, and how a file there is written.
pub(super) fn logging(p: Prefs, d: Prefs) -> SettingsGroup {
    SettingsGroup::new("Logging")
        .row("Log local shells", "Write everything a new local shell shows to a file.", switch(p.log_local))
        .back(p.log_local, d.log_local)
        .row("Log SSH spaces", "The same for each new SSH space.", switch(p.log_ssh))
        .back(p.log_ssh, d.log_ssh)
        .row(
            "Folder",
            "Where logs are saved. Leave empty for session-logs in OneTerm's data folder.",
            folder_field(p.log_folder),
        )
        .back(p.log_folder, d.log_folder)
        .row("File name", "%n is the shell's or the host's name.", text("%n_%Y-%m-%d_%H-%M-%S.log"))
        .row("Line format", "%msg is one line of what the space showed.", text("[%Y-%m-%d %H:%M:%S] %msg"))
        .row(
            "An existing file",
            "Add to a log that is already there, or replace it.",
            segmented(crate::logs::MODES, p.log_mode),
        )
        .back(p.log_mode, d.log_mode)
}

/// A folder typed, or chosen in the system's dialog.
fn folder_field(folder: quadra::Signal<String>) -> quadra::Div {
    let dialogs = quadra::use_context::<std::rc::Rc<dyn quadra_shell::Dialogs>>();
    let choose = button("Choose\u{2026}").variant(Variant::Secondary).on_click(move || {
        let asked = quadra_shell::FileDialog::new("The folder of logs");
        if let Some(chosen) = dialogs.as_ref().and_then(|it| it.pick_folder(&asked)) {
            folder.set(chosen.display().to_string());
        }
    });
    row().gap(token::SPACE_MD).child(input(folder).placeholder("session-logs").label("Folder")).child(choose)
}
