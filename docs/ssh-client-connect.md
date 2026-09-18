# SSH Client Connect Design — OneTerm

> **Status:** Historical design record. For current crate ownership and paths, see [`docs/architecture.md`](architecture.md). For the accepted password and private-key authentication behavior, see [`docs/ssh-authentication.md`](ssh-authentication.md).
>
> **Current state (review refresh 2026-08, Phase 2).** The shipped code in
> `crates/session-ui/` differs from the sketches below in three ways:
>
> - **Sessions are addressed by a stable id, not a `Vec` index.** `ssh_session.json`
>   is schema v2: `{ "schema_version": 2, "next_session_id": N, "sessions": [{ "id": 1, … }] }`.
>   `SshSessionStore` exposes `sessions() -> &[SshSessionEntry { id, session }]`,
>   `get(id)`, `add(session) -> id`, `update(id, session)`, `remove(id)`; tree item ids
>   are `session:{id}`; `open_connect_dialog(session, id, …)` and
>   `open_session_dialog(…, Some((id, session)))` capture the id, so a session removed
>   or reordered while a dialog is open is never retargeted. Loading a v0 (bare array)
>   or v1 file assigns ids by position and re-saves the file as v2; a v2 file whose rows
>   lack or repeat an `id` is repaired the same way. No field is dropped
>   (`crates/session-ui/src/session_state.rs`, fixtures under
>   `crates/session-ui/tests/fixtures/persistence/`). Saved sessions also persist a
>   terminal-logging override (`inherit`, `on`, or `off`); missing values inherit the
>   global SSH setting. See [`terminal-logging.md`](terminal-logging.md).
> - **One `user[@host[:port]]` parser** (`common::parse_user_host_port`, used by the
>   connect and quick-connect dialogs) with an explicit policy: `user` alone leaves
>   host/port to the caller's defaults; IPv6 hosts are `[addr]`, `[addr]:port` or a
>   bare address with several colons (no port); an invalid port (`:abc`, `:0`,
>   `:70000`), an empty user (`@host`) or an empty host (`user@`) is rejected with a
>   corrective notification — never a silent default, never folded into the host name.
>   `common::parse_port` applies the same `1..=65535` rule to the Port fields; an empty
>   Port field means 22. In quick-connect the typed Host / Port fields win over the parts
>   parsed from the Username field.
> - **Dialogs share `oneterm_state::form_dialog::FormDialog`** (title, content builder,
>   Cancel + confirm footer, Enter submits, Escape/Cancel hook) and `labelled_field`;
>   the connect dialogs plug their stateful `ConnectButton` in via `confirm_element`.

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

A session is *created* from the `+` in the right dock's "Session" header, from the context menu
on the blank area below the list, or from the tree's own "New Session" row — all three open the
same full session dialog (§6.5). Since `US-0129` there is a fourth: **Duplicate Saved Session**
on a session's own context menu, which writes the copy first and opens that same dialog on it,
so the user renames a session that already exists instead of filling an empty form.

Since `IN-0033` the same saved sessions are also listed in the centre tab bar's `+` (New
Terminal) dropdown, and picking one there enters this flow at exactly the same point with the
same `SshSessionId`. Since `US-0114` that dropdown also closes with both connect entry
points, each named after the dialog it opens: "Quick Connect..." reaches the quick-connect
dialog (§4, the `NewSession` action and its key binding), and "New Saved Session..." reaches
the full "New SSH Session" dialog — the same dialog `SessionPanel`'s tree opens, reached
through the `WorkspaceCommands::open_new_saved_session_dialog` fn pointer rather than a crate
edge. `WorkspaceCommands::open_quick_connect_dialog` (until `US-0114` named
`open_new_session_dialog`) is the other one. Everything below the dialog — credential branching, jump chain, host-key
approval, and where the connected tab lands — is shared, so the rest of this document describes
both surfaces.

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
| 1 | **Password is NOT persisted** to `ssh_session.json` | Security — the password lives only in RAM during the session, never written to disk. Since `US-0118` it also survives a failed connect **in the open dialog's own state**, so a retry costs no re-typing; closing the dialog drops it, and it still reaches no store, no config file and no log ([`DEC-0001`](decisions/0001-ssh-key-secret-persistence.md)). |
| 2 | **Username is persisted** to `ssh_session.json` (the field already exists) | Convenience — the user only enters a password next time. The username is less sensitive than the password. |
| 3 | **Use `LocalTerminalView`** for SSH too (via `dyn TerminalSession`) | The view is already backend-agnostic — it only needs `Entity<Box<dyn TerminalSession>>`. No separate `SshTerminalView` needed. |
| 4 | **Dialog uses `window.open_dialog`** (gpui-component Dialog) | Matches the pattern already used for the "New/Edit SSH Session" dialog in `session_tabs/tabs.rs`. |
| 5 | **Password field uses `InputState::masked(true)` + `.mask_toggle()`** | Shows `•••••`, with an eye-icon button to reveal/hide. API already available in gpui-component. |
| 6 | **Footer: Cancel (left) + Connect (right), right-aligned** | `DialogFooter` defaults to `justify_end` → buttons auto-align right. Matches the requirement. |
| 7 | **Connect runs async** — the dialog closes on success, the connection runs in the background | Avoids blocking the UI. On failure the dialog **stays open** with the form intact and shows the error inline as well as pushing the notification (`US-0118`, §4.7); connect itself is unchanged. |
| 8 | **Left-click = Open**, right-click keeps the context menu (Open / Properties / New Session / Delete, §6.5). Since `IN-0033` the centre tab bar's `+` menu is a second surface that opens the same dialog by session id | Keeps the current context-menu behavior, adds a left-click shortcut. The `+` menu reuses `open_connect_dialog` rather than duplicating the connect path, so the two surfaces cannot drift. |
| 9 | **Saved-session logging is tri-state** (`inherit` / `on` / `off`) | A saved session can use the global SSH policy or explicitly force either outcome; see [`DEC-0003`](decisions/DEC-0003-define-terminal-logging-capture-and-override-semantics.md). |

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

### 4.7. After a failed connect (`US-0118`)

The dialog stays open — it always did — and now it is usable when it does. This holds for
both credential dialogs (Connect SSH and SSH Quick Connect) and for the Duplicate dialog,
because all three share `SshAuthForm` and `connect_ssh_session`.

- **The credential fields keep what the user typed.** `SshAuthForm::take_auth` used to blank
  the password and the passphrase as it read them, so a failure 20 s later left an empty box, a
  re-enabled button and no explanation. It no longer clears; a successful connect closes the
  dialog, and closing it drops the form and its `InputState` entities. `DEC-0001` and §1.3
  decision 1 are untouched: the secret reaches no store, no config file and no log (checked on
  disk after a failed attempt with Save ticked), and an open modal's own state is not
  persistence.
- **One path has no inline error: an unknown host key.** `AppError::HostKeyUnknown` closes the
  dialog and opens the host-key prompt (§9.3), so a failure on the *retried* connect has no
  dialog to echo into and only the toast survives. That is the intended shape — the retry is a
  new attempt from a different surface — and it is why `connect_ssh_session` sets the inline
  error only on the arm that leaves the dialog open.
- **The failure is shown inline as well as in the toast.** `SshConnectRequest::on_failed`
  carries the message the notification shows — the same `SharedString`, so the two cannot
  drift — to an `InlineError` the dialog body renders under its fields. It clears when the user
  presses Connect again and on `InputEvent::Change` from any field it watches, so a corrected
  form never stands beside a stale error. This is a second channel beside the notification, not
  a replacement: §1.3 decision 7 stands and connect is still asynchronous.
- **A ticked "Save to SSH Sessions" says what happened.** `CORR-54` holds — a quick-connect
  session is saved only once the connection is authenticated — but the drop is no longer
  silent: the inline error carries a second line saying the session was not saved, that a
  session is saved once its connection succeeds, and that the tick is still on. The next
  successful attempt saves it.
- **The Group combobox commits with Enter.** Typing a name no group matches and pressing Enter
  creates it, selects it, and closes the dropdown; with a row on screen Enter still belongs to
  the list. The decision is `group_combo::group_commit(query, match_count)`. The handler is a
  **capture**-phase `Confirm` listener, because gpui stops an action after the first
  bubble-phase listener and the list would otherwise swallow the key.
  Creating clears the kit's **own** search input through `ComboboxState::set_query`, which
  re-runs the search and so refreshes every derived value at once. Clearing a private copy of
  the query instead left the box showing the spent text while the empty area claimed there were
  no groups and the footer offered to create nothing — and the next keystroke appended, so
  "Lab" followed by "inf" created a group called "Labinf".
  The no-match area names the group it would create instead of showing a bare icon, and it
  distinguishes "no groups exist" from "none match what you typed"
  (`group_combo::empty_message`).

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

### 6.5. Entry points and menus as they stand (`US-0119`)

The sections above describe how the panel was built. What it offers today:

| Surface | Action |
|---|---|
| The `+` in the "Session" section header (`SessionPanel::title_suffix`, drawn by `SshClientPanel::render_session_header`) | The full New SSH Session dialog, `open_session_dialog(window, cx, None)`. |
| Right-click the blank area below the list | One row, "New Session" — the same dialog. |
| The empty-list hint | Unchanged: "No SSH session yet. Right-click → New Session." |
| Double-click a session, or its "Open" row | `open_connect_dialog` for that session. |
| Right-click a session | `Open`, `Properties`, `Duplicate Saved Session`, `Move to Group ▸`, separator, `New Session`, separator, `Delete`. |
| Right-click a group | `Rename Group…`, separator, `New Session`. |

Three rules hold across them:

- **One dialog.** Every "new session" surface — the header `+`, the blank-area menu, the
  tree's own menu and the centre tab bar's `+` (§1.1) — calls `open_session_dialog`. There is
  no second new-session dialog to drift.
- **The global action is not in the first slot** of an item menu, where a misclick lands.
- **Delete confirms and is styled destructive.** It is last, behind its own separator, drawn in
  the theme's danger colour, and it opens the same confirmation the rebindable `DeleteSession`
  action opens — one function, `panel::confirm_delete_session`, so the two cannot diverge. What
  it takes from the SFTP browser is the **confirmation**: the thing being deleted named in the
  question and a danger confirm button. SFTP's menu *row* is not red; this one is, which is a
  deliberate step further and leaves the two menus differing on that point.

**Duplicate Saved Session and Move to Group (`US-0129`).** Both act on the right-clicked
session and both are ordinary store writes — `ssh_session.json` stays at `schema_version: 2`
and gains no field.

- **Duplicate Saved Session** copies the source's `SshSession` whole (`session.clone()`, so a
  field added later cannot be forgotten), replaces only the label, gives it the next id from
  `next_session_id` — never the source's, never a reused one — and inserts it immediately after
  its source in store order. The label is `"<label> (copy)"`, or `"(copy 2)"`, `"(copy 3)"`, …
  when an earlier candidate is already a label in the store; the rule appends to whatever label
  it is handed, so duplicating a copy gives `prod (copy) (copy)`. It then opens the Properties
  dialog on the copy so it can be renamed, and **connects nothing**. The copy is **saved before
  that dialog opens**, and the dialog's Cancel keeps it: Cancel discards the edits, not the
  duplicate — which is what makes the row "Duplicate" rather than "New from…". Undoing a
  duplicate means deleting the copy. The row is not called
  "Duplicate" because a terminal tab's context menu already has a row by that name
  (`US-0116`) which reopens a *running* connection through `oneterm_core::SshDuplicateConfig`;
  the two share no code and no data.
  Note that the tree sorts alphabetically inside a group, so "immediately after its source" is
  a statement about `ssh_session.json` and the `+` menu, which follow store order — not about
  where the copy appears in the right dock.
- **Move to Group ▸** is a submenu listing `No group`, then the groups already in use
  (`session_dialog::existing_group_names`, the same list the dialog's Group combobox offers),
  with the session's current group checked, then `New group…`. Choosing one rewrites that one
  session's `group` — trimmed, blank meaning ungrouped, the convention `rename_group` and the
  tree builder already share — and the tree re-sorts. The submenu offers only names that
  already exist, so it cannot create `Infra` beside `infra`; `New group…` hands that job to the
  dialog, opened on the session with the initial focus in the Group field (§6.6).
  The submenu is built when the parent menu is built, so it reads the store at right-click
  time. It is attached with `PopupMenuItem::submenu` over a `PopupMenu::build` rather than
  `PopupMenu::submenu`, because `Tree::context_menu` hands its builder a `Context<TreeState>`
  and not a `Context<PopupMenu>`; the kit supports that path and wires the parent link on the
  parent menu's next render.

The list container and each tree row both carry a context menu. Both hitboxes are hovered over
a row and gpui runs the container's handler first, so the container's builder checks a flag the
row's right mouse-down sets and returns an empty menu — which renders nothing — when the click
landed on a row. See `SessionPanel::row_was_right_clicked`. The container's menu covers the
empty-list state too, so the empty-state element carries none of its own.

### 6.6. The New / Edit SSH Session dialog (`US-0120`)

Top to bottom: **Label**, **Color**, **Host**, **Port**, **Username**, **Authentication**,
the **Advanced** disclosure, **Group**, **Logging**.

- **Advanced** folds away Jump host, agent forwarding and port forwards — most of the form's
  height, and fields most sessions never use. It is collapsed for a new session and **open**
  when the session being edited already sets any of them
  (`session_dialog::advanced_is_configured`): a user who set a jump host and then sees no jump
  host concludes it was lost.
  It is a **button**, not a styled row: it is the only route to those three fields, and all
  three were plain Tab stops before they were folded away, so a `div` with a click handler would
  have put them out of a keyboard user's reach entirely. It is a tab stop, announces its state,
  and toggles on **Space**. Not Enter: the dialog binds Enter to submit, and a keymap binding is
  dispatched before any element's key listener, so Enter never reaches it — exactly as for
  Browse, Cancel and Save in the same dialog.
  Save validates the forwards and the jump chain whether or not they are on screen, so a
  refused Save **opens the disclosure and focuses the offending field** before it shows the
  message; a message about a basic field leaves the disclosure alone
  (`session_dialog::reveals_advanced`).
- **Color** is a labelled row of eight swatches — `US-0110`'s `#56B6C2` default first, then
  theme colours — followed by the full picker, whose trigger carries the words "Custom…" so the
  text itself opens it. `session_dialog::swatch_colors` is the one place the eight are defined:
  the kit exposes no accessor for the picker's own featured row, only the
  `ColorPicker::featured_colors` setter, so the row is defined here and handed to the picker,
  and the eight swatches and the eight along the top of the popup are the same eight. A swatch
  writes through the same `ColorPickerState` the picker writes and Save still stores
  `state.value().to_hex()`, so both produce exactly the value `session_color_hex` already
  accepted and the tree and the `+` menu cannot disagree.
- **Initial focus** is normally left where the dialog puts it. One caller asks for it: the
  tree menu's "Move to Group ▸ New group…" opens this dialog with `focus_group = true`, which
  puts the caret in the Group combobox through `FormDialog::on_render` and
  `common::defer_initial_focus_once` — the same deferred-once helper the connect and
  quick-connect dialogs use, because focus can only be moved once the dialog exists. That row
  is a request to type a group name, so the caret belongs in the field that takes one
  (`US-0129`).
- **The body scrolls** when it outgrows the window. That belongs to `FormDialog`
  (`crates/state/src/form_dialog.rs`), not to this dialog: the footer sits outside the capped
  box, so Cancel and Save are reachable at any window height, and a form that fits keeps its
  natural height and shows no scrollbar.

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

### 9.2. Connection timeout, phases and cancellation

`crates/ssh/src/session.rs::connect` runs every step through `ConnectPhases`:
each step is one `oneterm_core::ConnectPhase` (`Transport` → `Authentication` →
`ChannelOpen` → `PtyRequest` → `ShellRequest` → `ShellIntegration` → `SftpSetup`)
awaited under a 20 s per-phase deadline, and the whole attempt under a 60 s
deadline. With jump hosts (§9.8) the `Transport` and `Authentication` pair runs
once per hop before the target's. Failures are typed (ARCH-06):

| Outcome | Error |
|---|---|
| Transport/protocol failure or timeout in a step | `AppError::Connect { phase, message }` — the UI shows `Display` (`SSH <phase> failed: <message>`). |
| Server rejected authentication | `AppError::Connect { phase: Authentication, message: "rejected by the server; the server accepts: …" }`. |
| A jump hop failed | The hop's `Connect` error with its message prefixed `jump host user@host:port: …` (`route::hop_error`); host-key errors and `Cancelled` are passed through unchanged because they already name the hop or belong to no hop. |
| Host-key problems | `AppError::HostKeyUnknown` / `AppError::HostKeyChanged` (see §9.3). |
| User pressed Cancel | `AppError::Cancelled` — `ConnectionCancellation::cancelled()` is a waker-driven future, so a phase in flight is woken immediately instead of polled every 25 ms (PERF-22). |

A failure reaches the user twice, from one value: the bottom-right notification, and — while
the dialog is still open — an inline block under its fields (`US-0118`, §4.7). Both render the
same `SharedString`, handed to the dialog through `SshConnectRequest::on_failed`.

The reporting text is built in one place,
`crates/session-ui/src/common.rs::connect_failure_message` (`BUG-0068`): the
variants above that already **name SSH and what failed** — `Connect`
(`SSH <phase> failed: …`), `HostKeyChanged` (`SSH host key changed for …`) and
`HostKeyUnknown` (`Unknown SSH host key for …`, which names SSH without leading
with it) — are shown verbatim, so the subject appears once and the failing phase
stays visible; every other error (`Cancelled`, `Io`, `Other`) is prefixed
`SSH connect failed: …` so it never reaches the user as a bare "operation
cancelled".

The `HostKeyUnknown` arm is defensive: the only caller matches that variant first
and opens the **Unknown SSH Host Key** dialog (§9.3), so it never reaches the
notification from here. It is kept so the function is total over the variants a
connect can produce, rather than correct only by the order of the arms above it.

Blocking work on the connect path (`known_hosts` read/append in
`check_server_key`, private-key loading/decryption) runs on
`tokio::task::spawn_blocking` so the two shared runtime workers keep serving the
other sessions (CORR-17).

### 9.3. Host key verification

Host keys are verified fail-closed against the OpenSSH `known_hosts` file
(`~/.ssh/known_hosts`) by `crates/ssh/src/handler.rs` (`SshClientHandler`).
Every entry recorded for `host` (`[host]:port` when the port is not 22, hashed
entries included) is loaded and compared with the key the server presents:

| known_hosts state for the host | Result |
|---|---|
| An entry matches the presented key exactly | Trusted, connection proceeds. |
| No entry for the host | `AppError::HostKeyUnknown` (algorithm + SHA-256 fingerprint). The connect UI shows an "accept host key?" dialog; approving retries with `HostKeyPolicy::AcceptNewFingerprint(fingerprint)`, and the handler learns the key only when the presented fingerprint equals the approved one. |
| An entry with the **same algorithm** but a different key | `AppError::HostKeyChanged` — refused, never approvable from the UI; the user must remove the stale entry by hand. |
| Entries exist but **only for other algorithms** (for example the host is known by `ssh-ed25519` and the server presents `ecdsa-sha2-nistp256`) | `AppError::HostKeyChanged` as well (`SshHandlerError::HostKeyAlgorithmMismatch`). A man-in-the-middle can always present a key type the client has never recorded, so this case must not fall through to the friendly first-use prompt. |
| `known_hosts` unreadable / malformed entry for the host | Key-store error, connection refused. |
| The server presents an **OpenSSH host certificate** instead of a bare key | `SshHandlerError::HostCertificate` → `AppError::Connect` — refused as an ordinary connect failure, with **no host-key dialog at all**. The message names host certificates as unsupported and carries the certified key's SHA-256 fingerprint prefixed `cert:`. It is deliberately *not* `HostKeyUnknown`: that variant opens the first-use "trust this host key?" prompt, and approving a certificate could never succeed (the arm returns before any policy check), so the user would be shown an Accept button that only fails again. OneTerm has no certificate-authority trust store and `known_hosts` records bare keys, so there is nothing to approve *against*. This cannot arise with a conforming server: OneTerm advertises no `*-cert-v01@openssh.com` host-key algorithm (russh's `Preferred::host_key_certificates` is empty by default and OneTerm builds no custom `Preferred`), so a server has no way to offer one. The refusal exists so the fail-closed rule holds even against a non-conforming server, and is proved by `us0095_verify_tests::a_host_certificate_is_refused_even_when_its_inner_key_is_trusted` — which pre-records the certificate's inner key in `known_hosts` and is still refused. |

To keep the algorithm-mismatch rule from firing on legitimate servers that
gained new key types after the first connection, the client's preferred
host-key algorithm list is reordered before key exchange so every algorithm
already recorded for the host is offered first
(`SshClientHandler::preferred_key_algorithms` → `client::Config.preferred.key`).
Servers pick the first client-preferred type they hold, so a host known only by
its RSA key keeps presenting RSA. If a server dropped the recorded key type
entirely, the mismatch is reported and the user removes the stale entry.

Connections also send `keepalive@openssh.com` requests every 30 s and disconnect
after 3 unanswered requests (`KEEPALIVE_INTERVAL` / `KEEPALIVE_MAX` in
`crates/ssh/src/session.rs`) so a dead peer or a dropped NAT mapping surfaces as
a closed session instead of a tab that hangs forever.

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

The accepted current behavior is defined in [`docs/ssh-authentication.md`](ssh-authentication.md). The backend supports no-auth, password, private-key, and SSH-agent authentication (`SshAuthMethod::{None, Password, PrivateKey, Agent}`); the saved-session, Quick Connect, and connect dialogs expose all three user choices. Every hop of a jump-host route authenticates with its own method (§9.8).

### 9.7. Remote shell environment — COLORTERM

The remote shell must see `COLORTERM=truecolor` (and `TERM_PROGRAM=OneTerm`) so truecolor-aware CLIs render correctly, matching the local-shell contract in `oneterm_core`'s `base_env()`. Because sshd starts a fresh login environment, OneTerm applies two layers (`crates/ssh/src/session.rs`):

1. **SSH `env` requests (RFC 4254 §6.4)** — after `request_pty` and before `request_shell`, `request_remote_shell_env` sends `COLORTERM=truecolor` and `TERM_PROGRAM=OneTerm` with `want_reply = false`. sshd only honors these when its `AcceptEnv` allows the variables; default OpenSSH accepts only `LANG LC_*` and silently drops the rest, which is harmless.
2. **Shell-integration bootstrap fallback** — when shell integration is enabled, the injected bootstrap starts with an unconditional `export COLORTERM=truecolor;`, guaranteeing truecolor even on servers that ignore env requests.

### 9.8. Connection route — jump hosts (IN-0023 / US-0058)

`SshConfig::jump_hops: Vec<SshHop>` lists the jump hosts from the client outwards (at most `MAX_JUMP_HOPS` = 4); each `SshHop` carries its own host, port, username, `SshAuthMethod`, and `HostKeyPolicy`. `crates/ssh/src/route.rs` walks the route in `connect`:

1. `connect_hop` opens the transport (`open_transport`): plain TCP for the first entry, otherwise `channel_open_direct_tcpip(host, port, "127.0.0.1", 0)` on the previous hop's handle turned into a stream for `russh::client::connect_stream`. A jump host that refuses the channel (`AllowTcpForwarding no`) fails the `Transport` phase with the next hop's address in the message.
2. Each hop gets its own `SshClientHandler`, so `known_hosts` checks, `HostKeyUnknown`, and `HostKeyChanged` name that hop's host and port; the UI's host-key confirmation marks `AcceptNewFingerprint` on the matching hop only and says "(jump host for <label>)". Accepting a hop's key never accepts the target's.
3. `authenticate` (the former inline match) runs the hop's method; the credential is moved out of the config first so it is zeroized as soon as that hop is authenticated. Agent hops need no input.
4. The authenticated hop handles are kept in `JumpHandles`, moved into `ssh_main_task` with the target handle. In the teardown block the target is dropped first, then the hops from the innermost outwards, so no connection closes under a live carrier.

The saved-session model stores a reference, not a copy: `SshSession::jump_host: Option<SshSessionId>` names another saved session, and `SshSessionStore::jump_chain` follows the references (rejecting a missing session, a cycle, or more than four hops) when a session is saved, when Quick Connect picks a jump host, and when a connect dialog opens. The connect dialog shows one credential block per hop above the target's; Duplicate Session keeps each hop's non-secret metadata (`SshDuplicateHop`) and prompts again for every hop (DEC 0002). Rationale: [`decisions/DEC-0010-jump-hosts-reference-saved-sessions.md`](decisions/DEC-0010-jump-hosts-reference-saved-sessions.md).

### 9.9. Port forwards (IN-0023 / US-0059)

`SshConfig::port_forwards: Vec<PortForward>` (`crates/core/src/port_forward.rs`; `Local`, `Remote`, `Dynamic`, binds default to loopback) is started by `crates/ssh/src/tunnel.rs::start_forwards` after the shell and the SFTP channel are up and before the handle moves into `ssh_main_task`; forwards are never a connect phase, so a forward that cannot start produces one warning toast (`SessionEvent::Notification`) and the shell still opens (DEC-0011).

- **Local**: a `TcpListener` per spec; each accepted connection sends `HandleRequest::OpenDirectTcpip` to `ssh_main_task` (the only owner of `russh::client::Handle`), which answers with the channel, and the relay runs `copy_bidirectional` between the socket and `channel.into_stream()`.
- **Dynamic**: the same listener with a SOCKS5 handshake first (RFC 1928, no authentication, CONNECT only; IPv4, IPv6, and domain targets resolved on the remote side; BIND and UDP ASSOCIATE answer `07`, a client without the no-auth method gets `05 FF`). The whole request is read before a refusal so the reply is not lost to a reset.
- **Remote**: `handle.tcpip_forward(bind_host, bind_port)` before spawn; the accepted `(bind_host, port)` and its local target go into the handler's `ForwardTable`. `SshClientHandler::server_channel_open_forwarded_tcpip` looks the address up and bridges the channel to a fresh local `TcpStream`; an address OneTerm never requested is dropped and logged. Only the target's handler carries the table, never a jump hop's.

Agent forwarding (US-0060) is requested on the session channel between `ChannelOpen` and `PtyRequest` (`agent::request_agent_forwarding`, which waits for the channel `Success` / `Failure` reply); the target's handler bridges server-opened agent channels through `agent::spawn_agent_bridge` only when the session's switch is on. See [`ssh-authentication.md`](ssh-authentication.md).

Every listener, relay, and bridge selects on the session `CancellationToken` (`session_shutdown`, formerly the SFTP-only token); the teardown block cancels it before the transport disconnects, so bound ports are released with the tab. The bandwidth indicator counts the terminal channel only. Saved sessions persist the list in `ssh_session.json::port_forwards`; Duplicate Session copies it, so a duplicate's local binds fail with the address-in-use warning while its shell opens. Rationale: [`decisions/DEC-0011-forwarding-defaults-loopback-and-opt-in.md`](decisions/DEC-0011-forwarding-defaults-loopback-and-opt-in.md).
