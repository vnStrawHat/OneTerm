# Work: The empty Space advertises New Terminal Here and the search bar is quiet until used

ID: US-0115
Intake: IN-0042
Created: 2026-09-17

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
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

Two surfaces stop misinforming. The empty Space's placeholder names the action the user most
likely wants — New Terminal Here — instead of advertising only the two rarer ones; and the
in-terminal search bar stops claiming `0/0` before anything has been searched, with its two
unlabelled toggles explained on hover.

## Findings and proposals covered

`P10` (placeholder half) — *"Give the empty-Space placeholder its own first line —
"Right-click → New Terminal Here""*. The `Delete`-styling half of `P10` belongs to `US-0119`.

`P14` — *"Hide the `0/0` counter until a query exists; add tooltips to `Aa`/`W`."*

Addresses `F25` (medium) and `F34` (low), quoted from
`research/ux-walkthrough-2026-09-16.md`:

> | F25 | empty Space | The placeholder reads "Drag a terminal tab here / or right-click to
> split" and never mentions **New Terminal Here**, which is the first item of its own context
> menu and the likeliest action. That item also spawns the *default* shell only — no shell
> picker, unlike the `+` menu. | Discoverability: the placeholder advertises the two rarer
> actions and hides the common one. | medium | 07, 08 |

> | F34 | in-terminal search | The bar shows `0/0` before anything is typed; the modifier
> toggles are bare `Aa` and `W` with no visible labels; no regex toggle although `oneterm-vt`
> ships a `regex` feature. | Minor polish on an otherwise good feature. | low | 40, 41 |

## Scope

- [x] In scope:
  - `crates/terminal-view/src/space/render.rs` — the empty-Space placeholder copy, with
    "New Terminal Here" as its first line.
  - Whether the empty Space's context menu offers a shell picker like the `+` menu does, or
    keeps spawning the default shell only. `F25` names it; decide and record (see Context).
  - `crates/terminal-view/src/terminal_view/search.rs:300-312` — the match counter, hidden
    until a query exists.
  - Tooltips on the `Aa` (match case) and `W` (whole word) toggles.
- [x] Out of scope:
  - A regex toggle in the search bar. `F34` notes `oneterm-vt` ships a `regex` feature, but
    `P14` does not propose exposing it, and turning on an optional engine feature in the
    application is a new capability with its own cost — not polish. Recorded in Gaps as a
    finding this packet deliberately leaves standing.
  - The search behaviour itself, which the walkthrough praised: full-scrollback search,
    viewport scrolling, all-match highlighting and an accurate `n/total` once typing starts.
  - `F27` (the status bar collapsing to the clock in an empty Space), documented as intended
    in `docs/gui-layout.md` and not proposed.
  - Split mechanics, drag-tab-into-Space, and the active-Space cue (`US-0117`).

## Acceptance

- [x] The empty Space's placeholder names New Terminal Here first, and still mentions the drag
      and the split.
- [x] The named action is actually reachable from the context menu the placeholder points at,
      with the same wording in both places.
- [x] The search bar shows no match counter before a query is entered, and shows an accurate
      `n/total` from the first character typed.
- [x] A query with no matches shows a "no matches" state, not `0/0` — the distinction between
      "nothing searched" and "nothing found" is visible.
- [ ] Hovering `Aa` shows what it does; hovering `W` shows what it does.
- [x] Clearing the query returns the bar to its quiet state.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/terminal-split.md` §9 — the empty Space and its placeholder. **Update required:** the
  placeholder copy, and the shell-picker decision if it changes what the context menu offers.
- `docs/terminal-split.md` §8 — the active-Space cue and the border decisions. Read to confirm
  this packet does not touch them; `US-0117` owns that. **No change.**
- `docs/gui-layout.md` §Status bar — the documented reason an empty Space yields no terminal
  metrics (`F27`). Read so the placeholder copy does not contradict it. **No change.**
- `docs/auto-completion.md` — unrelated to search, read only to confirm the overlay and the
  search bar do not share the counter widget. Record the answer. **No change expected.**
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

Update required: `docs/terminal-split.md` §9 for the placeholder copy. The search bar has no
owning design document section; if the walk shows it needs one, note that in Gaps rather than
inventing a document here.

Reason: the placeholder text is the documented contents of a documented surface.

### Reconciliation

Changed: `docs/terminal-split.md` decision 9 — the new placeholder copy, quoted, plus the
shell-picker call recorded in the sentence before it.

Unchanged, reasons still valid: `docs/terminal-split.md` decision 8 (this packet does not touch
the cue; `US-0117` amends it); `docs/gui-layout.md` §Status bar (the placeholder copy does not
contradict the documented empty-Space behaviour); `docs/auto-completion.md` (checked as asked —
the completion overlay shares no widget with the search bar); `docs/PROJECT.md`.

The search bar still has no owning design section anywhere; recorded in Gaps rather than
inventing a document here, as the packet directed.
## Context

- The placeholder half is copy. The only real question is the second sentence of `F25`: the
  context menu's New Terminal Here spawns the **default** shell, while the `+` menu offers a
  shell picker. Two defensible answers:
  - Leave it. The empty Space is a placement action ("put a terminal here"), and the user who
    wants a specific shell can use the `+` menu and drag. Smallest change; the placeholder is
    then honest about what the menu does.
  - Give the menu a submenu of shells, matching the `+` menu.
  Ladder says leave it unless the owner's reaction to the after frame says otherwise — but the
  placeholder must not promise a picker that is not there. Decide, record the call here, and
  write the copy to match.
- The counter half is a visibility condition on a widget that already computes the right
  number. `search.rs:300-312` is where it renders. "No query" and "no matches" are two states,
  and collapsing them into `0/0` is exactly what makes the bar look broken before use — so the
  fix is two states rendered differently, not one state hidden.
- Tooltips: the kit's standard tooltip, on controls that already exist. No new component.
- Ladder: all three changes are inside `crates/terminal-view`, none needs a new type, a new
  seam or a new dependency.
- `research/before/07-split-right-empty-space.png`, `08-empty-space-menu.png` (placeholder and
  its menu) and `40-search-bar.png`, `41-search-matches.png` (the search bar) are the before
  pictures.

## Plan

- [x] Decide the shell-picker question and record it here before writing the copy.
- [x] Placeholder copy; confirm the menu wording matches.
- [x] Counter states and tooltips.
- [x] Update `docs/terminal-split.md` §9.
- [x] Re-capture the scenes.

## Decisions

**The empty Space's menu keeps spawning the default shell; no shell submenu.** Taken before
the copy was written, as the packet requires. The empty Space is a *placement* action ("put a
terminal here"), the `+` menu already owns shell *choice*, and a tab opened there drags into
the Space. Adding a second shell-picking surface would duplicate a list that `US-0114` has
just made authoritative in one place (`ShellKind::display_name`). The placeholder copy is
therefore written to promise exactly what the menu does — it names the row, not a picker.
Recorded as a sentence in `docs/terminal-split.md` decision 9; not a `DEC`, because it is a
scope call inside one surface, not a rule future work inherits.

## Verification Plan

1. **Focused:** `cargo test -p oneterm-terminal-view` — a test over the search bar's counter
   state selection (`no query` / `no matches` / `n of total`), which is a pure function of the
   query and the match list. The placeholder copy and the tooltips are not unit-testable in
   the gpui element tree; their proof is the GUI walk, and Evidence must say so rather than
   implying the crate's test count covers them.
2. **Unit:** `cargo test -p oneterm-terminal-view`.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):**
   - `07-split-right-empty-space.png` — the empty Space with the new placeholder.
   - `08-empty-space-menu.png` — its context menu, with wording matching the placeholder.
   - `40-search-bar.png` — the bar just opened, with no counter.
   - `41-search-matches.png` — the bar with a query and matches, counter accurate.
   Plus one scene the walkthrough did not have: a query with **no** matches, to show it is
   distinguishable from the quiet state. Capture it as `41b`.
   The walkthrough reached the search bar only by temporarily rebinding Find to `F2` through
   the app's own Key Bindings page, because posted messages cannot deliver `Ctrl-F`. If the
   same route is used, reset the binding afterwards and say so in Evidence.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Promising what the menu does not do.** If the placeholder says "New Terminal Here" and the
  menu item spawns only the default shell while the user expected a picker, the copy has moved
  the confusion rather than removed it. The shell-picker decision has to be made before the
  copy is written, not after.
- **Hiding the counter too aggressively.** Hiding it while a query exists but has no matches
  would remove the feedback that the search ran. Two states, not one hidden one.
- **Layout shift.** A counter that appears and disappears moves the controls beside it. Either
  reserve the space or place the counter where nothing shifts; a bar that jumps on every
  keystroke is worse than `0/0`.
- **The regex finding stays open.** `F34` names three things and this packet fixes two. Say so
  in Gaps, and let the `US-0126` report record it as partially fixed rather than quietly
  ticking `F34`.

## Evidence and Gaps

### Evidence

Branch `worktree-agent-a8b32ce5d4725a1f5`, commit `feat(terminal-view): the empty Space names New Terminal
Here and the search bar is quiet until used`.

Changed:

- `crates/terminal-view/src/space/render.rs` — the placeholder copy.
- `crates/terminal-view/src/terminal_view/search.rs` — `MatchCount` + `match_count`, the
  counter's colour and its reserved width, plus three tests.
- `docs/terminal-split.md` decision 9.

Checks:

- `cargo test -p oneterm-terminal-view --lib` — 351 passed, 0 failed, including
  `the_counter_is_silent_until_a_query_exists`, `nothing_found_is_not_the_same_state_as_nothing_searched`
  and `a_match_list_counts_from_one`. Those three cover the counter's state selection, which is
  the only pure logic here: **the placeholder copy and the tooltips are element properties and
  are proved by the captures below, not by the crate's test count.**
- `pwsh scripts/ci-local.ps1` — see below.

GUI walk:

- `evidence/US-0115-07-split-right-empty-space.png` — the placeholder reads `Space #1`,
  `Right-click -> New Terminal Here`, `or split, or drag a terminal tab here`. The arrow is
  U+2192 and renders.
- `evidence/US-0115-08-empty-space-menu.png` — the context menu the copy points at, with
  **New Terminal Here** as its first row, worded identically.
- `evidence/US-0115-40-search-bar.png` — the bar just opened: the counter slot is empty, its
  width already reserved, so nothing moves when a query arrives.
- `evidence/US-0115-41-search-matches.png` — `1/2` with both matches highlighted.
- `evidence/US-0115-41b-search-no-matches.png` — `No matches` in muted text, visibly a
  different state from the quiet one.

Method note, as the packet requires: posted messages cannot deliver `Ctrl-F`, confirmed again
here (the `f` landed in the terminal). Rather than drive the Key Bindings page, Find was
rebound to `F2` by writing `key_bindings: { "find": "f2" }` into `target/ui_config.json`, and
**the file was restored from its backup afterwards** — it is back to
`{ "right_dock_mode": "ssh_client" }`, verified after the walk.

`docs/auto-completion.md` was read as the packet asked: the completion overlay shares no widget
with the search bar (no counter element in `crates/terminal-view/src/completion/`), so nothing
there changes. No doc update needed.

The `Aa` and `W` toggles **already carried tooltips** ("Match case", "Match whole word",
`search.rs`), so `P14`'s tooltip half needed no change. `F34` recorded them as bare because the
walkthrough never hovered them.

### Rework round (after independent verification)

- **`F-115.1` — the reserved counter width was exact, with no headroom.** The verifier measured
  `No matches` at x=756-822 in `US-0115-41b-search-no-matches.png`: 66 px in a 66 px slot. The
  floor was in pixels while the text is not — the kit sets the window's rem size from the
  theme's font size (`reference/gpui-kit/crates/component/src/root.rs:579`), which the user
  sets as `ui_font_size`, so `text_xs` scales and a fixed floor does not. It is now
  `min_w(rems(4.75))`: 76 px at the default 16 px rem (a sixth of headroom over the widest
  state) and it grows with the font instead of being overflowed by it.
- `evidence/US-0115-07-split-right-empty-space.png` and `-08-empty-space-menu.png` were
  re-captured on the reworked build, because `US-0117`'s corner label changed in the same
  round and those frames carry it. The placeholder copy and the menu row are unchanged and
  still byte-identical to each other.

### Gaps

- **The regex toggle is not shipped, deliberately.** `F34` names three things and this packet
  fixes two. `oneterm-vt` does ship a `regex` feature, but turning on an optional engine feature
  in the application is a new capability with its own cost, and `P14` does not propose it.
  `F34` should be reported as **partially fixed** by `US-0126`, not ticked.
- **The tooltips are not captured.** A tooltip needs a real hover; posted `WM_MOUSEMOVE` does
  not hold the pointer still long enough for the kit's delay. Their existence is shown by the
  source, not by a frame.
- **The search bar still has no owning design section.** The packet said to note this rather
  than invent a document: `docs/gui-layout.md` and `docs/terminal-split.md` both describe
  surfaces around it, neither describes the bar. A future packet could give it one.
- `F27` (the status bar collapsing to the clock in an empty Space) is visible in
  `US-0115-07-split-right-empty-space.png` and is unchanged, as scoped.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
