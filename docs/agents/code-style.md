# Quy ước code — myTerm2

> File tách từ `AGENTS.md` (section 4). Mọi rule ở đây là **bắt buộc** khi viết Rust cho project myTerm2.

## 1. Style & format

- Chạy `cargo fmt` trước khi commit. Config trong `.rustfmt.toml`.
- `cargo clippy --workspace --all-targets -- -D warnings` **phải pass** trước khi merge.
- Tên: `snake_case` (function, variable, module), `PascalCase` (type, trait, enum variant), `SCREAMING_SNAKE_CASE` (const).
- Import gộp theo nhóm, cách nhau 1 dòng trống:

```rust
use std::sync::Arc;

use gpui::*;
use gpui_component::{button::*, *};

use crate::state::AppState;
```

## 2. Quy tắc GPUI

> Chi tiết xem `reference/gpui-component/CLAUDE.md` — đây là nguồn chuẩn cho mọi quyết định về gpui.

- **Entry point bắt buộc**: gọi `gpui_component::init(cx)` trước khi dùng component.
- **Mọi window** phải bọc view trong `Root::new(view, window, cx)`.
- **Component stateless ưu tiên `RenderOnce`**, chỉ `Render` khi cần state nội bộ hoặc subscribe event.
- **Size**: dùng `Sizable` (`xs`/`sm`/`md`/`lg`).
- **Cursor**: button mặc định `default` cursor (theo desktop convention), chỉ `pointer` khi là link button.
- **Không** gọi `cx.spawn(...).detach()` mà quên cleanup — track task trong `AppState` nếu cần hủy theo session.
- **Global state**: dùng `cx.global::<AppState>()` cho data share toàn app; dùng `cx.new(|_| T)` cho entity riêng của view.

## 3. Quy tắc async & I/O

- Mọi network I/O chạy trong `cx.spawn(async move |cx| { ... })`.
- Kết quả trả về UI phải đi qua `cx.update(|cx| ...)` hoặc `cx.notify()`.
- **Không block** main thread. Không `std::sync::Mutex` chứa GPUI entity — dùng channel (`async-channel`) hoặc `smol::lock::Mutex` cho data chia sẻ.
- Logging dùng `tracing`, không `println!` (trừ khi debug nhanh, xóa trước khi commit).

## 4. Quy tắc domain

- `core` crate **không** phụ thuộc `gpui`, `ssh`, `local`. Nó chỉ chứa struct + trait.
- `ssh` và `local` đều implement một trait chung, ví dụ:

```rust
// crates/core/src/terminal/session.rs
#[async_trait]
pub trait TerminalSession: Send + Sync {
    async fn write(&self, data: &[u8]) -> Result<(), AppError>;
    async fn resize(&self, cols: u16, rows: u16) -> Result<(), AppError>;
    fn events(&self) -> broadcast::Receiver<TerminalEvent>;
    async fn shutdown(self: Box<Self>) -> Result<(), AppError>;
}
```

- `ui` chỉ biết trait này, không biết `russh` hay `alacritty_terminal::tty`.

## 5. Error handling

- Library crates (`core`, `ssh`, `local`, `ui`) trả `Result<T, AppError>` qua `thiserror`.
- Binary (`app`) dùng `anyhow` cho `main()`.
- Không `unwrap()` trong code production. Trong test/example thì được.
