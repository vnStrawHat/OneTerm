# High-Level Design: VT gaps

Intake: [`IN-0039`](IN-0039.md)
Lane: high_risk
Date: 2026-09-15

## Idea

An outside developer scored `oneterm-vt` 40 against `alacritty_terminal`'s 33 and `rio-vt`'s 26,
recommended it, and then wrote down five things that kept the recommendation conditional. Four of
them are gaps between what the crate **says** and what it **does**. The fifth -- that the crate is
not on crates.io -- is a trade-off the owner has weighed and kept; it is recorded in
[`IN-0039`](IN-0039.md), "Gap 3 is accepted, not scheduled", and no packet here touches it.

This intake closes the four, and it closes them in that framing rather than as a feature list,
because the crate's own stated rule -- guide chapter 11, "never claim a capability that does not
exist" -- is what three of the four violate.

Nothing here changes the engine's architecture. No lock is introduced, no callback, no dependency,
no crate. Every packet adds code inside a seam that already exists: `input::encode_key`, the CSI
and DCS dispatch tables, `vt-bench`'s tier list, and `lib.rs`'s re-export block.

The three claims the packets have to make true:

| Claim the crate makes today | Where | Made true by |
| --- | --- | --- |
| "These methods are part of the public API" | rustdoc renders `Terminal::resize`, `cursor_style`, `sync` and `placements` | `BUG-0059` -- their return types become nameable |
| "I support the kitty keyboard protocol" | `CSI ? u` answers with the pushed flags | `US-0105` -- the encoder honours what the answer promised |
| "Every unimplemented sequence is counted, never guessed at" | `FeedStats::unhandled_sequences` | `US-0106` -- three counted sequences become answered ones, and the count drops for a real reason |

And the one the crate does not make yet and should:

| Claim | Made by |
| --- | --- |
| "Here is what it costs, measured, with the command you run to reproduce it" | `US-0107` |

## Diagram

```text
                          crates/vt (the engine)
   +--------------------------------------------------------------+
   |                                                              |
   |  parser  --->  terminal::dispatch  --->  grid / snapshot     |
   |                      |                                        |
   |                      |  US-0106 adds three answers here:      |
   |                      |    CSI * y   DECRQCRA  (rect checksum) |
   |                      |    DCS $ q   DECRQSS   (status string) |
   |                      |    DCS + q   XTGETTCAP (terminfo)      |
   |                      |  + one bounded DCS payload buffer      |
   |                      |  + Config::allow_screen_readback       |
   |                      v                                        |
   |                  EventBatch  (VtEvent::Reply, as DA1 already) |
   |                                                              |
   |  input::encode_key  <--- US-0105: reads KeyboardFlags and     |
   |       ^                  modify_other_keys from ModeSnapshot, |
   |       |                  and a new KeyEvent carries the       |
   |       |                  event type, alternate keys and text  |
   |       |                                                      |
   |  lib.rs re-exports  <--- BUG-0059: + ResizeOutcome,           |
   |                          CursorStyle, SyncState, Placement    |
   +--------------------------------------------------------------+
            ^                                        |
            |                                        v
   scripts/vt-public-api.py                   crates/tools/vt-bench
   BUG-0059 adds a signature gate:            US-0107 adds --check,
   any type in a public signature whose       a committed baseline and
   own path is private is an error            a published tier-3 table
                                                     |
                                                     v
                                        crates/vt/README.md + guide ch14
```

Unchanged, and deliberately absent from the diagram: `crates/vt/Cargo.toml`'s `publish = false`,
`scripts/verify-dependency-graph.py`'s "no crate is published" assertion, and every workflow. The
crate is consumed by git before this intake and after it.

## UI Wireframe

`N/A -- no UI surface.` No OneTerm screen, panel, menu, dialog, setting or keybinding changes in
any packet. The one user-observable effect (`US-0105`) is in the bytes written to a pseudo-console,
not in anything drawn.

## Data Flow

The three flows this intake adds or changes, each end to end.

### 1. A key press under kitty flags (`US-0105`)

1. The embedder receives a platform key event and builds an `input::KeyEvent`: the `KeySpec` it
   already built, the `KeyMods`, and -- new -- the event kind (press, repeat, release), the shifted
   and base-layout code points if its platform knows them, and the text the key would produce.
2. The embedder calls `Terminal::encode_key_event`, which reads its own `ModeSnapshot`. That
   snapshot now carries `keyboard_flags` and `modify_other_keys` beside `app_cursor`.
3. The encoder picks one of three encodings in a fixed order: kitty `CSI u` if any flag applies to
   this key, `modifyOtherKeys` if its level applies, otherwise the legacy table that exists today.
   The order and the per-flag rules are
   [`low-level-design/kitty-keyboard.md`](low-level-design/kitty-keyboard.md).
4. Bytes come back. The engine writes nothing and remembers nothing: the function is pure, and the
   embedder owns the write to the transport, exactly as today.

The flags themselves arrive the other way, and that path is unchanged: the program writes
`CSI > Ps u`, the parser dispatches it, `FlagStack::push` records it, and the next snapshot carries
it. `US-0105` adds no new state to the terminal at all -- it reads state that has been live and
correct since `IN-0029`.

### 2. A conformance query (`US-0106`)

1. The program writes `CSI Pid ; Pp ; Pt ; Pl ; Pb ; Pr * y`, `DCS $ q <setting> ST` or
   `DCS + q <hex names> ST`.
2. For the CSI form, dispatch reads `Config::allow_screen_readback`. False -> `unhandled()`, no
   reply, and the counter rises: the terminal says nothing rather than lying either way.
3. For the two DCS forms, `dcs_hook` now recognises the intermediate and opens a **payload buffer**
   instead of a Sixel decoder; `dcs_put` appends to it under the existing `DCS_MAX_BYTES` ceiling;
   `dcs_unhook` parses the complete payload and answers, or discards it on abort.
4. The answer is pushed as `VtEvent::Reply` into the `EventBatch` the caller passed to `feed`, the
   same mechanism `DA1` and `DECRQM` have always used. Nothing is written by the engine.

### 3. A performance number (`US-0107`)

1. A maintainer or an evaluator runs one command:
   `cargo run -p oneterm-tools --release --bin vt-bench -- tier3 --json`.
2. `vt-bench` replays the committed fixtures through a real `Terminal` and prints MiB/s per
   fixture.
3. `--check crates/tools/bench-baseline.json` compares against a committed baseline and exits
   non-zero only outside a deliberately wide band. It is a trip-wire for a tenfold regression, not
   a measurement of the machine.
4. The table, the command and the machine it was measured on are printed in the README and in guide
   chapter 14, so an evaluator reads a number and the way to reproduce it in the same place.

## Detail Design

- [x] Detail design: **required (high-risk)**
- Reason: the lane is high-risk on two independent triggers (an external public contract, and a new
  screen-readback capability at a trust boundary), so `docs/HARNESS.md` blocks `story create` until
  at least one detail design exists. Four exist, one per concern, and each is the owning document
  for exactly one packet:

| File | Concern | Packet |
| --- | --- | --- |
| [`low-level-design/api-surface.md`](low-level-design/api-surface.md) | the four re-exports, the semver arithmetic for every change in this intake, and the gate that catches the next unnameable type | `BUG-0059` |
| [`low-level-design/kitty-keyboard.md`](low-level-design/kitty-keyboard.md) | the encoder: the five flags, the byte tables, the interplay with `modifyOtherKeys` and DECCKM, and the ceilings | `US-0105` |
| [`low-level-design/conformance-queries.md`](low-level-design/conformance-queries.md) | `DECRQCRA` and its checksum variant, `DECRQSS`, `XTGETTCAP`, the DCS payload buffer and the `esctest` run plan | `US-0106` |
| [`low-level-design/performance-evidence.md`](low-level-design/performance-evidence.md) | what is measured, where the table is published, and the shape of the regression guard | `US-0107` |

## What this design deliberately does not do

Recorded so a reviewer can see these were considered and declined, rather than missed.

- **Nothing about publishing.** The owner's ruling stands and this intake does not weaken, prepare
  for, or argue against it. `publish = false` and the workspace-wide "no crate is published"
  assertion are untouched, and the packets' acceptance criteria say so where a reader might wonder.
- **No kitty graphics protocol and no `OSC 1337`.** The evaluation's own honourable mention.
  Neither is a false claim, and `OscRoutes` already lets an embedder decode `1337`. A feature
  request, not a gap.
- **No `criterion`.** `vt-bench` already exists, already has five tiers and already emits JSON.
  Adding a benchmark framework to publish numbers a binary already prints buys a dev-dependency and
  a second way to run the same thing.
- **No new named keys for the keypad, the media keys or the modifier keys themselves.** The kitty
  specification assigns code points in the 57 3xx and 57 4xx range to about sixty keys that
  `input::NamedKey` cannot name. `NamedKey` is `#[non_exhaustive]`, so adding them later is a patch
  bump; adding sixty variants now to satisfy a flag combination no embedder in this repository
  sets would be speculative.
- **No runtime checksum-variant selector (`CSI Ps * x`).** xterm has one because it needed to
  reconcile its own history with the VT520's. This engine has no history to reconcile: it picks one
  variant, pins it with a doctest, and documents it.
