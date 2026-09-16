# Independent verification: US-0110 (IN-0033)

Verifier: a second agent, not the implementer.
Worktree: `D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-ab7b0bbe783c17932`
Under review: HEAD reset to `68783649`; `git diff main...HEAD` = `0408f1aa` (records),
`f904b05c` (code), `68783649` (proof). The branch also carries `fef4b866`
`chore(release): v0.6.0`, a `github-actions[bot]` commit that is not the implementer's work
and touches only `Cargo.toml` / `Cargo.lock` version strings.
Date: 2026-09-16.

## Verdict

**PASS-WITH-NOTES.**

The outcome is delivered. Every saved-session row in the "+" dropdown now draws the
session's own 8 px square, the `#56B6C2` default is applied once, in
`crates/session-ui`, next to the constant, and the seam stays primitive-only with no new
crate edge. All six kit facts the packet leans on re-derive correctly from
`reference/gpui-kit`, including the one that matters most — an `ElementItem` row lays out
to the same 26 px as a plain `Item` row, so `ROW_HEIGHT = 28.` was rightly left alone and
the rows do not go uneven next to the local-shell rows. The gate is green in this worktree.

Two things hold the verdict back from a plain PASS. **F1** (major): the rows lost their
accessible name — a plain `Item` row was announced by its label, an `ElementItem` row is
announced by nothing. The packet discloses this under Gaps, but a Gaps bullet in an
accepted packet is not a decision, and this is an accessibility basic regressing, not a
feature that was never built. **F2** (minor, reproduced with a failing test): the menu's
colour fallback chain is *not* equivalent to the tree's for a hex that will not parse, so a
hand-edited `ssh_session.json` can show one session teal in the right dock and theme-accent
in the "+" menu.

The GUI walk in step 7 of my brief was **not** performed: the desktop is locked and I
verified that independently rather than taking the packet's word for it (see Gaps).

## 1. Kit claims re-derived

All line numbers from `reference/gpui-kit/crates/component/src/menu/popup_menu.rs`
(gpui-kit 0.6.0, which is the version the workspace resolves: `gpui-component = "0.6"`,
`Cargo.toml:44`).

| Packet claim | Re-derived | Verdict |
|---|---|---|
| `Item` and `ElementItem` share the base `MenuItemElement` (padding, radius, selected, hover) "~1193-1210" | `let this = MenuItemElement::new(ix, &group_name)` at **1192**, through `.when_some(item.a11y_label(), …)` at **1210**: `.relative().text_sm().py_0().px(INNER_PADDING).rounded(radius).items_center().selected(selected).on_hover(…)`. `INNER_PADDING = px(8.)` at **1182**. | **Correct** |
| Height: `.min_h(item_height)` vs `.h(item_height)` "~1240-1274" | `ElementItem` arm **1229-1256**: no height on the outer element, inner `h_flex().flex_1().min_h(item_height)` at **1242-1244**. `Item` arm **1257-1290**: `.h(item_height)` on the outer element at **1275**. `item_height` = `px(26.)` (`px(20.)` at `Size::Small`) at **1187-1190**. | **Correct** |
| Same click listener "~1234" | `ElementItem` arm: `.when(!disabled, |this| this.on_click(cx.listener(move |this, _, window, cx| this.on_click(ix, window, cx))))` at **1235-1239**; the `Item` arm's copy is at **1269-1273**. Byte-identical. | **Correct** (arm opens at 1234) |
| `is_clickable()` includes non-disabled `ElementItem` (line 236) | `fn is_clickable` **229-244**; `} | PopupMenuItem::ElementItem {` at **236**, `disabled: false` at **237**. | **Correct, exact line** |
| `confirm()` has an `ElementItem` arm (~849) | `Some(PopupMenuItem::ElementItem { handler, action, .. })` at **849**; body **850-857** is the same handler-then-`dismiss(&Cancel)` as the `Item` arm at **838-848**. | **Correct, exact line** |
| `a11y_label()` is `None` for `ElementItem` (line 278) | `PopupMenuItem::Separator | PopupMenuItem::ElementItem { .. } => None,` at **278**. | **Correct, exact line** |

### 1a. Does an `ElementItem` row really come out the same height?

Yes, for this content. Plain `Item`: fixed `.h(px(26.))` on the `MenuItemElement`
(**1275**). `ElementItem`: no height on the `MenuItemElement`, so it sizes to its single
child, which is `min_h(px(26.))` (**1244**). The child content is an 8 px square beside a
`text_sm` label — `text_sm` is set once on the shared base at **1194**, so the label is
14 px with roughly a 17.5 px line box, well under 26. Both rows therefore lay out at 26 px
and `ROW_HEIGHT = 28.` (`terminal_panel.rs:731`, kit item + the 2 px gap) is unchanged and
still correct. `MenuItemElement`'s own default `py_1()`
(`menu/menu_item.rs:102`) is overridden by the base's `.py_0()` at **1195** through
`refine_style`, for both arms alike. The zoomed evidence PNG agrees: the three local-shell
rows and the six session rows are evenly pitched.

### 1b. Does `has_left_icon` shift the alignment differently?

No, and in this menu it is inert. `RenderOptions.has_left_icon` is `any(item.has_left_icon())`
over all items (**1416-1419**); `has_left_icon` returns `icon.is_some() || (left check && checked)`
for both `Item` (**252-254**) and `ElementItem` (**256-258**). No item in
`TerminalPanel::title_suffix` sets an icon or a check, so `has_left_icon` is `false` and
`render_icon` returns `None` (**1126-1128**) for every row — no icon gutter is reserved at
all. Were it ever `true`, both arms call the same `render_icon` (`ElementItem` at
**1247-1253**, `Item` at **1277-1283**) with the same `gap_x_1` before the content
(**1246** vs **1276**), so the label origins would still coincide. This is also why
`PopupMenuItem::new(name).icon(square)` was correctly rejected: it would flip
`has_left_icon` for the whole menu and indent the local-shell labels, and
`render_icon` forces `.xsmall()` (**1138**) = `size_3` = 12 px, so an 8 px square is not
expressible that way.

### 1c. Label text style and truncation

Same. `text_sm` is on the shared base (**1194**) and the custom row sets no font size, so
it inherits exactly what a plain `Item` label inherits. Colour likewise: `MenuItemElement`
paints `cx.theme().foreground` (`menu_item.rs:105`) and swaps to `accent_foreground` on
hover/selected (`menu_item.rs:114-121`); the row's label `div` sets no `text_color`, so it
follows. Note this is deliberately *not* the tree's own
`text_color(cx.theme().foreground)` (`tree_render.rs:122`) — matching the neighbouring menu
rows is the right choice here, and the rendered result is the same token anyway.
`.truncate()` on the label `div` (`terminal_panel.rs:102`) is a
no-op in practice because neither the row's `h_flex` nor the label carries `min_w_0`, so
flexbox will not shrink it below its content; plain `Item` labels do not truncate either,
so no divergence between neighbouring rows.

### 1d. Hover and keyboard selection background

Applies to the custom row. The hover/selected background lives on `MenuItemElement` itself
and is gated only on `!disabled` (`menu_item.rs:111-119`), and the row is not disabled, so
`group_hover` and `selected` both paint `bg(accent)` + `text(accent_foreground)` exactly as
for a plain item. Keyboard reach follows from `is_clickable()` (**236**) feeding the
selection iterator (**816-824**) and `confirm()`'s `ElementItem` arm (**849**). Not driven
in a running app — see Gaps.

## 2. Does the square match the tree?

Geometry: yes, exactly. Menu (`crates/terminal-view/src/panel/terminal_panel.rs:94-104`,
square at 101) and tree (`crates/session-ui/src/tree_render.rs:110-125`, square at 117)
both build
`h_flex().items_center().gap_2()` with `div().w(px(8.)).h(px(8.)).bg(color).flex_shrink_0()`
followed by a truncating label. No colour literal enters `crates/terminal-view` — grep for
`56B6C2|rgb(0x|hsla(` in `terminal_panel.rs` returns nothing, and `DEFAULT_COLOR_HEX` is
referenced only inside `crates/session-ui`.

Fallback chain: **not** equivalent. This is finding **F2**.

- Tree (`tree_render.rs:90-96`):
  `color.and_then(parse_hex).unwrap_or_else(|| parse_hex(DEFAULT_COLOR_HEX).unwrap_or(accent))`
  — *any* parse failure lands on `#56B6C2`. The theme accent is unreachable, because
  `DEFAULT_COLOR_HEX` always parses.
- Menu: `menu_entries` (`tree_builder.rs:68-71`) maps `None` / blank to `DEFAULT_COLOR_HEX`
  but passes every other string through verbatim, and `saved_session_row`
  (`terminal_panel.rs:96`) does `parse_hex(hex).unwrap_or_else(|_| cx.theme().accent)`.

`Colorize::parse_hex` (`reference/gpui-kit/crates/component/src/theme/color.rs:290-309`)
strips a leading `#` and then requires exactly 6 or 8 hex digits. Case by case:

| saved `color` | tree | menu | same? |
|---|---|---|---|
| `None` | `#56B6C2` | `#56B6C2` | yes |
| `""` | `#56B6C2` (parse fails) | `#56B6C2` (blank arm) | yes |
| `"  "` | `#56B6C2` (parse fails) | `#56B6C2` (trim → blank arm) | yes |
| `"#56b6c2"` | parses | parses | yes |
| `"56B6C2"` (no hash) | parses (`trim_start_matches('#')`) | parses | yes |
| `"#56B6C2FF"` (8-digit) | parses | parses | yes |
| `"#abc"` (3-digit) | `#56B6C2` | **theme accent** | **no** |
| `"#GGGGGG"` | `#56B6C2` | **theme accent** | **no** |
| `" #E06C75 "` (padded, valid) | `#56B6C2` (parse fails on the spaces) | **`#E06C75`** (trimmed first) | **no** |

Reachability is low: the only writer is the session dialog
(`session_dialog.rs:309`, `Hsla::to_hex()`), and `to_hex` always emits `#RRGGBB` or
`#RRGGBBAA` (`color.rs:269-288`). So only a hand-edited `ssh_session.json` reaches the
three divergent rows — which is exactly the population the blank-value arm was written for.

### The failing test

Added temporarily to `crates/session-ui/src/tree_builder.rs` tests, run, then reverted (the
worktree is clean; this file is the only change in my commit):

```rust
/// VERIFIER PROBE (US-0110): the tree resolves *any* colour `parse_hex`
/// rejects to `DEFAULT_COLOR_HEX` (`tree_render.rs` line 90-96), not only a
/// missing or blank one. The menu must agree, or the same session shows
/// teal in the right dock and the theme accent in the "+" menu.
#[test]
fn menu_entries_resolves_an_unparsable_colour_like_the_tree_does() {
    let mut bad = entry(1, "bad-hex", None);
    bad.session.color = Some("#GGGGGG".into());
    let mut short = entry(2, "three-digit", None);
    short.session.color = Some("#abc".into());
    let mut padded = entry(3, "padded", None);
    padded.session.color = Some(" #E06C75 ".into());
    assert_eq!(
        menu_entries(&[bad, short, padded]),
        vec![(
            String::new(),
            vec![row(1, "bad-hex"), row(2, "three-digit"), row(3, "padded")]
        )],
        "tree_render.rs falls back to DEFAULT_COLOR_HEX whenever parse_hex fails"
    );
}
```

```
cargo test -p oneterm-session-ui menu_entries_resolves
test tree_builder::tests::menu_entries_resolves_an_unparsable_colour_like_the_tree_does ... FAILED
assertion `left == right` failed: tree_render.rs falls back to DEFAULT_COLOR_HEX whenever parse_hex fails
  left: [("", [(1, "bad-hex", "#GGGGGG"), (2, "three-digit", "#abc"), (3, "padded", "#E06C75")])]
 right: [("", [(1, "bad-hex", "#56B6C2"), (2, "three-digit", "#56B6C2"), (3, "padded", "#56B6C2")])]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 62 filtered out; finished in 0.01s
```

The one-line fix, if the owner wants the surfaces to agree, is to make `menu_entries`
resolve rather than relay — `Some(hex) if Hsla::parse_hex(hex).is_ok() => hex.to_string()`
— after which `saved_session_row`'s `cx.theme().accent` arm becomes dead, matching the tree
where it is already dead. Alternatively make the tree trim and share one helper. Either way
it belongs in `crates/session-ui`; no colour knowledge moves into `crates/terminal-view`.

## 3. Crate rules

- `crates/terminal-view/Cargo.toml` lists `oneterm-core`, `-terminal`, `-settings`,
  `-state`, `-theme`, `-actions`, `-highlight`, `-completion`. No `oneterm-session-ui`.
  R1/R5 hold; the colour crosses the same `WorkspaceCommands` fn-pointer hop as the title.
- `crates/state/src/commands.rs:34` — `SavedSshSessionSections` is still
  `Vec<(String, Vec<(u64, String, String)>)>`: primitives only, no feature type named. R10
  holds. A third tuple element rather than a struct is the right call here for that reason.
- `python scripts/verify-dependency-graph.py` — run as part of the gate, passed.

## 4. Test quality (mutation-tested)

The two new tests are real, and the seven pre-existing ones became *stronger*, not weaker,
because the `row()` helper puts `DEFAULT_COLOR_HEX` into every expectation.

| Mutation | Result |
|---|---|
| **M1** — drop the default: `_ => String::new()` in `menu_entries` | **Caught.** `test result: FAILED. 2 passed; 7 failed` — including `menu_entries_applies_the_default_colour_when_none_is_saved` with its own message, plus all five ordering/label tests via `row()`. |
| **M2** — swap title and colour: `let row = (entry.id.raw(), color, title);` | **Caught.** `8 failed`, including `menu_entries_carries_the_saved_colour`. |
| **M3** — every row in a section takes the first row's colour | **SURVIVED.** `test result: ok. 9 passed; 0 failed`. |

**M3 is a blind spot (finding F4).** `menu_entries_carries_the_saved_colour` puts its two
differently-coloured sessions in *different* sections (one ungrouped, one in `infra`), and
every other test uses the default everywhere, so no test pins the colour to the right row
*within* a section — the exact mis-pairing a `for` loop over rows is most likely to produce.
One extra session in the existing test's `infra` group with a third colour closes it.

All three mutations were reverted; `git status --short` is empty apart from this document.

## 5. Docs

- `docs/gui-layout.md:65` — accurate line by line against the code: the 8 px square, the
  `#56B6C2` default, "the colour reaches the menu as a hex string in the same
  `WorkspaceCommands` row tuple", and "a hex that will not parse falls back to the theme
  accent". The only overclaim is the same one as F2 — "the same square the right dock's
  tree draws" is true for every colour the app itself can write, and false for the three
  hand-edited cases above.
- `high-level-design.md` — wireframe, the new "Rows carry the session's colour square"
  decision, the re-worded "No colour or icon literals **in the menu builder**" decision, and
  the widened row tuple in step 3 of the data flow. All match the code. The kit claim in the
  new decision ("same padding, height, hover and selection styling as a plain `Item`") is
  the one I re-derived in §1 and it is correct.
- `IN-0033.md` — `US-0110` added to Candidate Work Packets.
- `docs/ssh-client-connect.md` — the packet's no-change reason holds. Lines 65-69 and row 8
  of the decision table describe *which surfaces* enter the connect flow and that the `+`
  menu reuses `open_connect_dialog` by session id. Neither describes how a row is drawn, and
  neither the flow, the id nor the dialog changed.
- `docs/terminal-split.md` — the only match for "New Terminal" is line 90, the empty-Space
  "New Terminal Here" menu, unrelated. Not stale.
- `US-0094-...md:277` still says "title only", but that is a shipped packet recording what
  was accepted then, not a living contract; the living docs are updated. No action.
- Evidence honesty: **good.** The packet states plainly that the desktop was locked, that
  `CopyFromScreen` returned the lock screen, that the frames came from
  `PrintWindow(..., PW_RENDERFULLCONTENT)` and the clicks from posted `WM_*` messages
  against the one pid it launched, and that keyboard navigation was *not* driven. There is
  no `scripts/gui.ps1` in the repository and the packet does not claim one. The four PNGs
  exist and show what they say they show; the zoomed one is legible enough to read the two
  default-teal rows (`no-colour-saved`, `sandbox`) against the four tagged ones.

## 6. Findings

### F1 — major — the rows lost their accessible name

Before this change a row was `PopupMenuItem::new(name)`, whose `a11y_label()` returns
`Some(label)` (`popup_menu.rs:274-276`) and reaches the element as `.aria_label(label)`
(**1210**). `PopupMenuItem::element` is an `ElementItem`, whose `a11y_label()` is `None`
(**278**), so every saved-session row now renders as `Role::MenuItem` with no accessible
name. This is a regression in an existing row, not a gap in something new, and screen-reader
users lose the session name on the surface the owner asked to make *more* legible.

There is no in-repo fix: `gpui-component` comes from crates.io (`Cargo.toml:44`, no
`[patch]` section), the `a11y_label()` match is private, and `.icon()` cannot draw an 8 px
square (§1b). So the resolution is a call, not a patch: accept it in a `DEC`, or open a
follow-up to carry an `aria_label` on `ElementItem` upstream. What should not happen is that
it stays a bullet under "Gaps" in an accepted packet, which is where regressions go to die.
The packet's own note that "the same already applies to the two labelled separators" does
not carry: those are `disabled`, deliberately outside hover and keyboard navigation, so they
were never announced as items.

### F2 — minor — the colour fallback chain is not the tree's

Detailed in §2 with a failing test. Hand-edit-only reachability, which is why it is minor;
but the packet's Acceptance says "the same default square the tree shows" and the code
comment says "the theme accent is only the last resort for a hex that will not parse" —
in the tree, that last resort is `#56B6C2`, not the accent. One of the two should move.

### F3 — minor — the Verification Plan overstates what `-p oneterm-terminal-view` proves

Plan step 2 reads "`cargo test -p oneterm-terminal-view` — the panel's test double builds
the new tuple." It does not: the double at `crates/terminal-view/src/panel/tests.rs:454-456`
returns `Vec::new()` (`fn saved_sessions` at line 454), and no test in the crate exercises
`title_suffix` at all. The 341
passing tests are a compile check on the widened type and nothing more. Worth one honest
sentence rather than an implied coverage claim; the packet's own Gaps section already says
the right thing ("the square's colour is not asserted by an automated test at the render
layer"), so this is just the Plan line contradicting it.

### F4 — minor — no test pins the colour to the right row inside one section

See M3 in §4. One more session in `menu_entries_carries_the_saved_colour`'s `infra` group,
with a third colour, closes it.

### F5 — minor — `chore(release): v0.6.0` rides along on the branch

`fef4b866` is a `github-actions[bot]` commit that bumps every crate 0.5.2 → 0.6.0. It is not
the implementer's and is presumably just where the branch was cut from, but it means
`git diff main...HEAD` is not purely this packet. Worth knowing when the merge is reviewed.

## 7. Commands run

From the worktree root, `$env:CARGO_BUILD_JOBS = 6`.

```
pwsh scripts/ci-local.ps1
...
==> python scripts/third-party-notices.py --check
ci-local: all checks passed.
```

```
cargo test -p oneterm-session-ui
test result: ok. 62 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```

```
cargo test -p oneterm-terminal-view
test result: ok. 341 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.42s
```

Mutation runs (each reverted immediately afterwards):

```
M1  test result: FAILED. 2 passed; 7 failed; 0 ignored; 0 measured; 53 filtered out
M2  test result: FAILED. 1 passed; 8 failed; 0 ignored; 0 measured; 53 filtered out
M3  test result: ok.     9 passed; 0 failed; 0 ignored; 0 measured; 53 filtered out   <- survived
probe (F2)  test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 62 filtered out
```

Desktop-state check, before deciding on the GUI walk:

```
Get-Process -Name LogonUI            -> LogonUI running -> lock/logon screen present
Graphics.CopyFromScreen(200,200,400x300) -> distinct colors sampled: 1
```

## 8. Gaps in this verification

- **No GUI walk.** The desktop is locked — independently confirmed above, not taken from
  the packet — so `cargo run -p oneterm-app` was not launched and neither (a) the visual
  alignment of the saved-session rows against the local-shell rows nor (b) Down-arrow
  navigation and Enter on a saved row was driven in a running app. Keyboard reach therefore
  still rests on the kit source read in §1d, exactly as the packet says. No PNG was
  captured, and no `US-0110-verify-*.png` exists. This is the one acceptance bullet
  ("keyboard navigation still reaches the row") that neither the implementer nor I have
  exercised end to end; it should be confirmed by eye on an unlocked desktop before the
  packet is accepted.
- The implementer's four PNGs were inspected but not reproduced — I cannot rule out that
  `PrintWindow` renders the popup differently from the composited screen, though the popup
  in the capture is a child of the window it targeted, so the risk is small.
- Row height equality (§1a) is derived from the kit source and corroborated by the zoomed
  PNG, not measured in a layout dump.
- `Colorize::parse_hex` slices `&hex[0..2]` after only a byte-length check, so a hand-edited
  6- or 8-*byte* multibyte colour (e.g. `"a<3-byte char>bc"`) panics. This is a kit bug
  reachable identically from the tree and the menu, pre-dates this packet, and is out of its
  scope — noted only so it is not rediscovered as a US-0110 regression.
