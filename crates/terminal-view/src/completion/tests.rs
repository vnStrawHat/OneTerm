//! Unit tests for the gpui-free `CompletionController`.

use oneterm_completion::{CompletionHistory, ShellFamily};
use oneterm_core::ShellKind;
use oneterm_settings::CompletionSettings;

use super::controller::{CompletionController, params_from_settings};

fn settings() -> CompletionSettings {
    CompletionSettings::default()
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
    // Navigate to the "dir" row then accept → append "r".
    // Find dir's index.
    let idx = c
        .suggestions()
        .iter()
        .position(|s| s.text == "dir")
        .unwrap();
    for _ in 0..=idx {
        c.select_next();
    }
    assert_eq!(c.selected(), Some(idx));
    assert_eq!(c.accept_bytes().as_deref(), Some("r"));
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
