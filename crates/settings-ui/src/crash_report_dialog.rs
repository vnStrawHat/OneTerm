//! Sequential recovery dialogs for crash reports captured during previous runs.

use std::{collections::VecDeque, io, path::PathBuf};

use gpui::{
    App, AppContext as _, ClipboardItem, Context, ParentElement as _, Styled as _, Window, px,
};
use gpui_component::{
    Root, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{Dialog, DialogFooter},
    h_flex,
    input::{Input, InputState},
    v_flex,
};

const NEW_ISSUE_URL: &str = "https://github.com/vnStrawHat/OneTerm/issues/new";
const ISSUE_TITLE: &str = "Crash report";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrashReport {
    pub path: PathBuf,
    pub contents: String,
}

type CleanupReport = fn(PathBuf) -> io::Result<()>;

/// Show retained crash reports newest-first after the main window opens.
pub fn show_crash_reports(
    reports: Vec<CrashReport>,
    cleanup: CleanupReport,
    root: &mut Root,
    window: &mut Window,
    cx: &mut Context<Root>,
) {
    let mut reports = VecDeque::from(reports);
    let Some(report) = reports.pop_front() else {
        return;
    };
    let input = report_input(&report.contents, window, cx);
    root.open_dialog(crash_dialog(report, reports, cleanup, input), window, cx);
}

fn open_next_report(
    mut reports: VecDeque<CrashReport>,
    cleanup: CleanupReport,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(report) = reports.pop_front() else {
        return;
    };
    let input = report_input(&report.contents, window, cx);
    window.open_dialog(cx, crash_dialog(report, reports, cleanup, input));
}

fn report_input(report: &str, window: &mut Window, cx: &mut App) -> gpui::Entity<InputState> {
    cx.new(|cx| {
        InputState::new(window, cx)
            .multi_line(true)
            .default_value(report.to_owned())
    })
}

fn crash_dialog(
    report: CrashReport,
    remaining: VecDeque<CrashReport>,
    cleanup: CleanupReport,
    report_input: gpui::Entity<InputState>,
) -> impl Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static {
    let issue_url = create_issue_url();

    move |dialog, _, _| {
        let issue_url = issue_url.clone();
        let report_for_clipboard = report.contents.clone();
        let report_path = report.path.clone();
        let remaining = remaining.clone();

        dialog
            .title("OneTerm closed unexpectedly")
            .w(px(760.))
            .close_button(true)
            .overlay_closable(true)
            .child(
                v_flex()
                    .gap_3()
                    .w_full()
                    .child("OneTerm detected a crash report from a previous run.")
                    .child(
                        Input::new(&report_input)
                            .w_full()
                            .h(px(360.))
                            .disabled(true),
                    ),
            )
            .footer(
                DialogFooter::new().w_full().child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("crash-report-dismiss")
                                .label("Dismiss")
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                    let report_path = report_path.clone();
                                    cx.background_spawn(async move {
                                        if let Err(error) = cleanup(report_path) {
                                            log::error!(
                                                "Failed to delete pending crash report: {error}"
                                            );
                                        }
                                    })
                                    .detach();
                                    open_next_report(remaining.clone(), cleanup, window, cx);
                                }),
                        )
                        .child(Button::new("crash-report-copy").label("Copy").on_click({
                            move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    report_for_clipboard.clone(),
                                ));
                            }
                        }))
                        .child(
                            Button::new("crash-report-create-issue")
                                .primary()
                                .label("Create Issue")
                                .on_click(move |_, _, cx| cx.open_url(&issue_url)),
                        ),
                ),
            )
    }
}

fn create_issue_url() -> String {
    format!("{NEW_ISSUE_URL}?title={}", percent_encode(ISSUE_TITLE))
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            write!(&mut encoded, "%{byte:02X}").expect("writing to a String cannot fail");
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_queue_advances_newest_first() {
        let mut reports = VecDeque::from([
            CrashReport {
                path: "newest.crash.txt".into(),
                contents: "newest".into(),
            },
            CrashReport {
                path: "older.crash.txt".into(),
                contents: "older".into(),
            },
        ]);

        assert_eq!(reports.pop_front().unwrap().contents, "newest");
        assert_eq!(reports.pop_front().unwrap().contents, "older");
        assert!(reports.is_empty());
    }

    #[test]
    fn percent_encoding_is_safe_for_github_query_parameters() {
        assert_eq!(
            percent_encode("panic: a/b + c?"),
            "panic%3A%20a%2Fb%20%2B%20c%3F"
        );
    }

    #[test]
    fn issue_url_prefills_only_the_title() {
        let url = create_issue_url();

        assert_eq!(url, format!("{NEW_ISSUE_URL}?title=Crash%20report"));
        assert!(!url.contains("body="));
    }
}
