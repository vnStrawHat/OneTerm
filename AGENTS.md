# AGENTS.md — myTerm2

> Hướng dẫn dành cho AI agent (và contributor) khi làm việc với project **myTerm2** — một SSH / SFTP / LocalShell GUI Client viết bằng Rust + [gpui-component](https://github.com/longbridge/gpui-component).

## Mục lục

| # | Chủ đề | File |
|---|---|---|
| 1 | Giới thiệu dự án & nguyên tắc cốt lõi | `AGENTS.md` (file này) |
| 2 | Cấu trúc dự án (cây thư mục, quy tắc, dependency graph) | [`docs/agents/structure.md`](docs/agents/structure.md) |
| 3 | Hướng dẫn phát triển (workflow, lệnh, git) | `AGENTS.md` |
| 4 | Quy ước code (style, GPUI, async, error) | [`docs/agents/code-style.md`](docs/agents/code-style.md) |
| 5 | Dependencies, rev lock, gpui-component integration, reference-first research | [`docs/agents/dependencies.md`](docs/agents/dependencies.md) |
| 6 | Roadmap & quick ref | `AGENTS.md` |

> 📚 **Bắt buộc đọc** trước khi viết code:
> 1. `AGENTS.md` (file này) — để hiểu overview, workflow, git, quality gate.
> 2. [`docs/agents/structure.md`](docs/agents/structure.md) — để biết cấu trúc crate, cây thư mục, dependency graph.
> 3. [`docs/agents/code-style.md`](docs/agents/code-style.md) — để biết quy tắc code (GPUI, async, error).
> 4. [`docs/agents/dependencies.md`](docs/agents/dependencies.md) — để biết rev lock & reference-first research.

---

## 1. Giới thiệu dự án

**myTerm2** là một GUI client đa nền tảng (macOS / Linux / Windows) cung cấp:

- **SSH client** — kết nối shell từ xa (russh).
- **SFTP client** — duyệt, upload, download file từ xa.
- **LocalShell** — chạy shell cục bộ qua PTY.
- **Terminal emulator** — render ANSI/VT, màu sắc, scrollback.
- **Host manager** — lưu & quản lý danh sách host, credentials.
- **Session tabs** — mở nhiều session cùng lúc trong workspace dock.
- **Settings / themes** — cấu hình font, màu, phím tắt, theme.

### Nguyên tắc cốt lõi

1. **Tách lớp rõ ràng** — UI không chứa logic giao thức; giao thức không biết gì về UI.
2. **Tách crate theo domain** — mỗi crate một trách nhiệm duy nhất.
3. **Async-first** — mọi I/O (network, PTY, file) đều chạy bất đồng bộ qua `cx.spawn` / `smol` / `tokio`.
4. **Type-safe state** — dùng `Entity<T>` của GPUI, tránh chia sẻ `Rc<RefCell<…>>` ở tầng ứng dụng.
5. **Reference-first research** — khi cần tìm hiểu API, code example, hay tài liệu về `gpui` / `gpui-component`, **luôn đọc từ `D:\TrungKFC-Research\Rust\myTerm2\reference\gpui-component\`**. Chi tiết xem [`docs/agents/dependencies.md` § 5](docs/agents/dependencies.md).

> 📂 **Cấu trúc dự án** (cây thư mục, quy tắc tổ chức file, dependency graph giữa các crate) được tách ra file riêng: xem [`docs/agents/structure.md`](docs/agents/structure.md).

---

## 3. Hướng dẫn phát triển

### 3.0. Tìm hiểu API gpui-component — đọc reference trước tiên

Trước khi viết code UI, dùng `find` / `grep` / `read` trên `D:\TrungKFC-Research\Rust\myTerm2\reference\gpui-component\` để tra API. Xem [`docs/agents/dependencies.md` § 5](docs/agents/dependencies.md) để biết bảng tra cứu chi tiết. **Không** dùng `web_search` cho gpui-component trừ khi reference thiếu.

### 3.1. Lệnh thường dùng

> ⚠️ **Giới hạn an toàn**: Mọi lệnh dưới đây **chỉ được chạy trong** `D:\TrungKFC-Research\Rust\myTerm2`. Không cd ra ngoài, không chạy `cargo init` / `git clone` trên thư mục khác.

```bash
# Format
cargo fmt --all

# Lint (phải sạch warning)
cargo clippy --workspace --all-targets -- -D warnings

# Build
cargo build --workspace

# Build release
cargo build --workspace --release

# Chạy app
cargo run -p app

# Test
cargo test --workspace
```

**Không chạy**:

- ❌ `cargo init` / `cargo new` (project đã có sẵn).
- ❌ `git clone` ra ngoài workspace.
- ❌ Bất kỳ lệnh nào thay đổi file ngoài `D:\TrungKFC-Research\Rust\myTerm2`.
- ❌ `rm -rf` không có guard path.
- X `find /` hay các lệnh tương tự với đường dẫn nằm ngoài thư mục dự án

### 3.2. Quy trình thêm tính năng

1. **Đọc trước** `reference/gpui-component/CLAUDE.md` và code tương ứng trong `reference/gpui-component/` trước khi viết UI.
2. **Đọc** [`docs/agents/structure.md`](docs/agents/structure.md) để biết crate nào chứa gì + quy tắc phụ thuộc.
3. **Đọc** [`docs/agents/code-style.md`](docs/agents/code-style.md) để nắm quy tắc code.
4. **Cập nhật `core`** trước nếu cần thêm domain type / trait.
5. **Triển khai** trong crate tương ứng (`ssh` / `local` / `ui`).
6. **Gắn** vào `ui::state::AppState` và `layout::workspace`.
7. Chạy `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings`.
8. Commit với message theo Conventional Commits (xem mục 4).

### 3.3. Khi thêm gpui-component mới

- Đặt file trong `crates/ui/src/components/<name>.rs`.
- Nếu là widget stateless → ưu tiên `RenderOnce`.
- Nếu cần state → `Render` + `Entity<T>` cho state, expose qua `cx.new(|_| State::new())`.
- Viết doc comment `///` cho mọi public item.

### 3.4. Theme & icon

- Theme: tạo JSON trong `config/themes/`, đăng ký qua `crates/ui/src/theme.rs`.
- Icon: dùng [Lucide](https://lucide.dev). Đặt SVG tại `assets/icons/<name>.svg` với `<name>` trùng tên trong `IconName` (xem `reference/gpui-component/crates/ui/src/icon.rs`).
- Không hardcode màu trong component — đọc từ `cx.theme()`.

---

## 4. Git & Commit

- Branch naming: `feat/<scope>`, `fix/<scope>`, `refactor/<scope>`, `docs/<scope>`.
- Commit message (Conventional Commits):

```
<type>(<scope>): <short description>

<body — giải thích "tại sao", không chỉ "làm gì">

Refs: #issue
```

- **Không commit**: `target/`, `Cargo.lock` nếu là library (workspace binary vẫn commit `Cargo.lock`), `reference/`, `.pi/`.
- `.gitignore` đã loại `target/`, `reference/`, `.pi/`. Giữ nguyên.

---

## 5. Kiểm tra chất lượng tự động (quality gate)

Trước khi hoàn tất một task, agent **phải** chạy và xác nhận pass:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
```

Nếu một trong ba lệnh trên fail → sửa trước khi báo cáo hoàn thành.

> 📌 **Ghi nhớ**: Mọi lệnh phải chạy trong `D:\TrungKFC-Research\Rust\myTerm2`. Nếu agent cần kiểm tra file ở `reference/`, dùng **đường dẫn tương đối** từ workspace root, không `cd` ra ngoài.
>
> 📚 **Tra cứu gpui-component**: trước khi dùng `web_search` / `fetch_content` / `code_search` cho thông tin về gpui / gpui-component, **phải đọc trong `reference/gpui-component/` trước** — xem [`docs/agents/dependencies.md` § 5](docs/agents/dependencies.md).

---

## 6. Định hướng mở rộng (roadmap gợi ý)

- [ ] Workspace skeleton + Cargo.toml + .gitignore + .rustfmt.toml + clippy.toml.
- [ ] `core`: types + traits (`TerminalSession`, `FileTransfer`).
- [ ] `local`: PTY shell hoạt động được trong gpui view.
- [ ] `ssh`: connect bằng password + pubkey, shell channel.
- [ ] `ssh/sftp`: list / upload / download.
- [ ] `ui`: layout (sidebar + dock + statusbar) + host manager + session tabs.
- [ ] Settings UI (font, theme, key bindings).
- [ ] i18n (en, vi).
- [ ] CI: build & test trên macOS / Linux / Windows.
- [ ] Đóng gói installer (cargo-bundle hoặc cargo-dist).

---

## 7. Quick reference

### 7.1. Đường dẫn nhảy nhanh

| Cần biết gì | Đọc ở đâu |
|---|---|
| Cấu trúc dự án (cây thư mục, quy tắc, dependency graph) | [`docs/agents/structure.md`](docs/agents/structure.md) |
| Quy tắc code (style, GPUI, async, error) | [`docs/agents/code-style.md`](docs/agents/code-style.md) |
| Rev lock, dependencies, reference-first | [`docs/agents/dependencies.md`](docs/agents/dependencies.md) |
| **Thiết kế terminal backend** (local + ssh, render alacritty) | [`docs/terminal-backend.md`](docs/terminal-backend.md) |
| API overview gpui-component | `reference/gpui-component/CLAUDE.md` |
| Component list & source | `reference/gpui-component/crates/ui/src/` |
| Ví dụ app đơn giản | `reference/gpui-component/examples/hello_world/` |
| Ví dụ DockArea | `reference/gpui-component/examples/sidebar/src/main.rs` |
| Icon names | `reference/gpui-component/crates/ui/src/icon.rs` |
| Theme schema & color | `reference/gpui-component/.theme-schema.json` |
| Skill nội bộ của gpui | `reference/gpui-component/skills/` |
| Tài liệu (en) | `reference/gpui-component/docs/docs/` |

### 7.2. Câu hỏi thường gặp

**Q: Một file Rust mới nên đặt ở đâu?**
A: Xem [`docs/agents/structure.md`](docs/agents/structure.md). Có 5 crate (`app`, `core`, `ssh`, `local`, `ui`) với quy tắc phụ thuộc nghiêm ngặt; UI không được import từ `ssh`/`local`.

**Q: Cần thêm 1 gpui component mới — bắt đầu từ đâu?**
A: Đọc [`docs/agents/code-style.md` § 2](docs/agents/code-style.md), rồi `grep -rn "<tên component>" reference/gpui-component/crates/ui/src/` để tìm API.

**Q: Muốn thêm crate Rust mới?**
A: Xem [`docs/agents/dependencies.md` § 3](docs/agents/dependencies.md). Nếu crate đã có trong `reference/gpui-component/Cargo.toml` → dùng luôn rev đã lock. Nếu chưa → mở issue trước.

**Q: gpui-component vừa ra version mới — cập nhật thế nào?**
A: Xem [`docs/agents/dependencies.md` § 4 và 5.5](docs/agents/dependencies.md).
