# Terminal Backend Design — OneTerm

> **Status (2026-08):** design record kept current with the implementation. Rust
> blocks are design sketches (types/fields are abridged); the "Current
> implementation" notes and §5.3/§6.5/§7 describe the code as it is. Authoritative
> signatures live in `crates/terminal/src/session.rs`, `crates/terminal/src/backend/`,
> `crates/local-shell/src/` and `crates/ssh/src/`.
>
> Design document for the terminal part: **local shell** + **SSH session**, sharing a
> renderer based on `alacritty_terminal`. Windows-first priority. Local shell can be
> `cmd` / `powershell` / `pwsh` / custom.
>
> **Primary reference**: Zed (`zed-industries/zed`) uses exactly `alacritty_terminal`
> (tty + `EventLoop` + `FairMutex`) and renders via a custom GPUI Element. This design
> maps 1:1 to Zed, replacing the chrome layer with `gpui-component`.
>
> Zed source files referenced (same rev lock `1d217ee39…`):
> - `crates/terminal/src/terminal.rs` — model + EventLoop + PTY.
> - `crates/terminal_view/src/terminal_element.rs` — custom `Element` rendering the grid.
> - `crates/terminal_view/src/terminal_view.rs` — View + IME (`ImeState`).
>
> **Core decisions** (see brainstorm history):
> 1. **Local and SSH do not know about each other** — each keeps only its transport
>    (`PtyTransport`: write / resize / close) and its own read loop; parsing, OSC routing,
>    event delivery and the state cache come from the shared pump layer in
>    `oneterm-terminal::backend` (§5.3).
> 2. **Rendering shares `alacritty_terminal`** via a custom GPUI `Element`.
> 3. **Local uses `alacritty_terminal::tty` + `EventLoop`** (not `portable-pty`).
> 4. **`alacritty_terminal` is taken from the `zed-industries/alacritty` fork** @ rev `fcf32feacb367b75ec84dd40f041e4fd411d3cc1`
>    (patched version with `TerminalContent`/`display_iter`/`content()`). This is the rev Zed
>    uses for `gpui` rev `1d217ee39…`, but it is a separate repo — not the zed monorepo.
> 5. **alacritty concurrency model**: `Arc<FairMutex<Term<EP>>>` + snapshot.
> 6. **The pure kit** (`core`) does not depend on GPUI.

---

## 1. Principles

| # | Principle | Consequence |
|---|---|---|
| 1 | Clear layer separation | UI contains no protocol logic; protocol knows nothing about UI. |
| 2 | Local & SSH independent | The two backends do not depend on each other; both sit on the shared pump layer (`OscRouter<T: PtyTransport>` + `TerminalPump`) and add only transport + read loop. |
| 3 | Shared rendering | A single `TerminalElement` paints the grid for both local and ssh — only needs `&TerminalContent`. |
| 4 | Snapshot, no lock-while-paint | The pump updates the snapshot; render reads the snapshot, does not hold `FairMutex` while painting. |
| 5 | Windows-first | Local prefers ConPTY; `cmd`/`pwsh`/`powershell` shells are configurable. |
| 6 | Strict rev lock | `gpui` + `gpui_platform` at the same zed monorepo rev; `alacritty_terminal` fork `zed-industries/alacritty` rev `fcf32fe…`. |

---

## 2. Architecture diagram

```
┌─────────────────── ui crate (GPUI + gpui-component) ───────────────────┐
│  LocalTerminalView (impl Render; hosts local AND ssh sessions)          │
│   ├─ chrome: Button, Tabs, Dock… (gpui-component)                        │
│   └─ child: TerminalElement  (custom gpui::Element, shared)            │
│          • reads TerminalContent snapshot → paint_quad / shape_line      │
│          • EntityInputHandler (IME) + mouse + wheel                   │
└───────▲─────────────────────────────────────────▲──────────────────────┘
        │ TerminalSession trait (terminal)       │
   ┌────┴────────────────┐               ┌────────┴───────────────┐
   │  local-shell crate  │               │  ssh crate             │  ← INDEPENDENT
   │  tty::Pty + poll    │               │  russh + shared tokio   │     don't know each other
   │  loop (ConPTY)      │               │  channel + pty-req      │
   │  LocalTransport     │               │  SshTransport (Cmd)     │
   │  Term<OscRouter<    │               │  Term<OscRouter<        │
   │   LocalTransport>>  │               │   SshTransport>>        │
   └────┬────────────────┘               └────────┬───────────────┘
        └──────────────┬──────────────────────────┘
                ┌──────▼──────────┐
                │ terminal crate  │  backend pump layer: SharedState, SessionEventSink,
                │ (no GPUI)       │  OscRouter, ColorQueryReplier, LineAccounting,
                │                 │  TerminalPump, PtyTransport; TerminalSession,
                │                 │  TerminalContent, key/mouse encode, osc, url
                └──────┬──────────┘
                ┌──────▼───────┐
                │  core crate  │  SshConfig, ShellKind/LocalShellConfig, SftpBackend,
                │  (leaf)      │  AppError
                └──────────────┘
```

**Data flow**:
- Input: `Keystroke` (GPUI) → `core::key_encode` → `Vec<u8>` → `session.write(bytes)` → PTY/channel.
- Output: PTY/channel → pump (`ShellEventLoop` local / `ssh_main_task` tokio ssh) → `TerminalPump::advance` under the `Term` lock → `finish_batch` releases the lock and sends one `SessionEvent::Output` → View `cx.notify()` → `TerminalElement` prepaint calls `session.snapshot()` (short `Term` lock, copies `TerminalContent`, consumes damage) and paints from the copy.

---

## 3. Responsibilities per crate

| Crate | Terminal role |
|---|---|
| `core` | `ShellKind` + `LocalShellConfig` + `SshConfig` (config), `SftpBackend`, `AppError` (leaf, no GPUI). |
| `terminal` | `TerminalSession` trait + `SessionEvent`, `TerminalContent` snapshot, `TerminalPalette`, `key_encode`/`mouse_encode`/`osc`/`url`, and the **backend pump layer** (`backend` module: `SharedState`, `SessionEventSink`, `OscRouter`, `ColorQueryReplier`, `LineAccounting`, `TerminalPump`, `PtyTransport`) shared by both backends. |
| `local-shell` | `LocalSession` implementing `TerminalSession`. Spawns a shell via `alacritty_terminal::tty::new` and pumps it with a custom poll loop (`ShellEventLoop<P: EventedPty>`) feeding `TerminalPump`. ConPTY on Windows. `LocalTransport: PtyTransport` (notifier queue). Only `LocalSession` is public. |
| `ssh` | `SshSession` implementing `TerminalSession`. russh client on the shared tokio runtime; `ssh_main_task` feeds `TerminalPump`. pty-req + shell + `window_change` + exit-status. `SshTransport: PtyTransport` (bounded `Cmd` channel). SFTP task lifetime tied to the connection. Only `SshSession` + `connect` are public. |
| `terminal-view` | `TerminalElement` (custom `gpui::Element`), `LocalTerminalView` (`Render`; one view type hosts any `TerminalSession`, local or SSH), `TerminalPanel`/`PanelSpec` (dock tab), IME (`EntityInputHandler`), mouse/wheel, font measure, theme → `TerminalPalette`. |
| `app` | Installs the `SessionFactory` (`AppSessionFactory`) + `WorkspaceCommands` through `AppServices`; only crate that links `ssh`/`local-shell`. |

> Dependency rules: `app → {terminal-view, ssh, local-shell, terminal, core, …}`, `ssh → {terminal, core}`,
> `local-shell → {terminal, core}`. No UI crate imports `ssh`/`local-shell` — sessions are created via
> `oneterm_terminal::SessionFactory` and driven via `TerminalSession` (see `docs/agents/crate-dependency-rules.md` R3).

---

## 4. Dependencies & rev lock

```toml
# root Cargo.toml [workspace.dependencies] (authoritative list: docs/agents/dependencies.md §1/§3)
alacritty_terminal = { git = "https://github.com/zed-industries/alacritty", rev = "fcf32feacb367b75ec84dd40f041e4fd411d3cc1" }  # redirected to vendor/alacritty_terminal by [patch]
async-channel = "2"      # event sub (no tokio leaked out)
russh = { version = "0.61", default-features = false, features = ["ring", "flate2", "rsa"] }  # keys API is russh::keys (russh-keys was merged in)
russh-sftp = "2.3"
tokio = { version = "1", features = ["rt", "rt-multi-thread", "sync", "io-util", "net", "macros", "fs"] }
```

> The fork is **vendored**: `vendor/alacritty_terminal` = pristine `fcf32fe` + the
> patches in `vendor/patches/alacritty_terminal/` (single-pass OSC/clear hook), see
> [`vendor/README.md`](../vendor/README.md).

> ⚠️ **Mandatory**: `alacritty_terminal` must be taken from the `zed-industries/alacritty` fork @
> rev `fcf32fe…` (the rev Zed uses for `gpui` rev `1d217ee39…`). NOT the zed monorepo.
> Using crates.io `0.26` will be **missing** `TerminalContent`/`display_iter`/`content()`/`Block`
> that rendering needs → won't compile. When changing the `gpui` rev → check the Zed workspace deps
> to get the matching `alacritty_terminal` rev (the two revs can differ).
>
> `portable-pty` is **no longer used** for local (brainstorm decision). `ssh` doesn't need a
> local PTY — only needs `alacritty_terminal` for the Term grid.

---

## 5. Concurrency model: `Arc<FairMutex<Term<EP>>>` + snapshot

### 5.1. Why

- The **pump** (local `EventLoop` thread / ssh tokio task) advances Term on another thread.
- **Render** (`TerminalElement::paint`) runs on the GPUI main thread.
- Both need access to the same `Term` ⇒ use `alacritty_terminal::sync::FairMutex`
  (fair = the main thread doesn't starve for the lock while the pump is busy).

### 5.2. Snapshot vs live borrow (IMPORTANT)

| | Live borrow (WRONG) | Snapshot (CORRECT — what Zed does) |
|---|---|---|
| Paint | `let g = term.lock();` then paint **holding the guard** | `let snap = { let g = term.lock(); build content }; drop(g);` then paint |
| Problem | slow paint (thousands of GPU calls) → pump `term.lock().advance()` **blocks** → jitter under output bursts (`yes`, `cat large file`) | Lock only for µs to copy, pump runs in parallel with paint |
| Cost | 0 | 1 copy ~thousand cells/frame (far cheaper than paint) |

**Convention (as implemented)**: there is **no cached `last_content`**. The pump only
sends the `Output` hint; `TerminalSession::snapshot()` (`TerminalModel::snapshot`,
`crates/terminal/src/model.rs`) takes the `FairMutex` for the microseconds needed to
copy `TerminalContent` (and consume the damage), releases it, and the element paints
from that owned copy — the lock is never held **while painting**. Non-render reads use
`snapshot_query()` (damage-free), `query_state()` (O(1), no cells) or
`query_line_range_cells()`; every one of them is a short lock too, so the pump and the
UI contend only briefly (see the "never block inside a `Term` callback" rule in §5.3).

```rust
// Pump (ShellEventLoop / ssh_main_task) — per read chunk:
pump.advance(&mut *term.lock(), bytes);       // parse under the Term lock
pump.finish_batch_blocking(true);             // lock released: flush reliable events, then Output

// Render (TerminalElement prepaint):
let content = session.snapshot();             // short Term lock, owned TerminalContent
// paint from content.cells / content.cursor / content.mode ...
```

> Do NOT hold the `FairMutex<Term>` across layout/paint work; copy, drop the guard,
> then paint. `snapshot()` is called exactly once per frame from the render path.

### 5.3. Shared pump layer (`oneterm_terminal::backend`)

Both backends use the same `EventListener` and the same batch driver; they only
provide a transport and a read loop.

| Type | Role |
|---|---|
| `PtyTransport` (trait) | The backend half: `pty_write` / `pty_resize` / `pty_close`. Non-blocking, `Clone` (Arc handles). `LocalTransport` wraps the owner-thread notifier queue; `SshTransport` wraps the bounded `Cmd` channel (byte budget, coalesced resize, closing flag). |
| `SharedState` (`Arc<SharedSessionState>`) | Title / cwd / clipboard / exit code / OSC 133 counters / theme default colours / OSC 9;7 seq watermarks behind one mutex; `alive`, rx/tx bytes, absolute line count and clear epoch as atomics so a parse batch never takes the mutex. `SharedStateCwdSource` exposes cwd to the SFTP browser. |
| `SessionEventSink` | Delivery policy: `Output` is coalescible (dropped when the 4096-slot queue is full), everything else is reliable. `forward` never blocks — reliable events that do not fit go to a FIFO and `flush_reliable[_blocking]` delivers them after the batch, outside the `Term` lock. `forward_lifecycle*` flushes first so `Exited`/`Closed` arrive in order. Counters (`EventQueueDiagnostics`) for tests/diagnostics. |
| `OscRouter<T: PtyTransport>` | The `EventListener` installed in `Term`: `Wakeup` → `Output`; `Title`/`ResetTitle` → state + `Title`; OSC 52 store/load gated by `TerminalSecurityPolicy` + `ClipboardOrigin` (remote default off — the same code for both backends, so the policy cannot drift; the policy is the user's, derived from `TerminalSettings` by `terminal-view` and passed through `SessionFactory::{spawn_local, connect_ssh}` into `OscRouter::with_security`); `Event::Osc` (OSC 7/9/133/9;7 from the fork) → state + `Cwd`/`Notification`/`Progress`/`ShellIntegration`/`AgentStatus` (rate limit, seq dedup); `ClearScreen` → clear epoch; `ColorRequest` → `ColorQueryReplier`; `PtyWrite` → `transport.pty_write`; `Bell`. |
| `ColorQueryReplier` | Queue of OSC 10/11/12 (and OSC 4) queries collected during `advance`; `replies(term, defaults, queries)` formats answers from the live `Term` colours with the theme defaults as fallback. |
| `LineAccounting` | Absolute-line counter (gutter numbers keep growing after the scrollback is full). Owned by the pump, published to `SharedState` once per batch. |
| `TerminalPump<T>` | Owns `ansi::Processor` + `LineAccounting` + a router clone. Per chunk: `advance(term, bytes)` under the `Term` lock (or `process_chunk(&term_arc, bytes)` which also answers colour queries and writes the replies), then `finish_batch[_blocking](repaint)` once the lock is released: publish line count → flush deferred reliable events (backpressure) → `Output`. Lifecycle: `publish_exit*` / `publish_closed*`. |

Local (`ShellEventLoop<P>`) uses the blocking variants on the PTY owner thread;
SSH (`ssh_main_task`) uses the async ones on the tokio runtime. Neither backend
resizes the `Term` grid from its loop — the UI thread does that in
`TerminalSession::resize` before asking the transport for `pty_resize`.

**Never block inside a `Term` callback.** `send_event` runs during
`Processor::advance` with the `Term` lock held, and the UI thread needs that same
lock (`snapshot()`, `terminal_info()`) to drain the event queue. The sink
therefore only `try_send`s from a callback; deferred reliable events are
delivered by the pump's `finish_batch` after the parse batch, once the lock is
released (event loop: blocking send; tokio task: `send().await`). Ordering seen by
the UI: reliable events emitted during a batch → that batch's `Output` hint.
Lifecycle events (`Exited`/`Closed`) always flush the deferred queue first so
they arrive in order.

The layer is testable without a PTY or a network: `test_support::FakePtyTransport`
records writes, and `crates/terminal/src/backend/backend_tests.rs` drives the pump
end to end (title/bell ordering, colour replies, deferred flush, lifecycle).

---

## 6. Local backend (`local` crate, Windows-first)

### 6.1. Configurable shell

`core` defines:

```rust
/// Local shell kind.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ShellKind {
    /// Windows cmd.exe (COMSPEC).
    Cmd,
    /// Windows PowerShell 5.1 (powershell.exe).
    PowerShell,
    /// PowerShell 7+ (pwsh.exe).
    Pwsh,
    /// Unix shells.
    Bash,
    Zsh,
    Sh,
    /// Custom command.
    Custom,
}

/// Local shell spawn configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LocalShellConfig {
    pub kind: ShellKind,
    /// Executable path (None → auto-detect by kind + platform).
    pub program: Option<PathBuf>,
    /// Extra command-line args.
    pub args: Vec<String>,
    /// Env overrides (TERM, COLORTERM, LANG…). TERM=xterm-256color is set by default.
    pub env: HashMap<String, String>,
    /// Working directory (None → current app cwd).
    pub cwd: Option<PathBuf>,
    /// Force UTF-8 codepage (Windows cmd). Default true.
    pub utf8: bool,
}
```

Resolving `ShellKind` → executable + args + env (Windows-first):

| Kind | Default program | Default args | UTF-8 |
|---|---|---|---|
| `Cmd` | `%COMSPEC%` (cmd.exe) | `/K chcp 65001 >nul` (if `utf8`) | `chcp 65001` |
| `PowerShell` | `powershell.exe` (found in PATH / `where`) | `-NoLogo` | env `LANG=en_US.UTF-8`; `[Console]::OutputEncoding=UTF8` via `-NoExit -Command` arg |
| `Pwsh` | `pwsh.exe` | `-NoLogo` | like PowerShell |
| `Bash`/`Zsh`/`Sh` | `$SHELL` / `/bin/bash`… | `-l` (login) per config | env `LANG`/`LC_ALL` |
| `Custom` | `program` (required) | `args` | per `env`/`utf8` |

> The terminal settings panel (`crates/terminal-view/src/settings_panel.rs`) lets the user pick `kind`, type a custom
> `program`, add `args`, set `cwd`, toggle `utf8`. Persisted in `terminal.json` via `oneterm_settings::TerminalConfig`.

### 6.1.1 Windows local cwd reporting

OneTerm's generated prompt integration emits OSC 7 whenever it controls the Windows shell prompt:

- `cmd.exe`: the generated `PROMPT` emits the current `$P` path before OSC 133 markers. A user-supplied `LocalShellConfig.env["PROMPT"]` remains authoritative and may omit cwd reporting.
- Windows PowerShell and pwsh: the startup command wraps the existing global `prompt` function, emits the current `$pwd.Path` as OSC 7, then invokes the original prompt. This preserves the shell's normal prompt output while making cwd updates observable after `cd`/`Set-Location`.
- Custom shells or user prompt overrides must emit OSC 7 themselves if live cwd tracking is required.

The local listener already parses forwarded OSC 7 payloads into `SessionEvent::Cwd` and updates `TerminalSession::cwd()`.

### 6.2. Spawn via `alacritty_terminal::tty`

> Original design sketch (alacritty `EventLoop` + `ArcSwap` cache). The shipped code
> described below the sketch differs: a custom `ShellEventLoop`, no `last_content`
> cache, and `LocalTransport`/`OscRouter` from §5.3.

```rust
use alacritty_terminal::{event_loop::EventLoop, sync::FairMutex, term::{Config, Term}, tty::{self, Options, Shell, WindowSize}};

pub struct LocalSession {
    term: Arc<FairMutex<Term<LocalListener>>>,
    notifier: Notifier,                          // EventLoop channel (Msg::Input/Resize/Shutdown)
    last_content: Arc<ArcSwap<TerminalContent>>, // snapshot
    event_tx: Sender<SessionEvent>,
    config: LocalShellConfig,
    // child exit, alive flag…
}

impl LocalSession {
    pub fn spawn(cfg: LocalShellConfig, initial: PtySize) -> core::Result<Self> {
        let (program, args, env) = resolve_shell(&cfg)?;     // §6.1 table
        let opts = Options {
            shell: Some(Shell { program: program.into(), args: args.into_iter().map(Into::into).collect() }),
            working_directory: cfg.cwd.clone(),
            env: env.into_iter().collect(),
            ..Default::default()
        };
        let winsize = WindowSize { rows: initial.rows, cols: initial.cols, ..Default::default() };
        let pty = tty::new(&opts, winsize, 0).map_err(|e| AppError::msg(e.to_string()))?;

        let term = Arc::new(FairMutex::new(Term::new(
            Config { scrolling_history: 10_000, ..Default::default() },
            &TermSize::from(initial),
            LocalListener { /* event_tx clone */ },
        )));
        let mut event_loop = EventLoop::new(term.clone(), LocalListener::default(), pty, false, false)
            .map_err(|e| AppError::msg(e.to_string()))?;
        let notifier = event_loop.channel();           // for write/resize
        event_loop.run().detach();                      // spawn pump thread
        // … child exit watcher (ChildExitWatcher) → SessionEvent::Exited
        Ok(Self { term, notifier, last_content: Arc::new(ArcSwap::from_pointee(default())), event_tx, config: cfg })
    }

    pub fn write(&self, bytes: &[u8]) { self.notifier.tty_notify(bytes.to_vec().into()); }   // Msg::Input
    pub fn resize(&self, r: u16, c: u16) { self.notifier.notify_resize(WindowSize { rows: r, cols: c, ..Default::default() }); }
    pub fn shutdown(&self) { self.notifier.shutdown(); }
}
```

> For the exact `Notifier` API: note that
> `reference/gpui-component` **does not have it** — this is Zed's internal API; read directly
> from the `alacritty_terminal` source at the rev lock: `event_loop.rs` (`Notifier`, `Msg`),
> `tty/{mod,unix,windows}.rs`. When implementing, open that crate's source to match signatures.

**Current implementation** (`crates/local-shell/src/event_loop.rs`): the loop is a
custom `ShellEventLoop<P: EventedPty + OnResize>` on a dedicated "PTY owner"
thread — the PTY is created, polled and dropped there. It reads with a
heap-allocated 1 MiB buffer into `TerminalPump::advance` under a
`try_lock_unfair` guard (falling back to `lock_unfair` only when the buffer is
full), answers colour queries with the same guard, then calls
`finish_batch_blocking`. The poller waits **without a timeout**: every
`ShellNotifier::send` and the child watcher call `poller.notify()`, so an idle
tab does not wake up. Being generic over the PTY, the loop is unit-tested with a
loopback-socket PTY (`event_loop_tests.rs`) — no shell is spawned to cover
output parsing, input FIFO, resize, colour replies, child exit and shutdown.

### 6.3. Windows-specific

- **ConPTY**: `alacritty_terminal::tty` picks ConPTY automatically on Win10 1809+. No need
  to hand-code `CreatePseudoConsole`.
- **UTF-8**: `Cmd` → `chcp 65001` (via `/K` args). `pwsh`/`powershell` → set env
  `LANG`/`LC_ALL` + (optionally) an init arg `[Console]::OutputEncoding`.
- **TERM**: always `xterm-256color`, `COLORTERM=truecolor`.
- **Resize**: `Notifier::notify_resize` → ConPTY handles it (no SIGWINCH on Windows).
- **Ctrl-C**: byte `0x03` → shell handles it. OK.
- **Child exit**: `tty::Pty` provides `ChildExitWatcher` (race-free) → `SessionEvent::Exited(code)`.

### 6.4. Re-render perf (per Zed)

- The pump doesn't `notify` per byte — a read chunk is parsed as one batch and
  `finish_batch` sends a **single** coalescible `SessionEvent::Output` (§5.3/§6.5).
- The View `cx.notify()` only when `display_offset`/`mode`/`cursor`/cells actually change
  (compare old vs new snapshot). Avoids continuous redraw under `yes`.
- Log `layout took {:?}` for tuning (copy Zed's `log::debug!`).

### 6.5. Transport backpressure contract

SSH and LocalShell use the same observable overload semantics even though their
owner loops use different channel implementations:

- Command queues are bounded to 256 messages and a 4 MiB aggregate write-payload
  budget. The source-of-truth constants are `SSH_COMMAND_QUEUE_CAPACITY`,
  `SSH_COMMAND_BYTE_BUDGET`, `LOCAL_COMMAND_QUEUE_CAPACITY`, and
  `LOCAL_COMMAND_BYTE_BUDGET`.
- Writes preserve FIFO order and are atomic at enqueue time: the complete write is
  accepted, or `TerminalError::QueueFull`/`TerminalError::Closed` is returned.
  Paste uses the same write path and therefore cannot bypass the byte budget.
- Resize is latest-value delivery. Bursts overwrite one pending size instead of
  consuming one queue slot per intermediate geometry.
- Close/shutdown is out-of-band and has priority over queued input. A queue at
  capacity cannot prevent the owner loop from observing close.
- `SessionEvent::Output` is the only coalescible event. Clipboard, notification,
  progress, agent, title, working-directory, bell, and lifecycle events use
  reliable bounded-channel delivery: they are never dropped, and a slow consumer
  applies backpressure to the pump — but only *between* parse batches, never
  while the `Term` lock is held (§5.3). A closed consumer is logged and counted by
  diagnostic builds.
- Local child exit always ends the session: `alive = false`, then
  `SessionEvent::Exited(code)` (code may be `None` when the platform watcher could
  not read it) followed by `SessionEvent::Closed`.

Backends do not retry rejected writes because retrying after returning an error
could duplicate input. Callers must report failure or explicitly retry the same
payload. Saturation tests use the production policies and constants.

---

## 7. SSH backend (`ssh` crate)

The Tokio runtime is **hidden**: one process-wide `new_multi_thread` runtime with
`SSH_RUNTIME_WORKERS = 2` worker threads (`crates/ssh/src/session.rs`,
`shared_runtime()`), shared by every SSH session so the thread count does not grow
per tab. The exposed API is sync: `connect()` runs the handshake, auth, `pty-req`,
`shell` and the SFTP channel open inside `runtime.block_on`, then spawns
`ssh_main_task` + `sftp_task` and returns a `Box<dyn TerminalSession>`.

```rust
// abridged — see crates/ssh/src/{session,transport,session_terminal}.rs
pub struct SshSession {
    model: TerminalModel<SshListener>,          // Arc<FairMutex<Term<OscRouter<SshTransport>>>>
    transport: SshTransport,                    // bounded async_channel<Cmd> + closing flag
    state: SharedState,                         // title / cwd / counters (§5.3)
    events: Mutex<Option<async_channel::Receiver<SessionEvent>>>, // handed out once
    sftp: Option<Arc<SftpSession>>,             // same TCP connection, own task
    // …
}

enum Cmd { Write(Vec<u8>), Resize { rows, cols }, Close }

pub fn connect(cfg: SshConfig, initial: PtySize, scrollback: usize)
    -> core::Result<Box<dyn TerminalSession>>
{
    let runtime = shared_runtime()?;                       // 2-worker multi-thread runtime
    runtime.block_on(async {
        // russh::client::connect (host-key policy in handler.rs, known_hosts)
        // → authenticate (none / password / private key, keyboard-interactive fallback)
        // → channel_open_session + request_pty("xterm-256color") + request_shell
        // → open the SFTP channel
    })?;
    runtime.spawn(ssh_main_task(/* channel, term, pump, transport, … */));
    runtime.spawn(sftp_task(/* … */));
    Ok(Box::new(session))
}
```

- `SshListener = OscRouter<SshTransport>` — `PtyWrite(text)` → `SshTransport::pty_write`
  → `Cmd::Write` (256-message queue, 4 MiB byte budget, `Cmd::Resize` coalesced).
- `is_local() == false` (for OSC 7 cwd semantics: ssh can be `file://host/…`).
- Exit: `ChannelMsg::ExitStatus { exit_status }` → `SessionEvent::Exited(Some(code))`;
  `Eof`/`Close`/`Cmd::Close`/closing flag → `SessionEvent::Closed`.
- Shutdown signal: the transport's closing flag (set by `pty_close`) — the task holds
  `cmd_tx` clones itself, so the command channel never closes on its own.
  `SshSession::close()` and `Drop` request close for the shell **and** SFTP; the
  two are idempotent.
- `ssh_main_task` ends in a single teardown block: `channel.close()`,
  `publish_closed()` (flushes deferred reliable events, then `Closed`), then it
  cancels the SFTP `CancellationToken` so `sftp_task` exits and
  `SftpBackend::alive()` turns false with the connection.
- Reliable events emitted during `processor.advance` are flushed by
  `TerminalPump::finish_batch().await` after the batch, before the `Output` hint (§5.3).
- RSA keys authenticate with `rsa-sha2-*` chosen from the server's `server-sig-algs`
  (fallback SHA-512); legacy SHA-1 `ssh-rsa` is never used.
- Auth: `SshAuthMethod::{None, Password, PrivateKey}` (`crates/core/src/ssh_config.rs`)
  with keyboard-interactive as the password fallback; host keys are checked against
  `known_hosts` (`crates/ssh/src/handler.rs`) — see [`ssh-client-connect.md`](ssh-client-connect.md).
  No ssh-agent support yet.

> Sync→async bridge: `async_channel` in both directions — the UI thread sends `Cmd`
> through `SshTransport`, `ssh_main_task` receives it inside the runtime; outgoing
> `SessionEvent`s go over the bounded `async_channel` the view drains on the GPUI
> executor. No `std::sync::mpsc` and no nested `block_on` after connect.

---

## 8. Rendering (`terminal-view` crate) — `TerminalElement`

Custom `gpui::Element` (Zed `terminal_element.rs` pattern). Paints from the **snapshot**.

### 8.1. Structure

```rust
pub struct TerminalElement {
    session: Entity<dyn TerminalSession>,   // or generic
    bounds: TerminalBounds,                  // cell_width, line_height, rows, cols
    theme: TerminalTheme,                    // bg/fg/16 ANSI/cursor → gpui::Hsla
    focus: FocusHandle,
    focused: bool,
    cursor_visible: bool,
    interactivity: Interactivity,
}

impl InteractiveElement for TerminalElement { /*…*/ }
impl StatefulInteractiveElement for TerminalElement {}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = LayoutState;       // hitbox, bg_rects, text_runs, cursor, ime_bounds
    fn request_layout(&mut self, …) -> (LayoutId, ()) { /* size_full or size by rows×cols */ }
    fn paint(&mut self, …, layout: &mut LayoutState, window, cx) {
        let content = self.session.read(cx).snapshot();      // no FairMutex lock
        window.with_content_mask(Some(ContentMask { bounds }), |w| {
            w.paint_quad(fill(bounds, self.theme.bg));
            for rect in &layout.bg_rects { rect.paint(origin, &self.bounds, w); }   // batch background
            for run in &layout.text_runs { run.paint(origin, &self.bounds, w, cx); } // ShapedLine.paint
            // cursor + selection + ime marked text
        });
        window.handle_input(&ElementInputHandler::new(self.session.downgrade(), self.session.clone()));  // IME
    }
}
```

### 8.2. `layout_grid` (batched — copy Zed)

Iterate `content.display_iter` (`IndexedCell`):
- **Background**: group consecutive cells with the same background color (skip default bg) → `Vec<LayoutRect>`
  + `merge_background_regions` (merge horizontally/vertically) to reduce `paint_quad`.
- **Text**: group consecutive cells with the same `TextRun` (fg + bold/italic/underline + font) →
  `Vec<BatchedTextRun>`. Each run: `window.text_system().shape_line(text, font_size,
  &[run], Some(cell_width)).paint(pos, line_height, Left, None, window, cx)`.
- **Wide char spacers** + **zero-width chars** (emoji variation sequences): handle correctly
  (copy Zed's `is_wide_char_spacer` / `append_zero_width_chars` logic).
- **Contrast**: `ensure_minimum_contrast(fg, bg, min)` — skip if
  `is_app_chosen_exact_color` (truecolor/256≥16) or `is_decorative_character`
  (box-drawing/powerline). These functions live in `core` (pure).

### 8.3. Font measure (terminal-specific font)

`TerminalSettings` (own font, independent of gpui-component theme):
```rust
pub struct TerminalSettings {
    pub font_family: String,         // e.g. "Cascadia Mono", "JetBrains Mono"
    pub font_size: f32,               // px
    pub font_weight: u32,
    pub line_height: f32,             // multiplier (1.0 = default)
    pub font_features: FontFeatures,
    pub scrollback: usize,
    pub minimum_contrast: f32,
    pub shell: LocalShellConfig,     // §6.1
}
```
Measure (cached, re-measure on font/size change):
```rust
let probe = window.text_system().shape_line("M".repeat(cols).into(), font_size, &[base_run], Some(target_cell_w));
let cell_width = probe.width() / cols as f32;
let line_height = font_size * settings.line_height;     // or ascent+descent+leading
```

### 8.4. Colors (`terminal` + `terminal-view`)

- `oneterm_terminal::TerminalPalette` (`crates/terminal/src/palette.rs`, pure `Rgb`).
- `terminal-view` builds `TerminalTheme`/`TerminalPalette` from `cx.theme()` (`crates/terminal-view/src/theme/`).
- `oneterm_terminal::palette::resolve_color(&Color, &TerminalPalette) -> Rgb` (named/indexed/truecolor).
- `ensure_minimum_contrast(fg: Hsla, bg: Hsla, min: f32) -> Hsla` (`crates/terminal-view/src/theme/contrast.rs`, cached).

---

## 9. `TerminalSession` trait (`terminal` crate)

> Abridged design sketch. The real trait (`crates/terminal/src/session.rs`) is wider:
> `snapshot_query` / `query_state` / `query_line_range_cells` / `terminal_info` (damage-free
> reads), `write`/`resize`/`close` return `Result<(), TerminalError>`, plus search,
> selection, paste, `send_ctrl_c`, `dynamic_colors`, `set_default_colors` and
> `capabilities()`.

```rust
pub trait TerminalSession: Send + Sync + 'static {
    /// Grid snapshot for rendering (no FairMutex lock held during the call).
    fn snapshot(&self) -> TerminalContent;
    /// Write bytes to the PTY/channel (keystroke, paste, OSC response).
    fn write(&self, bytes: &[u8]);
    /// Resize rows×cols (PTY resize / ssh window_change).
    fn resize(&self, rows: u16, cols: u16);
    /// Scroll scrollback (only when not alt-screen / not mouse mode).
    fn scroll(&self, delta: i32);
    // Mouse
    fn mouse_down(&self, row: f32, col: f32, button: MouseButton, sel: SelectionType);
    fn mouse_move(&self, row: f32, col: f32);
    fn mouse_up(&self, row: f32, col: f32, button: MouseButton);
    fn wheel(&self, delta_y: f64, row: f32, col: f32);
    // IME
    fn set_marked_text(&self, text: String);
    fn clear_marked_text(&self);
    fn commit_text(&self, text: &str);
    fn marked_text(&self) -> Option<String>;
    fn cursor_bounds(&self) -> Option<Bounds<Pixels>>;     // for IME popup
    // Lifecycle
    fn take_events(&self) -> Option<Receiver<SessionEvent>>; // once-only; None after the first call
    fn alive(&self) -> bool;
    fn close(&self);
    fn is_local(&self) -> bool;
    fn title(&self) -> Option<String>;
    fn cwd(&self) -> Option<PathBuf>;                      // OSC 7
}
```

> This trait is only a **render/lifecycle interface** — it does not force a shared pump/transport.
> `LocalSession` and `SshSession` implement it independently. The two backends still don't know each other.

`TerminalSession::capabilities()` returns optional backend services as one scoped
`TerminalCapabilities` value. SSH supplies network counters, SFTP, and its live CWD
source; local sessions use the default empty value. This keeps optional features out
of the required implementation surface for test fakes and future backends. The app
installs the session factory and workspace callbacks together through `AppServices`;
feature crates read those handles from their GPUI application context.

`SessionEvent`: `Output | Title | Cwd | Clipboard | ClipboardRead | ShellIntegration |
Notification | Progress | AgentStatus | ForegroundProcess | Exited(Option<i32>) | Closed |
Bell` — `Output` is the only coalescible event (§6.5).

### 9.1 Session duplication metadata and cwd

Each terminal view retains a non-secret launch descriptor so the terminal context menu can duplicate the terminal in the right-clicked Space without importing a backend crate:

- Local descriptors contain the complete `LocalShellConfig` used to spawn the source session.
- SSH descriptors contain host, port, username, authentication preference, optional private-key path, and shell-integration preference. They never contain a password or private-key passphrase.

At invocation time, duplication reads `TerminalSession::cwd()`. Local duplication clones its descriptor, sets `LocalShellConfig::cwd` to the live value, and spawns a new sibling tab through `SessionFactory`; when the live value is absent, it clears `cwd` so the shell/backend chooses its normal default directory. SSH duplication crosses the app-composed workspace command boundary to `session-ui`, opens a prefilled authentication dialog with empty secret fields and one-shot initial focus on password/passphrase, reconnects through `SessionFactory`, and requests the selected cwd in the new remote login shell only when known. It does not use OneTerm's process cwd as a fallback. The source process/session and terminal contents are not cloned or modified.

See [`decisions/0002-ssh-duplicate-auth.md`](decisions/0002-ssh-duplicate-auth.md) for the accepted credential-lifetime decision and [`terminal-split/04-context-menu.md`](terminal-split/04-context-menu.md) for menu behavior.

---

## 10. Input: keystroke → byte + IME

Per the Zed README (4 input paths):

1. **Raw keystroke** (`on_key_down` in the element): `try_keystroke(keystroke, mods)`
   → `core::key_encode` → `session.write(bytes)`. Mapping: Ctrl+char → `& 0x1f`, F-key /
   arrow → ANSI escape, Enter → `\r`, Backspace → `0x7f`, Tab → `\t` / `\x1b[Z`…
   (copy `freya-terminal::write_key` logic, purify into `core::key_encode`).
2. **GPUI action** (Ctrl-Shift-C/V copy/paste, Ctrl-Tab…): map → `try_keystroke` or
   clipboard.
3. **IME**: keystroke not mapped → yield to GPUI IME → `EntityInputHandler` calls back
   `replace_text_in_range(text)` → `session.commit_text(text)`. Pre-edit:
   `replace_and_mark_text_in_range` → `session.set_marked_text` → paint marked text at
   the cursor with an underline.
4. **Paste**: `session.commit_text(text)` (bracketed paste if `TermMode::BRACKETED_PASTE`).

IME impl (`terminal-view`):
- `LocalTerminalView` (`crates/terminal-view/src/view/ime.rs`) impl `gpui::EntityInputHandler`:
  `selected_text_range`, `marked_text_range`, `replace_text_in_range`,
  `replace_and_mark_text_in_range`, `unmark_text`, `bounds_for_range`,
  `text_for_range`, `character_index_for_point`.
- `ImeState { marked_text: String }` kept on the View.
- In `paint`: `window.handle_input(&ElementInputHandler::new(view_handle))`.
- Paint marked text: shape separately, paint at `ime_cursor_bounds` + underline.

---

## 11. File layout (current)

```
crates/
├── core/src/
│   ├── ssh_config.rs         # SshConfig + SshAuthMethod
│   ├── session_duplicate.rs  # SessionDuplicateConfig (non-secret launch descriptor, §9.1)
│   └── config/shell.rs       # ShellKind, LocalShellConfig, resolve_shell
│
├── terminal/src/             # engine (no GPUI)
│   ├── session.rs            # TerminalSession trait + SessionEvent + TerminalCapabilities
│   ├── model.rs              # TerminalModel<EP>: snapshot / snapshot_query / query_state / input
│   ├── content.rs            # TerminalContent snapshot struct
│   ├── palette.rs / color_classification.rs / osc_color.rs
│   ├── key_encode.rs / mouse_encode.rs / paste.rs / search.rs
│   ├── osc.rs / osc_agent/ / url.rs / url_policy.rs / security_policy.rs
│   ├── factory.rs            # PtySize + SessionFactory
│   └── backend/              # shared pump layer (§5.3)
│       ├── transport.rs      # PtyTransport trait
│       ├── state.rs          # SharedState / SharedStateCwdSource
│       ├── event_sink.rs     # SessionEventSink (delivery policy, deferred flush)
│       ├── osc_router.rs     # OscRouter<T>: EventListener
│       ├── color_reply.rs    # ColorQueryReplier
│       ├── line_accounting.rs
│       ├── pump.rs           # TerminalPump<T>
│       └── backend_tests.rs  # in-memory transport tests
│
├── local-shell/src/
│   ├── lib.rs                # pub: LocalSession
│   ├── session.rs            # LocalSession: tty + ShellEventLoop
│   ├── session_terminal.rs   # impl TerminalSession
│   ├── event_loop.rs         # ShellEventLoop<P>, ShellNotifier (+ event_loop_tests.rs)
│   └── transport.rs          # LocalTransport: PtyTransport; LocalListener alias
│
├── ssh/src/
│   ├── lib.rs                # pub: SshSession, connect
│   ├── session.rs            # connect(): russh + shared tokio runtime
│   ├── session_terminal.rs   # impl TerminalSession
│   ├── task.rs               # ssh_main_task: channel ↔ TerminalPump
│   ├── transport.rs          # SshTransport: PtyTransport; SshListener alias
│   ├── handler.rs            # host-key policy (known_hosts)
│   ├── counting_stream.rs    # rx/tx byte counters
│   └── sftp.rs / sftp_task.rs / sftp_task/   # SftpSession + tokio task + transfers
│
└── terminal-view/src/        # feature crate (GPUI)
    ├── panel/                # TerminalPanel + PanelSpec (dock tab, Space tree)
    ├── view/                 # LocalTerminalView (Render, IME, keys, search, scrollbar)
    ├── element/              # TerminalElement: prepaint (layout_grid) + paint + measure
    ├── handlers/ · layout/ · space/ · theme/ · url/ · highlight/ · completion/
    └── settings_panel.rs     # terminal settings panel
```

---

## 12. Implementation order (roadmap)

> **Status:** steps 1–7 are complete; step 8 is partial (known_hosts done, agent
> auth and reconnect not implemented); step 9 is ongoing (see
> [`terminal-rendering-optimization.md`](terminal-rendering-optimization.md) and
> [`terminal-fullscreen-perf/`](terminal-fullscreen-perf/README.md)).

1. ✅ **`core`**: `TerminalSession` trait, `SessionEvent`, `TerminalContent`, `TerminalPalette`,
   `key_encode`, `mouse_encode`, `osc`/`url`, `ShellKind`/`LocalShellConfig` + `resolve_shell`.
2. ✅ **`local`** (Windows-first): `LocalSession` spawns `cmd` (ConPTY, `chcp 65001`),
   `LocalListener`, snapshot + event. E2E test: `echo oneterm_e2e` → snapshot contains the string.
3. ✅ **`ui`**: `TerminalElement` paints grid + cursor + font measure + resize-on-layout.
   `LocalTerminalView` (`Render`) wired into DockArea. Settings shell picker (`TerminalSettingsPanel`).
4. ✅ **`ui`**: mouse (down/move/up/wheel), selection (Simple/Semantic/Lines/Block),
   scrollback, hyperlink OSC 8 (Ctrl+click), copy/paste (select-to-copy, middle-click,
   Ctrl+Shift+C/V, OSC 52 clipboard), minimum-contrast.
5. ✅ **`ui`**: IME (`EntityInputHandler` + marked text, `handle_input` in paint,
   alt-screen → disable IME, `bounds_for_range` = cursor bounds).
6. ✅ **`local`**: `powershell`/`pwsh`/`bash`/`zsh`/`sh`/`custom`, child exit detection,
   resize, 10k-line scrollback.
7. ✅ **`ssh`**: `SshSession` password + key, pty-req + shell + window_change + exit.
8. 🟡 **`ssh`**: known_hosts ✅, agent ⬜, reconnect ⬜.
9. 🟡 Perf tuning (batch, snapshot diff, debounce notify).

---

## 13. Risks

| Risk | Mitigation |
|---|---|
| `alacritty_terminal` Zed-internal API changes between revs | Pin rev; open the crate source at the rev when implementing to match signatures. |
| Holding `FairMutex` in paint → jitter | Snapshot pattern (§5.2): short lock to copy, paint from the copy. |
| Tokio (ssh) vs smol (gpui) runtime conflict | Hidden shared tokio runtime inside `ssh` (2 workers), sync API, bridge via `async_channel`. |
| Windows cmd codepage not UTF-8 | `chcp 65001` (cmd), env `LANG` (pwsh). Document requires Win10 1903+ for good ConPTY. |
| `yes` spam → continuous redraw | Snapshot diff + debounce notify (§6.4). |
| Channel backpressure | 256 command messages plus a 4 MiB write budget; latest-value resize, priority close, coalescible repaint hints, and reliable stateful events (§6.5). |
| IME differs on Windows/Linux | Use GPUI's `EntityInputHandler` (ready abstraction), test both platforms. |
| SSH host key not verified | Require known_hosts + accept prompt, don't disable by default. |

---

## 14. Quick reference

| Need | Read |
|---|---|
| Model + EventLoop + PTY (local) | Zed `crates/terminal/src/terminal.rs` (rev `1d217ee39…`) |
| Grid rendering | Zed `crates/terminal_view/src/terminal_element.rs` |
| IME + View | Zed `crates/terminal_view/src/terminal_view.rs` (`ImeState`) |
| `Element`/`paint_quad`/`shape_line` | `reference/gpui-component` + GPUI docs (rev lock) |
| `EntityInputHandler` | `gpui::EntityInputHandler` trait (docs.rs matching rev) |
| `alacritty_terminal` API | source at rev `fcf32fe…` (`event_loop.rs`, `tty/`, `term.rs`, `sync.rs`) — fork `zed-industries/alacritty` |
| freya key/mouse encode | `freya-terminal` `handle.rs`/`parser.rs` (reference the logic, purify into `core`) |