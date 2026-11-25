# US-0202 verification (advisor)

- Packet: `US-0202-strip-drag-reorders-like-the-rail.md`; commits `452f34ca` (packet) and `61bad2ce` (code), reviewed at `018eccd5` on `verify/in-0055-drag-zoom` (base `d2d15e03`), 2026-10-23, worktree `agent-ab6a00f0423e3f789`. The records commit precedes the code commit.
- Read: `crates/terminal-view/src/panel/tab_title.rs` (`render_tab_strip`, `collapsed_row`), `crates/terminal-view/src/panel/spaces.rs`, `crates/terminal-view/src/space/render.rs` (`render_placeholder`), `space/mod.rs`, the terminal-view and workspace tests, `docs/terminal-split/03-drag-drop.md`, `05`, `07`, `docs/gui-layout.md`, the HLD; kit `reference/gpui-kit/crates/component/src/dock/tab_panel.rs:151-160, 400-415` (the `Tab`'s `on_drag`, guarded by `is_draggable`).
- No code changed, no pinned test added: no code defect.

## Advisor review (US-0198 rework 8, US-0202, US-0203)

### Verdict

**PASS.** The strip's row starts no drag (`tab_title.rs:706-733`: the row keeps middle-click close and the context menu only; the label keeps its double-click rename `on_click`, `:603-616`; the `x` keeps `on_mouse_down(Left, stop_propagation)`, `:689-700`). The kit's `Tab` is therefore the only drag source, and the empty Space resolves it (`render_placeholder`, `space/render.rs:256-266`, through `TerminalPanel::dragged_by`, `spaces.rs:199-203`).

### Gate re-run

As recorded in `US-0198-verify.md` "Advisor review (US-0198 rework 8, US-0202, US-0203)": every listed crate green, clippy clean, `check-english` and `check-doc-paths` passed.

### Attacked

1. **`DragTerminalTab` deletion completeness.** `rtk proxy grep -rn 'DragTerminalTab|handle_tab_drop|start_tab_drag' crates/ --include='*.rs'`: no hit. In `docs/` outside intakes and decisions the name survives only as history (`docs/terminal-split/03-drag-drop.md:13-19, 103`, `07-roadmap-risks.md:47, 62`), each marked as removed by `US-0202`. `space/mod.rs` no longer re-exports it; clippy has no dead-code warning.
2. **`x` close and double-click rename vs the drag.** The `x` stops the left press before the kit's `Tab` can arm its drag (`tab_title.rs:697-699`) and closes on click (`:700-706`). The label's `on_click` fires only at `click_count() == 2` and stops propagation there (`:606-615`); a press on it with no movement arms the kit's drag and releases it as a click, which is how the kit's own tabs select. Not pinned by a test (none existed before either); for the ui-verifier walk.
3. **Drop on non-empty terminal content, all three sources.** `verify_the_empty_space_still_wins_over_the_terminal_root_catch_all` (`tests.rs:2086-2200`) drives the real strip tab by its padding and by its title through gpui dispatch: the left edge zone of a live terminal swallows the drop (no split, one group), the empty Space takes it. A rail row and a collapsed cell are the same `DragPanel`, and `a_dragged_dock_panel_dropped_on_an_empty_space_moves_its_terminal` (`:640-741`) covers the occupied-Space and unknown-id cases for the payload itself.
4. **Data loss: the moved session is never closed.** `source_probe.close_calls() == 0` after the refused drops (`:710-714`) and after the real move (`:737-741`); `move_tab_into` hands the view back to the source on the impossible non-empty target (`spaces.rs:247-262`) and only shuts the session down when it has nowhere to go (`:285-291`, logged at error).
5. **Reorder by the title.** `a_strip_tab_dragged_by_its_title_reorders` (`tab_rail_tests.rs:934-981`), real skin and strip layout; the second half drops on content that takes the drop and the dock is unchanged.
6. **Crate rules.** No dependency edge moved (`Cargo.toml` untouched); `terminal-view` resolves the drag through `AppState::dock_area`, as before.

### Notes (non-blocking)

- **N1 (Info).** The packet's Plan still shows `- [ ] Docs, gate.` while Acceptance and Evidence record both as done; tick it or drop the line when the packet is accepted.

### Still by hand (ui-verifier)

Drag by the title in two themes; the drop line between strip tabs; a drop onto an empty Space; double-click rename and the `x` still work on the strip.
