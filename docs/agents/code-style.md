# Code conventions — OneTerm

> File split from `AGENTS.md` (section 4). Every rule here is **mandatory** when writing Rust for the OneTerm project.

## 1. Style & format

- Run `cargo fmt` before committing. Config lives in `.rustfmt.toml`.
- `cargo clippy --workspace --all-targets -- -D warnings` **must pass** before merging.
- Naming: `snake_case` (function, variable, module), `PascalCase` (type, trait, enum variant), `SCREAMING_SNAKE_CASE` (const).
- **English-only** — all code comments, doc comments (`///`), and any written content in the codebase **must be in English**. Do not write Vietnamese (or any other non-English language) in code. This is a hard rule with zero exceptions (see AGENTS.md Core principle 6).
- Group imports, separated by a blank line:

```rust
use std::sync::Arc;

use gpui::*;
use gpui_component::{button::*, *};

use crate::state::AppState;
```

## 2. GPUI rules

> See `reference/gpui-component/CLAUDE.md` for details — this is the authoritative source for every gpui decision.

- **Required entry point**: call `gpui_component::init(cx)` before using any component.
- **Every window** must wrap the view in `Root::new(view, window, cx)`.
- **Stateless components prefer `RenderOnce`**; use `Render` only when internal state or event subscription is needed.
- **Size**: use `Sizable` (`xs`/`sm`/`md`/`lg`).
- **Cursor**: buttons default to the `default` cursor (desktop convention); use `pointer` only for link buttons.
- **Do not** call `cx.spawn(...).detach()` and forget to clean up — track the task in `AppState` if it needs to be cancelled with the session.
- **Global state**: use `cx.global::<AppState>()` for data shared across the whole app; use `cx.new(|_| T)` for a view-specific entity.

## 3. Async & I/O rules

- All network I/O runs inside `cx.spawn(async move |cx| { ... })`.
- Results returned to the UI must go through `cx.update(|cx| ...)` or `cx.notify()`.
- **Never block** the main thread. Do not put a GPUI entity inside `std::sync::Mutex` — use a channel (`async-channel`) or `smol::lock::Mutex` for shared data.
- Logging uses `tracing`, not `println!` (except for quick debugging, removed before commit).

## 4. Domain rules

- The `core` crate **does not** depend on `gpui`, `ssh`, or `local`. It only holds structs + traits.
- Both `ssh` and `local` implement a shared trait, for example:

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

- `ui` only knows this trait; it does not know about `russh` or `alacritty_terminal::tty`.

## 5. Error handling

- Library crates (`core`, `ssh`, `local`, `ui`) return `Result<T, AppError>` via `thiserror`.
- The binary (`app`) uses `anyhow` for `main()`.
- No `unwrap()` in production code. In tests/examples it is allowed.