//! Unit tests for the gpui-free `CompletionController`.

use oneterm_completion::{CompletionHistory, ShellFamily};
use oneterm_core::ShellKind;
use oneterm_settings::CompletionSettings;

use super::controller::{CompletionController, params_from_settings};

fn settings() -> CompletionSettings {
    CompletionSettings::default()
}

fn memory_only_settings() -> CompletionSettings {
    CompletionSettings {
        source_manual: false,
        source_external: false,
        fuzzy: false,
        ..CompletionSettings::default()
    }
}

#[test]
fn gating_blocks_in_alt_screen() {
    let mut c = CompletionController::new(ShellKind::Cmd, &settings());
    assert!(c.gating_allows());
    c.set_alt_screen(true);
    assert!(!c.gating_allows());
    c.set_alt_screen(false);
    assert!(c.gating_allows());
}

#[test]
fn gating_requires_prompt_region_by_default() {
    let mut c = CompletionController::new(ShellKind::Cmd, &settings());
    c.set_in_prompt_region(false);
    assert!(!c.gating_allows());
    c.set_in_prompt_region(true);
    assert!(c.gating_allows());
}

#[test]
fn disabled_master_switch_blocks() {
    let mut s = settings();
    s.enabled = false;
    let c = CompletionController::new(ShellKind::Cmd, &s);
    assert!(!c.gating_allows());
}

#[test]
fn recompute_gated_off_yields_no_suggestions() {
    let mut c = CompletionController::new(ShellKind::Cmd, &settings());
    c.set_alt_screen(true);
    let h = CompletionHistory::new(10);
    c.recompute("d", 1, 1000, &h, false);
    assert!(!c.is_visible());
}

#[test]
fn recompute_lists_commands_and_navigation_and_accept() {
    let mut c = CompletionController::new(ShellKind::Cmd, &settings());
    let h = CompletionHistory::new(10);
    c.recompute("di", 2, 1000, &h, false);
    assert!(c.is_visible());
    assert!(c.suggestions().iter().any(|s| s.text == "dir"));
    // Run-first: nothing selected yet → accept returns None.
    assert_eq!(c.selected(), None);
    assert_eq!(c.accept_bytes(), None);
    // Explicitly select the first row, navigate to "dir", then accept.
    let idx = c
        .suggestions()
        .iter()
        .position(|s| s.text == "dir")
        .unwrap();
    assert!(c.select_first_if_none());
    for _ in 0..idx {
        c.select_next();
    }
    assert_eq!(c.selected(), Some(idx));
    assert_eq!(c.accept_bytes().as_deref(), Some(b"r".as_slice()));
}

#[test]
fn sole_exact_match_is_hidden_but_actionable_results_remain_visible() {
    let settings = memory_only_settings();
    let mut history = CompletionHistory::new(10);
    history.record(ShellFamily::Cmd, "ls extra", 1000);

    let mut controller = CompletionController::new(ShellKind::Cmd, &settings);
    controller.recompute("ls", 2, 2000, &history, false);
    assert!(
        !controller.is_visible(),
        "sole exact match should be hidden"
    );

    controller.recompute("l", 1, 2000, &history, false);
    assert!(
        controller.is_visible(),
        "prefix extension should remain visible"
    );

    controller.recompute("LS", 2, 2000, &history, false);
    assert!(
        controller.is_visible(),
        "case correction should remain actionable"
    );
}

#[test]
fn exact_match_remains_visible_when_other_suggestions_exist() {
    let settings = memory_only_settings();
    let mut history = CompletionHistory::new(10);
    history.record(ShellFamily::Cmd, "ls extra", 1000);
    history.record(ShellFamily::Cmd, "lsof", 1100);

    let mut controller = CompletionController::new(ShellKind::Cmd, &settings);
    controller.recompute("ls", 2, 2000, &history, false);

    assert!(controller.is_visible());
    assert_eq!(controller.suggestions().len(), 2);
}

#[test]
fn windows_history_acceptance_applies_exact_suggestion_casing() {
    for kind in [ShellKind::Cmd, ShellKind::PowerShell] {
        let family = ShellFamily::from_kind(kind);
        let mut history = CompletionHistory::new(10);
        history.record(family, "cd Project", 1000);

        for (line, expected_bytes) in [
            ("cd ", b"Project".as_slice()),
            ("cd p", b"\x7fProject".as_slice()),
            ("cd P", b"roject".as_slice()),
        ] {
            let mut controller = CompletionController::new(kind, &settings());
            controller.recompute(line, line.len(), 2000, &history, false);
            select_suggestion(&mut controller, "cd Project");

            assert_eq!(
                controller.accept_bytes().as_deref(),
                Some(expected_bytes),
                "history completion should apply exact casing for {kind:?} and {line:?}"
            );
        }
    }
}

#[test]
fn unix_history_acceptance_remains_case_sensitive() {
    let mut history = CompletionHistory::new(10);
    history.record(ShellFamily::Unix, "cd Project", 1000);
    let mut controller = CompletionController::new(ShellKind::Bash, &settings());

    controller.recompute("cd p", 4, 2000, &history, false);
    assert!(
        controller
            .suggestions()
            .iter()
            .all(|suggestion| suggestion.text != "cd Project")
    );

    controller.recompute("cd P", 4, 2000, &history, false);
    select_suggestion(&mut controller, "cd Project");
    assert_eq!(
        controller.accept_bytes().as_deref(),
        Some(b"roject".as_slice())
    );
}

#[test]
fn navigation_does_nothing_until_first_item_is_selected() {
    let mut controller = CompletionController::new(ShellKind::Cmd, &settings());
    let history = CompletionHistory::new(10);
    controller.recompute("d", 1, 1000, &history, false);

    controller.select_next();
    assert_eq!(controller.selected(), None);
    controller.select_prev();
    assert_eq!(controller.selected(), None);

    assert!(controller.select_first_if_none());
    assert!(!controller.select_first_if_none());
    assert_eq!(controller.selected(), Some(0));
    controller.select_next();
    assert_eq!(controller.selected(), Some(1));
    controller.select_prev();
    assert_eq!(controller.selected(), Some(0));
}

fn select_suggestion(controller: &mut CompletionController, text: &str) {
    let index = controller
        .suggestions()
        .iter()
        .position(|suggestion| suggestion.text == text)
        .expect("expected suggestion should be visible");
    assert!(controller.select_first_if_none());
    for _ in 0..index {
        controller.select_next();
    }
}

#[test]
fn force_bypasses_min_prefix() {
    let mut s = settings();
    s.min_prefix_len = 3;
    let mut c = CompletionController::new(ShellKind::Cmd, &s);
    let h = CompletionHistory::new(10);
    c.recompute("d", 1, 1000, &h, false);
    assert!(!c.is_visible());
    c.recompute("d", 1, 1000, &h, true);
    assert!(c.is_visible());
}

#[test]
fn capture_redacts_before_recording() {
    let c = CompletionController::new(ShellKind::Bash, &settings());
    let mut h = CompletionHistory::new(10);
    c.capture("az login --password S3cr3t!", 1000, &mut h);
    let entries = h.entries(ShellFamily::Unix);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].line, "az login --password");
    assert!(!entries[0].line.contains("S3cr3t"));
}

#[test]
fn capture_skips_in_alt_screen() {
    let mut c = CompletionController::new(ShellKind::Bash, &settings());
    c.set_alt_screen(true);
    let mut h = CompletionHistory::new(10);
    c.capture("ls -la", 1000, &mut h);
    assert!(h.is_empty());
}

#[test]
fn max_history_zero_disables_memory_source() {
    let mut s = settings();
    s.max_history = 0;
    let p = params_from_settings(&s);
    assert!(!p.sources.memory);
}

#[test]
fn force_family_overrides_kind() {
    let mut s = settings();
    s.force_family = Some("unix".into());
    let c = CompletionController::new(ShellKind::Cmd, &s);
    assert_eq!(c.family(), ShellFamily::Unix);
}
