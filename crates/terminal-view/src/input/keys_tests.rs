//! One test per row of the classification table in
//! `low-level-design/input.md`, plus the `map_key` cases carried over from the
//! old `view/key.rs`.

use gpui::{AppContext as _, Keystroke, Modifiers, TestAppContext};
use oneterm_terminal::test_support::FakeTerminalSession;
use oneterm_terminal::{KeyMods, KeySpec, NamedKey, encode_key};

use super::keys::{
    CompletionKey, KeyAction, KeyContext, classify_key, interrupt, map_key, send_key,
};

fn ks(key: &str, mods: Modifiers, key_char: Option<&str>) -> Keystroke {
    Keystroke {
        modifiers: mods,
        key: key.to_string(),
        key_char: key_char.map(str::to_string),
    }
}

fn classify(ks: &Keystroke) -> KeyAction {
    classify_key(ks, false, KeyContext::default())
}

fn ctrl() -> Modifiers {
    Modifiers {
        control: true,
        ..Default::default()
    }
}

fn ctrl_shift() -> Modifiers {
    Modifiers {
        control: true,
        shift: true,
        ..Default::default()
    }
}

fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Default::default()
    }
}

fn platform() -> Modifiers {
    Modifiers {
        platform: true,
        ..Default::default()
    }
}

/// The platform modifier: Cmd on macOS, Ctrl elsewhere.
fn secondary() -> Modifiers {
    if cfg!(target_os = "macos") {
        platform()
    } else {
        ctrl()
    }
}

fn secondary_shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..secondary()
    }
}

#[test]
fn ctrl_f_toggles_search() {
    assert_eq!(
        classify(&ks("f", secondary(), None)),
        KeyAction::ToggleSearch
    );
    // Shift or Alt disarm it so Ctrl+Shift+F still reaches the program.
    assert_ne!(
        classify(&ks("f", secondary_shift(), None)),
        KeyAction::ToggleSearch
    );
}

#[test]
fn enter_in_search_is_swallowed() {
    let focused = KeyContext {
        search_focused: true,
        ..KeyContext::default()
    };
    let enter = ks("enter", Modifiers::default(), Some("\r"));
    assert_eq!(
        classify_key(&enter, false, focused),
        KeyAction::SwallowInSearch
    );
    assert!(matches!(
        classify(&enter),
        KeyAction::Send(KeySpec::Named(NamedKey::Enter), _)
    ));
}

#[test]
fn ctrl_shift_space_triggers_completion() {
    assert_eq!(
        classify(&ks("space", ctrl_shift(), None)),
        KeyAction::TriggerCompletion
    );
}

#[test]
fn zoom_chords() {
    assert_eq!(classify(&ks("-", secondary(), None)), KeyAction::ZoomOut);
    assert_eq!(classify(&ks("=", secondary(), None)), KeyAction::ZoomIn);
    assert_eq!(classify(&ks("+", secondary(), None)), KeyAction::ZoomIn);
    assert_eq!(classify(&ks("0", secondary(), None)), KeyAction::ZoomReset);
    let with_alt = Modifiers {
        alt: true,
        ..secondary()
    };
    assert_ne!(classify(&ks("0", with_alt, None)), KeyAction::ZoomReset);
}

#[test]
fn shift_page_and_home_end_scroll() {
    assert_eq!(
        classify(&ks("pageup", shift(), None)),
        KeyAction::ScrollPages(1)
    );
    assert_eq!(
        classify(&ks("pagedown", shift(), None)),
        KeyAction::ScrollPages(-1)
    );
    assert_eq!(classify(&ks("home", shift(), None)), KeyAction::ScrollTop);
    assert_eq!(classify(&ks("end", shift(), None)), KeyAction::ScrollBottom);
    // Without Shift the same keys are ordinary terminal keys.
    assert!(matches!(
        classify(&ks("home", Modifiers::default(), None)),
        KeyAction::Send(KeySpec::Named(NamedKey::Home), _)
    ));
}

#[test]
fn platform_shift_arrows_scroll_one_line() {
    assert_eq!(
        classify(&ks("up", secondary_shift(), None)),
        KeyAction::ScrollLines(1)
    );
    assert_eq!(
        classify(&ks("down", secondary_shift(), None)),
        KeyAction::ScrollLines(-1)
    );
    // Shift alone keeps Shift+Up as a program key.
    assert!(matches!(
        classify(&ks("up", shift(), None)),
        KeyAction::Send(KeySpec::Named(NamedKey::ArrowUp), _)
    ));
}

#[test]
fn copy_paste_chords_per_platform() {
    let (copy, paste) = if cfg!(target_os = "macos") {
        (ks("c", platform(), None), ks("v", platform(), None))
    } else {
        (ks("c", ctrl_shift(), None), ks("v", ctrl_shift(), None))
    };
    assert_eq!(classify(&copy), KeyAction::Copy);
    assert_eq!(classify(&paste), KeyAction::Paste);
}

#[test]
fn shift_insert_pastes() {
    assert_eq!(classify(&ks("insert", shift(), None)), KeyAction::Paste);
}

#[test]
fn plain_char_on_primary_screen_is_ignored() {
    assert_eq!(
        classify(&ks("a", Modifiers::default(), Some("a"))),
        KeyAction::Ignore
    );
}

#[test]
fn plain_char_on_alt_screen_is_sent() {
    let alt = KeyContext {
        alt_screen: true,
        ..KeyContext::default()
    };
    assert!(matches!(
        classify_key(&ks("a", Modifiers::default(), Some("a")), false, alt),
        KeyAction::Send(KeySpec::Character(ref c), _) if c == "a"
    ));
}

#[test]
fn altgr_char_on_windows_is_ignored() {
    let ctrl_alt = Modifiers {
        control: true,
        alt: true,
        ..Default::default()
    };
    let altgr = ks("q", ctrl_alt, Some("@"));
    // The platform flag settles it everywhere.
    assert_eq!(
        classify_key(&altgr, true, KeyContext::default()),
        KeyAction::Ignore
    );
    // Windows reports AltGr as Ctrl+Alt with no flag, so printable text is
    // the discriminator there.
    if cfg!(windows) {
        assert_eq!(classify(&altgr), KeyAction::Ignore);
    } else {
        assert!(matches!(classify(&altgr), KeyAction::Send(..)));
    }
    // A real Ctrl+Alt chord (no printable text) always reaches the encoder.
    assert!(matches!(
        classify_key(&ks("q", ctrl_alt, None), true, KeyContext::default()),
        KeyAction::Send(..)
    ));
}

#[gpui::test]
fn ctrl_c_interrupts(cx: &mut TestAppContext) {
    // Ctrl+C is SIGINT even with a selection: it is never "copy" here.
    assert_eq!(classify(&ks("c", ctrl(), None)), KeyAction::Interrupt);
    assert_eq!(classify(&ks("C", ctrl(), None)), KeyAction::Interrupt);

    let (session, probe) = FakeTerminalSession::boxed(4, 8, "");
    cx.update(|cx| {
        let session = cx.new(|_| session);
        interrupt(&session, cx);
    });
    assert_eq!(probe.writes(), vec![b"\x03".to_vec()]);
}

#[gpui::test]
fn ctrl_space_encodes_nul(cx: &mut TestAppContext) {
    let (spec, mods) = map_key(&ks("space", ctrl(), None)).expect("Ctrl+Space maps");
    assert_eq!(spec, KeySpec::Character(" ".into()));
    assert!(mods.ctrl);

    let (session, probe) = FakeTerminalSession::boxed(4, 8, "");
    cx.update(|cx| {
        let session = cx.new(|_| session);
        assert!(send_key(&session, &spec, mods, false, cx));
    });
    assert_eq!(probe.writes(), vec![vec![0u8]]);
}

#[gpui::test]
fn unmapped_chord_writes_nothing(cx: &mut TestAppContext) {
    // Ctrl + non-ASCII has no encoding; `send_key` reports that nothing went out.
    let spec = KeySpec::Character("é".into());
    let mods = KeyMods {
        ctrl: true,
        ..Default::default()
    };
    assert!(encode_key(&spec, mods, false).is_none());
    let (session, probe) = FakeTerminalSession::boxed(4, 8, "");
    cx.update(|cx| {
        let session = cx.new(|_| session);
        assert!(!send_key(&session, &spec, mods, false, cx));
    });
    assert!(probe.writes().is_empty());
}

#[test]
fn unknown_named_key_is_unhandled() {
    assert_eq!(
        classify(&ks("f25", Modifiers::default(), None)),
        KeyAction::Unhandled
    );
    // Ctrl+PrintScreen: a multi-character name with modifiers is still
    // unencodable and must never be sent as the literal text "print".
    assert_eq!(classify(&ks("print", ctrl(), None)), KeyAction::Unhandled);
}

#[test]
fn alt_c_sends_escape_prefix() {
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    let KeyAction::Send(spec, mods) = classify(&ks("c", alt, None)) else {
        panic!("Alt+C must be sent");
    };
    assert_eq!(spec, KeySpec::Character("c".into()));
    assert!(mods.alt);
    assert_eq!(encode_key(&spec, mods, false), Some(b"\x1bc".to_vec()));
}

#[test]
fn ctrl_shift_c_is_copy_not_send() {
    let chord = if cfg!(target_os = "macos") {
        ks("c", platform(), None)
    } else {
        ks("C", ctrl_shift(), None)
    };
    assert_eq!(classify(&chord), KeyAction::Copy);
    // The chord still maps, so the ordering of the table is what makes it copy.
    assert!(map_key(&chord).is_some());
}

#[test]
fn completion_navigation_forwards_until_selected() {
    let visible = KeyContext {
        completion_visible: true,
        ..KeyContext::default()
    };
    let selected = KeyContext {
        completion_selected: true,
        ..visible
    };
    for (key, mods, expected) in [
        ("down", Modifiers::default(), CompletionKey::SelectNext),
        ("n", ctrl(), CompletionKey::SelectNext),
        ("up", Modifiers::default(), CompletionKey::SelectPrev),
        ("p", ctrl(), CompletionKey::SelectPrev),
    ] {
        let stroke = ks(key, mods, None);
        assert!(
            !matches!(
                classify_key(&stroke, false, visible),
                KeyAction::Completion(_)
            ),
            "{key} must reach the shell while nothing is selected"
        );
        assert_eq!(
            classify_key(&stroke, false, selected),
            KeyAction::Completion(expected),
            "{key} navigates once a suggestion is selected"
        );
    }
    // Escape always dismisses; Enter accepts only with a selection.
    assert_eq!(
        classify_key(&ks("escape", Modifiers::default(), None), false, visible),
        KeyAction::Completion(CompletionKey::Dismiss)
    );
    let enter = ks("enter", Modifiers::default(), Some("\r"));
    assert!(matches!(
        classify_key(&enter, false, visible),
        KeyAction::Send(..)
    ));
    assert_eq!(
        classify_key(&enter, false, selected),
        KeyAction::Completion(CompletionKey::Accept)
    );
}

#[test]
fn completion_tab_select_then_accept() {
    let visible = KeyContext {
        completion_visible: true,
        completion_accept_tab: true,
        ..KeyContext::default()
    };
    let tab = ks("tab", Modifiers::default(), None);
    assert_eq!(
        classify_key(&tab, false, visible),
        KeyAction::Completion(CompletionKey::SelectFirst)
    );
    let selected = KeyContext {
        completion_selected: true,
        ..visible
    };
    assert_eq!(
        classify_key(&tab, false, selected),
        KeyAction::Completion(CompletionKey::Accept)
    );
}

#[test]
fn completion_tab_forwards_when_disabled() {
    let visible = KeyContext {
        completion_visible: true,
        completion_selected: true,
        completion_accept_tab: false,
        ..KeyContext::default()
    };
    assert!(matches!(
        classify_key(&ks("tab", Modifiers::default(), None), false, visible),
        KeyAction::Send(KeySpec::Named(NamedKey::Tab), _)
    ));
}
