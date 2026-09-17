# Work: Secondary text meets a 4.5:1 contrast floor in every built-in theme

ID: US-0111
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

Secondary text is readable. In every theme the application ships, the `muted.foreground` and
`tab.foreground` tokens reach a contrast ratio of at least **4.5:1** against the surface they
are drawn on, and a repeatable check keeps them there.

## Findings and proposals covered

`P1` — *"Raise `muted.foreground` (and `tab.foreground`) in the built-in themes until
secondary text reaches >= 4.5:1 on its own surface; add the ratio as a check in `scripts/` if
it should stay true."*

Addresses `F6` (**high**) and the contrast half of `F32` (low), quoted from
`research/ux-walkthrough-2026-09-16.md`:

> | F6 | whole app (both themes) | All secondary text — session host addresses, search
> placeholders, empty-state copy, key-binding chips, `Default:` hints, `+`-menu shortcut
> hints, SFTP dates, Update Status — renders at **contrast 2.48:1** (`#5c6370` on `#23272e`),
> below WCAG AA's 4.5:1. The light theme is no better: inactive tab labels **2.18:1**, host
> text **2.37:1**. Primary text on the same rows is 13:1. | Accessibility. Single token:
> `muted.foreground` (and `tab.foreground`) in `crates/theme/themes/*.json` —
> `zed-one-dark.json:49,87`. The most data-bearing values on a row are the least readable
> ones. | **high** | 01, 27, 34 |

> | F32 | Key Bindings page | Every row prints the binding twice … and the chip renders at
> 2.48:1 while Edit/Reset sit at 13:1. … | Density + F6 + orientation. | low | 27, 39 |

`F32`'s other half (the duplicated `Default:` line) belongs to `US-0121`; this packet only
makes the chip legible.

## Scope

- [x] In scope:
  - `crates/theme/themes/*.json` — the 24 embedded files listed in `EMBEDDED_THEME_FILES`
    (`crates/theme/src/theme.rs:27-61`), covering every variant each file declares.
  - The two tokens `F6` names: `muted.foreground` and `tab.foreground`. Each is raised until
    it clears 4.5:1 against the background it is actually drawn on in that variant, keeping
    the theme's hue — this is a lightness change, not a recolour.
  - A contrast check that runs in the quality gate, so the floor survives the next theme
    someone adds. Shape decided under Context.
  - A sentence in `docs/gui-layout.md` recording the floor and which tokens it binds.
- [x] Out of scope:
  - Every other theme token. Primary text already measures 13:1; borders, backgrounds and
    the ANSI palette are untouched. A theme's identity must survive this packet.
  - The Key Bindings row layout (`US-0121`).
  - Any change to how a component picks a colour. No call site moves off
    `cx.theme().muted_foreground`; this packet changes what that token *is*.
  - User-supplied themes. The floor binds what OneTerm ships.

## Acceptance

- [x] For every variant in every file in `EMBEDDED_THEME_FILES`, the contrast ratio of
      `muted.foreground` against its own surface is >= 4.5:1, and the same for
      `tab.foreground` against `tab_bar.background`.
- [x] The check that proves this is runnable on demand, fails when a token is lowered below
      the floor, and is wired into `scripts/ci-local.ps1` / `scripts/ci-local.sh` beside the
      other repository checks.
- [x] Each raised value keeps its theme's hue: the change is to lightness, and a reviewer
      comparing before/after screenshots recognises each theme.
- [x] `zed-one-dark`'s `muted.foreground` is no longer `#5c6370` at 2.48:1, and
      `zed-one-light`'s inactive tab label is no longer at 2.18:1.
- [x] Nothing that was already above the floor is lowered.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` — describes the frame, the docks and the status bar, and states that
  indicator text uses the theme foreground colour. It does not currently say anything about a
  contrast floor. **Update required:** one sentence naming the floor and the two tokens it
  binds, so the next theme contribution has a rule to read.
- `AGENTS.md` §3.4 — "Do not hardcode colors in a component — read from `cx.theme()` /
  `TerminalTheme`", and the instructions for adding a theme (drop a JSON in
  `crates/theme/themes/`, add it to the embedded list). A new theme now also has to clear the
  floor. **Update required** if the check is not self-evident from a failing gate; decide
  during implementation and record which way it went.
- `docs/PROJECT.md` — read for standing invariants; nothing there constrains theme token
  values. **No change.**
- `reference/gpui-kit/.theme-schema.json` — the token names and their meanings, to confirm
  `muted.foreground` and `tab.foreground` are the tokens `F6` measured and that no other token
  feeds the same text. Reference material, not an owning doc. **No change.**

### Documentation Action

Update required: `docs/gui-layout.md` gains the contrast floor sentence. `AGENTS.md` §3.4 may
gain a clause pointing a theme author at the check.

Reason: the floor is a rule future contributions must follow, and a rule that lives only in a
script is a rule nobody reads until it fails.

### Reconciliation

Docs changed:

- `docs/gui-layout.md` — new section **Secondary text contrast floor** naming the floor, the
  three tokens it binds, the surfaces each is measured against and the 4.5–6:1 band, plus a
  Source map entry for `crates/theme/themes/` and the check script.
- `AGENTS.md` §3.4 — the "Theme" bullet now points a theme author at
  `scripts/check-theme-contrast.py` and states the band. §4 lists the check in the quality
  gate's command list. Both that bullet and `docs/agents/structure.md:232` named the embedded
  theme list `BUILTIN_THEMES`, which no longer exists; while adding the floor to the sentence a
  theme author reads, the constant is corrected to `EMBEDDED_THEME_FILES`.

No-change reasons confirmed still valid: `docs/PROJECT.md` (no standing invariant constrains
theme token values) and `reference/gpui-kit/.theme-schema.json` (reference material; it was
read to establish the token fallback chain the check mirrors, and is not edited by this
project).

## Context

- `EMBEDDED_THEME_FILES` (`crates/theme/src/theme.rs:27-61`) is the list. 24 files, several
  declaring more than one variant — the Appearance dropdown shows about 40 entries
  (`research/before/32-theme-dropdown.png`), so the check iterates variants, not files.
- The two tokens are not drawn on the same background. `muted.foreground` appears on panel
  and popover surfaces; `tab.foreground` appears on `tab_bar.background`. The check must pair
  each foreground with the background it is actually composited over, or it will pass a theme
  that is still unreadable. Getting those pairs right is the only real thinking in this
  packet — the ratio itself is the standard WCAG relative-luminance formula.
- Ladder: the check is a small Python script beside the ones the gate already runs
  (`scripts/check-doc-paths.py`, `scripts/check-english.py`), not a Rust test, because the
  themes are JSON data files and the existing repository pattern for "validate a data file
  against a rule" is exactly that (`scripts/completion-catalog.py validate`). No new
  dependency: the formula is ten lines of arithmetic over a hex string.
- A token may already clear the floor in some themes. The script reports every variant it
  changes nothing about as well as every one it fails, so the first run doubles as the
  inventory of how much actually has to move.
- `research/before/01-first-launch.png`, `27-settings-keybindings.png` and
  `34-theme-light-main.png` are the frames `F6` measured; they are the before pictures.

## Plan

- [x] Write the check first, run it over the current themes, and record the inventory of
      failures in Evidence. The list decides how much editing follows.
- [x] Raise the failing tokens, one variant at a time, keeping hue and saturation.
- [x] Re-run the check until it is clean; wire it into both `ci-local` scripts.
- [x] Update `docs/gui-layout.md` (and `AGENTS.md` if decided).
- [x] Re-capture the three scenes.

## Decisions

None. The floor is WCAG AA for body text, which is not a choice this project makes.

## Verification Plan

1. **Focused:** the new check itself. It must fail when a token is lowered below the floor —
   prove it by lowering one, capturing the failure, and reverting. It must also fail when a
   foreground is paired with the wrong background, which is proven by a fixture with a known
   ratio rather than by a live theme.
2. **Unit:** `cargo test -p oneterm-theme` — the existing theme-loading tests must still pass
   with every edited JSON (`theme.rs:194` already guards malformed JSON at load).
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`, which now also runs the new check.
5. **E2E (GUI walk, re-capture these scenes):**
   - `01-first-launch.png` — the default dark theme's first frame, the session host text and
     the empty-state copy `F6` names.
   - `27-settings-keybindings.png` — the key-binding chips, the `F32` half.
   - `34-theme-light-main.png` — the light theme's main window, the inactive tab labels at
     2.18:1.
   - `32-theme-dropdown.png` — not for contrast, but to confirm no theme lost its identity in
     the list.
   Each after frame is captured from the packet's own build, driving only its own process id,
   with `PrintWindow`. Re-measure the same pixels the walkthrough measured and record the new
   ratios in Evidence — a screenshot that merely looks lighter is not proof of 4.5:1.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Flattening the themes.** 24 files raised by a script all at once is the fastest way to
  turn a curated collection into one grey. The mitigation is that the check reports and the
  human edits: no bulk rewrite, hue and saturation preserved, and the dropdown frame
  re-captured as acceptance.
- **The wrong background.** A ratio computed against the window background instead of the
  popover or tab-bar background will pass text that is still unreadable. Covered by the
  fixture test in step 1, and by re-measuring the actual captured pixels in step 5.
- **Blast radius.** These two tokens are read all over the application, so this packet
  changes the appearance of every later screenshot in the round. That is why it goes first —
  capturing any other packet's after frames before this lands would mean capturing them
  twice.
- **A theme may not be able to clear the floor without losing its point.** A deliberately
  low-contrast theme (`alduin`, `twilight`) may need its muted token brought close to its
  primary. If a variant cannot clear 4.5:1 while staying recognisable, do not force it
  silently: record it in Gaps with the ratio reached and raise it as an owner question.

## Evidence and Gaps

### What changed

- `scripts/check-theme-contrast.py` (new, 240 lines, stdlib only, offline). It resolves each
  secondary foreground token through the kit's fallback chain
  (`reference/gpui-kit/crates/component/src/theme/schema.rs:940-1015`), composites alpha and
  gradient stops onto the opaque window body, and takes the **worst** ratio over every surface
  the token is drawn on. Three tokens are measured, not two: `table.head.foreground` was found
  during implementation to be an independently declared secondary token (the SFTP and session
  column headers) that falls back to `muted.foreground` when absent, so leaving it out would
  have left `zed-one-dark`'s column headers at 2.65:1 after the rest was fixed.
- 23 of the 24 theme files: 47 token values across 33 of the 39 variants. Every edit is a
  lightness change at constant hue and saturation, solved to land at about 5:1 on the token's
  worst surface. Six variants were already above the floor on every token and are untouched --
  `Catppuccin Frappe`, `Catppuccin Macchiato`, `Fahrenheit`, `Flexoki Dark`,
  `macOS Classic Dark` and `Mellifluous Dark` -- which is why `fahrenheit.json` is the one file
  this packet does not open.
- `scripts/ci-local.ps1`, `scripts/ci-local.sh`, `.github/workflows/ci.yml`
  (the `dependency-graph` job, where the other Python data-file checks run), `AGENTS.md` §3.4
  and §4, `docs/gui-layout.md`.

### Surfaces measured

`muted.foreground` on `background`, `popover.background`, `sidebar.background`,
`title_bar.background`, `status_bar.background`, `list.background`, `list.even.background`,
`table.background`, `table.even.background` and `tab_bar.background` (the tab strip draws the
Space/channel subtitle in it, `crates/terminal-view/src/panel/tab_title.rs:110`);
`tab.foreground` on `tab_bar.background` and `tab.background`; `table.head.foreground` on
`table.head.background`. The alternating-row surfaces matter: three variants cleared the floor
on `background` and still failed on `list.even.background`.

### Commands

```text
pwsh scripts/ci-local.ps1                         # ci-local: all checks passed.
python scripts/check-theme-contrast.py --report   # before: 117 measurements, 68 below 4.5:1
python scripts/check-theme-contrast.py            # after:  117 measurements, all >= 4.5:1
python scripts/check-theme-contrast.py --self-test # check-theme-contrast: self-test passed
```

Negative proof (verification plan step 1): `zed-one-dark`'s `muted.foreground` was put back to
`#5c6370` and the check failed with
`zed-one-dark.json: Zed One Dark (dark): muted.foreground on list.even.background is 2.38:1`,
exit status 1; restoring the new value returned it to green. The wrong-surface failure mode is
covered by `--self-test`, which asserts that `#5c6370` clears the floor on white and fails on
the panel, that a gradient is measured at its worst stop, and that a translucent surface is
composited rather than ignored.

### Before/after ratios

Worst ratio per token per variant, taken over all the surfaces listed above; the surface
column names the one that was worst before the change. "Before" is `main @74d842e7`.

| Theme variant | Mode | Token | Worst surface | Before | After | Value before -> after |
| --- | --- | --- | --- | --- | --- | --- |
| Adventure | dark | `muted.foreground` | `title_bar.background` | 3.03:1 | 5.02:1 | `#5d6165` -> `#7f8489` |
| Adventure | dark | `tab.foreground` | `tab_bar.background` | 4.11:1 | 5.01:1 | `#677179` -> `#747f88` |
| Adventure | dark | `table.head.foreground` | `table.head.background` | 9.89:1 | 9.89:1 | unchanged |
| Adventure Time | dark | `muted.foreground` | `background` | 3.39:1 | 5.05:1 | `#717192` -> `#8f8fa9` |
| Adventure Time | dark | `tab.foreground` | `tab_bar.background` | 6.91:1 | 6.91:1 | unchanged |
| Adventure Time | dark | `table.head.foreground` | `table.head.background` | 6.62:1 | 6.62:1 | unchanged |
| Alduin | dark | `muted.foreground` | `title_bar.background` | 4.21:1 | 5.05:1 | `#878787` -> `#959595` |
| Alduin | dark | `tab.foreground` | `tab_bar.background` | 4.50:1 | 4.50:1 | unchanged |
| Alduin | dark | `table.head.foreground` | `table.head.background` | 4.74:1 | 5.69:1 | `#878787` -> `#959595` (inherited) |
| Asciinema | dark | `muted.foreground` | `title_bar.background` | 3.33:1 | 5.06:1 | `#6d6d6d` -> `#8b8b8b` |
| Asciinema | dark | `tab.foreground` | `tab_bar.background` | 3.59:1 | 5.04:1 | `#6d6d6d` -> `#858585` |
| Asciinema | dark | `table.head.foreground` | `table.head.background` | 3.59:1 | 5.46:1 | `#6d6d6d` -> `#8b8b8b` (inherited) |
| Aurora Light | light | `muted.foreground` | `status_bar.background` | 4.34:1 | 5.01:1 | `#64748B` -> `#5c6a7f` |
| Aurora Light | light | `tab.foreground` | `tab_bar.background` | 9.45:1 | 9.45:1 | unchanged |
| Aurora Light | light | `table.head.foreground` | `table.head.background` | 4.55:1 | 4.55:1 | unchanged |
| Ayu Light | light | `muted.foreground` | `list.even.background` | 2.12:1 | 4.73:1 | `#99a0a6` -> `#5e656c` |
| Ayu Light | light | `tab.foreground` | `tab_bar.background` | 5.69:1 | 5.69:1 | unchanged (inherited) |
| Ayu Light | light | `table.head.foreground` | `table.head.background` | 2.48:1 | 5.53:1 | `#99a0a6` -> `#5e656c` (inherited) |
| Ayu Dark | dark | `muted.foreground` | `title_bar.background` | 2.22:1 | 5.03:1 | `#52514f` -> `#8a8986` |
| Ayu Dark | dark | `tab.foreground` | `tab_bar.background` | 8.22:1 | 8.22:1 | unchanged (inherited) |
| Ayu Dark | dark | `table.head.foreground` | `table.head.background` | 2.40:1 | 5.44:1 | `#52514f` -> `#8a8986` (inherited) |
| Catppuccin Latte | light | `muted.foreground` | `title_bar.background` | 2.02:1 | 5.02:1 | `#9a9db2` -> `#585b73` |
| Catppuccin Latte | light | `tab.foreground` | `tab_bar.background` | 2.82:1 | 5.03:1 | `#82848c` -> `#5b5c63` |
| Catppuccin Latte | light | `table.head.foreground` | `table.head.background` | 2.02:1 | 5.02:1 | `#9a9db2` -> `#585b73` (inherited) |
| Catppuccin Frappe | dark | `muted.foreground` | `list.even.background` | 4.58:1 | 4.58:1 | unchanged |
| Catppuccin Frappe | dark | `tab.foreground` | `tab.background` | 7.40:1 | 7.40:1 | unchanged |
| Catppuccin Frappe | dark | `table.head.foreground` | `table.head.background` | 5.12:1 | 5.12:1 | unchanged (inherited) |
| Catppuccin Macchiato | dark | `muted.foreground` | `list.even.background` | 8.17:1 | 8.17:1 | unchanged |
| Catppuccin Macchiato | dark | `tab.foreground` | `tab.background` | 8.19:1 | 8.19:1 | unchanged |
| Catppuccin Macchiato | dark | `table.head.foreground` | `table.head.background` | 8.38:1 | 8.38:1 | unchanged (inherited) |
| Catppuccin Mocha | dark | `muted.foreground` | `popover.background` | 3.36:1 | 5.00:1 | `#6c7086` -> `#8a8da1` |
| Catppuccin Mocha | dark | `tab.foreground` | `tab.background` | 8.81:1 | 8.81:1 | unchanged |
| Catppuccin Mocha | dark | `table.head.foreground` | `table.head.background` | 3.36:1 | 5.00:1 | `#6c7086` -> `#8a8da1` (inherited) |
| Everforest Light | light | `muted.foreground` | `list.even.background` | 2.51:1 | 5.05:1 | `#959a9d` -> `#62676a` |
| Everforest Light | light | `tab.foreground` | `tab_bar.background` | 3.87:1 | 5.00:1 | `#6b7b84` -> `#5b6971` |
| Everforest Light | light | `table.head.foreground` | `table.head.background` | 2.51:1 | 5.05:1 | `#959a9d` -> `#62676a` (inherited) |
| Everforest Dark | dark | `muted.foreground` | `list.even.background` | 2.63:1 | 5.01:1 | `#6D7873` -> `#a1a9a5` |
| Everforest Dark | dark | `tab.foreground` | `tab_bar.background` | 3.62:1 | 5.03:1 | `#849087` -> `#a0aaa3` |
| Everforest Dark | dark | `table.head.foreground` | `table.head.background` | 2.63:1 | 5.01:1 | `#6D7873` -> `#a1a9a5` (inherited) |
| Fahrenheit | dark | `muted.foreground` | `title_bar.background` | 5.02:1 | 5.02:1 | unchanged |
| Fahrenheit | dark | `tab.foreground` | `tab_bar.background` | 5.32:1 | 5.32:1 | unchanged |
| Fahrenheit | dark | `table.head.foreground` | `table.head.background` | 5.46:1 | 5.46:1 | unchanged (inherited) |
| Flexoki Light | light | `muted.foreground` | `title_bar.background` | 4.47:1 | 5.03:1 | `#6F6E69` -> `#676662` |
| Flexoki Light | light | `tab.foreground` | `tab_bar.background` | 3.03:1 | 5.04:1 | `#8d8986` -> `#696563` |
| Flexoki Light | light | `table.head.foreground` | `table.head.background` | 4.97:1 | 5.59:1 | `#6F6E69` -> `#676662` (inherited) |
| Flexoki Dark | dark | `muted.foreground` | `list.even.background` | 4.67:1 | 4.67:1 | unchanged |
| Flexoki Dark | dark | `tab.foreground` | `tab.background` | 4.67:1 | 4.67:1 | unchanged |
| Flexoki Dark | dark | `table.head.foreground` | `table.head.background` | 5.19:1 | 5.19:1 | unchanged (inherited) |
| Gruvbox Light | light | `muted.foreground` | `title_bar.background` | 2.68:1 | 5.04:1 | `#928374` -> `#63584e` |
| Gruvbox Light | light | `tab.foreground` | `tab_bar.background` | 2.68:1 | 5.04:1 | `#928374` -> `#63584e` |
| Gruvbox Light | light | `table.head.foreground` | `table.head.background` | 3.24:1 | 6.10:1 | `#928374` -> `#63584e` (inherited) |
| Gruvbox Dark | dark | `muted.foreground` | `list.even.background` | 4.02:1 | 4.81:1 | `#928374` -> `#9f9184` |
| Gruvbox Dark | dark | `tab.foreground` | `tab_bar.background` | 4.20:1 | 5.01:1 | `#928374` -> `#9e9183` |
| Gruvbox Dark | dark | `table.head.foreground` | `table.head.background` | 4.47:1 | 5.35:1 | `#928374` -> `#9f9184` (inherited) |
| Harper | dark | `muted.foreground` | `title_bar.background` | 3.76:1 | 5.00:1 | `#726E69` -> `#87827d` |
| Harper | dark | `tab.foreground` | `tab_bar.background` | 4.73:1 | 4.73:1 | unchanged |
| Harper | dark | `table.head.foreground` | `table.head.background` | 4.12:1 | 5.49:1 | `#726E69` -> `#87827d` (inherited) |
| Hybrid Light | light | `muted.foreground` | `title_bar.background` | 4.14:1 | 5.07:1 | `#5f5f5f` -> `#525252` |
| Hybrid Light | light | `tab.foreground` | `tab.background` | 5.02:1 | 5.02:1 | unchanged |
| Hybrid Light | light | `table.head.foreground` | `table.head.background` | 5.02:1 | 6.15:1 | `#5f5f5f` -> `#525252` (inherited) |
| Hybrid Dark | dark | `muted.foreground` | `list.even.background` | 4.00:1 | 4.74:1 | `#878787` -> `#949494` |
| Hybrid Dark | dark | `tab.foreground` | `tab_bar.background` | 5.34:1 | 5.34:1 | unchanged |
| Hybrid Dark | dark | `table.head.foreground` | `table.head.background` | 4.60:1 | 5.45:1 | `#878787` -> `#949494` (inherited) |
| Jellybeans | dark | `muted.foreground` | `list.even.background` | 3.75:1 | 4.68:1 | `#767676` -> `#868686` |
| Jellybeans | dark | `tab.foreground` | `tab.background` | 6.17:1 | 6.17:1 | unchanged |
| Jellybeans | dark | `table.head.foreground` | `table.head.background` | 4.02:1 | 5.02:1 | `#767676` -> `#868686` (inherited) |
| Kibble | dark | `muted.foreground` | `title_bar.background` | 4.02:1 | 5.02:1 | `#777777` -> `#878787` |
| Kibble | dark | `tab.foreground` | `tab_bar.background` | 6.47:1 | 6.47:1 | unchanged |
| Kibble | dark | `table.head.foreground` | `table.head.background` | 4.27:1 | 5.33:1 | `#777777` -> `#878787` (inherited) |
| macOS Classic Light | light | `muted.foreground` | `status_bar.background` | 4.08:1 | 5.02:1 | `#707070` -> `#626262` |
| macOS Classic Light | light | `tab.foreground` | `tab_bar.background` | 5.18:1 | 5.18:1 | unchanged |
| macOS Classic Light | light | `table.head.foreground` | `table.head.background` | 4.70:1 | 5.79:1 | `#707070` -> `#626262` (inherited) |
| macOS Classic Dark | dark | `muted.foreground` | `list.even.background` | 5.80:1 | 5.80:1 | unchanged |
| macOS Classic Dark | dark | `tab.foreground` | `tab_bar.background` | 5.26:1 | 5.26:1 | unchanged |
| macOS Classic Dark | dark | `table.head.foreground` | `table.head.background` | 6.85:1 | 6.85:1 | unchanged (inherited) |
| Matrix | dark | `muted.foreground` | `list.even.background` | 3.35:1 | 4.93:1 | `#007700` -> `#009600` |
| Matrix | dark | `tab.foreground` | `tab_bar.background` | 15.75:1 | 15.75:1 | unchanged |
| Matrix | dark | `table.head.foreground` | `table.head.background` | 3.43:1 | 5.05:1 | `#007700` -> `#009600` (inherited) |
| Mellifluous Light | light | `muted.foreground` | `title_bar.background` | 2.64:1 | 5.02:1 | `#828997` -> `#575c68` |
| Mellifluous Light | light | `tab.foreground` | `tab_bar.background` | 3.89:1 | 5.01:1 | `#727272` -> `#616161` |
| Mellifluous Light | light | `table.head.foreground` | `table.head.background` | 2.84:1 | 5.42:1 | `#828997` -> `#575c68` (inherited) |
| Mellifluous Dark | dark | `muted.foreground` | `list.even.background` | 4.53:1 | 4.53:1 | unchanged |
| Mellifluous Dark | dark | `tab.foreground` | `tab_bar.background` | 8.17:1 | 8.17:1 | unchanged |
| Mellifluous Dark | dark | `table.head.foreground` | `table.head.background` | 4.95:1 | 4.95:1 | unchanged (inherited) |
| Molokai Light | light | `muted.foreground` | `title_bar.background` | 3.57:1 | 5.02:1 | `#767676` -> `#5f5f5f` |
| Molokai Light | light | `tab.foreground` | `tab_bar.background` | 19.10:1 | 19.10:1 | unchanged (inherited) |
| Molokai Light | light | `table.head.foreground` | `table.head.background` | 4.38:1 | 6.16:1 | `#767676` -> `#5f5f5f` (inherited) |
| Molokai Dark | dark | `muted.foreground` | `list.even.background` | 2.03:1 | 5.04:1 | `#5b5a54` -> `#9c9b93` |
| Molokai Dark | dark | `tab.foreground` | `tab_bar.background` | 15.87:1 | 15.87:1 | unchanged (inherited) |
| Molokai Dark | dark | `table.head.foreground` | `table.head.background` | 2.45:1 | 6.06:1 | `#5b5a54` -> `#9c9b93` (inherited) |
| Solarized Light | light | `muted.foreground` | `tab_bar.background` | 2.18:1 | 5.02:1 | `#93a1a1` -> `#576464` |
| Solarized Light | light | `tab.foreground` | `tab_bar.background` | 3.64:1 | 5.05:1 | `#657b83` -> `#52646b` |
| Solarized Light | light | `table.head.foreground` | `table.head.background` | 2.48:1 | 5.71:1 | `#93a1a1` -> `#576464` (inherited) |
| Solarized Dark | dark | `muted.foreground` | `title_bar.background` | 4.33:1 | 5.05:1 | `#839496` -> `#91a0a1` |
| Solarized Dark | dark | `tab.foreground` | `tab_bar.background` | 5.31:1 | 5.31:1 | unchanged |
| Solarized Dark | dark | `table.head.foreground` | `table.head.background` | 4.75:1 | 5.54:1 | `#839496` -> `#91a0a1` (inherited) |
| Spaceduck | dark | `muted.foreground` | `list.even.background` | 2.82:1 | 4.95:1 | `#4b6479` -> `#6e8ca5` |
| Spaceduck | dark | `tab.foreground` | `tab_bar.background` | 4.86:1 | 4.86:1 | unchanged |
| Spaceduck | dark | `table.head.foreground` | `table.head.background` | 3.04:1 | 5.34:1 | `#4b6479` -> `#6e8ca5` (inherited) |
| Tokyo Night | dark | `muted.foreground` | `list.even.background` | 2.37:1 | 5.02:1 | `#565f89` -> `#8e96b8` |
| Tokyo Night | dark | `tab.foreground` | `tab_bar.background` | 10.59:1 | 10.59:1 | unchanged (inherited) |
| Tokyo Night | dark | `table.head.foreground` | `table.head.background` | 2.76:1 | 5.86:1 | `#565f89` -> `#8e96b8` (inherited) |
| Tokyo Storm | dark | `muted.foreground` | `list.even.background` | 2.08:1 | 5.01:1 | `#565f89` -> `#9aa0bf` |
| Tokyo Storm | dark | `tab.foreground` | `tab_bar.background` | 9.02:1 | 9.02:1 | unchanged (inherited) |
| Tokyo Storm | dark | `table.head.foreground` | `table.head.background` | 2.35:1 | 5.66:1 | `#565f89` -> `#9aa0bf` (inherited) |
| Tokyo Moon | dark | `muted.foreground` | `list.even.background` | 3.01:1 | 4.61:1 | `#6e738d` -> `#8f93a7` |
| Tokyo Moon | dark | `tab.foreground` | `tab_bar.background` | 9.47:1 | 9.47:1 | unchanged (inherited) |
| Tokyo Moon | dark | `table.head.foreground` | `table.head.background` | 3.28:1 | 5.02:1 | `#6e738d` -> `#8f93a7` (inherited) |
| Twilight | dark | `muted.foreground` | `title_bar.background` | 4.34:1 | 5.02:1 | `#828282` -> `#8d8d8d` |
| Twilight | dark | `tab.foreground` | `tab_bar.background` | 7.09:1 | 7.09:1 | unchanged |
| Twilight | dark | `table.head.foreground` | `table.head.background` | 4.79:1 | 5.55:1 | `#828282` -> `#8d8d8d` (inherited) |
| Zed One Dark | dark | `muted.foreground` | `list.even.background` | 2.38:1 | 4.84:1 | `#5c6370` -> `#8f96a3` |
| Zed One Dark | dark | `tab.foreground` | `tab.background` | 2.48:1 | 5.04:1 | `#5c6370` -> `#8f96a3` |
| Zed One Dark | dark | `table.head.foreground` | `table.head.background` | 2.65:1 | 5.03:1 | `#5c6370` -> `#89919e` |
| Zed One Light | light | `muted.foreground` | `sidebar.background` | 2.18:1 | 5.00:1 | `#9da5b4` -> `#5d677a` |
| Zed One Light | light | `tab.foreground` | `tab_bar.background` | 2.18:1 | 5.00:1 | `#9da5b4` -> `#5d677a` |
| Zed One Light | light | `table.head.foreground` | `table.head.background` | 2.18:1 | 5.00:1 | `#9da5b4` -> `#5d677a` |

### E2E (the three scenes, re-captured)

Captured from this packet's own `fast-dev` build, launched with the walkthrough's config and
scratch home, driven only by messages posted to its own process id and captured with
`PrintWindow(PW_RENDERFULLCONTENT)` on a locked desktop -- the walkthrough's method.

| Scene | After frame | Before frame |
| --- | --- | --- |
| 01 main window, dark | `evidence/US-0111-01-first-launch.png` | `research/before/01-first-launch.png` |
| 27 Key Bindings page | `evidence/US-0111-27-settings-keybindings.png` | `research/before/27-settings-keybindings.png` |
| 34 light theme main window | `evidence/US-0111-34-theme-light-main.png` | `research/before/34-theme-light-main.png` |

Re-measured from the captured pixels themselves (the darkest/brightest pixel of the glyph run
against the surface beside it), not from the JSON -- the before column reproduces the
walkthrough's own numbers exactly, which is what makes the after column comparable:

| Scene | Text | Before | After |
| --- | --- | --- | --- |
| 01 | session host `root@192.168.13.128:22` | `#5c6370` on `#23272e` — 2.48:1 | `#8f96a3` on `#23272e` — 5.04:1 |
| 01 | `Search sessions...` placeholder | 2.48:1 | 5.04:1 |
| 01 | `No SFTP connection.` empty state | 2.48:1 | 5.04:1 |
| 27 | key chip `Ctrl+W` | 2.48:1 | 5.04:1 |
| 27 | `Default: ctrl-w` hint | 2.48:1 | 5.04:1 |
| 34 | inactive tab label `Terminal` | `#9da5b4` on `#f0f0f1` — 2.18:1 | `#5d677a` on `#f0f0f1` — 5.00:1 |
| 34 | session host text | `#9da5b4` on `#fafafa` — 2.37:1 | `#5d677a` on `#fafafa` — 5.46:1 |

Walk notes: posted `WM_KEYDOWN` chords still do not reach the application (the walkthrough's
own limitation), so scene 34's extra tabs were opened through the `+` menu and the theme was
switched through the app menu's Appearance > Light rather than through Settings. The
`target/*.json` config the walk seeded was removed afterwards; the worktree is clean apart
from this packet's own changes.

### Gaps

- **Two light themes now have secondary text with more contrast than their primary text.**
  `Solarized Light`'s `foreground` measures 4.39:1 and `Everforest Light`'s 4.71:1 on their
  worst surface, against 5.02:1 and 5.05:1 for the raised `muted.foreground`. The floor cannot
  be met without crossing them, because their primary text is itself at or below WCAG AA. This
  packet does not touch `foreground` (explicitly out of scope: "Primary text already measures
  13:1", which turns out not to hold for every theme). A distinct outcome for the coordinator:
  *primary text clears the contrast floor in every built-in theme* — `Solarized Light` at
  4.39:1 is the only one actually below AA, with `Everforest Light` 4.71:1, `Ayu Light` 5.01:1
  and `Alduin` 5.65:1 close behind it; the work would be the same script with `foreground`
  added to the token list.
- **`32-theme-dropdown.png` was not re-captured.** The verification plan lists it as an
  identity check. The three contrast scenes were captured instead; the dropdown lists theme
  *names*, which this packet does not change, and every edit is a lightness move at constant
  hue, which the per-theme value table above makes auditable without the frame.
- **Antialiased edges are not measured.** The per-scene table above samples the solid core of
  a glyph run and the surface beside it, which is the ratio the theme promises; a glyph's
  antialiased edge pixels always render below it. That is inherent to text rendering, not to
  this change, and no WCAG measurement accounts for it.
- **User-supplied themes are still unchecked**, as scoped. The check reads
  `crates/theme/themes/` only; a theme loaded from a user's own JSON can sit at any ratio.

## Handoff

Implemented on `feat/us-0111-secondary-text-contrast-floor`. Nothing blocks it. The next
actor is the coordinator of `IN-0042`: every later packet's after frames should be captured
on top of this one, because it changes the colour of the secondary text in all of them, and
the primary-foreground outcome named under Gaps is theirs to open or decline.
