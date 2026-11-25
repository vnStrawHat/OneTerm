# Independent verification — `BUG-0073` and `US-0135`

Date: 2026-09-22
Subject: `fix/highlight-prompt-sign-and-bench` @ `c936d11c` (two commits on `main` @ `7f4dfe30`)
Verifier: an independent session, adversarial brief, no access to the implementer's reasoning
beyond the committed packets.

## Verdicts

| Packet | Verdict |
|---|---|
| `BUG-0073` — the Windows prompt sign is decided on the head alone | **PASS with findings.** Every attack in the brief behaves exactly as the shipped rule claims. One **undocumented new false negative** was introduced (PowerShell's provider-qualified UNC prompt) and is not in the packet's "two cosmetic costs remain" list. The E2E gap the packet recorded is now closed by this verification. |
| `US-0135` — the highlighter's cost is measured, its two scan bounds settled | **PASS with findings.** The benchmark, the baseline, the numbers, the cited `FrameStats` tests, the head-of-run cost statement and the policy edge all check out. Two owning docs the packet listed as "reviewed and unchanged" are now stale, and the packet's own follow-up recommendation ("one line in `[profile.fast-dev.package]`") is **wrong as written** — measured here. |

Neither finding is a behaviour regression the user would call a bug in what shipped; both
are accuracy defects in what the packets claim.

---

## `BUG-0073`

### The rule, as shipped

`crates/highlight/src/profile.rs:129` — `WIN_PATH_BODY = [^<>|"*?:\r\n]*[^\s<>|"*?:-]`
`crates/highlight/src/profile.rs:135` — `WIN_PATH_ROOT = (?:[A-Za-z]:|\\\\)`
`crates/highlight/src/profile.rs:139` — `PWSH_PATH_ROOT = (?:[A-Za-z]+:|\\\\|/)`
`crates/highlight/src/profile.rs:159-161` — `win_path_prompt_pattern()` = `^(?:ROOT BODY >[ ]?)`
`crates/highlight/src/profile.rs:166-171` — `PROMPT_PWSH` = `^(?:PS(?: PWSH_ROOT(?:BODY)?)?>[ ]?)|(?:^>+[ ]?)`

"First `>`" is structurally forced, not enforced by a rule: `>` is excluded from the body
class, so no match can reach past the first one. Confirmed by inspection and by the
`C:\work>dir > out.txt` probe below.

The sign is then located by `prompt_sign` (`crates/highlight/src/scanner/prompt.rs:44-56`)
as the **last** prompt glyph inside the match, and the universal fallback
(`crates/highlight/src/scanner/prompt.rs:27-34`) takes only the drive/UNC half, so the bare
`>` branch stays off Unix/SSH tabs — acceptance item 4, confirmed by probe (`>>` on `Unix`
is `sign=None`).

### Adversarial probe

A throwaway integration test (`crates/highlight/tests/zz_adversarial_probe.rs`, created,
run, deleted — the tree is unchanged) drove every line in the brief plus my own through
the public `scan_line_into` on all four profiles and through `ShellProfile::prompt_regex`
on the two Windows profiles. Result — `sign` is the char index of `Class::PromptSign`, `-`
means no prompt:

| Line | `Cmd` | `PowerShell` | `Unix` | `Dumb` | Expected? |
|---|---|---|---|---|---|
| `C:\src -> C:\dst` | - | - | - | - | yes — head ends in `-` |
| `c:\proj\x.cpp(5): error C2059: syntax error: '>'` | - | - | - | - | yes — `:` past the drive |
| `C:\Program Files (x86)>` | 22 | - | 22 | 22 | yes — `(` is legal in a cwd |
| `C:\work>dir > out.txt` | 7 | - | 7 | 7 | yes — sign at the **first** `>`, redirection outside |
| `C:\build->` | - | - | - | **9** | documented false negative — but **not on `Dumb`** (see F3) |
| `PS Env:\>` | - | 8 | - | - | yes — PSDrive root |
| `PS HKLM:\Software>` | - | 17 | - | - | yes |
| `PS /home/u>` (pwsh on Unix) | - | 10 | - | - | yes |
| `PS>` | - | 2 | - | 2 | yes |
| `>>` / `>> ` | 0 (`>`) | 1 (`>>`) | - | 1 | `N4` as scoped; `N1` respected on `Unix` |
| `C:\a>http://x` | 4 | - | 4 | 4 | yes — same class as `C:\work>dir`; head decides |
| `C:\Users\a - b\dir>` | 18 | - | 18 | 18 | **yes — this is NOT a false negative** (the `-` rule is on the *last* char only) |
| `\\?\C:\x>` | - | - | - | 8 | false negative; `cmd` never prints a `\\?\` prompt |
| `C:\file.txt:stream>` | - | - | - | 18 | false negative; `:` is illegal in a Windows dir name, so unreachable as a cwd |
| `C:\log size > 3` | - | - | - | - | yes — comparison stays output |
| `D:\a\b if x > y then` | - | - | - | - | yes |
| `3 > 2`, `E:\ -> F:\`, `see C:\x> not a prompt` | - | - | - | - | yes |
| `C:\>` | 3 | - | 3 | 3 | yes |
| `c:/unix/style>ls` | 13 | - | 13 | 13 | yes — forward-slash cwd |
| `C:\Users\O'Brien>` | 16 | - | 16 | 16 | yes |
| `PS C:\src -> C:\dst` | - | - | - | - | yes |
| `PS Note: x is > 3` | - | - | - | - | yes (root eats `Note:`, body then ends in a space) |
| **`PS Microsoft.PowerShell.Core\FileSystem::\\server\share>`** | - | **-** | - | - | **no — see F1** |

Two answers the brief asked for directly:

- **`C:\Users\a - b\dir>` is not a false negative.** The `-` rule bites only the character
  immediately before the `>`; a `-` anywhere else in the cwd is fine. Ordinary
  hyphenated directory names are unaffected.
- **Every "Windows path followed by ` > `" line is output**, on every profile.

### Mutation

Dropping the `:` exclusion from `WIN_PATH_BODY` (to `[^<>|"*?\r\n]*[^\s<>|"*?-]`) fails
**two** tests, not one:

```
test profile::tests::a_drive_anchored_line_is_a_prompt_only_when_its_head_is_a_path ... FAILED
  crates\highlight\src\profile.rs:297: cmd: "c:\\proj\\x.cpp(5): error C2059: syntax error: '>'"
test scanner::scanner_tests::drive_anchored_output_with_an_unspaced_angle_bracket_is_output ... FAILED
  crates\highlight\src\scanner\scanner_tests.rs:660
test result: FAILED. 81 passed; 2 failed
```

The head rule is therefore genuinely load-bearing at both the pattern and the behaviour
layer. `profile.rs` was restored byte-for-byte (`git status` clean afterwards).

### The table test and the wrapped-arrow test

- `crates/highlight/src/profile.rs:265`
  `a_drive_anchored_line_is_a_prompt_only_when_its_head_is_a_path` — 11 rows, exact match
  extents on both Windows patterns. Passes.
- `crates/highlight/src/scanner/scanner_tests.rs:646`
  `drive_anchored_output_with_an_unspaced_angle_bracket_is_output` — behaviour on all four
  profiles, `error` keeps `Class::Error`, `C:\src` keeps `Class::Path` while the arrow does
  not. Passes.
- `crates/terminal-view/src/render/row_plan.rs:1353`
  `a_wrapped_arrow_line_is_not_a_prompt` — wrapped at 8 columns so row 2 is literally
  `> C:\dst`, `cmd`'s continuation prompt; no `PromptSign` on either row. Passes. The
  logical-line application is real: `class_rows_into`
  (`crates/terminal-view/src/render/row_plan.rs:488`) joins the wrap run before scanning
  (`scan_logical_line`, same file line 519).

---

## `US-0135`

### The benchmark and the numbers

`crates/tools/src/bin/highlight-bench.rs` (403 lines), `[[bin]]` at
`crates/tools/Cargo.toml:59-61`, dependency at `crates/tools/Cargo.toml:91`.

- 144 cells in `crates/tools/highlight-bench-baseline.json`
  (4 shapes x 4 lengths x 3 widths x 3 profiles), `runs: 9`, machine recorded as
  `Intel Core i7-12700 (12 cores / 20 threads), Windows 11 26200, rustc 1.96.0, release
  profile, 2026-09-22` — the `machine`/`note` shape `crates/tools/bench-baseline.json`
  uses. Verified.
- `--json` is opt-in (`highlight-bench.rs:302, 316-320`): a casual run cannot overwrite the
  baseline. Verified.
- No `criterion`, no `[[bench]]`, no CI gate — `scripts/ci-local.ps1` does not mention the
  binary. Verified.

**Re-run on this machine** (`cargo run -p oneterm-tools --release --bin highlight-bench --
--runs 9`, while another worktree was compiling, which is also how the baseline was taken):

| Shape | this run, 8 000 chars | baseline | delta | §10's stated figure |
|---|---|---|---|---|
| prompt | 8.8-9.0 µs | 8.9-9.2 µs | -3% .. 0% | 9 µs ✔ |
| plain | 67.8-74.1 µs | 68.8-72.7 µs | -5% .. +6% | 70 µs ✔ |
| keyword-log | 92.8-98.0 µs | 85.3-92.8 µs | +4% .. +12% | 86-93 µs — **my run sits at/just above the top of the band** |
| cjk | 504.6-524.9 µs | 505.7-558.8 µs | -8% .. +1% | 0.51-0.56 ms ✔ |

Three of four shapes reproduce inside the cell's own spread. `keyword-log` is consistently
~6% high here; the baseline's own `spread_percent` for those cells runs 4.5-18.9%, and both
runs were taken on a loaded machine, so this is inside the honest reading the packet itself
states ("any comparison narrower than a cell's own spread is the machine"). It is worth
knowing that `keyword-log` is the least reproducible row of the four.

The arithmetic §10 derives from the table is correct: 40 rows x 80 cols of the CJK shape at
~65 ns/char = 0.21 ms = 1.2% of a 16.7 ms frame; the same viewport of `plain` = 28 µs.

### The two `FrameStats` tests §10 cites

Both exist and assert exactly what §10 says, so §10 citing them instead of adding a third
test is correct:

- `crates/terminal-view/src/render/plan_cache.rs:1216`
  `class_delta_replans_the_continuation_row` — `class_rows_scanned == 2` in a 12-row
  viewport, `class_scans == 1`.
- `crates/terminal-view/src/render/plan_cache.rs:1248`
  `a_line_longer_than_the_viewport_scans_the_viewport` — `class_rows_scanned ==
  rows_total`, `class_scans == 1`.

### The head-of-run decision, and its stated cost

Every claim in the cost statement is true at the cited location:

- `SnapshotState::rows()` — `crates/vt/src/snapshot/state.rs:296`, doc comment reads
  "Always the full viewport, indexed by viewport row." The packet cites `:296` exactly.
- `Terminal::screen()` — `crates/vt/src/terminal/mod.rs:539`, `pub`.
- `Screen::row(RowId)` — `crates/vt/src/grid/screen.rs:458`, `pub`, returns `RowRef<'_>`.
- So "no *engine* API would have to change, but the view cannot reach them at plan time"
  is accurate: `class_rows_into` is handed a `&Frame` built from the snapshot, and
  `crates/terminal-view/src/render/row_plan.rs:488-515` reads nothing else.

The rule is written where the code implements it
(`crates/terminal-view/src/render/row_plan.rs:480-486`), in §10.1 and in §13 `Q5`.

### Gates

- `python scripts/verify-dependency-graph.py` — *"Dependency graph policy passed for 20
  workspace packages and 20 explicit members"*. The new `oneterm-tools -> oneterm-highlight`
  edge (`scripts/dependency-graph-policy.json:115`, under `oneterm-tools`) is allowed.
- `python scripts/vt-public-api.py --check --no-doc` **standalone fails** with
  `no rustdoc output at target/doc/oneterm_vt` — it is not a runnable check on its own; it
  requires the preceding `cargo doc -p oneterm-vt --no-deps --all-features`
  (`scripts/ci-local.ps1:96, 104`). Inside the gate it passes: `public API surface unchanged (public-api.windows.txt)`. The packet's
  "`python scripts/vt-public-api.py --check --no-doc` — green" is true only in that order;
  a reader who copies the command out of the packet gets a failure.

---

## Findings, ranked

### F1 (MAJOR, `BUG-0073`) — an undocumented new false negative: PowerShell's provider-qualified UNC prompt

When PowerShell's location is a UNC share it prints the provider-qualified form, e.g.

```
PS Microsoft.PowerShell.Core\FileSystem::\\server\share>
```

Before this change `PROMPT_PWSH` was `^(?:PS(?: WIN_PATH_BODY)?>[ ]?)` — no root required
after `PS ` — and that line matched. `PWSH_PATH_ROOT` now requires `[A-Za-z]+:`, `\\` or
`/` immediately after `PS `, and `Microsoft.` supplies none of them, so it no longer
matches on any profile (the universal fallback is drive/UNC-anchored and also misses it).
Confirmed by direct comparison of the old and new pattern strings on that line:

```
old: <re.Match span=(0, 55)>      new: None
```

`PS \\server\share>` (the form the tests carry,
`crates/highlight/src/profile.rs:320-323`) still matches — only the provider-qualified form
regressed, and that is the form PowerShell actually prints after `Set-Location \\server\share`.

Severity is cosmetic and the class is the same as the two costs already accepted, but it is
a **new** cost this change introduced, and both `WIN_PATH_BODY`'s doc comment
(`profile.rs:126-128`) and §13 `Q7` present their list of remaining costs as complete
("Two cosmetic costs remain"). Fix is one alternative in `PWSH_PATH_ROOT`, or one line in
the enumeration; either way it should not stay unrecorded.

### F2 (MAJOR, `US-0135`) — the packet's own follow-up recommendation does not work as written

`US-0135`'s gap section and §10.1 both say `fast-dev` "leaves `regex` and `aho-corasick` at
`dev`'s" and that **"one line in `[profile.fast-dev.package]` would close it"**.

`regex` is a thin API layer. The matching engines live in `regex-automata`, and the literal
prefilters in `memchr` (`Cargo.lock:6736-6745`: `regex 1.12.4 -> aho-corasick, memchr,
regex-automata, regex-syntax`). A `[profile.fast-dev.package]` entry for `regex` and
`aho-corasick` alone leaves the hot code at `opt-level = 0`. The correct set is
`regex-automata`, `aho-corasick`, `memchr` — and `regex` for completeness.

**Is it safe? Yes — measured, not argued.** I applied the four-package override to
`[profile.fast-dev.package]`, rebuilt, re-measured and reverted (`Cargo.toml` restored
byte-for-byte; `git status` clean). 8 000-char scan, `Cmd`, median of 9:

| Shape | `fast-dev` as shipped | `fast-dev` + the four | speed-up | `release`, for scale |
|---|---|---|---|---|
| prompt | 13.3 µs | 12.3 µs | 1.1x | 8.8 µs |
| plain | 450-465 µs | 77-83 µs | **5.8x** | 69 µs |
| keyword-log | 735-934 µs | 98-103 µs | **7.8x** | 95 µs |
| cjk | 6.14-6.22 ms | 0.98-0.99 ms | **6.3x** | 0.51 ms |

`fast-dev` goes from 6-13x slower than `release` to **1.1-1.9x**, and the CJK worst case
drops from 6.2 ms — worse than the 4.14 ms figure `fast-dev` was added to fix — to 0.99 ms.

**Compile cost: 14 s wall, once**, on `CARGO_BUILD_JOBS=4` with another worktree
compiling (the four crates plus the `oneterm-tools` relink). They are third-party and
pinned in `Cargo.lock`, so they never rebuild when OneTerm code changes. That is verbatim
the rationale `[profile.dev.package]` already carries for `gpui-pre`/`smol`
(`Cargo.toml:189-190`: *"they rarely change (no rebuild cost) and are never stepped through
in a debugger"*). `opt-level` changes no behaviour, nobody steps through `regex-automata`,
and every other workspace crate that uses `regex` gets the same lift for free.

**Recommendation: do it** — add `regex`, `regex-automata`, `aho-corasick` and `memchr` to
`[profile.fast-dev.package]`, in the new packet `US-0135` already scoped this to, and fix
the `Cargo.toml:225-228` comment (F5) in the same edit. `[profile.dev.package]` is worth
considering too, so `cargo test` stops paying it, but that is a bigger blast radius and
belongs in the same packet's discussion rather than assumed here.

### F3 (MINOR, `BUG-0073`) — `C:\build->` *is* a prompt on the `Dumb` profile

The documented false negative is stated unconditionally on `WIN_PATH_BODY`
(`profile.rs:126-128`) and in §13 `Q7`. It holds on `Cmd`, `PowerShell` and `Unix`, but
`PROMPT_DUMB` (`profile.rs:176`, `^[^\s]*[\$#%>»](?: |$)`) matches `C:\build->` because the
`>` ends the line — `sign=9`. Harmless (Dumb is deliberately the permissive profile) and
arguably the *better* answer, but the "is not read as a prompt" sentence is profile-blind
where it should say "on the Windows and Unix profiles".

### F4 (MINOR, `US-0135`) — two owning docs are stale, one of them listed as "reviewed and unchanged"

- `docs/agents/crate-dependency-rules.md:36-37` still reads *"it may only reach down to the
  L0 leaf `vt`"* — singular, naming `vt`. `oneterm-tools` now also depends on
  `oneterm-highlight`. `US-0135`'s Reconciliation lists this file under **"Docs reviewed and
  unchanged"** with the gloss "the tools crate may reach a leaf crate", which is the rule
  the author wished the file stated rather than the sentence it contains. The machine check
  (`scripts/dependency-graph-policy.json`) was updated; the prose rule was not.
- `docs/agents/structure.md:262` — the `tools` row's dependency list
  (`oneterm-vt, russh, russh-sftp, tokio, polling, rand, anyhow, serde, serde_json`) is
  missing `oneterm-highlight`, and its binary list is missing `highlight-bench`. This file
  is not in the packet's "Owning Docs Reviewed" at all.

Neither is caught by `scripts/check-doc-paths.py` (it checks paths, not content), so the
gate passing does not cover them.

### F5 (MINOR, `US-0135`) — the 4.14 ms figure is not actually gone

§10.1 says the `opt-level = 0` 4.14 ms figure "is gone". It survives at
`Cargo.toml:225-228`, in the comment justifying `oneterm-highlight`'s presence in
`[profile.fast-dev.package]` — which F2 shows is also the comment that is now misleading
about where the time goes. One edit closes F2 and F5 together.

### F6 (INFO, `BUG-0073`) — `C:\a>http://x` is a prompt, and that is correct

Flagged in the brief as an attack. The head `C:\a` is a plausible path, so it is a prompt
under the shipped rule — exactly as `C:\work>dir` must be (`Q7`'s counterexample). No
defect: a `cmd` user who types a URL straight after the prompt gets the same treatment as
one who types a command.

---

## GUI walk (the E2E the implementer skipped)

`BUG-0073`'s verification plan asked for a `fast-dev` `cmd` tab printing both lines, with
frames under `evidence/`. The packet marked it "E2E not run". It is run here.

**Frame:** `evidence/BUG-0073-verify-cmd-tab.png` (`fast-dev`, own build, own pid,
`PrintWindow` with `PW_RENDERFULLCONTENT`, 1616x926).

**Method** — no keyboard injection, no interference with any other process:

- `cargo build -p oneterm-app --profile fast-dev`, run from this worktree only.
- `config_dir()` is `target/` in debug builds (`crates/core/src/config/shell.rs:109-113`),
  which in `fast-dev` resolves **relative to the launched process's working directory**, so
  `target/terminal.json` in this worktree is the only config touched — the owner's
  `~/.OneTerm/` is untouched and no running OneTerm was enumerated, signalled or closed.
- That `terminal.json` sets `shell.kind = "cmd"` (so the view picks `ShellProfile::Cmd` —
  `crates/terminal-view/src/terminal_view/render.rs:615`), `utf8 = false` so the kind
  contributes no default args, and `args = ["/K", "target\\bug0073-walk.cmd"]`. The batch
  turns `echo on` and echoes the two lines, so the frame carries the prompts, the commands
  and the outputs without a single synthetic keystroke.
- `layout.semantic_highlighting = "on"`. `US-0133` is not in this branch, so OSC 133 roles
  are not populated and every row goes through the regex fallback — which is exactly the
  path `BUG-0073` fixes.
- Only the pid `Start-Process` returned was screenshotted and then stopped.

**What the frame shows** (top to bottom):

1. `D:\...\agent-ae1ddf078e5c63162>echo C:\src -^> C:\dst` — a **normal prompt**: the cwd in
   the path colour, the sign, then `echo` in the command colour and its arguments after it.
2. `C:\src -> C:\dst` (the `->` drawn as a ligature) — **output**. `C:\src` and `C:\dst` are
   coloured by the ordinary path probe, the arrow is not, and nothing before the `>` is
   filled as a prompt region. No prompt sign.
3. `D:\...>echo c:\proj\x.cpp(5): error C2059: syntax error: '^>'` — a normal prompt again,
   wrapping across two visual rows, still one prompt.
4. `c:\proj\x.cpp(5): error C2059: syntax error: '>'` — **output**, and **`error` is red in
   both places**: the colouring `BUG-0071` `N2` recorded the prompt branch eating is back.
5. A bare `D:\...>` prompt, normally coloured.

That is the packet's acceptance items 1 and 2 and the first half of item 3, observed in the
real renderer rather than in a unit test. The **E2E proof box in `BUG-0073` can be ticked**
on this evidence.

A "before" frame was not captured: it needs a second full `fast-dev` build at `main`, and
the pre-fix behaviour is already a measured record (`BUG-0071` `N2`: `PromptSign` at char 8
and char 46) plus a live mutation (above) that puts the failure back on demand.

---

## Commands run

```text
git reset --hard c936d11c                                    # branch verify/in-0044-bug-0073-us-0135
cargo test -p oneterm-highlight -p oneterm-terminal-view -p oneterm-tools
python scripts/verify-dependency-graph.py
python scripts/vt-public-api.py --check --no-doc             # standalone: fails, needs cargo doc first
cargo run -p oneterm-tools --release --bin highlight-bench -- --runs 9
cargo build -p oneterm-app --profile fast-dev
cargo run -p oneterm-tools --profile fast-dev --bin highlight-bench -- --runs 9   # x2, F2
pwsh scripts/ci-local.ps1              # last line: `ci-local: all checks passed.`
```

`fast-dev` as shipped also reproduces §10.1's fourth column: prompt 13.3 µs (stated 14),
plain 450-465 µs (stated 0.43-0.45 ms), keyword-log 735-934 µs (stated 0.73-0.78 ms), CJK
6.14-6.22 ms (stated 7.0-7.5 ms — mine is ~15% faster, still far above `release` and still
above the 4.14 ms the profile was added to fix, so the point the sentence makes stands).

`target/fast-dev` was deleted after the walk.

## Gaps in this verification

- The provider-qualified PowerShell UNC prompt (F1) was reproduced against the pattern
  strings, not against a live PowerShell on a real UNC share — none is reachable here.
- The `fast-dev` column of §10.1 was re-measured on this machine (above), but under the
  same "another worktree is compiling" condition the baseline was taken under.
- `cfg(unix)` behaviour (pwsh on Unix, `PWSH_PATH_ROOT`'s `/` branch) was verified by
  regex only; no Unix host is available here.
