# SSH Client Connect Design — OneTerm

> **Status:** Historical design record. For current crate ownership and paths, see [`docs/architecture.md`](architecture.md). For the accepted password and private-key authentication behavior, see [`docs/ssh-authentication.md`](ssh-authentication.md).

> Design document for the SSH connect feature: click an item in the SSH Session list →
> open an SSH session to the target server, with a credential-entry dialog when needed.
>
> **Related references:**
> - [`docs/terminal-backend.md`](terminal-backend.md) §7 — `SshSession` design
>   (russh + hidden tokio runtime, `SshConfig`, auth).
> - [`docs/gui-layout.md`](gui-layout.md) — DockArea, Panel trait, TerminalPanel.
> - [`docs/agents/structure.md`](agents/structure.md) — crate rules, directory tree.

## Table of contents

1. [Overview & flow](#1-overview--flow)
2. [Data structures](#2-data-structures)
3. [Credential dialog branching logic](#3-credential-dialog-branching-logic)
4. [Dialog UI — Connect SSH](#4-dialog-ui--connect-ssh)
5. [Connection flow — create SshSession + open tab](#5-connection-flow--create-sshsession--open-tab)
6. [Integration into SessionPanel](#6-integration-into-sessionpanel)
7. [File structure](#7-file-structure)
8. [Implementation checklist](#8-implementation-checklist)

---

## 1. Overview & flow

### 1.1. Feature description

When the user left-clicks an item in the SSH Session list in `SessionPanel` (right dock),
the app opens an SSH session to the target server using the info in `SshSession` (host, port,
username). Before connecting, if credentials are missing (username or password), the app shows
a dialog for the user to enter them.

### 1.2. Flow diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│  User clicks a session item in SessionPanel                          │
│  (render_session_row → on_click)                                    │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
                               ▼
                     ┌─────────────────────┐
                     │  Read SshSession     │
                     │  from SshSessionStore│
                     │  (label, host,      │
                     │   port, username)   │
                     └──────────┬──────────┘
                               │
                               ▼
                     ┌─────────────────────┐      username = None
                     │  Is there a username?│──────────────────┐
                     └──────────┬──────────┘                  │
                               │ Some                         │ No
                               ▼                              │
                     ┌─────────────────────┐                  ▼
                     │  Dialog for         │         ┌────────────────────┐
                     │  PASSWORD only      │         │  Dialog for        │
                     │  (1 masked field)   │         │  USERNAME + PASSWORD│
                     └──────────┬──────────┘         │  (2 fields)        │
                               │                    └────────┬───────────┘
                               │                             │
                               ▼                             ▼
                     ┌──────────────────────────────────────────────┐
                     │  User clicks Connect                         │
                     │  (or Cancel → abort)                          │
                     └──────────────────────┬───────────────────────┘
                                            │ Connect
                                            ▼
                     ┌──────────────────────────────────────────────┐
                     │  Create SshConfig { host, port, username,      │
                     │    password, auth_method: Password }         │
                     └──────────────────────┬───────────────────────┘
                                            │
                                            ▼
                     ┌──────────────────────────────────────────────┐
                     │  SshSession::connect(cfg, pty_size)           │
                     │  → russh connect + auth + pty-req + shell     │
                     │  (see terminal-backend.md §7)                 │
                     └──────────────────────┬───────────────────────┘
                                            │ Ok(session)
                                            ▼
                     ┌──────────────────────────────────────────────┐
                     │  Create a new TerminalPanel with SshSession   │
                     │  (instead of the default LocalSession)       │
                     │  → add_panel into the DockArea center         │
                     │  → tab title = session.label                 │
                     └──────────────────────────────────────────────┘
```

### 1.3. Design decisions

| # | Decision | Rationale |
|---|---|---|
| 1 | **Password is NOT persisted** to `ssh_session.json` | Security — the password lives only in RAM during the session, never written to disk. |
| 2 | **Username is persisted** to `ssh_session.json` (the field already exists) | Convenience — the user only enters a password next time. The username is less sensitive than the password. |
| 3 | **Use `LocalTerminalView`** for SSH too (via `dyn TerminalSession`) | The view is already backend-agnostic — it only needs `Entity<Box<dyn TerminalSession>>`. No separate `SshTerminalView` needed. |
| 4 | **Dialog uses `window.open_dialog`** (gpui-component Dialog) | Matches the pattern already used for the "New/Edit SSH Session" dialog in `session_tabs/tabs.rs`. |
| 5 | **Password field uses `InputState::masked(true)` + `.mask_toggle()`** | Shows `•••••`, with an eye-icon button to reveal/hide. API already available in gpui-component. |
| 6 | **Footer: Cancel (left) + Connect (right), right-aligned** | `DialogFooter` defaults to `justify_end` → buttons auto-align right. Matches the requirement. |
| 7 | **Connect runs async** — the dialog closes immediately, the connection runs in the background | Avoids blocking the UI. If connect fails → `window.push_notification` reports the error. |
| 8 | **Left-click = Open**, right-click keeps the context menu (Open/Delete/Property) | Keeps the current context-menu behavior, adds a left-click shortcut. |

---

## 2. Data structures

### 2.1. `SshConfig` — connection config (`ssh` crate)

Defined in `crates/core/src/ssh_config.rs` and consumed through the terminal session factory.
This is the input for `SshSession::connect()`.

```rust
use std::path::PathBuf;

/// SSH authentication method.
#[derive(Clone)]
pub enum SshAuthMethod {
    None,
    Password { password: SecretString },
    PrivateKey {
        key_path: PathBuf,
        passphrase: Option<SecretString>,
    },
}

/// SSH connection config — input for [`crate::SshSession::connect`].
#[derive(Debug, Clone)]
pub struct SshConfig {
    /// Hostname or IP.
    pub host: String,
    /// SSH port (default 22).
    pub port: u16,
    /// SSH username.
    pub username: String,
    /// Authentication method.
    pub auth: SshAuthMethod,
}
```

> **Note:** `SshConfig` holds credentials in zeroizing `SecretString` values.
> Do not serialize `SshConfig` to disk. Credentials exist only in memory during
> connection setup and are removed from the long-lived session configuration.

### 2.2. Extending `SshSession` (state) — add a `password` field?

**NO.** `SshSession` in `session_state.rs` (UI store) keeps its 4 fields:
`label, host, port, username`. The password is not stored — it's only an ephemeral
input for `SshConfig` when connecting.

```rust
// session_state.rs — UNCHANGED
pub struct SshSession {
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,  // None → the dialog will ask
}
```

### 2.3. `SshConnectParams` — bundle of connect info (UI crate)

An intermediate struct holding everything needed to open the dialog + connect, created
from `SshSession` when the user clicks.

```rust
/// Info needed to open the connect dialog + create an SshConfig.
/// Created from `SshSession` when the user clicks an item.
pub(crate) struct SshConnectParams {
    pub label: String,       // for the tab title + dialog title
    pub host: String,
    pub port: u16,
    pub username: Option<String>,  // None → the dialog asks for the username
}
```

---

## 3. Credential dialog branching logic

### 3.1. Decision table

| `SshSession.username` | Dialog shown | Fields |
|---|---|---|
| `None` (no username yet) | **Username + Password** | 2 inputs: username (text) + password (masked) |
| `Some(u)` (username present) | **Password only** | 1 input: password (masked), username shown read-only |

### 3.2. Pseudocode

```text
fn on_session_click(session: &SshSession):
    match session.username:
        None  → open_connect_dialog(session, ask_username=true)
        Some  → open_connect_dialog(session, ask_username=false)
```

### 3.3. Dialog title

- When asking for username + password: `"Connect to {label}"` (e.g. `"Connect to Production Server"`)
- When asking only for password: `"Connect to {label} ({username}@{host}:{port})"`

The subtitle in the dialog content shows the server info:
- `"ssh://{username}@{host}:{port}"` (when there's a username)
- `"ssh://{host}:{port}"` (when there's no username yet)

---

## 4. Dialog UI — Connect SSH

### 4.1. Dialog layout

```
┌─ Connect to Production Server ──────────────────────────┐
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │  ssh://ubuntu@10.0.0.1:22                        │   │  ← server info (read-only)
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Username *                          ← only shown when ask_username=true
│  ┌──────────────────────────────────────────────────┐   │
│  │  [input text]                                    │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Password *                                              │
│  ┌──────────────────────────────────────────────────┐   │
│  │  [••••••••••]                              [👁]  │   │  ← masked + mask_toggle
│  └──────────────────────────────────────────────────┘   │
│                                                          │
├──────────────────────────────────────────────────────────┤
│                              [Cancel]  [Connect]        │  ← footer, justify_end
└──────────────────────────────────────────────────────────┘
```

**When `ask_username = false`** (username present), the Username field is hidden,
the dialog is shorter:

```
┌─ Connect to Production Server (ubuntu@10.0.0.1:22) ─────┐
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │  ssh://ubuntu@10.0.0.1:22                        │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Password *                                              │
│  ┌──────────────────────────────────────────────────┐   │
│  │  [••••••••••]                              [👁]  │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
├──────────────────────────────────────────────────────────┤
│                              [Cancel]  [Connect]        │
└──────────────────────────────────────────────────────────┘
```

### 4.2. Footer — Connect + Cancel, right-aligned

`DialogFooter` (gpui-component) defaults to `h_flex().gap_2().justify_end()` —
so children auto-align **right**. Order: Cancel (left) → Connect (right).

```rust
.footer(
    DialogFooter::new()
        .child(
            DialogClose::new().child(
                Button::new("cancel")
                    .label("Cancel")
                    .outline(),
            ),
        )
        .child(
            DialogAction::new().child(
                Button::new("connect")
                    .label("Connect")
                    .primary(),
            ),
        ),
)
```

- **Cancel** = `DialogClose` → dispatches `CancelDialog` → closes the dialog, does
  nothing. `on_cancel` returns `true` (allows closing).
- **Connect** = `DialogAction` → dispatches `ConfirmDialog` → calls the `on_ok`
  closure. `on_ok` reads the input values, validates, creates an `SshConfig`, calls
  `connect_and_open_terminal()`. Returns `true` to close the dialog.

### 4.3. Password input — masked + mask_toggle

```rust
// InputState for the password — masked from the start.
let password_state = cx.new(|cx| {
    InputState::new(window, cx)
        .placeholder("Enter password")
        .masked(true)             // ← shows ••••••
});

// Input element — mask_toggle adds an eye-icon reveal/hide button.
Input::new(&password_state)
    .mask_toggle()                 // ← 👁 button toggles reveal
    .cleanable(true)               // ← × clear button
```

API reference (gpui-component):
- `InputState::masked(bool)` — `reference/.../input/state.rs:874`
- `Input::mask_toggle()` — `reference/.../input/input.rs:144`
- Example: `reference/.../stories/input_story.rs:73` (`.masked(true)` +
  `.placeholder("Enter your password...")`)

### 4.4. Username input (when `ask_username = true`)

```rust
let username_state = cx.new(|cx| {
    InputState::new(window, cx)
        .placeholder("e.g. root, ubuntu, admin")
});

Input::new(&username_state)  // plain text, not masked
```

### 4.5. Server info banner (read-only)

Shows server info above the fields, using `div` + text, not an input.
Helps the user confirm they're connecting to the right server.

```rust
// Server info banner
let server_info = match &params.username {
    Some(u) => format!("ssh://{}@{}:{}", u, params.host, params.port),
    None => format!("ssh://{}:{}", params.host, params.port),
};

div()
    .w_full()
    .px_3()
    .py_2()
    .rounded_md()
    .bg(theme.muted)
    .text_sm()
    .text_color(theme.muted_foreground)
    .child(server_info)
```

### 4.6. Validation

| Field | Condition | On failure |
|---|---|---|
| Username (if asked) | Not empty after trim | `window.push_notification("Username is required.")`, return `false` (don't close dialog) |
| Password | Not empty after trim | `window.push_notification("Password is required.")`, return `false` |

---

## 5. Connection flow — create SshSession + open tab

### 5.1. `on_ok` closure — when the user clicks Connect

```rust
.on_ok({
    let params = params.clone();
    let username_state = username_state.clone();  // None if not asked
    let password_state = password_state.clone();
    let dock_area = dock_area.clone();            // WeakEntity<DockArea>
    move |_, window, cx| {
        // 1. Read + validate inputs
        let username = match &username_state {
            Some(st) => {
                let u = st.read(cx).value().trim().to_string();
                if u.is_empty() {
                    window.push_notification("Username is required.", cx);
                    return false;
                }
                u
            }
            None => params.username.clone().unwrap_or_default(),
        };

        let password = password_state.read(cx).value().to_string();
        if password.is_empty() {
            window.push_notification("Password is required.", cx);
            return false;
        }

        // 2. Create SshConfig
        let cfg = SshConfig {
            host: params.host.clone(),
            port: params.port,
            username: username.clone(),
            auth: SshAuthMethod::Password { password },
        };

        // 3. (Optional) save the username back to the store if the user entered a new one
        if username_state.is_some() {
            // params.session_index → store.update(index, session with the new username)
            // Convenient for the next connect
        }

        // 4. Connect async + open tab
        cx.spawn_in(window, async move |_this, window| {
            // SshSession::connect is sync (block_on inside) — run on the
            // background executor so it doesn't block the UI.
            let result = window.background_executor().spawn(async move {
                oneterm_ssh::SshSession::connect(
                    cfg,
                    PtySize { rows: 24, cols: 80 },
                    10_000,  // scrollback
                )
            }).await;

            window.update(|window, cx| {
                match result {
                    Ok(session) => {
                        let panel = create_ssh_terminal_panel(
                            session,
                            &params.label,
                            window, cx,
                        );
                        add_terminal_to_dock(&dock_area, panel, window, cx);
                    }
                    Err(e) => {
                        window.push_notification(
                            format!("SSH connect failed: {e}"),
                            cx,
                        );
                    }
                }
            }).ok();
        }).detach();

        true  // close dialog
    }
})
```

### 5.2. Creating a TerminalPanel with an SSH session

`TerminalPanel` currently creates a `LocalSession` inside `new()`. We need a constructor
that accepts a `Box<dyn TerminalSession>` from outside (factory pattern — already noted as a
TODO in `panel.rs`).

```rust
impl TerminalPanel {
    /// Create a panel from an existing session (SSH or local).
    /// The session is already spawned/connected; the panel just wraps the view.
    pub fn from_session(
        session: Box<dyn TerminalSession>,
        title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let session_entity = cx.new(|_| session);
        let view = cx.new(|cx| LocalTerminalView::new(session_entity, window, cx));
        view.read(cx).focus_handle(cx).focus(window, cx);
        Self {
            view,
            tab_panel: None,
            is_active: false,
            tab_title: title.to_string(),  // ← new field
        }
    }

    pub fn from_session_entity(
        session: Box<dyn TerminalSession>,
        title: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::from_session(session, title, window, cx))
    }
}
```

**Changes to `TerminalPanel`:**
- Add a field `tab_title: String` (default `"Terminal"` for local).
- `title()` in `impl Panel` uses `self.tab_title` instead of hardcoding `"Terminal"`.

```rust
pub struct TerminalPanel {
    view: Entity<LocalTerminalView>,
    tab_panel: Option<WeakEntity<TabPanel>>,
    is_active: bool,
    tab_title: String,  // ← NEW
}

// In Panel::title()
.child(div()
    .flex_1()
    .overflow_hidden()
    .text_ellipsis()
    .whitespace_nowrap()
    .child(self.tab_title.clone()),  // ← instead of "Terminal"
```

### 5.3. Adding an SSH terminal tab to the DockArea

Use the same logic as `on_action_add_panel` (see `workspace/actions.rs`):

```rust
fn add_ssh_terminal_to_dock(
    dock_area: &WeakEntity<DockArea>,
    panel: Arc<dyn PanelView>,
    window: &mut Window,
    cx: &mut App,
) {
    // Check whether the center has any visible tab (handle the edge case where all tabs were closed).
    let center_empty = dock_area.read_with(cx, |dock, cx| {
        super::center_has_no_visible_panel(&dock.center(), cx)
    }).unwrap_or(false);

    if center_empty {
        let weak = dock_area.clone();
        let center = DockItem::v_split(
            vec![DockItem::tabs(vec![panel], &weak, window, cx)],
            &weak, window, cx,
        );
        dock_area.update(cx, |dock, cx| {
            dock.set_center(center, window, cx);
        }).ok();
    } else {
        dock_area.update(cx, |dock, cx| {
            dock.add_panel(panel, DockPlacement::Center, None, window, cx);
        }).ok();
    }
}
```

---

## 6. Integration into SessionPanel

### 6.1. Add a left-click handler to `render_session_row`

Currently `render_session_row` only has a `context_menu` (right-click). Add an `.on_click`
for left-click:

```rust
fn render_session_row(
    ix: usize,
    session: &SshSession,
    focus: &FocusHandle,
    cx: &App,
) -> impl IntoElement {
    // ... (keep existing code)

    div()
        .id(("session-row", ix))
        .w_full()
        .px_2()
        .py_1p5()
        .rounded_md()
        .cursor_pointer()
        .hover(|t| t.bg(theme.muted))
        // ← NEW: left-click → open the SSH session
        .on_click(move |_, window, cx| {
            let session = SshSessionStore::global(cx)
                .read(cx)
                .sessions()
                .get(ix)
                .cloned();
            if let Some(s) = session {
                open_connect_dialog(s, ix, window, cx);
            }
        })
        // ... (keep children + context_menu)
}
```

### 6.2. Update the "Open" context menu

The "Open" context menu currently just pushes a "not implemented" notification.
Replace it with a call to the same `open_connect_dialog`:

```rust
.item(PopupMenuItem::new("Open").on_click(move |_, window, cx| {
    let session = SshSessionStore::global(cx)
        .read(cx)
        .sessions()
        .get(ix)
        .cloned();
    if let Some(s) = session {
        open_connect_dialog(s, ix, window, cx);
    }
}))
```

### 6.3. `open_connect_dialog` — main function

Place it in `session_tabs/tabs.rs` (same file as `open_session_dialog`).
It needs a `WeakEntity<DockArea>` reference — get it from `SessionPanel` or a global.

```rust
/// Open the SSH connect dialog.
///
/// - `session`: the SSH session info from the store.
/// - `index`: its position in the store (to update the username if the user enters a new one).
///
/// Branching logic:
/// - `session.username = None` → the dialog asks for username + password.
/// - `session.username = Some` → the dialog asks only for the password.
fn open_connect_dialog(
    session: SshSession,
    index: usize,
    window: &mut Window,
    cx: &mut App,
) {
    let ask_username = session.username.is_none();

    // Dialog title
    let title = if ask_username {
        format!("Connect to {}", session.label)
    } else {
        let u = session.username.as_deref().unwrap_or("");
        format!("Connect to {} ({}@{}:{})", session.label, u, session.host, session.port)
    };

    // Server info banner text
    let server_info = match &session.username {
        Some(u) => format!("ssh://{}@{}:{}", u, session.host, session.port),
        None => format!("ssh://{}:{}", session.host, session.port),
    };

    // Password state — always needed, masked.
    let password_state = cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder("Enter password")
            .masked(true)
    });

    // Username state — only create when we need to ask.
    let username_state: Option<Entity<InputState>> = if ask_username {
        Some(cx.new(|cx| {
            InputState::new(window, cx).placeholder("e.g. root, ubuntu, admin")
        }))
    } else {
        None
    };

    // DockArea weak ref — needed to add a terminal tab after connecting.
    // Get it via AppState or pass it in from SessionPanel.
    let dock_area = get_dock_area(cx);  // helper — see §6.4

    // Clone for the on_ok closure
    let password_ok = password_state.clone();
    let username_ok = username_state.clone();
    let session_ok = session.clone();
    let dock_area_ok = dock_area.clone();

    window.open_dialog(cx, move |dialog, _window, _cx| {
        dialog
            .title(title.as_str())  // NOTE: clone title into the closure
            .w(px(440.))
            .content({
                let server_info = server_info.clone();
                let username_state = username_state.clone();
                let password_state = password_state.clone();
                move |content, _window, cx| {
                    let theme = cx.theme();
                    content
                        // Server info banner
                        .child(
                            div()
                                .w_full()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(theme.muted)
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(server_info),
                        )
                        // Username field (only when ask_username)
                        .when_some(username_state, |content, st| {
                            content.child(field("Username", true, Input::new(&st), cx))
                        })
                        // Password field (always)
                        .child(
                            v_flex()
                                .gap_1()
                                .w_full()
                                .child(
                                    h_flex()
                                        .gap_1()
                                        .text_sm()
                                        .child(SharedString::from("Password"))
                                        .child(div().text_color(cx.theme().danger).child("*")),
                                )
                                .child(
                                    Input::new(&password_state)
                                        .mask_toggle()
                                        .cleanable(true),
                                ),
                        )
                }
            })
            .footer(
                DialogFooter::new()
                    .child(
                        DialogClose::new().child(
                            Button::new("cancel").label("Cancel").outline(),
                        ),
                    )
                    .child(
                        DialogAction::new().child(
                            Button::new("connect").label("Connect").primary(),
                        ),
                    ),
            )
            .button_props(
                DialogButtonProps::default()
                    .on_cancel(|_, _, _| true)
                    .on_ok(move |_, window, cx| {
                        // Read username
                        let username = match &username_ok {
                            Some(st) => {
                                let u = st.read(cx).value().trim().to_string();
                                if u.is_empty() {
                                    window.push_notification("Username is required.", cx);
                                    return false;
                                }
                                u
                            }
                            None => session_ok.username.clone().unwrap_or_default(),
                        };

                        // Read password
                        let password = password_ok.read(cx).value().to_string();
                        if password.is_empty() {
                            window.push_notification("Password is required.", cx);
                            return false;
                        }

                        // (Optional) save the username to the store if the user entered a new one
                        if username_ok.is_some() {
                            let mut updated = session_ok.clone();
                            updated.username = Some(username.clone());
                            SshSessionStore::global(cx).update(cx, |s, cx| {
                                s.update(index, updated, cx);
                            });
                        }

                        // Create SshConfig
                        let cfg = SshConfig {
                            host: session_ok.host.clone(),
                            port: session_ok.port,
                            username,
                            auth: SshAuthMethod::Password { password },
                        };

                        let label = session_ok.label.clone();
                        let dock_area = dock_area_ok.clone();

                        // Connect async
                        cx.spawn_in(window, async move |_this, window| {
                            let result = window.background_executor().spawn(async move {
                                oneterm_ssh::SshSession::connect(
                                    cfg,
                                    PtySize { rows: 24, cols: 80 },
                                    10_000,
                                )
                            }).await;

                            _ = window.update(|window, cx| {
                                match result {
                                    Ok(session) => {
                                        let panel: Arc<dyn PanelView> = Arc::new(
                                            TerminalPanel::from_session_entity(
                                                Box::new(session) as Box<dyn TerminalSession>,
                                                &label,
                                                window, cx,
                                            ),
                                        );
                                        add_ssh_terminal_to_dock(
                                            &dock_area, panel, window, cx,
                                        );
                                        window.push_notification(
                                            format!("Connected to \"{label}\"."),
                                            cx,
                                        );
                                    }
                                    Err(e) => {
                                        window.push_notification(
                                            format!("SSH connect failed: {e}"),
                                            cx,
                                        );
                                    }
                                }
                            });
                        }).detach();

                        true  // close dialog
                    }),
            )
    });
}
```

### 6.4. Getting `WeakEntity<DockArea>` inside the dialog

`SessionPanel` doesn't currently hold a `DockArea` reference. Two options:

**Option A (recommended): store `WeakEntity<DockArea>` in `SessionPanel`**

```rust
pub struct SessionPanel {
    focus_handle: FocusHandle,
    store: Entity<SshSessionStore>,
    dock_area: WeakEntity<DockArea>,  // ← NEW
}

impl SessionPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = SshSessionStore::global(cx);
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            focus_handle: cx.focus_handle(),
            store,
            dock_area: cx.dock_area(),  // helper — see note
        }
    }
}
```

> **Note on `cx.dock_area()`:** GPUI has no direct API to get the DockArea from a
> Context. You need to pass `WeakEntity<DockArea>` into `SessionPanel::new` from
> `register_panel` or `reset_default_layout`. Reference:
> `DockItem::tab(SessionPanel::new_entity(window, cx), ...)` — add a `dock_area`
> param.

**Option B: store `WeakEntity<DockArea>` in the global `AppState`**

```rust
pub struct AppState {
    pub dock_area: Option<WeakEntity<DockArea>>,  // set after DockArea is created
}
```

Set in `OneTermWorkspace::new`:
```rust
AppState::global(cx).update(cx, |s, cx| {
    s.dock_area = Some(dock_area.downgrade());
    cx.notify();
});
```

Read in `open_connect_dialog`:
```rust
fn get_dock_area(cx: &App) -> WeakEntity<DockArea> {
    AppState::global(cx).read(cx).dock_area.clone()
        .expect("dock_area not initialized")
}
```

> **Recommend Option B** — simpler, doesn't require changing the
> `SessionPanel::new` + `register_panel` + `reset_default_layout` signatures.

---

## 7. File structure

### 7.1. New / changed files

| File | Status | Responsibility |
|---|---|---|
| `crates/ssh/src/config.rs` | **NEW** | `SshConfig` + `SshAuthMethod` structs |
| `crates/ssh/src/lib.rs` | **Edit** | Re-export `config::*` |
| `crates/ssh/src/session.rs` | **NEW** (roadmap) | `SshSession::connect()` — see `terminal-backend.md` §7 |
| `crates/ui/src/views/session_tabs/tabs.rs` | **Edit** | Add `open_connect_dialog()` + left-click handler + update the "Open" context menu |
| `crates/ui/src/views/terminal/panel.rs` | **Edit** | Add `tab_title` field + `from_session()` / `from_session_entity()` constructor |
| `crates/ui/src/state/app_state.rs` | **Edit** | Add `dock_area: Option<WeakEntity<DockArea>>` |
| `crates/ui/src/layout/workspace/mod.rs` | **Edit** | Set `AppState.dock_area` after creating the DockArea |

### 7.2. Dependency changes

```toml
# crates/ui/Cargo.toml — add the ssh dependency
[dependencies]
oneterm-ssh = { path = "../ssh" }
```

> ⚠️ **Dependency rule**: `docs/agents/structure.md` says `ui` **does not** import
> `ssh`/`local` directly — it calls through the `TerminalSession` trait. However, to
> create an `SshSession` you need to call `SshSession::connect()` (a factory). Two solutions:
>
> **Solution 1 (recommended MVP):** Allow `ui` to depend on `ssh` to call
> `SshSession::connect()`. The session returns `Box<dyn TerminalSession>` — the UI only
> uses the trait, unaware of internals. This is the pattern `panel.rs` **already uses** with
> `oneterm_local::LocalSession`. Update the rule: `ui → {core, local, ssh}`.
>
> **Solution 2 (clean architecture):** Push the factory into the `app` crate. `app`
> creates `Box<dyn TerminalSession>` then passes it into `ui`. `ui` stays a leaf (only
> `core`). Needs an extra callback/registry pattern: `ui` calls `app` via a trait when it
> needs to connect. More complex — defer.

> **MVP decision:** Solution 1 — update `structure.md`'s dependency rule
> to `ui → {core, local, ssh}`. There's precedent (`panel.rs` imports
> `oneterm_local`).

---

## 8. Implementation checklist

### Step 1 — `ssh` crate: `SshConfig` + `SshAuthMethod`

- [ ] Create `crates/ssh/src/config.rs` — define `SshConfig` + `SshAuthMethod`.
- [ ] Update `crates/ssh/src/lib.rs` — `pub mod config;` + re-export.
- [ ] Update `crates/ui/Cargo.toml` — add the `oneterm-ssh` dependency.

### Step 2 — `TerminalPanel`: support an external session

- [ ] Add field `tab_title: String` to `TerminalPanel`.
- [ ] `new()` — set `tab_title = "Terminal"`.
- [ ] Add `from_session(session, title, window, cx)` + `from_session_entity(...)`.
- [ ] `Panel::title()` — use `self.tab_title` instead of hardcoding `"Terminal"`.
- [ ] `register_panel("terminal", ...)` — keep `new_entity` (local default).

### Step 3 — `AppState`: store `WeakEntity<DockArea>`

- [ ] Add field `dock_area: Option<WeakEntity<DockArea>>` to `AppState`.
- [ ] In `OneTermWorkspace::new` — set `AppState.dock_area` after creating the DockArea.

### Step 4 — `open_connect_dialog` in `session_tabs/tabs.rs`

- [ ] Add `open_connect_dialog(session, index, window, cx)` function.
- [ ] Branching logic: `ask_username = session.username.is_none()`.
- [ ] Dialog title + server info banner per `ask_username`.
- [ ] Username input (only when `ask_username = true`).
- [ ] Password input: `InputState::masked(true)` + `Input::mask_toggle()`.
- [ ] Footer: `DialogFooter` → Cancel (`DialogClose`) + Connect (`DialogAction`), `justify_end`.
- [ ] `on_ok`: validate → create `SshConfig` → (optional) save username → connect async.
- [ ] Connect succeeds → `TerminalPanel::from_session_entity` → add to dock.
- [ ] Connect fails → `window.push_notification`.

### Step 5 — Integrate the click handler into `render_session_row`

- [ ] Add `.on_click` left-click → `open_connect_dialog`.
- [ ] Update the "Open" context menu → call `open_connect_dialog` (instead of
      `push_notification("not implemented")`).

### Step 6 — `ssh` crate: `SshSession::connect()` (roadmap terminal-backend §7)

- [ ] `crates/ssh/src/session.rs` — russh client + hidden tokio runtime.
- [ ] `crates/ssh/src/listener.rs` — `SshListener: EventListener`.
- [ ] `crates/ssh/src/auth.rs` — password auth (MVP).
- [ ] `impl TerminalSession for SshSession`.
- [ ] Re-export `SshSession`, `PtySize` via `lib.rs`.

### Step 7 — Update docs

- [ ] Update `docs/agents/structure.md` — dependency rule `ui → {core, local, ssh}`.
- [ ] Update `AGENTS.md` — SSH roadmap check.

### Step 8 — Quality gate

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo build --workspace`
- [ ] `cargo run -p app` — click an SSH session item → the dialog appears → enter
      credentials → a terminal tab opens (needs a real SSH server for end-to-end testing).

---

## 9. Edge cases & notes

### 9.1. Reconnect / duplicate session

When the user clicks the same session item multiple times → opens multiple separate SSH tabs
(each tab = an independent connection). This is the desired behavior — like Tabby, Termius.
No connection caching/reuse.

### 9.2. Connection timeout

`SshSession::connect` should have a timeout (e.g. 30s). If the server is unreachable →
`Err` → `push_notification("SSH connect failed: connection timed out")`.
See `terminal-backend.md` §13 (risks).

### 9.3. Host key verification

MVP: accept any host key (NOT recommended for production). Roadmap: add
known_hosts + an accept/reject prompt (see `terminal-backend.md` §8, step 8).
When known_hosts is implemented, the connect dialog needs an extra step:
- Unknown host key → dialog "Accept host key? (fingerprint: xx:xx:...)"
- Host key mismatch → dialog "WARNING: host key changed!"

### 9.4. Password input — don't log it

`InputState::masked(true)` ensures the text shows `•••••`. However, make sure the password
isn't logged to console/tracing. Do NOT `tracing::info!` the raw password. In the `SshConfig`
Debug impl, mask the password:

```rust
impl Debug for SshConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("SshConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth", &"***")  // ← mask
            .finish()
    }
}
```

### 9.5. Keyboard focus in the dialog

- When the dialog opens, focus the first field (username if asked, password if not).
- Enter in the password field → trigger Connect (call `on_ok`).
- Esc → trigger Cancel (call `on_cancel`).

> gpui-component Dialog handles Esc itself (dispatches `CancelDialog`). Enter on the
> `DialogAction` button → dispatches `ConfirmDialog`. You need to bind the Enter key in
> the input field → dispatch `ConfirmDialog` (see the Dialog API).

### 9.6. Authentication status

The accepted current behavior is defined in [`docs/ssh-authentication.md`](ssh-authentication.md). The backend supports no-auth, password, and private-key authentication; the saved-session and Quick Connect UI expose password and private-key choices. SSH-agent authentication remains a roadmap item and is not exposed by `SshAuthMethod` until the backend can support it end to end.