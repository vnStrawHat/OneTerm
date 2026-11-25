//! [`SftpTableDelegate`] — data source + cell rendering for the SFTP DataTable.
//!
//! The file list is a `gpui_component::table::DataTable`: resizable, sortable
//! columns and virtual scroll. Column state (width + visibility) is persisted via
//! `persistence.rs` → `docks.json`; [`SftpPanel`] reads and writes it on the
//! background executor and hands the snapshot to this delegate.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    App, AppContext as _, Context, Div, InteractiveElement as _, IntoElement, ParentElement,
    Stateful, StatefulInteractiveElement as _, Styled, TextAlign, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, h_flex,
    menu::{ContextMenuExt as _, PopupMenu},
    table::{Column, ColumnFixed, ColumnSort, TableDelegate, TableState},
};
use oneterm_core::{FileEntry, SftpTableState};
use oneterm_theme::icon::AppIcon;

use super::panel::SftpPanel;
use super::table_delegate_menu::{MenuTarget, build_menu, menu_entries};
use super::types::{
    COLUMN_MAX_WIDTH, COLUMN_MIN_WIDTH, NAME_COLUMN_MIN_WIDTH, SftpColumnConfig, SortColumn,
    SortDir, format_date, format_owner, format_permissions, format_size, name_column_width,
    sort_dir_to_column_sort, sort_entries,
};

/// Indices into `col_configs` of the currently visible columns (display order).
type VisibleIndices = Vec<usize>;

/// The Name cell (folder/file icon + truncated name), shared by the remote and
/// local tables.
pub(crate) fn name_cell(name: &str, is_dir: bool, cx: &App) -> impl IntoElement {
    let icon = if is_dir {
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
                .text_color(cx.theme().foreground)
                .truncate()
                .child(name.to_string()),
        )
}

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
    /// Width of the list area, measured by the panel. `None` until the first
    /// frame has been laid out; Name then uses its configured fallback width.
    available_width: Option<f32>,
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
            available_width: None,
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

    /// The width the list area currently has. Returns whether it changed, so
    /// the caller can skip a refresh when the measurement is the same.
    pub(crate) fn set_available_width(&mut self, width: f32) -> bool {
        if self
            .available_width
            .is_some_and(|known| (known - width).abs() < 0.5)
        {
            return false;
        }
        self.available_width = Some(width);
        true
    }

    /// Width of the Name column: what the other visible columns leave over.
    fn name_width(&self) -> f32 {
        let Some(available) = self.available_width else {
            // Before the first measurement, the configured fallback width.
            return self
                .col_configs
                .iter()
                .find(|cfg| cfg.col == SortColumn::Name)
                .map_or(NAME_COLUMN_MIN_WIDTH, |cfg| cfg.width);
        };
        let others: f32 = self
            .visible_indices
            .iter()
            .filter_map(|&ix| self.col_configs.get(ix))
            .filter(|cfg| cfg.col != SortColumn::Name)
            .map(|cfg| cfg.width)
            .sum();
        name_column_width(available, others)
    }

    /// Apply the persisted column state (width + visibility) read from
    /// `docks.json`; the panel applies the dual-pane fields itself.
    /// Ignores invalid keys; Name is always visible and its width is derived,
    /// never restored.
    ///
    /// The state arrives already migrated: `docks.json`'s own schema version is
    /// what tells a pre-US-0124 layout apart, and `oneterm_state`'s v1 -> v2
    /// migration drops the widths and keeps only the columns the user hid by
    /// hand. This function sees a current-schema document and nothing else.
    pub(crate) fn apply_persisted_state(&mut self, state: &SftpTableState) {
        log::debug!(
            "SftpTableDelegate: apply persisted state — {} widths, {} visibility",
            state.column_widths.len(),
            state.column_visibility.len()
        );
        for cfg in &mut self.col_configs {
            if let Some(&w) = state.column_widths.get(cfg.col.key())
                && cfg.col != SortColumn::Name
            {
                if w >= COLUMN_MIN_WIDTH && w <= COLUMN_MAX_WIDTH {
                    cfg.width = w;
                }
            }
            if let Some(&visible) = state.column_visibility.get(cfg.col.key()) {
                // Name is always visible — ignore hidden for Name.
                cfg.visible = visible || cfg.col == SortColumn::Name;
            }
        }
        self.rebuild_visible_indices();
    }

    /// Read the current column config for persistence (the panel fills in the
    /// dual-pane fields).
    pub(crate) fn to_persisted_state(&self) -> SftpTableState {
        let mut column_widths = HashMap::new();
        let mut column_visibility = HashMap::new();
        for cfg in &self.col_configs {
            // Name's width follows the panel; storing it would only resurrect a
            // width that never fits some other window size.
            if cfg.col != SortColumn::Name {
                column_widths.insert(cfg.col.key().to_string(), cfg.width);
            }
            column_visibility.insert(cfg.col.key().to_string(), cfg.visible);
        }
        SftpTableState {
            column_widths,
            column_visibility,
            ..SftpTableState::default()
        }
    }

    /// Update widths for the visible columns from the DataTable's width list
    /// (in visible order). Used for `TableEvent::ColumnWidthsChanged`.
    pub(crate) fn apply_widths(&mut self, widths: &[gpui::Pixels]) {
        for (vis_ix, w) in widths.iter().enumerate() {
            if let Some(&cfg_ix) = self.visible_indices.get(vis_ix) {
                let cfg = &mut self.col_configs[cfg_ix];
                // Name is not resizable: its width is the panel's leftover.
                if cfg.col == SortColumn::Name {
                    continue;
                }
                cfg.width = w.as_f32().clamp(COLUMN_MIN_WIDTH, COLUMN_MAX_WIDTH);
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

        let is_name = cfg.col == SortColumn::Name;
        let mut col = Column::new(cfg.col.key(), cfg.label)
            .width(px(if is_name {
                self.name_width()
            } else {
                cfg.width
            }))
            .min_width(if is_name {
                NAME_COLUMN_MIN_WIDTH
            } else {
                COLUMN_MIN_WIDTH
            })
            .max_width(COLUMN_MAX_WIDTH)
            // Name fills whatever the other columns leave over, so there is
            // nothing for the user to drag it to.
            .resizable(!is_name)
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
        if is_name {
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
        let panel = self.panel.upgrade();
        let selected = panel.as_ref().and_then(|p| p.read(cx).browser().selected());
        let expanded = panel.is_some_and(|p| p.read(cx).expanded());
        let row = div().id(("row", row_ix));
        let row = if selected == Some(row_ix) {
            row.bg(cx.theme().tokens.table_hover)
        } else {
            row
        };
        // While the Local pane is shown a row can be dragged onto it (download).
        match self.entries.get(row_ix) {
            Some(entry) if expanded => {
                let drag = super::drag::RemoteRowDrag {
                    entry: entry.clone(),
                };
                row.on_drag(drag, |drag, _, _, cx| {
                    cx.new(|_| super::drag::DragPreview::new(&drag.entry.name))
                })
            }
            _ => row,
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
            SortColumn::Name => name_cell(&entry.name, entry.is_dir, cx).into_any_element(),
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

        let Some(entry) = self.entries.get(row_ix) else {
            return menu;
        };

        // The same list the toolbar's menu renders (F30), filtered by the same
        // rule for the same kind of entry (M2).
        let target = MenuTarget::for_selection(entry.is_dir);
        build_menu(menu, &self.panel, &menu_entries(target))
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
                build_menu(menu, &panel, &menu_entries(MenuTarget::EmptyArea))
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
            .map(|c| c.col.key())
            .collect()
    }

    fn width_of(delegate: &SftpTableDelegate, key: &str) -> f32 {
        delegate
            .col_configs
            .iter()
            .find(|c| c.col.key() == key)
            .map(|c| c.width)
            .unwrap()
    }

    #[gpui::test]
    fn persisted_state_round_trips_and_ignores_invalid_values(cx: &mut TestAppContext) {
        let mut delegate = delegate(cx);
        let mut state = SftpTableState::default();
        state.column_widths.insert("size".into(), 120.0);
        // Below the minimum width and an unknown key: both ignored.
        state.column_widths.insert("owner".into(), 1.0);
        state.column_widths.insert("bogus".into(), 50.0);
        state.column_visibility.insert("group".into(), true);
        // Name can never be hidden.
        state.column_visibility.insert("name".into(), false);

        delegate.apply_persisted_state(&state);

        assert_eq!(width_of(&delegate, "size"), 120.0);
        assert_eq!(width_of(&delegate, "owner"), 90.0);
        assert_eq!(
            visible_keys(&delegate),
            vec!["name", "size", "modified", "group"]
        );

        let persisted = delegate.to_persisted_state();
        assert_eq!(persisted.column_widths["size"], 120.0);
        assert!(persisted.column_visibility["group"]);
        assert!(persisted.column_visibility["name"]);
        assert!(!persisted.column_widths.contains_key("bogus"));
        // Name's width is derived from the panel, so it is not stored.
        assert!(!persisted.column_widths.contains_key("name"));
    }

    /// US-0124: what a pre-US-0124 document looks like after `oneterm-state`'s
    /// v1 -> v2 migration — no widths, and only the columns the user hid by
    /// hand. The delegate then shows the new defaults minus those hides.
    #[gpui::test]
    fn a_migrated_pre_version_state_gives_the_new_defaults(cx: &mut TestAppContext) {
        let mut delegate = delegate(cx);
        let mut state = SftpTableState::default();
        state.column_visibility.insert("modified".into(), false);

        delegate.apply_persisted_state(&state);

        assert_eq!(visible_keys(&delegate), vec!["name", "size"]);
        assert_eq!(width_of(&delegate, "size"), 72.0);
        assert_eq!(width_of(&delegate, "modified"), 116.0);
    }

    /// Name takes what the other visible columns leave over, so the table fits
    /// the panel it is drawn in instead of scrolling sideways.
    #[gpui::test]
    fn name_fills_the_measured_panel_width(cx: &mut TestAppContext) {
        let mut delegate = delegate(cx);
        assert!(delegate.set_available_width(317.0));
        assert!(
            !delegate.set_available_width(317.2),
            "same width, no refresh"
        );

        let others = 72.0 + 116.0;
        assert_eq!(delegate.name_width(), 317.0 - others - 16.0);
        delegate.set_available_width(1900.0);
        assert_eq!(delegate.name_width(), 1900.0 - others - 16.0);

        // A hidden column stops taking room from Name.
        delegate.toggle_visibility(SortColumn::Modified);
        assert_eq!(delegate.name_width(), 1900.0 - 72.0 - 16.0);
    }

    /// M1: widening a column has to come out of Name on the same gesture, or
    /// the table ends the drag wider than the panel it is drawn in.
    #[gpui::test]
    fn a_widened_column_comes_out_of_name(cx: &mut TestAppContext) {
        let mut delegate = delegate(cx);
        delegate.set_available_width(317.0);
        let before = delegate.name_width();

        // The kit reports the visible columns' widths on mouse-up: Size + 88.
        delegate.apply_widths(&[px(before), px(160.), px(116.)]);

        assert_eq!(width_of(&delegate, "size"), 160.0);
        assert_eq!(delegate.name_width(), NAME_COLUMN_MIN_WIDTH);
        assert!(
            delegate.name_width() + 160.0 + 116.0 + 16.0 >= 317.0,
            "a drag wider than the panel is allowed to scroll, but only from the floor"
        );

        // A narrower drag gives the room straight back to Name.
        delegate.apply_widths(&[px(delegate.name_width()), px(50.), px(116.)]);
        assert_eq!(delegate.name_width(), 317.0 - 50.0 - 116.0 - 16.0);
        assert!(delegate.name_width() > before);
    }

    #[gpui::test]
    fn toggling_visibility_never_hides_name(cx: &mut TestAppContext) {
        let mut delegate = delegate(cx);
        assert!(!delegate.toggle_visibility(SortColumn::Name));
        assert!(!visible_keys(&delegate).contains(&"owner"));
        assert!(delegate.toggle_visibility(SortColumn::Owner));
        assert!(visible_keys(&delegate).contains(&"owner"));
        assert!(delegate.toggle_visibility(SortColumn::Owner));
        assert!(!visible_keys(&delegate).contains(&"owner"));
    }

    #[gpui::test]
    fn widths_apply_in_visible_order_and_are_clamped(cx: &mut TestAppContext) {
        let mut delegate = delegate(cx);
        delegate.toggle_visibility(SortColumn::Permissions);
        // Visible order is now: name, size, modified, permissions.
        delegate.apply_widths(&[px(500.), px(10.), px(9999.), px(120.)]);
        assert_eq!(width_of(&delegate, "size"), 40.0);
        assert_eq!(width_of(&delegate, "modified"), 800.0);
        assert_eq!(width_of(&delegate, "permissions"), 120.0);
        // Name is not resizable — a width for it is ignored, not stored.
        assert_eq!(width_of(&delegate, "name"), NAME_COLUMN_MIN_WIDTH);
        // The hidden column keeps its default width.
        assert_eq!(width_of(&delegate, "owner"), 90.0);
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
