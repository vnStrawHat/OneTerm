//! Theme mapping — flat `[ClassStyle; Class::COUNT]` array, pre-resolved at build.
//!
//! Render-time lookup = `styles.style(class)` — one array index, branchless,
//! no string-scope selector engine. See §7 of the design doc.

use crate::class::Class;
use crate::color::Hsla;

/// Font style flags (additive OR with the cell's existing flags).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FontStyle {
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
}

/// Cell decoration (additive on top of ANSI fg — never replaces color).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Decoration {
    /// No decoration.
    #[default]
    None,
    /// Underline.
    Underline,
}

/// One class's resolved style (foreground, background, font, decoration).
#[derive(Clone, Copy, Debug, Default)]
pub struct ClassStyle {
    /// Foreground color (applied when the cell has no explicit ANSI fg).
    pub fg: Option<Hsla>,
    /// Background color (line-level / per-cell, additive).
    pub bg: Option<Hsla>,
    /// Font style flags (OR'd with the cell's existing flags).
    pub font: FontStyle,
    /// Decoration (underline, etc. — additive).
    pub deco: Decoration,
    /// Force class fg even over explicit ANSI fg (off by default).
    pub override_ansi: bool,
}

/// Pre-resolved theme styles for all classes — a flat array indexed by
/// `Class as usize`.
///
/// Themes without a `terminal.semantic` block → all `None` → Layer 2 is a
/// no-op (fully backwards compatible). The `ui` crate populates this from
/// the theme JSON's `terminal.semantic` block + the shipped default asset.
#[derive(Clone, Debug)]
pub struct ClassStyles {
    /// Resolved style per class.
    styles: Box<[ClassStyle; Class::COUNT]>,
    /// Line-level: prompt-line background.
    pub prompt_line_bg: Option<Hsla>,
}

impl Default for ClassStyles {
    fn default() -> Self {
        Self {
            styles: Box::new([const { ClassStyle::empty() }; Class::COUNT]),
            prompt_line_bg: None,
        }
    }
}

impl ClassStyle {
    /// The all-`None` style (const so the class array can be built in place).
    const fn empty() -> Self {
        Self {
            fg: None,
            bg: None,
            font: FontStyle {
                bold: false,
                italic: false,
            },
            deco: Decoration::None,
            override_ansi: false,
        }
    }
}

impl ClassStyles {
    /// Create an empty `ClassStyles` (all `None` — Layer 2 is a no-op).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Get the style for a class (clamped to `Default` for out-of-range).
    pub fn style(&self, class: u8) -> ClassStyle {
        self.styles.get(class as usize).copied().unwrap_or_default()
    }

    /// Mutable access to one class's style (for theme loading).
    pub fn style_mut(&mut self, class: Class) -> &mut ClassStyle {
        &mut self.styles[class as usize]
    }

    /// Set the foreground for a class.
    pub fn set_fg(&mut self, class: Class, color: Hsla) {
        self.style_mut(class).fg = Some(color);
    }

    /// Set the decoration for a class.
    pub fn set_deco(&mut self, class: Class, deco: Decoration) {
        self.style_mut(class).deco = deco;
    }

    /// Whether any semantic styling is configured (all-None → no-op).
    pub fn is_active(&self) -> bool {
        self.styles
            .iter()
            .any(|style| style.fg.is_some() || style.bg.is_some())
            || self.prompt_line_bg.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_noop() {
        let s = ClassStyles::empty();
        assert!(!s.is_active());
        let st = s.style(Class::Error as u8);
        assert_eq!(st.fg, None);
    }

    #[test]
    fn set_and_get() {
        let mut s = ClassStyles::empty();
        s.set_fg(Class::Error, Hsla::new(0.0, 0.8, 0.5, 1.0));
        assert!(s.is_active());
        let st = s.style(Class::Error as u8);
        assert!(st.fg.is_some());
    }

    #[test]
    fn style_mut_edits_one_class_only() {
        let mut s = ClassStyles::empty();
        s.style_mut(Class::Url).font.bold = true;
        assert!(s.style(Class::Url as u8).font.bold);
        assert!(!s.style(Class::Error as u8).font.bold);
        // Font-only styling does not make Layer 2 active (no colours).
        assert!(!s.is_active());
        // Out-of-range class → default style.
        assert_eq!(s.style(u8::MAX).fg, None);
    }
}
