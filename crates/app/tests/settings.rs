//! Settings (specs/oneterm, "Settings"): the page writes them, and the app follows them: new
//! spaces, the signals, the tabs' place.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use oneterm::{Launch, NewSession, OneTerm, keymap};
use quadra::{Key, Mods, Role, SemanticAction, Settings, provide};
use quadra_render::{Kind, Step};
use quadra_term::vt::CursorShape;
use quadra_term::{Session, echo};
use quadra_testing::Harness;

struct App {
    h: Harness,
    made: Rc<RefCell<Vec<(Launch, Session)>>>,
    settings: Rc<Settings>,
}

fn app() -> App {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let (made, settings) = (Rc::new(RefCell::new(Vec::new())), Settings::memory());
    let (kept, given) = (made.clone(), settings.clone());
    let sessions: NewSession = Rc::new(move |launch| {
        let s = echo();
        kept.borrow_mut().push((launch, s.clone()));
        s
    });
    h.mount(move || {
        provide(given.clone());
        OneTerm::new(sessions).view()
    });
    h.settle();
    App { h, made, settings }
}

impl App {
    fn feed(&mut self, n: usize, bytes: &[u8]) {
        let s = self.made.borrow()[n].1.clone();
        s.feeder().feed(bytes, Instant::now());
        self.h.settle();
    }

    fn key(&mut self, key: Key, mods: Mods) {
        self.h.key(key, mods);
        self.h.settle();
    }

    fn says(&self, words: &str) -> bool {
        self.h.has_text(words)
    }
}

#[test]
fn the_page_opens_from_the_title_bar_and_writes_what_is_chosen() {
    let mut a = app();
    a.h.get_by_role(Role::Button, "Settings").click();
    a.h.settle();
    assert!(a.says("Appearance"));
    a.h.get_by_role(Role::Radio, "Dark").click();
    a.h.settle();
    assert_eq!(a.settings.value("appearance.theme", String::new()).get(), "Dark");

    a.h.get_by_role(Role::TreeItem, "Tabs & spaces").click();
    a.h.settle();
    assert!(a.says("Tabs & spaces"));
    let switches = a.h.query(|n| n.semantics.role == Role::Switch);
    assert_eq!(switches.len(), 2, "the page's switches: vertical tabs, then a folded rail");
    a.h.tree.act(switches[0], &SemanticAction::Click);
    a.h.settle();
    assert!(!a.settings.value("tabs.vertical", true).get());

    a.h.get_by_role(Role::Button, "Done").click();
    a.h.settle();
    assert!(!a.says("Tabs & spaces"));
}

#[test]
fn a_new_look_keeps_the_screen_and_sizes_the_grid_again() {
    let mut a = app();
    a.feed(0, b"kept");
    let s = a.made.borrow()[0].1.clone();
    let rows = s.size().rows;
    a.settings.value("terminal.font_size", 15.0).set(26.0);
    a.settings.value("terminal.colors.foreground", String::new()).set("#efefef".into());
    a.h.settle();
    assert!(s.size().rows < rows, "fewer rows of larger text: {} then {}", rows, s.size().rows);
    assert_eq!(s.screen_text()[0], "kept", "the same terminal");
}

#[test]
fn the_zoom_keys_size_the_terminals_text() {
    let mut a = app();
    let size = a.settings.value("terminal.font_size", 15.0);
    a.key(Key::Equal, Mods::CTRL);
    a.key(Key::Equal, Mods::CTRL);
    assert_eq!(size.get(), 17.0);
    a.key(Key::Minus, Mods::CTRL);
    assert_eq!(size.get(), 16.0);
    a.key(Key::Digit0, Mods::CTRL);
    assert_eq!(size.get(), 15.0);
}

#[test]
fn new_spaces_take_the_cursor_and_the_history_set() {
    let mut a = app();
    a.settings.value("terminal.cursor", String::new()).set("Bar".into());
    a.settings.value("terminal.scrollback", 0.0).set(500.0);
    a.h.settle();
    a.key(Key::T, Mods::CTRL);
    let made = a.made.borrow();
    let (launch, _) = made.last().expect("a new tab's space");
    assert_eq!((launch.cursor.shape, launch.scrollback), (CursorShape::Beam, 500));
}

#[test]
fn the_default_shell_can_be_a_program_typed() {
    let mut a = app();
    a.settings.value("terminal.shell", String::new()).set("Custom program".into());
    a.settings.value("terminal.shell_program", String::new()).set("D:\\tools\\nu.exe".into());
    a.settings.value("terminal.shell_args", String::new()).set("--login  -i".into());
    a.h.settle();
    a.key(Key::T, Mods::CTRL);
    let made = a.made.borrow();
    let shell = made.last().and_then(|(launch, _)| launch.shell.clone()).expect("the program typed");
    assert_eq!(
        (shell.program, shell.args),
        ("D:\\tools\\nu.exe".into(), vec!["--login".into(), "-i".into()])
    );
}

#[test]
fn the_font_family_is_chosen_from_the_machines_with_a_search() {
    let mut a = app();
    a.h.get_by_role(Role::Button, "Settings").click();
    a.h.settle();
    a.h.get_by_role(Role::Button, "Font family").click();
    a.h.settle();
    a.h.type_text("fake");
    a.h.settle();
    a.key(Key::Enter, Mods::NONE);
    assert_eq!(a.settings.value("terminal.font_family", String::new()).get(), "Fake Sans");
}

#[test]
fn silent_notifications_and_ignored_bells_count_nothing() {
    let mut a = app();
    a.settings.value("signals.notifications", String::new()).set("Off".into());
    a.settings.value("signals.bell", String::new()).set("Off".into());
    a.h.settle();
    a.feed(0, b"\x1b]9;tests passed\x07");
    assert!(!a.says("tests passed"));
    assert!(a.says("0 bells and notices"));
    a.key(Key::D, Mods::CTRL);
    a.feed(0, b"\x07");
    assert!(a.says("0 bells and notices"), "a bell the settings ignore is seen as it rings");
}

#[test]
fn new_local_shells_are_logged_where_the_settings_say() {
    let mut a = app();
    let folder = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.tmp/oneterm-logs-test");
    _ = std::fs::remove_dir_all(&folder);
    a.settings.value("logging.folder", String::new()).set(folder.display().to_string());
    a.settings.value("logging.local", false).set(true);
    a.h.settle();
    a.key(Key::T, Mods::CTRL);
    let n = a.made.borrow().len() - 1;
    assert!(a.made.borrow()[n].1.logging());
    a.feed(n, b"\x1b[1mhello\x1b[0m\r\n");
    a.made.borrow()[n].1.stop_log();
    let files: Vec<_> = std::fs::read_dir(&folder).unwrap().flatten().map(|it| it.path()).collect();
    assert_eq!(files.len(), 1);
    let name = files[0].file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("local_") && name.ends_with(".log"), "{name}");
    let text = std::fs::read_to_string(&files[0]).unwrap();
    assert!(text.starts_with('[') && text.ends_with("] hello\n"), "{text:?}");
}

#[test]
fn the_look_page_sets_the_room_the_cells_the_contrast_the_ansi_colours_and_the_fallbacks() {
    let mut a = app();
    let s = a.made.borrow()[0].1.clone();
    let cols = s.size().cols;
    a.h.get_by_role(Role::Button, "Settings").click();
    a.h.settle();
    for words in
        ["Fallback fonts", "Cell width", "Padding top", "Padding left", "Minimum contrast", "ANSI colours"]
    {
        assert!(a.says(words), "{words}");
    }
    // More room at the left: fewer columns, a cell's worth for each 8 or 9 px.
    a.h.get_by_test_id("padding.left").set_value("110");
    a.h.settle();
    assert_eq!(a.settings.value("terminal.padding.left", 0.0).get(), 110.0);
    let roomier = s.size().cols;
    assert!(roomier < cols && roomier + 20 > cols, "{cols} then {roomier}");
    a.h.get_by_test_id("cell-width").set_value("20");
    a.h.settle();
    assert!(s.size().cols < roomier / 2, "wider cells: {roomier} then {}", s.size().cols);
    a.h.get_by_role(Role::Button, "Bright red").click(); // the colour picker
    a.h.settle();
    a.h.get_by_role(Role::TextInput, "Hex").set_value("#400000");
    a.h.key(Key::Escape, Mods::NONE);
    a.h.settle();
    a.h.get_by_role(Role::TextInput, "Fallback fonts").set_value("Cascadia Code, Segoe UI Symbol");
    a.h.settle();
    assert_eq!(a.settings.value("terminal.colors.ansi.bright_red", String::new()).get(), "#400000");
    a.h.get_by_role(Role::Button, "Done").click();
    a.h.settle();

    let families = a.h.tree.fonts().borrow().text.fallbacks().to_vec();
    assert_eq!(families, ["Cascadia Code", "Segoe UI Symbol"]);
    a.feed(0, b"\x1b[91mX");
    // A dark red on the dark canvas: lightened to the contrast of 4.5, then as it is with 1.
    let dark_red = quadra::Color::rgb(0x400000).to_array();
    assert!(!painted(&mut a.h, dark_red), "moved to the contrast");
    a.h.get_by_role(Role::Button, "Settings").click();
    a.h.settle();
    a.h.get_by_test_id("min-contrast").set_value("1");
    a.h.get_by_role(Role::Button, "Done").click();
    a.h.settle();
    assert!(painted(&mut a.h, dark_red), "a glyph in the bright red set");
}

/// Whether a glyph is drawn in `color`.
fn painted(h: &mut Harness, color: [f32; 4]) -> bool {
    h.frame();
    let scene = h.tree.scene();
    h.renderer.steps().iter().any(|step| match step {
        Step::Layer { id, .. } => {
            scene.layer(*id).instances().iter().any(|i| i.kind == Kind::Mono as u32 && i.color == color)
        }
        _ => false,
    })
}

/// The colours of the glyphs in the first terminal, a row after another from the left.
fn terminal_inks(h: &mut Harness) -> Vec<[f32; 4]> {
    h.frame();
    let node = h.query(|n| n.semantics.role == Role::TextInput && n.semantics.name == "Terminal")[0];
    let rect = h.tree.window_rect(node);
    let scene = h.tree.scene();
    let mut inks: Vec<([f32; 4], [f32; 4])> = h
        .renderer
        .steps()
        .iter()
        .filter_map(|step| match step {
            Step::Layer { id, .. } => Some(scene.layer(*id).instances()),
            _ => None,
        })
        .flatten()
        .filter(|i| {
            i.kind == Kind::Mono as u32 && rect.contains(quadra::Point::new(i.bounds[0], i.bounds[1]))
        })
        .map(|i| (i.bounds, i.color))
        .collect();
    inks.sort_by(|a, b| a.0[1].total_cmp(&b.0[1]).then(a.0[0].total_cmp(&b.0[0])));
    inks.into_iter().map(|it| it.1).collect()
}

#[test]
fn ligatures_grayscale_and_semantic_highlighting_are_on_and_follow_the_page() {
    let mut a = app();
    // Errors take the ANSI red, here one set, as it is.
    a.settings.value("terminal.colors.ansi.red", String::new()).set("#ff0000".into());
    a.settings.value("terminal.colors.min_contrast", 4.5).set(1.0);
    a.feed(0, b"error plain");
    let red = quadra::Color::rgb(0xFF0000).to_array();
    let inks = terminal_inks(&mut a.h);
    assert_eq!((inks[0], inks[5] == red), (red, false), "`error` in red, `plain` in the foreground");
    a.h.get_by_role(Role::Button, "Settings").click();
    a.h.settle();
    assert!(a.says("Ligatures") && a.says("Grayscale smoothing") && a.says("Semantic highlighting"));
    let switches = a.h.query(|n| n.semantics.role == Role::Switch);
    assert_eq!(switches.len(), 4, "the gutter, ligatures, grayscale, semantic highlighting");
    for on in &switches[1..] {
        a.h.tree.act(*on, &SemanticAction::Click);
    }
    a.h.get_by_role(Role::Button, "Done").click();
    a.h.settle();
    assert!(!a.settings.value("terminal.ligatures", true).get());
    assert!(!a.settings.value("terminal.grayscale", true).get());
    assert!(!a.settings.value("terminal.semantic_highlighting", true).get());
    let inks = terminal_inks(&mut a.h);
    assert_eq!(inks[0], inks[5], "off: both in the foreground");
}

#[test]
fn the_gutter_s_line_number_is_teal_until_set() {
    let look = oneterm::Prefs::defaults().look();
    assert_eq!(look.colors.line_number_fg, quadra::Color::parse_hex("#2b7f99"));
    assert_eq!(look.colors.gutter_fg, None, "the rest of the gutter follows the theme");
}
