//! AppState — state toàn cục của myTerm2.
//!
//! Skeleton: chưa có state chia sẻ. Sau này chứa danh sách host,
//! session state, ui_state (vd. `invisible_panels`).

use gpui::{App, AppContext, Entity, Global};

/// State toàn cục của ứng dụng.
#[derive(Default)]
pub struct AppState;

/// Global wrapper cho `Entity<AppState>`.
pub struct AppStateGlobal(pub Entity<AppState>);

impl Global for AppStateGlobal {}

impl AppState {
    /// Lấy `Entity<AppState>` toàn cục (panic nếu chưa init).
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<AppStateGlobal>().0.clone()
    }

    /// Khởi tạo AppState toàn cục.
    pub fn init(cx: &mut App) {
        let state = cx.new(|_| Self);
        cx.set_global(AppStateGlobal(state));
    }
}
