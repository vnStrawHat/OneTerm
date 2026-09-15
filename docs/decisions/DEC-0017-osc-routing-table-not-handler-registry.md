# DEC-0017 OSC extension is a routing table, not a handler registry

Date: 2026-09-15

## Status

Accepted (owner ruling, 2026-09-15: routing table with Builtin / BuiltinAndForward / Forward / Drop per OSC number, no embedder code inside `feed`; custom spellings such as OneTerm's OSC 9;7 and 20308 stay outside the engine)

## Context

`IN-0038` makes `oneterm-vt` an embeddable terminal core and moves OSC handling into it. The mission
statement asks for "a convenient mechanism to extend and override" OSC handling, and that mechanism
is the one part of the intake that would be expensive to change after the crate's first crates.io
release: it appears in `Config`, in `VtEvent`, and in every embedder's setup code.

Neither reference core has such a mechanism. `alacritty_terminal` matches OSC numbers in `vte`'s
`ansi.rs:1329` -- a different crate -- with a fixed `match params[0]` and a `debug!` on anything
unknown; `rio-vt` does the same in its own `performer/handler.rs:1095`. In both, the only extension
point is a trait of default-bodied methods whose method set *is* the OSC set, so adding a number
means forking. OneTerm's own OSC 20308 agent-status proposal is exactly that case, and forking is
the history `DEC-0014` exists to end.

Two engine invariants constrain any answer. `crates/vt/src/lib.rs` and
`IN-0029/low-level-design/events-and-api.md` state that the engine holds no lock and no interior
mutability, and that "an event is a **value**, never a callback: nothing runs inside the engine while
the caller holds the lock". OneTerm's `OscRouter::drain` runs with `TerminalHandle`'s guard held, so
an engine that called back into an embedder would reintroduce the deadlock class `US-0082` removed.
Separately, terminal input is untrusted: nothing may panic, every malformed case moves a `FeedStats`
counter, and the two-tier OSC payload ceiling (`OSC_INLINE = 2048`, `OSC_LARGE = 8 MiB`, opt-in per
number) must survive.

## Decision

OSC extensibility is expressed as **per-number routing data**, not as registered handlers.

`Config::osc_routes: OscRoutes` maps every OSC number to one of four routes:

- `Builtin` -- the engine's own handler runs and emits its typed event.
- `BuiltinAndForward` -- the handler runs **and** the raw parameters are also delivered as
  `VtEvent::Osc`, typed event first. This is how an embedder observes or wraps a built-in.
- `Forward` -- the handler is skipped; only `VtEvent::Osc` is delivered. This is how an embedder
  overrides a built-in, and how it handles a number the engine does not implement.
- `Drop` -- parsed, counted in `FeedStats::unhandled_sequences`, discarded. The default for any
  number with no built-in.

The table is two bitmaps over codes `0..2048` plus a sorted spill list, and the set of numbers the
engine implements is a third, built from `OscRoutes::BUILTIN` at compile time. A lookup is therefore
three bit tests and no allocation, plus a fourth for the payload ceiling the parser asks about
before it buffers. A number above 2048 costs a binary search of the spill instead. The payload ceiling stays a separate, orthogonal per-number opt-in
(`OscRoutes::large`), because who may spend 8 MiB is a memory question and who parses the bytes is a
routing question.

Future work must inherit three consequences:

1. **The engine never calls embedder code during `feed`.** Any proposal that adds a closure, a trait
   object, or a function pointer to the OSC path contradicts this decision and needs a new one.
2. **Built-in outcomes are typed events, raw outcomes are byte spans.** A number the engine
   implements gets a `VtEvent` variant with parsed fields; a number it does not gets
   `VtEvent::Osc { code, params, terminator, truncated }` and the embedder parses. Giving
   non-standard payloads typed shapes in the core is how a core stops being a core.
3. **Routing is per number, never per sub-code.** OSC sub-codes are payload. An embedder that needs
   sub-code-level behaviour uses `BuiltinAndForward` and discriminates in its own code.

## Alternatives

- [x] **Selected: a routing table of pure data.** Satisfies extend, override, wrap and typed
  outcomes; keeps `Config` `Clone`, and makes the table itself `Clone + PartialEq`; costs three bit
  tests per OSC, plus a fourth for the payload ceiling; and is a smaller change than the status quo,
  because today's `OscClaims` is already this table with one of the four states missing.
- [ ] **A trait-object handler registry called during `feed`** (`Vec<(u32, Box<dyn OscHandler>)>`) --
  the reference crates' shape made registerable. Rejected: it runs embedder code inside `feed`, under
  the embedder's own lock, mid-grid-mutation, which is the invariant above; the handler cannot touch
  the terminal because it is mutably borrowed; it puts an indirect call on a path a Sixel flood hits
  thousands of times a second; and a `Box<dyn ...>` in `Config` costs `Clone`, which `Config` has
  today and which every embedder building one terminal per pane depends on. (An earlier draft of
  this record also claimed `Config` is `PartialEq`. It is not, and never has been —
  `crates/vt/src/terminal/mod.rs` derives `Clone, Debug`. The rejection rests on the no-callback
  invariant above, which is the load-bearing half; `Clone` is the secondary cost. What is
  `PartialEq` is `OscRoutes` itself, which is what the design actually needed.)
- [ ] **`OscClaims` extended with a decoder function** (`claim_with(code, |params| -> Option<T>)`).
  Rejected twice over: the engine cannot name `T` without a generic parameter on `Terminal`, which
  infects every signature and every field of the adapter, or a `Box<dyn Any>`, which is an
  allocation per OSC plus a downcast -- worse ergonomics than the byte slices it was meant to
  replace. And a function pointer is still a call at feed time, inheriting the first alternative's
  problem with none of its flexibility. Its one good instinct, that the payload *ceiling* belongs on
  the number, already exists as `claim_large` and is kept verbatim.
- [ ] **Copy `alacritty_terminal` and `rio-vt`: a fixed set, no extension.** Rejected by the mission
  statement, and by the fact that OneTerm would immediately need the escape hatch for OSC 20308.

## Consequences

- [x] **Benefit confirmed** (`US-0098`, independently verified 2026-09-15): OneTerm's OSC 20308
  agent channel keeps working through three lines in `adapter_config` and **zero** lines in
  `crates/vt`. `grep -rn '20308\|AGENT_OSC' crates/vt/` returns nothing. If a future change to the
  engine ever makes the agent channel need an engine edit, this decision has failed and should be
  revisited rather than patched around.
- [~] **Benefit partly confirmed:** every OSC number is parsed once — the double-parse of OSC 7, 9
  and 133 is gone, and that half holds. The line count does not: `crates/terminal` shed **151**
  production lines, not 350. The estimate assumed the adapter would lose the parsers and gain
  nothing, and it gained the agent-channel dispatch that used to live inside `parse_osc`
  (`parse_agent_osc`, `AgentOsc`, `is_legacy_agent_notification`) plus four typed router arms in
  place of one raw one. `crates/vt` grew by 527, so the pair is not a wash either — which is the
  honest shape of moving parsing into a core: the core carries it, and the embedder keeps its
  policy. Future estimates on this intake should not use a line count as the proxy for "the
  duplication is gone"; the symbol list is the proxy that held.
- [ ] **Tradeoff:** an embedder extending the engine with a new OSC number gets byte slices and
  parses them itself, where a fork of `alacritty_terminal` would have got a typed method. This is
  deliberate and is the boundary that keeps the built-in set equal to the standard set.
- [ ] **Tradeoff:** routing is per number, so a built-in whose sub-codes an embedder wants to split
  (OneTerm's deprecated `OSC 9;7` is the only live instance) needs `BuiltinAndForward` plus a
  discriminating line in the embedder. The owner ruled (2026-09-15) that this custom spelling is
  handled outside the engine, so that instance stays and exercises the wrap route.
- [ ] **Follow-up:** `OscRoute` and `VtEvent` are `#[non_exhaustive]` so a fifth route or a new
  built-in event stays a patch-level change under the semver promise in
  `docs/spec-intakes/IN-0038-embeddable-vt-core/low-level-design/api-surface.md`.
- [ ] **Follow-up:** the mechanism is only proven convenient if the headless example registers a
  custom OSC number in under fifteen lines of embedder code. If it takes more, reopen this decision
  rather than shipping the example.
