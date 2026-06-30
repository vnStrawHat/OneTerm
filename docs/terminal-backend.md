# Thiết kế Terminal Backend — OneTerm

> Tài liệu thiết kế cho phần terminal: **local shell** + **SSH session**, dùng chung
> renderer dựa trên `alacritty_terminal`. Ưu tiên Windows-first. Shell cục bộ có thể
> chọn `cmd` / `powershell` / `pwsh` / custom.
>
> **Tham chiếu chính**: Zed (`zed-industries/zed`) dùng đúng `alacritty_terminal`
> (tty + `EventLoop` + `FairMutex`) và render bằng GPUI custom Element. Thiết kế này
> map 1:1 sang Zed, thay layer chrome bằng `gpui-component`.
>
> File nguồn Zed tham chiếu (cùng rev lock `1d217ee39…`):
> - `crates/terminal/src/terminal.rs` — model + EventLoop + PTY.
> - `crates/terminal_view/src/terminal_element.rs` — custom `Element` render grid.
> - `crates/terminal_view/src/terminal_view.rs` — View + IME (`ImeState`).
>
> **Quyết định cốt lõi** (xem lịch sử brainstorm):
> 1. **Local và SSH độc lập hoàn toàn** — không share trait pump, không biết nhau.
> 2. **Render dùng chung `alacritty_terminal`** qua một custom GPUI `Element`.
> 3. **Local dùng `alacritty_terminal::tty` + `EventLoop`** (không dùng `portable-pty`).
> 4. **`alacritty_terminal` lấy từ fork `zed-industries/alacritty`** @ rev `fcf32feacb367b75ec84dd40f041e4fd411d3cc1`
>    (bản patched có `TerminalContent`/`display_iter`/`content()`). Đây là rev mà Zed
>    dùng cho `gpui` rev `1d217ee39…`, nhưng repo riêng — không phải monorepo zed.
> 5. **Concurrency model của alacritty**: `Arc<FairMutex<Term<EP>>>` + snapshot.
> 6. **Kit thuần** (`core`) không phụ thuộc GPUI.

---

## 1. Nguyên tắc

| # | Nguyên tắc | Hệ quả |
|---|---|---|
| 1 | Tách lớp rõ ràng | UI không chứa logic giao thức; giao thức không biết UI. |
| 2 | Local & SSH độc lập | Hai backend không share trait pump, không phụ thuộc lẫn nhau. |
| 3 | Render dùng chung | Một `TerminalElement` vẽ grid cho cả local và ssh — chỉ cần `&TerminalContent`. |
| 4 | Snapshot, không lock-while-paint | Pump cập nhật snapshot; render đọc snapshot, không giữ `FairMutex` khi vẽ. |
| 5 | Windows-first | Local ưu tiên ConPTY; shell `cmd`/`pwsh`/`powershell` config được. |
| 6 | Rev lock nghiêm ngặt | `gpui` + `gpui_platform` cùng rev monorepo zed; `alacritty_terminal` fork `zed-industries/alacritty` rev `fcf32fe…`. |

---

## 2. Sơ đồ kiến trúc

```
┌─────────────────── ui crate (GPUI + gpui-component) ───────────────────┐
│  LocalTerminalView / SshTerminalView  (impl Render)                    │
│   ├─ chrome: Button, Tabs, Dock… (gpui-component)                        │
│   └─ child: TerminalElement  (custom gpui::Element, dùng chung)        │
│          • đọc TerminalContent snapshot → paint_quad / shape_line      │
│          • EntityInputHandler (IME) + mouse + wheel                   │
└───────▲─────────────────────────────────────────▲──────────────────────┘
        │ TerminalSession trait (core)           │
   ┌────┴────────────────┐               ┌────────┴───────────────┐
   │  local crate        │               │  ssh crate             │  ← ĐỘC LẬP
   │  alacritty_terminal │               │  russh + tokio (ẩn)     │     không biết nhau
   │   ::tty + EventLoop │               │  channel + pty-req      │
   │  ConPTY / chcp      │               │  window_change / exit   │
   │  Arc<FairMutex<     │               │  Arc<FairMutex<         │
   │   Term<LocalEP>>>   │               │   Term<SshEP>>>         │
   │  last_content       │               │  last_content           │
   └────┬────────────────┘               └────────┬───────────────┘
        └──────────────┬──────────────────────────┘
                ┌──────▼───────┐
                │  core crate  │  TerminalSession trait, TerminalContent,
                │  (leaf, no   │  TerminalPalette, key_encode, mouse_encode,
                │   GPUI)      │  osc, url, ShellKind/LocalShellConfig
                └──────────────┘
```

**Luồng dữ liệu**:
- Đầu vào: `Keystroke` (GPUI) → `core::key_encode` → `Vec<u8>` → `session.write(bytes)` → PTY/channel.
- Đầu ra: PTY/channel → pump (`EventLoop` local / tokio ssh) → `Term.advance(bytes)` → rebuild `last_content` snapshot → event → View `cx.notify()` → `TerminalElement::paint` đọc snapshot.

---

## 3. Trách nhiệm từng crate

| Crate | Vai trò terminal |
|---|---|
| `core` | `TerminalSession` trait, `TerminalContent` snapshot struct, `TerminalPalette`, `key_encode`/`mouse_encode`/`osc`/`url` (pure, không GPUI), `ShellKind` + `LocalShellConfig` (config), `SessionEvent`. |
| `local` | `LocalSession` implement `TerminalSession`. Spawn shell qua `alacritty_terminal::tty::new` + `EventLoop`. ConPTY trên Windows. Detect/chọn shell, `chcp 65001`, env. `LocalListener: EventListener`. |
| `ssh` | `SshSession` implement `TerminalSession`. russh client + tokio runtime ẩn. pty-req + shell + `window_change` + exit-status. `SshListener: EventListener`. |
| `ui` | `TerminalElement` (custom `gpui::Element`), `LocalTerminalView`/`SshTerminalView` (`Render`), IME (`EntityInputHandler`), mouse/wheel, font measure, theme → `TerminalPalette`. |
| `app` | Wire views vào DockArea, settings, host manager. |

> Quy tắc phụ thuộc giữ nguyên: `app → {ui, ssh, local, core}`, `ui → core`, `ssh → core`,
> `local → core`. `ui` **không** import `ssh`/`local` trực tiếp — gọi qua `TerminalSession`.

---

## 4. Dependencies & rev lock

```toml
# workspace Cargo.toml — thêm vào [workspace.dependencies]
alacritty_terminal = { git = "https://github.com/zed-industries/alacritty", rev = "fcf32feacb367b75ec84dd40f041e4fd411d3cc1" }
async-channel = "2"      # event sub (không tokio lộ ra)
russh = "0.46"
russh-keys = "0.46"
tokio = { version = "1", features = ["rt", "rt-multi-thread", "sync", "io-util", "process", "net", "macros"] }
```

> ⚠️ **Bắt buộc**: `alacritty_terminal` lấy từ fork `zed-industries/alacritty` @
> rev `fcf32fe…` (rev mà Zed dùng cho `gpui` rev `1d217ee39…`). KHÔNG phải monorepo
> zed. Dùng crates.io `0.26` sẽ **thiếu** `TerminalContent`/`display_iter`/`content()`/`Block`
> mà render cần → không compile. Khi đổi rev `gpui` → kiểm tra Zed workspace deps
> để lấy rev `alacritty_terminal` tương ứng (hai rev có thể khác nhau).
>
> `portable-pty` **không dùng** cho local nữa (quyết định brainstorm). `ssh` không cần
> PTY cục bộ — chỉ cần `alacritty_terminal` cho Term grid.

---

## 5. Concurrency model: `Arc<FairMutex<Term<EP>>>` + snapshot

### 5.1. Vì sao

- **Pump** (local `EventLoop` thread / ssh tokio task) advance Term ở thread khác.
- **Render** (`TerminalElement::paint`) chạy ở main thread GPUI.
- Cả hai cần truy cập cùng `Term` ⇒ dùng `alacritty_terminal::sync::FairMutex`
  (fair = không bị main thread "đói" lock khi pump bận).

### 5.2. Snapshot vs live borrow (QUAN TRỌNG)

| | Live borrow (SAI) | Snapshot (ĐÚNG — Zed làm vậy) |
|---|---|---|
| Paint | `let g = term.lock();` rồi vẽ **giữ guard** | `let snap = { let g = term.lock(); build content }; drop(g);` rồi vẽ |
| Vấn đề | paint chậm (nghìn lệnh GPU) → pump `term.lock().advance()` **bị block** → jitter khi output dồn (`yes`, `cat file lớn`) | Lock chỉ trong µs để copy, pump chạy song song paint |
| Chi phí | 0 | 1 copy ~nghìn cell/frame (rẻ hơn paint rất nhiều) |

**Quy ước**: backend giữ `last_content: TerminalContent` (cache, build sau mỗi tick
pump). `TerminalElement::paint` chỉ đọc `session.snapshot()` — **không bao giờ lock
`FairMutex` trong paint**.

```rust
// Pump (local EventLoop callback / ssh task) — sau khi advance Term:
let content = TerminalContent::from(&*term.lock());   // lock ngắn
last_content.store(content);                          // ArcSwap hoặc Mutex<TerminalContent>
event_tx.send(SessionEvent::Output).ok();              // → View cx.notify()

// Render (TerminalElement::paint):
let content = session.snapshot();                     // đọc cache, không lock Term
// vẽ từ content.cells / content.cursor / content.mode ...
```

> Dùng `arc-swap` cho `last_content` (lock-free read) hoặc `Mutex<TerminalContent>`
> (lock ngắn). KHÔNG giữ `FairMutex<Term>` khi đọc snapshot trong paint.

### 5.3. `EventListener` riêng mỗi backend

`EventProxy` (impl `alacritty_terminal::event::EventListener`) route side-effect:
- `PtyWrite(text)` → **local**: EventLoop tự ghi PTY; **ssh**: `channel.data(text)`.
- `Title(t)` → `last_title` + `SessionEvent::Title`.
- `ClipboardStore(_, t)` → `SessionEvent::Clipboard`.
- `Bell` / `ChildExit` / `ResetTitle` → event tương ứng.

Mỗi backend có `EP` riêng (`LocalListener` / `SshListener`). Không route xuyên backend.

---

## 6. Local backend (`local` crate, Windows-first)

### 6.1. Shell có thể config

`core` định nghĩa:

```rust
/// Loại shell cục bộ.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ShellKind {
    /// Windows cmd.exe (COMSPEC).
    Cmd,
    /// Windows PowerShell 5.1 (powershell.exe).
    PowerShell,
    /// PowerShell 7+ (pwsh.exe).
    Pwsh,
    /// Unix shells.
    Bash,
    Zsh,
    Sh,
    /// Lệnh tùy chỉnh.
    Custom,
}

/// Cấu hình spawn shell cục bộ.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LocalShellConfig {
    pub kind: ShellKind,
    /// Đường dẫn executable (None → tự detect theo kind + nền tảng).
    pub program: Option<PathBuf>,
    /// Tham số dòng lệnh thêm.
    pub args: Vec<String>,
    /// Env override (TERM, COLORTERM, LANG…). Mặc định đã set TERM=xterm-256color.
    pub env: HashMap<String, String>,
    /// Thư mục làm việc (None → cwd hiện tại của app).
    pub cwd: Option<PathBuf>,
    /// Ép UTF-8 codepage (Windows cmd). Mặc định true.
    pub utf8: bool,
}
```

Giải quyết `ShellKind` → executable + args + env (Windows-first):

| Kind | Default program | Args mặc định | UTF-8 |
|---|---|---|---|
| `Cmd` | `%COMSPEC%` (cmd.exe) | `/K chcp 65001 >nul` (nếu `utf8`) | `chcp 65001` |
| `PowerShell` | `powershell.exe` (tìm trong PATH / `where`) | `-NoLogo` | env `LANG=en_US.UTF-8`; `[Console]::OutputEncoding=UTF8` qua profile/arg |
| `Pwsh` | `pwsh.exe` | `-NoLogo` | như PowerShell |
| `Bash`/`Zsh`/`Sh` | `$SHELL` / `/bin/bash`… | `-l` (login) tuỳ config | env `LANG`/`LC_ALL` |
| `Custom` | `program` (bắt buộc) | `args` | theo `env`/`utf8` |

> Settings UI (`ui/views/settings/terminal.rs`) cho user chọn `kind`, gõ `program`
> custom, thêm `args`, set `cwd`, toggle `utf8`. Persist qua `core::config::store`.

### 6.2. Spawn qua `alacritty_terminal::tty`

```rust
use alacritty_terminal::{event_loop::EventLoop, sync::FairMutex, term::{Config, Term}, tty::{self, Options, Shell, WindowSize}};

pub struct LocalSession {
    term: Arc<FairMutex<Term<LocalListener>>>,
    notifier: Notifier,                          // EventLoop channel (Msg::Input/Resize/Shutdown)
    last_content: Arc<ArcSwap<TerminalContent>>, // snapshot
    event_tx: Sender<SessionEvent>,
    config: LocalShellConfig,
    // child exit, alive flag…
}

impl LocalSession {
    pub fn spawn(cfg: LocalShellConfig, initial: PtySize) -> core::Result<Self> {
        let (program, args, env) = resolve_shell(&cfg)?;     // §6.1 table
        let opts = Options {
            shell: Some(Shell { program: program.into(), args: args.into_iter().map(Into::into).collect() }),
            working_directory: cfg.cwd.clone(),
            env: env.into_iter().collect(),
            ..Default::default()
        };
        let winsize = WindowSize { rows: initial.rows, cols: initial.cols, ..Default::default() };
        let pty = tty::new(&opts, winsize, 0).map_err(|e| AppError::msg(e.to_string()))?;

        let term = Arc::new(FairMutex::new(Term::new(
            Config { scrolling_history: 10_000, ..Default::default() },
            &TermSize::from(initial),
            LocalListener { /* event_tx clone */ },
        )));
        let mut event_loop = EventLoop::new(term.clone(), LocalListener::default(), pty, false, false)
            .map_err(|e| AppError::msg(e.to_string()))?;
        let notifier = event_loop.channel();           // để write/resize
        event_loop.run().detach();                      // spawn thread pump
        // … child exit watcher (ChildExitWatcher) → SessionEvent::Exited
        Ok(Self { term, notifier, last_content: Arc::new(ArcSwap::from_pointee(default())), event_tx, config: cfg })
    }

    pub fn write(&self, bytes: &[u8]) { self.notifier.tty_notify(bytes.to_vec().into()); }   // Msg::Input
    pub fn resize(&self, r: u16, c: u16) { self.notifier.notify_resize(WindowSize { rows: r, cols: c, ..Default::default() }); }
    pub fn shutdown(&self) { self.notifier.shutdown(); }
}
```

> Tham chiếu chính xác `Notifier` API: đọc
> `reference/gpui-component` **không có** — đây là API nội bộ Zed; xem trực tiếp
> `alacritty_terminal` source tại rev lock: `event_loop.rs` (`Notifier`, `Msg`),
> `tty/{mod,unix,windows}.rs`. Khi triển khai, mở source crate đó để khớp signature.

### 6.3. Windows-specific

- **ConPTY**: `alacritty_terminal::tty` tự chọn ConPTY trên Win10 1809+. Không cần
  code `CreatePseudoConsole` thủ công.
- **UTF-8**: `Cmd` → `chcp 65001` (qua args `/K`). `pwsh`/`powershell` → set env
  `LANG`/`LC_ALL` + (tuỳ chọn) arg khởi tạo `[Console]::OutputEncoding`.
- **TERM**: luôn `xterm-256color`, `COLORTERM=truecolor`.
- **Resize**: `Notifier::notify_resize` → ConPTY xử lý (không SIGWINCH trên Windows).
- **Ctrl-C**: byte `0x03` → shell tự xử lý. OK.
- **Child exit**: `tty::Pty` cung cấp `ChildExitWatcher` (race-free) → `SessionEvent::Exited(code)`.

### 6.4. Re-render perf (theo Zed)

- Pump không `notify` từng byte — `EventLoop` đã coalesce; ta rebuild `last_content`
  sau mỗi tick và gửi **một** `SessionEvent::Output`.
- View `cx.notify()` chỉ khi `display_offset`/`mode`/`cursor`/cells thực sự đổi
  (compare snapshot cũ vs mới). Tránh redraw liên tục khi `yes`.
- Log `layout took {:?}` để tune (copy Zed `log::debug!`).

---

## 7. SSH backend (`ssh` crate)

Tokio runtime **ẩn** (current-thread, `enable_all`); API lộ ra ngoài là sync.

```rust
pub struct SshSession {
    term: Arc<FairMutex<Term<SshListener>>>,
    last_content: Arc<ArcSwap<TerminalContent>>,
    cmd_tx: std::sync::mpsc::SyncSender<Cmd>,   // bridge sync→tokio
    event_tx: Sender<SessionEvent>,
    runtime: tokio::runtime::Runtime,           // ẩn, drop khi close
    alive: Arc<AtomicBool>,
}

enum Cmd { Write(Vec<u8>), Resize(u16, u16), Close }

impl SshSession {
    pub fn connect(cfg: SshConfig, initial: PtySize) -> core::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        let (cmd_tx, cmd_rx) = std::sync::mpsc::sync_channel(64);
        let term = Arc::new(FairMutex::new(Term::new(/* cfg */, &TermSize::from(initial), SshListener { /* event_tx */ })));
        let event_tx_clone = event_tx.clone();
        runtime.block_on(async move {
            let handle = russh::client::connect(addr, client_cfg, handler).await?;
            let auth = resolve_auth(&cfg, &handle).await?;     // password / key / agent
            let auth_ok = handle.authenticate(username, auth).await?;
            let mut ch = handle.channel_open_session().await?;
            ch.request_pty("xterm-256color", initial.cols, initial.rows, 0, 0, &[]).await?;
            ch.request_shell(true).await?;
            // spawn 2 task: data reader + cmd consumer
            tokio::spawn(async move { /* reader: ch.wait() → term.lock().advance(data) → last_content → event_tx */ });
            tokio::spawn(async move { /* cmd: while let Ok(c)=cmd_rx.recv() { match c { Write→ch.data, Resize→ch.window_change, Close→ch.close } } */ });
            Ok::<_, anyhow::Error>(())
        })?;
        Ok(Self { term, last_content, cmd_tx, event_tx, runtime, alive })
    }

    pub fn write(&self, b: &[u8]) { let _ = self.cmd_tx.send(Cmd::Write(b.to_vec())); }
    pub fn resize(&self, r: u16, c: u16) { let _ = self.cmd_tx.send(Cmd::Resize(r, c)); }
    pub fn close(&self) { let _ = self.cmd_tx.send(Cmd::Close); }
}
```

- `SshListener: EventListener` — `PtyWrite(text)` → `cmd_tx.send(Cmd::Write(text.into_bytes()))`.
- `is_local() == false` (cho OSC 7 cwd semantics: ssh có thể là `file://host/…`).
- Exit: `ChannelMsg::ExitStatus { exit_status }` → `SessionEvent::Exited(Some(code))`;
  `Eof`/`Close` → `SessionEvent::Closed`.
- Auth (MVP): password + key file. Agent sau.

> Bridge sync→async: `std::sync::mpsc::SyncSender` gửi từ main thread, tokio task
> `recv()` (blocking) trong runtime. Tránh `block_on` lồng. Event ra ngoài dùng
> `async_channel` (sender Send+Sync, recv trong smol/GPUI task).

---

## 8. Render (`ui` crate) — `TerminalElement`

Custom `gpui::Element` (pattern Zed `terminal_element.rs`). Vẽ từ **snapshot**.

### 8.1. Cấu trúc

```rust
pub struct TerminalElement {
    session: Entity<dyn TerminalSession>,   // hoặc generic
    bounds: TerminalBounds,                  // cell_width, line_height, rows, cols
    theme: TerminalTheme,                    // bg/fg/16 ANSI/cursor → gpui::Hsla
    focus: FocusHandle,
    focused: bool,
    cursor_visible: bool,
    interactivity: Interactivity,
}

impl InteractiveElement for TerminalElement { /*…*/ }
impl StatefulInteractiveElement for TerminalElement {}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = LayoutState;       // hitbox, bg_rects, text_runs, cursor, ime_bounds
    fn request_layout(&mut self, …) -> (LayoutId, ()) { /* size_full hoặc size theo rows×cols */ }
    fn paint(&mut self, …, layout: &mut LayoutState, window, cx) {
        let content = self.session.read(cx).snapshot();      // không lock FairMutex
        window.with_content_mask(Some(ContentMask { bounds }), |w| {
            w.paint_quad(fill(bounds, self.theme.bg));
            for rect in &layout.bg_rects { rect.paint(origin, &self.bounds, w); }   // batch nền
            for run in &layout.text_runs { run.paint(origin, &self.bounds, w, cx); } // ShapedLine.paint
            // cursor + selection + ime marked text
        });
        window.handle_input(&ElementInputHandler::new(self.session.downgrade(), self.session.clone()));  // IME
    }
}
```

### 8.2. `layout_grid` (batch — copy Zed)

Duyệt `content.display_iter` (`IndexedCell`):
- **Background**: gom cell liên tiếp cùng màu nền (skip default bg) → `Vec<LayoutRect>`
  + `merge_background_regions` (gộp ngang/dọc) để giảm `paint_quad`.
- **Text**: gom cell liên tiếp cùng `TextRun` (fg + bold/italic/underline + font) →
  `Vec<BatchedTextRun>`. Mỗi run: `window.text_system().shape_line(text, font_size,
  &[run], Some(cell_width)).paint(pos, line_height, Left, None, window, cx)`.
- **Wide char spacers** + **zero-width chars** (emoji variation sequences): xử lý đúng
  (copy logic `is_wide_char_spacer` / `append_zero_width_chars` của Zed).
- **Contrast**: `ensure_minimum_contrast(fg, bg, min)` — bỏ qua nếu
  `is_app_chosen_exact_color` (truecolor/256≥16) hoặc `is_decorative_character`
  (box-drawing/powerline). Các hàm này ở `core` (pure).

### 8.3. Font measure (font riêng cho terminal)

`TerminalSettings` (font riêng, không phụ thuộc gpui-component theme):
```rust
pub struct TerminalSettings {
    pub font_family: String,         // vd "Cascadia Mono", "JetBrains Mono"
    pub font_size: f32,               // px
    pub font_weight: u32,
    pub line_height: f32,             // multiplier (1.0 = default)
    pub font_features: FontFeatures,
    pub scrollback: usize,
    pub minimum_contrast: f32,
    pub shell: LocalShellConfig,     // §6.1
}
```
Measure (cache, re-measure khi đổi font/size):
```rust
let probe = window.text_system().shape_line("M".repeat(cols).into(), font_size, &[base_run], Some(target_cell_w));
let cell_width = probe.width() / cols as f32;
let line_height = font_size * settings.line_height;     // hoặc ascent+descent+leading
```

### 8.4. Colors (`core` + `ui`)

- `core::TerminalPalette { default_fg, default_bg, ansi: [Rgba; 16], cursor }` (pure, `Rgba<u8>`).
- `ui` build `TerminalPalette` từ `cx.theme()` (gpui-component) → convert `Rgba→Hsla`.
- `core::resolve_color(fg: &AnsiColor, palette) -> Rgba` (named/indexed/truecolor).
- `ui::ensure_minimum_contrast(fg: Hsla, bg: Hsla, min: f32) -> Hsla` (copy từ Zed/UI util).

---

## 9. `TerminalSession` trait (`core`)

```rust
pub trait TerminalSession: Send + Sync + 'static {
    /// Snapshot grid để render (không lock FairMutex trong lúc gọi).
    fn snapshot(&self) -> TerminalContent;
    /// Ghi byte vào PTY/channel (keystroke, paste, OSC response).
    fn write(&self, bytes: &[u8]);
    /// Resize rows×cols (PTY resize / ssh window_change).
    fn resize(&self, rows: u16, cols: u16);
    /// Scroll scrollback (chỉ khi không alt-screen / không mouse mode).
    fn scroll(&self, delta: i32);
    // Mouse
    fn mouse_down(&self, row: f32, col: f32, button: MouseButton, sel: SelectionType);
    fn mouse_move(&self, row: f32, col: f32);
    fn mouse_up(&self, row: f32, col: f32, button: MouseButton);
    fn wheel(&self, delta_y: f64, row: f32, col: f32);
    // IME
    fn set_marked_text(&self, text: String);
    fn clear_marked_text(&self);
    fn commit_text(&self, text: &str);
    fn marked_text(&self) -> Option<String>;
    fn cursor_bounds(&self) -> Option<Bounds<Pixels>>;     // cho IME popup
    // Lifecycle
    fn subscribe(&self) -> Receiver<SessionEvent>;
    fn alive(&self) -> bool;
    fn close(&self);
    fn is_local(&self) -> bool;
    fn title(&self) -> Option<String>;
    fn cwd(&self) -> Option<PathBuf>;                      // OSC 7
}
```

> Trait này chỉ là **render/lifecycle interface** — không ép pump/transport chung.
> `LocalSession` và `SshSession` implement độc lập. Hai backend vẫn không biết nhau.

`SessionEvent`: `Output | Title(String) | Cwd(PathBuf) | Clipboard(Option<String>) |
Exited(Option<i32>) | Closed`.

---

## 10. Input: keystroke → byte + IME

Theo Zed README (4 đường input):

1. **Raw keystroke** (`on_key_down` trong element): `try_keystroke(keystroke, mods)`
   → `core::key_encode` → `session.write(bytes)`. Ánh xạ: Ctrl+char → `& 0x1f`, F-key /
   arrow → ANSI escape, Enter → `\r`, Backspace → `0x7f`, Tab → `\t` / `\x1b[Z`…
   (copy logic `freya-terminal::write_key`, thuần hoá thành `core::key_encode`).
2. **GPUI action** (Ctrl-Shift-C/V copy/paste, Ctrl-Tab…): map → `try_keystroke` hoặc
   clipboard.
3. **IME**: keystroke không map → nhường GPUI IME → `EntityInputHandler` gọi lại
   `replace_text_in_range(text)` → `session.commit_text(text)`. Pre-edit:
   `replace_and_mark_text_in_range` → `session.set_marked_text` → vẽ marked text tại
   cursor với underline.
4. **Paste**: `session.commit_text(text)` (bracketed paste nếu `TermMode::BRACKETED_PASTE`).

IME impl (`ui`):
- `LocalTerminalView`/`SshTerminalView` impl `gpui::EntityInputHandler`:
  `selected_text_range`, `marked_text_range`, `replace_text_in_range`,
  `replace_and_mark_text_in_range`, `unmark_text`, `bounds_for_range`,
  `text_for_range`, `character_index_for_point`.
- `ImeState { marked_text: String }` giữ trên View.
- Trong `paint`: `window.handle_input(&ElementInputHandler::new(view_handle))`.
- Vẽ marked text: shape riêng, paint tại `ime_cursor_bounds` + underline.

---

## 11. Layout file dự kiến

```
crates/
├── core/src/
│   ├── terminal/
│   │   ├── mod.rs
│   │   ├── session.rs         # TerminalSession trait + SessionEvent
│   │   ├── content.rs         # TerminalContent snapshot struct
│   │   ├── palette.rs         # TerminalPalette (Rgba), resolve_color
│   │   ├── colors_util.rs     # is_app_chosen_exact_color, is_decorative_character
│   │   ├── key_encode.rs      # key_encode(KeySpec, Modifiers) -> Vec<u8>
│   │   ├── mouse_encode.rs    # mouse press/move/release/wheel → CSI seq
│   │   ├── osc.rs             # OSC 7/8/52 parse
│   │   └── url.rs             # linkify
│   └── config/
│       ├── settings.rs        # TerminalSettings
│       └── shell.rs           # ShellKind, LocalShellConfig, resolve_shell
│
├── local/src/
│   ├── lib.rs
│   ├── session.rs            # LocalSession: tty + EventLoop
│   ├── listener.rs           # LocalListener: EventListener
│   ├── shell.rs              # resolve_shell (Windows: chcp, COMSPEC, where pwsh)
│   └── win.rs                # (cfg windows) ConPTY quirks nếu cần
│
├── ssh/src/
│   ├── lib.rs
│   ├── session.rs            # SshSession: russh + tokio runtime ẩn
│   ├── listener.rs          # SshListener: EventListener (PtyWrite → channel.data)
│   ├── auth.rs              # password / key / agent
│   └── runtime.rs           # tokio runtime + sync→async bridge
│
└── ui/src/views/terminal/
    ├── mod.rs
    ├── terminal_view.rs      # Render view (LocalTerminalView / SshTerminalView)
    ├── terminal_panel.rs     # PanelView (dock)
    ├── terminal_element.rs   # custom Element: layout_grid + paint
    ├── input.rs             # EntityInputHandler (IME) + try_keystroke
    └── theme.rs             # gpui Theme → TerminalPalette, ensure_minimum_contrast
```

---

## 12. Thứ tự triển khai (roadmap)

> **Trạng thái (bản terminal local đầy đủ):** các bước 1–6 đã hoàn thành.
> Core 55 test + local 17 test (kèm E2E `echo` → snapshot) pass, `cargo build`
> sạch 0 warning, `cargo run` mở terminal cmd thật (ConPTY) không panic.
> SSH (bước 7–8) và perf tuning (bước 9) còn lại.

1. ✅ **`core`**: `TerminalSession` trait, `SessionEvent`, `TerminalContent`, `TerminalPalette`,
   `key_encode`, `mouse_encode`, `osc`/`url`, `ShellKind`/`LocalShellConfig` + `resolve_shell`.
2. ✅ **`local`** (Windows-first): `LocalSession` spawn `cmd` (ConPTY, `chcp 65001`),
   `LocalListener`, snapshot + event. E2E test: `echo oneterm_e2e` → snapshot chứa chuỗi.
3. ✅ **`ui`**: `TerminalElement` vẽ grid + cursor + font measure + resize-on-layout.
   `LocalTerminalView` (`Render`) wire vào DockArea. Settings shell picker (`TerminalSettingsPanel`).
4. ✅ **`ui`**: mouse (down/move/up/wheel), selection (Simple/Semantic/Lines/Block),
   scrollback, hyperlink OSC 8 (Ctrl+click), copy/paste (select-to-copy, middle-click,
   Ctrl+Shift+C/V, OSC 52 clipboard), minimum-contrast.
5. ✅ **`ui`**: IME (`EntityInputHandler` + marked text, `handle_input` ở paint,
   alt-screen → tắt IME, `bounds_for_range` = cursor bounds).
6. ✅ **`local`**: `powershell`/`pwsh`/`bash`/`zsh`/`sh`/`custom`, child exit detection,
   resize, scrollback 10k dòng.
7. ⬜ **`ssh`**: `SshSession` password + key, pty-req + shell + window_change + exit.
8. ⬜ **`ssh`**: known_hosts, agent, reconnect.
9. ⬜ Tuning perf (batch, snapshot diff, debounce notify).

---

## 13. Rủi ro

| Rủi ro | Giải pháp |
|---|---|
| `alacritty_terminal` API nội bộ Zed đổi giữa rev | Pin rev; mở source crate tại rev khi triển khai để khớp signature. |
| Giữ `FairMutex` trong paint → jitter | Snapshot pattern (§5.2). |
| Tokio (ssh) vs smol (gpui) xung đột runtime | Tokio runtime ẩn trong `ssh`, API sync, bridge `std::mpsc` + `async_channel`. |
| Windows cmd codepage không UTF-8 | `chcp 65001` (cmd), env `LANG` (pwsh). Document yêu cầu Win10 1903+ cho ConPTY tốt. |
| `yes` spam → redraw liên tục | Snapshot diff + debounce notify (§6.4). |
| Backpressure channel | `async_channel` bounded + drop-oldest cho output; cmd channel `sync_channel(64)`. |
| IME trên Windows/Linux khác nhau | Dùng `EntityInputHandler` của GPUI ( abstraction sẵn), test cả hai nền tảng. |
| Host key SSH chưa verify | Bắt buộc known_hosts + prompt accept, không disable mặc định. |

---

## 14. Tham chiếu nhanh

| Cần | Đọc |
|---|---|
| Model + EventLoop + PTY (local) | Zed `crates/terminal/src/terminal.rs` (rev `1d217ee39…`) |
| Render grid | Zed `crates/terminal_view/src/terminal_element.rs` |
| IME + View | Zed `crates/terminal_view/src/terminal_view.rs` (`ImeState`) |
| `Element`/`paint_quad`/`shape_line` | `reference/gpui-component` + GPUI docs (rev lock) |
| `EntityInputHandler` | `gpui::EntityInputHandler` trait (docs.rs khớp rev) |
| `alacritty_terminal` API | source tại rev `fcf32fe…` (`event_loop.rs`, `tty/`, `term.rs`, `sync.rs`) — fork `zed-industries/alacritty` |
| freya key/mouse encode | `freya-terminal` `handle.rs`/`parser.rs` (tham khảo logic, thuần hoá vào `core`) |