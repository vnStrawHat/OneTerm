# Phân tích Gap: Terminal myTerm2 vs Zed Terminal

> Ngày tạo: 2025-07-14
> Tham chiếu Zed: `zed-industries/zed` @ commit `20a3f770` (main branch)
> Tham chiếu myTerm2: `crates/core/src/terminal/` + `crates/ui/src/views/terminal/`

---

## Tổng quan

| Khía cạnh | Zed | myTerm2 | Mức độ hoàn thành |
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

## Nhóm A — Rendering & Display

| ✅/❌ | Gap | Zed có | myTerm2 có | Mô tả / Ví dụ |
|---|---|---|---|---|
| ✅ | **Cursor blink** | Có — `BlinkManager` entity, `CURSOR_BLINK_INTERVAL = 500ms`, settings `TerminalBlink::On/Off/TerminalControlled` | Có — `cursor_blink_visible` toggle 500ms, `TerminalBlink::On/Off` setting | ✅ Đã lấp. Cursor nhấp nháy 2 lần/giây khi focus. Setting `cursor_blink: On/Off`. |
| ✅ | **Cursor shape config** | Có — `CursorShape::Block/Bar/Underline`, user chọn qua settings | Có — `TerminalCursorShape::Block/Bar/Underline` setting, paint theo shape | ✅ Đã lấp. Paint Block (đầy), Beam (thanh dọc 20% width), Underline (gạch dưới 15% height). Setting `cursor_shape`. |
| ✅ | **Selection inverse video** | Có — selected text đổi fg/bg (inverse) | Có — swap fg/bg cho cell trong selection (text dùng selection bg color) | ✅ Đã lấp. Build `HashSet<LayoutPoint>` từ selection rects, swap fg→selection color cho cell trong selection. |
| ❌ | **Image protocol (Sixel/iTerm2/Kitty)** | Có — hỗ trợ iTerm2 inline image qua `repl` crate | Không | Zed: `cat image.png` qua `imgcat` hiện ảnh inline. myTerm2: chỉ thấy escape sequences rác. |
| ✅ | **Font features / ligatures** | Có — `Font` với `features` field, settings cho ligatures | Có — `font_features: Vec<SharedString>` setting, pass vào `FontFeatures` | ✅ Đã lấp. Setting `font_features: ["calt", "liga"]` → bật ligatures. Mặc định rỗng (tắt). |
| ❌ | **DIM color (half-bright)** | Có | Có (alpha 0.7) | Cả hai đều xử lý `Flags::DIM`. ✅ Đã có. |
| ✅ | **Bold/Italic rendering** | Có | Có | Cả hai render bold/italic qua `FontWeight`/`FontStyle`. |
| ✅ | **Underline (straight/curly/dotted)** | Có | Có | `UNDERCURL` → wavy underline. Cả hai hỗ trợ. |
| ✅ | **Strikethrough** | Có | Có | `Flags::STRIKEOUT` → gạch ngang. Cả hai hỗ trợ. |
| ✅ | **Wide char / CJK** | Có | Có | `WIDE_CHAR_SPACER` skip + zerowidth append. Cả hai hỗ trợ. |
| ✅ | **Min contrast** | Có — `ensure_minimum_contrast` | Có — WCAG 4.5 | Cả hai ensure contrast fg/bg. |
| ✅ | **Terminal bell indicator** | Có — `has_bell` flag, show 🔔 trong tab | Có — `SessionEvent::Bell`, `has_bell` flag, 🔔 overlay góc trên-phải | ✅ Đã lấp. `Event::Bell` forward qua `SessionEvent::Bell` → view set `has_bell=true` → 🔔 overlay. Clear khi user gõ phím. Setting `bell_enabled`. |

---

## Nhóm B — Selection & Clipboard

| ✅/❌ | Gap | Zed có | myTerm2 có | Mô tả / Ví dụ |
|---|---|---|---|---|
| ✅ | **Simple selection (click-drag)** | Có | Có (fixed) | Kéo chuột select text. **Đã fix:** tách `mouse_drag` khỏi `mouse_move` + skip resize khi size không đổi (tránh shell redraw clear selection). |
| ✅ | **Semantic selection (double-click)** | Có | Có (fixed) | Double-click select word. `SelectionType::Semantic`. |
| ✅ | **Line selection (triple-click)** | Có | Có (fixed) | Triple-click select cả dòng. `SelectionType::Lines`. |
| ✅ | **Block selection (Alt+drag)** | Có | Có (fixed) | Alt+drag select block chữ nhật. `SelectionType::Block`. |
| ✅ | **Select-to-copy** | Có | Có (fixed) | Select xong → tự copy clipboard. Selection highlight dùng Zed blue (`#0d2847` dark / `#e6f4fe` light), text giữ màu gốc (không inverse video). |
| ✅ | **Middle-click paste** | Có | Có | Middle-click paste clipboard (X11 style). |
| ✅ | **Ctrl+Shift+C / Ctrl+Shift+V** | Có | Có | Copy/paste keyboard shortcut. |
| ✅ | **Select All** | Có | Có | Right-click menu → Select All. |
| ✅ | **Right-click context menu** | Có (richer) | Có (basic: Copy/Paste/Select All/Clear) | Zed thêm: New Terminal, Inline Assist, Close Tab. myTerm2: 4 items cơ bản. |
| ❌ | **Paste image as Ctrl+V** | Có — phát hiện `ClipboardEntry::Image` → gửi Ctrl+V | Không | Zed: copy ảnh → paste vào terminal → gửi raw Ctrl+V. myTerm2: paste ảnh → không làm gì. |
| ❌ | **Drag-and-drop file paths** | Có — `ExternalPaths` → quote path → write to PTY | Không | Zed: kéo file từ Finder vào terminal → path tự động quote. myTerm2: không nhận file kéo vào. **Ví dụ**: Kéo `main.rs` vào terminal → Zed auto `/path/to/main.rs`. |
| ❌ | **Copy with metadata** | Có — `CopyTemplate` + `task` info | Không | Zed: copy kèm task info khi chạy task. |

---

## Nhóm C — Scrolling

| ✅/❌ | Gap | Zed có | myTerm2 có | Mô tả / Ví dụ |
|---|---|---|---|---|
| ✅ | **Mouse wheel scroll** | Có | Có | Wheel up/down scroll scrollback. |
| ✅ | **Scrollbar (auto-hide)** | Có — `TerminalScrollHandle` | Có (fixed) | Scrollbar luôn hiện khi có scrollback (ScrollbarShow::Always). Kéo scrollbar thumb → jump đến vị trí. |
| ✅ | **Scrollbar drag** | Có | Có | Kéo scrollbar thumb → jump đến vị trí. |
| ✅ | **Scroll keyboard actions** | Có — `ScrollLineUp/Down`, `ScrollPageUp/Down`, `ScrollHalfPageUp/Down`, `ScrollToTop`, `ScrollToBottom` | Có (fixed) | Shift+PageUp/Down: scroll 1 viewport. Shift+Home/End: scroll to top/bottom. Ctrl+Shift+Up/Down: scroll 1 line. |
| ✅ | **Scroll-to-top / Scroll-to-bottom** | Có — action `ScrollToTop/Bottom` | Có (fixed) | Shift+Home → scroll_to_top, Shift+End → scroll_to_bottom. |
| ✅ | **Scroll multiplier setting** | Có — `scroll_multiplier` trong TerminalSettings | Có (fixed) | Setting `scroll_multiplier: f32` (default 1.0). Mouse wheel delta × multiplier. |
| ✅ | **Alternate scroll mode toggle** | Có — setting `alternate_scroll` | Có (fixed) | Setting `alternate_scroll: bool` (default true). Alacritty tự xử lý alt-screen mouse scroll. |

---

## Nhóm D — Search

| ✅/❌ | Gap | Zed có | myTerm2 có | Mô tả / Ví dụ |
|---|---|---|---|---|
| ❌ | **In-terminal search** | Có — `SearchableItem` trait, `SearchEvent`, `SearchQuery` | Không | Zed: Cmd+F → search bar hiện, gõ "error" → highlight tất cả match. myTerm2: không search được. **Ví dụ**: `cargo build` output 1000 dòng → Cmd+F "warning" → Zed highlight tất cả. myTerm2: phải đọc bằng mắt. |
| ❌ | **Search highlight (matches)** | Có — `matches: Vec<RangeInclusive<AlacPoint>>` | Không | Zed: match hiện vàng, current match cam. |
| ❌ | **Search navigation (next/prev)** | Có — `Direction::Next/Prev` | Không | Zed: Enter → next match, Shift+Enter → prev. |
| ❌ | **Search options (case, regex)** | Có — `SearchOptions` | Không | Zed: toggle case-sensitive, regex, whole word. |
| ❌ | **Search wrap-around** | Có | Không | Zed: search đến cuối → wrap lại từ đầu. |

---

## Nhóm E — Hyperlinks & Navigation

| ✅/❌ | Gap | Zed có | myTerm2 có | Mô tả / Ví dụ |
|---|---|---|---|---|
| ✅ | **OSC 8 hyperlink** | Có | Có | `cell.hyperlink()` → Ctrl+click mở URL. |
| ✅ | **Plain-text URL detection** | Có | Có (linkify) | `https://example.com` trong output → Ctrl+click mở. |
| ✅ | **Ctrl+click open URL** | Có | Có | Ctrl+click trên URL → mở browser. |
| ❌ | **Path-like hyperlink (file:line)** | Có — `hover_path_like_target`, `open_path_like_target`, regex cho `file.rs:42` | Không | Zed: hover `src/main.rs:42` → tooltip hiện path, Ctrl+click mở file tại line 42. myTerm2: chỉ URL. **Ví dụ**: Compiler output `error at src/main.rs:42:10` → Zed: click mở editor. myTerm2: text thường. |
| ❌ | **Hover tooltip** | Có — `HoverTarget { tooltip, hovered_word }` | Không | Zed: hover URL/path → tooltip hiện full URL/path. myTerm2: không có tooltip. |
| ❌ | **Hover underline** | Có — underline URL khi hover | Không | Zed: hover URL → URL gạch dưới. myTerm2: URL không có visual feedback khi hover. |
| ❌ | **File path detection (relative)** | Có — detect relative path + resolve against cwd | Không | Zed: `./src/main.rs` → Ctrl+click mở. myTerm2: không detect. |

---

## Nhóm F — Shell Integration

| ✅/❌ | Gap | Zed có | myTerm2 có | Mô tả / Ví dụ |
|---|---|---|---|---|
| ✅ | **OSC 7 (cwd)** | Có | Có | `parse_cwd_url` → update cwd. |
| ✅ | **OSC 52 (clipboard)** | Có | Có | `decode_osc52`/`encode_osc52` → clipboard set/get. |
| ✅ | **OSC 0/2 (title)** | Có | Có | Title change → tab title update. |
| ❌ | **OSC 133 (shell integration)** | Có — prompt start/end, command start/end markers | Không | Zed: shell integration (zsh/fish/bash) inject OSC 133 → detect prompt boundary, command boundaries. myTerm2: không. **Ví dụ**: Zed biết khi nào command kết thúc → scroll to bottom, mark prompt. myTerm2: không biết. |
| ❌ | **Foreground process detection** | Có — `PtyProcessInfo` (sysinfo + pgid) | Không | Zed: biết `node`, `python`, `cargo` đang chạy → tab title hiện "cargo". myTerm2: tab title luôn "Terminal". **Ví dụ**: `cargo build` → Zed tab: "cargo build". myTerm2: tab: "Terminal". |
| ❌ | **Shell environment detection** | Có — `ProjectEnvironment`, `capture_unix/windows`, `zed --printenv` | Không | Zed: spawn shell login mode → capture env JSON → inject vào terminal. myTerm2: inherit env trực tiếp. **Ví dụ**: Zed detect `.venv` → terminal tự activate venv. myTerm2: không. |
| ❌ | **ShellBuilder (quoting/escaping)** | Có — `ShellKind::Posix/Fish/Nushell/PowerShell`, `format_task_for_activation` | Không | Zed: biết cách quote path cho từng shell type. myTerm2: không. |
| ❌ | **Activation script** | Có — `activation_script: Vec<String>` | Không | Zed: task chạy → inject activation script trước command. myTerm2: không. |
| ❌ | **Breadcrumb text** | Có — `breadcrumb_text: String`, show trong toolbar | Không | Zed: toolbar terminal hiện breadcrumb (cwd path). myTerm2: chỉ title. |

---

## Nhóm G — Task Integration

| ✅/❌ | Gap | Zed có | myTerm2 có | Mô tả / Ví dụ |
|---|---|---|---|---|
| ❌ | **Task system** | Có — `TaskState`, `task: spawn`, `task: rerun` | Không | Zed: define task trong `tasks.json` → Cmd+Shift+T → spawn. myTerm2: không. **Ví dụ**: Task `cargo test` → Zed spawn terminal, auto-run, show status. |
| ❌ | **Task rerun** | Có — `RerunTask` action | Không | Zed: Cmd+Alt+R → rerun last task. |
| ❌ | **Task status tracking** | Có — `TaskStatus::Running/Completed/Failed` | Không | Zed: task tab hiện ✓ (thành công) hoặc ✗ (thất bại). |
| ❌ | **Task reveal/hide config** | Có — `reveal: always/no_focus/never`, `hide: never/always/on_success` | Không | Zed: config khi nào show/hide terminal tab. |
| ❌ | **Show command/summary** | Có — `show_summary`, `show_command` | Không | Zed: task output hiện command line + summary. |

---

## Nhóm H — Input & IME

| ✅/❌ | Gap | Zed có | myTerm2 có | Mô tả / Ví dụ |
|---|---|---|---|---|
| ✅ | **IME marked text (pre-edit)** | Có — `ImeState { marked_text }` | Có (fixed) | `set_marked_text`/`clear_marked_text`/`commit_text`. |
| ✅ | **IME commit** | Có | Có | `replace_text_in_range` → `commit_text`. |
| ✅ | **Alt-screen IME toggle** | Có | Có | Alt-screen → tắt IME (`selected_text_range` → None). |
| ✅ | **Keyboard mapping (arrows, F-keys)** | Có | Có | `key_encode.rs` → escape sequences. |
| ✅ | **Mouse mode encoding** | Có | Có | `mouse_encode.rs` → SGR/normal/X10 encoding. |
| ❌ | **Vi mode** | Có — `ToggleViMode`, `ViMotion::Left/Right/Up/Down/WordRight/WordLeft` | Không | Zed: Ctrl+Shift+Space → vi mode → hjkl navigate, v select, y yank. myTerm2: không. **Ví dụ**: Vi mode → `w` jump word, `yy` yank line. |
| ❌ | **Character palette** | Có — `ShowCharacterPalette` action | Không | Zed: Cmd+Ctrl+Space → character palette (emoji picker). |
| ❌ | **Send text action** | Có — `SendText(String)` action | Không | Zed: programmatically send text to terminal. **Ví dụ**: Extension gửi `make build\r` vào terminal. |
| ❌ | **Send keystroke action** | Có — `SendKeystroke(String)` | Không | Zed: programmatically send keystroke (vd `Ctrl+C`). |
| ❌ | **Bracketed paste detection** | Có — `Modes::BRACKETED_PASTE` | Một phần — alacritty handles | Cả hai đều qua alacritty. Nhưng Zed wrap paste trong `\x1b[200~...\x1b[201~` khi bracketed mode on. |

---

## Nhóm I — Panel & Workspace

| ✅/❌ | Gap | Zed có | myTerm2 có | Mô tả / Ví dụ |
|---|---|---|---|---|
| ✅ | **Dock panel** | Có — `TerminalPanel` impl `Panel` | Có | Terminal trong dock panel (bottom/side). |
| ❌ | **Multiple terminal tabs** | Có — `Pane` quản lý nhiều `TerminalView` | Không — 1 session/panel | Zed: tạo nhiều terminal, mỗi cái 1 tab trong pane. myTerm2: 1 terminal duy nhất. **Ví dụ**: Zed: "+" button → new terminal tab. myTerm2: không. |
| ❌ | **Terminal rename** | Có — `RenameTerminal` action, inline `Editor` | Không | Zed: right-click tab → Rename → gõ tên. myTerm2: không. |
| ❌ | **Terminal persistence** | Có — `TerminalDb`, `SerializableItem`, `WorkspaceId` | Không | Zed: restore terminal tabs khi reopen workspace. myTerm2: terminal mất khi close. |
| ❌ | **New terminal button in tab bar** | Có — `NewTerminal`, `NewCenterTerminal` buttons | Không | Zed: tab bar có nút "+" tạo terminal mới. |
| ❌ | **Block below cursor (inline blocks)** | Có — `BlockProperties { height, render }` | Không | Zed: Agent panel chèn block UI dưới cursor (vd inline prompt). myTerm2: không. **Ví dụ**: Agent → block 3 dòng "Press Enter to continue" cố định dưới cursor. |
| ❌ | **Embedded mode** | Có — `TerminalMode::Embedded { max_lines_when_unfocused }` | Không | Zed: terminal inline trong editor (Agent panel output). myTerm2: chỉ standalone. |
| ❌ | **Scroll state for blocks** | Có — `scroll_top: Pixels`, `max_scroll_top` | Không | Zed: scroll block content riêng khi có `block_below_cursor`. |

---

## Nhóm J — Settings & Configuration

| ✅/❌ | Gap | Zed có | myTerm2 có | Mô tả / Ví dụ |
|---|---|---|---|---|
| ✅ | **Shell program config** | Có — `terminal.shell: { program, args }` | Có — `LocalShellConfig { kind, program, args, cwd }` | Cả hai cho chọn shell. |
| ✅ | **Working directory** | Có — `WorkingDirectory` setting | Có — `cwd` in config | Cả hai set thư mục khởi động. |
| ❌ | **Cursor shape setting** | Có — `cursor_shape: Block/Bar/Underline` | Không | Zed: settings.json → `"cursor_shape": "bar"`. |
| ❌ | **Cursor blink setting** | Có — `blinking: On/Off/TerminalControlled` | Không | Zed: `"blinking": "off"` → cursor không nhấp nháy. |
| ❌ | **Font config (family/size)** | Có — `font_family`, `font_size`, `font_features` | Một phần — inherit từ theme | Zed: `"terminal": { "font_family": "JetBrains Mono" }`. myTerm2: dùng theme mono font. |
| ❌ | **Scrollback history config** | Có — `scrollback_history` setting | Hardcoded — 10,000 lines | Zed: `"scrollback_history": 50000`. myTerm2: cố định 10,000. |
| ❌ | **Scroll multiplier** | Có — `scroll_multiplier: f32` | Không | Zed: `"scroll_multiplier": 3.0`. |
| ❌ | **Toolbar breadcrumbs** | Có — `toolbar: { breadcrumbs: bool }` | Không | Zed: `"toolbar": { "breadcrumbs": true }` → toolbar hiện cwd path. |
| ❌ | **Bell setting** | Có — `bell: System/On/Off` | Không | Zed: `"bell": "off"` → tắt bell. |
| ❌ | **Alternate scroll** | Có — `alternate_scroll: bool` | Không | Zed: `"alternate_scroll": false` → tắt mouse scroll trong alt-screen. |
| ❌ | **Option as Meta** | Có — `option_as_meta: bool` | Không | Zed: macOS → Option key = Meta (Alt). myTerm2: không config. |
| ❌ | **Custom shell arguments** | Có — `with_arguments: { program, args }` | Có (trong config) | Cả hai hỗ trợ args. ✅ Đã có. |
| ❌ | **Environment variables injection** | Có — `env: { KEY: value }` | Không | Zed: `"env": { "MY_VAR": "value" }` → inject vào terminal. myTerm2: inherit env. |
| ❌ | **Path hyperlink regexes** | Có — `path_hyperlink_regexes`, `path_hyperlink_timeout` | Không | Zed: custom regex cho file path detection. |

---

## Nhóm K — Architecture & Backend

| ✅/❌ | Gap | Zed có | myTerm2 có | Mô tả / Ví dụ |
|---|---|---|---|---|
| ✅ | **alacritty_terminal backend** | Có | Có | Cả hai dùng `alacritty_terminal::Term` + `EventLoop`. |
| ✅ | **FairMutex concurrency** | Có | Có | `Arc<FairMutex<Term<EP>>>`. |
| ✅ | **Snapshot rendering** | Có | Có | `TerminalContent` snapshot → render không lock. |
| ✅ | **Batched text runs** | Có | Có | Group adjacent cells cùng style → 1 text shape call. |
| ✅ | **Local terminal (ConPTY/Unix)** | Có | Có | Windows ConPTY + Unix pty. |
| ❌ | **SSH terminal** | Có — remote terminal qua SSH | Thiết kế, chưa impl | Zed: `is_remote_terminal: bool`, remote PTY qua SSH. myTerm2: `SshSession` đã thiết kế (`docs/terminal-backend.md`) nhưng chưa implement. |
| ❌ | **TerminalBuilder (2-step init)** | Có — `TerminalBuilder::new()` → check → `subscribe()` | Không — `LocalSession::spawn` 1 bước | Zed: tách init để handle failure gracefully. myTerm2: `expect("spawn")` → panic nếu fail. **Ví dụ**: PTY fail → Zed show error view. myTerm2: crash. |
| ❌ | **CopyTemplate (shell context)** | Có — `CopyTemplate { shell }` | Không | Zed: lưu shell context cho copy/paste formatting. |
| ❌ | **Input log (test support)** | Có — `input_log: Vec<Vec<u8>>` | Không | Zed: log input cho test verification. |
| ❌ | **Event coalescing (VecDeque)** | Có — `events: VecDeque<InternalEvent>` | Một phần — drain Output events | Zed: queue `InternalEvent` coalesce. myTerm2: drain channel trong spawn task. |

---

## Nhóm L — Mouse & Interaction

| ✅/❌ | Gap | Zed có | myTerm2 có | Mô tả / Ví dụ |
|---|---|---|---|---|
| ✅ | **Mouse mode encoding** | Có | Có | SGR/normal/X10 mouse encoding. |
| ✅ | **Mouse motion/drag tracking** | Có | Có | `MOUSE_MOTION`, `MOUSE_DRAG` mode. |
| ✅ | **Wheel event encoding** | Có | Có | Wheel → SGR mouse or arrow keys. |
| ❌ | **Selection phase tracking** | Có — `SelectionPhase` enum | Không | Zed: track selection state (start/update/end) để handle click precisely. |
| ❌ | **Mouse-down hyperlink tracking** | Có — `mouse_down_hyperlink: Option<(String, bool, Match)>` | Không | Zed: lưu hyperlink khi mouse-down → Ctrl+click chỉ mở nếu cùng link. |
| ❌ | **Last mouse move time (debounce)** | Có — `last_mouse_move_time: Instant` | Không | Zed: debounce hover detection. |
| ❌ | **Hyperlink search caching** | Có — `last_hyperlink_search_position`, `hyperlink_regex_searches: RegexSearches` | Không | Zed: cache regex search results để hover nhanh. myTerm2: search mỗi frame. |

---

## Tóm tắt số liệu

| Nhóm | Tổng gaps | Đã có (✅) | Thiếu (❌) | % hoàn thành |
|---|---|---|---|---|
| A — Rendering & Display | 11 | 10 | 1 | 91% |
| B — Selection & Clipboard | 12 | 8 | 4 | 67% |
| C — Scrolling | 7 | 7 | 0 | 100% |
| D — Search | 5 | 0 | 5 | 0% |
| E — Hyperlinks & Navigation | 7 | 3 | 4 | 43% |
| F — Shell Integration | 9 | 3 | 6 | 33% |
| G — Task Integration | 5 | 0 | 5 | 0% |
| H — Input & IME | 10 | 5 | 5 | 50% |
| I — Panel & Workspace | 9 | 1 | 8 | 11% |
| J — Settings & Configuration | 16 | 3 | 13 | 19% |
| K — Architecture & Backend | 10 | 5 | 5 | 50% |
| L — Mouse & Interaction | 7 | 3 | 4 | 43% |
| **Tổng** | 108 | **49** | **63** | **45%** |

---

## Ưu tiên đề xuất (Roadmap)

### P0 — Cần fix ngay (trải nghiệm cơ bản)
1. ✅ ~~Fix selection highlight~~ (đã fix)
2. ✅ ~~Fix scrollbar visibility~~ (đã fix)
3. ✅ ~~Selection inverse video~~ (đã lấp — text đổi màu khi select)
4. ✅ ~~Cursor blink~~ (đã lấp — 500ms toggle, On/Off setting)
5. ✅ ~~Cursor shape config~~ (đã lấp — Block/Beam/Underline)
6. ✅ ~~Terminal bell indicator~~ (đã lấp — 🔔 overlay + clear on input)
7. ✅ ~~Font features/ligatures~~ (đã lấp — setting + FontFeatures)
8. Multiple terminal tabs (nút "+" tạo terminal mới)
9. TerminalBuilder (graceful error thay vì panic)

### P1 — Quan trọng (parity với terminal cơ bản)
7. In-terminal search (Cmd+F)
8. Cursor shape config
9. Font config (family/size riêng cho terminal)
10. Scrollback history config
11. ✅ ~~Scroll keyboard actions (ScrollLineUp/Down, PageUp/Down scrollback)~~ (đã lấp — Shift+PageUp/Down, Shift+Home/End, Ctrl+Shift+Up/Down)
12. Terminal rename
13. Path-like hyperlink (file:line → mở editor)
14. Hover tooltip + underline cho hyperlinks

### P2 — Tốt có (shell integration)
15. OSC 133 shell integration markers
16. Foreground process detection (dynamic tab title)
17. Shell environment detection
18. Vi mode
19. Drag-and-drop file paths
20. Bell indicator

### P3 — Tương lai (parity đầy đủ với Zed)
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