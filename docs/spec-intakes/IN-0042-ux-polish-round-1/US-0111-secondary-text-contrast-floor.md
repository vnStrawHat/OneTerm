# Work: Secondary text meets a 4.5:1 contrast floor in every built-in theme

ID: US-0111
Intake: IN-0042
Created: 2026-09-17

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [x] In progress
- [ ] Implemented
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

- [ ] In scope:
  - `crates/theme/themes/*.json` — the 24 embedded files listed in `EMBEDDED_THEME_FILES`
    (`crates/theme/src/theme.rs:27-61`), covering every variant each file declares.
  - The two tokens `F6` names: `muted.foreground` and `tab.foreground`. Each is raised until
    it clears 4.5:1 against the background it is actually drawn on in that variant, keeping
    the theme's hue — this is a lightness change, not a recolour.
  - A contrast check that runs in the quality gate, so the floor survives the next theme
    someone adds. Shape decided under Context.
  - A sentence in `docs/gui-layout.md` recording the floor and which tokens it binds.
- [ ] Out of scope:
  - Every other theme token. Primary text already measures 13:1; borders, backgrounds and
    the ANSI palette are untouched. A theme's identity must survive this packet.
  - The Key Bindings row layout (`US-0121`).
  - Any change to how a component picks a colour. No call site moves off
    `cx.theme().muted_foreground`; this packet changes what that token *is*.
  - User-supplied themes. The floor binds what OneTerm ships.

## Acceptance

- [ ] For every variant in every file in `EMBEDDED_THEME_FILES`, the contrast ratio of
      `muted.foreground` against its own surface is >= 4.5:1, and the same for
      `tab.foreground` against `tab_bar.background`.
- [ ] The check that proves this is runnable on demand, fails when a token is lowered below
      the floor, and is wired into `scripts/ci-local.ps1` / `scripts/ci-local.sh` beside the
      other repository checks.
- [ ] Each raised value keeps its theme's hue: the change is to lightness, and a reviewer
      comparing before/after screenshots recognises each theme.
- [ ] `zed-one-dark`'s `muted.foreground` is no longer `#5c6370` at 2.48:1, and
      `zed-one-light`'s inactive tab label is no longer at 2.18:1.
- [ ] Nothing that was already above the floor is lowered.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

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

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

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

- [ ] Write the check first, run it over the current themes, and record the inventory of
      failures in Evidence. The list decides how much editing follows.
- [ ] Raise the failing tokens, one variant at a time, keeping hue and saturation.
- [ ] Re-run the check until it is clean; wire it into both `ci-local` scripts.
- [ ] Update `docs/gui-layout.md` (and `AGENTS.md` if decided).
- [ ] Re-capture the three scenes.

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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
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

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
