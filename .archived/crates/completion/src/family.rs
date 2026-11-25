//! Shell-family mapping and catalog-category search paths.
//!
//! The engine keys everything off a single value: the [`ShellFamily`]. It maps
//! from [`oneterm_core::ShellKind`] and drives (a) the option trigger characters
//! and (b) the ordered set of [`CatalogCategory`]s searched high → low
//! precedence. See docs/auto-completion/03-shell-detection.md §2.

use oneterm_core::ShellKind;

/// The catalog categories, each backed by an `external` and/or `manual` folder
/// (docs/auto-completion/02 §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CatalogCategory {
    Cmd,
    Coreutils,
    PowerShell,
    Windows,
    Linux,
    Common,
}

impl CatalogCategory {
    /// Parse a folder-derived category string back into a [`CatalogCategory`].
    pub(crate) fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "cmd" => CatalogCategory::Cmd,
            "coreutils" => CatalogCategory::Coreutils,
            "powershell" => CatalogCategory::PowerShell,
            "windows" => CatalogCategory::Windows,
            "linux" => CatalogCategory::Linux,
            "common" => CatalogCategory::Common,
            _ => return None,
        })
    }
}

/// The running shell's completion-relevant family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShellFamily {
    Cmd,
    PowerShell,
    Unix,
}

impl ShellFamily {
    /// Collapse a [`ShellKind`] into a completion family. `Custom` is a
    /// conservative `Unix` default (refined by the caller via heuristics /
    /// `force_family` — docs/auto-completion/03 §5).
    pub fn from_kind(kind: ShellKind) -> Self {
        match kind {
            ShellKind::Cmd => ShellFamily::Cmd,
            ShellKind::PowerShell | ShellKind::Pwsh => ShellFamily::PowerShell,
            ShellKind::Bash | ShellKind::Zsh | ShellKind::Sh => ShellFamily::Unix,
            ShellKind::Custom => ShellFamily::Unix,
        }
    }

    /// Parse a `force_family` config string (`"cmd"`/`"powershell"`/`"unix"`).
    pub fn from_config_str(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "cmd" => ShellFamily::Cmd,
            "powershell" | "pwsh" => ShellFamily::PowerShell,
            "unix" | "bash" | "zsh" | "sh" => ShellFamily::Unix,
            _ => return None,
        })
    }

    /// Catalog categories searched, high → low precedence (docs 02 §4.1).
    /// `allow_coreutils` appends `Coreutils`/`Linux` for the Windows families.
    pub(crate) fn categories(self, allow_coreutils: bool) -> Vec<CatalogCategory> {
        use CatalogCategory::*;
        let mut c = match self {
            ShellFamily::Cmd => vec![Cmd, Windows, Common],
            ShellFamily::PowerShell => vec![PowerShell, Cmd, Windows, Common],
            ShellFamily::Unix => vec![Coreutils, Linux, Common],
        };
        if allow_coreutils && matches!(self, ShellFamily::Cmd | ShellFamily::PowerShell) {
            c.extend([Coreutils, Linux]);
        }
        c
    }

    /// Characters that start an option token for this family
    /// (docs 03 §2; Q2 — `Cmd` accepts both `/` and `-`).
    pub(crate) fn option_triggers(self) -> &'static [char] {
        match self {
            ShellFamily::Cmd => &['/', '-'],
            ShellFamily::PowerShell => &['-'],
            ShellFamily::Unix => &['-'],
        }
    }

    /// Whether command/subcommand/option matching is case-insensitive for this
    /// family. `Cmd`/`PowerShell` are case-insensitive; `Unix` is case-sensitive
    /// (POSIX commands are case-sensitive — docs 04 §4.1).
    pub(crate) fn case_insensitive(self) -> bool {
        matches!(self, ShellFamily::Cmd | ShellFamily::PowerShell)
    }

    /// A token starts an option in this family if its first char is a trigger.
    pub(crate) fn is_option_token(self, token: &str) -> bool {
        token
            .chars()
            .next()
            .is_some_and(|c| self.option_triggers().contains(&c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_maps_to_family() {
        assert_eq!(ShellFamily::from_kind(ShellKind::Cmd), ShellFamily::Cmd);
        assert_eq!(
            ShellFamily::from_kind(ShellKind::PowerShell),
            ShellFamily::PowerShell
        );
        assert_eq!(
            ShellFamily::from_kind(ShellKind::Pwsh),
            ShellFamily::PowerShell
        );
        assert_eq!(ShellFamily::from_kind(ShellKind::Bash), ShellFamily::Unix);
        assert_eq!(ShellFamily::from_kind(ShellKind::Zsh), ShellFamily::Unix);
        assert_eq!(ShellFamily::from_kind(ShellKind::Sh), ShellFamily::Unix);
        assert_eq!(ShellFamily::from_kind(ShellKind::Custom), ShellFamily::Unix);
    }

    #[test]
    fn category_search_paths() {
        use CatalogCategory::*;
        assert_eq!(
            ShellFamily::Cmd.categories(false),
            vec![Cmd, Windows, Common]
        );
        assert_eq!(
            ShellFamily::PowerShell.categories(false),
            vec![PowerShell, Cmd, Windows, Common]
        );
        assert_eq!(
            ShellFamily::Unix.categories(false),
            vec![Coreutils, Linux, Common]
        );
    }

    #[test]
    fn allow_coreutils_appends_for_windows_families_only() {
        use CatalogCategory::*;
        assert_eq!(
            ShellFamily::Cmd.categories(true),
            vec![Cmd, Windows, Common, Coreutils, Linux]
        );
        // Unix is unaffected by the flag.
        assert_eq!(
            ShellFamily::Unix.categories(true),
            vec![Coreutils, Linux, Common]
        );
    }

    #[test]
    fn option_triggers_per_family() {
        assert_eq!(ShellFamily::Cmd.option_triggers(), &['/', '-']);
        assert_eq!(ShellFamily::PowerShell.option_triggers(), &['-']);
        assert_eq!(ShellFamily::Unix.option_triggers(), &['-']);
    }

    #[test]
    fn force_family_parsing() {
        assert_eq!(ShellFamily::from_config_str("cmd"), Some(ShellFamily::Cmd));
        assert_eq!(
            ShellFamily::from_config_str("powershell"),
            Some(ShellFamily::PowerShell)
        );
        assert_eq!(
            ShellFamily::from_config_str("unix"),
            Some(ShellFamily::Unix)
        );
        assert_eq!(ShellFamily::from_config_str("nope"), None);
    }
}
