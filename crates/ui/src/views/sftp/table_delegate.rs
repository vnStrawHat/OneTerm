//! [`SftpTableDelegate`] — data source + cell rendering cho DataTable của SFTP.
//!
//! Thay thế render thủ công trong `render_list.rs`/`render.rs` bằng
//! `gpui_component::table::DataTable`: columns resizable, sortable, virtual
//! scroll. Trạng thái cột (width + visibility) được persist qua
//! `persistence.rs` → `docks.json`.

use std::collections::HashMap;

use gpui::{
    App, Context, Div, InteractiveElement as _, IntoElement, ParentElement,
    Stateful, Styled, TextAlign, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _,
    h_flex,
    menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem},
    table::{Column, ColumnFixed, ColumnSort, TableDelegate, TableState},
};
use crate::icon::AppIcon;
use myterm2_core::FileEntry;

use super::panel::SftpPanel;
use super::persistence::{read_sftp_table_state, write_sftp_table_state};
use super::types::{
    SftpColumnConfig, SftpTableStateJson, SortColumn, SortDir, format_date, format_owner,
    format_permissions, format_size, sort_dir_to_column_sort, sort_entries,
};

/// Index trong `col_configs` của các cột đang visible (thứ tự hiển thị).
type VisibleIndices = Vec<usize>;

/// DataTable delegate cho SFTP file list.
///
/// Sở hữu `entries` (dữ liệu), `col_configs` (config + width + visibility),
/// `sort` state, `loading` flag. Tham chiếu ngược `SftpPanel` qua `WeakEntity`
/// để trigger action từ context menu (rename, delete, ...).
pub(crate) struct SftpTableDelegate {
    pub(crate) entries: Vec<FileEntry>,
    pub(crate) col_configs: Vec<SftpColumnConfig>,
    visible_indices: VisibleIndices,
    /// `None` = default sort (Name asc, folder-first).
    pub(crate) sort: Option<(SortColumn, SortDir)>,
    pub(crate) loading: bool,
    panel: gpui::WeakEntity<SftpPanel>,
}

impl SftpTableDelegate {
    pub(crate) fn new(panel: gpui::WeakEntity<SftpPanel>) -> Self {
        let mut me = Self {
            entries: Vec::new(),
            col_configs: super::types::default_column_configs(),
            visible_indices: Vec::new(),
            sort: None,
            loading: false,
            panel,
        };
        me.apply_persisted_state();
        me.rebuild_visible_indices();
        me
    }

    // ── Config / persistence ──────────────────────────────────────

    /// Indices vào `col_configs` cho các cột đang visible.
    fn rebuild_visible_indices(&mut self) {
        self.visible_indices = self
            .col_configs
            .iter()
            .enumerate()
            .filter(|(_, c)| c.visible)
            .map(|(i, _)| i)
            .collect();
    }

    /// Áp dụng trạng thái đã persist (width + visibility) từ `docks.json`.
    /// Bỏ qua key không hợp lệ; Name luôn visible.
    fn apply_persisted_state(&mut self) {
        let Some(state) = read_sftp_table_state() else {
            return;
        };
        log::debug!(
            "SftpTableDelegate: apply persisted state — {} widths, {} visibility",
            state.column_widths.len(),
            state.column_visibility.len()
        );
        for cfg in &mut self.col_configs {
            if let Some(&w) = state.column_widths.get(cfg.key) {
                if w >= cfg.min_width && w <= cfg.max_width {
                    cfg.width = w;
                }
            }
            if let Some(&visible) = state.column_visibility.get(cfg.key) {
                // Name luôn visible — bỏ qua hidden cho Name.
                cfg.visible = visible || cfg.col == SortColumn::Name;
            }
        }
    }

    /// Đọc config hiện tại → `SftpTableStateJson` để persist.
    pub(crate) fn to_persisted_state(&self) -> SftpTableStateJson {
        let mut column_widths = HashMap::new();
        let mut column_visibility = HashMap::new();
        for cfg in &self.col_configs {
            column_widths.insert(cfg.key.to_string(), cfg.width);
            column_visibility.insert(cfg.key.to_string(), cfg.visible);
        }
        SftpTableStateJson {
            column_widths,
            column_visibility,
        }
    }

    /// Persist trạng thái cột hiện tại vào `docks.json`.
    pub(crate) fn persist(&self) {
        if let Err(e) = write_sftp_table_state(&self.to_persisted_state()) {
            log::warn!("SftpTableDelegate: persist failed: {e}");
        }
    }

    /// Cập nhật width cho các cột visible từ danh sách width của DataTable
    /// (theo thứ tự visible). Dùng cho `TableEvent::ColumnWidthsChanged`.
    pub(crate) fn apply_widths(&mut self, widths: &[gpui::Pixels]) {
        for (vis_ix, w) in widths.iter().enumerate() {
            if let Some(&cfg_ix) = self.visible_indices.get(vis_ix) {
                let cfg = &mut self.col_configs[cfg_ix];
                cfg.width = w.as_f32().clamp(cfg.min_width, cfg.max_width);
            }
        }
    }

    /// Toggle visibility của 1 cột. Name không thể ẩn. Trả về `false` nếu
    /// cố ẩn Name.
    pub(crate) fn toggle_visibility(&mut self, col: SortColumn) -> bool {
        if col == SortColumn::Name {
            return false;
        }
        if let Some(cfg) = self.col_configs.iter_mut().find(|c| c.col == col) {
            cfg.visible = !cfg.visible;
            log::debug!("SftpTableDelegate: toggle {:?} → visible={}", col, cfg.visible);
            self.rebuild_visible_indices();
            true
        } else {
            false
        }
    }

    // ── Entries / sort ────────────────────────────────────────────

    /// Thay thế entries + re-sort theo sort state hiện tại.
    pub(crate) fn set_entries(&mut self, mut entries: Vec<FileEntry>) {
        sort_entries(&mut entries, self.sort);
        self.entries = entries;
    }

    /// Re-sort entries hiện tại (dùng sau khi đổi sort state).
    fn resort(&mut self) {
        sort_entries(&mut self.entries, self.sort);
    }

    /// Config cột visible tại `col_ix` (index trong visible order).
    fn visible_cfg(&self, col_ix: usize) -> Option<&SftpColumnConfig> {
        self.visible_indices
            .get(col_ix)
            .and_then(|&i| self.col_configs.get(i))
    }
}

impl TableDelegate for SftpTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.visible_indices.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.entries.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let Some(cfg) = self.visible_cfg(col_ix) else {
            return Column::new(format!("col-{col_ix}"), "");
        };

        let mut col = Column::new(cfg.key, cfg.label)
            .width(cfg.width)
            .min_width(cfg.min_width)
            .max_width(cfg.max_width)
            .resizable(true)
            .movable(false)
            .sortable();

        if cfg.right_align {
            col = col.text_right();
        }

        // Sort indicator: active column → Ascending/Descending, other sortable → Default.
        col = match self.sort {
            Some((sort_col, dir)) if sort_col == cfg.col => col.sort(sort_dir_to_column_sort(dir)),
            _ => col.sort(ColumnSort::Default),
        };

        // Pin Name column ở bên trái (không scroll ra khỏi view khi horizontal scroll).
        if cfg.col == SortColumn::Name {
            col = col.fixed(ColumnFixed::Left);
        }

        col
    }


    fn render_th(
        &mut self,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .size_full()
            .items_center()
            .text_color(theme.foreground)
            .child(self.visible_cfg(col_ix).map_or(String::new(), |cfg| cfg.label.to_string()))
    }
    fn render_tr(
        &mut self,
        row_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        // Highlight dòng đang chọn = `table_hover` (giống hover, không border).
        // Border/overlay mặc định của DataTable đã bị tắt qua theme override
        // (`table_active` + `table_active_border` = transparent).
        //
        // Đọc `selected` trực tiếp từ SftpPanel (single source of truth) thay vì
        // sync qua event — tránh re-entrancy khi `clear_selection` emit trong
        // `table.update`.
        let selected = self
            .panel
            .upgrade()
            .and_then(|p| p.read(cx).selected);
        let row = div().id(("row", row_ix));
        if selected == Some(row_ix) {
            row.bg(cx.theme().tokens.table_hover)
        } else {
            row
        }
    }
    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(entry) = self.entries.get(row_ix) else {
            return div().into_any_element();
        };
        let Some(cfg) = self.visible_cfg(col_ix) else {
            return div().into_any_element();
        };

        let theme = cx.theme();
        let muted = theme.muted_foreground;

        match cfg.col {
            SortColumn::Name => {
                let icon = if entry.is_dir {
                    AppIcon::FolderA.colored().size(px(16.)).bg(gpui::transparent_black())
                } else {
                    AppIcon::File3.colored().size(px(16.)).bg(gpui::transparent_black())
                };

                h_flex()
                    .w_full()
                    .h_full()
                    .items_center()
                    .gap_1()
                    .min_w_0()
                    .child(div().w_4().flex_shrink_0().child(icon))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_color(theme.foreground)
                            .truncate()
                            .child(entry.name.clone()),
                    )
                    .into_any_element()
            }
            SortColumn::Modified => div()
                .text_xs()
                .text_color(muted)
                .child(format_date(entry.modified))
                .into_any_element(),
            SortColumn::Size => {
                let text = if entry.is_dir {
                    String::new()
                } else {
                    format_size(entry.size)
                };
                div()
                    .text_xs()
                    .text_color(muted)
                    .text_align(TextAlign::Right)
                    .child(text)
                    .into_any_element()
            }
            SortColumn::Permissions => div()
                .text_xs()
                .text_color(muted)
                .child(format_permissions(entry.permissions))
                .into_any_element(),
            SortColumn::Owner => div()
                .text_xs()
                .text_color(muted)
                .child(format_owner(entry.owner.as_deref(), entry.uid))
                .into_any_element(),
            SortColumn::Group => div()
                .text_xs()
                .text_color(muted)
                .child(format_owner(entry.group.as_deref(), entry.gid))
                .into_any_element(),
        }
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) {
        let Some(cfg) = self.visible_cfg(col_ix) else {
            return;
        };
        let col = cfg.col;
        self.sort = match sort {
            ColumnSort::Default => None,
            ColumnSort::Ascending => Some((col, SortDir::Asc)),
            ColumnSort::Descending => Some((col, SortDir::Desc)),
        };
        log::debug!("SftpTableDelegate: perform_sort {:?} → {:?}", col, self.sort);
        self.resort();
    }

    fn loading(&self, _: &App) -> bool {
        self.loading
    }

    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        // Select row trên right-click (mirror sang SftpPanel để toolbar actions dùng).
        if let Some(panel) = self.panel.upgrade() {
            panel.update(cx, |this, cx| {
                this.selected = Some(row_ix);
                cx.notify();
            });
        }

        let Some(entry) = self.entries.get(row_ix).cloned() else {
            return menu;
        };
        let is_dir = entry.is_dir;

        let panel = self.panel.clone();

        // First item: Open (dir) hoặc Download (file).
        let menu = if is_dir {
            menu.item(PopupMenuItem::new("Open").on_click({
                let panel = panel.clone();
                move |_, _, cx| {
                    if let Some(panel) = panel.upgrade() {
                        panel.update(cx, |this, cx| {
                            this.pending_action = Some(super::types::PendingAction::Open(row_ix));
                            cx.notify();
                        });
                    }
                }
            }))
        } else {
            menu.item(PopupMenuItem::new("Download").on_click({
                let panel = panel.clone();
                move |_, _, cx| {
                    if let Some(panel) = panel.upgrade() {
                        panel.update(cx, |this, cx| {
                            this.pending_action = Some(super::types::PendingAction::Download);
                            cx.notify();
                        });
                    }
                }
            }))
        };

        menu.separator()
            .item(PopupMenuItem::new("Rename").on_click({
                let panel = panel.clone();
                move |_, _, cx| {
                    if let Some(panel) = panel.upgrade() {
                        panel.update(cx, |this, cx| {
                            this.pending_action = Some(super::types::PendingAction::Rename);
                            cx.notify();
                        });
                    }
                }
            }))
            .item(PopupMenuItem::new("Delete").on_click({
                let panel = panel.clone();
                move |_, _, cx| {
                    if let Some(panel) = panel.upgrade() {
                        panel.update(cx, |this, cx| {
                            this.pending_action = Some(super::types::PendingAction::Delete);
                            cx.notify();
                        });
                    }
                }
            }))
            .separator()
            .item(PopupMenuItem::new("Properties").on_click({
                let panel = panel.clone();
                move |_, _, cx| {
                    if let Some(panel) = panel.upgrade() {
                        panel.update(cx, |this, cx| {
                            this.pending_action = Some(super::types::PendingAction::Properties);
                            cx.notify();
                        });
                    }
                }
            }))
            .separator()
            .item(PopupMenuItem::new("Upload").on_click({
                let panel = panel.clone();
                move |_, _, cx| {
                    if let Some(panel) = panel.upgrade() {
                        panel.update(cx, |this, cx| {
                            this.pending_action = Some(super::types::PendingAction::Upload);
                            cx.notify();
                        });
                    }
                }
            }))
            .item(PopupMenuItem::new("New Folder").on_click({
                let panel = panel.clone();
                move |_, _, cx| {
                    if let Some(panel) = panel.upgrade() {
                        panel.update(cx, |this, cx| {
                            this.pending_action = Some(super::types::PendingAction::NewFolder);
                            cx.notify();
                        });
                    }
                }
            }))
            .item(PopupMenuItem::new("Refresh").on_click({
                let panel = panel;
                move |_, _, cx| {
                    if let Some(panel) = panel.upgrade() {
                        panel.update(cx, |this, cx| {
                            this.pending_action = Some(super::types::PendingAction::Refresh);
                            cx.notify();
                        });
                    }
                }
            }))
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let panel = self.panel.clone();

        div()
            .id("sftp-empty-area")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(theme.muted_foreground)
            .child("Empty directory.")
            .context_menu(move |menu, _window, _cx| {
                let panel = panel.clone();
                menu.item(PopupMenuItem::new("Upload").on_click({
                    let panel = panel.clone();
                    move |_, _, cx| {
                        if let Some(panel) = panel.upgrade() {
                            panel.update(cx, |this, cx| {
                                this.pending_action = Some(super::types::PendingAction::Upload);
                                cx.notify();
                            });
                        }
                    }
                }))
                .item(PopupMenuItem::new("New Folder").on_click({
                    let panel = panel.clone();
                    move |_, _, cx| {
                        if let Some(panel) = panel.upgrade() {
                            panel.update(cx, |this, cx| {
                                this.pending_action = Some(super::types::PendingAction::NewFolder);
                                cx.notify();
                            });
                        }
                    }
                }))
                .separator()
                .item(PopupMenuItem::new("Refresh").on_click({
                    let panel = panel;
                    move |_, _, cx| {
                        if let Some(panel) = panel.upgrade() {
                            panel.update(cx, |this, cx| {
                                this.pending_action = Some(super::types::PendingAction::Refresh);
                                cx.notify();
                            });
                        }
                    }
                }))
            })
    }
}
