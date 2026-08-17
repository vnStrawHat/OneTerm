//! `impl TerminalSession for SshSession` — render, input, mouse/selection,
//! clipboard, scroll, IME, and lifecycle query methods.
//!
//! ARCH-05: Terminal-model operations are delegated to the shared
//! `TerminalModel` adapter in `oneterm_terminal`. Only transport (SSH channel),
//! lifecycle, and state remain on `SshSession`.

use std::path::PathBuf;

use alacritty_terminal::selection::SelectionType;
use async_channel::Receiver;

use oneterm_core::SftpBackend;
use oneterm_terminal::model::TerminalModel;
use oneterm_terminal::mouse_encode::{MouseModifiers, TerminalMouseButton};
use oneterm_terminal::{
    CursorBounds, DefaultColors, PtyTransport, SearchMatch, SearchOptions, SessionEvent,
    SharedStateCwdSource, TerminalCapabilities, TerminalError, TerminalSession,
    report_generated_input,
};
use oneterm_terminal::{DynamicColors, TerminalContent, TerminalInfo, TerminalQueryState};

use crate::session::SshSession;

impl SshSession {
    /// UI sets pixel cell metrics (after measuring the font) for `cursor_bounds`.
    pub fn set_cell_size(&self, cell_width: f32, line_height: f32) {
        *self.cell_width.lock().unwrap() = cell_width;
        *self.line_height.lock().unwrap() = line_height;
    }

    /// Get a `TerminalModel` adapter for the shared terminal-model operations.
    /// Cheap to create — just wraps the existing `Arc<FairMutex<Term>>`.
    pub(crate) fn model(&self) -> TerminalModel<crate::transport::SshListener> {
        TerminalModel::new(self.term.clone())
    }
}

impl TerminalSession for SshSession {
    // ── Render ──────────────────────────────────────────────────────
    fn snapshot(&self) -> TerminalContent {
        self.model().snapshot()
    }

    fn snapshot_query(&self) -> TerminalContent {
        self.model().snapshot_query()
    }

    fn query_state(&self) -> TerminalQueryState {
        self.model().query_state(self.alive())
    }

    fn query_line_range_cells(
        &self,
        start_line: usize,
        count: usize,
    ) -> (Vec<oneterm_terminal::IndexedCell>, usize) {
        self.model().query_line_range_cells(start_line, count)
    }

    fn dynamic_colors(&self) -> DynamicColors {
        self.model().dynamic_colors()
    }

    fn set_default_colors(
        &self,
        foreground: alacritty_terminal::vte::ansi::Rgb,
        background: alacritty_terminal::vte::ansi::Rgb,
        cursor: alacritty_terminal::vte::ansi::Rgb,
        ansi: [alacritty_terminal::vte::ansi::Rgb; 16],
    ) {
        self.state.set_default_colors(DefaultColors {
            foreground: Some(foreground),
            background: Some(background),
            cursor: Some(cursor),
            ansi: Some(ansi),
        });
    }

    fn terminal_info(&self) -> TerminalInfo {
        self.model()
            .terminal_info(self.state.absolute_line_count(), self.state.clear_epoch())
    }

    fn is_alt_screen(&self) -> bool {
        self.model().is_alt_screen()
    }

    // ── Input ───────────────────────────────────────────────────────
    fn write(&self, bytes: &[u8]) -> Result<(), TerminalError> {
        log::trace!("SshSession::write: {} bytes", bytes.len());
        self.transport().pty_write(bytes)
    }

    fn flush_pty(&self) {
        // SSH needs no ConPTY workaround — send a DSR query.
        if let Err(error) = self.transport().pty_write(b"\x1b[6n") {
            log::warn!("SshSession: PTY flush query failed: {error}");
        }
    }

    fn send_ctrl_c(&self) {
        if let Err(error) = self.transport().pty_write(b"\x03") {
            log::warn!("SshSession: Ctrl+C delivery failed: {error}");
        }
    }

    fn resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError> {
        if self.model().needs_resize(rows, cols) {
            self.transport().pty_resize(rows, cols)?;
            self.model().resize_grid(rows, cols);
        }
        Ok(())
    }

    fn scroll(&self, delta: i32) {
        self.model().scroll(delta);
    }

    fn scroll_to_bottom(&self) {
        self.model().scroll_to_bottom();
    }

    fn scroll_to_top(&self) {
        self.model().scroll_to_top();
    }

    // ── Mouse ────────────────────────────────────────────────────────
    fn mouse_down(
        &self,
        row: f32,
        col: f32,
        button: TerminalMouseButton,
        sel: SelectionType,
        mods: MouseModifiers,
    ) {
        if let Some(bytes) = self.model().mouse_down(row, col, button, sel, mods) {
            report_generated_input("SshSession mouse input", self.write(&bytes));
        }
    }

    fn mouse_move(&self, row: f32, col: f32, mods: MouseModifiers) {
        if let Some(bytes) = self.model().mouse_move(row, col, mods) {
            report_generated_input("SshSession mouse input", self.write(&bytes));
        }
    }

    fn mouse_drag(&self, row: f32, col: f32, mods: MouseModifiers) {
        if let Some(bytes) = self.model().mouse_drag(row, col, mods) {
            report_generated_input("SshSession mouse input", self.write(&bytes));
        }
    }

    fn mouse_up(&self, row: f32, col: f32, button: TerminalMouseButton, mods: MouseModifiers) {
        if let Some(bytes) = self.model().mouse_up(row, col, button, mods) {
            report_generated_input("SshSession mouse input", self.write(&bytes));
        }
    }

    fn wheel(&self, delta_y: f64, row: f32, col: f32, mods: MouseModifiers) {
        if let Some(bytes) = self.model().wheel(delta_y, row, col, mods) {
            report_generated_input("SshSession mouse input", self.write(&bytes));
        }
    }

    // ── Selection / clipboard ───────────────────────────────────────
    fn selection_text(&self) -> Option<String> {
        self.model().selection_text()
    }

    fn clear_selection(&self) {
        self.model().clear_selection();
    }

    fn select_all(&self) {
        self.model().select_all();
    }

    fn clear(&self) {
        // Send the `clear` command to the shell, exactly as if the user typed it.
        report_generated_input("SshSession clear command", self.write(b"clear\r"));
        self.clear_selection();
    }

    // ── Search ─────────────────────────────────────────────────────
    fn search(&self, query: &str, options: SearchOptions) -> Vec<SearchMatch> {
        self.model().search(query, options)
    }

    // ── IME ──────────────────────────────────────────────────────────
    fn set_marked_text(&self, text: String) {
        *self.marked_text.lock().unwrap() = Some(text);
    }

    fn clear_marked_text(&self) {
        *self.marked_text.lock().unwrap() = None;
    }

    fn commit_text(&self, text: &str) {
        self.clear_marked_text();
        report_generated_input("SshSession committed text", self.write(text.as_bytes()));
    }

    fn marked_text(&self) -> Option<String> {
        self.marked_text.lock().unwrap().clone()
    }

    fn cursor_bounds(&self) -> Option<CursorBounds> {
        let cw = *self.cell_width.lock().unwrap();
        let lh = *self.line_height.lock().unwrap();
        self.model().cursor_bounds(cw, lh)
    }

    // ── Lifecycle ────────────────────────────────────────────────────
    fn take_events(&self) -> Option<Receiver<SessionEvent>> {
        self.event_rx.lock().unwrap().take()
    }

    fn alive(&self) -> bool {
        self.state.alive()
    }

    fn close(&self) -> Result<(), TerminalError> {
        let result = self.transport().pty_close();
        self.state.set_alive(false);
        // SFTP shares the connection: closing the shell closes it too (ARCH-28).
        self.close_sftp();
        result
    }

    fn is_local(&self) -> bool {
        false
    }

    fn title(&self) -> Option<String> {
        self.state.title()
    }

    fn cwd(&self) -> Option<PathBuf> {
        self.state.cwd()
    }

    // ── Shell Integration ───────────────────────────────────────────
    fn prompt_count(&self) -> usize {
        self.state.prompt_count()
    }

    // ── Foreground Process ───────────────────────────────────────────
    fn foreground_process(&self) -> Option<String> {
        self.state.foreground_process()
    }

    // ── Optional capabilities ───────────────────────────────────────
    fn capabilities(&self) -> TerminalCapabilities {
        TerminalCapabilities {
            network_stats: Some(self.state.net_stats()),
            sftp: self
                .sftp
                .lock()
                .unwrap()
                .clone()
                .map(|session| session as std::sync::Arc<dyn SftpBackend>),
            cwd_source: Some(std::sync::Arc::new(SharedStateCwdSource::new(
                self.state.clone(),
            ))),
        }
    }
}
