# Low-Level Design: Tab status icon

Intake: IN-0049
HLD: [`../high-level-design.md`](../high-level-design.md)
Topic: view state, acknowledgement, the icon element (`oneterm-terminal-view`)
Date: 2026-10-15

## Concern

Show the terminal's OSC 7501 summary as an icon in its Terminal Tab's title, with the owner's
icon set, acknowledgement rule and split-tab exclusion, at no frame cost for tabs that report
nothing.

## Design

### View state (`TerminalView`)

- Field `program_status: Option<ProgramState>`.
- `SessionEvent::ProgramStatus(s)`: store `s` (an `Idle` summary is stored as `None`: it draws
  nothing), emit `TerminalViewEvent::StatusChanged` when the stored value changed.
- `SessionEvent::Exited(_)`: drop `Working` and `Blocked` (the spec's "MUST drop on exit";
  `Idle` is already `None`). `Done` and `Error` stay until acknowledged.
- `acknowledge_program_status()`: clears `Done` and `Error`, leaves the others. Called
  1. by `TerminalPanel::set_active(true)` for its terminal leaves (the tab was selected);
  2. by the view when the user's input is written to the PTY (key, paste, IME commit): typing
     in a terminal means its tab is already on screen.
- A later report replaces the state whatever was acknowledged (the router only emits on
  change, and a fresh `done` after `working` is a change).

### Panel (`TerminalPanel`, `tab_title.rs`)

- `tab_status(&self, cx) -> Option<ProgramState>`: `None` unless `self.tree.is_single()`; then
  the active terminal's `program_status`. Same shape as `shows_recording_dot`.
- The leaf subscription that already notifies the panel on `TitleChanged` also covers
  `StatusChanged`, so the strip repaints. A hidden tab still repaints its strip: the view's
  `in_hidden_tab` guard only skips the view's own frame.
- `render_tab_strip` puts the icon after the channel chips and before the label:

| State | Icon | Colour | Motion |
| --- | --- | --- | --- |
| `Working` | `IconName::LoaderCircle` | `theme.primary` (accent) | `with_animation`, 1 s repeat, `Icon::rotate(delta * 2π)` |
| `Blocked` | `IconName::Bell` | `theme.warning` | `with_animation`, 1.2 s repeat, opacity `0.35..1` on a sine pulse |
| `Done` | `IconName::CircleCheck` | `theme.success` | none |
| `Error` | `IconName::CircleX` | `theme.danger` | none |

  `xsmall` size, matching the recording dot's footprint. With `cx.reduce_motion()` the two
  animated icons render static. The animation element id is per panel
  (`("tab-status", panel_entity_id)`), so tabs do not share animation state.
- Accessibility: the icon carries a tooltip / accessible label `Working`, `Waiting for you`,
  `Done`, `Error`. No remote text.

## Interfaces

```rust
pub(crate) enum TerminalViewEvent { TitleChanged, StatusChanged }
impl TerminalView {
    pub(crate) fn program_status(&self) -> Option<ProgramState>;
    pub(crate) fn acknowledge_program_status(&mut self, cx: &mut Context<Self>);
}
impl TerminalPanel {
    fn tab_status(&self, cx: &App) -> Option<ProgramState>;
}
```

## Edge Cases and Failure Modes

- [ ] Split tab with a working terminal: no icon; collapse back to one Space shows it.
- [ ] `done` arriving on the selected tab stays until the user types; on a background tab it
  stays until the tab is selected.
- [ ] Process exits while `working`: icon gone. While `done`: icon stays until acknowledged.
- [ ] `idle` summary: no icon.
- [ ] Reduced motion: icons drawn, nothing animates.
- [ ] A tab that never received a 7501 report draws exactly what it drew before.

## Verification

- [ ] `view_tests.rs`: store / idle-as-none / exit rule / acknowledge rule / input acknowledges.
- [ ] `panel/tests.rs`: `tab_status` is `None` for a split tab and the view's state for a single
  one; `set_active(true)` acknowledges `Done`.
- [ ] Manual: `printf '\e]7501;state=working\e\\'`, `blocked`, `done`, `error`, `idle`,
  `clear` in a tab; a screenshot per state; then Claude Code >= 2.1.295.
