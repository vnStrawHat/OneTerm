//! `US-0198` rework 6: the collapsed rail's cell shows the tab's shell icon on its status colour,
//! with no position number and no status badge.

use std::path::PathBuf;

use gpui::{TestAppContext, VisualTestContext, px};
use gpui_component::ActiveTheme as _;
use oneterm_core::{LocalShellConfig, SessionDuplicateConfig, ShellKind};
use oneterm_state::TabRowLayout;
use oneterm_terminal::{ProgramState, SessionEvent, SessionKind};
use oneterm_theme::AppIcon;

use super::{tab_row_window_of_width, tab_row_window_with_panel};
use crate::panel::TerminalPanel;
use crate::panel::tab_title::{
    TAB_CLOSE_SELECTOR, TAB_CWD_SELECTOR, TAB_FILL_SELECTOR, TAB_LABEL_SELECTOR, TAB_ROW_SELECTOR,
    TAB_SHELL_ICON_SELECTOR, TAB_STATUS_SELECTOR, collapsed_fill, shell_icon,
};

fn local(kind: ShellKind, program: Option<&str>) -> SessionDuplicateConfig {
    SessionDuplicateConfig::Local(LocalShellConfig {
        kind,
        program: program.map(PathBuf::from),
        ..LocalShellConfig::default()
    })
}

/// cmd and the two PowerShells have their own glyph; every other shell, SSH and an unknown shell
/// take the generic terminal glyph. An explicit program names the shell, not the kind.
#[test]
fn the_shell_decides_the_icon() {
    let icon = |config: Option<SessionDuplicateConfig>| shell_icon(config.as_ref());

    assert_eq!(icon(Some(local(ShellKind::Cmd, None))), AppIcon::Cmd);
    assert_eq!(
        icon(Some(local(ShellKind::PowerShell, None))),
        AppIcon::Powershell
    );
    assert_eq!(
        icon(Some(local(ShellKind::Pwsh, None))),
        AppIcon::Powershell
    );
    // The program wins, either separator, any case, with or without `.exe`.
    for (kind, program, expected) in [
        (
            ShellKind::Custom,
            r"C:\Windows\System32\CMD.EXE",
            AppIcon::Cmd,
        ),
        (ShellKind::Custom, "cmd", AppIcon::Cmd),
        (
            ShellKind::Custom,
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
            AppIcon::Powershell,
        ),
        (
            ShellKind::Pwsh,
            "C:/Program Files/PowerShell/7/pwsh.exe",
            AppIcon::Powershell,
        ),
        // Git Bash is `Bash` with a program; others are not cmd or PowerShell.
        (
            ShellKind::Bash,
            r"C:\Program Files\Git\bin\bash.exe",
            AppIcon::Terminal,
        ),
        // The kind says cmd, the program says nushell: the label says "nu", so does the icon.
        (ShellKind::Cmd, "nu.exe", AppIcon::Terminal),
        (ShellKind::Custom, "/usr/bin/fish", AppIcon::Terminal),
        (ShellKind::Custom, "wsl.exe", AppIcon::Terminal),
    ] {
        assert_eq!(
            icon(Some(local(kind, Some(program)))),
            expected,
            "{kind:?} {program}"
        );
    }
    for kind in [
        ShellKind::Bash,
        ShellKind::Zsh,
        ShellKind::Sh,
        ShellKind::Custom,
    ] {
        assert_eq!(icon(Some(local(kind, None))), AppIcon::Terminal, "{kind:?}");
    }
    assert_eq!(icon(None), AppIcon::Terminal, "no launch data");
    let ssh = SessionDuplicateConfig::Ssh(oneterm_core::SshDuplicateConfig {
        host: "host".into(),
        port: 22,
        username: "user".into(),
        auth: oneterm_core::SshDuplicateAuth::Agent,
        shell_integration: false,
        jump_hops: Vec::new(),
        port_forwards: Vec::new(),
        agent_forwarding: false,
    });
    assert_eq!(icon(Some(ssh)), AppIcon::Terminal, "an SSH session");
}

/// The tab reads the shell from its active terminal's launch data.
#[gpui::test]
fn the_panel_reads_the_shell_from_its_terminal(cx: &mut TestAppContext) {
    let (_, _, panel, cx) =
        tab_row_window_with_panel(cx, TabRowLayout::Collapsed, 44., SessionKind::Local);
    let icon = |panel: &gpui::Entity<TerminalPanel>, cx: &mut VisualTestContext| {
        panel.read_with(cx, |panel, cx| panel.tab_shell_icon(cx))
    };
    assert_eq!(icon(&panel, cx), AppIcon::Terminal, "no launch data");
    for (config, expected) in [
        (local(ShellKind::Cmd, None), AppIcon::Cmd),
        (local(ShellKind::Pwsh, None), AppIcon::Powershell),
        (local(ShellKind::Bash, None), AppIcon::Terminal),
    ] {
        panel.update(cx, |panel, cx| {
            let view = panel.tree.active_terminal().expect("a terminal");
            view.update(cx, |view, _| view.duplicate_config = Some(config));
        });
        assert_eq!(icon(&panel, cx), expected);
    }
}

/// Each state fills with the colour its icon had, and the glyph is the better of the theme's
/// `background` and `foreground` on it; idle has no fill. A themed pair that reads poorly (the
/// themes' own `*_foreground`) is never used.
#[gpui::test]
fn each_state_has_its_own_fill_and_none_for_idle(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(|cx| {
        let theme = cx.theme();
        let fills: Vec<_> = [
            (ProgramState::Working, theme.primary),
            (ProgramState::Blocked, theme.warning),
            (ProgramState::Done, theme.success),
            (ProgramState::Error, theme.danger),
        ]
        .into_iter()
        .map(|(state, token)| {
            let (fill, glyph) = collapsed_fill(state, theme).expect("a reported state fills");
            assert_eq!(fill, token, "{state:?}");
            assert!(
                glyph == theme.background || glyph == theme.foreground,
                "{state:?}: the glyph is a theme token"
            );
            fill
        })
        .collect();
        assert_eq!(collapsed_fill(ProgramState::Idle, theme), None);
        for (i, a) in fills.iter().enumerate() {
            assert!(
                fills[i + 1..].iter().all(|b| a != b),
                "the four states read apart"
            );
        }
        // A dark fill gets the light token and a light one the dark token, whatever the theme.
        let mut theme = theme.clone();
        theme.background = gpui::hsla(0., 0., 0.05, 1.);
        theme.foreground = gpui::hsla(0., 0., 0.95, 1.);
        theme.primary = gpui::hsla(0.6, 0.6, 0.15, 1.);
        theme.danger = gpui::hsla(0.0, 0.3, 0.85, 1.);
        let glyph = |state| collapsed_fill(state, &theme).unwrap().1;
        assert_eq!(glyph(ProgramState::Working), theme.foreground);
        assert_eq!(glyph(ProgramState::Error), theme.background);
    });
}

fn report(
    probe: &oneterm_terminal::test_support::FakeSessionProbe,
    state: Option<ProgramState>,
    cx: &mut VisualTestContext,
) {
    probe
        .emit(SessionEvent::ProgramStatus(state))
        .expect("event channel must accept the status");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

/// A cell is the shell icon alone while idle; a reported state fills the cell, never as a badge,
/// and clearing it removes the fill.
#[gpui::test]
fn a_collapsed_cell_is_the_shell_icon_on_the_status_fill(cx: &mut TestAppContext) {
    let (_, probe, cx) =
        tab_row_window_of_width(cx, TabRowLayout::Collapsed, 44., SessionKind::Local);
    probe.set_cwd(Some(PathBuf::from("Z:/oneterm-test/x")));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let row = cx
        .debug_bounds(TAB_ROW_SELECTOR)
        .expect("the cell is drawn");
    let icon = cx
        .debug_bounds(TAB_SHELL_ICON_SELECTOR)
        .expect("the shell icon is drawn");
    assert_eq!((row.size.width, row.size.height), (px(44.), px(44.)));
    // Owner rework 7: 16 px (`size_4`), not the 14 px of `small`.
    assert_eq!((icon.size.width, icon.size.height), (px(16.), px(16.)));
    assert!(
        (icon.center().x - row.center().x).abs() <= px(1.)
            && (icon.center().y - row.center().y).abs() <= px(1.),
        "the icon {icon:?} is centred in the cell {row:?}"
    );
    let nothing_but_the_icon = |cx: &mut VisualTestContext| {
        for (selector, what) in [
            (TAB_LABEL_SELECTOR, "title"),
            (TAB_CWD_SELECTOR, "directory"),
            (TAB_CLOSE_SELECTOR, "close button"),
            (TAB_STATUS_SELECTOR, "status badge"),
        ] {
            assert!(cx.debug_bounds(selector).is_none(), "no {what} in the cell");
        }
    };
    nothing_but_the_icon(cx);
    assert!(
        cx.debug_bounds(TAB_FILL_SELECTOR).is_none(),
        "idle: no fill"
    );

    for state in [
        ProgramState::Working,
        ProgramState::Blocked,
        ProgramState::Done,
        ProgramState::Error,
    ] {
        report(&probe, Some(state), cx);
        let fill = cx
            .debug_bounds(TAB_FILL_SELECTOR)
            .unwrap_or_else(|| panic!("{state:?}: the cell is filled"));
        assert_eq!(fill.size.height, row.size.height, "{state:?}");
        assert_eq!(fill.right(), row.right(), "{state:?}");
        nothing_but_the_icon(cx);
    }

    report(&probe, None, cx);
    assert!(
        cx.debug_bounds(TAB_FILL_SELECTOR).is_none(),
        "a cleared state removes the fill"
    );
}

/// Owner rework 7 review: the fill of a cell reaches its whole width, selected or not (no notches
/// left at the corners of the old 3 px strip), and a selected filled cell draws its own accent pill
/// over the fill, inset 8 px top and bottom, which an unselected one does not.
#[gpui::test]
fn a_selected_fill_covers_the_cell_and_carries_its_own_pill(cx: &mut TestAppContext) {
    use crate::panel::tab_title::TAB_FILL_PILL_SELECTOR;

    let (_, probe, panel, cx) =
        tab_row_window_with_panel(cx, TabRowLayout::Collapsed, 44., SessionKind::Local);
    report(&probe, Some(ProgramState::Working), cx);
    let row = cx
        .debug_bounds(TAB_ROW_SELECTOR)
        .expect("the cell is drawn");
    let set_active = |active: bool, cx: &mut VisualTestContext| {
        panel.update_in(cx, |panel, window, cx| {
            <TerminalPanel as gpui_base::dock::Panel>::set_active(panel, active, window, cx);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.debug_bounds(TAB_FILL_SELECTOR)
            .expect("the cell is filled")
    };
    let selected = set_active(true, cx);
    assert_eq!(selected, row, "the fill covers the whole cell");
    let pill = cx
        .debug_bounds(TAB_FILL_PILL_SELECTOR)
        .expect("a selected filled cell draws its pill");
    assert_eq!(pill.left(), row.left());
    assert_eq!(pill.size.width, px(TabRowLayout::ACCENT_BAR_WIDTH));
    assert_eq!(pill.top() - row.top(), px(TabRowLayout::ACCENT_BAR_INSET));
    assert_eq!(
        row.bottom() - pill.bottom(),
        px(TabRowLayout::ACCENT_BAR_INSET)
    );
    let unselected = set_active(false, cx);
    assert_eq!(unselected, row);
    assert!(
        cx.debug_bounds(TAB_FILL_PILL_SELECTOR).is_none(),
        "an unselected cell has no pill of its own"
    );
}

/// The expanded row and the strip keep their status icon, and draw no fill.
#[gpui::test]
fn the_expanded_row_keeps_its_status_icon(cx: &mut TestAppContext) {
    for layout in [TabRowLayout::Vertical, TabRowLayout::Horizontal] {
        let (_, probe, cx) = tab_row_window_of_width(cx, layout, 220., SessionKind::Local);
        report(&probe, Some(ProgramState::Done), cx);
        assert!(
            cx.debug_bounds(TAB_STATUS_SELECTOR).is_some(),
            "{layout:?}: the status icon is drawn"
        );
        assert!(
            cx.debug_bounds(TAB_FILL_SELECTOR).is_none(),
            "{layout:?}: no fill"
        );
        assert!(
            cx.debug_bounds(TAB_SHELL_ICON_SELECTOR).is_none(),
            "{layout:?}: no shell icon"
        );
    }
}
