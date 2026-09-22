# High-Level Design: Semantic highlighting phase 2

Intake: IN-0044
Lane: normal
Date: 2026-09-22

## Idea

`docs/terminal-semantic-highlighting.md` calls the OSC 133 fast path "the OneTerm
advantage" (§4.2): a shell that emits shell-integration marks tells the terminal exactly
which rows are prompt, which are the command the user typed and which are its output, so
no regex has to guess. That path has never run. `RowRoles` exists
(`crates/highlight/src/role.rs`), `SemanticOverlay` holds one
(`crates/terminal-view/src/highlight/overlay.rs:18`), and nothing ever assigns it, so
`scan_into` takes the `role.is_empty()` branch on every line and every row is scanned as
`RowRole::Output` with the prompt regex deciding.

The reason it was never wired is written into the contract: §4.2 and §13 Q1 say `RowRoles`
is "rebuilt in the pump ... alongside the grid snapshot" by tracking the OSC 133 *event*
stream and attributing rows to regions as they are appended. That was the right shape for
the engine of the day. It is not the right shape for the engine `IN-0029` shipped, which
puts the region **on the cell**:

```rust
// crates/vt/src/cell.rs:83 — two bits per cell, set from the cell template
pub enum Semantic { None, Prompt, Input, Output }
// crates/vt/src/terminal/dispatch.rs:1014 — OSC 133 A/B/C write the template
// crates/vt/src/snapshot/row.rs:52 — and the snapshot carries it out verbatim
pub struct SnapshotCell { /* ... */ pub semantic: Semantic, /* ... */ }
```

So the fact the view needs is already in the frame it is already reading. It travels with
its content through scroll, through scrollback, through trimming and through reflow,
because it *is* the content — no anchor to maintain, no event to replay, no row id to
reconcile. The whole of `US-0133` is therefore a read, not a plumbing job: derive the
per-row role from the cells the frame already holds.

That unlocks the rest. `US-0134` paints the prompt-line background under the rows the
roles name. `US-0135` measures what the scan costs and settles two bounds §10 currently
states as arguments. `BUG-0073` tightens the regex that, from now on, only runs when the
shell said nothing.

## Diagram

```text
  shell                     oneterm-vt                         oneterm-terminal-view
  ─────                     ──────────                         ─────────────────────
  OSC 133;A ──┐
  OSC 133;B ──┼──► dispatch::shell_mark                        ┌───────────────────┐
  OSC 133;C ──┘      cell template .with_semantic(..)          │  PlanCache::update│
                         │                                    │   phase 2 rescan  │
                         ▼                                    │                   │
                     printed cells carry Semantic             │  fill_wraps       │
                         │        (2 bits, moves with         │  row_roles_into ◄─┼─ NEW (US-0133)
                         │         the content through        │  class_rows_into  │
                         │         scroll / trim / reflow)    │  url_masks_rows.. │
                         ▼                                    └────────┬──────────┘
                     SnapshotRow.cells[].semantic ─────────────────────┘
                                                                       │
  OSC 133;D;<code> ─► VtEvent::ShellMark(OutputEnd{exit_code})          ▼
                         │                               SemanticOverlay { row_roles }
                         ▼                                             │
              backend/state.rs  last_exit_code                         ▼
                         └──────────────────────────────►  scan_line_into(line, role)
                              (the one fact NOT in a cell)             │
                                                                       ▼
                                                            build_row_plan  (US-0134:
                                                            band first, then cell bg)
```

## UI Wireframe

The surface is the terminal grid itself. `US-0133` changes which colour a glyph takes;
`US-0134` adds one visible element, a background band behind the prompt line. No control,
menu, dialog or setting is added.

```text
Before (today) — a prompt whose cwd wraps, no band, roles guessed by regex
+------------------------------------------------------------+
| C:\Users\John Doe\projects\oneterm\crates\terminal-view\src |   <- Path (regex)
| \render> cargo test -p oneterm-highlight                    |   <- sign + Command + Option
| running 71 tests                                            |
| test result: ok. 71 passed; 0 failed                        |
| C:\Users\John Doe\projects\oneterm> _                       |
+------------------------------------------------------------+

After — roles from OSC 133, band under every row of the prompt line, sign tinted by exit code
+------------------------------------------------------------+
|############################################################|   <- prompt_line_bg,
|#C:\Users\John Doe\projects\oneterm\crates\terminal-view\src#|      the full row width,
|#\render> cargo test -p oneterm-highlight                   #|      every row of the run
|############################################################|
| running 71 tests                                            |
| test result: ok. 71 passed; 0 failed                        |
|############################################################|
|#C:\Users\John Doe\projects\oneterm> _                      #|   <- `>` tinted Success
|############################################################|      (exit code 0)
+------------------------------------------------------------+
     ^                    ^
     |                    the band spans column 0..cols on every row of the logical
     |                    line, so a LEADING_WIDE_CHAR_SPACER at the wrap boundary
     |                    is covered rather than left as a one-cell hole (Q4)
     the band is one rect under the row, painted before any per-cell background
```

A failing command, for the tint's other sign:

```text
|#C:\Users\John Doe\projects\oneterm> cargo buidl             #|
|                                   ^ `>` tinted Error (exit code 101)
```

## Data Flow

1. **The engine marks cells.** `OSC 133;A/B/C` reach
   `crates/vt/src/terminal/dispatch.rs:1014`, which puts `Semantic::Prompt`, `Input` or
   `Output` on the cell template. Every glyph printed afterwards carries that value in two
   of its bits until the next mark changes the template. `OSC 133;D` clears the template
   back to `Semantic::None` and carries the exit code, which no cell records.
2. **The snapshot carries it out.** `SnapshotRow::copy_from`
   (`crates/vt/src/snapshot/row.rs:105`) copies `cell.semantic()` into
   `SnapshotCell::semantic` with everything else the row resolves under the lock. The view
   already consumes this row; the field is simply not read yet
   (`crates/terminal-view/src/render/frame.rs` builds its `Cell` from it on demand).
3. **The view derives a role per display row (`US-0133`).** A new `row_roles_into` runs in
   the plan cache's phase-2 rescan, beside `fill_wraps`, `url_masks_rows_into` and
   `class_rows_into` (`crates/terminal-view/src/render/plan_cache.rs:198-240`). For each
   row in the rescan range it reduces the row's cells to one role:

   | Cells of the row | Role |
   | --- | --- |
   | any `Semantic::Prompt` | `RowRole::Prompt` |
   | otherwise, any `Semantic::Input` | `RowRole::Command` |
   | otherwise, any `Semantic::Output` | `RowRole::Output` |
   | all `Semantic::None` | no mark — the row falls back to the prompt regex |

   `Prompt` wins over `Input` on a row that holds both, because that row *starts* the
   prompt line and the scanner needs to enter `PromptLine` state to find the sign; the
   command portion after the sign is reached by the scanner's own state machine (§4.1), not
   by the row's role. The four-way table is deliberately a reduction and not a per-column
   map: §13 Q1 already decided roles are row-level and the sign is found by an in-row probe,
   and nothing here reopens that.
4. **A wrapped prompt needs no special case, and gets one anyway.** The role that matters is
   the role of the row that *starts* the logical line, because that is what
   `SemanticOverlay::scan_into` is handed (`BUG-0071` fixed the parameter to be the run's
   first row). A continuation row of a wrapped prompt carries `Semantic::Prompt` on its own
   cells too, so the derivation agrees with itself — but the scanner never asks it. The
   special case is the opposite one: a continuation row whose own cells are all
   `Semantic::None` (the shell printed the prompt before the mark, or the run straddles a
   mark) must not drag the run to `Output`. It cannot, because only the first row is
   consulted. This is worth a test rather than a mechanism.
5. **"Absent" becomes per row, not per terminal (`US-0133`).** Today `scan_into` asks
   `row_roles.role.is_empty()` — one question for the whole overlay. That is wrong once the
   roles are real: a session can start under a shell with no integration, have it enabled
   mid-session, or print output from a program that emits no marks between two marked
   prompts. The absent case is therefore per row: `RowRoles` gains a way to say *this row
   has no mark* (an `Option<RowRole>` read, or a `RowRole` plus a parallel "marked" bit —
   the packet picks; the cheaper of the two is one more `Box<[bool]>` beside the existing
   two). When a row has no mark, `scan_into` passes `RowRole::Output` exactly as today and
   the prompt regex fallback decides, unchanged. When it has one, the regex does not run at
   all. That is the whole of "the regex becomes the fallback only when no mark exists for
   the row".
6. **The exit-code tint (`US-0133`).** `OSC 133;D` is an event with no row attached
   (`crates/vt/src/events/vt_event.rs:62`), and the router already records it as
   `last_exit_code` (`crates/terminal/src/backend/osc_router.rs:236`,
   `crates/terminal/src/backend/state.rs:59`). The engine offers no public way to ask
   *which row* a block ended on: `state.marks` is `pub(crate)` and `Terminal` has no anchor
   accessor. The design therefore takes the cheap answer and writes down its limit:

   > The tint applies to the **most recent completed command block** — the newest run of
   > `Semantic::Input` rows whose following `Output` run has ended — and reads
   > `last_exit_code`. A prompt further back in scrollback keeps an untinted sign.

   That is one comparison per frame against state the view can already reach, it covers the
   case a user actually looks at (the command that just finished, at the bottom of the
   screen), and it adds nothing to the engine's public API. The upgrade path, if anyone
   ever wants tinted history, is a bounded `VecDeque<(RowId, Option<i32>)>` filled by the
   router — which first needs the engine to report the row a block ended on, and that is a
   public API change with a `scripts/vt-public-api.py` snapshot behind it. Out of scope
   here, and named in the intake's Open Decisions.

   The tint itself is the design's existing rule (§4.2, §13 Q1): the prompt sign's
   `Class::PromptSign` becomes `Class::Success` on exit code `0` and `Class::Error`
   otherwise. It is a class substitution at the sign's column, not a new class and not a new
   theme entry.
7. **The prompt-line background (`US-0134`).** `ClassStyles::prompt_line_bg` is parsed
   (`crates/terminal-view/src/highlight/bridge.rs:51`) and read by nothing. It becomes one
   `BgSpan` covering `0..cols`, pushed into the row plan **before** the per-cell loop in
   `build_row_plan` (`crates/terminal-view/src/render/row_plan.rs:575`, the function §8
   item 6 calls `layout_row`):

   ```rust
   plan.clear();
   classify(&row, classes, url_mask, scratch);
   if ctx.prompt_line_bg(role_of_this_row) { push_bg_span(plan, 0, cols, band); }  // NEW
   for (col, cell) in row.cells().enumerate() { /* ... push_bg(…) as today … */ }
   ```

   Order matters and is the whole of the mechanism: `RowPlan::bg` is painted in push order,
   so the band goes down first and every explicit cell background — a selection, an ANSI
   `bg`, an inverse cell — paints on top of it exactly as it does on any other row. Nothing
   in `resolve_style` changes, and `paint_bg` stays `inverse || bg_color != Color::Background`,
   so a default-background cell on a prompt row still emits no span of its own and the band
   shows through. Painting it as one row-wide rect rather than per cell also closes the
   `LEADING_WIDE_CHAR_SPACER` hole §13 Q4 and `BUG-0071` both flagged as the thing that would
   bite when this item was implemented: the spacer has no class, but it is inside `0..cols`.

   Which rows get it: every row whose role is `Prompt` or `Command`, which for a wrapped
   prompt is every row of the run, because each carries the mark on its own cells (step 4).
   Under the regex fallback the band is painted on the rows the fallback calls prompt rows,
   which is the same set the sign colouring already uses — the packet decides whether to
   ship the band in the fallback case at all, and the safe default is not to: a band that
   appears and disappears as a regex changes its mind is the flicker `BUG-0071` was reported
   for, while a wrong *foreground* on one word is not.
8. **Where the band's colour comes from (`US-0134`).** Today it is one fixed hex in
   `crates/terminal-view/assets/highlight/default.json`, applied to every theme, and
   `TerminalTheme::class_styles` is a `&'static ClassStyles` pointing straight at it
   (`crates/terminal-view/src/theme/terminal_theme.rs:49,105`). On a light theme that is a
   dark band under dark text. The band must therefore be **theme-relative**: resolved in
   `build_terminal_theme()` from the theme's own terminal background, nudged toward the
   terminal foreground by a small fixed alpha, with the asset value (and any future
   per-theme `terminal.semantic.promptLineBg`) kept as an explicit override. A band defined
   as "the background, slightly toward the text" cannot invert a theme, and it keeps §7's
   rule that a theme which says nothing still gets something sane.

   **The contrast gate.** `scripts/check-theme-contrast.py` is the repository's readability
   contract, and the rule `AGENTS.md` states is that text drawn on a surface the `SURFACES`
   table does not list is simply not checked — so a new surface must be added. Two facts
   make that not directly applicable here, and both must be in the packet rather than
   discovered during it:

   - The script measures **kit UI tokens** (`foreground`, `muted.foreground`,
     `popover.foreground`, ...) read out of `crates/theme/themes/*.json`. Terminal grid text
     is none of those. It is an ANSI palette entry or a semantic `Class` foreground, and the
     semantic palette is not in a theme file at all — it is in
     `crates/terminal-view/assets/highlight/default.json`, which the script never opens.
   - Adding `prompt_line_bg` to `SURFACES` therefore only means something once there is a
     *checked foreground token* drawn on it. There is not one today.

   The proposal is to keep the gate's scope as it is and prove the band in Rust instead: a
   test in `crates/terminal-view` that resolves the band for every embedded theme and asserts
   the floor of 4.5:1 for the terminal foreground and for each semantic class foreground
   drawn on a prompt row (`PromptSign`, `Command`, `Option`, `Path`, plus `Success`/`Error`
   for the tint) against the resolved band. That is the same arithmetic the Python script
   uses, applied where the colours actually live, and it runs inside `cargo test --workspace`
   with no change to what `check-theme-contrast.py` is for. Extending `SURFACES` to terminal
   tokens is the alternative; it is a larger change to that gate's scope and is recorded as
   an owner question in the intake, not assumed here.
9. **The benchmark (`US-0135`).** The repository has no `criterion` and no `[[bench]]`
   anywhere; its bench style is a plain binary in `crates/tools` that measures, prints a
   table, writes JSON and is compared against a committed baseline by hand
   (`crates/tools/src/bin/vt-bench.rs`, `crates/tools/bench-baseline.json`). The
   highlighter benchmark follows it exactly, including the rule that binary states in its
   own header: **recorded, never gated in CI**, except for one `--check` trip-wire that
   fires only at a large multiple of the baseline and is run by hand on the machine the
   baseline came from.

   Shape: one logical line of `N` characters scanned through
   `oneterm_highlight::scan_line_into`, over the grid of `N` and wrap width the §10 table
   claims to bound —

   | Axis | Values | Why |
   | --- | --- | --- |
   | `N` (logical line length) | 80, 200, 2 000, 8 000 | 200 is §10's per-line unit; 8 000 is the 40x200 worst case §10 already quotes at 4.14 ms |
   | wrap width (columns) | 80, 120, 200 | `N / width` is the number of visual rows one scan now covers; it is what turned N scans into one |
   | content | prompt line, plain output, keyword-dense log, a line with CJK | the four shapes with different costs: the prompt regex, the Aho-Corasick pass, the structural regexes, and the byte-to-char map `BUG-0071` `F3` fixed |
   | profile | `Unix`, `Cmd`, `PowerShell` | the prompt regex differs per profile and is a per-line fixed cost |

   `crates/tools` may only reach down to leaf crates
   (`docs/agents/crate-dependency-rules.md`), and `oneterm-highlight` is one, so the binary
   adds one dependency and no layering exception. It deliberately does **not** measure
   `class_rows_into` in `crates/terminal-view`, which is a GPUI crate the tools layer may not
   reach: the view-side half of the cost is the join and the scatter, both linear memcpy over
   the same characters, and it is already observable per frame through
   `FrameStats::class_scans` / `class_rows_scanned` (`BUG-0071` `F10`). The packet asserts the
   call and row counts there with a test, and measures the scanner here with a number.
10. **The two bounds `US-0135` settles.** Both are §10 claims that are currently arguments.

    - **The wrap-run bound.** §10 already says, honestly, that the rescan scope is the wrap
      run and that a logical line longer than the viewport makes that run the whole viewport.
      The question is whether that is acceptable or needs a cap. It is a measurement: if a
      full-viewport scan lands well inside a frame in `release` and in `fast-dev`, the answer
      is to keep the bound and delete the worry; if it does not, the answer is a cap on the
      joined line length, with the tail of an over-long line scanned as its own unit and the
      colour error that introduces written down. The packet measures first.
    - **A run whose head is above the viewport.** The view holds the viewport and nothing
      else: `SnapshotState::rows()` (`crates/vt/src/snapshot/state.rs:296`) is the visible
      rows, and there is no public way to read the row above the top without a new engine
      read path. So "scan from the run's true head up to a cap" is not a small change — it is
      a new engine API, a new cache dependency on rows the cache does not hold, and a second
      invalidation edge. The alternative is to keep the viewport-only contract §10 and §13 Q5
      already state, and document the limit next to the identical one the URL pass has carried
      since `US-0092`. The packet measures the first option's cost honestly before choosing,
      and if it chooses the limit, §10 says so in one sentence instead of leaving it in a
      packet's Gaps list.
11. **The Windows prompt false positive (`BUG-0073`).** Independent of everything above,
    because it is in the fallback the marked case never reaches. `WIN_PATH_BODY`
    (`crates/highlight/src/profile.rs:111`) admits any character but `< > | " * ? CR LF` and
    forbids only a *trailing space*, so any drive- or UNC-anchored line whose first `>` is
    not preceded by a space is a prompt. The fix tightens the rule at the sign, not at the
    body: the `>` must be the last non-space character of the **logical** line, or be
    followed by a space and then a command word. `C:\src -> C:\dst` fails both (the `>` is
    followed by a space and then `C:\dst`, which is a path, not a command — and the packet
    decides whether "a command word" is worth distinguishing from "any token", the cheaper
    rule being that a `-` immediately before the `>` rejects the line outright).
    `c:\proj\x.cpp(5): error C2059: syntax error: '>'` fails because its `>` is inside quotes
    and is not followed by a space. `C:\work>dir > out.txt` and the wrapped cwd cases
    `BUG-0071` `F1`/`F2` added must all still pass, and `N1`'s rule — the bare `>` branch
    stays out of `UNIVERSAL_PROMPT` — must not be disturbed. "Logical line" is load-bearing:
    the check runs on the joined wrap run, which is what the scanner already receives.

## Detail Design

- [ ] Detail design: not needed
- Reason: normal lane, no high-risk trigger (no auth, no data loss, no migration, no
  external effect, no public API change). Every seam this intake touches is already
  described: the scan pass and its cache by
  `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/render-pipeline.md`,
  the class model and the fast path by `docs/terminal-semantic-highlighting.md` §4.2, §8
  and §13 Q1/Q4/Q5. The four work packets each carry their own plan; a fifth document
  between this one and them would repeat both.
