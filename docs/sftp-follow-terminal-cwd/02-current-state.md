# SFTP theo Terminal CWD — Phần 2: Hiện trạng codebase

> Phần này ghi lại chính xác các mảnh code liên quan **đã tồn tại**, để thiết kế
> ở phần 03/04 chỉ cần "nối dây" chứ không xây lại. Tất cả trích dẫn dưới đây
> verify từ code hiện tại (không phải giả định).

---

## 2.1. `cwd` đã được track và expose sẵn

### Ở `core` — trait `TerminalSession`

`crates/core/src/terminal/session.rs`:

```rust
pub trait TerminalSession {
    // ...
    /// The current title (OSC 0/2).
    fn title(&self) -> Option<String>;
    /// The current cwd (OSC 7).
    fn cwd(&self) -> Option<PathBuf>;
    // ...
    /// SFTP backend if the session has an SFTP channel (SSH only).
    /// `None` for a local shell.
    fn sftp(&self) -> Option<Arc<dyn SftpBackend>> { None }
}
```

→ **`cwd()` đã có sẵn trên trait.** UI có thể gọi mà không cần import `ssh`/`local`.

### Ở `ssh` — nguồn của `cwd`

`crates/ssh/src/state.rs` — `SessionState`:

```rust
pub struct SessionState {
    pub title: Option<String>,
    /// Cwd (OSC 7 — set by the side-channel parser).
    pub cwd: Option<PathBuf>,
    // ...
}
```

`crates/ssh/src/task.rs` — parse OSC 7 từ luồng shell rồi ghi vào state:

```rust
fn handle_osc(payload: &OscPayload, state: &SharedState, listener: &SshListener) {
    match payload {
        OscPayload::Cwd(url) => {
            let cwd = parse_cwd_url(url);          // file://host/path → PathBuf
            { /* state.lock().cwd = cwd.clone() */ }
            listener.forward(SessionEvent::Cwd(cwd));   // ← có event Cwd!
        }
        // ...
    }
}
```

`crates/ssh/src/session_terminal.rs` — accessor đọc lại:

```rust
fn cwd(&self) -> Option<PathBuf> {
    self.state.lock().unwrap().cwd.clone()
}
```

**Hai điểm quan trọng:**
- `cwd` cập nhật **live** mỗi lần remote shell phát OSC 7 (thường sau mỗi prompt).
- Đã có **`SessionEvent::Cwd(...)`** được `forward` — đây là hook sẵn có cho phương
  án auto-follow (phần 03/05) mà không cần polling.

`crates/local/src/session_terminal.rs` có `fn cwd()` tương tự cho local shell.

---

## 2.2. `SftpPanel` — đã có `load_dir` / `goto_path`

`crates/ui/src/views/sftp/panel.rs`:

```rust
pub struct SftpPanel {
    pub(crate) sftp: Option<Arc<dyn SftpBackend>>,
    pub(crate) cwd: PathBuf,
    // table, selected, transfers, path_input, ...
}

impl SftpPanel {
    /// Read a directory — spawn background task, không block UI.
    pub fn load_dir(&mut self, path: PathBuf, cx: &mut Context<Self>) { /* ... */ }

    /// Goto path — stat trước; nếu là dir thì load_dir, nếu lỗi → path_error.
    fn goto_path(&mut self, path: PathBuf, cx: &mut Context<Self>) { /* ... */ }

    pub(crate) fn navigate_parent(&mut self, cx: &mut Context<Self>) { /* ... */ }
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) { /* ... */ }
    pub(crate) fn navigate_into(&mut self, idx: usize, cx: &mut Context<Self>) { /* ... */ }
}
```

→ Điều hướng SFTP tới 1 path bất kỳ **đã có** (`load_dir` / `goto_path`). Tính năng
chỉ cần gọi `load_dir(cwd_cua_terminal)`.

Lưu ý về **kiểu path**: `goto_path` gọi `sftp.stat(path)` để kiểm tra path tồn tại
+ là thư mục trước khi load. `cwd` từ OSC 7 là **đường dẫn tuyệt đối phía remote**
(POSIX, ví dụ `/var/www/html`) — dùng trực tiếp cho SFTP `read_dir`/`stat` được.

---

## 2.3. `AppState.active_sftp` — pattern "panel global, session per-tab"

`crates/ui/src/state/app_state.rs`:

```rust
pub struct AppState {
    pub dock_area: Option<WeakEntity<DockArea>>,
    /// SFTP backend của active SSH tab.
    /// None = local shell hoặc SSH không hỗ trợ SFTP.
    pub active_sftp: Option<Arc<dyn SftpBackend>>,
}
```

`SftpPanel` **observe** `AppState` và swap backend khi tab đổi:

```rust
cx.observe(&app_state, |this, state, cx| {
    let new_sftp = state.read(cx).active_sftp.clone();
    if sftp_changed(&this.sftp, &new_sftp) {
        this.sftp = new_sftp;
        // reset cwd/selection/transfers ...
        if this.sftp.is_some() { this.load_dir(PathBuf::from("."), cx); }
    }
    cx.notify();
}).detach();
```

Ai set `active_sftp`? — `TerminalPanel::set_active`
(`crates/ui/src/views/terminal/panel.rs`):

```rust
fn set_active(&mut self, active: bool, _: &mut Window, cx: &mut Context<Self>) {
    // ...
    if active {
        let sftp = self.view.read(cx).session.read(cx).sftp();
        AppState::global(cx).update(cx, |state, cx| {
            state.active_sftp = sftp;
            cx.notify();
        });
    }
}
```

**Đây là chỗ chốt của thiết kế:** cùng thời điểm lấy `sftp()`, ta cũng có `session`
— tức có thể lấy được `cwd()`. Session lưu dưới dạng
`Entity<Box<dyn TerminalSession>>` trong `LocalTerminalView`:

```rust
// crates/ui/src/views/terminal/view/mod.rs
pub struct LocalTerminalView {
    pub(crate) session: Entity<Box<dyn TerminalSession>>,
    // ...
}
```

---

## 2.4. Toolbar SFTP — nơi đặt nút mới

`crates/ui/src/views/sftp/render.rs` — `render_toolbar` hiện có: path input (flex-1)
+ nút Back + nút Refresh + nút "..." (menu). Các nút dùng `gpui_component::button::Button`
với `.icon(...).small().ghost().on_click(cx.listener(...))`. Ví dụ nút Refresh:

```rust
.child(
    Button::new("sftp-refresh")
        .icon(Icon::new(AppIcon::Refresh).small())
        .small()
        .ghost()
        .on_click(cx.listener(|this, _, _, cx| {
            this.refresh(cx);
        })),
)
```

→ Nút mới sẽ chèn vào đúng hàng toolbar này, cùng style.

---

## 2.5. Khoảng trống cần lấp (gap analysis)

| # | Gap | Chi tiết | Hướng giải quyết (phần 03/04) |
|---|-----|----------|-------------------------------|
| G1 | **SftpPanel không có đường lấy `cwd` của terminal** | Panel chỉ giữ `Arc<dyn SftpBackend>`, không có tham chiếu tới session/terminal. | Bổ sung 1 kênh cung cấp `cwd` live vào `AppState` (song song với `active_sftp`). |
| G2 | **`cwd` cần đọc live tại thời điểm click** | Không thể snapshot lúc `set_active` vì user `cd` sau đó. | Lưu cách "hỏi cwd" (một provider callable) chứ không lưu giá trị `cwd`. |
| G3 | **Chưa có nút trên toolbar** | `render_toolbar` chưa có nút sync. | Thêm `Button` + handler `sync_to_terminal_cwd`. |
| G4 | **Trạng thái disabled khi thiếu cwd/SFTP** | Cần biết "có cwd hay không" để bật/tắt nút. | Provider trả `Option<PathBuf>`; `None` → disable. |
| G5 | *(mở rộng)* **Auto-follow chưa có kênh sự kiện tới SFTP** | `SessionEvent::Cwd` có ở ssh nhưng chưa nối tới `SftpPanel`. | Tùy chọn: forward event → observe trong SftpPanel. |
