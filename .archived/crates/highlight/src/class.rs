//! The closed `Class` enum — one `u8` per cell, indexed into a flat theme array.

/// Semantic class of a terminal cell/token.
///
/// A closed enum with `#[repr(u8)]` so each cell carries one byte. Themes
/// pre-resolve a `[Style; Class::COUNT]` flat array → render-time lookup is a
/// single branchless array index.
///
/// Variants `19..=31` are reserved for future user-defined categories (see
/// §Q6 of the design doc). [`Class::COUNT`] is fixed at 32 so the flat array
/// stays O(1) even if custom classes are added later.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Class {
    /// No semantic class — ANSI/SGR colors only.
    #[default]
    Default = 0,
    // ── line roles ──
    /// The prompt sign glyph (`$`, `#`, `>`, `❯`, `➜`, `λ`, powerline `U+E0B0`).
    PromptSign = 1,
    /// The command word (first token after the prompt sign).
    Command = 2,
    /// A command-line option (`--flag`, `-x`, `/flag`).
    Option = 3,
    // ── semantic log tokens ──
    /// Error/fail/denied/refused/… keyword.
    Error = 4,
    /// Ok/success/passed/valid/… keyword.
    Success = 5,
    /// Warning/closed/exited/terminated/… keyword.
    Warn = 6,
    /// Info/login/access/connection/… keyword.
    Info = 7,
    /// Debug/trace keyword, or `ls -l` file-type char.
    Debug = 8,
    // ── structural ──
    /// A filesystem path (`/usr/bin`, `C:\Users\…`).
    Path = 9,
    /// An IPv4 or IPv6 address.
    Ip = 10,
    /// A MAC address (`aa:bb:cc:dd:ee:ff`).
    Mac = 11,
    /// A date/time token (`2026-06-23`, `14:30`, `Jan`, `Mon`).
    DateTime = 12,
    /// A numeric literal (`0x1F`, `42`, `1.5e3`).
    Number = 13,
    /// A quoted string (`"…"` / `'…'`).
    String = 14,
    /// An operator character (`= ; | ? * $ < > & + - :`).
    Operator = 15,
    /// A bracket character (`( ) [ ] { }`).
    Bracket = 16,
    /// A URL (subsumes the existing `url_mask`).
    Url = 17,
    /// Permission bits (`rwx` from `ls -l`) — the entire 10-char block.
    Permission = 18,
    // ── permission sub-classes (per-char coloring) ──
    /// File-type char of `ls -l` (`d`, `l`, `b`, `c`, `p`, `s`, `-`).
    PermType = 19,
    /// Read bit (`r`).
    PermRead = 20,
    /// Write bit (`w`).
    PermWrite = 21,
    /// Execute bit (`x`).
    PermExec = 22,
    /// Special bits (`s`, `S`, `t`, `T`).
    PermSpecial = 23,
    /// No-permission dash (`-`).
    PermNone = 24,
    // 25..=31 reserved for future user-defined categories (Class::COUNT = 32).
}

impl Class {
    /// Total number of class slots (including reserved). Fixed so the theme
    /// flat array stays O(1) regardless of future additions.
    pub const COUNT: usize = 32;

    /// Convert a raw `u8` to a `Class`, clamping unknown values to `Default`.
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Default,
            1 => Self::PromptSign,
            2 => Self::Command,
            3 => Self::Option,
            4 => Self::Error,
            5 => Self::Success,
            6 => Self::Warn,
            7 => Self::Info,
            8 => Self::Debug,
            9 => Self::Path,
            10 => Self::Ip,
            11 => Self::Mac,
            12 => Self::DateTime,
            13 => Self::Number,
            14 => Self::String,
            15 => Self::Operator,
            16 => Self::Bracket,
            17 => Self::Url,
            18 => Self::Permission,
            19 => Self::PermType,
            20 => Self::PermRead,
            21 => Self::PermWrite,
            22 => Self::PermExec,
            23 => Self::PermSpecial,
            24 => Self::PermNone,
            _ => Self::Default,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_is_32() {
        assert_eq!(Class::COUNT, 32);
    }

    #[test]
    fn default_is_zero() {
        assert_eq!(Class::Default as u8, 0);
    }

    #[test]
    fn from_u8_roundtrip() {
        for v in 0..=24 {
            let c = Class::from_u8(v);
            assert_eq!(c as u8, v);
        }
    }

    #[test]
    fn from_u8_unknown_clamps_to_default() {
        assert_eq!(Class::from_u8(25), Class::Default);
        assert_eq!(Class::from_u8(255), Class::Default);
    }
}
