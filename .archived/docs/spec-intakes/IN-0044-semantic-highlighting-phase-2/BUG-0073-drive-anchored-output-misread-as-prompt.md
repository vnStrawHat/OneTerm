# Work: Drive-anchored output with an unspaced `>` is not a prompt

ID: BUG-0073
Intake: IN-0044
Created: 2026-09-22

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

- [x] `C:\src -> C:\dst` (what `mklink` and `dir /AL` print) is output: no `PromptSign`, and
      no `Path` region imposed by the prompt branch.
- [x] `c:\proj\x.cpp(5): error C2059: syntax error: '>'` is output, and `error` keeps its
      `Class::Error` colouring.
- [x] Every prompt shape already guarded keeps working, as a table test over all of them:
      `C:\Users\John Doe\projects>` and its wrapped form (`F2`), `PS C:\path>` and `PS>`
      (`F1`), a UNC prompt `\\server\share>`, the bare `>` and `>>` continuations on the
      Windows profiles only (`N1`), `C:\work>dir > out.txt` with the redirection outside the
      prompt region, and `C:\log size > 3` still output.
- [x] The bare `>` branch stays out of `UNIVERSAL_PROMPT`, so a leading `> ` on a Unix or SSH
      tab (a mail quote, a blockquote, `git log` body, diff context) is still output
      (`BUG-0071` `N1`).
- [x] The rule is applied to the joined logical line, so a wrapped `C:\...\src -> ...` is
      rejected on the same grounds as an unwrapped one.
- [x] The comment on `WIN_PATH_BODY` is updated: it currently records this false positive as
      an accepted cost, which it no longer is. `N3` stays recorded there.
- [x] `cargo test -p oneterm-highlight -p oneterm-terminal-view` passes.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

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

**Answered.** The fix landed in the patterns, not at the sign, so §5's one-line description
of `ShellProfile::prompt` still holds and is unchanged. The rule is worth stating once
where a reader meets the fallback, so §4.2 now carries it in a block quote, and §13 gained
`Q7` for the half of the obvious rule that had to be rejected.

### Reconciliation

Docs changed:

- `docs/terminal-semantic-highlighting.md` §4.2 — the Windows sign rule, in one block
  quote, plus why it is applied to the logical line.
- `docs/terminal-semantic-highlighting.md` §13 `Q7` — "what follows the Windows prompt sign
  is not part of the rule", with the head-test table and the two remaining cosmetic costs.
- `crates/highlight/src/profile.rs` — the `WIN_PATH_BODY` doc comment now states the rule
  and no longer records this false positive as accepted; `BUG-0071` `N3` stays recorded
  there, joined by the hyphen case this fix adds and by the profiles each cost applies to.
  `PWSH_PATH_ROOT` carries why the provider qualifier belongs to the root.

Docs reviewed and unchanged: §5 (it names no specific pattern), §4.1, and `BUG-0071`
(its `N2` is this packet; `N1`/`N3` are respected and asserted).

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

- [x] Write the failing table test first, over both measured examples and all existing
      prompt shapes, on `Cmd`, `PowerShell` and `Unix`.
- [x] Try the cheaper character-class rule and the token rule; keep the smaller one that
      passes the whole table.
- [x] Verify the fix applies to the joined logical line, with a wrapped case in the table.
- [x] Update the `WIN_PATH_BODY` doc comment; keep `N3` recorded.

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
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

**The rule, reconciled.** The packet proposed "the sign is the last non-space character, or
is followed by a space and then a command". That cannot stand as written, and neither can
the sharper form "the first `>` whose head is a path *and* whose next character is a space
or the end": `cmd.exe` writes `C:\work>` and the typed command lands in the very next cell,
so `C:\work>dir` — a committed `BUG-0071` `F2` case — has no space after its sign.
Requiring one would trade two cosmetic false positives for a false negative on every `cmd`
prompt being typed at. The shipped rule is therefore stated on the **head** alone:

> The prompt sign is the first `>` of the logical line whose head — everything before it —
> is a plausible Windows prompt path: rooted at a drive (`C:`) or a UNC share (`\`),
> optionally behind PowerShell's `PS `; containing none of `< > | " * ? :`; and ending in
> neither a space nor `-`.

"First" is forced, not chosen: `>` cannot occur in a Windows path, so the head of any later
`>` contains one and is never plausible — which is exactly what keeps the redirection in
`C:\work>dir > out.txt` outside the prompt region.

Both measured false positives fall to the head test alone, which is why the cheaper
character-class mechanism was kept and no token test after the sign was written:
`C:\src -> C:\dst`'s head ends in `-`, and the MSVC diagnostic's head carries a `:` past
the drive — a character no path component may hold, which also disposes of its `(5)`
without banning `(`, since `C:\Program Files (x86)>` is an ordinary prompt and is in the
table.

**Changed.** `crates/highlight/src/profile.rs`: `WIN_PATH_BODY` excludes `:` and may not
end in `-`; the root moved out of the patterns into `WIN_PATH_ROOT` (`C:` / `\`) and
`PWSH_PATH_ROOT` (those plus a PSDrive `Env:` / `HKLM:` and, on pwsh for Unix, `/`), so the
`PS ` branch obeys the same head rule instead of an untightened body.

**Commands.**

- `cargo test -p oneterm-highlight` — 83 passed, 0 failed.
- `cargo test -p oneterm-terminal-view` — 364 passed, 0 failed, 3 ignored.
- `pwsh scripts/ci-local.ps1` — `ci-local: all checks passed`.

**Proof.**

- Unit: `profile::tests::a_drive_anchored_line_is_a_prompt_only_when_its_head_is_a_path`
  (the match extents of both Windows patterns over both false positives and every prompt
  shape `BUG-0071` established, plus `PS Env:\`, `>>` and `C:\Program Files (x86)>`), and
  `scanner_tests::drive_anchored_output_with_an_unspaced_angle_bracket_is_output` (the
  behaviour: no `PromptSign` on `Cmd`/`PowerShell`/`Unix`/`Dumb`, `error` keeps
  `Class::Error`, the arrow is not `Path`).
- Integration: `row_plan::tests::a_wrapped_arrow_line_is_not_a_prompt` — `C:\src -> C:\dst`
  wrapped at 8 columns, whose second row is `> C:\dst` and would be `cmd`'s continuation
  prompt if the rule were applied per visual row.
- Platform: `pwsh scripts/ci-local.ps1`.

**E2E — closed by independent verification** (`evidence/BUG-0073-US-0135-verify.md`,
frame `evidence/BUG-0073-verify-cmd-tab.png`). A `fast-dev` `cmd` tab, own build and own
pid, with `shell.kind = "cmd"` and `semantic_highlighting = "on"` so every row goes through
the regex fallback this packet fixes. The frame shows `C:\src -> C:\dst` and
`c:\proj\x.cpp(5): error C2059: syntax error: '>'` as output — no prompt sign, no `Path`
region before the `>`, `C:\src`/`C:\dst` coloured by the ordinary path probe, the arrow
not — with `error` red in both places, and the real prompts around them coloured normally.
That is acceptance items 1 and 2 and the first half of item 3, in the real renderer. No
"before" frame: it needs a second full `fast-dev` build at `main`, and the pre-fix
behaviour is already a measured record (`BUG-0071` `N2`) that a live mutation reproduces on
demand.

**Rework after verification.** `F1`: requiring a root after PowerShell's `PS ` dropped the
**provider-qualified** prompt `PS Microsoft.PowerShell.Core\FileSystem::\\server\share>`,
the form PowerShell prints once the location is not a plain drive — it matched the *old*
pattern, so this was a new, unrecorded cost rather than an accepted one. `PWSH_PATH_ROOT`
now admits a `<module>\<provider>::` qualifier before the root (the qualifier ends at `::`,
which is why it belongs to the root: the body excludes `:`), and the table test carries the
UNC and the drive form plus `PS Env:\>` and `PS HKLM:\Software>`. `F3`: the remaining-cost
sentence was profile-blind — `C:\build->` *is* a prompt on `Dumb`, which is deliberately
the permissive profile — and now says so, on `WIN_PATH_BODY` and in §13 `Q7`.

**Gaps.**

- Two cosmetic costs remain on `Cmd`/`PowerShell`/`Unix`, recorded in §13 `Q7` and on
  `WIN_PATH_BODY`: a cwd ending in a space (`BUG-0071` `N3`) or in a hyphen (`C:\build->`)
  is not read as a prompt. A hyphen elsewhere in the cwd is fine — `C:\Users\a - b\dir>` is
  a prompt. `N4` (the first `>` of a `>>` continuation) is untouched, as scoped.
- The provider-qualified PowerShell prompt was fixed and tested against the pattern, not
  against a live PowerShell on a real UNC share — none is reachable from this machine.
- `PWSH_PATH_ROOT`'s `/` branch (pwsh on Unix) is verified by regex only; no Unix host
  here.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
