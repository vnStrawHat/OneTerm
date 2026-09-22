# Independent verification — US-0133 (row roles from OSC 133) and US-0134 (prompt-line background)

Verifier: independent agent, adversarial pass.
Under test: `feat/semantic-row-roles` @ `0dfd2070`, two commits on `main` @ `7f4dfe30`.
Worktree: `.claude/worktrees/agent-a729b0fdcc715ea82`, reset to `0dfd2070`, no source change
left behind (every probe and mutation below was reverted; `git status` clean before the
commit that carries this file).

## Verdicts

| Packet | Verdict |
|---|---|
| `US-0133` — row roles come from the shell's OSC 133 marks | **REJECT** — one blocker (MAJ-1) plus a headline acceptance item that cannot fire in any shipped configuration (MAJ-2). The mechanism is correct for the shells the packet tested; it is wrong for the shells the packet did not. |
| `US-0134` — the prompt line gets its background | **ACCEPT WITH CHANGES** — the band itself is correct and well tested, but it inherits MAJ-1 (it is painted on every output row of an A-only session) and its own central design choice is unasserted (MAJ-3). |

Both packets' code is otherwise as described: every structural claim in the brief was
checked against the source and holds. The defects are in the *premises*, not in the
implementation of those premises.

---

## Claims, checked

### US-0133

| Claim | Verdict | Where |
|---|---|---|
| Roles derived per logical line inside `class_rows_into` | HOLDS | `crates/terminal-view/src/render/row_plan.rs:495` → `:555` `scan_logical_line` → `:577` `line_role` |
| `RowRoles.role: Vec<Option<RowRole>>`, one entry per display row | HOLDS | `crates/highlight/src/role.rs:50` |
| `roles`/`roles_cur` beside `class_prev`/`class_cur`, same delta + swap + rotation | HOLDS | `crates/terminal-view/src/render/plan_cache.rs:63-67` (fields), `:261-270` (delta/copy), `:425-431` (rotate on scroll) |
| A line's role is the region of its **first** marked char | HOLDS | `crates/terminal-view/src/render/row_plan.rs:537` |
| `Prompt` carries `input_at` = first `Semantic::Input` cell of the logical line | HOLDS | `crates/terminal-view/src/render/row_plan.rs:544-547` |
| The sign is the last prompt glyph in `0..input_at`, else the last non-space | HOLDS (see MIN-3) | `crates/highlight/src/scanner/prompt.rs:69-79` |
| An `Input`-headed line is reported UNMARKED (regex fallback) | HOLDS, and is the **right** rule — see MED-1 | `crates/terminal-view/src/render/row_plan.rs:549` |
| `RowRole::Command` is never produced by the derivation | HOLDS — the variant is unreachable from `line_role`; command mode is entered only through `scan_prompt_line` → `scan_command_mode` | `crates/highlight/src/scanner/prompt.rs:169` |
| `Some(Output)` = no prompt regex | HOLDS | `crates/highlight/src/scanner/mod.rs:75` |
| Exit-code tint via `TerminalInfo::last_exit_code` → `SemanticOverlay::set_exit_code` | HOLDS as wiring, DEAD in practice — MAJ-2 | `crates/terminal/src/backend/osc_router.rs:236` → `crates/terminal/src/session.rs:598` → `crates/terminal-view/src/terminal_view/render.rs:273` |
| Tint suppressed at `display_offset > 0` | HOLDS | `crates/terminal-view/src/terminal_view/render.rs:274-277` |
| Tint applied to `last_completed_prompt` (second-to-last prompt run) at plan time, no rescan | HOLDS | `crates/highlight/src/role.rs:91-109`, `crates/terminal-view/src/render/plan_cache.rs:282`, `:323`; asserted by `the_tint_lands_on_the_completed_block` (`rows_planned == 1`, `class_rows_scanned == 0`) |
| The rescan scope did not grow | HOLDS | `roles_ride_the_class_rescan` asserts `class_rows_scanned == 2` |
| `crates/vt` untouched, public API unchanged | HOLDS | no `crates/vt/**` file in `git diff main...HEAD`; `scripts/vt-public-api.py --check` green inside the gate |

### US-0134

| Claim | Verdict | Where |
|---|---|---|
| `TerminalTheme::prompt_line_bg(reverse_video)` = bg moved `0.11` in lightness toward the fg's side | HOLDS | `crates/terminal-view/src/theme/terminal_theme.rs:34`, `:95-113` |
| `ClassStyles::prompt_line_bg` override | HOLDS (see NIT-2) | `crates/terminal-view/src/theme/terminal_theme.rs:96-98` |
| The shipped asset's fixed `#1a1a1a` is removed | HOLDS | `crates/terminal-view/assets/highlight/default.json`, asserted by `default_styles_loaded` |
| One full-width `BgSpan` pushed before the per-cell loop, for `Prompt`/`Command` rows only | HOLDS | `crates/terminal-view/src/render/row_plan.rs:651-672`, `:699` |
| `resolve_style` uses `row_bg` as the contrast reference | HOLDS | `crates/terminal-view/src/render/row_plan.rs:192-198`, `:230` |
| `every_prompt_row_foreground_clears_the_band` covers all embedded themes | HOLDS | `crates/terminal-view/src/theme/tests.rs`, `assert!(configs.len() > 20)` |
| No band under the regex fallback | HOLDS | `an_unmarked_prompt_row_gets_no_band` |
| `LEADING_WIDE_CHAR_SPACER` covered | HOLDS | `the_band_covers_a_leading_wide_char_spacer` |

---

## Findings, ranked

### MAJ-1 — BLOCKER. Every SSH session, and every local bash, paints its whole screen as a prompt

**What.** The role derivation trusts a `Semantic::Prompt` region that the shell never
closes, so on an A-only shell **every output row** is classified as a prompt line and
painted with the US-0134 band.

**Chain.**

1. `crates/vt/src/terminal/dispatch.rs:1014-1022` — `OSC 133;A` sets `Semantic::Prompt`
   **on the cell template**. Nothing but another OSC 133 clears it. Verified: `SGR 0`
   does not clear it; only `RIS` (`ESC c`) does.
2. OneTerm's own integrations that emit `A` and nothing else:
   - `crates/core/src/config/shell.rs:371-378` — bash `PROMPT_COMMAND` = OSC 7 + `133;A`.
   - `crates/ssh/src/session.rs:711` — `SHELL_INTEGRATION_BOOTSTRAP`, injected into
     **every** SSH session; `shell_integration` defaults to `true`
     (`crates/core/src/ssh_config.rs:337,364,384,441`,
     `crates/session-ui/src/connect_dialog.rs:311`).
3. So every cell written after the first prompt carries `Semantic::Prompt`.
4. `crates/terminal-view/src/render/row_plan.rs:537-547` — the logical line's first marked
   char is `Prompt`, so `line_role` returns `Some(RowRole::Prompt)` for output.
5. `crates/highlight/src/scanner/mod.rs:73` takes the marked prompt path;
   `crates/highlight/src/scanner/prompt.rs:69-79` — with `input_at == None` (no `;B` is ever
   emitted, so no cell is ever `Input`) the "prompt region" is the whole line and the sign
   is the **last** `$ # % > < ❯ ➜ λ → »` on it. Everything to its left is painted `Path`,
   everything to its right is scanned in command mode.
6. `crates/terminal-view/src/render/row_plan.rs:651-672` paints the full-width band on it.

**Reproduced.** Two temporary probes (reverted):

- `crates/vt` — `feed(b"\x1b]133;A\x1b\\user@host:~$ ls\r\na.txt\r\nERROR: 100% nope\r\n")`
  → `cell(1,0).semantic() == Prompt`, `cell(2,0).semantic() == Prompt`.
- `crates/terminal-view` — a row `"ERROR: 100% nope"` marked `Semantic::Prompt` →
  `role = Some(Prompt)`, `PromptSign` lands on the `%` at char 10, `nope` is classed
  `Command`, and the plan carries `BgSpan { col: 0, cols: 24, color: hsla(0,0,0.11,1) }`.

Default semantic-highlighting mode is `Auto` (= on,
`crates/settings/src/terminal_config/layout.rs:41`), and SSH sessions run
`ShellProfile::Unix` (`crates/terminal-view/src/terminal_view/render.rs:209-212`), so this
is the out-of-the-box path, not a corner.

**Why the existing guard misses it.** The `Input`-headed rule
(`row_plan.rs:549`, §4.2 "Why an `Input`-headed line is not trusted") was written for
`cmd.exe`'s `A`+`B`-only flood. The symmetric `Prompt`-headed flood from an `A`-only shell
has no guard at all. The packet's Gaps state that `cmd.exe` is *"the one local shell OneTerm
gives OSC 133 to"* — that is factually wrong: bash gets `A`, zsh gets `A`+`B`
(`shell.rs:160`), and every SSH session gets `A`.

**Smallest candidate fixes (owner decision; a BUG packet is required either way).**

- (a) Trust a `Prompt`-headed line only when the logical line actually contains an `Input`
  or `Output` cell — i.e. the shell proved it closes the region. One `input_at.is_some()`
  test. Cost: a freshly drawn prompt has no `Input` cell until the first keystroke, so the
  band would arrive on the first character typed.
- (b) Trust only the **first** logical line of a consecutive `Prompt` run (a wrapped prompt
  is one logical line, so it is unaffected); the flood's second line onward is dropped.
- (c) Make the emitters complete — add `;B` to bash's `PROMPT_COMMAND` and to the SSH
  bootstrap, and `;C`/`;D` where the shell allows. Necessary anyway for MAJ-2, but it does
  not protect against a third-party `A`-only integration.

### MAJ-2 — the exit-code tint cannot fire in any shipped configuration

`last_exit_code` is written only from `ShellMark::OutputEnd`
(`crates/terminal/src/backend/osc_router.rs:236`), i.e. `OSC 133;D`. No integration OneTerm
ships emits `D`:

| Shell | What OneTerm injects | Marks |
|---|---|---|
| `cmd` | `CMD_OSC7_PROMPT`, `shell.rs:156` | `A`, `B` |
| `zsh` | `ZSH_OSC133_PS1`, `shell.rs:160` | `A`, `B` |
| `bash` | `PROMPT_COMMAND`, `shell.rs:375` | `A` |
| `powershell` / `pwsh` | `POWERSHELL_OSC7_PROMPT_INIT`, `shell.rs:161` | none (OSC 7 only) |
| SSH (any remote) | `SHELL_INTEGRATION_BOOTSTRAP`, `crates/ssh/src/session.rs:711` | `A` |

So `TerminalInfo::last_exit_code` is permanently `None`, `update_tint` always computes
`None`, and the `Success`/`Error` sign — an explicit acceptance item of US-0133 — never
appears unless the user hand-writes a prompt function. The packet's own tint frames
(`us-0133-tint-success.png`, `us-0133-tint-error.png`) come from exactly such a
hand-instrumented PowerShell tab; the packet does not say the feature is unreachable
otherwise. This also makes the brief's proposed walk (a `cmd` tab and `cmd /c exit 3`)
incapable of showing a tint, which the GUI walk below confirms.

### MAJ-3 — US-0134's central design choice is unasserted: inverting the band leaves the suite green

Mutation applied to `crates/terminal-view/src/theme/terminal_theme.rs:105`:

```rust
-        let toward = if fg.l >= bg.l { 1.0 } else { 0.0 };
+        let toward = if fg.l >= bg.l { 0.0 } else { 1.0 };
```

`cargo test -p oneterm-terminal-view` → **389 passed, 0 failed**. Nothing caught it.

`the_band_never_crosses_its_theme` asserts `(band.l - bg.l).abs() < (band.l - fg.l).abs()`,
which an *inverted* band satisfies more comfortably than the intended one, and
`assert_ne!(band, bg)` is satisfied by any non-zero move. Under the mutation the light-theme
band is `l = 0.98 + (1.0 - 0.98) * 0.11 = 0.9822` against a `0.98` background — a band nobody
can see, which is the exact failure mode `PROMPT_BAND_MIX`'s comment says the design avoids.
(The intended value is `0.8722`, matching the packet's measured `#fafafa` → `#dedede`.)

Fix: assert that `sign(band.l - bg.l) == sign(fg.l - bg.l)` and that `|band.l - bg.l|` clears
a floor, in `the_band_never_crosses_its_theme`.

The other two required mutations **do** fail as expected (see Commands), so the role
derivation and `last_completed_prompt` are genuinely pinned.

### MED-1 — attack (1) answered: the implemented `Input`-headed rule is right, the proposed replacement regresses `cmd`

The brief proposed "`Input`-headed AND the previous logical line was a marked `Prompt` →
`Command`". Checked:

- With a full `A`/`B`/`C`/`D` shell the command line is **not** `Input`-headed. The prompt
  and the echoed command share one logical line, so the line is `Prompt`-headed and carries
  `input_at`; `scan_prompt_line` (`prompt.rs:169`) hands the tail to `scan_command_mode`.
  No marked-command benefit is lost — the packet's own
  `a_marked_prompt_with_a_space_in_it_still_finds_its_sign` shows `dir` classed `Command`.
  An `Input`-headed line arises only for a continuation line of a multi-line command, or
  under an `A`/`B`-only shell.
- Under `cmd`, the proposal misfires on **every command**. Probe (reverted): a frame with
  `C:\ws>dir` (`Prompt` 0..6, `Input` 6..9) above `Volume in drive C` (`Input` 0..17) gives
  `roles = [Some(Prompt), None]`. The first output line of every `cmd` command sits directly
  under a marked `Prompt` line, so the proposed rule would scan it in command mode — the
  regression the implemented rule exists to prevent.

Recommendation: **keep the implemented rule.** The gap to close is MAJ-1, not this.

### MED-2 — the band's "never crosses its theme" claim is false on a low-contrast theme

Probe (reverted) over `prompt_line_bg`:

```
bg.l=0.5 fg.l=0.55 -> band.l=0.5550 (past the foreground)  on_bg_side=false
bg.l=0.5 fg.l=0.45 -> band.l=0.4450 (past the foreground)  on_bg_side=false
bg.l=0.5 fg.l=0.52 -> band.l=0.5550                        on_bg_side=false
bg.l=0.5 fg.l=0.50 -> band.l=0.5550                        on_bg_side=false
```

`PROMPT_BAND_MIX`'s doc comment (`terminal_theme.rs:19-33`) says the band "can never be a
dark stripe under dark text, because it is always on the background's side of the pair".
Whenever the fg/bg lightness gap is under `0.11` of the room, it is not. Text stays legible
because `resolve_style` runs `ensure_contrast` against `row_bg` (`row_plan.rs:230`), and
every shipped theme has a wide gap, so this is a comment/test accuracy defect rather than a
rendering one — but it is the same blind spot as MAJ-3.

### MIN-1 — an exit code arriving on an unchanged frame is dropped until the next changed frame

`PlanCache::update` returns at `plan_cache.rs:171` on `SnapshotUpdate::Unchanged` before
`update_tint` (`:282`), and the exit code is not part of `StyleKey`. Self-healing (the next
prompt redraw dirties rows) and unreachable today because of MAJ-2, but it is an
invalidation hole with no test.

### MIN-2 — `last_completed_prompt` heap-allocates once per frame

`crates/highlight/src/role.rs:93` builds a `Vec<Range<usize>>` of every prompt run on every
frame that carries an exit code, in a pipeline that is otherwise careful to reuse buffers
(`scan_line_into`'s PERF-23 comment, `Scratch`). Two locals (`prev`, `last`) give the same
answer with no allocation.

### MIN-3 — `marked_sign`'s last-resort rule only skips ASCII spaces

`crates/highlight/src/scanner/prompt.rs:78` uses `c != ' '`. Probes confirm the common
cases are fine — `PS C:\> `, `~/src ❯ ` and `~/src ❯\u{00a0}` all resolve to the right index,
because `❯` is a known glyph. The hole is an *unknown* sign glyph followed by a tab or a
no-break space, which then gets tagged `PromptSign` on an invisible cell.
`char::is_whitespace` closes it.

### MIN-4 — docs

- `docs/terminal-semantic-highlighting.md` §4.2 — the section US-0133 rewrote — still cites
  `crates/core/src/terminal/osc.rs` (`Osc133Kind`). That directory does not exist; OSC 133 is
  parsed in `crates/vt`. `scripts/check-doc-paths.py` does not cover this document (its
  checked set is `docs/architecture.md`, `docs/agents/*.md`, `docs/README.md`,
  `docs/terminal-backend.md`, `README.md`, `AGENTS.md`), so the gate cannot see it.
- The same section states the `cmd.exe` `A`/`B` case as *the* incomplete-integration case and
  never mentions bash/zsh/SSH — the root of MAJ-1. §4.2 and both packets' Gaps need the
  correction.
- §8 item 6 says "Nothing in `resolve_style`'s `paint_bg` rule changes" — true, but the
  section does not record that US-0134 changed `resolve_style`'s **contrast reference** to
  `row_bg`, which is the load-bearing half of the change.

### NIT-1 — `crates/terminal-view/src/render/row_plan.rs:801`: `The whole frame'''s semantic classes`.

### NIT-2 — `prompt_line_bg`'s explicit override ignores `reverse_video`: with
`class_styles.prompt_line_bg` set, DECSCNM returns the same fixed colour. Unreachable today
(nothing ships an override).

---

## Attack answers not already covered

**(2) An unknown sign glyph / a trailing space.** Covered by MIN-3. `PS C:\> ` (trailing
space) resolves correctly: `rposition(is_prompt_sign)` finds `>` at index 6 before the
non-space rule is consulted. The non-space rule is reached only when no known glyph is in
the region.

**(3) `input_at` on a wrapped prompt.** Correct by construction: `scan_logical_line`
(`row_plan.rs:563-577`) builds `char_semantic` across **all** rows of the wrap run in the
same loop that builds `line_text`, and `input_at` is a char index into that joined string,
which is exactly what `scan_line_into` is given. A boundary on row 2 is therefore an index
past the row-1 chars, with no separate bookkeeping to drift. A cell cannot hold two regions,
so "prompt and input share a cell after reflow" cannot arise; the reflow carries the region
with the cell.

**(4) The tint.** Two prompts on adjacent rows vs one wrapped prompt is decided from the
frame's `WRAPLINE` flags (`role.rs:91-107`) and is pinned by
`two_prompts_on_adjacent_rows_are_two_runs`, which asserts both directions. A command that
prints nothing is exactly that case. `last_completed_prompt`'s wrap walk cannot index out of
range (`is_prompt(row + 1)` returns `false` past the end). Staleness "a new prompt appears
before `D` arrives" is possible in principle (the older code would tint the newer prompt) but
is unreachable today (MAJ-2) and does not occur with integrations that emit `D` from the
prompt function, where `D` precedes `A`. Suppression while scrolled is at
`terminal_view/render.rs:274`. The "dirties only the affected rows" claim is asserted with
`FrameStats` by `the_tint_lands_on_the_completed_block` (`rows_planned == 1`,
`class_rows_scanned == 0`) — and by the mutation, which fails four `role.rs` tests.
One residual: `self.tint`'s row range is not rotated by `shift()`, so after a scroll the
*old* range can dirty unrelated rows — over-invalidation only, never under.

**(5) The band.** Mid-grey / low-contrast: MED-2. Reverse video: `prompt_line_bg(true)`
swaps the pair and `push_prompt_band` takes `Color::Foreground` as the row default
(`row_plan.rs:657-661`) — consistent, and asserted by
`the_shipped_asset_leaves_the_band_to_the_theme`. Theme-asset override: parsed and honoured
(`prompt_line_bg_is_parsed_when_a_theme_sets_it`), with NIT-2. `resolve_style` contrast
fallback: a cell whose fg is close to the band is lifted by `ensure_contrast(fg, row_bg)`,
but **only** when `!fg_color.is_app_chosen_exact()` — a program that names an exact colour
close to the band is left alone, which is the same policy as on the normal background, one
band-width further from what the program measured against. `LEADING_WIDE_CHAR_SPACER` and
"no band under the fallback": both asserted, both hold.

**(6) The suspected theme-refresh bug.** Not reproduced and not refuted; see Gaps. Code
reading supports the implementer's hypothesis being *possible* but not *shown*:
`refresh_theme` (`crates/terminal-view/src/terminal_view/render.rs:443-477`) reads
`dynamic_colors()` at `:173`, before `set_default_colors` pushes the new palette at `:460`,
and `apply_dynamic_colors` (`theme/terminal_theme.rs`) overwrites `theme.fg`/`theme.bg`
whenever `dc.foreground`/`dc.background` is `Some`. If a program on that tab had set OSC
10/11, those values survive every theme switch — which is correct behaviour, not a bug — so a
`BUG` is only warranted if the owner sees washed-out text on a tab where **no** program set
OSC 10/11. Recommendation: **do not open a BUG yet**; attach a one-line diagnostic
(`log::debug!` of `dynamic` in `refresh_theme`) to the next theme-switch report. The band no
longer depends on `fg`'s value, only on its side, so US-0134 is not blocked either way.

**(7) Perf.** The roles pass adds **no extra pass over cells**. `append_text_into`
(`crates/terminal-view/src/render/frame.rs:409`) pushes one `Semantic` byte per non-spacer
cell inside the loop it already runs; `Scratch::char_semantic` is `with_capacity(256)` and
reused, so no per-frame allocation. `line_role` is one `position` over `char_semantic`
(early-exit at the first marked char, normally index 0) plus, for `Prompt`-headed lines only,
a second `position` for the first `Input`. `class_rows_into` adds one `roles[r] = None` per
row in the rescan range and one `roles[r] = role` per row per run. The plan cache adds a
`Vec<Option<RowRole>>` (1 byte/row), a per-row compare and copy, and a rotate on scroll — the
same shape as `class_prev`. The rescan bound is unchanged, asserted by
`roles_ride_the_class_rescan`. The only added per-frame allocation is MIN-2.

**(8) Public API and docs.** `git diff main...HEAD` touches no `crates/vt/**` file;
`python scripts/vt-public-api.py --check --no-doc` and `--diff-platforms` are green inside
the gate. Docs: accurate for what was built, incomplete on the premise — MIN-4.

---

## Commands

```
git reset --hard 0dfd2070
cargo test -p oneterm-highlight -p oneterm-terminal-view -p oneterm-theme -p oneterm-terminal
    -> 94 / 389 (3 ignored) / 218 / 5 passed, 0 failed
```

Mutations (each applied alone, then reverted with `git checkout --`):

| Mutation | Expected | Result |
|---|---|---|
| `line_role`: `position` → `rposition` (last marked char decides) | must fail | **FAILED 9 tests**: `roles_are_read_from_the_marks`, `the_tint_lands_on_the_completed_block`, `a_wrapped_prompt_takes_its_role_from_the_marks`, `marks_appearing_mid_session_are_decided_per_row`, `output_left_tagged_input_by_a_shell_without_osc_133_c_is_unmarked`, `a_marked_prompt_row_carries_a_full_width_band`, `every_row_of_a_wrapped_prompt_carries_the_band`, `every_glyph_on_a_prompt_row_clears_the_band`, `the_band_is_painted_under_the_per_cell_backgrounds` |
| `last_completed_prompt`: `runs.len() - 2` → `runs.len() - 1` (return the last run) | must fail | **FAILED 4 tests**: `last_completed_prompt_is_the_one_above_the_newest`, `a_wrapped_prompt_is_tinted_as_a_whole_run`, `a_running_command_tints_the_previous_prompt_not_its_own`, `two_prompts_on_adjacent_rows_are_two_runs` |
| `prompt_line_bg`: invert the band's direction (toward the background) | must fail | **PASSED 389, 0 failed — MAJ-3** |

Gate:

```
pwsh scripts/ci-local.ps1
```

final line:

```
ci-local: all checks passed.
```

(`cargo test -p oneterm-vt` re-run on the clean tree afterwards: 563 passed, 2 ignored.)

---

## GUI walk (Windows, `fast-dev`, own build, own pid)

`cargo build -p oneterm-app --profile fast-dev`, launched with a scratch `USERPROFILE` and
driven by posted `WM_CHAR` / `WM_KEYDOWN`. Frames captured with `PrintWindow(hwnd, dc, 2)`
against the pid this session started. `target/fast-dev` deleted afterwards.

**`evidence/US-0133-US-0134-verify-cmd-bands-no-tint.png`** — a `cmd` tab (the shell
OneTerm gives `A`+`B` to), after `cmd /c exit 3` and `echo hello world`.

- US-0134 works as specified: all three prompt rows carry the full-width band; the output
  row `hello world` carries none. The first prompt wrapped over two rows in an earlier
  frame and both rows carried it.
- US-0133's classification works on the prompt rows: the cwd is `Path` (blue), `cmd` and
  `echo` are `Command` (orange), `/c` is `Option`.
- **MAJ-2 visible:** `cmd /c exit 3` completed with a non-zero code and the prompt sign of
  the block that ran it is **not** red — nor is the `echo` block's sign green. `cmd` emits
  no `OSC 133;D`, so `last_exit_code` is never set and the tint never fires. This is the
  walk the brief asked for, and it shows the feature not firing rather than firing.

**`evidence/US-0133-US-0134-verify-a-only-flood.png`** — MAJ-1, reproduced end to end.
A script writes exactly what OneTerm's bash `PROMPT_COMMAND` and the SSH bootstrap put on
the wire — `OSC 7` then `OSC 133;A`, and nothing else — and then prints five ordinary output
lines. Result:

- **Every one of the six output rows carries the prompt-line band.**
- `ERROR: build failed, 100% of targets stale` — the `%` of `100%` is painted as the prompt
  sign and `of` behind it as a `Command`.
- `cc -o a.out main.c   # 2 warnings` — the `#` is the prompt sign, `2` is a `Command`.
- `see http://example.com/log for details` and `done in 12s` — the last non-space character
  of the line (`s`) is painted as the prompt sign, which is the `marked_sign` fallback at
  `crates/highlight/src/scanner/prompt.rs:78` doing what it is told.

That screen is what every SSH tab and every local bash tab looks like after the first prompt.

---

## Gaps in this verification

- The `cfg(unix)` shell-integration paths (bash `PROMPT_COMMAND`, zsh `PS1`) and a real SSH
  remote were not exercised on this machine. MAJ-1 is proved from the injected strings
  (`shell.rs:375`, `ssh/session.rs:711`), the engine's template behaviour (three `crates/vt`
  probes), and the render pipeline (one `terminal-view` probe) rather than from a live bash
  or a live remote. The byte stream those two integrations produce is reproduced exactly in
  the GUI walk.
- The theme-refresh suspicion (attack 6) was analysed but not driven to a verdict: the
  settings UI could not be opened with the posted-message driver in the budget available,
  so the "switch theme twice and read a pixel" walk was not run. A light-theme band frame
  was not captured either, for the same reason; `us-0134-band-light-theme.png` from the
  implementing packet stands, and MAJ-3 is proved by mutation rather than by pixels.
- No benchmark was run for attack 7; the answer is from reading the added code paths, and
  `US-0135` owns the measured budget.

---

# Re-verification of `e0d85801` — 2026-09-22

Second independent agent, adversarial pass over the acceptance rework.

Under test: `feat/semantic-row-roles` @ `e0d85801` — the rework commit `106ece85` on the
first verifier's `ddc28122`, then `main` @ `bf13cd21` (`BUG-0073`, `US-0135`) merged in.
Worktree `.claude/worktrees/agent-a045c7f23df73bf81`, reset to `e0d85801`. Every probe and
mutation below was reverted; `git status` was clean before the commit that carries this
section.

## Verdicts

| Packet | Verdict |
|---|---|
| `US-0133` — row roles come from the shell's OSC 133 marks | **ACCEPT WITH CHANGES** — the blocker is genuinely fixed and pinned by a mutation, but the transition rule brings a new, untested and undocumented hole of its own (RV-MAJ-1: two prompts on adjacent rows), and the merge left `US-0135`'s own scan-bound record contradicting the shipped assertions (RV-MED-2). |
| `US-0134` — the prompt line gets its background | **ACCEPT** — MAJ-3 and MED-2 are both closed and both now fail under mutation. It inherits RV-MAJ-1 from `US-0133` (one prompt row loses its band) but owns no defect of its own. |
| Overall | **ACCEPT WITH CHANGES** — no blocker. RV-MAJ-1 needs an owner decision, a `BUG` packet and a record; RV-MED-1/2 are documentation the merge owes. |

## Status of every finding from the first verification

| ID | Status | Evidence |
|---|---|---|
| **MAJ-1** — A-only integrations flood every line as prompt | **FIXED** | `line_role` (`crates/terminal-view/src/render/row_plan.rs:575-591`) gives `Prompt` only on a transition. Asserted by `an_a_only_shell_does_not_turn_its_output_into_prompts`, `an_a_only_shell_paints_no_band`, `the_viewports_first_line_is_never_a_prompt`. Reproduced in the GUI on the exact wire bytes and **no row is banded** — `US-0133-US-0134-reverify-a-only-and-back-to-back.png`, top block, against the first verification's `US-0133-US-0134-verify-a-only-flood.png`, where one continuous band covered the whole flood and `%`, `#` and a trailing `s` were painted as prompt signs. Mutation M1 (ignore `prev`) fails 4 tests. |
| **MAJ-2** — no shipped integration emits `133;D`, so the tint is dead out of the box | **NOT FIXED — deferred by decision** | Recorded honestly in §4.2 ("It needs a shell that emits `OSC 133;D`, and none of the integrations above does") and in both packets' Gaps, with the per-shell table; completing the emitters is `US-0136`. `US-0133`'s headline acceptance item therefore remains unreachable in every shipped configuration. Accepted as an owner call, not re-litigated here — but the packet is `Implemented`, and a reader of the acceptance list alone still cannot tell. |
| **MAJ-3** — inverting the band's direction left the suite green | **FIXED** | `assert_band_between` (`crates/terminal-view/src/theme/tests.rs:193`) asserts `signum(band.l - bg.l) == signum(fg.l - bg.l)`, a strict "did not reach the text" bound and a floor. Mutation M3 (`toward` flipped) now fails `the_band_sits_between_the_background_and_the_text` **and** `every_prompt_row_foreground_clears_the_band`. |
| **MED-1** — the implemented `Input`-headed rule is right | **CONFIRMED, unchanged** | `output_left_tagged_input_by_a_shell_without_osc_133_c_is_unmarked` still holds; the rejected alternative is recorded in §4.2 and in the packet's rework note. |
| **MED-2** — the band crosses the text on a narrow fg/bg gap | **FIXED** | `prompt_line_bg` caps the move at half the gap (`terminal_theme.rs:115`). Probed at `0.5/0.55`, `0.5/0.45`, `0.5/0.52`, `0.98/0.239`, `0.159/0.937` and the degenerate `0.5/0.5` — all pass (the first three are now in the shipped test). |
| **MIN-1** — an exit code on an `Unchanged` frame was dropped | **FIXED** | `plan_cache.rs:190-198` compares `exit_code` before the early return; `an_exit_code_on_an_unchanged_frame_is_not_dropped` asserts `frames_unchanged == 0` and the tint landing. |
| **MIN-2** — `last_completed_prompt` allocated a `Vec` per frame | **FIXED** | `crates/highlight/src/role.rs:112-131`: two locals (`before_last`, `last`), no allocation. |
| **MIN-3** — `marked_sign` skipped only ASCII spaces | **FIXED** | `crates/highlight/src/scanner/prompt.rs:81`: `!c.is_whitespace()`. |
| **MIN-4** — doc citations | **PARTLY FIXED** | §4.2 and §13 Q1 now say `crates/vt` and name the old parser as gone; §4.2 carries the per-shell mark table; §8 item 6 records the `row_bg` contrast reference. **But** the merge duplicated a whole §4.2 block (RV-MED-1) and left §10.1 stating a scan bound this branch changed (RV-MED-2). |
| **NIT-1** — `The whole frame'''s` typo | **NOT FIXED** | still at `crates/terminal-view/src/render/row_plan.rs:851`. |
| **NIT-2** — the explicit `promptLineBg` override ignores `reverse_video` | **NOT FIXED, by decision** | recorded in the `US-0134` rework note; unreachable, nothing ships an override. |

## New findings

### RV-MAJ-1 — two prompts on adjacent rows: the second one is unmarked

The transition rule reads the **previous logical line's own head region**, and a prompt
line's head is `Prompt`. So when one prompt line is immediately followed by another — which
is what a command that printed nothing leaves behind — the second prompt does not
transition and is reported unmarked: no band, no marked-prompt scan, and no tint.

Proved three ways:

- **Unit, real OSC 133 stream through the engine** (probe, reverted). Feeding
  `one\r\n` `133;A` `user@host:~$ ` `133;B` `true\r\n` `133;C` `133;D;0` `133;A`
  `user@host:~$ ` `133;B` into a 6-row viewport gives
  `roles = [None, Some(Prompt), None, None, None, None]` — row 2 is the new prompt and it
  is unmarked — and `tint = None`, although `133;D;0` was delivered. A full `A`/`B`/`C`/`D`
  shell is the configuration `US-0133` calls "exact".
- **GUI, `A`/`B` wire bytes** —
  `US-0133-US-0134-reverify-a-only-and-back-to-back.png`, bottom block: `C:\ws>cd ..`
  carries the band (it follows an output line), `C:\ws>rem quiet` and the final `C:\ws>`
  directly below it do not.
- **GUI, a real `cmd` tab** — `US-0133-US-0134-reverify-cmd-real-tab.png`: here **every**
  prompt row is banded, including `cd .` twice and `rem quiet` twice. `cmd`'s `$P$G` prompt
  is preceded by a blank line, and a blank row's head is `Semantic::None`, which *is* a
  transition. So Windows `cmd` — the only marked shell exercisable on this machine — is
  accidentally immune, and the defect is invisible there.

Reachability: any shell whose prompt does not begin with a blank line, after any command
that printed nothing (`cd`, `export`, `set`, `rem`, a successful `mkdir`). `ZSH_OSC133_PS1`
(`crates/core/src/config/shell.rs:160`) is exactly that shape —
`%{…133;A…%}%n@%m:%~ %# %{…133;B…%}`, one line, no leading newline — and `zsh` is one of
the two shells OneTerm gives `A`/`B` to. It was not exercisable here (`cfg(unix)`).

Impact: the band appears and disappears on the live prompt row depending on whether the
*previous* command printed anything — the flashing-row failure mode `US-0134`'s own
"not under the regex fallback" rule exists to avoid, now reached by a different route. The
exit-code tint is lost for the same blocks (`last_completed_prompt` needs two prompt runs),
which matters the moment `US-0136` lands.

`role.rs`'s `two_prompts_on_adjacent_rows_are_two_runs` builds `[P, P]` by hand and asserts
the tint picks the older one. Its hard-newline half is now a shape the derivation cannot
produce (its wrapped half still is), so that assertion pins nothing the pipeline reaches.
Nothing in the packets, the Gaps or §4.2 mentions the case.

Candidate fixes are the owner's call; the cheapest is to treat a `Prompt`-headed line whose
predecessor is also `Prompt`-headed as a transition when the predecessor carried an `Input`
region (i.e. the shell did close `Prompt` once on that line) — `input_at.is_some()` on the
previous line, one extra byte of per-row state beside `heads`. A `BUG` packet is needed
either way, together with the record §4.2 does not yet carry.

### RV-MED-1 — the merge duplicated a §4.2 block

`docs/terminal-semantic-highlighting.md` carries **The Windows sign rule (`BUG-0073`)**
twice, verbatim: lines 279-293 and 295-309. Both merge parents carry it once
(`git show bf13cd21:docs/…` and `git show 106ece85:docs/…` each match once); the copy was
introduced by the merge `e0d85801` itself. No conflict markers anywhere else in the tree.

### RV-MED-2 — the merge left `US-0135`'s scan bound stating the pre-rework number

The rework widened the semantic rescan to the dirty runs **plus one logical line**, and
changed `class_delta_replans_the_continuation_row` to assert `class_rows_scanned == 3` and
`class_scans == 2`. Two records still state the old numbers as facts:

- `docs/terminal-semantic-highlighting.md` §10.1: "an edit inside a wrapped line scans the
  wrap run and nothing else — `class_rows_scanned == 2` in a 12-row viewport,
  `class_scans == 1`".
- `US-0135-highlighter-benchmark-and-scan-bounds.md`, Decision 3, same two numbers, plus
  the checked acceptance box "`class_scans` and `class_rows_scanned` stay at the wrap run
  for an edit inside a wrapped line", which the branch makes false.
- §13 Q5: "the scan scope is the dirty rows **closed under wrap runs**… the same scope and
  the same invalidation as the URL masks" — the two scopes are no longer the same, which is
  the whole reason the rework split the loop in two.

§4.2 states the new bound correctly and `US-0133`'s own acceptance list even records the
`2 -> 3` move, so this is purely the other side of the merge: §10.1 and `US-0135` were not
reconciled with the branch that changed the number they quote.

### RV-NIT-1 — the committed bench baseline's `"measured"` label is now stale

The branch moved every `scan_line_into` call site in
`crates/tools/src/bin/highlight-bench.rs` from `RowRole::Output` to `(None, None)`, and
changed the emitted JSON's label from `"scan_line_into, RowRole::Output"` to
`"scan_line_into, unmarked (the prompt-regex fallback)"`. The committed
`crates/tools/highlight-bench-baseline.json` is byte-identical to `main` and still carries
the old label.

This was checked for a workload change and there is **none**: on `main`,
`RowRole::Output` *was* the fallback arm — it ran `prompt::prompt_sign` first and only then
`output::scan_output` (`git show bf13cd21:crates/highlight/src/scanner/mod.rs`) — and that
is exactly what `None` does now. The rename is what `US-0133` introduced: `Some(Output)` is
the new "marked output, regex never runs" arm, which the bench does not use. So the
baseline's numbers and §10.1's table stay comparable and the only defect is the label.
Refresh it with the next deliberate baseline run.

### RV-NIT-2 — a garbled assertion message

`crates/terminal-view/src/render/plan_cache.rs:1400`: `"…whose role depends on the
              region this one starts in…"` — a line break collapsed into a run of spaces
inside the string literal.

### RV-NIT-3 — `assert_band_between` is vacuous on the pair it most needs to judge

`crates/terminal-view/src/theme/tests.rs:195` returns early when `|fg.l - bg.l| < EPSILON`.
For that pair `prompt_line_bg` returns the background itself (the cap is `0`), i.e. an
invisible band — the exact outcome the floor exists to rule out — and the helper asserts
nothing. Harmless (no theme is in that state; the text would be invisible too) but the
comment claims more than the code checks.

## Adversarial checks, and what they answered

**The transition rule (attack 1).**

- `line_head_region` (`row_plan.rs:543`) is the first non-`None` region of the joined
  logical line; `line_role` (`:575`) returns `Prompt` only for
  `Semantic::Prompt` with `prev == Some(p), p != Prompt`; `Output` for `Semantic::Output`;
  `None` for `Prompt`-without-transition, `Input` and unmarked. Matches the brief exactly.
- **The chain is provably one line long.** `line_head_region` is a pure function of the
  line's own cells, so a change at run *N* cannot move `heads[N+1]`; and `line_role(N+2)`
  reads only `heads[N+1]` and *N+2*'s own cells. The carry in `mark_scan_runs`
  (`plan_cache.rs:451-454`) therefore has to cover exactly one further line, and no
  fixture can break it. Probed anyway: a frame where changing row 1's region flips row 2
  from `Prompt` to unmarked with **two** consecutive prompts below gives
  `after == truth` (a from-scratch cache on the same frame) at `class_rows_scanned == 2`.
- **No drift under scroll.** A probe that feeds six marked blocks into a 6-row viewport and
  scrolls three rows back and three forward after each, comparing all six roles against a
  fresh cache every step — 42 comparisons — passes. `heads` rotates with `roles`,
  `class_prev`, `mask_prev` and `wraps_prev` in `shift()` (`:503-508`); a reflow changes
  `frame.size()`, which sets `restyled`, which clears every key and rescans the viewport.
- `url_rows_scanned == 2` and `class_rows_scanned == 3` in `roles_ride_the_class_rescan`:
  the URL bound is unchanged and only the class pass pays for the forward dependency.
- A-only bash stream → no prompt, no band (GUI + unit). `cmd` `A`/`B` → prompt found (GUI +
  `output_left_tagged_input_by_a_shell_without_osc_133_c_is_unmarked`). Full `A`/`B`/`C`/`D`
  → exact (`a_full_a_b_c_d_shell_is_classified_exactly`). The top-of-viewport prompt stays
  unmarked across scroll (`a_prompt_at_the_top_of_the_viewport_is_unmarked_and_stays_so`,
  and it fails under mutation M2, so it pins behaviour and not only a counter).
- The one case the rule gets wrong is RV-MAJ-1.

**The band (attacks 2 and 3).** `PROMPT_BAND_MIX` direction from `fg`, distance
`|toward - bg.l| * 0.11` capped at `|fg.l - bg.l| / 2`. Probes above. The floor in
`assert_band_between` is `min(|gap|/2, 0.05)`, which the `0.5/0.55` pair meets exactly —
tight, but correct by construction rather than by luck.

**Minors (attack 3).** Exit code on `Unchanged`: fixed and tested. Per-frame `Vec`: gone.
`is_whitespace`: in. Doc citations: fixed, then damaged by the merge (RV-MED-1/2).

**Merge sanity with `BUG-0073`/`US-0135` (attack 4).** `scanner_tests.rs` and
`highlight-bench.rs` call sites are all on the `(None, None)` fallback; the
"mark changes the answer" assertion in `marked_output_never_runs_the_prompt_regex` uses
`C:\work>dir > out.txt`, the line the tightened regex still reads as a prompt, and
`a_marked_output_row_is_not_read_as_a_prompt` mirrors it in the view. The bench builds and
runs (`cargo run -p oneterm-tools --bin highlight-bench --release -- --runs 3`). The
baseline JSON is untouched and, after checking `main`'s `RowRole::Output` arm, correctly so
— only its `"measured"` label is stale (RV-NIT-1).

## Commands

```
git reset --hard e0d85801
cargo test -p oneterm-highlight -p oneterm-terminal-view -p oneterm-theme \
           -p oneterm-terminal -p oneterm-tools
    -> 96 / 396 (3 ignored) / 218 / 5 / 14 passed, 0 failed
```

Probes (each reverted with `git checkout --`):

| Probe | Result |
|---|---|
| A — `A`/`B`/`C`/`D`, a command that printed nothing | `roles = [None, Some(Prompt), None, …]`, `tint = None` — **RV-MAJ-1** |
| B — the chain: a changed run flips the next line's role, two prompts below | incremental `== ` full rescan, `class_rows_scanned == 2` |
| C — roles vs a full rescan over 6 appended blocks x 6 scroll steps | 42/42 equal, no drift |
| band pairs `0.98/0.239`, `0.159/0.937`, `0.5/0.5` added to `the_band_sits_between…` | pass |

Mutations (each applied alone, then reverted):

| Mutation | Expected | Result |
|---|---|---|
| `line_role`: ignore `prev` (`\|\| true`) | must fail | **FAILED 4**: `an_a_only_shell_does_not_turn_its_output_into_prompts`, `an_a_only_shell_paints_no_band`, `the_viewports_first_line_is_never_a_prompt`, `a_prompt_at_the_top_of_the_viewport_is_unmarked_and_stays_so` |
| `mark_scan_runs`: drop the carry (`carry = false`) | must fail | **FAILED 3**: `a_prompt_at_the_top_of_the_viewport_is_unmarked_and_stays_so`, `class_delta_replans_the_continuation_row`, `roles_ride_the_class_rescan` |
| `prompt_line_bg`: invert the band's direction | must fail | **FAILED 2**: `the_band_sits_between_the_background_and_the_text`, `every_prompt_row_foreground_clears_the_band` — the MAJ-3 hole is closed |

Bench:

```
cargo run -p oneterm-tools --bin highlight-bench --release -- --runs 3
```

It builds and runs. The CJK worst case reproduces §10.1's `release` figure
(0.54-0.63 ms for an 8 000-char scan against the recorded 0.51-0.56 ms), which is the
independent check that the `(None, None)` call sites did not change the workload. The
`prompt` shape reads 1.7-2.9 ns/char here against the baseline's 1.15, at 9-67% spread on
a loaded machine with `--runs 3` against the baseline's 9 — noise, and not this branch's
code path in any case (`prompt::prompt_sign` is `BUG-0073`'s). The baseline was not
refreshed; the comparison above is the reason it did not need to be.

Gate:

```
pwsh scripts/ci-local.ps1
```

final line:

```
ci-local: all checks passed.
```

## GUI walk (Windows, `fast-dev`, own build, own pid)

`cargo build -p oneterm-app --profile fast-dev`, launched with a scratch `USERPROFILE` and
driven with posted `WM_CHAR`/`WM_KEYDOWN`; frames captured with `PrintWindow(hwnd, dc, 2)`
against the pid this session started, which was killed by pid afterwards.
`target/fast-dev` deleted.

**`evidence/US-0133-US-0134-reverify-a-only-and-back-to-back.png`** — three scenes in one
`cmd` tab, each fed as raw bytes so the wire stream is exactly what the integrations send.

- *A-only* (`OSC 7` + `OSC 133;A`, then five output lines — OneTerm's bash
  `PROMPT_COMMAND` and the SSH bootstrap): **no row carries a band**, `ERROR` and `failed`
  keep their output classes, the URL keeps its underline, and no `%`, `#` or trailing `s`
  is painted as a prompt sign. This is the first verification's MAJ-1 frame, fixed.
- *`A`/`B` with output*: `C:\ws>dir` and `C:\ws>echo hi` are banded, `a.txt` and `hi` are
  not.
- *`A`/`B` with nothing printed*: `C:\ws>cd ..` is banded, and the two prompt rows directly
  below it are not — **RV-MAJ-1**.

**`evidence/US-0133-US-0134-reverify-cmd-real-tab.png`** — the same six commands typed into
a real `cmd` tab with OneTerm's own `CMD_OSC7_PROMPT`. Every prompt row is banded, output
rows are not, and `echo`/`cd`/`rem` are classed `Command` over the band. `cmd`'s blank line
before `$P$G` is what hides RV-MAJ-1 on Windows.

## Gaps in this re-verification

- The `cfg(unix)` integrations (bash `PROMPT_COMMAND`, zsh `PS1`) and a live SSH remote
  were again not exercised. MAJ-1's fix and RV-MAJ-1 are both proved from the exact byte
  streams those integrations put on the wire, fed to the real engine in unit probes and to
  the real app in the GUI walk — not from a live bash or a live remote. RV-MAJ-1's
  *reachability* on `zsh` is therefore an inference from `ZSH_OSC133_PS1`'s shape, not an
  observation.
- MAJ-2 was not re-tested: nothing about it changed, and no shell here emits `133;D`
  without hand instrumentation.
- The theme-refresh suspicion from the first verification (`US-0134` Gaps) was not
  investigated; it is untouched by the rework and the band no longer depends on
  `TerminalTheme::fg`'s value.
- No light-theme band frame was captured; the direction is pinned by mutation and by the
  per-theme sweep in `theme::tests`, which covers every embedded variant.
- The bench was run at `--runs 3` on a machine that had just finished the gate, so its
  numbers settle the *workload* question (the CJK figure matches) but are too noisy to
  judge the `prompt` shape against the recorded baseline. Whether `BUG-0073`'s new sign
  rule moved that row is `US-0135`'s question, not this branch's.
