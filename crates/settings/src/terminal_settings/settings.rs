//! The live `TerminalSettings` model — global shell config + rendering options.
//!
//! The config is loaded from `terminal.json` (see [`crate::terminal_config`]) at
//! init. `TerminalSettingsPanel` updates the shell kind → notify.
//!
//! The reverse mapping (settings → config) and persistence live in
//! [`super::persist`]; config → settings in [`super::apply`].

use gpui::{App, AppContext, Entity, FontWeight, Global, Hsla, SharedString};
use oneterm_core::LocalShellConfig;

use crate::terminal_config::{SemanticHighlightingMode, TabTitleMode, TerminalConfig};

/// Live mirror of the `completion` config group (docs/auto-completion/06).
///
/// A plain resolved copy the `terminal-view` layer projects into the engine's
/// `CompletionParams` (keeping the engine free of a `settings` dependency).
#[derive(Debug, Clone, PartialEq)]
pub struct CompletionSettings {
    pub enabled: bool,
    pub accept_tab: bool,
    pub max_history: usize,
    pub min_prefix_len: usize,
    pub max_visible_items: usize,
    pub source_memory: bool,
    pub source_manual: bool,
    pub source_external: bool,
    pub fuzzy: bool,
    pub inherit_ancestor_options: bool,
    pub disable_in_alt_screen: bool,
    pub require_prompt_region: bool,
    pub windows_allow_coreutils: bool,
    pub force_family: Option<String>,
    pub redact_sensitive: bool,
}

impl Default for CompletionSettings {
    fn default() -> Self {
        // Mirrors CompletionConfig::default().
        Self {
            enabled: true,
            accept_tab: true,
            max_history: 500,
            min_prefix_len: 1,
            max_visible_items: 8,
            source_memory: true,
            source_manual: true,
            source_external: true,
            fuzzy: true,
            inherit_ancestor_options: true,
            disable_in_alt_screen: true,
            require_prompt_region: true,
            windows_allow_coreutils: false,
            force_family: None,
            redact_sensitive: true,
        }
    }
}

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
#[derive(Debug, Clone, Copy, Default)]
pub struct TerminalPadding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// Color overrides — if `Some`, override the theme; `None` = use the theme.
#[derive(Debug, Clone, Default)]
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
    pub ansi: Vec<Hsla>,
}

/// Global terminal config (shell + rendering options).
/// Loaded from `terminal.json` at init.
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
    pub completion: CompletionSettings,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            shell: LocalShellConfig::default(),
            font_family: None,
            font_size: None,
            base_font_size: None,
            font_weight: FontWeight::default(),
            font_features: Vec::new(),
            cursor_shape: TerminalCursorShape::Block,
            cursor_blink: TerminalBlink::On,
            cursor_color: None,
            line_height_factor: 1.2,
            cell_width: None,
            padding: TerminalPadding::default(),
            show_gutter: false,
            semantic_highlighting: SemanticHighlightingMode::Auto,
            tab_title_mode: TabTitleMode::Default,
            show_context_menu: true,
            scroll_multiplier: 1.0,
            alternate_scroll: true,
            scrollback_history: 10_000,
            bell_enabled: true,
            allow_clipboard_read: false,
            color_overrides: ColorOverrides::default(),
            completion: CompletionSettings::default(),
        }
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

    /// Initialize the global — load `terminal.json` and apply it (called from `ui::init`).
    pub fn init(cx: &mut App) {
        let cfg = TerminalConfig::load();
        let entity = cx.new(|_| {
            let mut settings = Self::default();
            settings.apply_config(&cfg);
            settings
        });
        cx.set_global(TerminalSettingsGlobal(entity));
    }
}
