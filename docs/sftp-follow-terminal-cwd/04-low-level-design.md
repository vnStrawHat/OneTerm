# SFTP theo Terminal CWD — Phần 4: Thiết kế chi tiết (Low-Level Design)

> Chi tiết struct, chữ ký hàm, và code mẫu cho từng crate. Code mẫu minh hoạ ý
> tưởng, có thể tinh chỉnh khi implement thực tế. Thứ tự sửa: `core` → `ssh`/`local`
> → `ui`.

---

## 4.1. `core` — trait `CwdSource` + accessor trên `TerminalSession`

`crates/core/src/terminal/session.rs`:

```rust
use std::path::PathBuf;
use std::sync::Arc;

/// Nguồn đọc "thư mục hiện tại" (OSC 7) của một session — đọc live, không lộ
/// chi tiết implementation của session. Cho phép UI lấy cwd mà không cần giữ
/// tham chiếu tới `Entity` hay import crate `ssh`/`local`.
pub trait CwdSource: Send + Sync {
    /// Thư mục hiện tại (OSC 7). `None` nếu chưa nhận được OSC 7.
    fn cwd(&self) -> Option<PathBuf>;
}

pub trait TerminalSession {
    // ── (đã có) ──
    fn cwd(&self) -> Option<PathBuf>;
    fn sftp(&self) -> Option<Arc<dyn SftpBackend>> { None }

    // ── MỚI ──
    /// Handle đọc cwd live, chia sẻ với UI. `None` = session không cung cấp
    /// (mặc định). SSH/local override để trả `Arc` bọc quanh state chia sẻ.
    fn cwd_source(&self) -> Option<Arc<dyn CwdSource>> {
        None
    }
}
```

Re-export ở `crates/core/src/lib.rs`:

```rust
pub use terminal::{
    CursorBounds, CwdSource, DynamicColors, NetStats, SessionEvent, TerminalInfo,
    TerminalProgress, TerminalSession,
};
```

**Vì sao trait riêng thay vì trả `Arc<Mutex<SessionState>>`?** `SessionState` nằm
trong `ssh`/`local`, không nên lộ ra `core`/`ui`. `CwdSource` là interface tối giản
(1 method) → UI chỉ thấy đúng thứ cần.

---

## 4.2. `ssh` — implement `CwdSource` chia sẻ `SharedState`

`crates/ssh/src/state.rs` — `SharedState = Arc<Mutex<SessionState>>` đã có `cwd`.
Thêm một newtype cung cấp `CwdSource`:

```rust
// crates/ssh/src/state.rs (hoặc session_terminal.rs)
use std::path::PathBuf;
use std::sync::Arc;
use oneterm_core::CwdSource;

/// Đọc `cwd` live từ `SharedState`. Clone rẻ (Arc), chia sẻ đúng state mà
/// listener cập nhật khi nhận OSC 7.
pub struct SshCwdSource {
    state: SharedState,
}

impl SshCwdSource {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }
}

impl CwdSource for SshCwdSource {
    fn cwd(&self) -> Option<PathBuf> {
        self.state.lock().unwrap().cwd.clone()
    }
}
```

`crates/ssh/src/session_terminal.rs` — override accessor:

```rust
fn cwd_source(&self) -> Option<Arc<dyn CwdSource>> {
    Some(Arc::new(SshCwdSource::new(self.state.clone())))
}
```

> `self.state` là `SharedState` (`Arc<Mutex<SessionState>>`) — clone chỉ tăng
> refcount, cùng trỏ tới state mà `handle_osc`/listener ghi `cwd`. Do đó
> `cwd_source().cwd()` luôn phản ánh OSC 7 mới nhất.

`crates/local/src/session_terminal.rs` — tương tự (tùy chọn; local shell cũng có
`cwd`, nhưng SFTP không áp dụng cho local nên có thể bỏ qua override — SFTP panel
sẽ ẩn với local tab). Để nhất quán có thể vẫn implement.

---

## 4.3. `ui` — `AppState` thêm `active_cwd_source`

`crates/ui/src/state/app_state.rs`:

```rust
use std::sync::Arc;
use oneterm_core::{CwdSource, SftpBackend};

pub struct AppState {
    pub dock_area: Option<WeakEntity<DockArea>>,
    pub active_sftp: Option<Arc<dyn SftpBackend>>,
    /// MỚI: nguồn cwd của session active — để SFTP "sync theo terminal".
    /// None = tab active không cung cấp cwd (local, hoặc chưa hỗ trợ).
    pub active_cwd_source: Option<Arc<dyn CwdSource>>,
}
```

Cập nhật khởi tạo `Default`/`new` của `AppState` để thêm field `active_cwd_source: None`.

---

## 4.4. `ui` — `TerminalPanel::set_active` set thêm cwd source

`crates/ui/src/views/terminal/panel.rs`:

```rust
fn set_active(&mut self, active: bool, _: &mut Window, cx: &mut Context<Self>) {
    if self.is_active != active {
        self.is_active = active;
        cx.notify();
    }

    if active {
        let session = self.view.read(cx).session.read(cx);
        let sftp = session.sftp();
        let cwd_source = session.cwd_source();   // ← MỚI
        AppState::global(cx).update(cx, |state, cx| {
            state.active_sftp = sftp;
            state.active_cwd_source = cwd_source;  // ← MỚI
            cx.notify();
        });
    }
}
```

> Lưu ý mượn `session` một lần rồi lấy cả 2 giá trị để tránh gọi `.read(cx)` hai
> lần. Nếu borrow checker phàn nàn, tách thành 2 câu `let sftp = ...; let cwd_source = ...;`
> mỗi câu tự `read`.

---

## 4.5. `ui` — `SftpPanel` giữ `cwd_source` + observe

`crates/ui/src/views/sftp/panel.rs` — thêm field:

```rust
use oneterm_core::{CwdSource, FileEntry, SftpBackend};

pub struct SftpPanel {
    // ... các field cũ ...
    pub(crate) sftp: Option<Arc<dyn SftpBackend>>,
    /// MỚI: nguồn cwd của terminal active (để nút Sync đọc live).
    pub(crate) cwd_source: Option<Arc<dyn CwdSource>>,
    // ...
}
```

Khởi tạo trong `new`: `cwd_source: None`.

Cập nhật trong observe (cùng block đang xử lý `active_sftp`):

```rust
cx.observe(&app_state, |this, state, cx| {
    let st = state.read(cx);
    let new_sftp = st.active_sftp.clone();
    let new_cwd_source = st.active_cwd_source.clone();   // ← MỚI

    // Luôn cập nhật cwd_source theo tab active (kể cả khi sftp không đổi).
    this.cwd_source = new_cwd_source;                    // ← MỚI

    if sftp_changed(&this.sftp, &new_sftp) {
        this.sftp = new_sftp;
        // ... reset như cũ ...
        if this.sftp.is_some() {
            this.load_dir(PathBuf::from("."), cx);
        }
    }
    cx.notify();
}).detach();
```

Handler sync:

```rust
impl SftpPanel {
    /// Chuyển SFTP Browser tới thư mục hiện tại của terminal active.
    /// No-op nếu không có SFTP hoặc terminal chưa báo cwd.
    pub(crate) fn sync_to_terminal_cwd(&mut self, cx: &mut Context<Self>) {
        if self.sftp.is_none() {
            return;
        }
        let cwd = match self.cwd_source.as_ref().and_then(|s| s.cwd()) {
            Some(p) => p,
            None => {
                log::debug!("SftpPanel::sync_to_terminal_cwd: terminal cwd unavailable");
                return;
            }
        };
        log::info!(
            "SftpPanel::sync_to_terminal_cwd: \"{}\" → \"{}\"",
            self.cwd.display(),
            cwd.display()
        );
        // goto_path đã stat + xử lý lỗi + load_dir.
        self.goto_path(cwd, cx);
    }

    /// Cwd hiện tại của terminal (để render tính trạng thái nút + tooltip).
    pub(crate) fn terminal_cwd(&self) -> Option<PathBuf> {
        self.cwd_source.as_ref().and_then(|s| s.cwd())
    }
}
```

> `goto_path` hiện là `fn` private (không `pub(crate)`) — cần giữ nguyên vì
> `sync_to_terminal_cwd` cùng `impl`/module nên gọi được. Nếu tách module, đổi
> visibility thành `pub(crate)`.

---

## 4.6. `ui` — nút trên toolbar

`crates/ui/src/views/sftp/render.rs`, trong `render_toolbar`, chèn nút giữa Back và
Refresh (hoặc cạnh Refresh). Tính trạng thái trước khi build hàng:

```rust
// Trong render_toolbar, trước khi build h_flex():
let terminal_cwd = self.terminal_cwd();          // Option<PathBuf>
let sync_enabled = terminal_cwd.is_some();
let sync_tooltip = match &terminal_cwd {
    Some(p) => format!("Đến thư mục hiện tại của terminal: {}", p.display()),
    None => "Terminal chưa báo thư mục hiện tại (cần shell integration / OSC 7)"
        .to_string(),
};
```

Nút (đặt trong chuỗi `.child(...)` của toolbar `h_flex`):

```rust
// Sync-to-terminal-cwd button
.child(
    Button::new("sftp-sync-cwd")
        .icon(Icon::new(IconName::FolderSync).small())   // xem 4.7 về icon
        .small()
        .ghost()
        .disabled(!sync_enabled)
        .tooltip(sync_tooltip)
        .on_click(cx.listener(|this, _, _, cx| {
            this.sync_to_terminal_cwd(cx);
        })),
)
```

> `Button` của gpui-component hỗ trợ `.disabled(bool)` và `.tooltip(impl Into<SharedString>)`.
> Xác nhận API chính xác trong `reference/gpui-component/crates/ui/src/button/`. Nếu
> `.tooltip` nhận closure/`Tooltip`, chỉnh theo signature thực tế (tra reference-first
> theo AGENTS.md §3.0).

---

## 4.7. Icon

Cần một icon gợi ý "đồng bộ thư mục / nhảy theo". Ưu tiên tên có sẵn trong
`IconName` (tra `reference/gpui-component/crates/ui/src/icon.rs`). Ứng viên:
`FolderSync`, `FolderInput`, `LocateFixed`, `Crosshair`, `RefreshCw`.

- Nếu tên chưa có trong `IconName` của gpui-component → thêm SVG Lucide vào
  `crates/ui/assets/icons/<name>.svg` và đăng ký qua `AppIcon` (giống `AppIcon::Refresh`
  đang dùng cho nút Refresh). Xem AGENTS.md §3.4 (Theme & icon).
- Đặt tên file SVG trùng tên biến trong `AppIcon`/`IconName`.

Đề xuất: dùng `AppIcon::FolderSync` (thêm mới) hoặc tái dùng một icon "target/locate"
sẵn có để giảm việc thêm asset.

---

## 4.8. Tổng hợp thay đổi theo file

| Crate | File | Thay đổi |
|-------|------|----------|
| core | `terminal/session.rs` | + trait `CwdSource`; + `fn cwd_source()` (default `None`) trên `TerminalSession` |
| core | `lib.rs` | + re-export `CwdSource` |
| ssh | `state.rs` (hoặc `session_terminal.rs`) | + struct `SshCwdSource` impl `CwdSource` |
| ssh | `session_terminal.rs` | + override `fn cwd_source()` |
| local | `session_terminal.rs` | *(tùy chọn)* + override `fn cwd_source()` |
| ui | `state/app_state.rs` | + field `active_cwd_source` + cập nhật khởi tạo |
| ui | `views/terminal/panel.rs` | `set_active`: set `active_cwd_source` |
| ui | `views/sftp/panel.rs` | + field `cwd_source`; observe cập nhật; + `sync_to_terminal_cwd`, `terminal_cwd` |
| ui | `views/sftp/render.rs` | `render_toolbar`: + nút Sync (disabled/tooltip theo trạng thái) |
| ui | `assets/icons/` + `icon.rs` | *(nếu cần)* + icon mới |

---

## 4.9. Phương án thay thế cho §4.1–4.2 (không thêm trait mới)

Nếu team muốn tránh trait `CwdSource`, có thể lưu **`WeakEntity<Box<dyn TerminalSession>>`**
vào `AppState` (phương án B ở phần 03) và trong `SftpPanel` gọi:

```rust
let cwd = self.active_session
    .as_ref()
    .and_then(|w| w.upgrade())
    .and_then(|e| e.read(cx).cwd());
```

- **Ưu:** không thêm type ở `core`; dùng thẳng `cwd()` đã có.
- **Nhược:** `AppState`/`SftpPanel` phải biết kiểu `Entity<Box<dyn TerminalSession>>`
  (đây là kiểu `ui`-side nên vẫn hợp lệ về layering); cần `cx` để `read` (đã có trong
  handler); phải quản lý `WeakEntity` lifetime.

Khuyến nghị: **phương án C (`CwdSource`)** cho ranh giới sạch và mở đường auto-follow;
phương án B nếu muốn thay đổi tối thiểu và chấp nhận `SftpPanel` giữ weak-ref session.
