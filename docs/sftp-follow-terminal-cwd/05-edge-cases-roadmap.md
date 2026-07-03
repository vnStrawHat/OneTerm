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
__oneterm_osc7() { printf '\x1b]7;file://%s%s\x1b\\' "${HOSTNAME:-$(hostname)}" "$PWD"; printf '\x1b]133;A\x1b\\'; };
export -f __oneterm_osc7 2>/dev/null;
export PROMPT_COMMAND='__oneterm_osc7';
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
