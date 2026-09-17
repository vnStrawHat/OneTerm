# Work: The session form scrolls, folds advanced fields, and offers a short colour row

ID: US-0120
Intake: IN-0042
Created: 2026-09-17

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change
- Risk lane: normal
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md`

## Outcome

Save is always reachable. The session dialog's body scrolls when it outgrows the window, the
fields most users never touch are folded away behind an "Advanced" disclosure, and choosing a
session colour costs one click instead of a decision among 130 swatches.

## Findings and proposals covered

`P22` (effort M) — *"Make the session form survive short windows: scroll the `FormDialog` body
(the `ponytail:` note at `session_dialog.rs:424` already flags this) and collapse Jump host /
Port forwards / Agent forwarding into an "Advanced" disclosure."*

`P23` (effort S–M) — *"Replace the 130-swatch colour popup with the eight-swatch row it
already renders on top (plus "Custom…" for the full picker), and label the control."*

Addresses `F20` (medium) and `F21` (low), quoted from
`research/ux-walkthrough-2026-09-16.md`:

> | F20 | New/Edit SSH Session dialog | With Private Key selected the form is ~735 px of fields
> and the footer sits at y≈900 in a 1000 px window; the code already carries a `ponytail:` note
> that `FormDialog` does not scroll (`session_dialog.rs:424-426`), so a session with several
> port forwards pushes Save off-screen. | Reachability on short windows / laptops. | medium |
> 12 |

> | F21 | session colour | The colour control is an **unlabelled square** beside the Label
> field; clicking it opens a 130-swatch palette plus an HSLA tab plus a hex field — to choose
> the tint of an 8 px square. | Discoverability (nothing says it is a colour) + decision cost
> far above the payoff. | low | 11, 54 |

## Scope

- [ ] In scope:
  - `crates/state/src/form_dialog.rs` — the shared `FormDialog` body gains a scroll when it
    outgrows the available height. This is where the fix belongs (see Context).
  - `crates/session-ui/src/session_dialog.rs` — the "Advanced" disclosure over Jump host, Port
    forwards and Agent forwarding; the shortened colour row and its label; and removing the
    `ponytail:` note at line 424 once its ceiling is lifted.
  - Every other `FormDialog` caller, checked for the scroll's side effects.
- [ ] Out of scope:
  - The persisted `ssh_session.json` shape. Folding fields away changes where they are drawn,
    not what is saved, and the colour row keeps writing the same hex the full picker wrote.
  - The colour's *default* and its resolution, settled by `US-0110` — `session_color_hex` and
    the `#56B6C2` fallback stay exactly as they are, and both surfaces must keep agreeing.
  - The Logging block's clipped descenders (`BUG-0069`), although both packets touch the
    bottom of the same dialog — see Context for the sequencing.
  - The quick-connect dialog's failure behaviour (`US-0118`).
  - Jump host and port-forward *semantics* (`IN-0023`, `DEC-0010`, `DEC-0011`).

## Acceptance

- [ ] In a 1000 px-tall window with Private Key selected and three port forwards configured,
      Cancel and Save are both visible and clickable without resizing the window.
- [ ] The dialog body scrolls when, and only when, it does not fit. A dialog that fits shows
      **no** scrollbar — the application theme keeps scrollbars permanently visible, so an
      always-present scroll container would put a bar on every small dialog in the application.
- [ ] The footer never scrolls out of view.
- [ ] Jump host, Port forwards and Agent forwarding are behind a disclosure that is collapsed
      by default for a new session, and **expanded** when the session being edited has any of
      them set — a hidden configured field is worse than a long form.
- [ ] The colour control carries a visible label saying it is the session colour.
- [ ] The colour row offers a short set of swatches plus a route to the full picker; choosing a
      swatch writes the same kind of value the full picker wrote, and the tree and the `+` menu
      draw it identically (`US-0110`'s shared resolver is untouched).
- [ ] Every other `FormDialog` in the application is checked at the default window size and
      shows no new scrollbar and no changed footer position. The packet lists the dialogs it
      checked.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/ssh-client-connect.md` §4 — the session dialog's fields and layout. **Update
  required:** the Advanced disclosure changes which fields are visible by default, and the
  colour row changes how the colour is chosen.
- `docs/gui-layout.md` — the `US-0110` paragraph in §Panel registration describes the colour
  square on both surfaces and the shared resolver. `P23` names it as an owning doc.
  **Update required** only if the paragraph describes the *picker*; it primarily describes the
  square, which does not change. Confirm and record.
- `crates/state/src/form_dialog.rs` module doc — describes `FormDialog` as a titled dialog with
  a content builder and a footer. **Update required:** the body's scroll behaviour, because
  every feature crate builds on this and needs to know what it now does.
- `docs/decisions/DEC-0010-jump-hosts-reference-saved-sessions.md` and
  `DEC-0011-forwarding-defaults-loopback-and-opt-in.md` — read to confirm that folding these
  fields into a disclosure does not change their defaults or their meaning. **No change.**
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

Update required: `docs/ssh-client-connect.md` §4, the `form_dialog.rs` module doc, and
`docs/gui-layout.md` if its `US-0110` paragraph describes the picker.

Reason: the packet changes which fields a documented dialog shows by default, and it changes
the behaviour of a shared scaffold that four feature crates build on.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- **The scroll belongs in `FormDialog`, not in `session_dialog.rs`.** The `ponytail:` note is
  in the session dialog, but the ceiling it names is the scaffold's: *"`FormDialog` does not
  scroll"*. Every dialog built on it has the same failure on a short window; one guard in the
  shared builder is a smaller change than a guard in each caller, and patching only the dialog
  the walkthrough happened to open would leave the siblings broken. This is the note coming
  due, and the note is removed when it does.
- **The scrollbar is the hazard.** The application's theme uses `ScrollbarMode::Always` — the
  `+` menu's code comments say so explicitly and go to some trouble to set `scrollable` only
  when the rows really cannot fit (`crates/terminal-view/src/panel/terminal_panel.rs:718-738`).
  The same trap applies here: a body that is always scrollable is a bar on every dialog. The
  `+` menu's approach — decide `scrollable` from an estimate against a cap — is the in-repo
  precedent, and it comes with its own `ponytail:` note about estimating row height. A dialog
  body may be able to do better, because its content height can be measured after layout where
  a popup's cannot. Check before copying the estimate.
- The Advanced disclosure is `session_dialog.rs` only. The "expanded when configured" rule is
  what keeps it from hiding a jump host the user already set.
- The colour row: `F21`'s own observation is that the dialog *already renders* an eight-swatch
  row on top of the full picker. So the short row exists; the work is to make it the default
  surface and put the full picker behind "Custom…". That is the lazy path and it is `P23`'s.
- `US-0110` settled that one resolver (`session_color_hex`) decides a session's colour for both
  the tree and the `+` menu, with `#56B6C2` as the default. This packet must not touch it. Any
  swatch it offers must write a value `Colorize::parse_hex` accepts, or the resolver will fall
  back and the user's choice will vanish.
- Sequencing with `BUG-0069`: both touch the bottom edge of this dialog. If the scroll lands
  first, re-capture the Logging block — the clip may be gone. If `BUG-0069` lands first,
  re-check it here.
- `research/before/12-session-dialog-privatekey.png` (the tall form, footer at y≈900),
  `11-session-property-dialog.png` (the unlabelled colour square) and `54-session-color-picker.png`
  (the 130-swatch popup) are the before pictures.

## Plan

- [ ] Read how the `+` menu decides `scrollable`, then decide whether the dialog can measure
      instead of estimate.
- [ ] Implement the `FormDialog` scroll; walk **every** dialog built on it at the default size.
- [ ] Advanced disclosure, with the expanded-when-configured rule.
- [ ] Colour row: promote the existing short row, "Custom…" behind it, label the control.
- [ ] Remove the `ponytail:` note; update the three docs.
- [ ] Re-capture the scenes at a 1000 px-tall window.

## Decisions

None expected. The scroll is a ceiling being lifted, not a new rule; the Advanced split is
local to one dialog.

## Verification Plan

1. **Focused:** `cargo test -p oneterm-state` over the scroll predicate — given a content
   height and an available height, does the body scroll? Pure arithmetic, and the one place a
   test can bite. Plus `cargo test -p oneterm-session-ui` over the disclosure's
   expanded-when-configured rule and the colour value the short row writes (it must be a hex
   `session_color_hex` accepts — reuse `US-0110`'s existing resolver tests rather than writing
   a parallel set).
2. **Unit:** `cargo test -p oneterm-state`, `cargo test -p oneterm-session-ui`.
3. **Integration:** `cargo test --workspace` — `FormDialog` is shared, so every crate that
   builds one must still compile and pass.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):**
   - `12-session-dialog-privatekey.png` — at a 1000 px-tall window, with three port forwards
     configured, showing Save reachable. This is the acceptance frame.
   - `11-session-property-dialog.png` — the labelled colour control and the collapsed Advanced
     section.
   - `54-session-color-picker.png` — the short colour row, with the full picker behind
     "Custom…" captured as `54b`.
   - `12b-session-dialog-agent.png` — agent forwarding now inside Advanced, and expanded
     because it is set.
   Plus a regression sweep the walkthrough did not do: open each of the application's other
   `FormDialog` dialogs at the default window size and capture one frame each, to prove no new
   scrollbar appeared. List them in Evidence.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **A scrollbar on every dialog.** The single largest risk, because the theme always shows
  bars. The regression sweep exists for this and is not optional.
- **Hiding a configured field.** A user with a jump host set who opens the dialog and sees no
  jump host will conclude it was lost. The expanded-when-configured rule is the mitigation and
  is in the acceptance.
- **Breaking `US-0110`'s colour agreement.** A swatch that writes a value the shared resolver
  rejects turns every chosen colour into the default teal — silently, and on two surfaces.
  Reuse the existing resolver tests.
- **Touching four crates' dialogs at once.** `FormDialog` is shared scaffolding. The change is
  small but its blast radius is every form in the application; the sweep is the only way to
  know.
- **Nested scrolling.** If a field inside the body scrolls (a list of port forwards, a
  combobox popup), a scrolling body around it can capture the wheel. Walk the port-forward list
  with the wheel specifically.

## Evidence and Gaps

### The scroll, and the three shapes that did not work

The cap is `form_body_max_height(window_height)` = window height minus 260 px of chrome,
floored at 240 px, in `crates/state/src/form_dialog.rs`. The form goes into a box carrying
that `max_h` and `overflow_y_scroll`, with the kit's `Scrollbar` on a `relative()` wrapper
outside the scrolling box. The dialog's footer is a sibling of the whole content block in the
kit's own layout, so capping the body is exactly what keeps Cancel and Save on screen.

Getting there took three GUI walks, and the reasons are worth keeping because they are all
about **definite heights**:

1. `max_h` on the element handed to the kit's `overflow_y_scrollbar`. `Scrollable::render`
   *copies* the size styles onto its wrapper (`root_style_from`) and leaves them on the
   content, so the content was capped too, measured the same as its container, and produced
   neither scrolling nor a bar. The body just clipped.
2. `v_flex().max_h(cap)` with the scroll area as `flex_1().min_h_0()` inside it. A zero
   flex-basis in an auto-height column resolves to a zero-height body: the dialog rendered as a
   title and a footer with nothing between them.
3. The kit's `Scrollable` at all: its wrapper is `size_full()`, and `height: 100%` of the
   dialog box — which has no height of its own — is indefinite.

What works is the plain one: the cap on the scroll container itself, so its height is
`min(content, cap)` from a clamp, with no percentage and no flex basis anywhere. The scrollbar
sits outside the scrolling box so it does not scroll away with the form, and it draws nothing
while `scroll_area_size <= container_size` (`gpui-base/src/scrollbar.rs:1293`) — which is the
answer to the packet's largest risk: **the theme's always-visible mode makes a bar visible, not
present**. A dialog that fits has no bar.

### The regression sweep

| Dialog | Result |
|---|---|
| New / Edit SSH Session, Advanced collapsed | No scrollbar; footer in place. `US-0120-11`. |
| New / Edit SSH Session, Advanced expanded | Bar appears, body capped, footer outside it. `US-0120-11b`. |
| ...scrolled with the wheel | Logging row fully reached, Save still outside. `US-0120-12`. |
| SSH Quick Connect | No scrollbar, unchanged. `US-0120-sweep-quick-connect`. |
| Rename Group | No scrollbar, unchanged. `US-0120-sweep-rename-group`. |

### Advanced

`advanced_is_configured(jump_host, port_forwards, agent_forwarding)` decides the initial state
and is unit-tested. Collapsed, the dialog is about 660 px instead of about 820 px, which is why
the collapsed case no longer scrolls at all in a 1000 px window.

### The colour row

Eight swatches — `US-0110`'s `#56B6C2` default first so a new session's colour is one of them,
then theme colours, no hard-coded values — a "Custom…" square that opens the full picker, and a
visible **Colour** label. The swatches write through the very same `ColorPickerState` the
picker writes, and `submit` still stores `color_state.read(cx).value().map(|h| h.to_hex())`.
**There is no new value format and no second code path**, which is why this packet adds no
colour test: `US-0110`'s resolver tests already cover the only value that is produced. The
captured picker shows `#56B6C2` in its hex field with the first swatch ringed, from the same
state.

### Commands

- `cargo test -p oneterm-state --lib form_dialog` — 1 passed
  (`the_body_cap_follows_the_window_and_has_a_floor`: follows the window, has a floor, and is
  monotonic).
- `cargo test -p oneterm-session-ui` — 69 passed, including
  `advanced_opens_only_when_the_session_already_uses_it`.
- `cargo clippy -p oneterm-state -p oneterm-session-ui --all-targets -- -D warnings` — clean.
- `cargo test --workspace` — passed.
- `pwsh scripts/ci-local.ps1` — "ci-local: all checks passed".

### Evidence frames

- `evidence/US-0120-11-session-property-dialog.png` — the labelled colour row, the collapsed
  Advanced, no scrollbar.
- `evidence/US-0120-54c-colour-row-4x.png` — the row at 4x, the default swatch ringed.
- `evidence/US-0120-11b-advanced-expanded.png` — Advanced open, the body capped and the bar
  present, footer below it.
- `evidence/US-0120-12-session-dialog-scrolled.png` — **the acceptance frame**: scrolled to the
  bottom of the form with Cancel and Save on screen and no window resize.
- `evidence/US-0120-54-session-color-row.png`, `US-0120-54b-session-color-picker.png` — the
  short row, and the full 130-swatch picker behind "Custom…" showing the same hex.
- `evidence/US-0120-sweep-rename-group.png`, `US-0120-sweep-quick-connect.png` — the sweep.

### Documentation

- `crates/state/src/form_dialog.rs` module doc — **updated**: what the body does now, and why
  a dialog that fits shows no bar. Every feature crate builds on this.
- `docs/ssh-client-connect.md` 6.6 — **new**: the session dialog's field order, the Advanced
  rule, the colour row, and where the scroll lives.
- `docs/gui-layout.md` — its `US-0110` paragraph describes the 8 px **square** and the shared
  resolver, not the picker. Both are unchanged. **No change**, as the packet allowed.
- `DEC-0010` and `DEC-0011` — read; folding jump hosts and forwards into a disclosure changes
  neither their defaults nor their meaning. **No change.**
- The `ponytail:` note at `session_dialog.rs:424` is gone: its ceiling is lifted.

### Gaps

- **The four SFTP `FormDialog` dialogs were not captured.** `sftp-ui`'s rename, new-folder and
  edit dialogs need a live SFTP connection to reach, and this walk had none. They are smaller
  forms than the ones swept and go through the identical code path, but the frames are missing
  and the sweep is therefore partial. `US-0126`'s full re-capture will cover them.
- **The port-forward list was not walked with the wheel.** The packet flagged nested scrolling
  as a risk; the port-forward rows are a plain column with no scroll of their own
  (`forward_rows.rs`), so there is no inner scroll area to capture the wheel — but that is read
  from the code, not from a frame.
- **The cap is a constant, not a measurement.** 260 px of chrome and a 240 px floor are tuned
  to the current dialog shape rather than derived from the laid-out title and footer, which are
  not knowable while the content closure runs. The unit test pins the shape of the function;
  a much taller footer would want the number revisited.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
