# SFTP follow Terminal CWD — Part 2: Codebase current state

> This part records the exact pieces of related code that **already exist**, so the design
> in parts 03/04 only needs to "wire up" rather than rebuild. All excerpts below are
> verified from the current code (not assumptions).

---

## 2.1. `cwd` is already tracked and exposed

### In `core` — the `TerminalSession` trait

`crates/core/src/terminal/session.rs`:

```rust
pub trait TerminalSession {
    // ...
    /// The current title (OSC 0/2).
    fn title(&self) -> Option<String>;
    /// The current cwd (OSC 7).
    fn cwd(&self) -> Option<PathBuf>;
    // ...
    /// SFTP backend if the session has an SFTP channel (SSH only).
    /// `None` for a local shell.
    fn sftp(&self) -> Option<Arc<dyn SftpBackend>> { None }
}
```

→ **`cwd()` is already on the trait.** UI can call it without importing `ssh`/`local`.

### In `ssh` — the source of `cwd`

`crates/ssh/src/state.rs` — `SessionState`:

```rust
pub struct SessionState {
    pub title: Option<String>,
    /// Cwd (OSC 7 — set by the side-channel parser).
    pub cwd: Option<PathBuf>,
    // ...
}
```

`crates/ssh/src/task.rs` — parses OSC 7 from the shell stream then writes it to state:

```rust
fn handle_osc(payload: &OscPayload, state: &SharedState, listener: &SshListener) {
    match payload {
        OscPayload::Cwd(url) => {
            let cwd = parse_cwd_url(url);          // file://host/path → PathBuf
            { /* state.lock().cwd = cwd.clone() */ }
            listener.forward(SessionEvent::Cwd(cwd));   // ← there is a Cwd event!
        }
        // ...
    }
}
```

`crates/ssh/src/session_terminal.rs` — the accessor reads it back:

```rust
fn cwd(&self) -> Option<PathBuf> {
    self.state.lock().unwrap().cwd.clone()
}
```

**Two important points:**
- `cwd` updates **live** each time the remote shell emits OSC 7 (usually after each prompt).
- There is already a **`SessionEvent::Cwd(...)`** being `forward`ed — this is a ready-made hook
  for the auto-follow option (parts 03/05) without needing polling.

`crates/local/src/session_terminal.rs` has a similar `fn cwd()` for the local shell.

---

## 2.2. `SftpPanel` — already has `load_dir` / `goto_path`

`crates/ui/src/views/sftp/panel.rs`:

```rust
pub struct SftpPanel {
    pub(crate) sftp: Option<Arc<dyn SftpBackend>>,
    pub(crate) cwd: PathBuf,
    // table, selected, transfers, path_input, ...
}

impl SftpPanel {
    /// Read a directory — spawn background task, doesn't block UI.
    pub fn load_dir(&mut self, path: PathBuf, cx: &mut Context<Self>) { /* ... */ }

    /// Goto path — stat first; if it's a dir then load_dir, on error → path_error.
    fn goto_path(&mut self, path: PathBuf, cx: &mut Context<Self>) { /* ... */ }

    pub(crate) fn navigate_parent(&mut self, cx: &mut Context<Self>) { /* ... */ }
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) { /* ... */ }
    pub(crate) fn navigate_into(&mut self, idx: usize, cx: &mut Context<Self>) { /* ... */ }
}
```

→ Navigating SFTP to an arbitrary path **already exists** (`load_dir` / `goto_path`). The
feature just needs to call `load_dir(terminal_cwd)`.

Note on the **path type**: `goto_path` calls `sftp.stat(path)` to check the path exists
+ is a directory before loading. The `cwd` from OSC 7 is an **absolute remote-side path**
(POSIX, e.g. `/var/www/html`) — usable directly for SFTP `read_dir`/`stat`.

---

## 2.3. `AppState.active_sftp` — the "global panel, per-tab session" pattern

`crates/ui/src/state/app_state.rs`:

```rust
pub struct AppState {
    pub dock_area: Option<WeakEntity<DockArea>>,
    /// SFTP backend of the active SSH tab.
    /// None = local shell or SSH without SFTP support.
    pub active_sftp: Option<Arc<dyn SftpBackend>>,
}
```

`SftpPanel` **observes** `AppState` and swaps the backend when the tab changes:

```rust
cx.observe(&app_state, |this, state, cx| {
    let new_sftp = state.read(cx).active_sftp.clone();
    if sftp_changed(&this.sftp, &new_sftp) {
        this.sftp = new_sftp;
        // reset cwd/selection/transfers ...
        if this.sftp.is_some() { this.load_dir(PathBuf::from("."), cx); }
    }
    cx.notify();
}).detach();
```

Who sets `active_sftp`? — `TerminalPanel::set_active`
(`crates/ui/src/views/terminal/panel.rs`):

```rust
fn set_active(&mut self, active: bool, _: &mut Window, cx: &mut Context<Self>) {
    // ...
    if active {
        let sftp = self.view.read(cx).session.read(cx).sftp();
        AppState::global(cx).update(cx, |state, cx| {
            state.active_sftp = sftp;
            cx.notify();
        });
    }
}
```

**This is the crux of the design:** at the same time we fetch `sftp()`, we also have `session`
— so we can get `cwd()`. The session is stored as
`Entity<Box<dyn TerminalSession>>` in `LocalTerminalView`:

```rust
// crates/ui/src/views/terminal/view/mod.rs
pub struct LocalTerminalView {
    pub(crate) session: Entity<Box<dyn TerminalSession>>,
    // ...
}
```

---

## 2.4. SFTP toolbar — where the new button goes

`crates/ui/src/views/sftp/render.rs` — `render_toolbar` currently has: a path input (flex-1)
+ a Back button + a Refresh button + a "..." (menu) button. The buttons use
`gpui_component::button::Button` with `.icon(...).small().ghost().on_click(cx.listener(...))`.
Example, the Refresh button:

```rust
.child(
    Button::new("sftp-refresh")
        .icon(Icon::new(AppIcon::Refresh).small())
        .small()
        .ghost()
        .on_click(cx.listener(|this, _, _, cx| {
            this.refresh(cx);
        })),
)
```

→ The new button will be inserted into this same toolbar row, with the same style.

---

## 2.5. The gap to fill (gap analysis)

| # | Gap | Detail | Resolution direction (parts 03/04) |
|---|-----|----------|-------------------------------|
| G1 | **SftpPanel has no way to get the terminal's `cwd`** | The panel only holds `Arc<dyn SftpBackend>`, no reference to the session/terminal. | Add a channel supplying live `cwd` to `AppState` (parallel to `active_sftp`). |
| G2 | **`cwd` must be read live at click time** | Can't snapshot at `set_active` because the user `cd`s afterward. | Store a "ask for cwd" (a callable provider) rather than storing the `cwd` value. |
| G3 | **No button on the toolbar yet** | `render_toolbar` has no sync button. | Add a `Button` + handler `sync_to_terminal_cwd`. |
| G4 | **Disabled state when cwd/SFTP missing** | Need to know "is there a cwd" to toggle the button. | Provider returns `Option<PathBuf>`; `None` → disable. |
| G5 | *(extension)* **Auto-follow has no event channel to SFTP yet** | `SessionEvent::Cwd` exists in ssh but isn't wired to `SftpPanel`. | Optional: forward the event → observe in SftpPanel. |