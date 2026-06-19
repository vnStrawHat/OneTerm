# Dependencies & gpui-component — myTerm2

> File tách từ `AGENTS.md` (sections 2, 7, 11). Chứa thông tin về rev lock, dependency cho phép, cách tích hợp upstream và quy tắc **reference-first research**.

---

## 1. Rev đã lock (cố định — chỉ đổi khi chủ động upgrade)

Workspace đang lock đúng bộ rev đã được `reference/gpui-component` (upstream) verify tương thích:

| Crate | Source | Rev | Resolved version |
|---|---|---|---|
| `gpui` | `https://github.com/zed-industries/zed` | `1d217ee39d381ac101b7cf49d3d22451ac1093fe` | `0.2.2` |
| `gpui_platform` | `https://github.com/zed-industries/zed` | `1d217ee39d381ac101b7cf49d3d22451ac1093fe` | (cùng rev — monorepo) |
| `gpui-component` | `https://github.com/longbridge/gpui-component` | `ea6b194db04cc7c0474851f07c7d5b7a9df6a98b` | `0.5.2` (chưa tag, đang ở HEAD giữa `v0.5.1` → `v0.5.2`) |

> 📌 **Quy tắc bất di bất dịch**:
>
> 1. `gpui` và `gpui_platform` **phải cùng rev** (cùng monorepo `zed-industries/zed`).
> 2. Không thêm `gpui` từ crates.io hoặc git khác. Nếu cần tính năng ngoài 3 crate trên → patch upstream hoặc fork cục bộ, đừng tự ý swap dependency.
> 3. Khi upstream tag `gpui-component` `v0.5.2`, cân nhắc đổi rev → tag để ổn định dài hạn.

## 2. Khai báo trong `Cargo.toml` workspace

```toml
[workspace.dependencies]
# GPUI core — cùng rev, cùng monorepo
gpui = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe" }
gpui_platform = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe", features = ["font-kit"] }
gpui-component = { git = "https://github.com/longbridge/gpui-component", rev = "ea6b194db04cc7c0474851f07c7d5b7a9df6a98b" }
```

Trong mỗi crate con (vd. `crates/ui/Cargo.toml`):

```toml
[dependencies]
gpui.workspace = true
gpui_platform.workspace = true
gpui-component.workspace = true
```

> ⚠️ **Tên crate chính xác**: trong Cargo, tên crate là `gpui_platform` (gạch dưới) — không phải `gpui-platform`. Khai báo `use gpui_platform::...` cũng theo gạch dưới. Xem `reference/gpui-component/examples/hello_world/Cargo.toml` để tham chiếu (`gpui_platform = { workspace = true }`).

## 3. Crate phụ trợ được phép dùng

| Mục đích | Crate khuyến nghị |
|---|---|
| SSH protocol | `russh` + `russh-sftp` |
| PTY (local shell) | `portable-pty` |
| Terminal parser / grid | `alacritty_terminal` |
| Async runtime (re-export) | `smol` / `futures` (đã có sẵn trong gpui) |
| Serialization | `serde`, `serde_json`, `toml` |
| Storage (host list, settings) | `directories` (XDG / AppData) |
| Logging | `tracing` + `tracing-subscriber` |
| Error | `anyhow` (binary), `thiserror` (library) |
| Crypto bổ sung | `russh-cryptovec`, `ssh-key` |
| i18n | `rust-i18n` (khớp với gpui-component) |

Trước khi thêm crate mới, hỏi: "crate này đã có trong `reference/gpui-component/Cargo.toml` chưa?" Nếu có → dùng luôn rev đã lock. Nếu chưa và là crate mới → mở issue trước khi thêm.

---

## 4. Tích hợp với gpui-component upstream

Project này dùng gpui-component trực tiếp từ git. Khi upstream đổi API:

1. Đọc release note / PR diff trong `reference/gpui-component/`.
2. Cập nhật code tương ứng trong `crates/ui/`.
3. Nếu thay đổi breaking → cập nhật `CHANGELOG.md` (nếu có) + `docs/architecture.md`.

### Tham chiếu nhanh các entry point quan trọng của gpui-component

- `crates/ui/src/dock/` — `DockArea`, `Panel`, `StackPanel`, `TabPanel`.
- `crates/ui/src/input/` — `InputState`, `Input`.
- `crates/ui/src/dialog/` — `Dialog` overlay.
- `crates/ui/src/notification/` — toast/notification.
- `crates/ui/src/sheet/` — side panel.
- `crates/ui/src/theme.rs` — `Theme`, `ActiveTheme`, `ThemeColor`.

---

## 5. Reference-first research (QUAN TRỌNG)

> 🚨 **RÀNG BUỘC CỨNG**: Khi cần thông tin liên quan đến `gpui` / `gpui-component` (API, pattern, code example, doc, theme, icon, skill, changelog), **agent PHẢI đọc từ `D:\TrungKFC-Research\Rust\myTerm2\reference\gpui-component\` trước tiên**. **Không** dùng `web_search` / `fetch_content` / `code_search` để tra cứu thông tin gpui-component trừ khi đã đọc reference mà vẫn thiếu.

### 5.1. Tại sao

1. **Khớp version**: `reference/gpui-component` được pin đúng tại rev `ea6b194d...` (khớp `Cargo.lock` của project). Web search có thể trả về docs/code của version cũ / mới hơn → lỗi compile khó debug.
2. **Đầy đủ tài nguyên**: reference chứa `CLAUDE.md` (agent guide), `crates/ui/src/` (source đầy đủ), `examples/` (11 ví dụ chạy được), `skills/` (knowledge base), `docs/` (en + zh-CN), `.theme-schema.json`, icons, …
3. **Nhanh hơn**: đọc file local không cần network, không phải parse HTML, có thể `grep` chính xác.
4. **Tránh hallucination**: web search trả về snippet có thể sai tên method / signature; đọc source thật thì chính xác tuyệt đối.

### 5.2. Công cụ tra cứu trong reference

```bash
# Tìm file / module liên quan
find reference/gpui-component -name "*.rs" | xargs grep -l "DockArea"
find reference/gpui-component -name "*.rs" -path "*dock/*"

# Tìm struct / trait / method
grep -rn "pub trait Panel" reference/gpui-component/crates/ui/src/
grep -rn "fn on_click" reference/gpui-component/crates/ui/src/button/

# Đọc file nguồn
read reference/gpui-component/crates/ui/src/dock/dock.rs
read reference/gpui-component/CLAUDE.md

# Xem story example cho một component cụ thể
ls reference/gpui-component/crates/story/src/
grep -rn "Button::new" reference/gpui-component/examples/
```

**Tip**: dùng `read` + `grep` + `find` với **đường dẫn tương đối từ `D:\TrungKFC-Research\Rust\myTerm2`** (vd. `reference/gpui-component/...`).

### 5.3. Bảng tra cứu nhanh trong reference

| Cần biết gì | File cụ thể trong reference |
|---|---|
| API overview, init pattern | `reference/gpui-component/CLAUDE.md` |
| Component list & API | `reference/gpui-component/crates/ui/src/` (chia theo file: `button.rs`, `input/`, `dialog/`, `dock/`, …) |
| Icon names | `reference/gpui-component/crates/ui/src/icon.rs` |
| Theme schema & color tokens | `reference/gpui-component/.theme-schema.json` + `crates/ui/src/theme.rs` |
| Dock / Panel / Tab system | `reference/gpui-component/crates/ui/src/dock/` |
| Input / TextField | `reference/gpui-component/crates/ui/src/input/` |
| Form | `reference/gpui-component/crates/ui/src/form/` |
| Chart | `reference/gpui-component/crates/ui/src/chart/` |
| WebView | `reference/gpui-component/crates/webview/` + `examples/webview/` |
| Ví dụ hello world | `reference/gpui-component/examples/hello_world/src/main.rs` |
| Ví dụ DockArea | `reference/gpui-component/examples/sidebar/src/main.rs` |
| Skill agent (gpui, gpui-component) | `reference/gpui-component/skills/` |
| Tài liệu (en) | `reference/gpui-component/docs/docs/` |
| Tài liệu (zh-CN) | `reference/gpui-component/docs/zh-CN/docs/` |
| Story gallery source | `reference/gpui-component/crates/story/src/` |

### 5.4. Khi nào ĐƯỢC dùng web search

Chỉ dùng `web_search` / `fetch_content` / `code_search` cho gpui-component khi:

- **Tìm GitHub issue / PR cụ thể** (vd. biết số #2484 → search để đọc full thread).
- **Tra doc rust crate khác** (vd. `russh`, `portable-pty`, `alacritty_terminal`) — KHÔNG thuộc gpui-component.
- **Reference thiếu thông tin** (hiếm, vì reference là mirror đầy đủ).

Khi dùng web search cho gpui-component, **luôn ghi rõ trong response** lý do tại sao không tra trong reference.

### 5.5. Cập nhật reference

Nếu cần reference mới hơn (vd. upstream ra tag mới):

```bash
# Trong D:\TrungKFC-Research\Rust\myTerm2\reference\gpui-component\
git fetch origin
git checkout <tag-or-rev>
```

Sau đó cập nhật rev trong `Cargo.toml` workspace (section 1) cho khớp, và chạy lại `cargo build` để refresh `Cargo.lock`.

> ⚠️ Lệnh `git fetch` / `git checkout` trên reference chỉ chạy trong thư mục `D:\TrungKFC-Research\Rust\myTerm2\reference\gpui-component\` — vẫn nằm trong workspace, không vi phạm ràng buộc "không cd ra ngoài project".
