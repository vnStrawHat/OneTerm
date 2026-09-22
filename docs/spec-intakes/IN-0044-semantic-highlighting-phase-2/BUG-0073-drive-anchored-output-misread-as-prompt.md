# Work: Drive-anchored output with an unspaced `>` is not a prompt

ID: BUG-0073
Intake: IN-0044
Created: 2026-09-22

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug
- Risk lane: normal
- Spec Intake, when required: `IN-0044` — `docs/spec-intakes/IN-0044-semantic-highlighting-phase-2/IN-0044.md`

Independent of the other packets. It is a defect in the prompt-regex fallback, which
`US-0133` demotes but does not remove: every shell without OSC 133 still reaches it.

## Source

`BUG-0071` gap `N2`, measured there and left as an accepted cosmetic trade, and re-raised by
the owner on 2026-09-22 as one of the open highlighting points.

## Outcome

A line of output that begins with a drive letter or a UNC root and happens to contain an
unspaced `>` is coloured as output. The two measured examples stop being prompts, and every
prompt shape `BUG-0071` `F1`/`F2`/`N1` established stays a prompt.

## Scope

- [ ] In scope:
  - `crates/highlight/src/profile.rs` — `WIN_PATH_BODY` (line 111) and the patterns built
    from it: `PROMPT_CMD`, `win_path_prompt_pattern`, `PROMPT_PWSH`.
  - `crates/highlight/src/scanner/prompt.rs` — `looks_like_prompt` / `prompt_sign` and the
    `UNIVERSAL_PROMPT` fallback, if the tightening belongs at the sign rather than in the
    pattern.
  - `docs/terminal-semantic-highlighting.md` §5 (`ShellProfile`'s prompt detector) if the
    stated rule changes.
- [ ] Out of scope:
  - `BUG-0071` `N3` — a cwd that legally ends in a space (`C:\trailing >`) is still not a
    prompt. It is the other side of the same rule and is not what the owner reported.
  - `BUG-0071` `N4` — the first `>` of a `>>` continuation is not the sign. Cosmetic,
    unrelated to this rule, and still recorded there.
  - The Unix and Dumb profiles, which are not drive-anchored and are not affected.
  - Populating roles from OSC 133 — `US-0133`.

## Outcome detail — the rule

`WIN_PATH_BODY` admits every character but `< > | " * ? CR LF` and forbids only a *trailing*
space, so any drive- or UNC-anchored line whose first `>` is not immediately preceded by a
space is read as a prompt and the whole region before the `>` is filled with `Path`. The
tightening is at the sign, not in the body: a `>` is a prompt sign only when it is

- the **last non-space character of the logical line** (an empty prompt awaiting input), or
- followed by a space and then a command, which for this purpose is a token that is not
  itself a path.

`C:\src -> C:\dst` fails both: its `>` is followed by a space and then `C:\dst`. The cheaper
equivalent — reject a body whose last character is `-` — covers the same measured case for a
character-class test instead of a token test, and the packet picks whichever is smaller once
both are written; the acceptance criteria are stated on the behaviour, not on the mechanism.
`c:\proj\x.cpp(5): error C2059: syntax error: '>'` fails because its `>` is inside quotes and
is followed by `'` rather than a space, and is not the last non-space character either.

"Logical line" is load-bearing. The check runs on the joined wrap run, which is what the
scanner already receives since `BUG-0071`; applying it per visual row would reintroduce that
defect.

## Acceptance

- [ ] `C:\src -> C:\dst` (what `mklink` and `dir /AL` print) is output: no `PromptSign`, and
      no `Path` region imposed by the prompt branch.
- [ ] `c:\proj\x.cpp(5): error C2059: syntax error: '>'` is output, and `error` keeps its
      `Class::Error` colouring.
- [ ] Every prompt shape already guarded keeps working, as a table test over all of them:
      `C:\Users\John Doe\projects>` and its wrapped form (`F2`), `PS C:\path>` and `PS>`
      (`F1`), a UNC prompt `\\server\share>`, the bare `>` and `>>` continuations on the
      Windows profiles only (`N1`), `C:\work>dir > out.txt` with the redirection outside the
      prompt region, and `C:\log size > 3` still output.
- [ ] The bare `>` branch stays out of `UNIVERSAL_PROMPT`, so a leading `> ` on a Unix or SSH
      tab (a mail quote, a blockquote, `git log` body, diff context) is still output
      (`BUG-0071` `N1`).
- [ ] The rule is applied to the joined logical line, so a wrapped `C:\...\src -> ...` is
      rejected on the same grounds as an unwrapped one.
- [ ] The comment on `WIN_PATH_BODY` is updated: it currently records this false positive as
      an accepted cost, which it no longer is. `N3` stays recorded there.
- [ ] `cargo test -p oneterm-highlight -p oneterm-terminal-view` passes.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/terminal-semantic-highlighting.md` §5 (the `ShellProfile` prompt detector as the
  fallback for shells without OSC 133), §4.1 (the prompt and command states and what the
  prompt region does to the classes inside it), §4.2 (the fallback's place once OSC 133 is
  live).
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/BUG-0071-semantic-highlight-unstable-on-wrapped-lines.md`
  — the "Rework after independent verification" table (`F1`, `F2`, the new regexes) and gaps
  `N1`-`N4`. `N2` is this defect, and `N1`/`N3` are the constraints the fix must not break.
- `crates/highlight/src/profile.rs` and `crates/highlight/src/scanner/prompt.rs` — the
  patterns and the sign-location rule, both of which carry the rationale in their doc
  comments.

### Documentation Action

Choose one and explain it before implementation:

- No contract change expected: §5 describes the prompt detector as the fallback and names no
  specific pattern, so tightening the pattern does not change what the document promises. If
  the fix lands at the sign rather than in the regex, §5's one-line description of
  `ShellProfile::prompt` may need a clause — decide when the shape is known and record the
  answer here.

Reason: the defect is a false positive inside an implementation detail the contract
deliberately does not pin. The rationale lives in the code's own doc comments, which this
packet must update because they currently record the false positive as accepted.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- The two examples are measured, not hypothetical: `BUG-0071` `N2` records the
  `PromptSign` at char 8 for `C:\src -> C:\dst` and at char 46 for the MSVC diagnostic,
  where it also loses the `error` colouring.
- The false positive is the deliberate other side of the rule that keeps `C:\log size > 3`
  as output: the body may hold spaces, so only a space immediately before the `>` rejects a
  line. Any fix must keep both.
- The prompt region does more than colour the sign: `windows_prompt_path`
  (`crates/highlight/src/scanner/prompt.rs`) fills the region before the sign with `Path`,
  which is why a false positive is visible across the whole line rather than on one
  character.

## Plan

- [ ] Write the failing table test first, over both measured examples and all existing
      prompt shapes, on `Cmd`, `PowerShell` and `Unix`.
- [ ] Try the cheaper character-class rule and the token rule; keep the smaller one that
      passes the whole table.
- [ ] Verify the fix applies to the joined logical line, with a wrapped case in the table.
- [ ] Update the `WIN_PATH_BODY` doc comment; keep `N3` recorded.

## Decisions

None. The rule is a tightening of an existing pattern and binds no future work.

## Verification Plan

- Unit: the table test above, in `crates/highlight`, plus the existing prompt tests
  unchanged.
- Integration: a `crates/terminal-view` row-plan case for a wrapped `->` line, so the
  logical-line application is proved where `BUG-0071` proved the rest.
- E2E: a `fast-dev` `cmd` tab running `mklink` (or printing the same text) and an MSVC-style
  diagnostic, before and after frames under `evidence/`.
- Platform: `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
