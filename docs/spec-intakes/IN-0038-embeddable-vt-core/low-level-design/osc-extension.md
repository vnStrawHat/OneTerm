# Low-Level Design: OSC extension and override

Intake: IN-0038
HLD: [`../high-level-design.md`](../high-level-design.md)
Topic: osc-extension
Date: 2026-09-15

> One concern per file. This file owns the OSC layer only: the routing table, the built-in arms, the
> typed events they emit, and the migration of every parser that lives in `crates/terminal` today.

## Concern

How an embedder of `oneterm-vt` gets OSC behaviour it can extend and override, without the engine
ever running embedder code, taking a lock, or allocating per sequence -- and how OneTerm's OSC 20308
agent channel and `TerminalSecurityPolicy` stay outside the crate while doing it.

"Allocating per sequence" is a **binding** claim on the routing table and on the built-in arms
alike, and `US-0098`'s verification found the arms breaking it: they each built a `String` to join
or decode what the batch arena was about to copy anyway. An arm that has to assemble a payload
assembles it **in the arena** -- `EventBatch::mark`, `extend`, then one `finish_trimmed` or
`finish_lossy` -- so the steady state allocates nothing. Two exceptions, both deliberate and both
the case that was going to cost an allocation whatever happened:

* `OSC 0` / `OSC 2` keep one owned `String`, because `Terminal::title` and the title stack own the
  title after the batch is gone. The event itself is a span.
* a percent escape that decodes to bytes that are not valid UTF-8 costs one repair in
  `finish_lossy`. A valid payload costs nothing.

## The problem the mechanism has to solve

Four requirements, and they fight:

1. **Built-ins ship in the core.** Titles, palette and colour queries, hyperlinks, clipboard,
   semantic prompts, cwd, progress, pointer and cursor shape must all work out of the box. An
   embedder should get a usable terminal from `Terminal::new(size, Config::default())`.
2. **Any number is extensible.** An embedder must be able to handle an OSC number the engine has
   never heard of -- OSC 20308, OSC 1337, whatever a future proposal invents -- with no fork.
3. **A built-in is overridable and wrappable.** Override: "I implement OSC 7 myself, do not parse
   it." Wrap: "keep parsing OSC 9, but also show me the raw bytes."
4. **The outcome is typed.** An embedder should get `Progress::Set(42)`, not four byte slices it
   has to reparse -- which is precisely the duplication this intake exists to delete.

And two invariants from `IN-0029` that are not negotiable:

- **No callback at feed time.** `crates/vt/src/lib.rs`: "The embedder owns the lock and the event
  loop". `events-and-api.md`: "An event is a **value**, never a callback: nothing runs inside the
  engine while the caller holds the lock". OneTerm's `OscRouter::drain` runs with `TerminalHandle`'s
  guard held; an engine that called back into it would reintroduce exactly the deadlock class
  `US-0082` deleted.
- **Hostile input never panics and is always counted.** Every malformed case moves a `FeedStats`
  counter and parsing continues. The `OSC_INLINE = 2048` / `OSC_LARGE = 8 MiB` two-tier ceiling and
  its per-number opt-in must survive unchanged.

## Options evaluated

### Option 1 -- a trait-object handler registry called during `feed`

```rust
pub trait OscHandler { fn osc(&mut self, code: u32, params: &OscParams<'_>, term: StringTerm); }
// Terminal holds Vec<(u32, Box<dyn OscHandler>)> and calls into it from dispatch.
```

This is the shape both reference crates use (alacritty's `Handler`, rio's `EventListener`), scaled
up to be registerable. **Rejected.** It breaks the no-callback invariant outright: embedder code
would run inside `feed`, under whatever lock the embedder holds, in the middle of a grid mutation.
It also means the handler cannot touch the terminal (it is mutably borrowed), cannot easily push a
reply, and turns every OSC into an indirect call on a path that a Sixel flood hits thousands of
times per second. And a `Box<dyn ...>` in `Config` makes `Config` no longer `Clone`, which it is
today and which an embedder building one terminal per pane relies on. (An earlier draft said
"neither `Clone` nor `PartialEq`, both of which it is today". `Config` has never been `PartialEq` —
`crates/vt/src/terminal/mod.rs` derives `Clone, Debug`. The rejection rests on the no-callback
invariant, which is the load-bearing half; `Clone` is the secondary cost. What is `PartialEq` is
`OscRoutes` itself, which is what this design needed.)

### Option 2 -- route the number, parse into typed events

The engine decides only **what to do with a number**, never **who handles it**. The decision is data
(two bits per code), the built-ins are ordinary match arms, and everything an embedder learns
arrives as a `VtEvent` value in the batch it already drains. **Selected.** It satisfies all four
requirements and both invariants, costs one bitmap lookup per OSC, and is a strictly smaller change
than what is there today: `OscClaims` already is this table with one of the four states missing.

### Option 3 -- `OscClaims` extended with a decoder function

```rust
claims.claim_with(20308, |params| -> Option<T> { ... });
```

**Rejected**, for two reasons that are each fatal. The engine cannot name `T` without a generic
parameter on `Terminal` (`Terminal<E>` infects every signature, every field of the adapter, and the
`RenderState` borrow) or a `Box<dyn Any>` (an allocation per OSC on the hot path, plus a downcast at
the far end -- worse ergonomics than the byte slices it was meant to improve). And a function
pointer is still a call at feed time: it inherits Option 1's problem with none of its flexibility.

The part of Option 3 worth keeping is its instinct that the *ceiling* belongs on the number. That
already exists as `claim_large` and survives verbatim.

## Design

### The routing table

`OscClaims` becomes `OscRoutes`. Internally it is the same structure -- a `[u64; 32]` bitmap over
codes `0..2048` plus a sorted `Vec<u32>` spill for larger numbers -- three times over, plus the
existing large bitmap:

```rust
/// What the engine does with one OSC number.
///
/// The default for a number the engine implements is [`OscRoute::Builtin`];
/// the default for every other number is [`OscRoute::Drop`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum OscRoute {
    /// The engine's own handler runs and emits its typed event. The raw
    /// sequence is not delivered.
    Builtin,
    /// The engine's own handler runs *and* the raw parameters are delivered as
    /// [`VtEvent::Osc`], after the typed event. Use this to observe a number
    /// without changing what the terminal does with it.
    BuiltinAndForward,
    /// The engine's own handler is skipped; only [`VtEvent::Osc`] is delivered.
    /// Use this to replace a built-in, or to handle a number the engine does
    /// not implement.
    Forward,
    /// Nothing happens. The sequence is parsed, counted in
    /// [`FeedStats::unhandled_sequences`], and discarded.
    Drop,
}

/// Which OSC numbers the engine handles, forwards, or ignores.
///
/// Cheap to clone and compare; holds no allocation for any number below 2048.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct OscRoutes { /* forward, suppress, large: bitmap + sorted spill each */ }

impl OscRoutes {
    /// The OSC numbers the engine implements itself, in the order the reference
    /// tabulates them.
    pub const BUILTIN: [u32; 18] =
        [0, 1, 2, 4, 7, 8, 9, 10, 11, 12, 22, 50, 52, 104, 110, 111, 112, 133];

    /// A table in which every built-in runs and nothing is forwarded.
    pub fn new() -> OscRoutes;

    /// Whether the engine implements this number itself.
    pub fn has_builtin(code: u32) -> bool;

    /// Set the route for one number. Idempotent; later calls win.
    pub fn route(&mut self, code: u32, route: OscRoute) -> &mut Self;

    /// Set the route for several numbers at once.
    pub fn route_all(&mut self, codes: &[u32], route: OscRoute) -> &mut Self;

    /// Allow this number's payload to grow from [`parser::OSC_INLINE`] to
    /// [`parser::OSC_LARGE`].
    ///
    /// A **memory ceiling only**, orthogonal to the route: who may write the
    /// clipboard and under what limits is the embedder's policy, not the
    /// engine's. `large(52, true)` buys a large clipboard write while OSC 52
    /// stays a built-in.
    pub fn large(&mut self, code: u32, allow: bool) -> &mut Self;

    pub fn get(&self, code: u32) -> OscRoute;
    pub fn allows_large(&self, code: u32) -> bool;

    /// Every number whose route differs from the default, for diagnostics.
    pub fn overrides(&self) -> impl Iterator<Item = (u32, OscRoute)> + '_;
}
```

`OscRoute` is stored as two independent bits, and the set of numbers the engine implements is a
third bitmap built from `OscRoutes::BUILTIN` at compile time, so the hot path is three bit tests and
no branch table (a fourth answers `allows_large` before the parser buffers; a number above 2048
costs a binary search of the spill instead):

| `forward` | `suppress` | `has_builtin` | `get()` returns |
| --- | --- | --- | --- |
| 0 | 0 | yes | `Builtin` |
| 0 | 0 | no | `Drop` |
| 1 | 0 | yes | `BuiltinAndForward` |
| 1 | 0 | no | `Forward` |
| 0 | 1 | either | `Drop` |
| 1 | 1 | either | `Forward` |

The bits stored are the bits of the route the number will **actually** get, not of the one that was
asked for: `BuiltinAndForward` on a number with no built-in stores `Forward`'s bits, and `Drop` on a
number that has no built-in stores nothing at all. Two tables that route every number the same way
are therefore the same value -- `PartialEq` is semantic, not structural -- and a number above 2048
put back to its default leaves the spill list instead of sitting in it for ever.

The table is read when a `Terminal` is built and is **not live**. `Terminal::config` hands out a
shared reference and there is no setter, so an embedder that wants a different route mid-session
builds a new terminal. This is deliberate: a route that could change under a half-parsed sequence
would be a race the engine has no way to describe, and the batch-of-values contract has no place to
report it.

`route(code, Builtin)` on a number with no built-in is a **debug assertion**, the same shape as
today's `claim` on a `NATIVE` number: the caller asked for something that can never happen and would
otherwise never find out. It is an assertion and not a panic in release, because `Config` can be
built from data.

`large(code, true)` on a number whose route is `Drop` is also a debug assertion: the payload ceiling
was bought and the payload is thrown away, which is a pure memory hazard with no benefit. The
assertion reads the table **as it stands**, so the ceiling must be bought after the route:
`route(n, Forward).large(n, true)`, never the other way round. The builder chain makes that order
natural; a table assembled from configuration data in an arbitrary order should apply every route
before any ceiling.

### The dispatch path

`Handler::osc` becomes:

```rust
fn osc(&mut self, code: Option<u32>, params: &OscParams<'_>, term: StringTerm, truncated: bool) {
    self.state.dispatched = true;
    if truncated {
        self.state.stats.truncated_osc = self.state.stats.truncated_osc.saturating_add(1);
    }
    let Some(code) = code else { return self.unhandled() };
    let route = self.state.config.osc_routes.get(code);
    if matches!(route, OscRoute::Builtin | OscRoute::BuiltinAndForward) {
        self.osc_builtin(code, params, term);   // the match that exists today, plus six arms
    }
    if matches!(route, OscRoute::Forward | OscRoute::BuiltinAndForward) {
        self.forward_osc(code, params, term, truncated);
    }
    if matches!(route, OscRoute::Drop) {
        self.unhandled();
    }
}
```

Order inside a `BuiltinAndForward` is **typed event first, raw second**. That matters and is a
contract: an embedder wrapping a number wants to see the engine's interpretation before the bytes it
is second-guessing, and the batch preserves byte order across sequences regardless.

`osc_builtin` keeps the existing arms and gains six. Every new arm obeys the same rules the existing
ones do: no `Result`, no panic, every rejection moves `unhandled_sequences`, and every string
payload goes through the batch arena rather than a `String` per sequence.

### New built-in arms

| OSC | Arm | Emits |
| --- | --- | --- |
| 1 | icon name (`US-0102`) | `VtEvent::IconName(StrSpan)` |
| 7 | `file://host/path`, percent-decoded, Windows drive slash stripped | `VtEvent::Cwd { host: StrSpan, path: StrSpan }` |
| 9;4 | ConEmu progress, `st` in `0..=4`, `pr` clamped to 100 (both read as `u32`: the adapter read them as `u8`, so `9;4;1;1000` failed the parse and became `Set(0)` before the clamp could run, and a state above 255 silently became `Remove`) | `VtEvent::Progress(Progress)` |
| 9;`<text>` | desktop notification, remaining parameters rejoined on `;` | `VtEvent::Notification { title: StrSpan, body: StrSpan }` |
| 22 | pointer shape name, passed through verbatim | `VtEvent::Pointer(StrSpan)` |
| 50 | cursor shape (already applies to engine state) | `VtEvent::CursorStyleChanged` |
| 133 | `A` / `B` / `C` / `D[;exit]` (already marks the template) | `VtEvent::ShellMark(ShellMark)` |

with:

```rust
/// ConEmu taskbar progress, `OSC 9 ; 4 ; state ; percent`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum Progress {
    /// `state = 0`: clear the indicator.
    Remove,
    /// `state = 1`: normal progress, 0..=100.
    Set(u8),
    /// `state = 2`: error, 0..=100.
    Error(u8),
    /// `state = 3`: indeterminate.
    Indeterminate,
    /// `state = 4`: paused, 0..=100.
    Paused(u8),
}

/// A shell-integration boundary, `OSC 133`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum ShellMark {
    PromptStart,
    PromptEnd,
    OutputStart,
    OutputEnd { exit_code: Option<i32> },
}
```

`VtEvent::Cwd` carries `host` and `path` **separately and unresolved**. The engine does not build a
`PathBuf`, does not consult the current directory, does not check whether the host is this machine,
and does not touch the filesystem. That split is the whole boundary: a remote shell sending
`OSC 7 ; file://evil/../../etc` is data, and deciding what to do with it is the embedder's
`sanitize_cwd`.

`VtEvent::Notification` splits `title` and `body` because `OSC 777;notify;<title>;<body>` and
`OSC 9;<body>` are the same event with different shapes; for `OSC 9` the title span is empty. (OSC
777 itself is out of scope for this intake; the variant is shaped so adding it later is a new arm,
not a new variant.)

### Payload ceilings and hostile input

Unchanged in mechanism, extended in coverage:

- The parser asks `allows_large(code)` **before** buffering, exactly as today
  (`crates/vt/src/parser/osc.rs:133`). A number the embedder has not opted into is capped at 2048
  bytes no matter its route, so a `Forward` route cannot be used to buy memory.
- `truncated` is reported on the forwarded event as it is today, and is now also reported to the
  built-in arms, which reject a truncated payload where truncation makes it a lie rather than merely
  short. Concretely: `OSC 7` and `OSC 9;4` reject truncated input (a cut path or a cut number is
  wrong, not partial); `OSC 0/1/2`, `OSC 9;<text>` and `OSC 22` accept it (a cut title is still a
  title) -- which is the same distinction `osc_router.rs:212-223` already draws for the agent
  channel, lifted into the engine and generalised.
- No new `FeedStats` counter. Every new rejection path moves `unhandled_sequences`; a base64 or
  UTF-8 failure moves `malformed_sequences`, as `osc_clipboard` does today.
- **The ceiling is unchanged; the cost per ceiling-sized sequence on a wrapped number is not.** A
  `BuiltinAndForward` number pays for both outcomes: the built-in arm's payload *and* the forwarded
  parameters land in the arena. OneTerm wraps exactly one number, `OSC 9`, and buys it the 8 MiB
  tier for the legacy alias's sake, so one hostile 8 MiB `OSC 9` now costs the parser spill plus two
  arena copies where it used to cost the spill plus one. Bounded, verified
  (`v_a_hostile_8_mib_osc_9_under_the_shipped_table_is_bounded`), and the price of the owner's
  ruling on the alias -- but it is the reason to keep `BuiltinAndForward` rare rather than the
  default for anything an embedder is merely curious about.
- The existing fuzz target (`crates/vt/fuzz/fuzz_targets/parser.rs`) gains a randomised
  `OscRoutes` derived from the fuzz input's first bytes, so every route combination is fuzzed rather
  than only the default.

## Interfaces

New and changed `pub` items, exhaustively:

```rust
// crates/vt/src/terminal/osc.rs
pub enum  OscRoute { Builtin, BuiltinAndForward, Forward, Drop }     // new, #[non_exhaustive]
pub struct OscRoutes { .. }                                          // renamed from OscClaims
impl OscRoutes {
    pub const BUILTIN: [u32; 18];
    pub fn new() -> OscRoutes;
    pub fn has_builtin(code: u32) -> bool;
    pub fn route(&mut self, code: u32, route: OscRoute) -> &mut Self;
    pub fn route_all(&mut self, codes: &[u32], route: OscRoute) -> &mut Self;
    pub fn large(&mut self, code: u32, allow: bool) -> &mut Self;
    pub fn get(&self, code: u32) -> OscRoute;
    pub fn allows_large(&self, code: u32) -> bool;
    pub fn overrides(&self) -> impl Iterator<Item = (u32, OscRoute)> + '_;
}

// crates/vt/src/events/vt_event.rs
pub enum Progress   { Remove, Set(u8), Error(u8), Indeterminate, Paused(u8) }  // new
pub enum ShellMark  { PromptStart, PromptEnd, OutputStart, OutputEnd { exit_code: Option<i32> } }
pub enum VtEvent {                                                   // becomes #[non_exhaustive]
    // ... the thirteen existing variants, unchanged ...
    Cwd { host: StrSpan, path: StrSpan },                            // new
    IconName(StrSpan),                                               // new
    Notification { title: StrSpan, body: StrSpan },                  // new
    Pointer(StrSpan),                                                // new
    Progress(Progress),                                              // new
    ShellMark(ShellMark),                                            // new
    CursorStyleChanged,                                              // new
}

// crates/vt/src/terminal/mod.rs
pub struct Config { /* .. */ pub osc_routes: OscRoutes, /* was: osc_claims: OscClaims */ }
```

Removed: `OscClaims`, `OscClaims::NATIVE`, `OscClaims::is_native`, `OscClaims::claim`,
`OscClaims::claim_large`, `OscClaims::is_claimed`. No deprecation shim: the crate has not been
published yet, so this is the last moment the rename is free.

## Migration of each adapter parser

`crates/terminal/src/osc.rs` is 623 lines, of which roughly 420 are parsing and its tests. After
this packet it holds only what is OneTerm's.

| Adapter item today | Where it goes | Note |
| --- | --- | --- |
| `parse_osc` OSC 7 arm, `parse_cwd_url`, `percent_decode`, `strip_windows_drive_slash` | **engine**, OSC 7 built-in | `CORR-46` (the `file:///C:/...` drive-slash rule) moves with them; it is a wire-format rule, not a policy |
| `parse_osc` OSC 9;4 arm, `TerminalProgress` | **engine**, as `Progress` | the adapter re-exports `oneterm_vt::Progress as TerminalProgress` for one release so `crates/terminal-view` does not churn in the same packet |
| `parse_osc` OSC 9 notification arm | **engine**, as `Notification` | rejoining on `;` moves; the 8 KiB cap and the ten-per-second limiter **stay** in `security_policy.rs` |
| `parse_osc` OSC 133 arm, `Osc133Kind` | **engine**, as `ShellMark` | the adapter keeps `prompt_count` / `last_exit_code` caching and `SessionEvent::ShellIntegration` |
| `OscPayload` enum | **deleted** | it was the adapter's private re-typing of what the engine now emits |
| `parse_osc`'s `u32` re-parse of `params[0]` | **deleted** | it existed because the engine forwarded the number as bytes; the typed events carry no number |
| `AGENT_OSC` / `LEGACY_AGENT_OSC` arms, `parse_agent_status_param`, `agent_support_reply`, `osc_agent/` | **stays in the adapter** | the worked example; see below |
| `osc_color.rs` (`DynamicColors`, `PendingColorQuery`, `ColorFormatter`, `default_color_for_key`) | **stays** | `VtEvent::ColorQuery` already exists and the theme fallback is the embedder's |
| `encode_osc52` | **stays** | it formats a *reply*; the engine never writes a clipboard |
| `security_policy.rs` in full | **stays** | decision (f) |
| `osc_router.rs` | **stays**, shrinks | its `VtEvent::Osc` arm loses the `parse_osc` call and gains typed arms |

`OscRouter::handle` after the move, for the four numbers that change:

```rust
VtEvent::Cwd { host, path } => {
    // Policy, not parsing: the engine gave us bytes, we decide if we trust them.
    let raw = batch.str(*path);
    if let Some(sanitized) = self.security.sanitize_cwd(raw) {
        let dir = PathBuf::from(&sanitized);
        self.state.lock().cwd = Some(dir.clone());
        out.push(SessionEvent::Cwd(dir));
    }
}
VtEvent::Progress(progress) => out.push(SessionEvent::Progress(*progress)),
VtEvent::Notification { body, .. } => {
    let Some(text) = self.security.sanitize_notification(batch.str(*body)) else { return };
    if self.notification_limiter.lock()...allow() {
        out.push(SessionEvent::Notification(text));
    }
}
VtEvent::ShellMark(mark) => { /* prompt_count / last_exit_code, then SessionEvent */ }
```

Note what did **not** change: every policy call is still there, in the same order, on the same
thread, with the same limits. Only the parsing moved.

### The worked example: OSC 20308 stays outside

`adapter_config` (`crates/terminal/src/handle.rs:184`) becomes:

```rust
fn adapter_config(scrollback: usize) -> Config {
    let mut routes = OscRoutes::new();
    // The agent channel is OneTerm's own proposal (docs/osc-agent-status.md),
    // not a terminal standard, so the engine must not know it exists. It is
    // above the 2048-bit bitmap, so it lands in the sorted spill list.
    routes.route(AGENT_OSC, OscRoute::Forward).large(AGENT_OSC, true);
    // Base64-wrapped JSON: a tool-call event with a diff runs past OSC_INLINE.
    routes.large(52, true);
    Config { osc_routes: routes, product_name: Some("OneTerm(..)".into()), .. }
}
```

Three lines. No engine change, no fork, no `[patch]`. Everything else -- the sub-code match, the
base64 decode, the JSON schema validation, the `seq` dedup, the deprecation counter, the support
reply, the truncation drop -- stays in `crates/terminal/src/osc_agent/` where it already is. That is
the demonstration the mission statement asks for, and it is also the regression test: if a future
change to the engine makes OSC 20308 need an engine edit, the mechanism has failed.

### The `OSC 9;7` collision

The legacy agent alias is `OSC 9;7;<base64>`. With `OSC 9` as a clean `Builtin`, the engine's
notification arm would parse it as a desktop notification with body `7;<base64>`. Two ways out:

1. **Drop the alias.** `osc_router.rs:264` and `docs/osc-agent-status.md` section 3.1 already say it
   "is dropped in the next release". `OSC 9` stays a clean built-in and the adapter keeps only
   `route(20308, Forward)`. Was the author's recommendation; **overruled by the owner**
   (2026-09-15): the alias is OneTerm's custom spelling and is handled outside the engine, so
   option 2 below is what `US-0098` implements.
2. **Keep it**, with `routes.route(9, OscRoute::BuiltinAndForward).large(9, true)`. The adapter then
   sees both `VtEvent::Notification` and `VtEvent::Osc { code: 9, .. }` for the same sequence, and
   drops the notification whose forwarded first parameter is `7`. One line in the adapter, and the
   legacy knowledge stays in the crate that owns the legacy. It works, it is ugly, and it is
   *exactly* what `BuiltinAndForward` is for -- so it is a fair demonstration of the wrap mode even
   though the alias should die.

This collision is not specific to `OSC 9;7`; it is the general property that routes are per-number
and OSC sub-codes are payload. Per-sub-code routing was considered and rejected: it would double the
table, and the two-event wrap already covers the case in one adapter line.

## Why this differs from alacritty's `Handler` and rio-vt's `EventListener`

| | `alacritty_terminal` / `vte` | `rio-vt` | `oneterm-vt` after IN-0038 |
| --- | --- | --- | --- |
| Where OSC numbers are matched | `vte/src/ansi.rs:1329`, a fixed `match params[0]` in a **different crate** | `rio-vt/src/performer/handler.rs:1095`, a fixed `match params[0]` | `osc_builtin`, a fixed match **preceded by a routing lookup** |
| Numbers handled | 14 | 17 | 18 built-in plus any number |
| Unknown number | `debug!("[unhandled osc_dispatch]")`, dropped | `unhandled(params)`, dropped | `Drop` by default; `Forward` on request |
| Adding a number | fork `vte` | fork `rio-vt` | one `route()` call |
| Overriding a built-in | impossible | impossible | `route(n, Forward)` |
| Observing a built-in | impossible | impossible | `route(n, BuiltinAndForward)` |
| Delivery | `EventListener::send_event(&self, Event)` -- a callback invoked inside `Term` | four default-bodied `EventListener` callbacks, events tagged with `route_id` | values pushed into an `EventBatch` the caller drains |
| Embedder code runs inside the parser | yes | yes | **no** |

The substantive difference is the **separation of routing from handling**. Both reference crates fuse
them: the `match` decides both "which number is this" and "what do I do about it", so the only
extension point is the trait whose methods that match calls, and that trait's method set is the OSC
set. Splitting them costs three bit tests and buys the entire requirement list.

The second difference is **delivery by value**. Alacritty and rio both invoke a listener from inside
the terminal, which forces `Arc<dyn Fn(..) -> String + Send + Sync>` closures into their event enums
for anything that needs a reply (`ClipboardLoad`, `ColorRequest`, `TextAreaSizeRequest` in both).
`oneterm-vt` has no such variant and needs none: the embedder answers after the batch, off the lock,
from its own state. That is not an improvement this intake makes -- `IN-0029` already made it -- but
it is why the routing table can be pure data instead of a handler registry.

Where this design is *worse*: an embedder who wants to add an OSC number gets byte slices and has to
parse them, where an alacritty fork would have got a typed method. That is the deliberate trade. The
built-in set covers what is standard; anything past it is by definition non-standard, and giving
non-standard payloads a typed shape in the core is how a core stops being a core.

## Edge Cases and Failure Modes

- [ ] `route(code, Builtin)` where `has_builtin(code)` is false -- debug assertion; release behaviour
  is `Drop`, and `get()` reports `Drop`, so the caller can detect it.
- [ ] `large(code, true)` where the route is `Drop` -- debug assertion; the ceiling is not applied.
- [ ] A number above 2048 -- lands in the sorted spill `Vec`; `route()` is `O(log n)` insert,
  `get()` is `O(log n)`. Unchanged from today's `OscClaims`.
- [ ] An OSC with no number (`ESC ] ; foo ST`) -- `unhandled()`, as today.
- [ ] A truncated payload on a built-in that rejects truncation (`OSC 7`, `OSC 9;4`) -- dropped and
  counted in `unhandled_sequences`, and **not** forwarded either, because a `BuiltinAndForward`
  route still forwards the raw truncated bytes for an embedder that wants to count them.
- [ ] `OSC 4` with more than 16 parameters -- the existing `osc_args` re-split on the sixteenth slot
  (correction `C7`) is untouched.
- [ ] A payload that is valid base64 but invalid UTF-8 on `OSC 52` -- `malformed_sequences`, silent
  drop, as today.
- [ ] `OSC 9;4;99;999` -- percent clamps to 100; state 99 is rejected and counted.
- [ ] `OSC 7` with a `file://` URL whose path is empty, or a bare relative path -- reported verbatim
  in the `path` span; the engine does not resolve or reject. The embedder's `sanitize_cwd` does.
- [ ] `OSC 133;D;not-a-number` -- `OutputEnd { exit_code: None }`, matching the adapter's behaviour
  today.
- [ ] An `OscRoutes` built by an embedder that routes **every** number to `Forward` -- legal; the
  engine becomes a pure parser and every OSC arrives raw. This is the escape hatch for an embedder
  that wants the reference crates' model back, and it is a test case.

## Verification

- [ ] **Routing truth table.** All six rows of the `forward`/`suppress`/`has_builtin` table, for a
  built-in number, a non-built-in number below 2048, and a number above 2048. Asserts `get()` and
  the observable behaviour of one `feed` each.
- [ ] **Override.** `route(0, Forward)`, feed `OSC 0;hello ST`: exactly one `VtEvent::Osc { code: 0 }`
  and **zero** `VtEvent::Title`; the terminal's title is unchanged.
- [ ] **Wrap.** `route(0, BuiltinAndForward)`, same input: exactly two events, `Title` then `Osc`, in
  that order.
- [ ] **Suppress.** `route(8, Drop)`, feed a hyperlink: no event, no hyperlink on the cell,
  `unhandled_sequences` incremented by one.
- [ ] **Extend.** `route(20308, Forward).large(20308, true)`, feed a 3 MiB payload: one
  `VtEvent::Osc` with `truncated == false` and the full payload readable through
  `batch.params(..)`. Without `large`, the same input yields `truncated == true` and 2048 bytes.
- [ ] **Migration equivalence -- the hostile-verifier criterion.** Every test case in
  `crates/terminal/src/osc.rs`'s current `mod tests` (OSC 7 forms including `file:///C:/Users`,
  `%20` decoding, the host form, the bare-path form; OSC 9;4 states 0 to 4 and the clamp; OSC 9
  notification with embedded `;`; OSC 133 A/B/C/D and D with and without an exit code) moves to
  `crates/vt` **unchanged in input and expectation**, rewritten only to assert on a `VtEvent`
  instead of an `OscPayload`. A reviewer diffs the two test modules and every input literal must
  appear on both sides.
- [ ] **Ceiling preservation.** The existing `crates/vt/tests/parser_limits.rs` suite passes
  unchanged, plus one new case: a `Forward`-routed number without `large` cannot exceed
  `OSC_INLINE`.
- [ ] **Hostile input.** The existing proptest `arbitrary_bytes_never_panic_and_chunking_is_invariant`
  is extended to draw an arbitrary `OscRoutes` alongside the byte stream; it must still never panic,
  and `FeedStats` counters must stay monotone within a batch.
- [ ] **Fuzz.** `crates/vt/fuzz/fuzz_targets/parser.rs` derives its route table from the input;
  a one-hour local run with no crash is the gate, recorded in the packet, not in CI.
- [ ] **The example is the proof of ergonomics.** `crates/vt/examples/headless.rs` registers a
  custom OSC number and prints what it receives, in under fifteen lines of embedder code. If it
  takes more, the mechanism is wrong.
- [ ] **OneTerm end to end.** `cargo test -p oneterm-terminal -p oneterm-local-shell -p oneterm-ssh`
  unchanged, plus a manual Windows walk: cwd following in the SFTP panel (OSC 7), an agent event in
  the Agent Panel (OSC 20308), a `printf` progress sequence (OSC 9;4), and a prompt mark
  (OSC 133).
