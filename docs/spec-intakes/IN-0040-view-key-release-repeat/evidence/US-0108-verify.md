# Independent verification: US-0108 (IN-0040)

Packet: [`US-0108`](../US-0108-view-key-release-repeat.md)
Intake: [`IN-0040`](../IN-0040.md), lane `high_risk`
Branch verified: `feat/view-key-release-repeat` at `4c57f542` (base `main` at `980bf5da`)
Date: 2026-09-16
Verifier: adversarial, independent worktree. Nothing was committed to the packet's branch.

## Verdict

**PASS-WITH-NOTES.**

The central claim holds and is now proved in the strong form the packet could not run: with no
kitty keyboard flag pushed, this branch writes byte-for-byte what `main` writes, over **12528
compared cases** (8128 of which produced bytes). The kind mapping, the held set, the swallowed-press
guard, the stray key-up, the per-view isolation and the closed release fan-out all behave as
claimed, and the `:2` / `:3` integration test is genuinely non-vacuous.

It is not a clean PASS because three of the packet's own stated failure modes are still open:

- a **stuck key** is reachable on Windows through a key-name change the design assumed away (F1);
- **Ctrl+C on a broadcast channel** now sends escape bytes to peers that negotiated nothing, which
  is the same hazard the packet forbade release fan-out to avoid (F2);
- the **blur drain**, the only defence against a stranded key, is proved by no test that exercises
  the subscription, and cannot be, in this harness (F3). With the manual Windows walk NOT RUN,
  nothing in the repository shows a release is ever sent on focus loss.

None of these touches a program that negotiated nothing: the flagless byte stream is unchanged, and
that is measured rather than argued. The packet also remains correctly reported as **unverified on
the platform that ships**.

## Findings

### F1. A stuck key: the held set is keyed on a name Windows changes under Shift

**Severity: medium-high** (the packet's first-named failure mode; scope limited to programs that
pushed `REPORT_EVENT_TYPES`).

`crates/terminal-view/src/terminal_view/input.rs:130` inserts `e.keystroke.key`, and
`input.rs:152` removes by the same name on the key-up. The detail design justifies the choice:

> Keyed by `Keystroke::key`, the chord name ... it is stable under a modifier changing between the
> two (holding `a`, then pressing and releasing `Shift`, then releasing `a` still produces a key-up
> whose `key` is `a`).
> -- `low-level-design/input-events.md`, "The held set"

That is true for letters and false for digits and OEM punctuation. The Windows backend resolves the
key name through `get_keystroke_key`
(`gpui-pre-windows-0.3.3/src/keyboard.rs:148`), which, when `modifiers.shift` is set and the virtual
key is in `need_to_convert_to_shifted_key` (`keyboard.rs:175`, covering `VK_0` .. `VK_9` and the
whole `VK_OEM_*` set), returns the **shifted** glyph and clears `shift` from the modifiers.

So `Shift+1` arrives as `key: "!"` and is inserted as `"!"`. If the user lifts Shift before the
digit, the key-up is resolved without the shift branch and arrives as `key: "1"`. `held_keys.remove`
misses, no `:3` is written, and `"!"` stays in the set until the next blur. The program is left
believing `!` is held.

Reproduced at two levels in this worktree:

- `crates/terminal-view/src/input/us0108_verify_tests.rs`,
  `a_digit_key_changes_name_when_shift_is_released_first` -- both names map and both would encode.
- `crates/terminal-view/src/terminal_view/view_tests.rs`,
  `verify_a_shifted_digit_release_is_lost` -- press `"!"`, key-up `"1"`, one write (`!`) and no
  release, with `held_keys` left holding one entry.

Not a regression against `main` (which sent no releases at all), and invisible without
`REPORT_EVENT_TYPES`. The cheap fix is available and small: the encoder already collapses the two
names to the same key, because `unshifted("!")` and `unshifted("1")` are both `49`
(`crates/vt/src/input/kitty.rs:142`, the PC-101 table). Keying the held set on the `KeySpec` that
`map_key` produces -- or on the code point the encoder derives from it -- rather than on the raw
`Keystroke::key` makes the press and the release pair by construction. That is a change to this
packet's own instrument, so it belongs to this packet rather than to a follow-up.

### F2. Ctrl+C under `REPORT_ALL_KEYS_AS_ESC` fans escape bytes at broadcast peers

**Severity: medium** (a live behaviour regression for a user broadcasting to a channel).

The packet closed release fan-out with an explicit rule, stated twice:

> a peer whose program negotiated nothing must not receive a `:3` sequence, which is a byte form it
> has never seen.

The Ctrl+C row it *did* change reopens exactly that hole. With `all_keys_as_esc` true the row falls
through to `KeyAction::Send` (`crates/terminal-view/src/input/keys.rs:180`), and the `Send` arm fans
the encoded bytes to every channel peer (`input.rs:130-133`). On `main` the same keystroke took
`KeyAction::Interrupt` and fanned `BroadcastInput::Interrupt` (`input.rs:114-118`), so each peer
received a signal.

Measured, not read: `view_tests.rs`,
`verify_ctrl_c_under_all_keys_as_esc_fans_escape_bytes_at_peers`. With only the **origin** pane's
program pushing `CSI > 8 u`, a broadcast Ctrl+C writes `\x1b[99;5u` to the origin *and* to the peer,
where before it wrote `\x03` to both. A user who broadcasts Ctrl+C to four panes to stop four
runaway programs now stops one of them and prints `^[[99;5u` into the other three.

The consistent answer is the one the packet already chose for releases: keep the fan-out on the
`Interrupt` path (or fan the event, not the bytes) whenever the origin's encoding is flag-dependent.

### F3. The blur drain is unproven, and no view test can prove it

**Severity: medium** (it is the recovery path for F1 and for every stranded key).

The packet's acceptance criterion "Blur cannot strand a held key" is satisfied by
`blur_drains_every_held_key`, which calls `view.release_held_keys(cx)` directly. That proves the
helper, not the wiring at `crates/terminal-view/src/terminal_view/view.rs:259-262`.

The wiring cannot be reached from a view test. GPUI raises focus events only inside a draw, and
builds the event's `previous_focus_path` only `if previous_window_active`
(`gpui-pre-0.3.3/src/window.rs:3107-3145`), while `Context::on_blur` fires only when
`previous_focus_path.last()` is the handle (`gpui-pre-0.3.3/src/app/context.rs:596-616`). A test
window's `is_active` is hard-coded `false` (`gpui-pre-0.3.3/src/platform/test/window.rs:309`), so
the previous path is always empty and the listener can never run.

Demonstrated rather than asserted: `view_tests.rs`,
`verify_the_blur_drain_is_unprovable_in_a_test_window` gives the terminal real window focus, presses
a key, calls `window.blur`, draws, and then shows that the handle is no longer focused, that
`view.focused` is still `true` (so the subscription did not run), that the key is still held, and
that nothing was written. The same limitation explains why no pre-existing test in the crate ever
asserts `focused == false`.

Consequence: the blur drain rests entirely on the manual Windows walk, and the walk is **NOT RUN**.
The packet is right to say so; this finding is that the gap is wider than the packet states, because
the drain is untested in *both* places rather than only on the platform.

### F4. `Ctrl+C` under `DISAMBIGUATE_ESC_CODES` alone still becomes a signal

**Severity: medium-low** (not a regression; an incomplete answer to the decision the packet reopened).

`classify_key` gates the Interrupt row on `ctx.all_keys_as_esc` only
(`crates/terminal-view/src/input/keys.rs:180`). The accepted encoder design puts a ctrl chord on the
kitty rung under the disambiguate flag too:

> `DISAMBIGUATE_ESC_CODES` | ... any chord with `alt`, `ctrl`, `ctrl+alt` or `shift+alt`
> -- `IN-0039/low-level-design/kitty-keyboard.md:147`

and `encode_key_event` agrees: `kitty_applies` returns true for `Ctrl+C` under that flag
(`crates/vt/src/input/kitty.rs:231`), producing `\x1b[99;5u`. So a program that pushed `CSI > 1 u`
is promised bytes by the engine's own contract and handed a `SIGINT` by the view.

Asserted both ways in `us0108_verify_tests.rs`, `ctrl_c_across_the_flag_states`: the view returns
`KeyAction::Interrupt`, and the encoder the view never reaches would have returned `\x1b[99;5u`.

Either gate is defensible; leaving the view's gate narrower than the encoder's rung is the one
answer that makes the two halves disagree. Whichever way the owner rules, it should be recorded in
open decision 1 rather than left implicit.

### F5. `KeyEvent::text` is supplied for Ctrl and Alt chords

**Severity: low** (latent; inert on Windows).

`crates/terminal-view/src/input/keys.rs:273` assigns `event.text = ks.key_char` for every press and
repeat, with no modifier test. The engine states the opposite rule for its own fallback and
deliberately does not apply it to embedder-supplied text:

> Reporting the bare payload for either would tell the program that `Ctrl+A` inserted an `a`.
> -- `crates/vt/src/input/kitty.rs:310-316`

On Windows this cannot fire: `process_key` drops a control-character `key_char`
(`gpui-pre-windows-0.3.3/src/events.rs:1606`), so `Ctrl+A` carries no text, and the engine's
`is_control` filter catches anything that slips through. On a backend that reports a printable
`key_char` for a Ctrl chord, the associated-text field would claim an insertion that never happened.
One `&& !(mods.control || mods.alt)` closes it.

### F6. The harness row contradicts the packet it belongs to

**Severity: low** (record hygiene; nothing was written to `harness.db`, correctly).

The `Harness Row` snippet in `US-0108-view-key-release-repeat.md` sets `status="planned"` (line 393),
`unit_proof=0` and `integration_proof=0` (lines 394-395) and `last_verified_result=None`, while the
packet's own `HARNESS:STATUS` block says **Implemented** and its `HARNESS:PROOF` block ticks Unit,
Integration and "Verify command passed". Whoever applies the row would insert a record that
disagrees with the document it came from. `intake_id` is left as `IN_0040 = None  # <- fill in`
(line 378); there is no numeric intake id anywhere in the file.

### F7. The detail design was not reconciled with the code it specifies

**Severity: low** (documentation).

The packet's Evidence discloses the deviations, which is the important half, but the owning design
still specifies the superseded shapes:

- `low-level-design/input-events.md`, "The held set" and the Interfaces table, specify
  `held_keys: HashSet<SharedString>`; the code ships `HashSet<String>`
  (`crates/terminal-view/src/terminal_view/view.rs:142`). The intake's own open decision 3 already
  says `HashSet<String>`, so the design is the only document left disagreeing.
- The Interfaces table lists `TerminalView::on_key_up` and `held_keys` but not
  `release_held_keys` (`input.rs:166`), which is a fourth item the design's "three write sites and
  no others" table does not name as a function.
- The design's `on_key_up` sketch is `send_key(&self.session, ...)`; the code is
  `send_key(&self.session.clone(), ...)` (`input.rs:161`) -- a needless handle clone kept for the
  borrow checker. Cosmetic, listed here only because the sketch is quoted as the specification.

### F8. `IN-0040` claims a drain on session close that does not exist

**Severity: low** (record accuracy).

`IN-0040.md`, "Data ownership and lifecycle": "is drained on blur and on session close". The code
drains on blur only (`view.rs:259-262`); nothing touches `held_keys` when a session ends. Harmless
-- a dead session has no program to strand -- but the record should say what the code does.

## What was verified and holds

| Claim | Result | Evidence |
| --- | --- | --- |
| `KeyAction::Send(KeyEvent)` with kind press / repeat / release | holds | `keys.rs:42,87,238`; packet test `the_kind_reaches_the_event_and_text_stops_at_a_release` |
| kind from `KeyDownEvent::is_held` and from the key-up path | holds | `input.rs:58-63,155` |
| `held_keys` is `HashSet<String>` of keys whose press was written | holds | `view.rs:142`, `input.rs:130` (insert only after `send_key` returned `Some`) |
| a release is sent only for a member | holds | `input.rs:152`; my `verify_two_held_keys_release_in_either_order` (stray key-up writes nothing) |
| the key-up path runs no `classify_key` and no `stop_propagation` | holds | `input.rs:144-164` |
| a release is never fanned to broadcast peers | holds | `input.rs:144-164` has no `fan_out`; packet test `a_release_is_never_fanned_out_to_channel_peers` |
| Ctrl+C is the encoded key only when `REPORT_ALL_KEYS_AS_ESC` is pushed | holds for that flag; see F4 for `DISAMBIGUATE` | `ctrl_c_across_the_flag_states` |
| IME ownership stays `alt_screen`-gated | holds, unchanged | `keys.rs:154-161` untouched by the diff |
| the `:2` / `:3` test cannot pass vacuously | holds | deleting the `window.draw` in a scratch copy fails at the guard (`view_tests.rs:540`, "the repaint must carry the pushed flags"), before any byte assertion; restored afterwards |
| byte identity against `main` | holds, **strong form** | 12528 cases, see below |
| four re-exports in `crates/terminal` (`KeyboardFlags` added) | holds, disclosed | `crates/terminal/src/lib.rs:50-67` |
| diff inside the reported numbers | holds | `git diff --numstat main...HEAD`: terminal-view +151 / -44, terminal +9 / -7, exactly as the packet's Evidence states, and both overruns are disclosed |
| no manual walk | holds, correctly reported NOT RUN | packet acceptance list and Gaps |
| a release cannot interleave a bracketed paste | holds | `paste_clipboard` -> `paste_text` writes inside one `session.update` (`crates/terminal-view/src/input/edit.rs:38-70`); nothing is async, so no key-up can land between the paste markers |
| a key-up at the search overlay or a non-focused view | no leak | the press never entered that view's set; `verify_a_key_up_at_another_view_writes_nothing` |

### Byte identity: the strong form, 12528 cases

The packet delivered the weak form and said so. This run delivers the strong one.
`crates/terminal-view/src/input/us0108_verify_tests.rs` carries `main`'s `map_key` and `named_key`
copied verbatim from `git show main:crates/terminal-view/src/input/keys.rs`, and compares, for every
case, `encode_key(spec, mods, modes)` on `main`'s mapping against `encode_key_event(event, modes)`
on this branch's:

- 65 (key name, `key_char`) pairs: every `named_key` row, `enter` and `tab` with layout text,
  letters, digits, the OEM punctuation, a shifted glyph, a non-ASCII character, the `space`
  translation, and two names with no encoding (`print`, `f25`);
- 8 modifier combinations (shift x ctrl x alt);
- DECCKM off and on;
- `modifyOtherKeys` levels 0, 1 and 2;
- kinds `Press` and `Repeat`;
- no kitty flag pushed, asserted inside the loop.

Result: **12528 compared cases, 0 divergences**; 8128 of them wrote bytes. Mappability, `KeySpec`,
`KeyMods` and the encoded bytes all match. The same loop asserts that a `Release` with no flag
encodes to `None` in every case, so a release adds no entry to the stream.

Also checked against the specification rather than against `main`:
`repeat_under_report_event_types_follows_the_specification` (a held letter under
`REPORT_EVENT_TYPES` alone keeps typing the letter; an arrow becomes `\x1b[1;1:2A`; `Enter`, `Tab`
and `Backspace` keep their legacy bytes and emit no release) and
`a_text_key_repeat_carries_text_under_all_keys_as_esc` (a repeat carries associated text, a release
does not).

## Commands run

All in an isolated worktree with `CARGO_BUILD_JOBS=2`.

| Command | Result |
| --- | --- |
| `git reset --hard feat/view-key-release-repeat` | `4c57f542` |
| `cargo fmt --all -- --check` | clean on the packet's files; only this verification's own new test file needed formatting |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean, with the verification tests compiled in |
| `cargo test -p oneterm-terminal-view` | 323 passed, 0 failed, 3 ignored (the packet's 312 plus this verification's 11) |
| `python scripts/check-doc-paths.py` | "Doc path check passed for 198 current paths in 11 documents." |
| `python scripts/check-english.py` | "English contributor-text check passed for 906 files." |
| `pwsh scripts/ci-local.ps1 -Full` | **`ci-local: all checks passed.`** -- all 26 steps including `cargo test --workspace`, `oneterm-vt --features vt-paranoid`, the `vt-public-api` checks and `cargo deny check licenses bans advisories` ("advisories ok, bans ok, licenses ok"), with the verification tests compiled in |

Verification tests added in this worktree only (not committed, not part of the packet):

- `crates/terminal-view/src/input/us0108_verify_tests.rs` (registered from
  `crates/terminal-view/src/input/mod.rs`)
- five `verify_*` tests appended to `crates/terminal-view/src/terminal_view/view_tests.rs`

## What could not be verified

1. **Everything platform.** `is_held` delivery, key-up delivery, and the blur drain are Windows
   message-pump behaviour. The manual walk was not run here either: the maintainer runs their coding
   agent inside OneTerm and a second `oneterm.exe` is not started unasked. F1 and F3 both end at
   this boundary, and F1's reproduction uses the key names the Windows backend is *shown* to
   produce rather than a real key press.
2. **The blur subscription itself**, for the harness reason in F3.
3. **`REPORT_ALTERNATE_KEYS` and the modifier keys.** Correctly out of scope and correctly recorded
   as ceilings; nothing here contradicts them.
4. **Whether a key typed into the focused search bar reaches the PTY on the alternate screen.** The
   search input is a child of the div carrying `on_key_down` / `on_key_up`
   (`crates/terminal-view/src/terminal_view/render.rs:290-330`), and `KeyContext::search_focused` is
   set only for `enter`. Whether the input consumes other keys first was not established; if it does
   not, the leak predates this packet and this packet does not widen it -- the release is gated by
   the same `held_keys` entry the press created. Flagged as a question, not a finding.
