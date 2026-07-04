//! OneTerm's major screens.

pub mod session_tabs;
pub mod settings;
pub mod sftp;
pub mod terminal;

pub use session_tabs::SessionPanel;
pub use settings::SettingsPanel;
pub use sftp::SftpPanel;
pub use terminal::{TerminalPanel, TerminalSettingsPanel};
