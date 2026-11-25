//! `CompletionOverlay` — the cursor-anchored suggestion list (docs 05).
//!
//! A stateless [`RenderOnce`] element re-emitted each frame from the controller's
//! current suggestions. All colors come from `cx.theme()` (never hardcoded); the
//! three tag badges (`H`/`C`/`O`) use the theme's semantic palette (docs 05 §5).
//! The caller positions the overlay by wrapping it in an absolutely-positioned
//! container anchored to the token-start cell (docs 05 §2–3).

use gpui::{
    Div, FontWeight, IntoElement, ParentElement as _, RenderOnce, Styled as _, Window, div, px,
};
use gpui_component::ActiveTheme as _;

use oneterm_completion::{Suggestion, SuggestionKind};

/// A single row's precomputed display data (kept simple + owned so the element
/// is `RenderOnce`).
#[derive(Clone)]
struct Row {
    /// Matched prefix (highlighted) + the remainder (normal weight).
    matched: String,
    rest: String,
    /// Optional italic hint shown right-aligned before the kind label.
    description: Option<String>,
    kind: SuggestionKind,
    selected: bool,
}

/// The completion overlay list element.
#[derive(IntoElement)]
pub struct CompletionOverlay {
    rows: Vec<Row>,
    /// Optional breadcrumb of the resolved command path (`git › remote ›`).
    breadcrumb: Option<String>,
    /// How many suggestions are scrolled off the top / bottom of the window.
    hidden_above: usize,
    hidden_below: usize,
}

impl CompletionOverlay {
    /// Build the overlay from a windowed slice of the controller's suggestions.
    ///
    /// `suggestions` is already the visible window (≤ `max_visible`); `selected`
    /// is the index **within that window**. `hidden_above`/`hidden_below` are the
    /// counts scrolled out of view, rendered as `↑ N` / `↓ M` hint rows.
    pub fn new(
        suggestions: &[Suggestion],
        selected: Option<usize>,
        breadcrumb: Option<String>,
        hidden_above: usize,
        hidden_below: usize,
    ) -> Self {
        let rows = suggestions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let split = s.match_len.min(s.text.len());
                Row {
                    matched: s.text[..split].to_string(),
                    rest: s.text[split..].to_string(),
                    description: s.description.clone(),
                    kind: s.kind,
                    selected: selected == Some(i),
                }
            })
            .collect();
        Self {
            rows,
            breadcrumb,
            hidden_above,
            hidden_below,
        }
    }
}

impl RenderOnce for CompletionOverlay {
    fn render(self, _window: &mut Window, cx: &mut gpui::App) -> impl IntoElement {
        let theme = cx.theme();
        let mut container: Div = div()
            .flex()
            .flex_col()
            .min_w(px(180.0))
            .rounded(px(6.0))
            .overflow_hidden()
            .p_0p5()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_md()
            .text_sm();

        if let Some(crumb) = self.breadcrumb {
            container = container.child(
                div()
                    .px_2()
                    .py_0p5()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(crumb),
            );
        }

        if self.hidden_above > 0 {
            container = container.child(
                div()
                    .px_2()
                    .py_0p5()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("↑ {} more", self.hidden_above)),
            );
        }

        for row in self.rows {
            let selected = row.selected;
            // White text on the vivid blue selected row; otherwise the theme's
            // primary accent for the matched prefix + popover text for the body.
            let matched_fg = if selected { white() } else { theme.primary };
            let body_fg = if selected {
                white()
            } else {
                theme.popover_foreground
            };
            let muted_fg = if selected {
                white()
            } else {
                theme.muted_foreground
            };
            let weight = if selected {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            };

            let mut row_div = div()
                .flex()
                .flex_row()
                .items_center()
                .gap_4()
                .px_2()
                .py_0p5()
                .rounded(px(4.0))
                .text_color(body_fg)
                .font_weight(weight);
            if selected {
                row_div = row_div.bg(theme.blue);
            }
            row_div = row_div
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .child(div().text_color(matched_fg).child(row.matched))
                        .child(div().child(row.rest)),
                )
                // Spacer pushes the optional hint + kind label to the right edge.
                .child(div().flex_1());
            if let Some(desc) = row.description {
                row_div = row_div.child(div().italic().text_color(muted_fg).child(desc));
            }
            // The suggestion kind as muted words (`option` / `command` /
            // `history`) instead of a single-letter tag.
            row_div = row_div.child(
                div()
                    .text_xs()
                    .text_color(muted_fg)
                    .child(kind_label(row.kind)),
            );

            container = container.child(row_div);
        }

        if self.hidden_below > 0 {
            container = container.child(
                div()
                    .px_2()
                    .py_0p5()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("↓ {} more", self.hidden_below)),
            );
        }

        container
    }
}

/// Opaque white — the high-contrast text color on the vivid blue selected row.
fn white() -> gpui::Hsla {
    gpui::hsla(0.0, 0.0, 1.0, 1.0)
}

/// The suggestion kind rendered as a muted lowercase word.
fn kind_label(kind: SuggestionKind) -> &'static str {
    match kind {
        SuggestionKind::History => "history",
        SuggestionKind::Command => "command",
        SuggestionKind::Option => "option",
    }
}
