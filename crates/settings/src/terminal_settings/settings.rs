//! The live `TerminalSettings` model — global shell config + rendering options.
//!
//! The config is loaded from `terminal.json` (see [`crate::terminal_config`]) at
//! init.
//!
//! The reverse mapping (settings → config) and persistence live in
//! [`super::persist`]; config → settings in [`super::apply`]. Defaults are
//! single-sourced in [`TerminalConfig::default`].

use gpui::{App, AppContext, Entity, FontWeight, Global, Hsla, SharedString};
use oneterm_core::LocalShellConfig;

use crate::terminal_config::{
    CompletionConfig, LoggingConfig, SemanticHighlightingMode, SftpConfig, SshSettingsConfig,
    TabTitleMode, TerminalConfig,
};

/// Terminal cursor shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalCursorShape {
    /// Filled block — default.
    #[default]
    Block,
    /// Narrow vertical bar `│`.
    Bar,
    /// Underline `_`.
    Underline,
}

impl TerminalCursorShape {
    /// Parse from a string ("block" | "bar" | "underline").
    pub(super) fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "bar" => Self::Bar,
            "underline" => Self::Underline,
            _ => Self::Block,
        }
    }
}

/// Cursor blink mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalBlink {
    /// Blink when focused (default).
    #[default]
    On,
    /// Do not blink.
    Off,
}

/// Padding on all 4 sides for the terminal content (px).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TerminalPadding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// Color overrides — if `Some`, override the theme; `None` = use the theme.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColorOverrides {
    pub foreground: Option<Hsla>,
    pub background: Option<Hsla>,
    pub cursor: Option<Hsla>,
    pub selection: Option<Hsla>,
    pub gutter_fg: Option<Hsla>,
    pub gutter_bg: Option<Hsla>,
    pub clock_fg: Option<Hsla>,
    pub line_number_fg: Option<Hsla>,
    pub min_contrast: f32,
    /// ANSI-16 overrides by slot; `None` keeps the theme colour for that slot.
    /// One invalid entry in `terminal.json` therefore never shifts the colours
    /// after it (CORR-60).
    pub ansi: Vec<Option<Hsla>>,
}

/// Global terminal config (shell + rendering options).
/// Loaded from `terminal.json` at init.
#[derive(Debug, Clone)]
pub struct TerminalSettings {
    // ── Shell ──
    pub shell: LocalShellConfig,

    // ── Font ──
    /// Primary font family (None = use the theme mono font).
    pub font_family: Option<SharedString>,
    /// Font size in px (None = use the theme mono font size).
    /// Can change at runtime via zoom shortcuts (Ctrl +/−/0).
    pub font_size: Option<f32>,
    /// Original font size from the config (terminal.json) — used for Ctrl+0 reset zoom.
    /// Not changed by zooming.
    pub base_font_size: Option<f32>,
    /// Font weight.
    pub font_weight: FontWeight,
    /// Font features (OpenType): e.g. ["calt", "liga"].
    pub font_features: Vec<SharedString>,

    // ── Cursor ──
    /// Cursor shape (Block/Bar/Underline).
    pub cursor_shape: TerminalCursorShape,
    /// Enable/disable cursor blinking.
    pub cursor_blink: TerminalBlink,
    /// Cursor color (None = theme caret).
    pub cursor_color: Option<Hsla>,

    // ── Layout ──
    /// Line height multiplier (1.2 = 120% of font size).
    pub line_height_factor: f32,
    /// Cell width override in px (None = auto from font advance).
    pub cell_width: Option<f32>,
    /// Padding around the terminal content.
    pub padding: TerminalPadding,
    /// Enable/disable the gutter (timestamp + line number on the left of the terminal).
    pub show_gutter: bool,
    /// Semantic highlighting mode for plain-text terminal output.
    pub semantic_highlighting: SemanticHighlightingMode,

    // ── Mouse ──
    /// Show OneTerm's context menu on right click.
    /// Disable this to let CLI apps receive right click directly.
    pub show_context_menu: bool,
    /// Copy the selection to the clipboard when the mouse button is released.
    pub copy_on_select: bool,

    // ── Tab title ──
    /// How the terminal tab title is determined: static label ("Terminal" /
    /// SSH session label) or the live OSC 0/2 title set by the shell.
    pub tab_title_mode: TabTitleMode,

    // ── Scroll ──
    /// Scroll multiplier for the mouse wheel.
    pub scroll_multiplier: f32,
    /// Alternate scroll mode (alt-screen → arrow keys).
    pub alternate_scroll: bool,
    /// Maximum number of scrollback history lines (default 10000).
    /// Total lines in the gutter = scrollback_history + viewport lines.
    pub scrollback_history: usize,

    // ── Bell ──
    /// Enable/disable the bell indicator.
    pub bell_enabled: bool,

    // ── Security ──
    /// Allow programs to read the system clipboard via OSC 52 (`52;c;?`).
    /// Default `false`: reading is refused because it exposes the local
    /// clipboard to programs, including remote ones over SSH.
    pub allow_clipboard_read: bool,

    // ── Colors ──
    /// Color overrides for the terminal theme.
    pub color_overrides: ColorOverrides,

    // ── Completion ──
    /// Live mirror of the `completion` config group.
    pub completion: CompletionConfig,

    /// Live mirror of the terminal printable-output logging config group.
    pub logging: LoggingConfig,

    /// Live mirror of the `sftp` config group (SFTP browser edit workflow).
    pub sftp: SftpConfig,

    /// SSH connection settings captured when a new session starts.
    pub ssh: SshSettingsConfig,

    /// `terminal.json` existed but could not be read at startup (e.g. permission
    /// denied), so these are built-in defaults and must not be written back
    /// over a possibly valid file (CORR-61). Never persisted.
    pub persist_blocked: bool,
}

impl Default for TerminalSettings {
    /// The live defaults are the `terminal.json` defaults — single-sourced in
    /// [`TerminalConfig::default`] (see [`TerminalSettings::from_config`]).
    fn default() -> Self {
        Self::from_config(&TerminalConfig::default())
    }
}

/// Global wrapper (same pattern as `AppStateGlobal`).
pub struct TerminalSettingsGlobal(pub Entity<TerminalSettings>);

impl Global for TerminalSettingsGlobal {}

impl TerminalSettings {
    /// The global `Entity<TerminalSettings>` (panics if not initialized).
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<TerminalSettingsGlobal>().0.clone()
    }

    /// Initialize the global — load `terminal.json` once (called from the
    /// composition root). Idempotent: later calls keep the loaded entity.
    pub fn init(cx: &mut App) {
        if cx.has_global::<TerminalSettingsGlobal>() {
            return;
        }
        let settings = match TerminalConfig::load() {
            Ok(config) => Self::from_config(&config),
            Err(error) => {
                log::error!("{error}; using defaults and refusing to overwrite the file");
                Self {
                    persist_blocked: true,
                    ..Self::default()
                }
            }
        };
        let entity = cx.new(|_| settings);
        cx.set_global(TerminalSettingsGlobal(entity));
    }
}
