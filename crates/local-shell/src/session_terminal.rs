//! `impl TerminalSession for LocalSession` — render, input, mouse/selection,
//! clipboard, scroll, IME, and lifecycle query methods.
//!
//! ARCH-05: Terminal-model operations (snapshot, query, scroll, selection,
//! search, mouse encoding) are delegated to the shared `TerminalModel` adapter
//! in `oneterm_terminal`. Only transport (PTY write), lifecycle, and state remain
//! on `LocalSession`.

use std::path::PathBuf;

use alacritty_terminal::selection::SelectionType;
use async_channel::Receiver;

use oneterm_terminal::mouse_encode::{MouseModifiers, TerminalMouseButton};
use oneterm_terminal::{
    CursorBounds, DefaultColors, PtyTransport, SearchMatch, SearchOptions, SessionEvent,
    TerminalError, TerminalSession, report_generated_input,
};
use oneterm_terminal::{DynamicColors, TerminalContent, TerminalInfo, TerminalQueryState};

use crate::session::LocalSession;

impl TerminalSession for LocalSession {
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
        self.transport().pty_write(bytes)
    }

    fn flush_pty(&self) {
        // Send a DSR (Device Status Report) query → ConPTY processes the escape
        // sequence and responds with the cursor position → flushes the output buffer.
        // Windows ConPTY buffers output and only flushes on interaction.
        if let Err(error) = self.transport().pty_write(b"\x1b[6n") {
            log::warn!("LocalSession: PTY flush query failed: {error}");
        }
    }

    /// Send a Ctrl+C signal to the shell process.
    ///
    /// Sends \x03 over the PTY — ConPTY (with OpenConsole.exe from Windows
    /// Terminal) routes the signal correctly: CTRL_C_EVENT reaches only the child
    /// process, without exiting the shell or OneTerm.
    ///
    /// Requirement: conpty.dll + OpenConsole.exe must sit in the same directory as
    /// the exe. See crates/app/build.rs — they are copied from assets/ to the
    /// target directory automatically.
    #[cfg(windows)]
    fn send_ctrl_c(&self) {
        if let Err(error) = self.transport().pty_write(b"\x03") {
            log::warn!("LocalSession: Ctrl+C delivery failed: {error}");
        }
    }

    #[cfg(not(windows))]
    fn send_ctrl_c(&self) {
        if let Err(error) = self.transport().pty_write(b"\x03") {
            log::warn!("LocalSession: Ctrl+C delivery failed: {error}");
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
            report_generated_input("LocalSession mouse input", self.write(&bytes));
        }
    }

    fn mouse_move(&self, row: f32, col: f32, mods: MouseModifiers) {
        if let Some(bytes) = self.model().mouse_move(row, col, mods) {
            report_generated_input("LocalSession mouse input", self.write(&bytes));
        }
    }

    fn mouse_drag(&self, row: f32, col: f32, mods: MouseModifiers) {
        if let Some(bytes) = self.model().mouse_drag(row, col, mods) {
            report_generated_input("LocalSession mouse input", self.write(&bytes));
        }
    }

    fn mouse_up(&self, row: f32, col: f32, button: TerminalMouseButton, mods: MouseModifiers) {
        if let Some(bytes) = self.model().mouse_up(row, col, button, mods) {
            report_generated_input("LocalSession mouse input", self.write(&bytes));
        }
    }

    fn wheel(&self, delta_y: f64, row: f32, col: f32, mods: MouseModifiers) {
        if let Some(bytes) = self.model().wheel(delta_y, row, col, mods) {
            report_generated_input("LocalSession mouse input", self.write(&bytes));
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
        report_generated_input("LocalSession clear command", self.write(b"clear\r"));
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
        report_generated_input("LocalSession committed text", self.write(text.as_bytes()));
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
        let result = self.shutdown_owner();
        self.state.set_alive(false);
        result
    }

    fn is_local(&self) -> bool {
        true
    }

    fn title(&self) -> Option<String> {
        self.state.title()
    }

    fn cwd(&self) -> Option<PathBuf> {
        self.state.cwd()
    }

    // ── Shell Integration (OSC 133) ────────────────────────────
    fn prompt_count(&self) -> usize {
        self.state.prompt_count()
    }
    fn scroll_to_prompt(&self, n: usize) {
        // TODO: implement scroll-to-prompt using prompt marker line positions.
        let _ = n;
    }
}
