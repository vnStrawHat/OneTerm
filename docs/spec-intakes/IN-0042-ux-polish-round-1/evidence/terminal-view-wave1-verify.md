# Independent verification — US-0114, US-0115, US-0116, US-0117 (IN-0042, wave 1)

Verifier: an independent agent that did **not** write this work. Adversarial read: every claim was
re-derived from the source, the pinned kit, the gpui source, the captures' pixels, or a re-run of
the checks. Nothing below is taken from the packets' own Evidence sections.

- Base: `5d6f2e28` (branch `worktree-agent-a8b32ce5d4725a1f5`, main merged at `ffc02c19`).
- Reviewed: `git diff main...HEAD` in full (39 files, +1111/-162), the four packets, `F1`/`F7`/
  `F11`/`F12`/`F13`/`F25`/`F26`/`F34` in `research/ux-walkthrough-2026-09-16.md`, the `research/before/`
  frames, `docs/gui-layout.md`, `docs/terminal-split.md`, `docs/ssh-client-connect.md`,
  `AGENTS.md`, `docs/HARNESS.md`, `docs/agents/crate-dependency-rules.md`.
- Working tree was restored to `5d6f2e28` after the mutation run; `git status` clean before commit.

## Verdicts

| packet | verdict |
| --- | --- |
| US-0114 | **PASS** — every claim verified; 2 minor findings. |
| US-0115 | **PASS** — every claim verified; 1 minor finding. |
| US-0116 | **PASS** — every claim verified, including all nine kit citations; 0 major, 4 minor. |
| US-0117 | **PASS WITH RESERVATIONS** — the cue is real, independently measured, and its missing light-theme frame was taken here; but the new corner label permanently covers live terminal output, and two written statements (what the ring paints over, and `#N` being "0-based") are wrong. 1 major, 3 minor. |
| **overall** | **PASS WITH RESERVATIONS.** Nothing here is broken or unsafe: the gate is green, the pure logic is mutation-proof, every kit citation holds, and the two bug fixes found during the walk are correct. `US-0117`'s corner label needs an owner call (F-117.1) and three documented sentences need correcting (F-117.2, F-117.3, F-116.1) before the round is reported by `US-0126`. |

Harness compliance: the four packet files already existed on `main` and appear in this diff as
*modifications*, so the pre-code documentation gate was met; the implementation commits
(`99b70eb2`, `3855dcae`, `03763c40`, `713a57cf`) carry exactly the subjects the packets claim, and
`fa1030e2` adds the evidence afterwards. Only the branch name in the four Evidence sections
(`ux/tabs-spaces-menus`) is wrong — the work is on `worktree-agent-a8b32ce5d4725a1f5`. Cosmetic.

## Commands run (final lines)

| command | final line |
| --- | --- |
| `python scripts/verify-dependency-graph.py` | `Dependency graph policy passed for 20 workspace packages and 20 explicit members, and no tracked path is over 150 characters.` |
| `cargo test -p oneterm-terminal-view -p oneterm-core` | `test result: ok. 351 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.44s` (terminal-view lib; whole invocation exit code 0) |
| `pwsh scripts/ci-local.ps1` | `ci-local: all checks passed.` (exit code 0; includes `check-theme-contrast: 702 foreground/surface pairings across 117 token/variant rows, all >= 4.5:1` and `Doc path check passed for 200 current paths in 11 documents.`) |

## Mutation testing — the three decision functions are actually pinned

Four mutations applied at once, `cargo test -p oneterm-terminal-view --lib`, then `git status`
verified clean again.

| # | mutation | caught by |
| --- | --- | --- |
| M-a | `resolve_tab_label` (`tab_title.rs:72`) returns the fallback even when a live OSC title exists | `a_live_osc_title_still_wins_over_the_shell_name`, `live_title_is_used`, `windows_drive_path_shortened_to_basename`, `posix_path_shortened_to_basename`, `relative_or_descriptive_titles_not_trimmed`, `phase1_terminal_titles_are_sanitized_by_policy` |
| M-b | `tabs_to_close` (`tab_title.rs:270`) drops `victims.reverse()` (ascending order) | `close_others_keeps_the_right_clicked_tab_and_closes_right_to_left`, `close_to_the_right_stops_at_the_right_clicked_tab` |
| M-c | `match_count` (`search.rs:59`) returns `NoMatches` instead of `Idle` with no query | `the_counter_is_silent_until_a_query_exists` |
| M-d | `ShellKind::display_name` (`core/src/config/shell.rs:42`) gives `Pwsh` the same label as `PowerShell` | `each_shell_kind_gets_its_own_tab_label` |

`test result: FAILED. 341 passed; 10 failed` — every mutation is caught, and only by the tests that
should catch it (`a_target_outside_the_list_closes_nothing`, `nothing_found_is_not_the_same_state_as_nothing_searched`
and `a_match_list_counts_from_one` correctly stayed green under M-b/M-c). The three "decision"
functions the brief named are genuinely pinned, not merely present.

## US-0114 — tabs named after their shell, and the menu names its dialog

### Claims checked

| claim | verdict | where |
| --- | --- | --- |
| `ShellKind::display_name` in `crates/core` labels both the "+" rows and the tab fallback | **true** | `crates/core/src/config/shell.rs:34-47`; menu rows `crates/terminal-view/src/panel/terminal_panel.rs:695-704`; tab fallback via `shell_tab_title` `crates/terminal-view/src/panel/tab_title.rs:82-93`, used at `terminal_panel.rs:558-577` |
| `DEFAULT_TAB_TITLE` only for the reset tab / program-less custom shell | **true** | `terminal_panel.rs:536` (const), reset path `crates/terminal-view/src/panel/spaces.rs:80`, custom-with-no-program `tab_title.rs:87-90`; no other producer of `DEFAULT_TAB_TITLE` remains |
| OSC 0/2 and manual rename still win | **true** | precedence is `tab_title_override` → live → `tab_title` at `tab_title.rs:33-38` + `72-77`, reached through `tab_label_with_title` (`terminal_panel.rs:443-449`), which also honours `TabTitleMode::Default`. Tested by `a_live_osc_title_still_wins_over_the_shell_name` and mutation M-a |
| `open_new_session_dialog` → `open_quick_connect_dialog`, `open_new_saved_session_dialog` added, both registered in `crates/app/src/init.rs` | **true** | `crates/state/src/commands.rs:41-48`, `crates/app/src/init.rs:67-68`, `crates/session-ui/src/lib.rs:47-59`, call site `crates/workspace/src/layout/workspace/actions.rs:99`; both test doubles updated (`state/src/services.rs:148`, `terminal-view/src/panel/tests.rs:461`) |
| last rows "Quick Connect…" + "New Saved Session…", `FIXED_ROWS` 6→7, order above the closing separator untouched | **true** | `terminal_panel.rs:736-742`, `:763`. Arithmetic re-derived: 3 shells + "SSH Sessions" heading + closing separator + 2 closing rows = 7. Frame `US-0114-02-plus-menu.png` shows the three shells, the heading, the disabled "No saved sessions" hint and the separator all unmoved |
| no new crate edge | **true** | `verify-dependency-graph.py` passes (line above); the new command is an `fn(&mut Window, &mut App)` on a `crates/state` type, so R1/R4/R5/R10 hold and `terminal-view` still names no `session-ui` type |

### Precedence table, re-derived from source (not from the packet)

| situation | label | proof |
| --- | --- | --- |
| manual rename | the rename | `tab_title.rs:34-36` |
| live OSC 0/2 title, `TabTitleMode::Osc` (default) | the title, path-trimmed | `tab_title.rs:73`, `terminal_panel.rs:444-447` |
| live OSC 0/2 title, `TabTitleMode::Default` | the shell name | `terminal_panel.rs:446` |
| "+" menu → PowerShell | `"PowerShell"` | `spawn_local_view` clears `program` for an explicit kind (`terminal_panel.rs:277-278`), so `shell_tab_title(kind, None)` is the kind's name |
| startup / New Terminal Here, settings kind = Custom + program | the program's file stem | `tab_title.rs:83-91`; `custom_shell_is_named_after_its_program` |
| settings kind = Custom, no program | `"Terminal"` | same |
| SSH tab | the session label | `PanelSpec::Session` still takes `title` — untouched by this diff |

### Findings

- **minor (F-114.1) — the "one list" claim is not true, and the second list already disagrees.**
  `crates/core/src/config/shell.rs:36-37` says *"One list, so a menu row and the tab it opens can
  never disagree"*, and `US-0115`'s Decisions section calls `display_name` *"a list that `US-0114` has
  just made authoritative in one place"*. There is a third list:
  `crates/settings-ui/src/terminal/shell.rs:11-19` (`SHELL_KINDS`), which names the same enum
  `"cmd.exe (Windows)"`, `"Windows PowerShell 5.x"`, `"PowerShell 7+ (pwsh)"`, `"Custom"`. So the
  Settings dropdown a user picks their default shell from disagrees with the tab it produces
  ("cmd.exe (Windows)" → a tab reading "Command Prompt"), and nothing prevents further drift. The
  packet did not review that file. This is the R10 "never duplicated" smell, not an R-rule
  violation the script can see. Either route `SHELL_KINDS` through `display_name` (it uses the label
  as the persisted key, so that is not a free change) or drop the "one list" sentence.
- **minor (F-114.2) — an explicit `program` under a non-`Custom` kind still produces a lying label.**
  `resolve_shell` honours `cfg.program` for *every* kind (`crates/core/src/config/shell.rs:240-244`
  and the sibling arms), but `shell_tab_title` consults `program` only when `kind == Custom`
  (`tab_title.rs:83`). A user with `kind: cmd, program: C:\tools\nu.exe` runs nushell in a tab
  labelled "Command Prompt" — the same class of untruth `F1` is about, in a rarer configuration.
  Only the default-shell path is affected (the "+" menu clears `program`). One-line fix if wanted:
  prefer the program stem whenever `program.is_some()`.
- Both unchecked Acceptance boxes ("works with sessions saved / does not reorder", "keyboard
  navigation reaches the new row") match the recorded Gaps honestly. The unchecked boxes are the
  correct state, not an oversight.

## US-0115 — the empty Space advertises New Terminal Here, the search bar is quiet until used

### Claims checked

| claim | verdict | where |
| --- | --- | --- |
| `MatchCount::{Idle, NoMatches, Nth}` with a reserved width | **true** | `crates/terminal-view/src/terminal_view/search.rs:38-45`, `47-56`, `59-67`; reserved width `search.rs:416-421` (`min_w(px(66.))` + `text_center`) |
| `0/0` no longer shows at Idle | **true** | `MatchCount::Idle.label()` is `SharedString::default()` (`search.rs:50`); the `"0/0"` literal is gone from the file. Frame `US-0115-40-search-bar.png` shows the empty slot |
| placeholder copy matches the menu's actual item label | **true, verbatim** | placeholder `crates/terminal-view/src/space/render.rs:307-308`; the row it names is `PopupMenuItem::new("New Terminal Here")` at `render.rs:337` — byte-identical strings |
| the placeholder promises the default shell correctly | **true** | the row calls `new_terminal_here` (`render.rs:340`) → `spawn_local_view(&self.deps, None, …)` (`crates/terminal-view/src/panel/spaces.rs:113`), i.e. exactly the settings default shell, no picker — which is what the copy and `terminal-split.md` decision 9 now say |
| the `Aa` / `W` tooltips already existed | **true** | `search.rs:384` `"Match case"`, `search.rs:397` `"Match whole word"` — both predate this diff (`git diff` touches neither line) |
| `terminal-split.md` decision 9 accurate | **true** | the doc quotes the two rendered lines exactly and records the no-shell-picker call; decision 8 is left to `US-0117` as claimed |

The reserved width was checked against the pixels, not the source: in
`US-0115-41b-search-no-matches.png` the string `No matches` occupies x≈756-822 — 66 px, exactly the
reserved slot, so the longest state fits without pushing the arrow buttons. `1/2` in
`US-0115-41-search-matches.png` sits in the same slot. The bar does not jump.

### Findings

- **minor (F-115.1) — the reserved width is exact, with no headroom.** 66 px holds `No matches` at
  the current `text_xs` and the current UI font, with ~0 px to spare. `ui_font_size` is
  user-configurable (`ui_config.json`), and a larger one will overflow the slot and start moving
  the buttons again — the precise failure the packet's "Layout shift" risk names. A `min_w` in `em`,
  or simply a wider floor, would be robust. Not a defect today; it is a one-setting-away defect.

## US-0116 — tab context menu and tab list

### Kit citations — all nine verified line by line against `reference/gpui-kit/`

| citation | claim | verdict |
| --- | --- | --- |
| `component/src/tab/tab.rs:871-873` | the only handler on a `Tab` is `on_click` | **accurate** (`.when_some(self.on_click…)`; an exhaustive scan for `on_mouse_down`/`secondary`/`on_hover`/`on_drag` in the file hits only 557, 871, 872) |
| `base/src/tabs.rs:172` | the base element stops **left** mouse-down only | **accurate** (`.on_mouse_down(MouseButton::Left, |_,_,cx| cx.stop_propagation())`, the only `stop_propagation` in the render body) |
| `component/src/dock/tab_panel.rs:333-337` | a panel may add rows, prepended above the kit's separator | **accurate** (`:333` `.dropdown_menu(`, `:335` `panel.dropdown_menu(menu, window, cx)`, `:337` `.separator()`) |
| `component/src/dock/tab_panel.rs:338-345` | the zoom row is appended after the panel's rows; `zoom_control()` only enables/disables it | **accurate** (`menu_with_disabled(t!("Dock.Zoom In"|"Dock.Zoom Out"), ToggleZoom, !menu_zoom)`; `menu_zoom` comes from `zoom_control(group, cx)` at `:283-285`) |
| `component/locales/ui.yml:163-164` + `component/src/lib.rs:123` | wording is the compiled-in `Dock.Zoom In`, `rust_i18n` stores per crate | **accurate** (`163: Zoom In:` / `164: en: Zoom In` under `Dock:` at `:150`; `lib.rs:123` is the `rust_i18n::i18n!("locales", fallback="en")` invocation) |
| `component/src/tab/tab.rs:715-718`, `794-802` | `Tab` is content-sized, `flex_shrink_0`, no `min_w` | **accurate**; the load-bearing line is `:800 .flex_shrink_0()`, inside both the doc's `794-802` and the in-code comment's `794-800`. "No `min_w`" confirmed: the only min-width call in the file is `:735 .min_w_0()` |
| `component/src/dock/tab_panel.rs:462-466` | the strip scrolls into view only when the active tab changes | **accurate** (`if self.last_active_ix.replace(Some(active_ix)) != Some(active_ix) { … scroll_to_item … }`, the only `scroll_to_item` in the file) |
| `component/src/tab/tab_bar.rs:521-552` (and `499-519`) | the kit's own overflow list exists, the dock never enables it, it would label every dock tab `Dock.Unnamed` | **accurate, all three parts**: `menu: bool` defaults false (`:52`,`:74`), `tab_panel.rs:468` builds `TabBar::new("tab-bar")` and never calls `.menu(…)`, and because `tab_panel.rs:495-501` uses `.child(tab_name)` rather than `.label(…)`/`.icon(…)`, the `else` at `:535` (`t!("Dock.Unnamed")`) fires for every dock tab |
| `base/src/dock/tab_group.rs:232` | `TabGroup::select_tab(ix, window, cx)` | **accurate** (signature exactly there) |

Two further things the kit read turned up, both in the change's favour:

- `ContextMenuExt::context_menu` derives its element id from `self.interactivity().element_id` and
  falls back to `ElementId::CodeLocation(caller)` when there is none
  (`component/src/menu/context_menu.rs:13-39`). The per-panel `title_row_id`
  (`tab_title.rs:150,153`) is therefore **required**, not cosmetic — without it every tab in the
  strip would share one menu state. The comment at `tab_title.rs:147-149` states this correctly.
- `component::Tab` does implement `InteractiveElement`, so a handler *could* have been hung on the
  kit's `Tab` — but `gpui_base::Tab` keeps its id in its own field and applies it only at render
  (`base/src/tabs.rs:24,41,155`), so the menu would have landed on the `CodeLocation` fallback
  anyway. Putting the menu on OneTerm's own row is the only correct placement.

### Re-entrancy fix

`Panel::dropdown_menu` hands `&mut self` (`terminal_panel.rs:669-676`), so the panel entity is
leased. Every path reachable from inside `tab_list_menu` was traced:

- `tabs_of(panel, panel_id, cx)` (`tab_title.rs:297-319`) reads the **`TabGroup`** entity and the
  sibling `TerminalPanel` entities, never `panel`'s own entity — correct.
- the active row's own label uses the borrowed `panel.tab_label(cx)` (`tab_title.rs:439`), and
  `tab_label` (`terminal_panel.rs:425-431`) only reads the `TerminalView`/`TerminalSession`/settings
  entities — no self-read. Correct.
- `sibling_tabs` (`tab_title.rs:288-294`) does `panel.read(cx)` and is only ever called from
  `close_scope_row` (`tab_title.rs:361`), whose `cx` is a `Context<PopupMenu>` — a different entity,
  so no lease conflict.
- no other `dropdown_menu`/`title`/`title_suffix` path reads the panel entity back.

I found **no remaining double-lease path**. The split into `tabs_of`/`sibling_tabs` is the right
shape and the doc comments at `:295-296` and `:433-435` say why.

### Semantics

| case | behaviour, re-derived | verdict |
| --- | --- | --- |
| right-clicked tab ≠ active tab | every row closes over the right-clicked `Entity<TerminalPanel>` (`tab_title.rs:377-425`), never over `active_ix` | correct; `US-0116-58-tab-rename-dialog.png` is real proof: the dialog is prefilled `Command Prompt` while `PowerShell 7` is the active tab |
| one tab | `tabs_to_close(1, 0, Others)` is empty → `.disabled(true)` (`tab_title.rs:362-367`); `ToTheRight` likewise on the last tab | correct, unit-tested |
| bulk close of N > 1 | one alert, danger OK, "Close N terminal tabs? Their sessions end." (`tab_title.rs:327-343`) | correct |
| bulk close of exactly 1 | unconfirmed (`tab_title.rs:329`) | deliberate and documented; consistent with `×`/middle-click |
| SSH tabs in the victim set | no special case: an SSH tab is closed exactly as `×` closes it | consistent with existing behaviour; there is no "unsaved SSH tab" concept in the code to honour |
| ordering | victims are captured as **entities** at menu-build time and closed right-to-left; `close_tab` defers the `remove_panel` (`spaces.rs:57-76`), and each victim still sees `panels().len() > 1` when it runs, so the "reset the last tab in place" branch is never taken mid-bulk | correct |
| right-click no longer activates the tab | acceptable: every row carries the right-clicked identity, so activation would buy nothing. It is recorded in Gaps as a deliberate change from `F12`'s observed behaviour | acceptable |
| 220 px ceiling with a long OSC title | the label is `flex_1().min_w_0().text_ellipsis().whitespace_nowrap()` (`tab_title.rs:116`) inside the capped row (`tab_title.rs:164-165`), so it ellipses; it does not clip to a bare `×` | correct by construction; still uncaptured, as Gaps says |
| doubled separator | `tab_list_menu` ends with `.separator()` (`tab_title.rs:456`) and the kit adds its own at `tab_panel.rs:337` | **not a defect**: a pixel scan of `US-0116-20-tabbar-more-menu.png` between the last tab row and "Zoom In" finds exactly one rule, at y=347 |

### Findings

- **minor (F-116.1) — the evidence overcounts the tabs, twice; it is the only place a frame
  contradicts the text.** The packet says
  *"the `...` menu: 'Terminal Tabs' and **ten rows**"* and *"`US-0116-19-many-tabs.png` — **ten tabs**
  at 1400 px: the leftmost visible tab reads **'PowerShell 7'** in full"*. The frames show **nine**:
  `US-0116-20-tabbar-more-menu.png` has nine rows (y≈105…329, the ninth checked), and
  `US-0116-51-large-1900.png` — the widened frame that can show every tab at once — has nine tabs.
  In `US-0116-19-many-tabs.png` the leftmost visible tab reads **"PowerShell"**, not "PowerShell 7".
  The "ten" is inherited from the before scene, which genuinely had ten
  (`research/before/51-large-1900.png`: nine "Terminal" tabs plus `dev@127.0.0.1:22…`) — the after
  walk could open no SSH tab, so it has nine. The *substance* is unaffected: nine tabs still exceed
  the 1400 px strip, the tab list still reaches
  a hidden tab (`US-0116-20b` shows tab 1 activated and the strip scrolled back to it), and no tab is
  a bare `×`. Fix the two sentences.
- **minor (F-116.2)** — the in-code comment at `tab_title.rs:421` cites `tab_panel.rs:334-336`; the
  range that carries the full sense (rows *above the kit's separator*) is `333-337`, as the packet's
  own table has it. The comment does not overclaim, so this is cosmetic.
- **minor (F-116.3)** — the in-code comment at `tab_title.rs:157` cites `tab.rs:715-718,794-800`
  while the packet cites `794-802`. Both contain the load-bearing `:800 .flex_shrink_0()`; the
  in-code range is the tighter and better one. Harmless divergence between two records of the same
  fact.
- **minor (F-116.4)** — `close_tabs`' doc comment (`tab_title.rs:315-318`) says the confirmation
  fires when *"more than one **running shell**"* would go, but the code counts **tabs**
  (`tab_title.rs:327-330`); a tab whose shell already exited is counted the same. The user-facing
  string ("Close N terminal tabs?") is the honest one; the comment is not.
- Accessibility: every added row is a plain `PopupMenuItem::new` (`tab_title.rs:333-425`, `:398`), so
  `US-0110`'s `ElementItem`-has-no-`aria_label` finding is genuinely not re-hit. Claim confirmed.
- The "no overflow chevrons" limit is correctly recorded as upstream and correctly **not** ticked;
  the owed upstream issue is still owed.

## US-0117 — the active Space is unmistakable

### Claims checked

| claim | verdict | where |
| --- | --- | --- |
| 2 px absolutely-positioned overlay ring, no hitbox, no id | **true, and verified in gpui's source** | `crates/terminal-view/src/space/render.rs:226-228`, used at `:211`. gpui is `gpui-pre 0.3.3`; `Interactivity::prepaint` inserts a hitbox only when `should_insert_hitbox` holds (`gpui-pre-0.3.3/src/elements/div.rs:2335-2339`), and that predicate (`div.rs:2352-2376`) is a disjunction of listeners / `hover_style` / `mouse_cursor` / `group` / `scroll_offset` / focus / tooltip / non-`Normal` behaviour — **`element_id` is not even in it**. `absolute/inset_0/border_2/border_color` set none of them, so the ring contributes **no** hitbox. And a `Normal` hitbox would not occlude anyway: only `BlockMouse` breaks the hit-test walk (`window.rs:1094-1114`), and it is only reachable through `.occlude()`. Clicks, hover and **scroll** all reach the terminal and the scrollbar beneath |
| measured accent 1 px → 2 px | **true, re-measured by this verifier** | see the table below |
| the content box does not move on focus change | **true** | the ring is absolute and the `p(px(1.))` / `border_1()` are untouched (`render.rs:202-204`); nothing in the focus path changes geometry |
| decision 8 amended, not reversed | **true** | `docs/terminal-split.md:87-101`: the 1 px + 1 px frame and the borderless lone Space are restated, the cue is what changed |
| per-Space `#N` + live session title in the channel-badge row | **true** | `render.rs:134-165`, `236-245`; one `h_flex`, label left of badge, `gap_1` |
| the label cannot hide the badge | **true by construction** | same row, no overlap possible; a Space with no channel simply has no badge |
| no colour literal added | **true** | `space_border_color` for the ring, `muted_foreground` / `background.opacity(0.75)` for the label |

Re-measured independently from the PNGs (`PIL`, row y=400, colours as `rgb`):

| frame | accent run | colour | neutral run beside it |
| --- | --- | --- | --- |
| `research/before/09-split-two-terminals.png` | **1 px**, x=561 | `rgb(82,139,255)` | 3 px `rgb(62,68,81)` at 558-560 |
| `evidence/US-0117-09-split-two-terminals.png` | **2 px**, x=461-462 | `rgb(82,139,255)` | 3 px at 458-460 |
| `evidence/US-0117-09b-split-two-terminals-focus-moved.png` | **2 px**, x=457-458 | `rgb(82,139,255)` | 3 px at 459-461 |

The packet's numbers are exactly right, and the cue is shown moving with focus. Independently
confirmed.

### Findings

- **major (F-117.1) — the corner label permanently covers live terminal output.** In every split,
  each terminal Space now paints a chip up to 160 px wide (`render.rs:155`) with a
  `background.opacity(0.75)` backdrop (`render.rs:146,159`) over the **top-right of the terminal's
  own viewport**. It is visible in the implementer's own captures: in
  `US-0117-09-split-two-terminals.png` and `US-0115-07-split-right-empty-space.png` the first
  prompt line reads `…\claude\D--Tr` with the tail of the line washed out under `#0 cmd.exe`. The
  top row of a running shell is not decoration — it is wherever the output currently is. The
  packet's acceptance *"the terminal grid does not lose a row or a column to the label — the label
  overlays or sits in existing chrome, it does not push content"* is satisfied **literally** (no
  reflow) while failing its intent, and the packet's own "Chrome creep" risk is the one that landed:
  the pre-existing badge this slot was borrowed from is ~16 px and opt-in (a Space must join a
  broadcast channel), whereas the label is ~10× wider and unconditional. This needs an owner call,
  not a silent ship. Cheap options inside the existing design: fade the label out a second after a
  focus change, show it only while the split is being navigated, drop the backdrop to a corner
  badge with just `#N` (the number is the part the cue cannot carry), or move it to the
  bottom-right where a shell prompt rarely is.
- **minor (F-117.2) — what the ring paints over is documented wrong.** `render.rs:216-217` and
  `docs/terminal-split.md:92-94` both say the 2 px ring is painted *"on top of its own border and
  gutter"*. It is not: an absolutely-positioned child is laid out against the **padding box**, so
  `inset_0` starts inside the 1 px border, and `border_2` covers the 1 px gutter plus **1 px of the
  terminal's own content area**. The measurement proves it — the neutral run beside the accent is
  3 px in the before frame and 3 px in both after frames, i.e. the outer border was never
  overpainted. The code comment at `render.rs:210` ("The cue last, so it paints over the terminal's
  own edge pixels") is the accurate one; the other two sentences contradict it.
- **minor (F-117.3) — `#N` is a stable id, not a 0-based index.** Gaps says *"Space numbers are
  0-based (`#0`, `#1`), which is what `SpaceId::display_number` already returns"*.
  `display_number` returns the raw `SpaceId` (`crates/terminal-view/src/space/tree.rs:24-29`), which
  is allocated monotonically and never reused, so after a few split/close cycles a two-Space split
  reads `#0` and `#5`. "0-based" implies a positional numbering the code does not provide. (The
  behaviour matches the existing `Space #N` placeholder, so it is consistent — only the sentence is
  wrong.)
- **minor (F-117.4) — the Space label ignores two things the tab label honours.** It reads
  `session.title()` raw (`render.rs:135-138`), so it disregards `TabTitleMode::Default` (the setting
  that tells OneTerm *not* to use OSC titles — `terminal_panel.rs:444-447`) and disregards a manual
  tab rename. With `tab_title_mode = default` a tab reads "Command Prompt" while its own Space reads
  `#0 cmd.exe`. Routing the label through `tab_label_with_title` would cost one line.
- **light theme (the packet's own outstanding debt).** Analytically the implementer's argument
  holds and can be given numbers. The cue is `table_active_border`, or the kit's fallback
  `background.blend(primary @ 0.6)` when a theme does not define it
  (`reference/gpui-kit/crates/component/src/theme/schema.rs:961-964,1003`). For the two built-in
  light themes that define it, contrast against their own `background` is **3.96:1** (Zed One
  Light, `#526fff` on `#fafafa`) and **2.54:1** (Aurora Light, `#60A5FA` on `#FFFFFF`) — against
  **1.28:1** and **1.23:1** for the inactive `border` those Spaces keep. So the active edge is
  2-3x more separable than the inactive one *and* twice as wide as before. The corner label is
  `muted.foreground` over `background` at 0.75 alpha, and `muted.foreground`-on-`background` is
  exactly what `scripts/check-theme-contrast.py` already enforces at >= 4.5:1 in every theme
  (`ci-local` reports 702 pairings, all passing), so label legibility on light is covered by the
  existing gate rather than by luck. **And it is no longer only reasoning: this verifier took the
  missing frame** — `evidence/US-0117-verify-09-light-split.png`, measured in the GUI section
  below. The gap is closed; `US-0117`'s last unchecked acceptance box can be ticked against it.
- The no-channel-collision claim is sound by construction, but the whole corner row is
  right-anchored with no width cap (`render.rs:147-150`): a 160 px label plus a badge in a Space
  narrower than ~190 px will run past the Space's left edge. Not reachable in a two-way split of a
  1400 px window; noted for the 2×2 case the packet also left unwalked.

## GUI walk — the light-theme frame the packet owed, taken by this verifier

Done with **this verifier's own `fast-dev` build, its own pid and its own scratch working
directory**, so the app's dev config (`target/ui_config.json` is resolved relative to the CWD —
`crates/core/src/config/shell.rs:107-111`) came from a throwaway directory and **no repository file
was touched**. Config used: `{"theme_name": "Zed One Light", "key_bindings": {"split_right": "f2"}}`.
The window was sized to 1400x900 and captured with `PrintWindow(…, PW_RENDERFULLCONTENT)` at the
window's own scale, so one pixel in the file is one pixel on screen. Only that pid's window was
addressed, and only that pid was closed. `target/fast-dev` was deleted afterwards.

Walk: launch -> `F2` (split right) -> right-click the empty Space -> **New Terminal Here** -> capture.

- `evidence/US-0117-verify-09-light-split.png` — **the light-theme split with the ring and both
  labels.** This closes `US-0117`'s single outstanding acceptance item ("the cue is visible in both
  a light and a dark theme"), which the packet honestly recorded as unverified.
- `evidence/US-0115-verify-08-light-empty-space-menu.png` — incidental, but it independently
  reproduces two `US-0115` claims on a second theme and a second machine state: the placeholder
  reads `Right-click -> New…` / `or split, or drag a te…` and the menu it points at opens with
  **New Terminal Here** as its first row.

Measured on my own light-theme capture (`Zed One Light`, y=400):

| what | value | contrast vs the `#fafafa` terminal ground |
| --- | --- | --- |
| active-Space ring, left edge x=461-462 and right edge x=909-910 | **2 px**, `rgb(82,111,255)` = `#526fff` = `table.active.border` | **3.96:1** |
| the inactive Space's border, x=458-460 | 3 px, `rgb(220,223,232)` = `#dcdfe8` = `border` | 1.28:1 |
| corner-label glyphs (`#0 cmd.exe`, `#1 cmd.exe`) | `rgb(93,103,122)` = `#5d677a` = `muted.foreground` | **5.46:1** |

So on light: the cue is a full 2 px ring on all four edges (not one edge), it is ~3x more separable
than the border every inactive Space keeps, and the label is comfortably legible. **The acceptance
item is met; the packet may tick it and cite this frame.** Two things the frame also shows that the
packet's dark captures did not: the ring is a complete ring rather than a single accent edge, and the
`F2` row proves the rebinding route the walk used.

Not walked here either, and still owed by the round: the broadcast-channel collision case, a 2x2
split, "Close Others" and its confirmation dialog, and keyboard navigation into either popup.

## Gaps in this verification

- `reference/gpui-kit/` is not tracked inside this worktree; the kit was read from the shared
  checkout `D:\TrungKFC-Research\Rust\myTerm2\reference\gpui-kit\` (read-only, nothing modified).
  Line numbers therefore refer to that pinned checkout.
- No SSH host is reachable here, so "an SSH tab's label is unchanged" and "Duplicate on an SSH tab
  still goes through its auth dialog" remain code-path arguments in this verification too.
- "Close Others" / "Close to the Right" were exercised only as pure functions plus a source trace of
  the close path; the confirmation dialog is still uncaptured, here as in the packet.
- Keyboard navigation into either popup is still unexercised, for the same posted-message reason.
