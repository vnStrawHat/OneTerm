# SFTP follow Terminal CWD — Part 1: Overview & goals

> **Status (2026-08): historical design record — see [`docs/architecture.md`](architecture.md) and the current code for the implemented state.**
> Shipped state differs from the sketches below: `CwdSource` lives in `oneterm-terminal`
> (`crates/terminal/src/session.rs`, exposed as `TerminalCapabilities::cwd_source` from `TerminalSession::capabilities()`), the follow state is
> `FollowCwd` in `crates/sftp-ui/src/browser_view.rs`, and the "auto-follow" extension is
> implemented as an on/off toggle backed by a 500 ms polling timer in
> `crates/sftp-ui/src/panel.rs` (no `SessionEvent::Cwd` wiring); the browser keeps its cwd as
> `oneterm_core::RemotePath` and converts the OSC 7 path at the boundary.

> Design document for the feature: **a "Sync SFTP Browser to the current directory of the
> SSH session" button**. When the user `cd`s to another directory in the terminal, clicking
> this button makes the SFTP Browser jump to that exact directory.
>
> **Related references:**
> - [`docs/sftp-browser-design.md`](../sftp-browser-design.md) — SFTP browser design + `SftpPanel`
> - [`docs/osc-sequences-checklist.md`](../osc-sequences-checklist.md) §F — OSC 7 (CWD)
> - [`docs/terminal-backend.md`](../terminal-backend.md) — `SshSession`, `TerminalSession`
> - [`docs/agents/structure.md`](../agents/structure.md) — crate structure & dependency graph
>
> **Parts of this document** (split for readability, merged back later):
> 1. `01-overview.md` — overview & goals (this file)
> 2. `02-current-state.md` — related codebase current state
> 3. `03-high-level-design.md` — high-level design (architecture, data flow)
> 4. `04-low-level-design.md` — detailed design (structs, functions, code)
> 5. `05-edge-cases-roadmap.md` — edge cases, risks, implementation roadmap

---

## 1.1. Feature description

The SFTP Browser (right panel) and the Terminal (SSH shell, center panel) are currently **two
independent streams** running on the same SSH connection. The directory the SFTP is browsing
(`cwd` of `SftpPanel`) is **unrelated** to the directory the shell is in (`pwd` on the
remote side). If the user runs `cd /var/log` in the terminal, SFTP stays at `~`.

This feature adds **one button on the SFTP Browser toolbar**. When the user clicks:

1. Read the current directory (`cwd`) of the SSH session attached to the active terminal tab.
2. Navigate the SFTP Browser to that exact directory (`load_dir`).

This is **manual sync** (sync on demand): each time the user wants SFTP to "follow" the shell
location, they click the button. No auto-follow by default (see §1.4 for the rationale and the
optional auto-follow extension).

### Example usage flow

```
Terminal:  user@host:~$ cd /var/www/html
SFTP:      still at /home/user
           │
           └─ user clicks the [⤢ Sync to terminal] button on the SFTP toolbar
                     │
                     └─ SFTP Browser jumps to /var/www/html
```

---

## 1.2. Functional requirements

| # | Requirement | Notes |
|---|---------|---------|
| R1 | The SFTP toolbar has a "sync to terminal cwd" button | Clear icon + tooltip |
| R2 | Click the button → SFTP navigates to the `cwd` of the active SSH session | Use the existing `load_dir` |
| R3 | `cwd` is read live at click time (not a stale snapshot) | Reflects the most recent `cd` |
| R4 | If `cwd` is unavailable (no OSC 7 received yet) → button disabled + explanatory tooltip | No crash, no wrong jump |
| R5 | Local shell tab or SSH without SFTP → button not shown (or disabled) | Consistent with `render_no_connection` |
| R6 | Don't break the crate architecture: `ui` doesn't import `ssh`/`local` | Communicate via the `TerminalSession` trait |
| R7 | (Extension, optional) Toggle "auto-follow" — auto-sync each time cwd changes | Not required for the first version |

---

## 1.3. Prerequisite: OSC 7 must work

The terminal `cwd` is determined via **OSC 7** (`ESC]7;file://host/path ST`). Per
[`osc-sequences-checklist.md`](../osc-sequences-checklist.md) §F, OneTerm **already supports**
parsing OSC 7 (self-parses in parallel because `alacritty_terminal` drops it), stores it in
`SessionState.cwd`, and exposes it via `TerminalSession::cwd() -> Option<PathBuf>`.

**Key point for SSH:** OSC 7 is emitted by the **remote-side shell**. It is only present when
the remote shell is configured to emit OSC 7 (via bash's `PROMPT_COMMAND`, zsh's `precmd`/`PS1`,
or the VTE integration that many distros preinstall at `/etc/profile.d/`). If the remote shell
does **not** emit OSC 7, then `cwd()` returns `None` and the feature cannot "follow".

→ This is the **foundational assumption** of the design. Handling the missing-OSC-7 case is
in R4 (disabled button + tooltip) and is discussed in depth in `05-edge-cases-roadmap.md`.
**Actively injecting shell integration on SSH login** to guarantee OSC 7 is always present is
an extension direction, also discussed in part 05.

---

## 1.4. Manual sync vs Auto-follow

| Option | Pros | Cons |
|-----------|-----|-------|
| **Manual (button)** — first version | Simple, no wasted read_dir, user-driven | Must click each time |
| **Auto-follow (toggle)** — extension | SFTP always matches the shell automatically | Each `cd` → 1 `read_dir` (bandwidth cost), can cause unwanted "jumps" while manipulating files |

The user's original request is "click a button and SFTP follows automatically" → this is
**manual sync** by nature. Auto-follow is left as an optional on/off toggle for later (R7),
because it changes UX significantly and costs resources when the user types many `cd` commands
in a row.

---

## 1.5. Design principles

1. **Reuse existing infrastructure** — `cwd()` (trait), `load_dir()` (SftpPanel),
   the `AppState.active_sftp` pattern already exist. Don't reinvent.
2. **Live read** — read `cwd` at click time, don't cache a stale snapshot.
3. **Respect layering** — `ui` only touches `dyn TerminalSession` (in `core`), does not
   import `ssh`/`local`.
4. **Fail safe** — missing OSC 7 / no SFTP → disabled, don't jump to the wrong place.
5. **Don't touch the SFTP backend** — the feature is pure UI + 1 `cwd` data channel; don't
   modify `sftp_task`, `SftpCmd`, the protocol.
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
# SFTP follow Terminal CWD — Part 3: High-Level Design

> This part describes the overall architecture: components, data flow, and how to wire the
> terminal's `cwd` to `SftpPanel` while respecting layering. Code details are in part 04.

---

## 3.1. Core idea

`SftpPanel` needs to answer the question **"what is the current directory of the active SSH
session?"** right when the user clicks the button. Since the user can `cd` after opening the
tab, we **don't** snapshot `cwd` once; instead we store a **"cwd provider"** — something we
can call to get the latest `cwd` at any time.

Two pieces already exist:
- `TerminalSession::cwd() -> Option<PathBuf>` reads live from `SessionState`.
- `AppState` is the interface point between `TerminalPanel` (per-tab) and `SftpPanel` (global).

→ **Extend the `active_sftp` pattern**: add to `AppState` a handle that allows reading the
`cwd` of the active session. `TerminalPanel::set_active` sets this handle at the same time as
`active_sftp`.

---

## 3.2. Choosing the "cwd provider" mechanism

Three options, considered by layering + "live"-ness:

| Option | Description | Live? | Layering | Assessment |
|----|-------|:-----:|----------|----------|
| **A. Snapshot `PathBuf`** | Store `active_cwd: Option<PathBuf>` in AppState at `set_active` | ❌ | OK | Wrong — doesn't follow later `cd` |
| **B. Weak handle to session entity** | Store `WeakEntity<...>` then `.read(cx).cwd()` on click | ✅ | ⚠️ needs the session type | Good but requires exposing the `Entity<Box<dyn TerminalSession>>` type |
| **C. Closure provider** | Store `Arc<dyn Fn() -> Option<PathBuf>>` wrapping session state | ✅ | ✅ clean | **Chosen** — doesn't depend on gpui entity, pure `core` type |

**Choose option C** with a refinement: instead of a closure that's hard to store/compare, we
expose **the same data source that `cwd()` reads** — i.e. a handle to the shared state.
But that state lives in the `ssh`/`local` crate (UI must not import it). So we wrap it behind
a **small trait in `core`**:

```rust
// core: a "cwd source" that can be read live, without exposing session details
pub trait CwdSource: Send + Sync {
    fn cwd(&self) -> Option<PathBuf>;
}
```

`SshSession`/`LocalSession` can provide an `Arc<dyn CwdSource>` sharing the same
`SharedState` (read-only the `cwd` field). UI keeps `Arc<dyn CwdSource>` in `AppState`.

> **Consideration note:** if the `CwdSource` trait feels redundant, one could reuse
> `Arc<dyn SftpBackend>` by... no — `SftpBackend` doesn't know `cwd`. Keep `CwdSource`
> separate, that's the correct separation of concerns. Cost: 1 trait + 1 accessor. See part 04
> for a comparison with the "read via entity" option (B) if you want to avoid a new trait.

---

## 3.3. Component diagram

```
                          crates/core
         ┌───────────────────────────────────────────────┐
         │ trait TerminalSession { fn cwd() -> Option<..> │
         │                         fn cwd_source() -> ..  │  ← NEW (default None)
         │ trait CwdSource { fn cwd() -> Option<PathBuf> }│  ← NEW
         └───────────────────────────────────────────────┘
               ▲                                   ▲
               │ impl                              │ impl (shares SharedState.cwd)
      ┌────────┴─────────┐              ┌──────────┴───────────┐
      │ crates/ssh       │              │ crates/local         │
      │  SshSession      │              │  LocalSession        │
      │  SharedState.cwd │              │  SharedState.cwd     │
      └──────────────────┘              └──────────────────────┘

                          crates/ui
    ┌──────────────────────────────────────────────────────────────┐
    │ TerminalPanel::set_active(active)                             │
    │   if active {                                                 │
    │     state.active_sftp   = session.sftp();          (existing) │
    │     state.active_cwd_source = session.cwd_source();  ← NEW    │
    │   }                                                           │
    │                                                               │
    │ AppState { active_sftp, active_cwd_source }   ← add field     │
    │                                                               │
    │ SftpPanel (observe AppState)                                  │
    │   - keep cwd_source: Option<Arc<dyn CwdSource>>                │
    │   - toolbar: [Sync to terminal cwd] button                    │
    │       on_click → sync_to_terminal_cwd():                      │
    │           match cwd_source.cwd() {                            │
    │             Some(p) => self.goto_path(p)  // stat + load_dir  │
    │             None    => (button already disabled)               │
    │           }                                                   │
    └──────────────────────────────────────────────────────────────┘
```

---

## 3.4. Data flow — Manual sync (first version)

```
User clicks [Sync]  ─────────────────────────────────────────┐
                                                              ▼
SftpPanel::sync_to_terminal_cwd(cx)                          │
  1. let src = self.cwd_source.clone()?          // None → return│
  2. let cwd = src.cwd()?                         // None → return
  3. self.goto_path(cwd, cx)                                   │
        │                                                      │
        ├─ sftp.stat(cwd)  (background)                        │
        │     ├─ Ok(dir)  → load_dir(cwd) → read_dir → render  │
        │     ├─ Ok(file) → path_error (rare, cwd is always a dir)│
        │     └─ Err      → path_error / notify                 │
        ▼                                                      │
   SFTP Browser shows the contents of the dir = shell's pwd ──┘
```

Notes:
- `goto_path` **already** does the `stat` step + error handling → reuse, don't rewrite.
- All I/O (`stat`, `read_dir`) runs in the background like the existing `load_dir` → no UI block.

---

## 3.5. Button state (enabled / disabled / hidden)

The button decides its state based on 2 conditions, read in `render_toolbar`:

| Condition | Button result |
|-----------|---------------|
| `self.sftp.is_none()` (local shell / no SFTP) | Toolbar doesn't render (existing `render_no_connection`) → button doesn't appear |
| Has SFTP but `cwd_source` is `None` or `cwd_source.cwd() == None` | Button **disabled** + tooltip: "Terminal hasn't reported its current directory (needs shell integration / OSC 7)" |
| Has SFTP and `cwd_source.cwd() == Some(p)` | Button **enabled**; tooltip: "Jump to the terminal's current directory: {p}" |

> Reading `cwd_source.cwd()` in `render` is a light operation (lock + clone `PathBuf`),
> acceptable. If you want to avoid calling it every frame, you can cache and update via
> observe (see auto-follow §3.6).

---

## 3.6. (Optional extension) Auto-follow

If implementing R7, leverage the **`SessionEvent::Cwd`** already `forward`ed by `ssh`:

```
remote shell `cd` → OSC 7 → ssh task → SessionEvent::Cwd(path)
    → (existing) updates SharedState.cwd
    → (NEW) UI forwards to SftpPanel when auto-follow is on
        → SftpPanel.load_dir(path)  (only when path differs from current cwd + panel active)
```

Auto-follow design:
- Add an `auto_follow: bool` flag in `SftpPanel` (toggle on the toolbar, persisted to
  `docks.json` like other SFTP settings).
- Event channel: either (a) `LocalTerminalView` already subscribes to `SessionEvent` to
  re-render — add: on receiving `Cwd`, if the tab is active + auto-follow, call into
  `SftpPanel`; or (b) push the new `cwd` into `AppState` (add `active_cwd: Option<PathBuf>`
  updated in realtime) and `SftpPanel` observes.
- Anti-jitter: debounce + only load when `path != self.cwd` to avoid a flood of `read_dir`
  when the user types many `cd`s.

Auto-follow is **out of scope for the first version**; recorded here so the manual design
doesn't block the extension path (e.g. `CwdSource` + `SessionEvent::Cwd` are both reusable).

---

## 3.7. Why not put the button on the Terminal side?

One could put an "open this directory in SFTP" button in the terminal toolbar/breadcrumb. But:
- The SFTP Browser is where the user is looking at the file list → placing the button there is
  more intuitive ("pull SFTP toward me").
- The SFTP toolbar already has the navigation button cluster (Back/Refresh) → the Sync button
  joins the same "navigation" semantic group.
- Avoids a reverse dependency: the terminal view doesn't need to know about the SFTP panel.

→ Put the button on the **SFTP toolbar**. Matches the user's original request.
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
# SFTP follow Terminal CWD — Part 5: Edge cases, risks & roadmap

---

## 5.1. Edge cases

| # | Situation | Expected behavior |
|---|-----------|-------------------|
| E1 | Remote shell **doesn't** emit OSC 7 → `cwd() == None` | Button disabled + explanatory tooltip. No jump. |
| E2 | `cwd` points to a directory **without read permission** (permission denied) | `goto_path`→`stat`/`read_dir` errors → show `path_error`/error message (already exists). Keep the old cwd. |
| E3 | `cwd` is a directory that was just deleted | Same as E2 — stat error → report, don't change cwd. |
| E4 | The active tab is a **local shell** (no SFTP) | Toolbar doesn't render (`render_no_connection`) → button doesn't appear. |
| E5 | SSH has a shell but **can't open an SFTP channel** | `self.sftp == None` → button doesn't appear (or is disabled). |
| E6 | User clicks Sync while a **transfer is running** | `load_dir` only changes the listing; the transfer runs independently in the background (separate channel) → no impact. |
| E7 | `cwd` equals the directory SFTP is already in | Still `load_dir` (refresh) — acceptable; or skip if equal to save (minor optional). |
| E8 | OSC 7 returns a path with **special characters / non-UTF8** | `parse_cwd_url` already handles it in the ssh layer; `PathBuf` stays as-is; SFTP `stat` reports an error if the server rejects it. |
| E9 | Path from OSC 7 carries a **different hostname** (weird mount, container) | Only use the path part of `file://host/path`. `parse_cwd_url` already drops the host. If the host differs from the real remote, the directory may not exist → E2. |
| E10 | Rapid tab switching | `active_cwd_source` updates to the latest `set_active`; observe reads the current value → always matches the tab being viewed. |

---

## 5.2. Risks & mitigations

| Risk | Level | Mitigation |
|--------|:---:|-----------|
| **OSC 7 isn't available on many servers** making the feature "useless" for that user | Medium | Clear tooltip; docs on enabling shell integration; consider injecting shell integration on SSH login (§5.4) |
| Calling `cwd_source.cwd()` every frame in `render` (Mutex lock) | Low | Lock is extremely short (clone `Option<PathBuf>`); if needed, cache + update via observe |
| Adding the `CwdSource` trait widens the `core` API surface | Low | 1-method trait, well-documented; or use option B (weak entity) if desired |
| Auto-follow (if done) causes a flood of `read_dir` when typing many `cd`s | Medium | Debounce + only load when `path != cwd` + only when panel is active |
| Borrow checker when fetching `sftp()` + `cwd_source()` together in `set_active` | Low | Split into two `let` statements, each with its own `read(cx)` |

---

## 5.3. Testing

**Unit / logic:**
- `SshCwdSource::cwd()` reflects the `SharedState.cwd` value after setting it (simulate
  OSC 7 → update state → read back).
- `sync_to_terminal_cwd`: when `cwd_source == None` → no-op; when `Some(path)` → calls
  `goto_path(path)`.

**Manual:**
1. SSH to a server with shell integration (bash + `PROMPT_COMMAND` emitting OSC 7).
2. `cd /var/log` in the terminal → click Sync → SFTP shows `/var/log`.
3. `cd /etc` → click Sync → SFTP jumps to `/etc`.
4. SSH to a server that **doesn't** emit OSC 7 → button disabled, tooltip correct.
5. Local shell tab → no panel/button visible.
6. `cd` to a directory without read permission → click Sync → error, SFTP cwd unchanged.

**Quality gate (mandatory, per AGENTS.md §5):**
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
```

---

## 5.4. (Implemented) OSC 7 over SSH — silent bootstrap after `request_shell`

For the local shell, OneTerm generates OSC 7/133 via env at spawn (silent). For SSH, the working approach is now:

1. request a PTY with `ECHO=0` when shell integration is enabled,
2. call `request_shell(true)` so sshd/PAM still prints the normal login banner, MOTD, and `Last login`,
3. send a compact bootstrap command into the running shell,
4. restore echo with `stty echo`.

| Approach | Silent? | Needs sshd config? | Result |
|------|:--------:|:------------------:|---------|
| Write a snippet to stdin (`channel.data`) | ❌ (PTY echoes it) | No | Tried — shows up in the terminal |
| Strip echo on the client side (`EchoSuppressor`) | ❌ (echo gets reformatted) | No | Tried — still shows up |
| `channel.set_env("PROMPT_COMMAND")` | ✅ | **Yes** (`AcceptEnv`) | Tried — server rejects it → OSC 7 lost |
| **PTY echo off + `request_shell` + bootstrap** | ✅ | **No** | **Currently used** |

**The approach in use** — keep the shell login flow, then bootstrap the prompt hook:

```
__oneterm_osc7() { printf '\x1b]7;file://%s%s\x1b\\' "${HOSTNAME:-$(hostname)}" "$PWD"; printf '\x1b]133;A\x1b\\'; };
case ";${PROMPT_COMMAND:-};" in *";__oneterm_osc7;"*) ;; *) PROMPT_COMMAND="__oneterm_osc7${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;; esac;
__oneterm_osc7;
stty echo 2>/dev/null
```

- `request_shell(true)` preserves the normal sshd/PAM banner, MOTD, and `Last login`.
- `Pty::ECHO = 0` keeps the bootstrap command itself from being echoed while it is sent.
- The bootstrap installs `PROMPT_COMMAND` in the running shell; no child `exec` and no manual MOTD replay are needed.
- `stty echo` restores normal user input once the hook is installed.
- **Doesn't depend on** `AcceptEnv` (unlike `set_env`).

**Remaining limitations:**
- bash-oriented (`PROMPT_COMMAND`). zsh/other shells may not keep the hook, but the login shell still starts normally.
- A `.bashrc` that overwrites `PROMPT_COMMAND` will disable the hook (most distros don't touch it by default).
- `SshConfig::shell_integration = false` disables the bootstrap and leaves a plain `request_shell(true)` session.

---

## 5.5. Implementation roadmap

Suggested order (each step must build + clippy clean before moving on):

- [ ] **B1 — core**: add the `CwdSource` trait + `fn cwd_source()` default `None` +
  re-export. `cargo build -p oneterm-core`.
- [ ] **B2 — ssh**: `SshCwdSource` + override `cwd_source()`. Build ssh.
- [ ] **B3 — (optional) local**: override `cwd_source()` for consistency.
- [ ] **B4 — ui state**: add `AppState.active_cwd_source` + update init.
- [ ] **B5 — ui terminal**: `set_active` sets `active_cwd_source`.
- [ ] **B6 — ui sftp panel**: field `cwd_source`, observe, `sync_to_terminal_cwd`,
  `terminal_cwd`.
- [ ] **B7 — ui sftp render**: Sync button on the toolbar (disabled/tooltip by state).
- [ ] **B8 — icon**: add/pick an icon (`FolderSync` or equivalent).
- [ ] **B9 — quality gate**: fmt + clippy + build workspace; manual test per §5.3.
- [ ] **B10 — (extension)** auto-follow toggle (R7): `auto_follow` flag, wire
  `SessionEvent::Cwd`, debounce, persist.

---

## 5.6. Definition of Done — first version

- The Sync button appears on the SFTP toolbar when the active tab is an SSH tab with SFTP.
- Clicking the button → SFTP navigates to the terminal's current `cwd` (read live).
- Missing OSC 7 → button disabled + tooltip; no crash, no wrong jump.
- No layering violation (`ui` doesn't import `ssh`/`local`).
- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build --workspace` all pass.
