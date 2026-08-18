# Gap Analysis: OneTerm Terminal vs Zed Terminal

> **Status:** Historical gap analysis. Current terminal ownership and paths are documented in [`docs/architecture.md`](architecture.md).

> Created: 2025-07-14
> Zed reference: `zed-industries/zed` @ commit `20a3f770` (main branch)
> OneTerm reference: `crates/core/src/terminal/` + `crates/ui/src/views/terminal/`

---

## Overview

| Aspect | Zed | OneTerm | Completion level |
|---|---|---|---|
| Backend (PTY + Term) | `alacritty_terminal` + `EventLoop` | `alacritty_terminal` + `EventLoop` | ≈ 90% (Local) |
| Rendering (Element) | `TerminalElement` (GPUI) | `TerminalElement` (GPUI) + cursor blink/shape/selection inverse/bell | ≈ 90% |
| View (Input + IME) | `TerminalView` | `LocalTerminalView` | ≈ 50% |
| Panel / Workspace | `TerminalPanel` (Dock + Tabs) | `TerminalPanel` (1 session) | ≈ 25% |
| Search | `SearchableItem` trait | ❌ | 0% |
| Shell Integration | OSC 7/133 + `PtyProcessInfo` | OSC 7 (cwd) only | ≈ 20% |
| Task Integration | `TaskState` + rerun | ❌ | 0% |
| Settings | `TerminalSettings` (full) | Shell picker only | ≈ 10% |

---

## Group A — Rendering & Display

| ✅/❌ | Gap | Zed has | OneTerm has | Description / Example |
|---|---|---|---|---|
| ✅ | **Cursor blink** | Yes — `BlinkManager` entity, `CURSOR_BLINK_INTERVAL = 500ms`, settings `TerminalBlink::On/Off/TerminalControlled` | Yes — `cursor_blink_visible` toggle 500ms, `TerminalBlink::On/Off` setting | ✅ Filled. Cursor blinks twice per second when focused. Setting `cursor_blink: On/Off`. |
| ✅ | **Cursor shape config** | Yes — `CursorShape::Block/Bar/Underline`, user picks via settings | Yes — `TerminalCursorShape::Block/Bar/Underline` setting, paint by shape | ✅ Filled. Paint Block (full), Beam (vertical bar 20% width), Underline (underline 15% height). Setting `cursor_shape`. |
| ✅ | **Selection inverse video** | Yes — selected text swaps fg/bg (inverse) | Yes — swap fg/bg for cells in selection (text uses selection bg color) | ✅ Filled. Build `HashSet<LayoutPoint>` from selection rects, swap fg→selection color for cells in selection. |
| ❌ | **Image protocol (Sixel/iTerm2/Kitty)** | Yes — supports iTerm2 inline image via `repl` crate | No | Zed: `cat image.png` via `imgcat` shows inline image. OneTerm: only sees garbage escape sequences. |
| ✅ | **Font features / ligatures** | Yes — `Font` with `features` field, settings for ligatures | Yes — `font_features: Vec<SharedString>` setting, passed to `FontFeatures` | ✅ Filled. Setting `font_features: ["calt", "liga"]` → enables ligatures. Default empty (off). |
| ❌ | **DIM color (half-bright)** | Yes | Yes (alpha 0.7) | Both handle `Flags::DIM`. ✅ Already present. |
| ✅ | **Bold/Italic rendering** | Yes | Yes | Both render bold/italic via `FontWeight`/`FontStyle`. |
| ✅ | **Underline (straight/curly/dotted)** | Yes | Yes | `UNDERCURL` → wavy underline. Both support. |
| ✅ | **Strikethrough** | Yes | Yes | `Flags::STRIKEOUT` → strikethrough. Both support. |
| ✅ | **Wide char / CJK** | Yes | Yes | `WIDE_CHAR_SPACER` skip + zerowidth append. Both support. |
| ✅ | **Min contrast** | Yes — `ensure_minimum_contrast` | Yes — WCAG 4.5 | Both ensure fg/bg contrast. |
| ✅ | **Terminal bell indicator** | Yes — `has_bell` flag, show 🔔 in tab | Yes — `SessionEvent::Bell`, `has_bell` flag, 🔔 overlay top-right corner | ✅ Filled. `Event::Bell` forwarded via `SessionEvent::Bell` → view sets `has_bell=true` → 🔔 overlay. Cleared when user presses a key. Setting `bell_enabled`. |

---

## Group B — Selection & Clipboard

| ✅/❌ | Gap | Zed has | OneTerm has | Description / Example |
|---|---|---|---|---|
| ✅ | **Simple selection (click-drag)** | Yes | Yes (fixed) | Drag mouse to select text. **Fixed:** separate `mouse_drag` from `mouse_move` + skip resize when size unchanged (avoid shell redraw clearing selection). |
| ✅ | **Semantic selection (double-click)** | Yes | Yes (fixed) | Double-click selects word. `SelectionType::Semantic`. |
| ✅ | **Line selection (triple-click)** | Yes | Yes (fixed) | Triple-click selects entire line. `SelectionType::Lines`. |
| ✅ | **Block selection (Alt+drag)** | Yes | Yes (fixed) | Alt+drag selects rectangular block. `SelectionType::Block`. |
| ✅ | **Select-to-copy** | Yes | Yes (fixed) | Select → auto-copy to clipboard. Selection highlight uses Zed blue (`#0d2847` dark / `#e6f4fe` light), text keeps original color (no inverse video). |
| ✅ | **Middle-click paste** | Yes | Yes | Middle-click paste clipboard (X11 style). |
| ✅ | **Ctrl+Shift+C / Ctrl+Shift+V** | Yes | Yes | Copy/paste keyboard shortcut. |
| ✅ | **Select All** | Yes | Yes | Right-click menu → Select All. |
| ✅ | **Right-click context menu** | Yes (richer) | Yes (basic: Copy/Paste/Select All/Clear) | Zed adds: New Terminal, Inline Assist, Close Tab. OneTerm: 4 basic items. |
| ❌ | **Paste image as Ctrl+V** | Yes — detects `ClipboardEntry::Image` → sends Ctrl+V | No | Zed: copy image → paste into terminal → sends raw Ctrl+V. OneTerm: paste image → does nothing. |
| ❌ | **Drag-and-drop file paths** | Yes — `ExternalPaths` → quote path → write to PTY | No | Zed: drag file from Finder into terminal → path auto-quoted. OneTerm: doesn't accept dragged files. **Example**: Drag `main.rs` into terminal → Zed auto `/path/to/main.rs`. |
| ❌ | **Copy with metadata** | Yes — `CopyTemplate` + `task` info | No | Zed: copy includes task info when running a task. |

---

## Group C — Scrolling

| ✅/❌ | Gap | Zed has | OneTerm has | Description / Example |
|---|---|---|---|---|
| ✅ | **Mouse wheel scroll** | Yes | Yes | Wheel up/down scrolls scrollback. |
| ✅ | **Scrollbar (auto-hide)** | Yes — `TerminalScrollHandle` | Yes (fixed) | Scrollbar always visible when there is scrollback (ScrollbarShow::Always). Drag scrollbar thumb → jump to position. |
| ✅ | **Scrollbar drag** | Yes | Yes | Drag scrollbar thumb → jump to position. |
| ✅ | **Scroll keyboard actions** | Yes — `ScrollLineUp/Down`, `ScrollPageUp/Down`, `ScrollHalfPageUp/Down`, `ScrollToTop`, `ScrollToBottom` | Yes (fixed) | Shift+PageUp/Down: scroll 1 viewport. Shift+Home/End: scroll to top/bottom. Ctrl+Shift+Up/Down: scroll 1 line. |
| ✅ | **Scroll-to-top / Scroll-to-bottom** | Yes — action `ScrollToTop/Bottom` | Yes (fixed) | Shift+Home → scroll_to_top, Shift+End → scroll_to_bottom. |
| ✅ | **Scroll multiplier setting** | Yes — `scroll_multiplier` in TerminalSettings | Yes (fixed) | Setting `scroll_multiplier: f32` (default 1.0). Mouse wheel delta × multiplier. |
| ✅ | **Alternate scroll mode toggle** | Yes — setting `alternate_scroll` | Yes (fixed) | Setting `alternate_scroll: bool` (default true). Alacritty handles alt-screen mouse scroll itself. |

---

## Group D — Search

| ✅/❌ | Gap | Zed has | OneTerm has | Description / Example |
|---|---|---|---|---|
| ❌ | **In-terminal search** | Yes — `SearchableItem` trait, `SearchEvent`, `SearchQuery` | No | Zed: Cmd+F → search bar appears, type "error" → highlights all matches. OneTerm: no search. **Example**: `cargo build` outputs 1000 lines → Cmd+F "warning" → Zed highlights all. OneTerm: must read by eye. |
| ❌ | **Search highlight (matches)** | Yes — `matches: Vec<RangeInclusive<AlacPoint>>` | No | Zed: matches shown yellow, current match orange. |
| ❌ | **Search navigation (next/prev)** | Yes — `Direction::Next/Prev` | No | Zed: Enter → next match, Shift+Enter → prev. |
| ❌ | **Search options (case, regex)** | Yes — `SearchOptions` | No | Zed: toggle case-sensitive, regex, whole word. |
| ❌ | **Search wrap-around** | Yes | No | Zed: search reaches end → wraps to beginning. |

---

## Group E — Hyperlinks & Navigation

| ✅/❌ | Gap | Zed has | OneTerm has | Description / Example |
|---|---|---|---|---|
| ✅ | **OSC 8 hyperlink** | Yes | Yes | `cell.hyperlink()` → Ctrl+click opens URL. |
| ✅ | **Plain-text URL detection** | Yes | Yes (terminal-view url detection) | `https://example.com` in output → Ctrl+click opens. |
| ✅ | **Ctrl+click open URL** | Yes | Yes | Ctrl+click on URL → opens browser. |
| ❌ | **Path-like hyperlink (file:line)** | Yes — `hover_path_like_target`, `open_path_like_target`, regex for `file.rs:42` | No | Zed: hover `src/main.rs:42` → tooltip shows path, Ctrl+click opens file at line 42. OneTerm: URLs only. **Example**: Compiler output `error at src/main.rs:42:10` → Zed: click opens editor. OneTerm: plain text. |
| ❌ | **Hover tooltip** | Yes — `HoverTarget { tooltip, hovered_word }` | No | Zed: hover URL/path → tooltip shows full URL/path. OneTerm: no tooltip. |
| ❌ | **Hover underline** | Yes — underline URL on hover | No | Zed: hover URL → URL underlined. OneTerm: URL has no visual feedback on hover. |
| ❌ | **File path detection (relative)** | Yes — detect relative path + resolve against cwd | No | Zed: `./src/main.rs` → Ctrl+click opens. OneTerm: no detection. |

---

## Group F — Shell Integration

| ✅/❌ | Gap | Zed has | OneTerm has | Description / Example |
|---|---|---|---|---|
| ✅ | **OSC 7 (cwd)** | Yes | Yes (fixed) | `parse_cwd_url` → update cwd. **Wired up**: custom `ShellEventLoop` feeds PTY bytes to both `ansi::Processor` (Term) AND `vte::Parser` (OscSink) in parallel → OscSink catches OSC 7 → updates `SessionState.cwd` → forwards `SessionEvent::Cwd`. |
| ✅ | **OSC 52 (clipboard)** | Yes | Yes | `decode_osc52`/`encode_osc52` → clipboard set/get. |
| ✅ | **OSC 0/2 (title)** | Yes | Yes | Title change → tab title update. |
| ✅ | **OSC 133 (shell integration)** | Yes | Yes (fixed) | Custom `ShellEventLoop` parses OSC 133 markers (A=prompt start, B=prompt end, C=output start, D;exit_code=output end). Shell integration script auto-injected for PowerShell/Bash/Zsh. Markers track prompt boundaries + exit codes. |
| ⚡ | **Foreground process detection** | Yes — `PtyProcessInfo` (sysinfo + pgid) | Partial | Tab title uses OSC 0/2 (title) + OSC 133 markers (command running vs. prompt). No `sysinfo`-based process tree detection yet. |
| ⚡ | **Shell environment detection** | Yes — `ProjectEnvironment`, `capture_unix/windows`, `zed --printenv` | No | Zed: spawn shell in login mode → capture env JSON → inject into terminal. OneTerm: inherits env directly. Shell integration script injects instead of capturing env. |
| ⚡ | **ShellBuilder (quoting/escaping)** | Yes — `ShellKind::Posix/Fish/Nushell/PowerShell`, `format_task_for_activation` | Partial | `ShellKind` enum (Cmd/PowerShell/Pwsh/Bash/Zsh/Sh/Custom) + shell-specific integration script injection. No `format_task_for_activation` yet. |
| ✅ | **Activation script** | Yes — `activation_script: Vec<String>` | Yes (fixed) | `shell_integration_script(kind)` → auto-inject OSC 7 + OSC 133 script into PTY after spawn. PowerShell: override `prompt` function. Bash/Zsh: precmd/preexec hooks. Cmd: `prompt` command. |
| ✅ | **Breadcrumb text** | Yes — `breadcrumb_text: String`, shown in toolbar | Yes (fixed) | `TerminalPanel::breadcrumb_label()` formats `TerminalSession::cwd()` (OSC 7); the session trait carries no presentation methods (ARCH-02). Shown in the StatusBar breadcrumb widget. |

---

## Group G — Task Integration

| ✅/❌ | Gap | Zed has | OneTerm has | Description / Example |
|---|---|---|---|---|
| ❌ | **Task system** | Yes — `TaskState`, `task: spawn`, `task: rerun` | No | Zed: define task in `tasks.json` → Cmd+Shift+T → spawn. OneTerm: none. **Example**: Task `cargo test` → Zed spawns terminal, auto-runs, shows status. |
| ❌ | **Task rerun** | Yes — `RerunTask` action | No | Zed: Cmd+Alt+R → rerun last task. |
| ❌ | **Task status tracking** | Yes — `TaskStatus::Running/Completed/Failed` | No | Zed: task tab shows ✓ (success) or ✗ (failure). |
| ❌ | **Task reveal/hide config** | Yes — `reveal: always/no_focus/never`, `hide: never/always/on_success` | No | Zed: config when to show/hide terminal tab. |
| ❌ | **Show command/summary** | Yes — `show_summary`, `show_command` | No | Zed: task output shows command line + summary. |

---

## Group H — Input & IME

| ✅/❌ | Gap | Zed has | OneTerm has | Description / Example |
|---|---|---|---|---|
| ✅ | **IME marked text (pre-edit)** | Yes — `ImeState { marked_text }` | Yes (fixed) | `set_marked_text`/`clear_marked_text`/`commit_text`. |
| ✅ | **IME commit** | Yes | Yes | `replace_text_in_range` → `commit_text`. |
| ✅ | **Alt-screen IME toggle** | Yes | Yes | Alt-screen → disable IME (`selected_text_range` → None). |
| ✅ | **Keyboard mapping (arrows, F-keys)** | Yes | Yes | `key_encode.rs` → escape sequences. |
| ✅ | **Mouse mode encoding** | Yes | Yes | `mouse_encode.rs` → SGR/normal/X10 encoding. |
| ✅ | **Vi mode** | Yes — `ToggleViMode`, `ViMotion::Left/Right/Up/Down/WordRight/WordLeft` | Yes (fixed) | Ctrl+Shift+Space → toggle vi mode. hjkl/arrows navigate, v select, y yank, w/b word jump, gg/G top/bottom, 0/$ line start/end, q quit. Vi cursor overlay + indicator. |
| ⚡ | **Character palette** | Yes — `ShowCharacterPalette` action | No | Zed: Cmd+Ctrl+Space → character palette (emoji picker). Platform-specific (macOS NSPasteboard). Windows uses native Win+.. |
| ✅ | **Send text action** | Yes — `SendText(String)` action | Yes (fixed) | `TerminalSession::send_text(text)` — writes raw text to PTY. Default impl on trait. |
| ✅ | **Send keystroke action** | Yes — `SendKeystroke(String)` | Yes (fixed) | `TerminalSession::send_keystroke(keystroke)` — parse format `Ctrl+C`/`Alt+Enter`/`Up` → encode → write PTY. `parse_keystroke()` public function. |
| ✅ | **Bracketed paste detection** | Yes — `Modes::BRACKETED_PASTE` | Yes (fixed) | `TerminalSession::is_bracketed_paste()` checks `TermMode::BRACKETED_PASTE`. `paste(text)` auto-wraps in `\x1b[200~...\x1b[201~`. All paste paths (middle-click, Ctrl+Shift+V, context menu) use `paste()`. |

---

## Group I — Panel & Workspace

| ✅/❌ | Gap | Zed has | OneTerm has | Description / Example |
|---|---|---|---|---|
| ✅ | **Dock panel** | Yes — `TerminalPanel` impl `Panel` | Yes | Terminal in dock panel (bottom/side). |
| ❌ | **Multiple terminal tabs** | Yes — `Pane` manages multiple `TerminalView` | No — 1 session/panel | Zed: create multiple terminals, each a tab in pane. OneTerm: single terminal. **Example**: Zed: "+" button → new terminal tab. OneTerm: none. |
| ❌ | **Terminal rename** | Yes — `RenameTerminal` action, inline `Editor` | No | Zed: right-click tab → Rename → type name. OneTerm: none. |
| ❌ | **Terminal persistence** | Yes — `TerminalDb`, `SerializableItem`, `WorkspaceId` | No | Zed: restore terminal tabs when reopening workspace. OneTerm: terminal lost on close. |
| ❌ | **New terminal button in tab bar** | Yes — `NewTerminal`, `NewCenterTerminal` buttons | No | Zed: tab bar has "+" button to create new terminal. |
| ❌ | **Block below cursor (inline blocks)** | Yes — `BlockProperties { height, render }` | No | Zed: Agent panel inserts UI block below cursor (e.g. inline prompt). OneTerm: none. **Example**: Agent → 3-line block "Press Enter to continue" pinned below cursor. |
| ❌ | **Embedded mode** | Yes — `TerminalMode::Embedded { max_lines_when_unfocused }` | No | Zed: terminal inline in editor (Agent panel output). OneTerm: standalone only. |
| ❌ | **Scroll state for blocks** | Yes — `scroll_top: Pixels`, `max_scroll_top` | No | Zed: scroll block content separately when `block_below_cursor` exists. |

---

## Group J — Settings & Configuration

| ✅/❌ | Gap | Zed has | OneTerm has | Description / Example |
|---|---|---|---|---|
| ✅ | **Shell program config** | Yes — `terminal.shell: { program, args }` | Yes — `LocalShellConfig { kind, program, args, cwd }` | Both allow choosing shell. |
| ✅ | **Working directory** | Yes — `WorkingDirectory` setting | Yes — `cwd` in config | Both set startup directory. |
| ❌ | **Cursor shape setting** | Yes — `cursor_shape: Block/Bar/Underline` | No | Zed: settings.json → `"cursor_shape": "bar"`. |
| ❌ | **Cursor blink setting** | Yes — `blinking: On/Off/TerminalControlled` | No | Zed: `"blinking": "off"` → cursor doesn't blink. |
| ❌ | **Font config (family/size)** | Yes — `font_family`, `font_size`, `font_features` | Partial — inherits from theme | Zed: `"terminal": { "font_family": "JetBrains Mono" }`. OneTerm: uses theme mono font. |
| ❌ | **Scrollback history config** | Yes — `scrollback_history` setting | Hardcoded — 10,000 lines | Zed: `"scrollback_history": 50000`. OneTerm: fixed 10,000. |
| ❌ | **Scroll multiplier** | Yes — `scroll_multiplier: f32` | No | Zed: `"scroll_multiplier": 3.0`. |
| ❌ | **Toolbar breadcrumbs** | Yes — `toolbar: { breadcrumbs: bool }` | No | Zed: `"toolbar": { "breadcrumbs": true }` → toolbar shows cwd path. |
| ❌ | **Bell setting** | Yes — `bell: System/On/Off` | No | Zed: `"bell": "off"` → disables bell. |
| ❌ | **Alternate scroll** | Yes — `alternate_scroll: bool` | No | Zed: `"alternate_scroll": false` → disables mouse scroll in alt-screen. |
| ❌ | **Option as Meta** | Yes — `option_as_meta: bool` | No | Zed: macOS → Option key = Meta (Alt). OneTerm: no config. |
| ❌ | **Custom shell arguments** | Yes — `with_arguments: { program, args }` | Yes (in config) | Both support args. ✅ Already present. |
| ❌ | **Environment variables injection** | Yes — `env: { KEY: value }` | No | Zed: `"env": { "MY_VAR": "value" }` → inject into terminal. OneTerm: inherit env. |
| ❌ | **Path hyperlink regexes** | Yes — `path_hyperlink_regexes`, `path_hyperlink_timeout` | No | Zed: custom regex for file path detection. |

---

## Group K — Architecture & Backend

| ✅/❌ | Gap | Zed has | OneTerm has | Description / Example |
|---|---|---|---|---|
| ✅ | **alacritty_terminal backend** | Yes | Yes | Both use `alacritty_terminal::Term` + `EventLoop`. |
| ✅ | **FairMutex concurrency** | Yes | Yes | `Arc<FairMutex<Term<EP>>>`. |
| ✅ | **Snapshot rendering** | Yes | Yes | `TerminalContent` snapshot → render without lock. |
| ✅ | **Batched text runs** | Yes | Yes | Group adjacent cells with same style → 1 text shape call. |
| ✅ | **Local terminal (ConPTY/Unix)** | Yes | Yes | Windows ConPTY + unix pty. |
| ❌ | **SSH terminal** | Yes — remote terminal via SSH | Designed, not implemented | Zed: `is_remote_terminal: bool`, remote PTY via SSH. OneTerm: `SshSession` designed (`docs/terminal-backend.md`) but not implemented. |
| ❌ | **TerminalBuilder (2-step init)** | Yes — `TerminalBuilder::new()` → check → `subscribe()` | No — `LocalSession::spawn` 1 step | Zed: separate init to handle failure gracefully. OneTerm: `expect("spawn")` → panic on fail. **Example**: PTY fails → Zed shows error view. OneTerm: crashes. |
| ❌ | **CopyTemplate (shell context)** | Yes — `CopyTemplate { shell }` | No | Zed: stores shell context for copy/paste formatting. |
| ❌ | **Input log (test support)** | Yes — `input_log: Vec<Vec<u8>>` | No | Zed: logs input for test verification. |
| ❌ | **Event coalescing (VecDeque)** | Yes — `events: VecDeque<InternalEvent>` | Partial — drain Output events | Zed: queues `InternalEvent` coalesce. OneTerm: drains channel in spawn task. |

---

## Group L — Mouse & Interaction

| ✅/❌ | Gap | Zed has | OneTerm has | Description / Example |
|---|---|---|---|---|
| ✅ | **Mouse mode encoding** | Yes | Yes | SGR/normal/X10 mouse encoding. |
| ✅ | **Mouse motion/drag tracking** | Yes | Yes | `MOUSE_MOTION`, `MOUSE_DRAG` mode. |
| ✅ | **Wheel event encoding** | Yes | Yes | Wheel → SGR mouse or arrow keys. |
| ❌ | **Selection phase tracking** | Yes — `SelectionPhase` enum | No | Zed: tracks selection state (start/update/end) to handle clicks precisely. |
| ❌ | **Mouse-down hyperlink tracking** | Yes — `mouse_down_hyperlink: Option<(String, bool, Match)>` | No | Zed: stores hyperlink on mouse-down → Ctrl+click only opens if same link. |
| ❌ | **Last mouse move time (debounce)** | Yes — `last_mouse_move_time: Instant` | No | Zed: debounces hover detection. |
| ❌ | **Hyperlink search caching** | Yes — `last_hyperlink_search_position`, `hyperlink_regex_searches: RegexSearches` | No | Zed: caches regex search results for fast hover. OneTerm: searches every frame. |

---

## Summary Statistics

| Group | Total gaps | Present (✅) | Missing (❌) | % complete |
|---|---|---|---|---|
| A — Rendering & Display | 11 | 10 | 1 | 91% |
| B — Selection & Clipboard | 12 | 8 | 4 | 67% |
| C — Scrolling | 7 | 7 | 0 | 100% |
| D — Search | 5 | 0 | 5 | 0% |
| E — Hyperlinks & Navigation | 7 | 3 | 4 | 43% |
| F — Shell Integration | 9 | 6 | 3 | 67% |
| G — Task Integration | 5 | 0 | 5 | 0% |
| H — Input & IME | 10 | 9 | 1 | 90% |
| I — Panel & Workspace | 9 | 1 | 8 | 11% |
| J — Settings & Configuration | 16 | 3 | 13 | 19% |
| K — Architecture & Backend | 10 | 5 | 5 | 50% |
| L — Mouse & Interaction | 7 | 3 | 4 | 43% |
| **Total** | 108 | **55** | **53** | **51%** |

---

## Suggested Priorities (Roadmap)

### P0 — Must fix now (basic experience)
1. ✅ ~~Fix selection highlight~~ (fixed)
2. ✅ ~~Fix scrollbar visibility~~ (fixed)
3. ✅ ~~Selection inverse video~~ (filled — text changes color when selected)
4. ✅ ~~Cursor blink~~ (filled — 500ms toggle, On/Off setting)
5. ✅ ~~Cursor shape config~~ (filled — Block/Beam/Underline)
6. ✅ ~~Terminal bell indicator~~ (filled — 🔔 overlay + clear on input)
7. ✅ ~~Font features/ligatures~~ (filled — setting + FontFeatures)
8. Multiple terminal tabs ("+" button to create new terminal)
9. TerminalBuilder (graceful error instead of panic)

### P1 — Important (basic terminal parity)
7. In-terminal search (Cmd+F)
8. Cursor shape config
9. Font config (family/size specific to terminal)
10. Scrollback history config
11. ✅ ~~Scroll keyboard actions (ScrollLineUp/Down, PageUp/Down scrollback)~~ (filled — Shift+PageUp/Down, Shift+Home/End, Ctrl+Shift+Up/Down)
12. Terminal rename
13. Path-like hyperlink (file:line → open editor)
14. Hover tooltip + underline for hyperlinks

### P2 — Nice to have (shell integration)
15. OSC 133 shell integration markers
16. Foreground process detection (dynamic tab title)
17. Shell environment detection
18. Vi mode
19. Drag-and-drop file paths
20. Bell indicator

### P3 — Future (full parity with Zed)
21. Task system integration
22. Terminal persistence (restore tabs)
23. Image protocol (iTerm2/Sixel)
24. Block below cursor (inline blocks)
25. Embedded mode (inline terminal)
26. SSH terminal implementation
27. Font ligatures
28. Send text/keystroke actions
29. Character palette
30. Environment variables injection