# SFTP theo Terminal CWD — Phần 1: Tổng quan & mục tiêu

> Tài liệu thiết kế cho tính năng: **nút "Sync SFTP Browser theo thư mục hiện tại
> của SSH session"**. Khi user `cd` sang thư mục khác trong terminal, nhấn nút này
> để SFTP Browser tự nhảy tới đúng thư mục đó.
>
> **Tham chiếu liên quan:**
> - [`docs/sftp-browser-design.md`](../sftp-browser-design.md) — thiết kế SFTP browser + `SftpPanel`
> - [`docs/osc-sequences-checklist.md`](../osc-sequences-checklist.md) §F — OSC 7 (CWD)
> - [`docs/terminal-backend.md`](../terminal-backend.md) — `SshSession`, `TerminalSession`
> - [`docs/agents/structure.md`](../agents/structure.md) — cấu trúc crate & dependency graph
>
> **Các phần của tài liệu này** (chia nhỏ để dễ đọc, gộp lại sau):
> 1. `01-overview.md` — tổng quan & mục tiêu (file này)
> 2. `02-current-state.md` — hiện trạng codebase liên quan
> 3. `03-high-level-design.md` — thiết kế cấp cao (kiến trúc, luồng dữ liệu)
> 4. `04-low-level-design.md` — thiết kế chi tiết (struct, hàm, code)
> 5. `05-edge-cases-roadmap.md` — edge case, rủi ro, roadmap triển khai

---

## 1.1. Mô tả chức năng

SFTP Browser (panel bên phải) và Terminal (SSH shell, panel giữa) hiện là **hai
luồng độc lập** chạy trên cùng 1 kết nối SSH. Thư mục mà SFTP đang duyệt (`cwd`
của `SftpPanel`) **không liên quan** tới thư mục mà shell đang đứng (`pwd` phía
remote). User `cd /var/log` trong terminal thì SFTP vẫn ở `~`.

Tính năng này thêm **một nút trên toolbar của SFTP Browser**. Khi user click:

1. Đọc thư mục hiện tại (`cwd`) của SSH session gắn với tab terminal đang active.
2. Điều hướng SFTP Browser tới đúng thư mục đó (`load_dir`).

Đây là hành vi **manual sync** (đồng bộ theo yêu cầu): mỗi lần user muốn SFTP
"đuổi theo" vị trí shell thì bấm nút. Không auto-follow theo mặc định (xem
§1.4 để biết lý do và phương án mở rộng auto-follow tùy chọn).

### Ví dụ luồng sử dụng

```
Terminal:  user@host:~$ cd /var/www/html
SFTP:      vẫn đang ở /home/user
           │
           └─ user click nút [⤢ Sync to terminal] trên SFTP toolbar
                     │
                     └─ SFTP Browser nhảy tới /var/www/html
```

---

## 1.2. Yêu cầu chức năng

| # | Yêu cầu | Ghi chú |
|---|---------|---------|
| R1 | SFTP toolbar có 1 nút "sync theo cwd của terminal" | Icon + tooltip rõ nghĩa |
| R2 | Click nút → SFTP điều hướng tới `cwd` của SSH session active | Dùng `load_dir` sẵn có |
| R3 | `cwd` lấy live tại thời điểm click (không phải snapshot cũ) | Phản ánh đúng lần `cd` gần nhất |
| R4 | Nếu `cwd` không có (chưa nhận OSC 7) → nút disabled + tooltip giải thích | Không crash, không nhảy sai |
| R5 | Local shell tab hoặc SSH không có SFTP → không hiện nút (hoặc disabled) | Nhất quán với `render_no_connection` |
| R6 | Không phá kiến trúc crate: `ui` không import `ssh`/`local` | Giao tiếp qua trait `TerminalSession` |
| R7 | (Mở rộng, tùy chọn) Toggle "auto-follow" — tự sync mỗi lần cwd đổi | Không bắt buộc cho bản đầu |

---

## 1.3. Điều kiện tiên quyết: OSC 7 phải hoạt động

`cwd` của terminal được xác định qua **OSC 7** (`ESC]7;file://host/path ST`). Theo
[`osc-sequences-checklist.md`](../osc-sequences-checklist.md) §F, OneTerm **đã hỗ trợ**
parse OSC 7 (tự parse song song vì `alacritty_terminal` drop nó), lưu vào
`SessionState.cwd` và expose qua `TerminalSession::cwd() -> Option<PathBuf>`.

**Điểm mấu chốt cho SSH:** OSC 7 do **shell phía remote** phát ra. Nó chỉ có mặt khi
remote shell được cấu hình phát OSC 7 (qua `PROMPT_COMMAND` của bash, `precmd`/`PS1`
của zsh, hoặc VTE integration mà nhiều distro cài sẵn ở `/etc/profile.d/`). Nếu
remote shell **không** phát OSC 7 thì `cwd()` trả về `None` và tính năng không thể
"đuổi theo".

→ Đây là **giả định nền tảng** của thiết kế. Xử lý khi thiếu OSC 7 nằm ở R4
(nút disabled + tooltip) và được bàn kỹ trong `05-edge-cases-roadmap.md`. Việc
**chủ động inject shell integration khi SSH login** để đảm bảo OSC 7 luôn có là một
hướng mở rộng, cũng bàn trong phần 05.

---

## 1.4. Manual sync vs Auto-follow

| Phương án | Ưu | Nhược |
|-----------|-----|-------|
| **Manual (nút bấm)** — bản đầu | Đơn giản, không tốn read_dir thừa, user chủ động | Phải bấm mỗi lần |
| **Auto-follow (toggle)** — mở rộng | SFTP luôn khớp shell tự động | Mỗi `cd` → 1 `read_dir` (tốn băng thông), có thể gây "nhảy" ngoài ý muốn khi đang thao tác file |

Yêu cầu gốc của user là "**nhấn button thì SFTP tự chuyển theo**" → đúng bản chất
**manual sync**. Auto-follow để dành làm tùy chọn bật/tắt sau (R7), vì nó thay đổi
UX đáng kể và tốn tài nguyên khi user gõ nhiều lệnh `cd` liên tiếp.

---

## 1.5. Nguyên tắc thiết kế

1. **Tái sử dụng hạ tầng sẵn có** — `cwd()` (trait), `load_dir()` (SftpPanel),
   pattern `AppState.active_sftp` đã tồn tại. Không phát minh lại.
2. **Live read** — đọc `cwd` tại thời điểm click, không cache snapshot lỗi thời.
3. **Tôn trọng layering** — `ui` chỉ chạm `dyn TerminalSession` (ở `core`), không
   import `ssh`/`local`.
4. **Fail an toàn** — thiếu OSC 7 / không có SFTP → disabled, không nhảy sai chỗ.
5. **Không đụng SFTP backend** — tính năng thuần UI + 1 kênh dữ liệu `cwd`; không
   sửa `sftp_task`, `SftpCmd`, giao thức.

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

# SFTP theo Terminal CWD — Phần 3: Thiết kế cấp cao (High-Level Design)

> Phần này mô tả kiến trúc tổng thể: các thành phần, luồng dữ liệu, và cách nối
> `cwd` của terminal tới `SftpPanel` mà vẫn tôn trọng layering. Chi tiết code ở
> phần 04.

---

## 3.1. Ý tưởng cốt lõi

`SftpPanel` cần trả lời được câu hỏi **"thư mục hiện tại của SSH session đang active
là gì?"** ngay tại thời điểm user click nút. Vì user có thể `cd` sau khi mở tab,
ta **không** snapshot `cwd` một lần, mà lưu một **"cwd provider"** — một thứ có thể
gọi để lấy `cwd` mới nhất bất kỳ lúc nào.

Có sẵn 2 mảnh:
- `TerminalSession::cwd() -> Option<PathBuf>` đọc live từ `SessionState`.
- `AppState` là điểm giao tiếp giữa `TerminalPanel` (per-tab) và `SftpPanel` (global).

→ **Mở rộng pattern `active_sftp`**: đặt thêm vào `AppState` một handle cho phép đọc
`cwd` của session đang active. `TerminalPanel::set_active` set handle này cùng lúc
với `active_sftp`.

---

## 3.2. Lựa chọn cơ chế "cwd provider"

Ba phương án, cân nhắc theo layering + tính "live":

| PA | Mô tả | Live? | Layering | Đánh giá |
|----|-------|:-----:|----------|----------|
| **A. Snapshot `PathBuf`** | Lưu `active_cwd: Option<PathBuf>` vào AppState lúc `set_active` | ❌ | OK | Sai — không đuổi theo `cd` sau đó |
| **B. Weak handle tới session entity** | Lưu `WeakEntity<...>` rồi `.read(cx).cwd()` khi click | ✅ | ⚠️ cần kiểu session | Tốt nhưng cần lộ kiểu `Entity<Box<dyn TerminalSession>>` |
| **C. Closure provider** | Lưu `Arc<dyn Fn() -> Option<PathBuf>>` bọc quanh session state | ✅ | ✅ sạch | **Chọn** — không phụ thuộc gpui entity, thuần `core` type |

**Chọn phương án C** với tinh chỉnh: thay vì closure khó lưu/khó so sánh, ta expose
**cùng nguồn dữ liệu mà `cwd()` đọc** — tức là một handle tới phần state chia sẻ.
Nhưng state đó nằm trong crate `ssh`/`local` (không được để UI import). Vì vậy ta
gói nó sau một **trait nhỏ trong `core`**:

```rust
// core: một "nguồn cwd" có thể đọc live, không lộ chi tiết session
pub trait CwdSource: Send + Sync {
    fn cwd(&self) -> Option<PathBuf>;
}
```

`SshSession`/`LocalSession` có thể cung cấp một `Arc<dyn CwdSource>` chia sẻ cùng
`SharedState` (chỉ đọc field `cwd`). UI giữ `Arc<dyn CwdSource>` trong `AppState`.

> **Lưu ý cân nhắc:** nếu thấy thêm trait `CwdSource` là thừa, có thể tái dùng luôn
> `Arc<dyn SftpBackend>` bằng cách... không — `SftpBackend` không biết `cwd`. Giữ
> `CwdSource` tách bạch đúng trách nhiệm. Chi phí: 1 trait + 1 accessor. Xem phần 04
> để so sánh với phương án "đọc qua entity" (B) nếu muốn tránh trait mới.

---

## 3.3. Sơ đồ thành phần

```
                         crates/core
        ┌───────────────────────────────────────────────┐
        │ trait TerminalSession { fn cwd() -> Option<..> │
        │                         fn cwd_source() -> ..  │  ← MỚI (default None)
        │ trait CwdSource { fn cwd() -> Option<PathBuf> }│  ← MỚI
        └───────────────────────────────────────────────┘
              ▲                                   ▲
              │ impl                              │ impl (chia sẻ SharedState.cwd)
     ┌────────┴─────────┐              ┌──────────┴───────────┐
     │ crates/ssh       │              │ crates/local         │
     │  SshSession      │              │  LocalSession        │
     │  SharedState.cwd │              │  SharedState.cwd     │
     └──────────────────┘              └──────────────────────┘

                         crates/ui
   ┌──────────────────────────────────────────────────────────────┐
   │ TerminalPanel::set_active(active)                             │
   │   if active {                                                 │
   │     state.active_sftp   = session.sftp();          (đã có)    │
   │     state.active_cwd_source = session.cwd_source();  ← MỚI    │
   │   }                                                           │
   │                                                               │
   │ AppState { active_sftp, active_cwd_source }   ← thêm field    │
   │                                                               │
   │ SftpPanel (observe AppState)                                  │
   │   - lưu cwd_source: Option<Arc<dyn CwdSource>>                │
   │   - toolbar: nút [Sync to terminal cwd]                       │
   │       on_click → sync_to_terminal_cwd():                      │
   │           match cwd_source.cwd() {                            │
   │             Some(p) => self.goto_path(p)  // stat + load_dir  │
   │             None    => (nút đã disabled)                      │
   │           }                                                   │
   └──────────────────────────────────────────────────────────────┘
```

---

## 3.4. Luồng dữ liệu — Manual sync (bản đầu)

```
User click nút [Sync]  ─────────────────────────────────────────┐
                                                                 ▼
SftpPanel::sync_to_terminal_cwd(cx)                              │
  1. let src = self.cwd_source.clone()?          // None → return│
  2. let cwd = src.cwd()?                         // None → return
  3. self.goto_path(cwd, cx)                                     │
        │                                                        │
        ├─ sftp.stat(cwd)  (background)                          │
        │     ├─ Ok(dir)  → load_dir(cwd) → read_dir → render    │
        │     ├─ Ok(file) → path_error (hiếm, cwd luôn là dir)   │
        │     └─ Err      → path_error / thông báo               │
        ▼                                                        │
   SFTP Browser hiển thị nội dung thư mục = pwd của shell ───────┘
```

Ghi chú:
- `goto_path` **đã** làm bước `stat` + xử lý lỗi → tái dùng, không viết mới.
- Toàn bộ I/O (`stat`, `read_dir`) chạy nền như `load_dir` hiện tại → không block UI.

---

## 3.5. Trạng thái của nút (enabled / disabled / hidden)

Nút quyết định trạng thái dựa trên 2 điều kiện, đọc tại `render_toolbar`:

| Điều kiện | Kết quả nút |
|-----------|-------------|
| `self.sftp.is_none()` (local shell / không SFTP) | Toolbar không render (đã có `render_no_connection`) → nút không xuất hiện |
| Có SFTP nhưng `cwd_source` là `None` hoặc `cwd_source.cwd() == None` | Nút **disabled** + tooltip: "Terminal chưa báo thư mục hiện tại (cần shell integration / OSC 7)" |
| Có SFTP và `cwd_source.cwd() == Some(p)` | Nút **enabled**; tooltip: "Chuyển tới thư mục hiện tại của terminal: {p}" |

> Đọc `cwd_source.cwd()` trong `render` là thao tác nhẹ (lock + clone `PathBuf`),
> chấp nhận được. Nếu muốn tránh gọi mỗi frame, có thể cache và cập nhật qua
> observe (xem auto-follow §3.6).

---

## 3.6. (Mở rộng tùy chọn) Auto-follow

Nếu triển khai R7, tận dụng **`SessionEvent::Cwd`** đã được `ssh` `forward`:

```
remote shell `cd` → OSC 7 → ssh task → SessionEvent::Cwd(path)
    → (đã có) cập nhật SharedState.cwd
    → (MỚI) UI forward tới SftpPanel khi auto-follow bật
        → SftpPanel.load_dir(path)  (chỉ khi khác cwd hiện tại + panel active)
```

Thiết kế auto-follow:
- Thêm cờ `auto_follow: bool` trong `SftpPanel` (toggle trên toolbar, persist vào
  `docks.json` như các setting SFTP khác).
- Kênh sự kiện: hoặc (a) `LocalTerminalView` vốn đã subscribe `SessionEvent` để
  re-render — bổ sung: khi nhận `Cwd`, nếu tab active + auto-follow, gọi vào
  `SftpPanel`; hoặc (b) đẩy `cwd` mới vào `AppState` (thêm `active_cwd: Option<PathBuf>`
  cập nhật realtime) và `SftpPanel` observe.
- Chống nhiễu: debounce + chỉ load khi `path != self.cwd` để tránh `read_dir` dồn dập
  khi user gõ nhiều `cd`.

Auto-follow **không thuộc phạm vi bản đầu**; ghi ở đây để thiết kế manual không chặn
đường mở rộng (ví dụ `CwdSource` + `SessionEvent::Cwd` đều dùng lại được).

---

## 3.7. Vì sao không đặt nút ở phía Terminal?

Có thể đặt nút "mở thư mục này trong SFTP" ở toolbar/breadcrumb của terminal. Nhưng:
- SFTP Browser là nơi user đang nhìn danh sách file → đặt nút ở đó trực quan hơn
  ("kéo SFTP theo tôi").
- Toolbar SFTP đã có sẵn cụm nút điều hướng (Back/Refresh) → nút Sync cùng nhóm ngữ
  nghĩa "điều hướng".
- Tránh phụ thuộc ngược: terminal view không cần biết về SFTP panel.

→ Đặt nút ở **SFTP toolbar**. Phù hợp yêu cầu gốc của user.

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

# SFTP theo Terminal CWD — Phần 5: Edge cases, rủi ro & roadmap

---

## 5.1. Edge cases

| # | Tình huống | Hành vi mong muốn |
|---|-----------|-------------------|
| E1 | Remote shell **không** phát OSC 7 → `cwd() == None` | Nút disabled + tooltip giải thích. Không nhảy. |
| E2 | `cwd` trỏ tới thư mục **không có quyền đọc** (permission denied) | `goto_path`→`stat`/`read_dir` lỗi → hiện `path_error`/thông báo lỗi (đã có sẵn). Giữ nguyên cwd cũ. |
| E3 | `cwd` là thư mục vừa bị xoá | Tương tự E2 — lỗi stat → báo lỗi, không đổi cwd. |
| E4 | Tab active là **local shell** (không SFTP) | Toolbar không render (`render_no_connection`) → nút không hiện. |
| E5 | SSH có shell nhưng **không mở được SFTP channel** | `self.sftp == None` → nút không hiện (hoặc disabled). |
| E6 | User click Sync khi đang có **transfer đang chạy** | `load_dir` chỉ đổi listing; transfer chạy nền độc lập (channel riêng) → không ảnh hưởng. |
| E7 | `cwd` đã trùng thư mục SFTP đang đứng | Vẫn `load_dir` (refresh lại) — chấp nhận được; hoặc bỏ qua nếu bằng nhau để tiết kiệm (tùy chọn nhỏ). |
| E8 | OSC 7 trả path có **ký tự đặc biệt / non-UTF8** | `parse_cwd_url` đã xử lý ở tầng ssh; `PathBuf` giữ nguyên; SFTP `stat` tự báo lỗi nếu server không chấp nhận. |
| E9 | Path từ OSC 7 kèm **hostname khác** (mount lạ, container) | Chỉ dùng phần path của `file://host/path`. `parse_cwd_url` đã bỏ host. Nếu host khác remote thật, thư mục có thể không tồn tại → E2. |
| E10 | Chuyển tab nhanh liên tục | `active_cwd_source` cập nhật theo `set_active` mới nhất; observe đọc giá trị hiện tại → luôn khớp tab đang xem. |

---

## 5.2. Rủi ro & giảm thiểu

| Rủi ro | Mức | Giảm thiểu |
|--------|:---:|-----------|
| **OSC 7 không có trên nhiều server** khiến tính năng "vô dụng" với user đó | Trung bình | Tooltip giải thích rõ; tài liệu hướng dẫn bật shell integration; cân nhắc inject shell integration khi SSH login (§5.4) |
| Gọi `cwd_source.cwd()` mỗi frame trong `render` (lock Mutex) | Thấp | Lock cực ngắn (clone `Option<PathBuf>`); nếu cần, cache + cập nhật qua observe |
| Thêm trait `CwdSource` làm tăng bề mặt API `core` | Thấp | Trait 1 method, tài liệu hoá; hoặc dùng phương án B (weak entity) nếu muốn |
| Auto-follow (nếu làm) gây `read_dir` dồn dập khi gõ nhiều `cd` | Trung bình | Debounce + chỉ load khi `path != cwd` + chỉ khi panel active |
| Borrow checker khi lấy `sftp()` + `cwd_source()` cùng lúc trong `set_active` | Thấp | Tách 2 câu `let` mỗi câu tự `read(cx)` |

---

## 5.3. Kiểm thử

**Unit / logic:**
- `SshCwdSource::cwd()` phản ánh giá trị `SharedState.cwd` sau khi set (mô phỏng
  OSC 7 → cập nhật state → đọc lại).
- `sync_to_terminal_cwd`: khi `cwd_source == None` → no-op; khi `Some(path)` → gọi
  `goto_path(path)`.

**Thủ công (manual):**
1. SSH tới server có shell integration (bash + `PROMPT_COMMAND` phát OSC 7).
2. `cd /var/log` trong terminal → click Sync → SFTP hiển thị `/var/log`.
3. `cd /etc` → click Sync → SFTP nhảy `/etc`.
4. SSH tới server **không** phát OSC 7 → nút disabled, tooltip đúng.
5. Local shell tab → không thấy panel/nút.
6. `cd` tới thư mục không quyền đọc → click Sync → báo lỗi, cwd SFTP giữ nguyên.

**Quality gate (bắt buộc, theo AGENTS.md §5):**
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
```

---

## 5.4. (Đã triển khai) OSC 7 trên SSH — im lặng, dùng `exec`

Với local shell, OneTerm sinh OSC 7/133 qua env lúc spawn (im lặng). Với SSH, các
cách khác đều có nhược điểm:

| Cách | Im lặng? | Cần sshd config? | Kết quả |
|------|:--------:|:----------------:|---------|
| Ghi snippet vào stdin (`channel.data`) | ❌ (bị PTY echo) | Không | Đã thử — hiện ra terminal |
| Strip echo phía client (`EchoSuppressor`) | ❌ (echo bị reformat) | Không | Đã thử — vẫn hiện |
| `channel.set_env("PROMPT_COMMAND")` | ✅ | **Có** (`AcceptEnv`) | Đã thử — server không nhận → mất OSC 7 |
| **`channel.exec(...)` rồi `exec` shell** | ✅ | **Không** | **Đang dùng** |

**Cách đang dùng** — thay `request_shell` bằng `channel.exec(true, cmd)`:

```
__oneterm_osc7term_osc7() { printf '\x1b]7;file://%s%s\x1b\\' "${HOSTNAME:-$(hostname)}" "$PWD"; printf '\x1b]133;A\x1b\\'; };
export -f __oneterm_osc7term_osc7 2>/dev/null;
export PROMPT_COMMAND='__oneterm_osc7term_osc7';
[ -f /run/motd.dynamic ] && cat /run/motd.dynamic 2>/dev/null;
[ -r /etc/motd ] && cat /etc/motd 2>/dev/null;
exec "${SHELL:-/bin/bash}" -il
```

- sshd chạy lệnh này qua `$SHELL -c <cmd>` (**non-interactive** → không readline →
  **không echo**).
- Bước 1–2 định nghĩa hook + export (function qua `export -f`, và `PROMPT_COMMAND`).
- Bước 3 **in lại MOTD**: `exec` bỏ qua bước sshd/PAM in banner đăng nhập (chỉ chạy
  cho `request_shell`), nên tự `cat /run/motd.dynamic` (Ubuntu cache MOTD động) +
  `/etc/motd` (guard nếu thiếu file → không in gì).
- Bước 4 `exec` shell đăng nhập tương tác → **kế thừa** hook + `PROMPT_COMMAND` →
  phát OSC 7 + OSC 133;A trước mỗi prompt.
- **Không phụ thuộc** `AcceptEnv` (khác `set_env`).

**Giới hạn còn lại:**
- Hướng bash (`export -f` + `PROMPT_COMMAND`). zsh/shell khác: không OSC 7 nhưng vô
  hại.
- `.bashrc` ghi đè `PROMPT_COMMAND` sẽ vô hiệu hoá hook (đa số distro mặc định không
  đụng).
- MOTD: khôi phục qua `/run/motd.dynamic` + `/etc/motd`. Dòng "Last login:" (do sshd
  in riêng) không có. Nếu server nào PAM vẫn tự in MOTD cho exec → có thể trùng.
- Tắt hẳn: `SshConfig::shell_integration = false` → dùng `request_shell` như cũ.

---

## 5.5. Roadmap triển khai

Thứ tự đề xuất (mỗi bước build + clippy sạch trước khi sang bước sau):

- [ ] **B1 — core**: thêm trait `CwdSource` + `fn cwd_source()` default `None` +
  re-export. `cargo build -p oneterm-core`.
- [ ] **B2 — ssh**: `SshCwdSource` + override `cwd_source()`. Build ssh.
- [ ] **B3 — (tùy chọn) local**: override `cwd_source()` cho nhất quán.
- [ ] **B4 — ui state**: thêm `AppState.active_cwd_source` + cập nhật khởi tạo.
- [ ] **B5 — ui terminal**: `set_active` set `active_cwd_source`.
- [ ] **B6 — ui sftp panel**: field `cwd_source`, observe, `sync_to_terminal_cwd`,
  `terminal_cwd`.
- [ ] **B7 — ui sftp render**: nút Sync trên toolbar (disabled/tooltip theo trạng thái).
- [ ] **B8 — icon**: thêm/chọn icon (`FolderSync` hoặc tương đương).
- [ ] **B9 — quality gate**: fmt + clippy + build workspace; test thủ công theo §5.3.
- [ ] **B10 — (mở rộng)** auto-follow toggle (R7): cờ `auto_follow`, nối
  `SessionEvent::Cwd`, debounce, persist.

---

## 5.6. Tiêu chí hoàn thành (Definition of Done) — bản đầu

- Nút Sync xuất hiện trên SFTP toolbar khi tab active là SSH có SFTP.
- Click nút → SFTP điều hướng tới `cwd` hiện tại của terminal (đọc live).
- Thiếu OSC 7 → nút disabled + tooltip; không crash, không nhảy sai.
- Không vi phạm layering (`ui` không import `ssh`/`local`).
- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build --workspace` đều pass.

