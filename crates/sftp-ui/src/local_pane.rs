//! [`LocalPane`] — the Local (this machine) side of the expanded SFTP browser.
//!
//! A plain `std::fs` directory browser with its own `DataTable`: path box, back,
//! refresh, sortable Name / Date Modified / Size columns, and New Folder /
//! Rename / Delete. Listing and every mutation run on the background executor.
//! Files move to the remote side through [`SftpPanel::do_upload_paths`] and
//! arrive from it through [`SftpPanel::download_entry_to_local`]
//! (see `docs/spec-intakes/IN-0025-sftp-dual-pane/`).

use std::cmp::Reverse;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement,
    Render, StatefulInteractiveElement as _, Styled, Subscription, TextAlign, WeakEntity, Window,
    div,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogButtonProps, DialogFooter},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem},
    notification::NotificationType,
    table::{Column, ColumnSort, DataTable, TableDelegate, TableEvent, TableState},
    v_flex,
};
use oneterm_state::form_dialog::{FieldRequirement, FormDialog, labelled_field};
use oneterm_theme::icon::AppIcon;
use oneterm_theme::notif_ext::notify;

use super::actions::{delete_confirmation, validate_entry_name};
use super::drag::{DragPreview, LocalRowDrag, RemoteRowDrag};
use super::panel::SftpPanel;
use super::table_delegate::name_cell;
use super::table_delegate_menu::on_click_entity;
use super::types::{
    COLUMN_MAX_WIDTH, COLUMN_MIN_WIDTH, SortDir, format_date, format_size, sort_dir_to_column_sort,
};

// ── Entries ──────────────────────────────────────────────────

/// One row of the local listing.
#[derive(Clone, Debug)]
pub(crate) struct LocalEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// Columns of the local table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum LocalColumn {
    Name,
    Modified,
    Size,
}

impl LocalColumn {
    const ALL: [LocalColumn; 3] = [LocalColumn::Name, LocalColumn::Modified, LocalColumn::Size];

    fn key(self) -> &'static str {
        match self {
            LocalColumn::Name => "name",
            LocalColumn::Modified => "modified",
            LocalColumn::Size => "size",
        }
    }

    fn label(self) -> &'static str {
        match self {
            LocalColumn::Name => "Name",
            LocalColumn::Modified => "Date Modified",
            LocalColumn::Size => "Size",
        }
    }

    fn default_width(self) -> f32 {
        match self {
            LocalColumn::Name => 260.0,
            LocalColumn::Modified => 140.0,
            LocalColumn::Size => 80.0,
        }
    }
}

/// Folders first; within each group by the selected column (Name asc default,
/// case-insensitive) — the same rule as the remote listing.
pub(crate) fn sort_local_entries(entries: &mut [LocalEntry], sort: Option<(LocalColumn, SortDir)>) {
    let (col, dir) = sort.unwrap_or((LocalColumn::Name, SortDir::Asc));
    match (col, dir) {
        (LocalColumn::Name, SortDir::Asc) => {
            entries.sort_by_cached_key(|e| (!e.is_dir, e.name.to_lowercase()))
        }
        (LocalColumn::Name, SortDir::Desc) => {
            entries.sort_by_cached_key(|e| (!e.is_dir, Reverse(e.name.to_lowercase())))
        }
        (LocalColumn::Modified, SortDir::Asc) => entries.sort_by_key(|e| (!e.is_dir, e.modified)),
        (LocalColumn::Modified, SortDir::Desc) => {
            entries.sort_by_key(|e| (!e.is_dir, Reverse(e.modified)))
        }
        (LocalColumn::Size, SortDir::Asc) => entries.sort_by_key(|e| (!e.is_dir, e.size)),
        (LocalColumn::Size, SortDir::Desc) => entries.sort_by_key(|e| (!e.is_dir, Reverse(e.size))),
    }
}

/// List `dir` (blocking — background executor only). The path is made absolute
/// so the pane's cwd is usable as a transfer target; an entry whose metadata
/// cannot be read is still listed from its directory-entry file type.
pub(crate) fn read_local_dir(dir: &Path) -> io::Result<(PathBuf, Vec<LocalEntry>)> {
    let dir = std::path::absolute(dir)?;
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        // `metadata` follows symlinks so a linked folder navigates like a folder.
        let (is_dir, size, modified) = match std::fs::metadata(&path) {
            Ok(meta) => (meta.is_dir(), meta.len(), meta.modified().ok()),
            Err(_) => (
                entry.file_type().map(|t| t.is_dir()).unwrap_or(false),
                0,
                None,
            ),
        };
        entries.push(LocalEntry {
            name,
            path,
            is_dir,
            size,
            modified,
        });
    }
    Ok((dir, entries))
}

/// The directory the pane starts in: the persisted one, else home, else `.`.
pub(crate) fn initial_local_dir(persisted: Option<PathBuf>) -> PathBuf {
    persisted
        .filter(|dir| !dir.as_os_str().is_empty())
        .or_else(oneterm_core::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

// ── Table delegate ───────────────────────────────────────────

pub(crate) struct LocalTableDelegate {
    entries: Vec<LocalEntry>,
    widths: [f32; 3],
    sort: Option<(LocalColumn, SortDir)>,
    pub(crate) loading: bool,
    pane: WeakEntity<LocalPane>,
}

impl LocalTableDelegate {
    fn new(pane: WeakEntity<LocalPane>) -> Self {
        Self {
            entries: Vec::new(),
            widths: [
                LocalColumn::Name.default_width(),
                LocalColumn::Modified.default_width(),
                LocalColumn::Size.default_width(),
            ],
            sort: None,
            loading: false,
            pane,
        }
    }

    pub(crate) fn entries(&self) -> &[LocalEntry] {
        &self.entries
    }

    pub(crate) fn set_entries(&mut self, mut entries: Vec<LocalEntry>) {
        sort_local_entries(&mut entries, self.sort);
        self.entries = entries;
    }
}

impl TableDelegate for LocalTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        LocalColumn::ALL.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.entries.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let col = LocalColumn::ALL[col_ix];
        let mut column = Column::new(col.key(), col.label())
            .width(self.widths[col_ix])
            .min_width(COLUMN_MIN_WIDTH)
            .max_width(COLUMN_MAX_WIDTH)
            .resizable(true)
            .movable(false)
            .sortable();
        if col == LocalColumn::Size {
            column = column.text_right();
        }
        column.sort(match self.sort {
            Some((sort_col, dir)) if sort_col == col => sort_dir_to_column_sort(dir),
            _ => ColumnSort::Default,
        })
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div()
            .size_full()
            .items_center()
            .text_color(cx.theme().foreground)
            .child(LocalColumn::ALL[col_ix].label())
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> gpui::Stateful<gpui::Div> {
        let selected = self.pane.upgrade().and_then(|pane| pane.read(cx).selected);
        let row = div().id(("local-row", row_ix));
        let row = if selected == Some(row_ix) {
            row.bg(cx.theme().tokens.table_hover)
        } else {
            row
        };
        match self.entries.get(row_ix) {
            Some(entry) => {
                let drag = LocalRowDrag {
                    path: entry.path.clone(),
                    name: entry.name.clone(),
                };
                row.on_drag(drag, |drag, _, _, cx| {
                    cx.new(|_| DragPreview::new(&drag.name))
                })
            }
            None => row,
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
        let muted = cx.theme().muted_foreground;
        match LocalColumn::ALL[col_ix] {
            LocalColumn::Name => name_cell(&entry.name, entry.is_dir, cx).into_any_element(),
            LocalColumn::Modified => div()
                .text_xs()
                .text_color(muted)
                .child(format_date(entry.modified))
                .into_any_element(),
            LocalColumn::Size => div()
                .text_xs()
                .text_color(muted)
                .text_align(TextAlign::Right)
                .child(if entry.is_dir {
                    String::new()
                } else {
                    format_size(entry.size)
                })
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
        let col = LocalColumn::ALL[col_ix];
        self.sort = match sort {
            ColumnSort::Default => None,
            ColumnSort::Ascending => Some((col, SortDir::Asc)),
            ColumnSort::Descending => Some((col, SortDir::Desc)),
        };
        let Some(pane) = self.pane.upgrade() else {
            sort_local_entries(&mut self.entries, self.sort);
            return;
        };
        // The selection names an entry, not a row: keep it on the same file.
        let selected_path = pane
            .read(cx)
            .selected
            .and_then(|ix| self.entries.get(ix))
            .map(|entry| entry.path.clone());
        sort_local_entries(&mut self.entries, self.sort);
        let remapped =
            selected_path.and_then(|path| self.entries.iter().position(|e| e.path == path));
        pane.update(cx, |pane, cx| {
            pane.selected = remapped;
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
        let Some(pane) = self.pane.upgrade() else {
            return menu;
        };
        pane.update(cx, |pane, cx| {
            pane.selected = Some(row_ix);
            cx.notify();
        });
        let Some(entry) = self.entries.get(row_ix) else {
            return menu;
        };
        let weak = self.pane.clone();
        let first = if entry.is_dir {
            PopupMenuItem::new("Open")
                .on_click(on_click_entity(weak.clone(), move |this, _, cx| {
                    this.navigate_into(row_ix, cx)
                }))
        } else {
            PopupMenuItem::new("Upload")
                .icon(Icon::new(IconName::ArrowRight))
                .on_click(on_click_entity(weak.clone(), LocalPane::upload_selected))
        };
        let menu = menu.item(first);
        let menu = if entry.is_dir {
            menu.item(
                PopupMenuItem::new("Upload")
                    .icon(Icon::new(IconName::ArrowRight))
                    .on_click(on_click_entity(weak.clone(), LocalPane::upload_selected)),
            )
        } else {
            menu
        };
        menu.separator()
            .item(
                PopupMenuItem::new("Rename")
                    .icon(Icon::new(IconName::Replace))
                    .on_click(on_click_entity(weak.clone(), LocalPane::do_rename)),
            )
            .item(
                PopupMenuItem::new("Delete")
                    .icon(Icon::new(IconName::Delete))
                    .on_click(on_click_entity(weak.clone(), LocalPane::do_delete)),
            )
            .separator()
            .item(
                PopupMenuItem::new("New Folder")
                    .icon(Icon::new(IconName::Plus))
                    .on_click(on_click_entity(weak.clone(), LocalPane::do_new_folder)),
            )
            .item(
                PopupMenuItem::new("Refresh")
                    .icon(Icon::new(AppIcon::Refresh))
                    .on_click(on_click_entity(weak, |this, _, cx| this.refresh(cx))),
            )
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let weak = self.pane.clone();
        div()
            .id("local-empty-area")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("Empty directory.")
            .context_menu(move |menu, _, _| {
                menu.item(
                    PopupMenuItem::new("New Folder")
                        .icon(Icon::new(IconName::Plus))
                        .on_click(on_click_entity(weak.clone(), LocalPane::do_new_folder)),
                )
                .item(
                    PopupMenuItem::new("Refresh")
                        .icon(Icon::new(AppIcon::Refresh))
                        .on_click(on_click_entity(weak.clone(), |this, _, cx| {
                            this.refresh(cx)
                        })),
                )
            })
    }
}

// ── Pane ─────────────────────────────────────────────────────

/// The Local pane: current directory, listing table, path box, selection.
pub(crate) struct LocalPane {
    /// Absolute directory shown; empty until the first listing completes.
    cwd: PathBuf,
    /// Directory to load on the first `ensure_loaded`.
    initial_dir: PathBuf,
    loaded: bool,
    selected: Option<usize>,
    error: Option<String>,
    path_error: bool,
    /// Bumped per listing request; a result applies only when it still matches.
    generation: u64,
    table: Entity<TableState<LocalTableDelegate>>,
    path_input: Entity<InputState>,
    panel: WeakEntity<SftpPanel>,
    _subscriptions: Vec<Subscription>,
}

impl LocalPane {
    pub(crate) fn new(
        panel: WeakEntity<SftpPanel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let delegate = LocalTableDelegate::new(cx.entity().downgrade());
        let table = cx.new(|cx| {
            TableState::new(delegate, window, cx)
                .col_movable(false)
                .col_resizable(true)
                .sortable(true)
                .col_selectable(false)
                .row_selectable(true)
        });
        let path_input = cx.new(|cx| InputState::new(window, cx).placeholder("Local path"));
        let subscriptions = vec![
            cx.subscribe_in(&table, window, Self::on_table_event),
            cx.subscribe_in(&path_input, window, Self::on_path_input_event),
        ];
        Self {
            cwd: PathBuf::new(),
            initial_dir: initial_local_dir(None),
            loaded: false,
            selected: None,
            error: None,
            path_error: false,
            generation: 0,
            table,
            path_input,
            panel,
            _subscriptions: subscriptions,
        }
    }

    /// The directory shown (empty before the first listing).
    pub(crate) fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// The directory to persist: the shown one, else the pending initial one.
    pub(crate) fn dir_for_persistence(&self) -> Option<PathBuf> {
        if self.cwd.as_os_str().is_empty() {
            None
        } else {
            Some(self.cwd.clone())
        }
    }

    #[cfg(test)]
    pub(crate) fn table(&self) -> &Entity<TableState<LocalTableDelegate>> {
        &self.table
    }

    #[cfg(test)]
    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn select(&mut self, index: Option<usize>) {
        self.selected = index;
    }

    /// Where the pane starts (persisted `local_dir`). Applied by the next
    /// `ensure_loaded`; when a listing already happened before the persisted
    /// state arrived (a zoom restored at startup expands the pane first), the
    /// persisted directory replaces the default one right away.
    pub(crate) fn set_initial_dir(&mut self, dir: Option<PathBuf>, cx: &mut Context<Self>) {
        let persisted = dir.is_some();
        self.initial_dir = initial_local_dir(dir);
        if self.loaded && persisted && self.cwd != self.initial_dir {
            let dir = self.initial_dir.clone();
            self.load_dir(dir, cx);
        }
    }

    /// Load the initial directory once (called when the pane becomes visible).
    pub(crate) fn ensure_loaded(&mut self, cx: &mut Context<Self>) {
        if !self.loaded {
            self.loaded = true;
            let dir = self.initial_dir.clone();
            self.load_dir(dir, cx);
        }
    }

    pub(crate) fn selected_entry(&self, cx: &App) -> Option<LocalEntry> {
        self.selected
            .and_then(|ix| self.table.read(cx).delegate().entries().get(ix).cloned())
    }

    /// List `path` on the background executor and show it when it is still the
    /// newest request. A failure keeps the previous rows under an error banner.
    pub(crate) fn load_dir(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        log::debug!("LocalPane::load_dir: {}", path.display());
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.loaded = true;
        self.cwd = path.clone();
        self.error = None;
        self.selected = None;
        self.table.update(cx, |table, cx| {
            table.delegate_mut().loading = true;
            table.clear_selection(cx);
            cx.notify();
        });
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { read_local_dir(&path) })
                .await;
            // The pane may be gone before the listing arrives; nothing to apply then.
            _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok((dir, entries)) => {
                        this.cwd = dir;
                        this.error = None;
                        this.table.update(cx, |table, cx| {
                            table.delegate_mut().loading = false;
                            table.delegate_mut().set_entries(entries);
                            table.refresh(cx);
                        });
                        // Persist the new local directory (debounced by the panel).
                        _ = this
                            .panel
                            .update(cx, |panel, cx| panel.schedule_save_table_state(cx));
                    }
                    Err(error) => {
                        log::warn!("LocalPane::load_dir failed: {error}");
                        this.error = Some(error.to_string());
                        this.table.update(cx, |table, cx| {
                            table.delegate_mut().loading = false;
                            table.refresh(cx);
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.cwd.as_os_str().is_empty() {
            self.ensure_loaded(cx);
        } else {
            self.load_dir(self.cwd.clone(), cx);
        }
    }

    pub(crate) fn navigate_parent(&mut self, cx: &mut Context<Self>) {
        if let Some(parent) = self.cwd.parent().map(Path::to_path_buf) {
            self.load_dir(parent, cx);
        }
    }

    pub(crate) fn navigate_into(&mut self, index: usize, cx: &mut Context<Self>) {
        let entry = self.table.read(cx).delegate().entries().get(index).cloned();
        if let Some(entry) = entry.filter(|entry| entry.is_dir) {
            self.load_dir(entry.path, cx);
        }
    }

    /// Enter in the path box: open the directory, or flag the box when the
    /// path is not a directory (checked off the UI thread).
    fn goto_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let probe = path.clone();
            let is_dir = cx
                .background_executor()
                .spawn(async move { probe.is_dir() })
                .await;
            _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                if is_dir {
                    this.path_error = false;
                    this.load_dir(path, cx);
                } else {
                    this.path_error = true;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    // ── Events ───────────────────────────────────────────────

    fn on_table_event(
        &mut self,
        _: &Entity<TableState<LocalTableDelegate>>,
        event: &TableEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TableEvent::SelectRow(ix) => {
                self.selected = Some(*ix);
                cx.notify();
            }
            TableEvent::ClearSelection => {
                self.selected = None;
                cx.notify();
            }
            TableEvent::DoubleClickedRow(ix) => {
                let entry = self.table.read(cx).delegate().entries().get(*ix).cloned();
                match entry {
                    Some(entry) if entry.is_dir => self.load_dir(entry.path, cx),
                    Some(_) => {
                        self.selected = Some(*ix);
                        self.upload_selected(window, cx);
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    fn on_path_input_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::PressEnter { .. } => {
                let text = self.path_input.read(cx).value().trim().to_string();
                if !text.is_empty() {
                    self.goto_path(PathBuf::from(text), cx);
                }
            }
            InputEvent::Change => {
                if self.path_error {
                    self.path_error = false;
                    cx.notify();
                }
            }
            _ => {}
        }
    }

    // ── Transfers ────────────────────────────────────────────

    /// Upload the selected entry into the remote cwd.
    pub(crate) fn upload_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry(cx) else {
            window.push_notification(
                notify(
                    NotificationType::Warning,
                    "Select a local file or folder to upload.",
                    cx,
                ),
                cx,
            );
            return;
        };
        _ = self.panel.update(cx, |panel, cx| {
            panel.do_upload_paths(vec![entry.path], cx);
        });
    }

    // ── Local mutations (see the IN-0025 LLD) ────────────────

    pub(crate) fn do_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cwd.as_os_str().is_empty() {
            return;
        }
        let pane = cx.entity();
        let cwd = self.cwd.clone();
        let name_state = cx.new(|cx| InputState::new(window, cx).placeholder("Folder name"));
        let submit = {
            let name_state = name_state.clone();
            move |window: &mut Window, cx: &mut App| {
                let name = match validate_entry_name(&name_state.read(cx).value()) {
                    Ok(name) => name.to_string(),
                    Err(error) => {
                        window.push_notification(
                            notify(NotificationType::Warning, error.to_string(), cx),
                            cx,
                        );
                        return false;
                    }
                };
                let path = cwd.join(&name);
                run_local_mutation(
                    pane.clone(),
                    format!("Folder \"{name}\" created."),
                    "Create folder failed",
                    // `create_dir`, not `create_dir_all`: an existing name is an error.
                    move || std::fs::create_dir(&path),
                    window,
                    cx,
                );
                false
            }
        };
        FormDialog::new(
            "New Local Folder",
            move |content, _window, cx| {
                content.child(labelled_field(
                    "Folder name",
                    FieldRequirement::Required,
                    Input::new(&name_state),
                    cx,
                ))
            },
            submit,
        )
        .confirm_label("Create")
        .open(window, cx);
    }

    pub(crate) fn do_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry(cx) else {
            window.push_notification(
                notify(
                    NotificationType::Warning,
                    "Select a local file or folder to rename.",
                    cx,
                ),
                cx,
            );
            return;
        };
        let pane = cx.entity();
        let cwd = self.cwd.clone();
        let name_state = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("New name");
            state.set_value(&entry.name, window, cx);
            state
        });
        let submit = {
            let name_state = name_state.clone();
            move |window: &mut Window, cx: &mut App| {
                let new_name = match validate_entry_name(&name_state.read(cx).value()) {
                    Ok(name) => name.to_string(),
                    Err(error) => {
                        window.push_notification(
                            notify(NotificationType::Warning, error.to_string(), cx),
                            cx,
                        );
                        return false;
                    }
                };
                let from = entry.path.clone();
                let to = cwd.join(&new_name);
                run_local_mutation(
                    pane.clone(),
                    format!("Renamed to \"{new_name}\"."),
                    "Rename failed",
                    move || {
                        // Never replace a sibling silently.
                        if to.exists() {
                            return Err(io::Error::new(
                                io::ErrorKind::AlreadyExists,
                                "an entry with that name already exists",
                            ));
                        }
                        std::fs::rename(&from, &to)
                    },
                    window,
                    cx,
                );
                false
            }
        };
        FormDialog::new(
            "Rename",
            move |content, _window, cx| {
                content.child(labelled_field(
                    "New name",
                    FieldRequirement::Required,
                    Input::new(&name_state),
                    cx,
                ))
            },
            submit,
        )
        .confirm_label("Rename")
        .open(window, cx);
    }

    pub(crate) fn do_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry(cx) else {
            window.push_notification(
                notify(
                    NotificationType::Warning,
                    "Select a local file or folder to delete.",
                    cx,
                ),
                cx,
            );
            return;
        };
        let pane = cx.entity();
        let description = delete_confirmation(&entry.name, entry.is_dir);
        window.open_alert_dialog(cx, move |alert, _window, _cx| {
            let entry = entry.clone();
            let pane = pane.clone();
            alert
                .confirm()
                .title("Confirm Delete")
                .description(description.clone())
                .footer(
                    DialogFooter::new()
                        .child(Button::new("cancel").label("Cancel").outline().on_click(
                            |_, window, cx| {
                                window.close_dialog(cx);
                            },
                        ))
                        .child(Button::new("delete").label("Delete").danger().on_click(
                            move |_, window, cx| {
                                let path = entry.path.clone();
                                let is_dir = entry.is_dir;
                                run_local_mutation(
                                    pane.clone(),
                                    "Deleted successfully.".to_string(),
                                    "Delete failed",
                                    move || {
                                        if is_dir {
                                            std::fs::remove_dir_all(&path)
                                        } else {
                                            std::fs::remove_file(&path)
                                        }
                                    },
                                    window,
                                    cx,
                                );
                            },
                        )),
                )
                .button_props(
                    DialogButtonProps::default()
                        .on_cancel(|_, _, _| true)
                        .on_ok(|_, _, _| false),
                )
        });
    }

    // ── Render ───────────────────────────────────────────────

    fn render_toolbar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let path_focused = self.path_input.read(cx).focus_handle(cx).is_focused(window);
        let show_custom_border = self.path_error || !path_focused;
        let path_border = if self.path_error {
            theme.danger
        } else {
            theme.border
        };
        let weak = cx.entity().downgrade();
        let more_btn = Button::new("local-more")
            .icon(Icon::new(IconName::EllipsisVertical).small())
            .small()
            .ghost()
            .dropdown_menu(move |menu, _window, _cx| {
                menu.item(
                    PopupMenuItem::new("New Folder")
                        .icon(Icon::new(IconName::Plus))
                        .on_click(on_click_entity(weak.clone(), LocalPane::do_new_folder)),
                )
                .item(
                    PopupMenuItem::new("Upload")
                        .icon(Icon::new(IconName::ArrowRight))
                        .on_click(on_click_entity(weak.clone(), LocalPane::upload_selected)),
                )
                .item(
                    PopupMenuItem::new("Rename")
                        .icon(Icon::new(IconName::Replace))
                        .on_click(on_click_entity(weak.clone(), LocalPane::do_rename)),
                )
                .item(
                    PopupMenuItem::new("Delete")
                        .icon(Icon::new(IconName::Delete))
                        .on_click(on_click_entity(weak.clone(), LocalPane::do_delete)),
                )
                .separator()
                .item(
                    PopupMenuItem::new("Refresh")
                        .icon(Icon::new(AppIcon::Refresh))
                        .on_click(on_click_entity(weak.clone(), |this, _, cx| {
                            this.refresh(cx)
                        })),
                )
            });

        h_flex()
            .w_full()
            .h_8()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .px_2()
            .py_5()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Local"),
            )
            .child(
                Input::new(&self.path_input)
                    .flex_1()
                    .border_b_1()
                    .border_t_0()
                    .border_l_0()
                    .border_r_0()
                    .when(show_custom_border, |input| input.border_color(path_border))
                    .small()
                    .bg(gpui::transparent_black()),
            )
            .child(
                Button::new("local-back")
                    .icon(Icon::new(IconName::ArrowLeft).small())
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.navigate_parent(cx))),
            )
            .child(
                Button::new("local-upload")
                    .icon(Icon::new(IconName::ArrowRight).small())
                    .small()
                    .ghost()
                    .tooltip("Upload the selected entry to the remote directory")
                    .on_click(cx.listener(|this, _, window, cx| this.upload_selected(window, cx))),
            )
            .child(
                Button::new("local-refresh")
                    .icon(Icon::new(AppIcon::Refresh).small())
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            )
            .child(more_btn)
    }

    fn render_file_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let error_banner = self.error.as_ref().map(|error| {
            div()
                .id("local-listing-error")
                .w_full()
                .flex_shrink_0()
                .px_2()
                .py_1()
                .text_xs()
                .bg(theme.danger.opacity(0.12))
                .text_color(theme.danger)
                .child(format!(
                    "Could not open \"{}\": {error}",
                    self.cwd.display()
                ))
        });
        let panel = self.panel.clone();
        v_flex()
            .id("local-file-list")
            .flex_1()
            .min_h_0()
            .children(error_banner)
            .child(
                DataTable::new(&self.table)
                    .bordered(false)
                    .scrollbar_visible(true, true)
                    .small(),
            )
            // A remote row dropped here downloads into this directory.
            .can_drop(|drag, _, _| drag.is::<RemoteRowDrag>())
            .on_drop(move |drag: &RemoteRowDrag, window, cx| {
                let entry = drag.entry.clone();
                _ = panel.update(cx, |panel, cx| {
                    panel.download_entry_to_local(entry, window, cx);
                });
            })
    }
}

impl Render for LocalPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Mirror cwd into the path box unless the user is typing in it.
        let cwd_display = self.cwd.display().to_string();
        let path_input = self.path_input.clone();
        let path_focused = path_input.read(cx).focus_handle(cx).is_focused(window);
        if !path_focused && path_input.read(cx).value().as_ref() != cwd_display {
            path_input.update(cx, |state, cx| state.set_value(cwd_display, window, cx));
        }
        v_flex()
            .id("sftp-local-pane")
            .size_full()
            .child(self.render_toolbar(window, cx))
            .child(self.render_file_list(cx))
    }
}

/// Run one blocking local mutation on the background executor: on success
/// notify, refresh the pane and close the dialog; on failure notify and keep
/// the dialog open.
fn run_local_mutation(
    pane: Entity<LocalPane>,
    success_message: String,
    failure_prefix: &'static str,
    op: impl FnOnce() -> io::Result<()> + Send + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    window
        .spawn(cx, async move |cx| {
            let result = cx.background_executor().spawn(async move { op() }).await;
            // The dialog may close before the background result arrives.
            _ = cx.update(|window, cx| match result {
                Ok(()) => {
                    window.push_notification(
                        notify(NotificationType::Success, success_message, cx),
                        cx,
                    );
                    pane.update(cx, |pane, cx| pane.refresh(cx));
                    window.close_dialog(cx);
                }
                Err(error) => {
                    log::error!("LocalPane: {failure_prefix}: {error}");
                    window.push_notification(
                        notify(
                            NotificationType::Error,
                            format!("{failure_prefix}: {error}"),
                            cx,
                        ),
                        cx,
                    );
                }
            });
        })
        .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_backend::TempDir;

    fn entry(name: &str, is_dir: bool, size: u64, mtime: u64) -> LocalEntry {
        LocalEntry {
            name: name.to_string(),
            path: PathBuf::from(name),
            is_dir,
            size,
            modified: Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(mtime)),
        }
    }

    fn names(entries: &[LocalEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn local_sort_is_folders_first_then_by_column() {
        let mut entries = vec![
            entry("zeta.txt", false, 5, 1),
            entry("Beta", true, 0, 3),
            entry("alpha.txt", false, 50, 2),
            entry("alpha", true, 0, 4),
        ];
        sort_local_entries(&mut entries, None);
        assert_eq!(
            names(&entries),
            vec!["alpha", "Beta", "alpha.txt", "zeta.txt"]
        );
        sort_local_entries(&mut entries, Some((LocalColumn::Size, SortDir::Desc)));
        assert_eq!(
            names(&entries),
            vec!["alpha", "Beta", "alpha.txt", "zeta.txt"]
        );
        sort_local_entries(&mut entries, Some((LocalColumn::Modified, SortDir::Desc)));
        assert_eq!(
            names(&entries),
            vec!["alpha", "Beta", "alpha.txt", "zeta.txt"]
        );
        sort_local_entries(&mut entries, Some((LocalColumn::Name, SortDir::Desc)));
        assert_eq!(
            names(&entries),
            vec!["Beta", "alpha", "zeta.txt", "alpha.txt"]
        );
    }

    #[test]
    fn read_local_dir_lists_files_and_folders_with_an_absolute_cwd() {
        let temp = TempDir::new();
        std::fs::create_dir(temp.0.join("sub")).unwrap();
        std::fs::write(temp.0.join("a.txt"), b"hello").unwrap();

        let (dir, mut entries) = read_local_dir(&temp.0).unwrap();
        assert!(dir.is_absolute());
        sort_local_entries(&mut entries, None);
        assert_eq!(names(&entries), vec!["sub", "a.txt"]);
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].size, 5);
        assert!(entries[1].modified.is_some());
        assert_eq!(entries[1].path, temp.0.join("a.txt"));

        assert!(read_local_dir(&temp.0.join("missing")).is_err());
    }

    fn test_pane(
        cx: &mut gpui::TestAppContext,
    ) -> (Entity<LocalPane>, &mut gpui::VisualTestContext) {
        cx.update(gpui_component::init);
        cx.update(oneterm_state::AppState::init);
        cx.update(crate::browser_state::SftpBrowserStore::init);
        let (panel, cx) = cx.add_window_view(|window, cx| SftpPanel::new(window, cx));
        let pane = panel.read_with(cx, |panel, _| panel.local().clone());
        (pane, cx)
    }

    fn listed(pane: &Entity<LocalPane>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
        pane.read_with(cx, |pane, cx| {
            pane.table()
                .read(cx)
                .delegate()
                .entries()
                .iter()
                .map(|e| e.name.clone())
                .collect()
        })
    }

    /// The pane lists a directory off the UI thread; a failed listing keeps the
    /// previous rows under an error, and the back button walks up.
    #[gpui::test]
    fn pane_lists_navigates_and_keeps_rows_on_a_failed_listing(cx: &mut gpui::TestAppContext) {
        let temp = TempDir::new();
        std::fs::create_dir(temp.0.join("sub")).unwrap();
        std::fs::write(temp.0.join("b.txt"), b"x").unwrap();
        let (pane, cx) = test_pane(cx);

        pane.update(cx, |pane, cx| pane.load_dir(temp.0.clone(), cx));
        cx.run_until_parked();
        assert_eq!(listed(&pane, cx), vec!["sub", "b.txt"]);
        assert!(pane.read_with(cx, |pane, _| pane.error().is_none()));
        assert_eq!(
            pane.read_with(cx, |pane, _| pane.dir_for_persistence()),
            Some(std::path::absolute(&temp.0).unwrap())
        );

        pane.update(cx, |pane, cx| pane.navigate_into(0, cx));
        cx.run_until_parked();
        assert!(listed(&pane, cx).is_empty());
        assert!(pane.read_with(cx, |pane, _| pane.cwd().ends_with("sub")));

        pane.update(cx, |pane, cx| pane.navigate_parent(cx));
        cx.run_until_parked();
        assert_eq!(listed(&pane, cx), vec!["sub", "b.txt"]);

        pane.update(cx, |pane, cx| pane.load_dir(temp.0.join("missing"), cx));
        cx.run_until_parked();
        assert_eq!(listed(&pane, cx), vec!["sub", "b.txt"]);
        assert!(pane.read_with(cx, |pane, _| pane.error().is_some()));
    }

    #[test]
    fn initial_dir_prefers_the_persisted_value_then_home() {
        assert_eq!(
            initial_local_dir(Some(PathBuf::from("C:/work"))),
            PathBuf::from("C:/work")
        );
        let fallback = initial_local_dir(Some(PathBuf::new()));
        assert_eq!(fallback, initial_local_dir(None));
        assert!(!fallback.as_os_str().is_empty());
    }
}
