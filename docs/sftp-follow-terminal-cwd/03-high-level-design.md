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
