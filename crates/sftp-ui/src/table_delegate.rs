//! [`SftpTableDelegate`] — data source + cell rendering for the SFTP DataTable.
//!
//! The file list is a `gpui_component::table::DataTable`: resizable, sortable
//! columns and virtual scroll. Column state (width + visibility) is persisted via
//! `persistence.rs` → `docks.json`; [`SftpPanel`] reads and writes it on the
//! background executor and hands the snapshot to this delegate.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    App, Context, Div, InteractiveElement as _, IntoElement, ParentElement, Stateful, Styled,
    TextAlign, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, h_flex,
    menu::{ContextMenuExt as _, PopupMenu},
    table::{Column, ColumnFixed, ColumnSort, TableDelegate, TableState},
};
use oneterm_core::{FileEntry, SftpTableState};
use oneterm_theme::icon::AppIcon;

use super::panel::SftpPanel;
use super::types::{
    SftpColumnConfig, SortColumn, SortDir, format_date, format_owner, format_permissions,
    format_size, sort_dir_to_column_sort, sort_entries,
};

/// Indices into `col_configs` of the currently visible columns (display order).
type VisibleIndices = Vec<usize>;

/// DataTable delegate for the SFTP file list.
///
/// Owns `entries` (data), `col_configs` (config + width + visibility),
/// `sort` state, and the `loading` flag. Holds a back-reference to `SftpPanel`
/// via `WeakEntity` to trigger actions from the context menu (rename, delete, ...).
///
/// Entries are kept behind an `Arc<[FileEntry]>` so the per-backend store can
/// snapshot the listing without cloning every row; sorting rebuilds the Arc.
pub(crate) struct SftpTableDelegate {
    entries: Arc<[FileEntry]>,
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
            entries: Arc::from([]),
            col_configs: super::types::default_column_configs(),
            visible_indices: Vec::new(),
            sort: None,
            loading: false,
            panel,
        };
        me.rebuild_visible_indices();
        me
    }

    // ── Config / persistence ──────────────────────────────────────

    /// Indices into `col_configs` for the currently visible columns.
    fn rebuild_visible_indices(&mut self) {
        self.visible_indices = self
            .col_configs
            .iter()
            .enumerate()
            .filter(|(_, c)| c.visible)
            .map(|(i, _)| i)
            .collect();
    }

    /// Apply the persisted state (width + visibility) read from `docks.json`.
    /// Ignores invalid keys; Name is always visible.
    pub(crate) fn apply_persisted_state(&mut self, state: &SftpTableState) {
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
                // Name is always visible — ignore hidden for Name.
                cfg.visible = visible || cfg.col == SortColumn::Name;
            }
        }
        self.rebuild_visible_indices();
    }

    /// Read the current config for persistence.
    pub(crate) fn to_persisted_state(&self) -> SftpTableState {
        let mut column_widths = HashMap::new();
        let mut column_visibility = HashMap::new();
        for cfg in &self.col_configs {
            column_widths.insert(cfg.key.to_string(), cfg.width);
            column_visibility.insert(cfg.key.to_string(), cfg.visible);
        }
        SftpTableState {
            column_widths,
            column_visibility,
        }
    }

    /// Update widths for the visible columns from the DataTable's width list
    /// (in visible order). Used for `TableEvent::ColumnWidthsChanged`.
    pub(crate) fn apply_widths(&mut self, widths: &[gpui::Pixels]) {
        for (vis_ix, w) in widths.iter().enumerate() {
            if let Some(&cfg_ix) = self.visible_indices.get(vis_ix) {
                let cfg = &mut self.col_configs[cfg_ix];
                cfg.width = w.as_f32().clamp(cfg.min_width, cfg.max_width);
            }
        }
    }

    /// Toggle the visibility of a column. Name cannot be hidden. Returns `false`
    /// if attempting to hide Name.
    pub(crate) fn toggle_visibility(&mut self, col: SortColumn) -> bool {
        if col == SortColumn::Name {
            return false;
        }
        if let Some(cfg) = self.col_configs.iter_mut().find(|c| c.col == col) {
            cfg.visible = !cfg.visible;
            log::debug!(
                "SftpTableDelegate: toggle {:?} → visible={}",
                col,
                cfg.visible
            );
            self.rebuild_visible_indices();
            true
        } else {
            false
        }
    }

    // ── Entries / sort ────────────────────────────────────────────

    /// The listing in display order.
    pub(crate) fn entries(&self) -> &[FileEntry] {
        &self.entries
    }

    /// Share the current listing without copying rows.
    pub(crate) fn entries_snapshot(&self) -> Arc<[FileEntry]> {
        Arc::clone(&self.entries)
    }

    /// Replace entries + re-sort by the current sort state.
    pub(crate) fn set_entries(&mut self, mut entries: Vec<FileEntry>) {
        sort_entries(&mut entries, self.sort);
        self.entries = Arc::from(entries);
    }

    /// Re-sort the current entries (used after changing the sort state).
    fn resort(&mut self) {
        let mut entries = self.entries.to_vec();
        sort_entries(&mut entries, self.sort);
        self.entries = Arc::from(entries);
    }

    /// Display index of the entry at `path`, if it is listed.
    fn index_of_path(&self, path: &oneterm_core::RemotePath) -> Option<usize> {
        self.entries.iter().position(|entry| entry.path == *path)
    }

    /// Config for the visible column at `col_ix` (index in visible order).
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

        // Pin the Name column to the left (won't scroll out of view on horizontal scroll).
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
            .child(
                self.visible_cfg(col_ix)
                    .map_or(String::new(), |cfg| cfg.label.to_string()),
            )
    }
    fn render_tr(
        &mut self,
        row_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        // Highlight the selected row = `table_hover` (same as hover, no border).
        // DataTable's default border/overlay is disabled via theme override
        // (`table_active` + `table_active_border` = transparent).
        //
        // Read `selected` directly from SftpPanel (single source of truth) instead of
        // syncing via events — avoids re-entrancy when `clear_selection` emits inside
        // `table.update`.
        let selected = self
            .panel
            .upgrade()
            .and_then(|p| p.read(cx).browser().selected());
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
                    AppIcon::Folder.colored().size(px(19.))
                } else {
                    AppIcon::File.colored().size(px(19.))
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
        cx: &mut Context<TableState<Self>>,
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
        log::debug!(
            "SftpTableDelegate: perform_sort {:?} → {:?}",
            col,
            self.sort
        );
        let Some(panel) = self.panel.upgrade() else {
            self.resort();
            return;
        };
        // The selection is an index into the listing; remember which entry it
        // names so it can follow that entry to its new position (CORR-30).
        let selected_path = panel
            .read(cx)
            .browser()
            .selected()
            .and_then(|ix| self.entries.get(ix))
            .map(|entry| entry.path.clone());
        self.resort();
        let remapped = selected_path.and_then(|path| self.index_of_path(&path));
        panel.update(cx, |panel, cx| {
            panel.browser_mut().select(remapped);
            panel.mark_entries_dirty();
            cx.notify();
        });
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
        // Select the row on right-click (mirror to SftpPanel so toolbar actions can use it).
        if let Some(panel) = self.panel.upgrade() {
            panel.update(cx, |this, cx| {
                this.browser_mut().select(Some(row_ix));
                cx.notify();
            });
        }

        let Some(entry) = self.entries.get(row_ix).cloned() else {
            return menu;
        };
        let is_dir = entry.is_dir;

        super::table_delegate_menu::build_entry_menu(menu, &self.panel, row_ix, is_dir)
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
                super::table_delegate_menu::build_empty_menu(menu, &panel)
            })
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, px};
    use oneterm_core::{RemotePath, SftpTableState};

    use super::*;
    use crate::test_backend::dir_entry;

    /// A delegate wired to a throw-away panel (the delegate needs a back-reference).
    fn delegate(cx: &mut TestAppContext) -> SftpTableDelegate {
        cx.update(gpui_component::init);
        cx.update(oneterm_state::AppState::init);
        cx.update(crate::browser_state::SftpBrowserStore::init);
        let (panel, _cx) = cx.add_window_view(|window, cx| SftpPanel::new(window, cx));
        SftpTableDelegate::new(panel.downgrade())
    }

    fn visible_keys(delegate: &SftpTableDelegate) -> Vec<&'static str> {
        delegate
            .col_configs
            .iter()
            .filter(|c| c.visible)
            .map(|c| c.key)
            .collect()
    }

    #[gpui::test]
    fn persisted_state_round_trips_and_ignores_invalid_values(cx: &mut TestAppContext) {
        let mut delegate = delegate(cx);
        let mut state = SftpTableState::default();
        state.column_widths.insert("size".into(), 120.0);
        // Below the minimum width and an unknown key: both ignored.
        state.column_widths.insert("owner".into(), 1.0);
        state.column_widths.insert("bogus".into(), 50.0);
        state.column_visibility.insert("group".into(), false);
        // Name can never be hidden.
        state.column_visibility.insert("name".into(), false);

        delegate.apply_persisted_state(&state);

        let width = |key: &str| {
            delegate
                .col_configs
                .iter()
                .find(|c| c.key == key)
                .map(|c| c.width)
                .unwrap()
        };
        assert_eq!(width("size"), 120.0);
        assert_eq!(width("owner"), 90.0);
        assert_eq!(
            visible_keys(&delegate),
            vec!["name", "modified", "permissions", "size", "owner"]
        );

        let persisted = delegate.to_persisted_state();
        assert_eq!(persisted.column_widths["size"], 120.0);
        assert!(!persisted.column_visibility["group"]);
        assert!(persisted.column_visibility["name"]);
        assert!(!persisted.column_widths.contains_key("bogus"));
    }

    #[gpui::test]
    fn toggling_visibility_never_hides_name(cx: &mut TestAppContext) {
        let mut delegate = delegate(cx);
        assert!(!delegate.toggle_visibility(SortColumn::Name));
        assert!(delegate.toggle_visibility(SortColumn::Owner));
        assert!(!visible_keys(&delegate).contains(&"owner"));
        assert!(delegate.toggle_visibility(SortColumn::Owner));
        assert!(visible_keys(&delegate).contains(&"owner"));
    }

    #[gpui::test]
    fn widths_apply_in_visible_order_and_are_clamped(cx: &mut TestAppContext) {
        let mut delegate = delegate(cx);
        delegate.toggle_visibility(SortColumn::Modified);
        // Visible order is now: name, permissions, size, owner, group.
        delegate.apply_widths(&[px(500.), px(10.), px(9999.)]);
        let width = |key: &str| {
            delegate
                .col_configs
                .iter()
                .find(|c| c.key == key)
                .map(|c| c.width)
                .unwrap()
        };
        assert_eq!(width("name"), 500.0);
        assert_eq!(width("permissions"), 40.0);
        assert_eq!(width("size"), 800.0);
        // The hidden column keeps its default width.
        assert_eq!(width("modified"), 140.0);
    }

    /// CORR-30: the selection names an entry, not a row number — after a
    /// re-sort it must still point at the same file.
    #[gpui::test]
    fn sorting_keeps_the_selected_entry_selected(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(oneterm_state::AppState::init);
        cx.update(crate::browser_state::SftpBrowserStore::init);
        let (panel, cx) = cx.add_window_view(|window, cx| SftpPanel::new(window, cx));
        let table = panel.read_with(cx, |panel, _| panel.table().clone());
        let root = RemotePath::root();

        table.update(cx, |table, _| {
            table.delegate_mut().set_entries(vec![
                dir_entry(&root, "a.txt", false),
                dir_entry(&root, "b.txt", false),
                dir_entry(&root, "c.txt", false),
            ]);
        });
        panel.update(cx, |panel, _| panel.browser_mut().select(Some(0)));

        // Name column (visible index 0), descending: c, b, a.
        table.update_in(cx, |table, window, cx| {
            table
                .delegate_mut()
                .perform_sort(0, ColumnSort::Descending, window, cx);
        });
        let names: Vec<String> = table.read_with(cx, |table, _| {
            table
                .delegate()
                .entries()
                .iter()
                .map(|entry| entry.name.clone())
                .collect()
        });
        assert_eq!(names, vec!["c.txt", "b.txt", "a.txt"]);
        assert_eq!(
            panel.read_with(cx, |panel, _| panel.browser().selected()),
            Some(2)
        );

        // A selection that no longer names a listed entry is dropped, not left
        // pointing at whatever now sits at that index.
        table.update(cx, |table, _| {
            table
                .delegate_mut()
                .set_entries(vec![dir_entry(&root, "x.txt", false)]);
        });
        table.update_in(cx, |table, window, cx| {
            table
                .delegate_mut()
                .perform_sort(0, ColumnSort::Ascending, window, cx);
        });
        assert_eq!(
            panel.read_with(cx, |panel, _| panel.browser().selected()),
            None
        );
    }
}
