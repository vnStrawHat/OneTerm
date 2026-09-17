# DEC-0018 App shortcuts leave the single-Ctrl keys to the terminal

Date: 2026-09-17

## Status

Accepted 2026-09-17 by the owner, with the explicit ruling that `ctrl-w` (Close Panel) and
`ctrl-t` (New Terminal Tab) stay as they are. `US-0123` (`IN-0042`) implements it.

Where it landed: the four defaults in
`crates/settings-ui/src/key_bindings/key_bindings_actions.rs`, the rule itself as
`key_bindings_actions::tests::the_only_bare_ctrl_defaults_are_the_ones_dec_0018_accepted`,
and the collision rule as `key_bindings/state.rs`'s `collisions_with_overrides`
called from `resolve_default_collisions` at the head of `apply_key_bindings`.

## Context

OneTerm is a terminal. A keystroke the user presses with a remote program in the foreground
belongs to that program unless the application has a very good reason to take it, and in a
terminal the single-Ctrl range is where the control characters live.

The UX walkthrough of `main @2f12628a` recorded this as `F31`
(`docs/spec-intakes/IN-0042-ux-polish-round-1/research/ux-walkthrough-2026-09-16.md`). The
current app-level defaults, in `crates/settings-ui/src/key_bindings/key_bindings_actions.rs`,
are:

| Action | id | Default | Line | What the terminal loses |
| --- | --- | --- | --- | --- |
| Zoom Active Panel | `toggle_zoom` | `shift-escape` | 53 | nothing |
| Close Panel | `close_panel` | `ctrl-w` | 62 | `^W` — delete previous word (readline, and the `werase` tty setting) |
| New Terminal Tab | `new_terminal_tab` | `ctrl-t` | 71 | `^T` — transpose characters (readline); `SIGINFO` on BSD |
| New SSH Session | `new_ssh_session` | `ctrl-s` | 80 | `^S` — XOFF, stop output |
| Toggle Gutter | `toggle_gutter` | `ctrl-g` | 89 | `^G` — BEL, and "abort" in readline and Emacs |
| About OneTerm | `about` | `ctrl-space` | 98 | `^@` / NUL — set-mark in Emacs and readline; the IME toggle on several input methods |
| Quit | `quit` | `ctrl-q` | 107 | `^Q` — XON, resume output |
| Open Settings | `open_settings` | `ctrl-,` | 116 | nothing in practice |

`^S` and `^Q` are the sharpest: a user who presses `Ctrl-S` inside `less`, `vim` or a
serial console expects flow control, and instead gets a dialog. `Ctrl-G` is the abort key in
readline and Emacs. `Ctrl-Space` is set-mark, and on a machine with a Vietnamese, Chinese,
Japanese or Korean input method it is frequently the IME toggle — so the About dialog can
open from a keystroke the user pressed to change their keyboard layout. Spending a prime key
on **About**, a dialog opened perhaps twice in a product's life, is the worst trade in the
table regardless of the conflict.

The walkthrough could not exercise any of this: it drove the application with posted `WM_*`
messages on a locked desktop, which do not set real modifier state, so no Ctrl chord was ever
delivered. `F31` is a code read of the table above, and the conflicts are the documented
meanings of those control characters, not measured captures. That is enough to decide the
defaults — the table says what OneTerm binds, and the control characters are not in dispute —
but it is why this record exists instead of a one-line commit.

Two constraints shape the answer:

1. **Defaults are not overrides.** `crates/settings-ui/src/key_bindings/state.rs`'s
   `overrides_from_effective` writes only entries that *differ* from the built-in default
   into `ui_config.json`, and `init_state` resolves each action as `override.or(default)`.
   A user who never rebound an action, and a user who deliberately chose the key that *was*
   the default, both have no entry and are indistinguishable on disk.
2. **`apply_key_bindings` does not detect conflicts.** `conflicting_action` runs only in the
   rebind capture UI. At apply time every effective binding is registered, so two actions
   holding one keystroke both get bound.

## Decision

**App-level default key bindings do not use a bare `Ctrl` plus a letter, digit or Space that
carries a terminal control character.** The application reaches for `Ctrl+Shift`, a function
key, or a punctuation key the terminal does not use.

### The defaults `US-0123` ships

| Action | Old default | New default |
| --- | --- | --- |
| New SSH Session | `ctrl-s` | `ctrl-shift-n` |
| Quit | `ctrl-q` | `ctrl-shift-q` |
| About OneTerm | `ctrl-space` | `f1` |
| Toggle Gutter | `ctrl-g` | unbound |

`ctrl-shift-n` rather than `P18`'s alternative `ctrl-shift-s`, because `ctrl-shift-s`
conventionally means "Save As" and this action creates rather than saves, and because it
keeps `New Terminal Tab` and `New SSH Session` on the same letterless mnemonic family
(`Ctrl-T` / `Ctrl-Shift-N`). `f1` rather than `ctrl-shift-/`, because `F1` is the platform's
help key, is unambiguous to type, and is not a chord a terminal program receives by accident.
Toggle Gutter ships **unbound** rather than moved: it is a view toggle with no discoverable
entry point outside this table, so a default keystroke buys nothing; the row stays in
`BINDABLE_ACTIONS` with `default: None`, so any user who wants it can bind it from the Key
Bindings page, exactly as `Clear` and `Duplicate Session` already work.

`ctrl-w` (Close Panel) and `ctrl-t` (New Terminal Tab) are **kept as they are**, and this is
a deliberate exception to the rule above. Both collide with a readline binding rather than a
flow-control or abort character, both are the near-universal cross-application meaning of
that chord ("close tab", "new tab"), and a user who wants `^W` to reach the shell can unbind
Close Panel from the Key Bindings page. Future work that revisits them should treat them as
this decision's known soft spot, not as an oversight.

### The migration rule

**Rewrite only users still on the old default — which is what changing the default table
already does, so `US-0123` ships no migration code for the common case.**

- No entry in `ui_config.json` for that action: the user resolves through `default`, so the
  table edit moves them. This covers both the user who never rebound and the user who chose
  the old default explicitly, because the sparse map does not distinguish them.
- An entry that differs from the old default: the user's own choice, untouched.
- An entry equal to the **new** default for a *different* action — the one case that needs
  code. `apply_key_bindings` would otherwise bind that keystroke twice. The rule: **the
  surviving user override wins, and the action whose new default collided with it is left
  unbound and logged once at `warn` with both action ids.** The user's explicit choice is
  never silently taken away, and the Key Bindings page shows the displaced action as
  unbound so it can be rebound in one click.

The rule is expressible as a pure function over `(BINDABLE_ACTIONS, overrides)` and is
`US-0123`'s focused test: the three cases above, plus a table-wide assertion that no two
shipped defaults hold the same keystroke in the same key context.

### What future work inherits

Any new action added to `BINDABLE_ACTIONS` obeys the rule when it declares a default.
Adding a default that takes a single-Ctrl control character needs this record amended, not
a comment in the table.

The rule reaches the whole registry, not the App Menu group: every row in
`BINDABLE_ACTIONS` has `context: None`, so its binding is global and every default is in
the rule's scope. The **complete** set of bare-`Ctrl` defaults this record accepts is
therefore four: `ctrl-w` (Close Panel), `ctrl-t` (New Terminal Tab), `ctrl-,` (Open
Settings) and `ctrl-f` (Find). **`find` stays on `ctrl-f` by the same exception `ctrl-w`
and `ctrl-t` are kept by** — it is the near-universal cross-application meaning of that
chord, it collides with a readline motion (`forward-char`, and page-forward in `less`)
rather than with flow control or abort, and a user who wants `^F` in a shell can unbind
Find from the Key Bindings page. `US-0123` did not move it, because moving it would have
been a fifth default change this record did not authorise.
`the_only_bare_ctrl_defaults_are_the_ones_dec_0018_accepted` asserts that set exactly, so a
fifth cannot appear without amending this record.

## Alternatives

- [x] Selected: move the four defaults off single-Ctrl keys, ship the table edit as the
  migration, and resolve the one collision case in favour of the user's override.
- [ ] **Leave the defaults alone and document the conflict.** Rejected: the conflict is with
  flow control and abort, which are not obscure. `Ctrl-S` freezing a terminal is a decades-old
  source of "my terminal is hung" reports; having it instead open a dialog does not fix that,
  it adds a second surprise.
- [ ] **Keep the keys but suppress them while a terminal has focus.** Rejected: it makes one
  keystroke mean two things depending on focus, which is worse to explain than either
  behaviour alone, and it leaves the Key Bindings page showing a binding that does not fire
  where the user is looking. It also needs a key context OneTerm does not currently set for
  these actions — every row in the App Menu group has `context: None`.
- [ ] **Migrate by writing the new keystroke into every user's `ui_config.json`.** Rejected:
  it converts a default into an override for every user, so the next default change would no
  longer reach any of them, and `overrides_from_effective` would have to start recording
  entries it currently omits. The sparse map is the feature, not an obstacle.
- [ ] **Detect the collision by refusing to apply the new default and keeping the old one.**
  Rejected: it keeps the exact binding this decision exists to remove, for the one user most
  likely to be an advanced user. Leaving the displaced action unbound and visible is honest
  and one click from fixed.
- [ ] **Move `ctrl-w` and `ctrl-t` too.** Not selected now; see the exception above. Recorded
  as the known soft spot rather than silently omitted.

## Consequences

- [ ] Benefit to confirm: `Ctrl-S`, `Ctrl-Q`, `Ctrl-G` and `Ctrl-Space` reach the foreground
  program. This cannot be proven by the walkthrough's capture method — posted `WM_*` messages
  do not set modifier state — so `US-0123` proves the table and the migration rule by focused
  test, and the end-to-end half is an interactive check by a human at a real keyboard, or it
  is recorded as unverified. It must not be claimed as working on the strength of the table
  alone.
- [ ] Tradeoff: every existing user who used `Ctrl-S` for New SSH Session or `Ctrl-Q` to quit
  loses that keystroke at the next launch, with no prompt. This is the intended effect of "only
  rewrite users still on the old default", and it is the reason this change needs the owner's
  acceptance rather than shipping as polish. The release notes for the version carrying
  `US-0123` must list the four moved defaults.
- [ ] Tradeoff: Toggle Gutter ships with no keystroke at all, so a user who used `Ctrl-G` for
  it must bind it again from the Key Bindings page. The alternative — `ctrl-shift-g` — was
  not taken because it spends a chord on a toggle nothing else in the application surfaces.
- [ ] Follow-up: `ctrl-w`, `ctrl-t` and `ctrl-f` remain on readline keys by explicit
  exception. If a user reports losing `^W`, `^T` or `^F` in a shell, that is this decision's
  soft spot surfacing, and it is an amendment here plus a packet — not a new argument.
- [ ] Tradeoff, confirmed after acceptance rather than before it: `f1` is taken from the
  foreground program too. Every `BINDABLE_ACTIONS` row has `context: None`, and gpui
  dispatches a matched binding before any key-down listener
  (`reference/zed/crates/gpui/src/window.rs:4901-4923`; the `skip_bindings` escape at
  `:4886-4899` needs a `key_char`, which `F1` has none), so while OneTerm is focused `F1` no
  longer reaches the terminal view's key map (`crates/terminal-view/src/input/keys.rs:350`).
  A user loses F1 help in `mc`, `nano`, `htop`, `vim` and `less`. This is the same class of
  cost the record set out to remove from `^S`, `^Q`, `^G` and `^@`, on a key used by fewer
  programs; it is recorded here because it was established by an independent verification of
  `US-0123` **after** the owner accepted `f1`, and it is an amendment here plus a packet if
  the owner now wants About moved again.
- [ ] Follow-up: `apply_key_bindings` gains the collision rule, which is the first time it
  makes a decision rather than registering what it is given. Any future work that adds a
  second source of bindings must route through the same rule or this guarantee stops holding.
