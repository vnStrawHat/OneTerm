//! What is open over the window, and the parts of the app that are neither a tab nor a
//! setting (specs/oneterm).

use quadra::{Signal, signal};

/// What is open over the window: the settings page (it takes the body), the quick connect
/// dialog, the dialog that adds a host.
#[derive(Clone, Copy)]
pub struct Ui {
    pub settings: Signal<bool>,
    pub connecting: Signal<bool>,
    pub adding: Signal<bool>,
    /// The hosts panel at the left while the tabs are in a strip.
    pub hosts: Signal<bool>,
    /// The settings' page shown.
    pub page: Signal<usize>,
    /// The saved host being edited, by name.
    pub editing: Signal<Option<String>>,
    /// The SFTP dock.
    pub dock: Signal<bool>,
    /// SFTP in the workspace's place.
    pub expanded: Signal<bool>,
    /// Connections waiting for the master password to open their saved secrets, and whether
    /// in a space here.
    pub unlocking: Signal<Option<(Vec<crate::shells::Remote>, bool)>>,
    /// A saved host's Connect dialog, and the space it is connecting.
    pub opening: crate::host_connect::Opening,
    /// Command completion: its settings and the app's one engine.
    pub complete: crate::complete::Completion,
    /// Whether a newer OneTerm is published, and installing it.
    pub updates: crate::updates::Updates,
}

impl Ui {
    /// Nothing open; in the scope the model is made in.
    pub(crate) fn new(net: crate::prefs::NetPrefs) -> Ui {
        Ui {
            settings: signal(false),
            connecting: signal(false),
            adding: signal(false),
            hosts: signal(false),
            page: signal(0),
            editing: signal(None),
            dock: signal(false),
            expanded: signal(false),
            unlocking: signal(None),
            opening: crate::host_connect::Opening::new(),
            complete: crate::complete::Completion::new(),
            updates: crate::updates::Updates::new(net),
        }
    }
}
