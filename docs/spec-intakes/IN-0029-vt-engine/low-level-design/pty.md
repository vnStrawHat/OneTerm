# Low-Level Design: Pseudo-console transport

Intake: IN-0029
HLD: ../high-level-design.md
Topic: pty
Date: 2026-09-12

> One concern per file. Implementation-level mechanics for `crates/pty` (`oneterm-pty`).
>
> This file is not in the owner's original list: it exists because `US-0071` is a packet in a
> high-risk lane, and `story create` requires a detail design. It is deliberately the shortest of
> the twelve — the extraction is a move, not a redesign.

## Concern

The pseudo-console transport, extracted from `alacritty_terminal::tty` into a crate of its own
**before** any engine work, so it survives the engine decision independently
([`../research/prior-art.md`](../research/prior-art.md) § 10 risk 13). It has three consumers
with different needs, which is what justifies a separate crate at all: `crates/local-shell`
(PTY plus grid), `crates/tools` (PTY, no grid), `crates/ssh` (grid, no PTY).

About 1 660 lines today, with no dependency on the grid.

## Design

### What is kept

The trait shape OneTerm's own poll loop already drives
(`crates/local-shell/src/event_loop.rs`), because
`crates/local-shell/src/event_loop_tests.rs:270-332` implements the whole contract over loopback
sockets and proves it is implementable outside the engine. The traits stay public for that
reason.

```rust
pub struct Options {
    pub shell: Option<Shell>,               // program + args
    pub working_directory: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub drain_on_exit: bool,
    pub glyph_width: GlyphWidth,            // NEW: see below
    #[cfg(unix)]    pub child_signal_mask: SignalMask,
    #[cfg(windows)] pub escape_args: bool,
}

pub struct WindowSize { pub rows: u16, pub cols: u16, pub cell_width: u16, pub cell_height: u16 }
// `cell_width`/`cell_height` are the pixel metrics the embedder already owns and passes on a font
// change; the engine gets the same pair through `Terminal::set_cell_pixels` for `CSI 14 t`.
// One owner, two destinations (R-40) — they are not stored twice.

pub trait EventedReadWrite {
    type Reader: Read;                 // associated types, not RPITIT (R-33): the trait stays
    type Writer: Write;                // object-safe and the reader type is nameable
    unsafe fn register(&mut self, poller: &Poller, interest: Event, mode: PollMode) -> io::Result<()>;
    fn reregister(&mut self, poller: &Poller, interest: Event, mode: PollMode) -> io::Result<()>;
    fn deregister(&mut self, poller: &Poller) -> io::Result<()>;
    fn reader(&mut self) -> &mut Self::Reader;
    fn writer(&mut self) -> &mut Self::Writer;
}
pub trait EventedPty: EventedReadWrite {
    fn next_child_event(&mut self) -> Option<ChildEvent>;
}
pub trait OnResize { fn on_resize(&mut self, size: WindowSize); }
pub enum ChildEvent { Exited(Option<ExitStatus>) }

pub const PTY_CHILD_EVENT_TOKEN: usize = 1;     // public on BOTH platforms
pub const PTY_READ_WRITE_TOKEN:  usize = 2;

pub struct PseudoConsole { /* platform inner */ }
impl PseudoConsole {
    pub fn spawn(options: &Options, size: WindowSize) -> io::Result<Self>;
    pub fn child_pid(&self) -> Option<u32>;     // one function, both platforms
}
```

Three API defects fixed while moving, each of which OneTerm works around today:

| Defect | Today | Fixed |
| --- | --- | --- |
| `PTY_CHILD_EVENT_TOKEN` is `pub(crate)` on Unix | hard-coded as `const … = 1` with a comment about the hazard (`crates/local-shell/src/event_loop.rs:58-64`) | exported on both platforms |
| The child pid is reached differently per platform (`pty.child().id()` on Unix, `pty.child_watcher().pid()` on Windows) | two cfg'd helper functions (`crates/local-shell/src/event_loop.rs:168-179`) | one `child_pid()` |
| `setup_env()` sets `TERM` and `COLORTERM` process-globally with `unsafe { env::set_var }` | never called; OneTerm sets them through `Options::env` (`crates/core/src/config/shell.rs`) | not ported |

### Windows

**The bundled console host is load-bearing and must be preserved.** OneTerm ships Windows
Terminal's matched `conpty.dll` + `x64/OpenConsole.exe` pair next to `oneterm.exe`, and
`oneterm-pty` must reproduce today's resolution order:

```
1. LoadLibraryW("conpty.dll")            // next to the executable, the bundled host
   + GetProcAddress for CreatePseudoConsole / ResizePseudoConsole / ClosePseudoConsole
2. kernel32                              // fallback, only when the files are missing
```

This is not a detail. The inbox `conhost.exe` on Windows 11 24H2 (10.0.26100) **swallows Sixel
DCS payloads**, so IN-0028's graphics work only survives the round trip through the bundled
host. The `LoadLibraryW` probe is therefore the primary path and `kernel32` is the degraded one;
a build with the files missing still runs, without Sixel passthrough.

`DEC-0013` is being rewritten (IN-0030's own `US-0070`, on another branch) to say exactly
this: the pair is bundled, versions are bumped through a script plus a manifest, and the two
binaries keep their `THIRD-PARTY-NOTICES.md` § 1 rows with their recorded SHA-256 hashes.
**Nothing in `oneterm-pty` may hard-code a kernel32-only path**, and no packaging or build step
may drop the pair.

```rust
pub enum ConptyBackend { Bundled { module: HMODULE }, System }
pub struct ConptyApi {
    create: PfnCreatePseudoConsole,
    resize: PfnResizePseudoConsole,
    close:  PfnClosePseudoConsole,
    backend: ConptyBackend,     // reported in diagnostics, so a bug report names the host
}
impl ConptyApi { pub fn resolve() -> io::Result<Self>; }
```

`ConptyApi::resolve()` is called once per process and its outcome is logged at `info`
(`"conpty: bundled"` or `"conpty: system"`), because a host-dependent defect must be reportable
with the host that produced it.

Minimum supported Windows is 10 1809 (build 17763), the first release whose `kernel32` exports
`CreatePseudoConsole` — that bound governs the fallback path only.

**Later option, not this packet: drop the DLL, keep `OpenConsole.exe`.** `conpty.dll` is a thin
launcher; the same job can be done from Rust by spawning
`OpenConsole.exe --headless --width <cols> --height <rows> --signal <handle> --server <handle>`
over a `\Device\ConDrv\Server` handle. That would remove one redistributed binary while keeping
the host that makes Sixel work. It is recorded here so the abstraction above
(`ConptyApi` behind an enum, resolved once) leaves room for a third `ConptyBackend::Spawned`
variant, and it is explicitly **not** in scope for `US-0071`.

Mechanics, preserved from the implementation being replaced:

- Two anonymous pipes; `CreatePseudoConsole(COORD { X: cols, Y: rows }, conin, conout, flags,
  &mut hpcon)`; a `STARTUPINFOEXW` whose attribute list carries
  `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE`; `CreateProcessW` with `EXTENDED_STARTUPINFO_PRESENT`
  (plus `CREATE_UNICODE_ENVIRONMENT` when a custom environment block is supplied).
  `STARTF_USESTDHANDLES` with null handles prevents handle inheritance.
- Custom environment: keys deduplicated **case-insensitively**, user entries winning, then the
  parent environment appended; the block is `name=value\0…\0\0` in UTF-16.
- `ClosePseudoConsole` **blocks until the conout pipe is drained**, so the pseudo-console handle
  must be the first field of the struct and therefore drop before the pipes. That ordering is an
  invariant with a comment, not an accident.
- `ChildExitWatcher`: `RegisterWaitForSingleObject(..., WT_EXECUTEINWAITTHREAD |
  WT_EXECUTEONLYONCE)`; the callback reads `GetExitCodeProcess`, pushes `ChildEvent::Exited` down
  an `mpsc::Sender`, and posts an IOCP completion packet so the poller wakes.
  `UnregisterWait` on drop. This is the race-free path and it stays.
- Blocking pipe I/O runs on two named threads feeding a ring buffer and posting IOCP completion
  packets, with one forced spurious readiness event on first registration so the first poll does
  not hang.
- `on_resize` calls `ResizePseudoConsole`. **A failed resize must not panic** — the
  implementation being replaced asserts `result == S_OK`
  ([`../research/engine-semantics.md`](../research/engine-semantics.md) § 6). Here it returns
  `io::Result` and the caller logs at `warn` and keeps the session alive, per
  `docs/agents/error-policy.md` (transport row: return a typed error, do not panic).
- Default shell remains `powershell`; `escape_args` selects between raw concatenation and
  MSVCRT-style quoting, with the existing quoting test table ported.

**`GlyphWidth` — the new option.** `CreatePseudoConsole`'s flags include
`PSEUDOCONSOLE_GLYPH_WIDTH_WCSWIDTH (0x10)`, `..._GRAPHEMES (0x08)`, `..._CONSOLE (0x18)` and
`..._AMBIGUOUS_IS_WIDE (0x20)` ([`../research/prior-art.md`](../research/prior-art.md) § 5.7).
If conhost measures character widths differently from the engine, columns drift on CJK and
emoji. The caller therefore passes the flag matching the engine's width mode
([`cell-and-style.md`](cell-and-style.md)): `WcsWidth` when mode 2027 is off (the default),
`Graphemes` when it is on. Known limitation: the flag is fixed at spawn, so a program that
enables 2027 mid-session keeps the spawn-time flag. Older hosts ignore the bits.

Note for the record: there is **no** live `PSEUDOCONSOLE_PASSTHROUGH_MODE`. It existed up to
about 1.18, was experimental, and `0x8` has since been reused for `GLYPH_WIDTH_GRAPHEMES`.
Advice to pass `0x8` for "passthrough" is wrong and must not be followed.

**`polling` is a public dependency (R-33).** `Poller`, `Event` and `PollMode` appear in
`EventedReadWrite`'s signatures, so `oneterm-pty`'s semver contract includes `polling`'s. That is
deliberate — OneTerm drives its own poll loop and the PTY must be a passive pollable object — and
it is recorded in `docs/agents/dependencies.md` § 3 at `US-0071` so a `polling` bump is understood
as a breaking change to this crate.

**Glyph width (R-38).** `Options::glyph_width` is fixed to `GlyphWidth::WcsWidth`, matching the
engine, because mode 2027 is deferred whole ([`cell-and-style.md`](cell-and-style.md), R-56). The
field exists so the later 2027 packet can set it at spawn; the engine will refuse `CSI ? 2027 h`
on a session whose transport was spawned with `WcsWidth`, so the engine and conhost can never
disagree about how to measure a cluster.

### Unix

`openpty`, `fork`, `setsid`, `TIOCSCTTY`, `TIOCSWINSZ`, and `SIGCHLD` through the existing
signal-mask option. Ported as-is; Linux and macOS compile and are packaged but are not
QA-tested (`docs/PROJECT.md`, "Platforms").

### What is deliberately not here

- No read loop, no notifier, no message enum. OneTerm owns its loop
  (`crates/local-shell/src/event_loop.rs`, whose header says so), and alacritty's `EventLoop`
  has zero references in `crates/`.
- No grid, no parser, no `Term`. `crates/ssh` must be able to depend on the engine without
  pulling in a PTY, which is the reason for the split.
- No `portable-pty`. `docs/agents/dependencies.md` § 3 rules it out explicitly.

## Interfaces

Listed above. The crate's whole public surface is `Options`, `Shell`, `WindowSize`,
`GlyphWidth`, `PseudoConsole`, the three traits, `ChildEvent`, and the two token constants.

## Edge Cases and Failure Modes

- [ ] **`ClosePseudoConsole` blocking on drop** — field order is an invariant with a comment and
  a test that drops a live pseudo-console with unread output.
- [ ] **Resize failure** returns `io::Result` instead of panicking.
- [ ] **The child exits before the watcher registers** — `RegisterWaitForSingleObject` fires
  immediately for an already-signalled handle; the test covers a child that exits instantly.
- [ ] **The platform watcher cannot read an exit code** — `ChildEvent::Exited(None)`; the
  session still ends.
- [ ] **A partial write of input** (keystrokes) — the writer reports the count; the caller
  retains the remainder, as it does today.
- [ ] **Spawn failure** (bad program, bad working directory) — `io::Error` with context, surfaced
  as a user-facing notification per `docs/agents/error-policy.md` (user-action row).
- [ ] **`conpty.dll` present but unloadable, or missing an export** — fall back to `kernel32`,
  log at `warn` naming the missing export, and keep the session. A half-loaded bundled host must
  never be fatal.
- [ ] **`conpty.dll` and `OpenConsole.exe` out of step** (one bumped without the other) — the
  pair is versioned together by the bump script and the manifest that IN-0030 owns; `oneterm-pty`
  does not validate the pairing, it only reports which backend it resolved.
- [ ] **Every `unsafe` block** carries a comment naming the invariant it maintains, per
  `docs/agents/code-style.md`.

## Verification

`cargo test -p oneterm-pty`

- [ ] `pty::tests::loopback_implements_the_evented_contract` — the loopback PTY from
  `crates/local-shell/src/event_loop_tests.rs:270-332`, moved here, proving the traits are
  implementable outside the crate.
- [ ] `pty::tests::child_exit_is_reported_once_with_its_code`
- [ ] `pty::tests::child_exit_without_a_code_still_ends_the_session`
- [ ] `pty::tests::instant_exit_is_not_missed`
- [ ] `pty::tests::resize_failure_returns_an_error_not_a_panic`
- [ ] `pty::tests::drop_order_drains_the_output_pipe`
- [ ] `pty::tests::windows_argument_escaping_table` — the existing quoting table, ported.
- [ ] `pty::tests::custom_environment_deduplicates_case_insensitively`
- [ ] `pty::tests::child_pid_is_available_on_both_platforms`
- [ ] `pty::tests::glyph_width_flag_is_passed_through` — asserts the flag bits for both width
  modes; the behavioural effect is only observable on a host that honours them.
- [ ] `pty::tests::conpty_api_prefers_the_bundled_host` — with `conpty.dll` present next to the
  test executable, `resolve()` reports `Bundled`; with it absent, `System`. This is the test that
  stops a future refactor from quietly turning Sixel passthrough off.
- [ ] `pty::tests::resolve_is_logged_once_with_the_backend_name`

Integration: `crates/local-shell` keeps its own loop tests unchanged in substance, and
`crates/tools/src/bin/pty-throughput.rs` still runs and still reports the ConPTY ceiling that
every engine benchmark is printed against ([`testing-and-bench.md`](testing-and-bench.md)).

Manual, on Windows, as `US-0071`'s exit criterion: a local shell opens, runs a command, resizes
with the window, and exits cleanly; `grep -rn "alacritty_terminal::tty" crates/` is empty.
