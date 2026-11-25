# IN-0055 spec review (advisor)

- Reviewed: `IN-0055.md`, `high-level-design.md` at commit `c71ac256`.
- Scope (narrow, as asked): HLD § Rendering seam (with § Rail anatomy), the drag rows of
  § Interactions, intake Q8; spot-checks of the `US-0197` width examples and the contrast-pair
  claim.
- Sources read: `crates/workspace/src/layout/workspace/{mod.rs,dock_skin.rs,layout.rs}`,
  `crates/terminal-view/src/panel/{tab_title.rs,terminal_panel.rs}`,
  `crates/terminal-view/src/space/render.rs`, `scripts/check-theme-contrast.py`,
  `docs/gui-layout.md`, `IN-0054`, `US-0193`; the pinned kit under `reference/gpui-kit/`
  (identical, line endings aside, to the `gpui-base` / `gpui-component` 0.7.3 sources that
  `Cargo.lock` builds); `gpui-pre` 0.3.9 from the cargo registry for event dispatch.
- No cargo run (spec review). `check-english` and `check-doc-paths` pass.

## Verdict

**Sound with fixes.** Option B1 (absolute rail, content frame offset by a left margin) is viable
and is the right choice; the drag-priority claim holds in gpui as built. Two medium findings
(width guarantee with a split centre; the reorder affordance) and several low ones below.

## Answers to the four questions

1. **Absolute rail (B1).** The column claim is correct: base applies `.flex().flex_col()
   .size_full().overflow_hidden()` after `frame()` (`reference/gpui-kit/crates/base/src/dock/tab_group.rs:691-709`)
   and `.flex().flex_col().flex_1().min_h(0)` after `content_frame()` (`:710-728`), so a skin
   cannot turn the group into a row, but it keeps full control of the two elements' own
   positioning and margins (base never sets a margin). Drop handling is installed **only on the
   content frame** (`:729-742`) and measured against that element's own bounds
   (`on_panel_drag_move` / `on_item_drag_move` use `drag.bounds`, `:516-558`). Base has its own
   test that a skin may place the content frame anywhere and the drag is measured there:
   `the_content_frame_is_what_a_drag_is_measured_against` (`:1764-1784`; its renderer at
   `:1592-1615` positions the content frame absolutely). A left **margin** (not padding) keeps the
   rail out of those bounds; padding would put the rail inside them. While a drag hovers the
   rail, the content frame's move listener sees the pointer outside its bounds and clears the
   indicator (`:523-526`). The drop placeholder is drawn relative to the content frame
   (`base/src/dock/drag.rs:230-255`, kit `component/src/dock/tab_panel.rs:794-835`), so the
   offset does not misplace it.
   **The terminal resizes, it is not overlapped:** the content frame is a stretched item of a
   column, so its width is the group width minus the margin; the active panel is
   `panel.cached(absolute().size_full())` inside it (`tab_panel.rs:785-791`), and a cached view
   is laid out again when its bounds change (`gpui-pre-0.3.9/src/view.rs:530-535`). One resize
   per visible terminal, as the HLD says.
2. **Drag priority.** Holds. gpui registers an element's mouse listeners before painting its
   children (`gpui-pre-0.3.9/src/elements/div.rs:2589` vs `:2615`) and dispatches the bubble
   phase in reverse registration order (`src/window.rs:5956-5964`), so the inner title row's
   move listener runs first; when it starts a drag it sets `active_drag` and calls
   `stop_propagation` (`div.rs:2979-3011`), the outer row's listener is never reached, and every
   other pending mouse-down is cleared on the next frame (`div.rs:2240-2245`). Both elements do
   record a pending mouse-down (`div.rs:2956-2969`), so the outer `DragPanel` starts only where
   the inner row has no hitbox, i.e. the leading padding. The `x` cannot start either drag: it
   stops left mouse-down (`crates/terminal-view/src/panel/tab_title.rs:330-332`). Same mechanism
   as the strip today (`tab_panel.rs:580-593` around the title row's `on_drag`,
   `tab_title.rs:271-281`).
3. **Fallback B2.** Realistic without patching: `TabGroupRenderer::render_active_panel` is a
   public hook and `OneTermTabGroupSkin` already delegates to the kit's version
   (`crates/workspace/src/layout/workspace/dock_skin.rs:143-151`), so it can wrap that element
   in a row with the rail. Its weaknesses are larger than the HLD states, see finding 4.
4. **Contradictions with `docs/gui-layout.md`, `IN-0054`, `US-0193`.** None of substance.
   Wording items in findings 7, 9 and 11.

## Findings

1. **Medium - the width guarantee ignores a split centre.** HLD § Width
   (`high-level-design.md:291-294`) says the terminal keeps at least 40 % of a 1000 px window.
   That holds only with one centre group. Q7 recommends one rail per group, and
   `clamp_tab_rail_width` clamps against the window, not the group: at 1000 px with the dock open
   and two side-by-side groups, 1000 - 350 - 2 x 250 = 150 px for two terminals. Fix: either clamp
   each rail against its own group's width (the renderer may read painted group bounds through
   `on_prepaint` on its frame, `tab_group.rs:910-911`) or state the limitation next to the 40 %
   claim and in Q3 / Q7.
2. **Medium - the reorder handle is small, invisible, and the natural gesture silently fails.**
   In the rail, almost the whole row is the title element, whose drag is `DragTerminalTab`
   (`tab_title.rs:271-281`); rows accept only `DragPanel` / `AnyDrag` (HLD § Rail anatomy,
   `high-level-design.md:104-105`), and the workspace cannot accept `DragTerminalTab` (the type is
   `pub(crate)` in `oneterm-terminal-view`, `crates/terminal-view/src/space/mod.rs:18`, and R4
   forbids the shell from naming it). So dragging a row by its label onto another row does
   nothing, which is the first thing a user tries in a vertical list. The leading edge is 8 px,
   smaller than the strip's 12 px kit padding
   (`reference/gpui-kit/crates/component/src/tab/tab.rs:72-89`), and nothing marks it. The intake
   lists label-reorder as a follow-up (`IN-0055.md:196-197`), which is fine, but the HLD should at
   least: draw a grip (or a grab cursor) on the leading edge on hover, use 12 px to match the
   strip, and name the silent failure in § Interactions. Proposed owner question below.
3. **Low - Q8 is mostly answerable now; narrow it.** (b) is settled by the dispatch analysis
   above; keep it as a regression test in `US-0196`, not a gate. (c) passes today: with
   `tab_bar.background` added to `tab.active.foreground`'s surfaces in a scratch copy of the
   script, `check-theme-contrast.py` reports 1638 pairings all >= 4.5:1 and 624 hierarchy
   comparisons passing across every built-in variant. (a) is half-proven by base's own test
   (`tab_group.rs:1764-1784`); what is left for the spike is the layout composition (an absolute
   first child plus a left-margined content frame in base's forced column) and the terminal
   resize, which is a short gpui test.
4. **Low - B2 has a drop defect the HLD does not list.** With the rail inside the content frame,
   a `DragPanel` released on any rail pixel that has no drop handler of its own (the header, the
   gaps) bubbles to the content frame's `on_drop` and **splits the centre** using the placement
   the move listener resolved over the rail (`tab_group.rs:729-742`, `:626-669`); rows win only
   because the inner `on_drop` runs first and stops propagation (`div.rs:2903-2931`). The split
   placeholder is also painted over the rail while reordering. If B2 is ever used, every rail
   pixel needs a drop handler (header = append) and the indicator must be suppressed over the
   rail.
5. **Low - the `AnyDrag` handlers on rows and the drop space are dead code in OneTerm.** No crate
   creates a `gpui_base::dock::AnyDrag` and nothing subscribes to `TabGroupEvent::DragDrop`
   (no match for `AnyDrag` or `DragDrop` under `crates/`). Mirroring the kit is harmless but
   untestable here; drop those two clauses of § Rail anatomy, or keep them and say why.
6. **Low - which groups get a rail is not stated.** The right dock is not locked
   (`crates/workspace/src/layout/workspace/layout.rs:50-55`; `droppable` is `!locked`,
   `tab_group.rs:454-460`), so a terminal tab dropped onto it makes a two-panel group for which
   `owns_header` is false (`dock_skin.rs:105-110`), and the rail would be drawn inside a
   240-480 px dock. Code reading only, not reproduced on screen. State the rule: for example,
   a rail only when every panel in the group is a `terminal` panel (the `owns_header` pattern of
   checking `panel_name`), else the kit strip.
7. **Low - the contrast table overstates what is measured.** HLD § Themes and contrast row
   "`x`, chips, recording dot, bell ... In `SURFACES` today: yes": only `muted.foreground` and
   `warning.background` are measured on the tab surfaces
   (`scripts/check-theme-contrast.py:186-202`, `:245-252`); `chart_1..5` and `danger` have no
   entry. Likewise § Interactions row OSC 7501 ("colours measured on the tab surfaces
   already"): only the bell's `warning` is (`US-0193-tab-status-icon.md:93`); the `primary`,
   `success` and `danger` icons are not. No new gap (same surfaces as the strip), but the wording
   should say "as for the strip", not "yes".
8. **Low - the new surface also turns on a hierarchy check.** Adding `tab_bar.background` to
   `tab.active.foreground` makes `HIERARCHY`'s `("tab.active.foreground", "muted.foreground")`
   (`check-theme-contrast.py:264`) compare the pair on `tab_bar.background` too, because
   `muted.foreground` is already listed there (`:200`). It passes today (finding 3); the HLD and
   the `US-0196` acceptance should name it so a failure there is not a surprise. The intake's
   fact (`IN-0055.md:75-76`) and the HLD's "no" in the `SURFACES` column are correct.
9. **Low - drag source citation and wording.** The intake attributes the `DragTerminalTab`
   source to `crates/terminal-view/src/space/render.rs:283-287` (`IN-0055.md:59-60`); that is the
   empty Space's drop target. The source is the whole title row, `tab_title.rs:271-281`, so it
   covers the chips, recording dot, status icon and label, not only "the label" as the HLD
   § Interactions row "Drag a tab into an empty Space" says; only the `x` is excluded.
10. **Info - zoom and drags.** A zoomed group is neither draggable nor droppable
    (`tab_group.rs:441-460`), so rail reorder and drops are off while zoomed, as on the strip;
    say so in the Zoom row. The kit draws Zoom Out whenever the group is zoomed, whatever the
    control says (`tab_panel.rs:305-307`); the HLD's rule (`high-level-design.md:101`) should
    include that case. The kit's tooltips use its private `t!` strings, so the rail carries its
    own "Zoom In" / "Zoom Out" text.
11. **Info - doc consistency.** `docs/gui-layout.md:30` says "the dock buttons in the tab bar and
    the status bar"; `IN-0054`'s HLD lists the tab-bar dock button as one of three entry points
    (`docs/spec-intakes/IN-0054-right-dock-follows-the-tab-kind/high-level-design.md:33-35`,
    `:65`). The vertical layout removes that one (Q5), which is consistent, but the `US-0196`
    amendment to `gui-layout.md` must qualify the sentence. `US-0193` behaviour (single-Space
    only, acknowledge on `set_active(true)`,
    `crates/terminal-view/src/panel/terminal_panel.rs:670-684`) is unaffected: a row click goes
    through `select_tab`, the same path as a strip click.

## Spot-checks

- **Width examples (HLD § Width): all six reproduce.** 1920/220 -> ceiling 400, 220;
  1920/520 -> 400; 1000/300 -> ceiling 250, 250; 600/220 -> ceiling 150, 150; 480/220 ->
  ceiling max(140, 120) = 140, 140; 0/100 -> clamp(100, 140, 400) = 140. The companion figures
  also reproduce: 900 px -> dock 315 (35 %, the 480 px default capped,
  `crates/workspace/src/layout/workspace/mod.rs:32-42`, `:76-85`), rail 220, terminal 365;
  1000 px -> 350 + 250 leaves 400; 600 px -> 240 + 150 leaves 210, and 240 + 220 leaves 140.
  (Single centre group only, finding 1.)
- **Contrast pair.** Correct: `tab.active.foreground` is measured only on
  `tab.active.background` (`scripts/check-theme-contrast.py:129`); the strip draws the hovered
  label in `tab.active.foreground` on the bar fill (kit `component/src/tab/tab.rs:172-184`,
  transparent tab fill over `tab_bar`).
- **Other citations in scope, checked and correct:** `tab_group.rs:683-757`, `:768-893`;
  `tab_panel.rs:140-146`, `:191-198`, `:433-687`, `:464-470`, `:580-661`, `:690-792`;
  `base/src/tabs.rs:149-234` (the base `Tab` stops left mouse-down at `:173` and has
  `set_position`); `base/src/toggle_group.rs:60`; `component/src/dock/panel.rs:233`;
  `dock_skin.rs:39-170`; `tab_title.rs:207-345`, `:253-254`, `:257-267`, `:268`;
  `terminal_panel.rs:721`; `layout/workspace/mod.rs:192`, `:562-594`. The component `TabBar` /
  `Tab` have no axis option (no match for `vertical` / `Axis` under `component/src/tab/`).

## Owner questions to add or change

- **Q8 (change):** reduce to (a) only, the layout composition and terminal resize; move (b) and
  (c) to `US-0196` acceptance tests (finding 3).
- **Q9 (add, owner): the reorder handle.** Recommended: keep reorder on the leading edge as on
  the strip, but 12 px wide with a grip glyph and grab cursor on row hover. Alternative: reorder
  by dragging the label (the listed follow-up), which needs `oneterm-terminal-view` to accept its
  own drop on a row, since the shell cannot name `DragTerminalTab`.
- **Q3 / Q7 (amend):** say whether the rail width is clamped per window or per group once the
  centre is split (finding 1). Recommended: per group.
- **Rule to record (architect, not owner):** which groups draw a rail (finding 6).
