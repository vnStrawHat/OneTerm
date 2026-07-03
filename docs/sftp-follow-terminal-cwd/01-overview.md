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
