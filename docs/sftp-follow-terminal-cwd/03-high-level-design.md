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