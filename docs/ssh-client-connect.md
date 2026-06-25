# Thiết kế SSH Client Connect — myTerm2

> Tài liệu thiết kế cho chức năng kết nối SSH: click vào item trong SSH Session →
> mở phiên SSH tới server đích, kèm dialog nhập credentials khi cần.
>
> **Tham chiếu liên quan:**
> - [`docs/terminal-backend.md`](terminal-backend.md) §7 — thiết kế `SshSession`
>   (russh + tokio runtime ẩn, `SshConfig`, auth).
> - [`docs/gui-layout.md`](gui-layout.md) — DockArea, Panel trait, TerminalPanel.
> - [`docs/agents/structure.md`](agents/structure.md) — quy tắc crate, cây thư mục.

## Mục lục

1. [Tổng quan & flow](#1-tổng-quan--flow)
2. [Cấu trúc dữ liệu](#2-cấu-trúc-dữ-liệu)
3. [Logic phân nhánh credential dialog](#3-logic-phân-nhánh-credential-dialog)
4. [Dialog UI — Connect SSH](#4-dialog-ui--connect-ssh)
5. [Connection flow — tạo SshSession + mở tab](#5-connection-flow--tạo-sshsession--mở-tab)
6. [Tích hợp vào SessionPanel](#6-tích-hợp-vào-sessionpanel)
7. [Cấu trúc file](#7-cấu-trúc-file)
8. [Implementation checklist](#8-implementation-checklist)

---

## 1. Tổng quan & flow

### 1.1. Mô tả chức năng

Khi user click (left-click) vào 1 item trong danh sách SSH Session ở
`SessionPanel` (right dock), app mở phiên SSH tới server đích theo thông tin
trong `SshSession` (host, port, username). Trước khi kết nối, nếu thiếu
credentials (username hoặc password), app hiển thị dialog để user nhập.

### 1.2. Sơ đồ flow

```
┌─────────────────────────────────────────────────────────────────────┐
│  User click vào session item trong SessionPanel                     │
│  (render_session_row → on_click)                                    │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │  Đọc SshSession     │
                    │  từ SshSessionStore │
                    │  (label, host,      │
                    │   port, username)   │
                    └──────────┬──────────┘
                               │
                               ▼
                    ┌─────────────────────┐      username = None
                    │  username có không? │──────────────────┐
                    └──────────┬──────────┘                  │
                               │ Some                         │ No
                               ▼                              │
                    ┌─────────────────────┐                  │
                    │  Dialog nhập        │                  ▼
                    │  PASSWORD only      │         ┌────────────────────┐
                    │  (1 field masked)   │         │  Dialog nhập       │
                    └──────────┬──────────┘         │  USERNAME + PASSWORD│
                               │                    │  (2 fields)        │
                               │                    └────────┬───────────┘
                               │                             │
                               ▼                             ▼
                    ┌──────────────────────────────────────────────┐
                    │  User click Connect                           │
                    │  (hoặc Cancel → hủy)                          │
                    └──────────────────────┬───────────────────────┘
                                           │ Connect
                                           ▼
                    ┌──────────────────────────────────────────────┐
                    │  Tạo SshConfig { host, port, username,        │
                    │    password, auth_method: Password }          │
                    └──────────────────────┬───────────────────────┘
                                           │
                                           ▼
                    ┌──────────────────────────────────────────────┐
                    │  SshSession::connect(cfg, pty_size)           │
                    │  → russh connect + auth + pty-req + shell     │
                    │  (xem terminal-backend.md §7)                 │
                    └──────────────────────┬───────────────────────┘
                                           │ Ok(session)
                                           ▼
                    ┌──────────────────────────────────────────────┐
                    │  Tạo TerminalPanel mới với SshSession         │
                    │  (thay vì LocalSession mặc định)              │
                    │  → add_panel vào DockArea center              │
                    │  → tab title = session.label                 │
                    └──────────────────────────────────────────────┘
```

### 1.3. Quyết định thiết kế

| # | Quyết định | Lý do |
|---|---|---|
| 1 | **Password KHÔNG persist** vào `ssh_session.json` | Bảo mật — password chỉ giữ trong RAM trong phiên làm việc, không ghi ra disk. |
| 2 | **Username persist** vào `ssh_session.json` (đã có sẵn field) | Tiện lợi — user chỉ nhập password lần sau. Username không nhạy cảm như password. |
| 3 | **Dùng `LocalTerminalView`** cho cả SSH (qua `dyn TerminalSession`) | View đã thiết kế backend-agnostic — chỉ cần `Entity<Box<dyn TerminalSession>>`. Không cần `SshTerminalView` riêng. |
| 4 | **Dialog dùng `window.open_dialog`** (gpui-component Dialog) | Khớp pattern đã dùng cho "New/Edit SSH Session" dialog trong `session_tabs/tabs.rs`. |
| 5 | **Password field dùng `InputState::masked(true)` + `.mask_toggle()`** | Hiển thị `•••••`, có nút eye-icon reveal/hide. API sẵn có trong gpui-component. |
| 6 | **Footer: Cancel (trái) + Connect (phải), căn lề phải** | `DialogFooter` mặc định `justify_end` → button tự căn phải. Khớp yêu cầu. |
| 7 | **Connect chạy async** — dialog đóng ngay, kết nối chạy nền | Tránh block UI. Nếu lỗi connect → `window.push_notification` báo lỗi. |
| 8 | **Left-click = Open**, right-click giữ context menu (Open/Delete/Property) | Giữ hành vi hiện tại của context menu, thêm left-click shortcut. |

---

## 2. Cấu trúc dữ liệu

### 2.1. `SshConfig` — cấu hình kết nối (crate `ssh`)

Định nghĩa trong `crates/ssh/src/config.rs`, re-export qua `ssh::lib.rs`.
Đây là input cho `SshSession::connect()`.

```rust
use std::path::PathBuf;

/// Phương thức xác thực SSH.
#[derive(Debug, Clone)]
pub enum SshAuthMethod {
    /// Xác thực bằng password.
    Password { password: String },
    /// Xác thực bằng private key file (sẽ triển khai sau).
    PrivateKey {
        key_path: PathBuf,
        passphrase: Option<String>,
    },
    /// SSH agent (sẽ triển khai sau).
    Agent,
}

/// Cấu hình kết nối SSH — input cho [`crate::SshSession::connect`].
#[derive(Debug, Clone)]
pub struct SshConfig {
    /// Hostname hoặc IP.
    pub host: String,
    /// Cổng SSH (mặc định 22).
    pub port: u16,
    /// Username SSH.
    pub username: String,
    /// Phương thức xác thực.
    pub auth: SshAuthMethod,
}
```

> **Lưu ý:** `SshConfig` chứa `password` ở dạng `String` (plaintext trong RAM).
> Không serialize `SshConfig` ra disk. Password chỉ tồn tại trong memory trong
> thời gian kết nối + phiên làm việc.

### 2.2. Mở rộng `SshSession` (state) — thêm field `password`?

**KHÔNG.** `SshSession` trong `session_state.rs` (UI store) giữ nguyên 4 field:
`label, host, port, username`. Password không lưu vào store — nó chỉ là input
ephemeral cho `SshConfig` khi connect.

```rust
// session_state.rs — KHÔNG đổi
pub struct SshSession {
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,  // None → dialog sẽ hỏi
}
```

### 2.3. `SshConnectParams` — bundle thông tin connect (UI crate)

Struct trung gian chứa mọi thứ cần để mở dialog + connect, tạo từ `SshSession`
khi user click.

```rust
/// Thông tin cần thiết để mở dialog connect + tạo SshConfig.
/// Tạo từ `SshSession` khi user click vào item.
pub(crate) struct SshConnectParams {
    pub label: String,       // cho tab title + dialog title
    pub host: String,
    pub port: u16,
    pub username: Option<String>,  // None → dialog hỏi username
}
```

---

## 3. Logic phân nhánh credential dialog

### 3.1. Bảng quyết định

| `SshSession.username` | Dialog hiển thị | Fields |
|---|---|---|
| `None` (chưa có username) | **Username + Password** | 2 input: username (text) + password (masked) |
| `Some(u)` (đã có username) | **Password only** | 1 input: password (masked), username hiển thị read-only |

### 3.2. Pseudocode

```text
fn on_session_click(session: &SshSession):
    match session.username:
        None  → open_connect_dialog(session, ask_username=true)
        Some  → open_connect_dialog(session, ask_username=false)
```

### 3.3. Dialog title

- Khi hỏi username + password: `"Connect to {label}"` (vd `"Connect to Production Server"`)
- Khi chỉ hỏi password: `"Connect to {label} ({username}@{host}:{port})"`

Subtitle trong dialog content hiển thị thông tin server:
- `"ssh://{username}@{host}:{port}"` (khi có username)
- `"ssh://{host}:{port}"` (khi chưa có username)

---

## 4. Dialog UI — Connect SSH

### 4.1. Layout dialog

```
┌─ Connect to Production Server ──────────────────────────┐
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │  ssh://ubuntu@10.0.0.1:22                        │   │  ← server info (read-only)
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Username *                          ← chỉ hiện khi ask_username=true
│  ┌──────────────────────────────────────────────────┐   │
│  │  [input text]                                    │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Password *                                              │
│  ┌──────────────────────────────────────────────────┐   │
│  │  [••••••••••]                              [👁]  │   │  ← masked + mask_toggle
│  └──────────────────────────────────────────────────┘   │
│                                                          │
├──────────────────────────────────────────────────────────┤
│                              [Cancel]  [Connect]        │  ← footer, justify_end
└──────────────────────────────────────────────────────────┘
```

**Khi `ask_username = false`** (đã có username), field Username bị ẩn,
dialog ngắn hơn:

```
┌─ Connect to Production Server (ubuntu@10.0.0.1:22) ─────┐
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │  ssh://ubuntu@10.0.0.1:22                        │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Password *                                              │
│  ┌──────────────────────────────────────────────────┐   │
│  │  [••••••••••]                              [👁]  │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
├──────────────────────────────────────────────────────────┤
│                              [Cancel]  [Connect]        │
└──────────────────────────────────────────────────────────┘
```

### 4.2. Footer — Connect + Cancel, căn lề phải

`DialogFooter` (gpui-component) mặc định `h_flex().gap_2().justify_end()` —
tức là children tự căn **phải**. Thứ tự: Cancel (trái) → Connect (phải).

```rust
.footer(
    DialogFooter::new()
        .child(
            DialogClose::new().child(
                Button::new("cancel")
                    .label("Cancel")
                    .outline(),
            ),
        )
        .child(
            DialogAction::new().child(
                Button::new("connect")
                    .label("Connect")
                    .primary(),
            ),
        ),
)
```

- **Cancel** = `DialogClose` → dispatch `CancelDialog` → đóng dialog, không
  thực hiện gì. `on_cancel` trả `true` (cho phép đóng).
- **Connect** = `DialogAction` → dispatch `ConfirmDialog` → gọi `on_ok`
  closure. `on_ok` đọc input values, validate, tạo `SshConfig`, gọi
  `connect_and_open_terminal()`. Trả `true` để đóng dialog.

### 4.3. Password input — masked + mask_toggle

```rust
// InputState cho password — masked ngay từ đầu.
let password_state = cx.new(|cx| {
    InputState::new(window, cx)
        .placeholder("Enter password")
        .masked(true)             // ← hiển thị ••••••
});

// Input element — mask_toggle thêm nút eye-icon reveal/hide.
Input::new(&password_state)
    .mask_toggle()                 // ← nút 👁 toggle reveal
    .cleanable(true)               // ← nút × clear
```

API tham chiếu (gpui-component):
- `InputState::masked(bool)` — `reference/.../input/state.rs:874`
- `Input::mask_toggle()` — `reference/.../input/input.rs:144`
- Example: `reference/.../stories/input_story.rs:73` (`.masked(true)` +
  `.placeholder("Enter your password...")`)

### 4.4. Username input (khi `ask_username = true`)

```rust
let username_state = cx.new(|cx| {
    InputState::new(window, cx)
        .placeholder("e.g. root, ubuntu, admin")
});

Input::new(&username_state)  // text thường, không masked
```

### 4.5. Server info banner (read-only)

Hiển thị thông tin server phía trên fields, dùng `div` + text, không phải input.
Giúp user xác nhận đang kết nối tới server đúng.

```rust
// Server info banner
let server_info = match &params.username {
    Some(u) => format!("ssh://{}@{}:{}", u, params.host, params.port),
    None => format!("ssh://{}:{}", params.host, params.port),
};

div()
    .w_full()
    .px_3()
    .py_2()
    .rounded_md()
    .bg(theme.muted)
    .text_sm()
    .text_color(theme.muted_foreground)
    .child(server_info)
```

### 4.6. Validation

| Field | Điều kiện | Khi fail |
|---|---|---|
| Username (nếu hỏi) | Không rỗng sau trim | `window.push_notification("Username là bắt buộc.")`, trả `false` (không đóng dialog) |
| Password | Không rỗng sau trim | `window.push_notification("Password là bắt buộc.")`, trả `false` |

---

## 5. Connection flow — tạo SshSession + mở tab

### 5.1. `on_ok` closure — khi user click Connect

```rust
.on_ok({
    let params = params.clone();
    let username_state = username_state.clone();  // None nếu không hỏi
    let password_state = password_state.clone();
    let dock_area = dock_area.clone();            // WeakEntity<DockArea>
    move |_, window, cx| {
        // 1. Đọc + validate inputs
        let username = match &username_state {
            Some(st) => {
                let u = st.read(cx).value().trim().to_string();
                if u.is_empty() {
                    window.push_notification("Username là bắt buộc.", cx);
                    return false;
                }
                u
            }
            None => params.username.clone().unwrap_or_default(),
        };

        let password = password_state.read(cx).value().to_string();
        if password.is_empty() {
            window.push_notification("Password là bắt buộc.", cx);
            return false;
        }

        // 2. Tạo SshConfig
        let cfg = SshConfig {
            host: params.host.clone(),
            port: params.port,
            username: username.clone(),
            auth: SshAuthMethod::Password { password },
        };

        // 3. (Tuỳ chọn) Lưu username lại vào store nếu user nhập mới
        if username_state.is_some() {
            // params.session_index → store.update(index, session với username mới)
            // Tiện lợi cho lần connect sau
        }

        // 4. Connect async + mở tab
        cx.spawn_in(window, async move |_this, window| {
            // SshSession::connect là sync (block_on bên trong) — chạy trên
            // background executor để không block UI.
            let result = window.background_executor().spawn(async move {
                myterm2_ssh::SshSession::connect(
                    cfg,
                    PtySize { rows: 24, cols: 80 },
                    10_000,  // scrollback
                )
            }).await;

            window.update(|window, cx| {
                match result {
                    Ok(session) => {
                        let panel = create_ssh_terminal_panel(
                            session,
                            &params.label,
                            window, cx,
                        );
                        add_terminal_to_dock(&dock_area, panel, window, cx);
                    }
                    Err(e) => {
                        window.push_notification(
                            format!("SSH connect failed: {e}"),
                            cx,
                        );
                    }
                }
            }).ok();
        }).detach();

        true  // đóng dialog
    }
})
```

### 5.2. Tạo TerminalPanel với SSH session

`TerminalPanel` hiện tại tự tạo `LocalSession` trong `new()`. Cần thêm
constructor nhận `Box<dyn TerminalSession>` từ ngoài (factory pattern —
đã được TODO ghi nhận trong `panel.rs`).

```rust
impl TerminalPanel {
    /// Tạo panel từ session có sẵn (SSH hoặc local).
    /// Session đã spawn/connect xong, panel chỉ wrap view.
    pub fn from_session(
        session: Box<dyn TerminalSession>,
        title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let session_entity = cx.new(|_| session);
        let view = cx.new(|cx| LocalTerminalView::new(session_entity, window, cx));
        view.read(cx).focus_handle(cx).focus(window, cx);
        Self {
            view,
            tab_panel: None,
            is_active: false,
            tab_title: title.to_string(),  // ← thêm field mới
        }
    }

    pub fn from_session_entity(
        session: Box<dyn TerminalSession>,
        title: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::from_session(session, title, window, cx))
    }
}
```

**Thay đổi `TerminalPanel`:**
- Thêm field `tab_title: String` (mặc định `"Terminal"` cho local).
- `title()` trong `impl Panel` dùng `self.tab_title` thay vì hardcode `"Terminal"`.

```rust
pub struct TerminalPanel {
    view: Entity<LocalTerminalView>,
    tab_panel: Option<WeakEntity<TabPanel>>,
    is_active: bool,
    tab_title: String,  // ← MỚI
}

// Trong Panel::title()
.child(div()
    .flex_1()
    .overflow_hidden()
    .text_ellipsis()
    .whitespace_nowrap()
    .child(self.tab_title.clone()),  // ← thay "Terminal"
```

### 5.3. Thêm SSH terminal tab vào DockArea

Dùng cùng logic như `on_action_add_panel` (xem `workspace/actions.rs`):

```rust
fn add_ssh_terminal_to_dock(
    dock_area: &WeakEntity<DockArea>,
    panel: Arc<dyn PanelView>,
    window: &mut Window,
    cx: &mut App,
) {
    // Kiểm tra center có tab nào không (xử lý edge case tất cả tab đã đóng).
    let center_empty = dock_area.read_with(cx, |dock, cx| {
        super::center_has_no_visible_panel(&dock.center(), cx)
    }).unwrap_or(false);

    if center_empty {
        let weak = dock_area.clone();
        let center = DockItem::v_split(
            vec![DockItem::tabs(vec![panel], &weak, window, cx)],
            &weak, window, cx,
        );
        dock_area.update(cx, |dock, cx| {
            dock.set_center(center, window, cx);
        }).ok();
    } else {
        dock_area.update(cx, |dock, cx| {
            dock.add_panel(panel, DockPlacement::Center, None, window, cx);
        }).ok();
    }
}
```

---

## 6. Tích hợp vào SessionPanel

### 6.1. Thêm left-click handler vào `render_session_row`

Hiện tại `render_session_row` chỉ có `context_menu` (right-click). Thêm
`.on_click` cho left-click:

```rust
fn render_session_row(
    ix: usize,
    session: &SshSession,
    focus: &FocusHandle,
    cx: &App,
) -> impl IntoElement {
    // ... (giữ nguyên code hiện tại)

    div()
        .id(("session-row", ix))
        .w_full()
        .px_2()
        .py_1p5()
        .rounded_md()
        .cursor_pointer()
        .hover(|t| t.bg(theme.muted))
        // ← MỚI: left-click → open SSH session
        .on_click(move |_, window, cx| {
            let session = SshSessionStore::global(cx)
                .read(cx)
                .sessions()
                .get(ix)
                .cloned();
            if let Some(s) = session {
                open_connect_dialog(s, ix, window, cx);
            }
        })
        // ... (giữ nguyên children + context_menu)
}
```

### 6.2. Cập nhật context menu "Open"

Context menu "Open" hiện tại chỉ push notification "chưa triển khai".
Thay bằng gọi cùng `open_connect_dialog`:

```rust
.item(PopupMenuItem::new("Open").on_click(move |_, window, cx| {
    let session = SshSessionStore::global(cx)
        .read(cx)
        .sessions()
        .get(ix)
        .cloned();
    if let Some(s) = session {
        open_connect_dialog(s, ix, window, cx);
    }
}))
```

### 6.3. `open_connect_dialog` — hàm chính

Đặt trong `session_tabs/tabs.rs` (cùng file với `open_session_dialog`).
Cần tham chiếu `WeakEntity<DockArea>` — lấy từ `SessionPanel` hoặc global.

```rust
/// Mở dialog connect SSH.
///
/// - `session`: thông tin SSH session từ store.
/// - `index`: vị trí trong store (để update username nếu user nhập mới).
///
/// Logic phân nhánh:
/// - `session.username = None` → dialog hỏi username + password.
/// - `session.username = Some` → dialog chỉ hỏi password.
fn open_connect_dialog(
    session: SshSession,
    index: usize,
    window: &mut Window,
    cx: &mut App,
) {
    let ask_username = session.username.is_none();

    // Dialog title
    let title = if ask_username {
        format!("Connect to {}", session.label)
    } else {
        let u = session.username.as_deref().unwrap_or("");
        format!("Connect to {} ({}@{}:{})", session.label, u, session.host, session.port)
    };

    // Server info banner text
    let server_info = match &session.username {
        Some(u) => format!("ssh://{}@{}:{}", u, session.host, session.port),
        None => format!("ssh://{}:{}", session.host, session.port),
    };

    // Password state — luôn cần, masked.
    let password_state = cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder("Enter password")
            .masked(true)
    });

    // Username state — chỉ tạo khi cần hỏi.
    let username_state: Option<Entity<InputState>> = if ask_username {
        Some(cx.new(|cx| {
            InputState::new(window, cx).placeholder("e.g. root, ubuntu, admin")
        }))
    } else {
        None
    };

    // DockArea weak ref — cần để add terminal tab sau khi connect.
    // Lấy qua AppState hoặc truyền vào từ SessionPanel.
    let dock_area = get_dock_area(cx);  // helper — xem §6.4

    // Clone cho on_ok closure
    let password_ok = password_state.clone();
    let username_ok = username_state.clone();
    let session_ok = session.clone();
    let dock_area_ok = dock_area.clone();

    window.open_dialog(cx, move |dialog, _window, _cx| {
        dialog
            .title(title.as_str())  // NOTE: cần clone title vào closure
            .w(px(440.))
            .content({
                let server_info = server_info.clone();
                let username_state = username_state.clone();
                let password_state = password_state.clone();
                move |content, _window, cx| {
                    let theme = cx.theme();
                    content
                        // Server info banner
                        .child(
                            div()
                                .w_full()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(theme.muted)
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(server_info),
                        )
                        // Username field (chỉ khi ask_username)
                        .when_some(username_state, |content, st| {
                            content.child(field("Username", true, Input::new(&st), cx))
                        })
                        // Password field (luôn)
                        .child(
                            v_flex()
                                .gap_1()
                                .w_full()
                                .child(
                                    h_flex()
                                        .gap_1()
                                        .text_sm()
                                        .child(SharedString::from("Password"))
                                        .child(div().text_color(cx.theme().danger).child("*")),
                                )
                                .child(
                                    Input::new(&password_state)
                                        .mask_toggle()
                                        .cleanable(true),
                                ),
                        )
                }
            })
            .footer(
                DialogFooter::new()
                    .child(
                        DialogClose::new().child(
                            Button::new("cancel").label("Cancel").outline(),
                        ),
                    )
                    .child(
                        DialogAction::new().child(
                            Button::new("connect").label("Connect").primary(),
                        ),
                    ),
            )
            .button_props(
                DialogButtonProps::default()
                    .on_cancel(|_, _, _| true)
                    .on_ok(move |_, window, cx| {
                        // Đọc username
                        let username = match &username_ok {
                            Some(st) => {
                                let u = st.read(cx).value().trim().to_string();
                                if u.is_empty() {
                                    window.push_notification("Username là bắt buộc.", cx);
                                    return false;
                                }
                                u
                            }
                            None => session_ok.username.clone().unwrap_or_default(),
                        };

                        // Đọc password
                        let password = password_ok.read(cx).value().to_string();
                        if password.is_empty() {
                            window.push_notification("Password là bắt buộc.", cx);
                            return false;
                        }

                        // (Tuỳ chọn) Lưu username vào store nếu user nhập mới
                        if username_ok.is_some() {
                            let mut updated = session_ok.clone();
                            updated.username = Some(username.clone());
                            SshSessionStore::global(cx).update(cx, |s, cx| {
                                s.update(index, updated, cx);
                            });
                        }

                        // Tạo SshConfig
                        let cfg = SshConfig {
                            host: session_ok.host.clone(),
                            port: session_ok.port,
                            username,
                            auth: SshAuthMethod::Password { password },
                        };

                        let label = session_ok.label.clone();
                        let dock_area = dock_area_ok.clone();

                        // Connect async
                        cx.spawn_in(window, async move |_this, window| {
                            let result = window.background_executor().spawn(async move {
                                myterm2_ssh::SshSession::connect(
                                    cfg,
                                    PtySize { rows: 24, cols: 80 },
                                    10_000,
                                )
                            }).await;

                            _ = window.update(|window, cx| {
                                match result {
                                    Ok(session) => {
                                        let panel: Arc<dyn PanelView> = Arc::new(
                                            TerminalPanel::from_session_entity(
                                                Box::new(session) as Box<dyn TerminalSession>,
                                                &label,
                                                window, cx,
                                            ),
                                        );
                                        add_ssh_terminal_to_dock(
                                            &dock_area, panel, window, cx,
                                        );
                                        window.push_notification(
                                            format!("Connected to \"{label}\"."),
                                            cx,
                                        );
                                    }
                                    Err(e) => {
                                        window.push_notification(
                                            format!("SSH connect failed: {e}"),
                                            cx,
                                        );
                                    }
                                }
                            });
                        }).detach();

                        true  // đóng dialog
                    }),
            )
    });
}
```

### 6.4. Lấy `WeakEntity<DockArea>` trong dialog

`SessionPanel` hiện không持有 `DockArea` reference. Có 2 cách:

**Cách A (khuyến nghị): Lưu `WeakEntity<DockArea>` vào `SessionPanel`**

```rust
pub struct SessionPanel {
    focus_handle: FocusHandle,
    store: Entity<SshSessionStore>,
    dock_area: WeakEntity<DockArea>,  // ← MỚI
}

impl SessionPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = SshSessionStore::global(cx);
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            focus_handle: cx.focus_handle(),
            store,
            dock_area: cx.dock_area(),  // helper — xem ghi chú
        }
    }
}
```

> **Ghi chú `cx.dock_area()`:** GPUI không có API trực tiếp lấy DockArea từ
> Context. Cần truyền `WeakEntity<DockArea>` vào `SessionPanel::new` từ
> `register_panel` hoặc `reset_default_layout`. Tham chiếu:
> `DockItem::tab(SessionPanel::new_entity(window, cx), ...)` — truyền thêm
> `dock_area` param.

**Cách B: Lưu `WeakEntity<DockArea>` vào `AppState` global**

```rust
pub struct AppState {
    pub dock_area: Option<WeakEntity<DockArea>>,  // set sau khi DockArea tạo
}
```

Set trong `MyTermWorkspace::new`:
```rust
AppState::global(cx).update(cx, |s, cx| {
    s.dock_area = Some(dock_area.downgrade());
    cx.notify();
});
```

Đọc trong `open_connect_dialog`:
```rust
fn get_dock_area(cx: &App) -> WeakEntity<DockArea> {
    AppState::global(cx).read(cx).dock_area.clone()
        .expect("dock_area not initialized")
}
```

> **Khuyến nghị Cách B** — đơn giản hơn, không cần thay đổi signature
> `SessionPanel::new` + `register_panel` + `reset_default_layout`.

---

## 7. Cấu trúc file

### 7.1. File mới / thay đổi

| File | Trạng thái | Trách nhiệm |
|---|---|---|
| `crates/ssh/src/config.rs` | **MỚI** | `SshConfig` + `SshAuthMethod` struct |
| `crates/ssh/src/lib.rs` | **Sửa** | Re-export `config::*` |
| `crates/ssh/src/session.rs` | **MỚI** (roadmap) | `SshSession::connect()` — xem `terminal-backend.md` §7 |
| `crates/ui/src/views/session_tabs/tabs.rs` | **Sửa** | Thêm `open_connect_dialog()` + left-click handler + cập nhật context menu "Open" |
| `crates/ui/src/views/terminal/panel.rs` | **Sửa** | Thêm `tab_title` field + `from_session()` / `from_session_entity()` constructor |
| `crates/ui/src/state/app_state.rs` | **Sửa** | Thêm `dock_area: Option<WeakEntity<DockArea>>` |
| `crates/ui/src/layout/workspace/mod.rs` | **Sửa** | Set `AppState.dock_area` sau khi tạo DockArea |

### 7.2. Dependency thay đổi

```toml
# crates/ui/Cargo.toml — thêm ssh dependency
[dependencies]
myterm2-ssh = { path = "../ssh" }
```

> ⚠️ **Quy tắc phụ thuộc**: `docs/agents/structure.md` ghi `ui` **không** import
> `ssh`/`local` trực tiếp — gọi qua `TerminalSession` trait. Tuy nhiên, để tạo
> `SshSession` cần gọi `SshSession::connect()` (factory). Hai giải pháp:
>
> **Giải pháp 1 (khuyến nghị MVP):** Cho phép `ui` phụ thuộc `ssh` để gọi
> `SshSession::connect()`. Session trả về `Box<dyn TerminalSession>` — UI chỉ
> dùng trait, không biết internals. Đây là pattern mà `panel.rs` **đã dùng** với
> `myterm2_local::LocalSession`. Cập nhật quy tắc: `ui → {core, local, ssh}`.
>
> **Giải pháp 2 (clean architecture):** Đẩy factory ra `app` crate. `app` tạo
> `Box<dyn TerminalSession>` rồi truyền vào `ui`. `ui` giữ nguyên leaf (chỉ
> `core`). Cần thêm callback/registry pattern: `ui` gọi `app` qua trait khi cần
> connect. Phức tạp hơn — để sau.

> **Quyết định MVP:** Giải pháp 1 — cập nhật `structure.md` quy tắc phụ thuộc
> thành `ui → {core, local, ssh}`. Đã có tiền lệ (`panel.rs` import
> `myterm2_local`).

---

## 8. Implementation checklist

### Bước 1 — `ssh` crate: `SshConfig` + `SshAuthMethod`

- [ ] Tạo `crates/ssh/src/config.rs` — define `SshConfig` + `SshAuthMethod`.
- [ ] Cập nhật `crates/ssh/src/lib.rs` — `pub mod config;` + re-export.
- [ ] Cập nhật `crates/ui/Cargo.toml` — thêm `myterm2-ssh` dependency.

### Bước 2 — `TerminalPanel`: hỗ trợ session từ ngoài

- [ ] Thêm field `tab_title: String` vào `TerminalPanel`.
- [ ] `new()` — set `tab_title = "Terminal"`.
- [ ] Thêm `from_session(session, title, window, cx)` + `from_session_entity(...)`.
- [ ] `Panel::title()` — dùng `self.tab_title` thay vì hardcode `"Terminal"`.
- [ ] `register_panel("terminal", ...)` — giữ `new_entity` (local mặc định).

### Bước 3 — `AppState`: lưu `WeakEntity<DockArea>`

- [ ] Thêm field `dock_area: Option<WeakEntity<DockArea>>` vào `AppState`.
- [ ] Trong `MyTermWorkspace::new` — set `AppState.dock_area` sau khi tạo DockArea.

### Bước 4 — `open_connect_dialog` trong `session_tabs/tabs.rs`

- [ ] Thêm `open_connect_dialog(session, index, window, cx)` function.
- [ ] Logic phân nhánh: `ask_username = session.username.is_none()`.
- [ ] Dialog title + server info banner theo `ask_username`.
- [ ] Username input (chỉ khi `ask_username = true`).
- [ ] Password input: `InputState::masked(true)` + `Input::mask_toggle()`.
- [ ] Footer: `DialogFooter` → Cancel (`DialogClose`) + Connect (`DialogAction`), `justify_end`.
- [ ] `on_ok`: validate → tạo `SshConfig` → (tuỳ chọn) lưu username → connect async.
- [ ] Connect thành công → `TerminalPanel::from_session_entity` → add to dock.
- [ ] Connect lỗi → `window.push_notification`.

### Bước 5 — Tích hợp click handler vào `render_session_row`

- [ ] Thêm `.on_click` left-click → `open_connect_dialog`.
- [ ] Cập nhật context menu "Open" → gọi `open_connect_dialog` (thay vì
      `push_notification("chưa triển khai")`).

### Bước 6 — `ssh` crate: `SshSession::connect()` (roadmap terminal-backend §7)

- [ ] `crates/ssh/src/session.rs` — russh client + tokio runtime ẩn.
- [ ] `crates/ssh/src/listener.rs` — `SshListener: EventListener`.
- [ ] `crates/ssh/src/auth.rs` — password auth (MVP).
- [ ] `impl TerminalSession for SshSession`.
- [ ] Re-export `SshSession`, `PtySize` qua `lib.rs`.

### Bước 7 — Cập nhật docs

- [ ] Cập nhật `docs/agents/structure.md` — quy tắc phụ thuộc `ui → {core, local, ssh}`.
- [ ] Cập nhật `AGENTS.md` — roadmap SSH check.

### Bước 8 — Quality gate

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo build --workspace`
- [ ] `cargo run -p app` — click vào SSH session item → dialog hiện → nhập
      credentials → terminal tab mở (cần server SSH thật để test end-to-end).

---

## 9. Edge cases & ghi chú

### 9.1. Reconnect / duplicate session

Khi user click vào cùng 1 session item nhiều lần → mở nhiều tab SSH riêng biệt
(mỗi tab = 1 connection độc lập). Đây là behavior mong muốn — giống Tabby,
Termius. Không cache/reuse connection.

### 9.2. Connection timeout

`SshSession::connect` nên có timeout (vd 30s). Nếu server unreachable →
`Err` → `push_notification("SSH connect failed: connection timed out")`.
Xem `terminal-backend.md` §13 (rủi ro).

### 9.3. Host key verification

MVP: chấp nhận mọi host key (KHÔNG khuyến nghị production). Roadmap: thêm
known_hosts + prompt accept/reject (xem `terminal-backend.md` §8, bước 8).
Khi triển khai known_hosts, dialog connect cần thêm bước:
- Host key chưa know → dialog "Accept host key? (fingerprint: xx:xx:...)"
- Host key mismatch → dialog "WARNING: host key changed!"

### 9.4. Password input — không log

`InputState::masked(true)` đảm bảo text hiển thị `•••••`. Tuy nhiên, cần đảm
 bảo password không bị log ra console/tracing. KHÔNG `tracing::info!` raw
 password. Trong `SshConfig` debug impl, mask password:

```rust
impl Debug for SshConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("SshConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth", &"***")  // ← mask
            .finish()
    }
}
```

### 9.5. Keyboard focus trong dialog

- Khi dialog mở, focus vào field đầu tiên (username nếu hỏi, password nếu không).
- Enter trong password field → trigger Connect (gọi `on_ok`).
- Esc → trigger Cancel (gọi `on_cancel`).

> gpui-component Dialog tự handle Esc (dispatch `CancelDialog`). Enter trong
> `DialogAction` button → dispatch `ConfirmDialog`. Cần bind Enter key trong
> input field → dispatch `ConfirmDialog` (xem Dialog API).

### 9.6. Auth method khác (roadmap)

Hiện MVP chỉ support password. Roadmap thêm:
- **Private key**: dialog thêm field "Key file path" + "Passphrase (optional)".
  `SshAuthMethod::PrivateKey { key_path, passphrase }`.
- **SSH agent**: tự detect agent, không cần dialog. `SshAuthMethod::Agent`.
- Khi có nhiều auth method, dialog thêm dropdown "Authentication method" →
  show/hide fields tương ứng.