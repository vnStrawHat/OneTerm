# DEC-0009 Input channel membership is per Space, in memory, without default keys

Date: 2026-09-09

## Status

accepted

## Context

The owner asked for broadcast input channels (A..E) that group "tabs" (IN-0022), then noted
that one tab can hold several Spaces, each with its own terminal session. The unit of
membership decides what "typing into a channel" means in a split tab, what a drag into a
Space or a Close Space does to membership, and how the tab strip can show it honestly.

## Decision

- A channel member is one Space (one `TerminalView` and its session), keyed by the view's
  `EntityId`. A tab is a container: its chips list the distinct channels of its Spaces, and
  the tab-wide menu items ("Join All Spaces In Tab To X", "Leave With All Spaces In Tab") are
  loops over the tab's Spaces, never a second kind of membership.
- Sibling Spaces in the same tab and channel receive each other's input like any other
  member; this is the side-by-side "type once into every pane" layout.
- New Spaces from Split and Duplicate Session start as non-members. A tab dragged into an
  empty Space keeps its membership because the same view entity moves.
- Membership is in-memory only, like the manual tab title. Nothing about channels is written
  to `docks.json` or any other document.
- The seven actions ship unbound; users bind them in Settings › Key Bindings.

## Alternatives

- [x] Selected approach described above.
- [ ] Membership per tab (every Space in the tab is a member): breaks the common split layout
  of a shell next to a `htop`/`vim` pane, which would then receive broadcast bytes, and makes
  the side-by-side pane layout impossible to mix with a non-member pane.
- [ ] Membership per tab with the active Space as the only writer and reader: ambiguous when
  the active Space changes mid-typing, and still cannot express one non-member pane.
- [ ] Persist membership in `docks.json`: sessions are not restored across restarts, so a
  persisted channel would point at nothing; revisit only together with session restore.
- [ ] Default Ctrl+Alt+A..E bindings: Ctrl+Alt is AltGr on Vietnamese and most European
  layouts, so the defaults would swallow typed characters.

## Consequences

- [ ] Benefit to confirm: a split tab can hold members of one channel, members of different
  channels, and non-members at once, and the chips and frames show which is which.
- [ ] Tradeoff: joining a whole tab is two menu clicks (join this Space, then join all
  Spaces in the tab) instead of one.
- [ ] Follow-up: if session restore ever lands, decide then whether channels persist with it.
