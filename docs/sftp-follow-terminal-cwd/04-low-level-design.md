# SFTP follow Terminal CWD — Part 4: Low-Level Design

> Detailed structs, function signatures, and sample code per crate. The sample code
> illustrates the idea and may be refined during actual implementation. Edit order:
> `core` → `ssh`/`local` → `ui`.

---

## 4.1. `core` — `CwdSource` trait + accessor on `TerminalSession`

`crates/core/src/terminal/session.rs`:

```rust
use std::path::PathBuf;
use std::sync::Arc;

/// A source to read the "current directory" (OSC 7) of a session — read live, without
/// exposing the session's implementation details. Lets the UI get cwd without holding
/// a reference to `Entity` or importing the `ssh`/`local` crate.
pub trait CwdSource: Send + Sync {
    /// The current directory (OSC 7). `None` if OSC 7 hasn't been received yet.
    fn cwd(&self) -> Option<PathBuf>;
}

pub trait TerminalSession {
    // ── (existing) ──
    fn cwd(&self) -> Option<PathBuf>;
    fn sftp(&self) -> Option<Arc<dyn SftpBackend>> { None }

    // ── NEW ──
    /// Handle to read cwd live, shared with the UI. `None` = the session doesn't provide
    /// one (default). SSH/local override to return an `Arc` wrapping the shared state.
    fn cwd_source(&self) -> Option<Arc<dyn CwdSource>> {
        None
    }
}
```

Re-export in `crates/core/src/lib.rs`:

```rust
pub use terminal::{
    CursorBounds, CwdSource, DynamicColors, NetStats, SessionEvent, TerminalInfo,
    TerminalProgress, TerminalSession,
};
```

**Why a separate trait instead of returning `Arc<Mutex<SessionState>>`?** `SessionState`
lives in `ssh`/`local`, it shouldn't leak to `core`/`ui`. `CwdSource` is a minimal interface
(1 method) → the UI only sees what it needs.

---

## 4.2. `ssh` — implement `CwdSource` sharing `SharedState`

`crates/ssh/src/state.rs` — `SharedState = Arc<Mutex<SessionState>>` already has `cwd`.
Add a newtype providing `CwdSource`:

```rust
// crates/ssh/src/state.rs (or session_terminal.rs)
use std::path::PathBuf;
use std::sync::Arc;
use oneterm_core::CwdSource;

/// Reads `cwd` live from `SharedState`. Clone is cheap (Arc), shares the exact state the
/// listener updates when it receives OSC 7.
pub struct SshCwdSource {
    state: SharedState,
}

impl SshCwdSource {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }
}

impl CwdSource for SshCwdSource {
    fn cwd(&self) -> Option<PathBuf> {
        self.state.lock().unwrap().cwd.clone()
    }
}
```

`crates/ssh/src/session_terminal.rs` — override the accessor:

```rust
fn cwd_source(&self) -> Option<Arc<dyn CwdSource>> {
    Some(Arc::new(SshCwdSource::new(self.state.clone())))
}
```

> `self.state` is `SharedState` (`Arc<Mutex<SessionState>>`) — cloning only increases the
> refcount, pointing to the same state that `handle_osc`/listener writes `cwd` to. So
> `cwd_source().cwd()` always reflects the latest OSC 7.

`crates/local/src/session_terminal.rs` — similar (optional; the local shell also has
`cwd`, but SFTP doesn't apply to local so you can skip the override — the SFTP panel will
hide on a local tab). For consistency you may still implement it.

---

## 4.3. `ui` — `AppState` adds `active_cwd_source`

`crates/ui/src/state/app_state.rs`:

```rust
use std::sync::Arc;
use oneterm_core::{CwdSource, SftpBackend};

pub struct AppState {
    pub dock_area: Option<WeakEntity<DockArea>>,
    pub active_sftp: Option<Arc<dyn SftpBackend>>,
    /// NEW: cwd source of the active session — for SFTP "sync to terminal".
    /// None = the active tab doesn't provide cwd (local, or not supported yet).
    pub active_cwd_source: Option<Arc<dyn CwdSource>>,
}
```

Update `AppState`'s `Default`/`new` init to add the field `active_cwd_source: None`.

---

## 4.4. `ui` — `TerminalPanel::set_active` also sets the cwd source

`crates/ui/src/views/terminal/panel.rs`:

```rust
fn set_active(&mut self, active: bool, _: &mut Window, cx: &mut Context<Self>) {
    if self.is_active != active {
        self.is_active = active;
        cx.notify();
    }

    if active {
        let session = self.view.read(cx).session.read(cx);
        let sftp = session.sftp();
        let cwd_source = session.cwd_source();   // ← NEW
        AppState::global(cx).update(cx, |state, cx| {
            state.active_sftp = sftp;
            state.active_cwd_source = cwd_source;  // ← NEW
            cx.notify();
        });
    }
}
```

> Note: borrow `session` once then fetch both values to avoid calling `.read(cx)` twice.
> If the borrow checker complains, split into two `let sftp = ...; let cwd_source = ...;`
> statements, each with its own `read`.

---

## 4.5. `ui` — `SftpPanel` keeps `cwd_source` + observe

`crates/ui/src/views/sftp/panel.rs` — add the field:

```rust
use oneterm_core::{CwdSource, FileEntry, SftpBackend};

pub struct SftpPanel {
    // ... existing fields ...
    pub(crate) sftp: Option<Arc<dyn SftpBackend>>,
    /// NEW: cwd source of the active terminal (for the Sync button to read live).
    pub(crate) cwd_source: Option<Arc<dyn CwdSource>>,
    // ...
}
```

Init in `new`: `cwd_source: None`.

Update in the observe (same block that handles `active_sftp`):

```rust
cx.observe(&app_state, |this, state, cx| {
    let st = state.read(cx);
    let new_sftp = st.active_sftp.clone();
    let new_cwd_source = st.active_cwd_source.clone();   // ← NEW

    // Always update cwd_source per the active tab (even when sftp is unchanged).
    this.cwd_source = new_cwd_source;                    // ← NEW

    if sftp_changed(&this.sftp, &new_sftp) {
        this.sftp = new_sftp;
        // ... reset as before ...
        if this.sftp.is_some() {
            this.load_dir(PathBuf::from("."), cx);
        }
    }
    cx.notify();
}).detach();
```

Sync handler:

```rust
impl SftpPanel {
    /// Navigate the SFTP Browser to the active terminal's current directory.
    /// No-op if there's no SFTP or the terminal hasn't reported cwd.
    pub(crate) fn sync_to_terminal_cwd(&mut self, cx: &mut Context<Self>) {
        if self.sftp.is_none() {
            return;
        }
        let cwd = match self.cwd_source.as_ref().and_then(|s| s.cwd()) {
            Some(p) => p,
            None => {
                log::debug!("SftpPanel::sync_to_terminal_cwd: terminal cwd unavailable");
                return;
            }
        };
        log::info!(
            "SftpPanel::sync_to_terminal_cwd: \"{}\" → \"{}\"",
            self.cwd.display(),
            cwd.display()
        );
        // goto_path already stats + handles errors + load_dir.
        self.goto_path(cwd, cx);
    }

    /// The terminal's current cwd (to render button state + tooltip).
    pub(crate) fn terminal_cwd(&self) -> Option<PathBuf> {
        self.cwd_source.as_ref().and_then(|s| s.cwd())
    }
}
```

> `goto_path` is currently a private `fn` (not `pub(crate)`) — keep it as-is because
> `sync_to_terminal_cwd` is in the same `impl`/module so it can call it. If you split the
> module, change its visibility to `pub(crate)`.

---

## 4.6. `ui` — the toolbar button

`crates/ui/src/views/sftp/render.rs`, in `render_toolbar`, insert the button between Back and
Refresh (or next to Refresh). Compute the state before building the row:

```rust
// In render_toolbar, before building h_flex():
let terminal_cwd = self.terminal_cwd();          // Option<PathBuf>
let sync_enabled = terminal_cwd.is_some();
let sync_tooltip = match &terminal_cwd {
    Some(p) => format!("Go to the terminal's current directory: {}", p.display()),
    None => "Terminal hasn't reported its current directory (needs shell integration / OSC 7)"
        .to_string(),
};
```

The button (inside the toolbar `h_flex`'s `.child(...)` chain):

```rust
// Sync-to-terminal-cwd button
.child(
    Button::new("sftp-sync-cwd")
        .icon(Icon::new(IconName::FolderSync).small())   // see 4.7 for the icon
        .small()
        .ghost()
        .disabled(!sync_enabled)
        .tooltip(sync_tooltip)
        .on_click(cx.listener(|this, _, _, cx| {
            this.sync_to_terminal_cwd(cx);
        })),
)
```

> gpui-component's `Button` supports `.disabled(bool)` and `.tooltip(impl Into<SharedString>)`.
> Confirm the exact API in `reference/gpui-component/crates/ui/src/button/`. If
> `.tooltip` takes a closure/`Tooltip`, adjust to the real signature (reference-first per
> AGENTS.md §3.0).

---

## 4.7. Icon

Need an icon suggesting "sync directory / follow". Prefer an existing name in
`IconName` (check `reference/gpui-component/crates/ui/src/icon.rs`). Candidates:
`FolderSync`, `FolderInput`, `LocateFixed`, `Crosshair`, `RefreshCw`.

- If the name isn't in gpui-component's `IconName` yet → add a Lucide SVG to
  `crates/ui/assets/icons/<name>.svg` and register it via `AppIcon` (like `AppIcon::Refresh`
  used by the Refresh button). See AGENTS.md §3.4 (Theme & icon).
- Name the SVG file the same as the variable name in `AppIcon`/`IconName`.

Suggestion: use `AppIcon::FolderSync` (new) or reuse an existing "target/locate" icon
to reduce asset additions.

---

## 4.8. Consolidated changes per file

| Crate | File | Change |
|-------|------|--------|
| core | `terminal/session.rs` | + `CwdSource` trait; + `fn cwd_source()` (default `None`) on `TerminalSession` |
| core | `lib.rs` | + re-export `CwdSource` |
| ssh | `state.rs` (or `session_terminal.rs`) | + struct `SshCwdSource` impl `CwdSource` |
| ssh | `session_terminal.rs` | + override `fn cwd_source()` |
| local | `session_terminal.rs` | *(optional)* + override `fn cwd_source()` |
| ui | `state/app_state.rs` | + field `active_cwd_source` + update init |
| ui | `views/terminal/panel.rs` | `set_active`: set `active_cwd_source` |
| ui | `views/sftp/panel.rs` | + field `cwd_source`; observe update; + `sync_to_terminal_cwd`, `terminal_cwd` |
| ui | `views/sftp/render.rs` | `render_toolbar`: + Sync button (disabled/tooltip by state) |
| ui | `assets/icons/` + `icon.rs` | *(if needed)* + new icon |

---

## 4.9. Alternative for §4.1–4.2 (no new trait)

If the team wants to avoid the `CwdSource` trait, you can store
**`WeakEntity<Box<dyn TerminalSession>>`** in `AppState` (option B in part 03) and in
`SftpPanel` call:

```rust
let cwd = self.active_session
    .as_ref()
    .and_then(|w| w.upgrade())
    .and_then(|e| e.read(cx).cwd());
```

- **Pros:** no new type in `core`; uses the existing `cwd()` directly.
- **Cons:** `AppState`/`SftpPanel` must know the `Entity<Box<dyn TerminalSession>>` type
  (this is a `ui`-side type so it's still valid layering-wise); needs `cx` to `read`
  (available in the handler); must manage `WeakEntity` lifetime.

Recommendation: **option C (`CwdSource`)** for a clean boundary and to open the auto-follow
path; option B if you want minimal change and accept `SftpPanel` holding a weak-ref to the
session.