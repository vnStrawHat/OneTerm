//! AppState — state toàn cục của OneTerm.
//!
//! Skeleton: chưa có state chia sẻ. Sau này chứa danh sách host,
//! session state, ui_state (vd. `invisible_panels`).

use std::sync::{Arc, Mutex};

use gpui::{App, AppContext, Entity, Global, WeakEntity};
use gpui_component::dock::DockArea;
use oneterm_core::SftpBackend;

/// State toàn cục của ứng dụng.
#[derive(Default)]
pub struct AppState {
    /// Tham chiếu yếu tới DockArea — dùng cho dialog connect SSH
    /// (thêm terminal tab sau khi kết nối thành công).
    /// Set trong `OneTermWorkspace::new` sau khi DockArea được tạo.
    pub dock_area: Option<WeakEntity<DockArea>>,
    /// Mirror trạng thái zoom (tên panel đang fullscreen) — chia sẻ với
    /// `on_release` callback trong `window.rs` để save khi close window.
    pub zoomed_panel: Option<Arc<Mutex<Option<String>>>>,
    /// Mirror toggle_button_visible — chia sẻ với `on_release` callback.
    pub toggle_button_visible: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// SFTP backend của terminal tab đang active.
    /// `None` = tab active không có SFTP (local shell hoặc SSH không hỗ trợ SFTP).
    /// Set bởi `TerminalPanel::set_active(true)` — khi tab đổi, ghi đè giá trị cũ.
    pub active_sftp: Option<Arc<dyn SftpBackend>>,
}

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
        let state = cx.new(|_| Self::default());
        cx.set_global(AppStateGlobal(state));
    }
}