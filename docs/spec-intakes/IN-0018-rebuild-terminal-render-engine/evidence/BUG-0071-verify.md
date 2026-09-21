# Trace: independent verification of `BUG-0071`

ID: BUG-0071-verify
Intake: IN-0018
Date: 2026-09-21
Subject: `3c0e9976` "fix(terminal-view): classify a wrapped line as one logical line"
(two commits on `main` @ `39a6d853`)
Reviewer: a second session, no prior context on the change, adversarial brief.

## Verdict

**PASS with findings.**

The mechanism is right and the packet's description of it is accurate: every file:line the
packet cites resolves to what it says it does, the joined-scan / scatter / cache-delta design
is sound, and the incremental class cache is byte-for-byte identical to a from-scratch scan
across scrolling, reflow and edits in every case I could construct. Two mutations of the fix
kill the named tests. The defect the owner reported is really fixed for the case that was
captured.

The findings are not about whether the fix works. They are about **three real defects in the
adjoining scanner that the packet's "review everything" audit cleared**, and **two acceptance
items that are ticked but are not what the code does**:

- a PowerShell prompt is never detected at all, so Acceptance item 1 ("a `cmd.exe` **or
  PowerShell** prompt … is highlighted exactly like a short prompt") is false for half of
  what it claims;
- a Windows cwd containing a space (`C:\Users\John Doe\…>`) is not detected as a cmd prompt
  either — the same population the owner's report comes from;
- the scanner's byte→char map is not a byte→char map, so any non-ASCII char on a line shifts
  every keyword/structural class to the wrong column — and the fix makes the blast radius a
  whole wrap run instead of one row;
- Acceptance item 2's "a quoted string opened on row 1 stays `String` on row 2" does not
  happen on a command line, because command mode classifies no arguments at all;
- "the rescan … never the viewport" is not the guarantee the code gives: the guarantee is
  "the wrap run", and a wrap run can be the whole viewport.

None of these is a reason to revert. All of them are reasons for follow-up packets, and two
of the Acceptance ticks should be qualified.

## What was checked, and where

| Claim in the packet | Verified at | Result |
| --- | --- | --- |
| Root cause: `classify` scanned one visual row | `git show main:crates/terminal-view/src/render/row_plan.rs` — `fn classify` at 452, `row.text_into(` at **457**, `overlay.scan_into(&scratch.line_text, row.index(), …)` at **463** | **Confirmed.** The packet cites 457 for the `scan_into` call; 457 is the `text_into` call, `scan_into` is 463. Six lines of citation drift, nothing more. |
| `scan_line_into` got the row straight | `crates/terminal-view/src/highlight/overlay.rs:74` (pre-fix, `role_at(display_row)` at :70) | **Confirmed, exact.** |
| cmd prompt regex fails on the first and last visual row | `crates/highlight/src/profile.rs:99` `^(?:[A-Za-z]:[^\s>]*>[ ]?)\|(?:^>[ ]?)` | **Confirmed, exact.** A row with no `>` fails the first branch; a row `fore>` has no drive letter, and `crates/highlight/src/scanner/prompt.rs:22` `UNIVERSAL_PROMPT` does not rescue it either. |
| `plan_cache.rs` applied the wrap-run rescan to URL masks only | `crates/terminal-view/src/render/plan_cache.rs:211-226` (post-fix) vs. `main` | **Confirmed.** The class pass is added inside the same `self.scan` block, `class_prev`/`class_cur` beside `mask_prev`/`mask_cur`. |
| `mark_scan_runs` closes runs in both directions | `crates/terminal-view/src/render/plan_cache.rs:275-301` | **Confirmed.** `while start > 0 && connected(start - 1)` walks *up* as well, so the only run a rescan can enter mid-way is one cut by display row 0. |
| `RowRoles` is never populated (OSC 133 fast path dead) | only writer is `RowRoles::default()` at `crates/terminal-view/src/highlight/overlay.rs:34`; only readers at `:74-75`. No setter anywhere under `crates/` | **Confirmed.** Pre-existing; already recorded in `high-level-design.md:363`. Deserves its own packet, as the packet says. |
| `prompt_line_bg` is never painted | written at `crates/terminal-view/src/highlight/bridge.rs:53`, read only by the "any style set" check at `crates/highlight/src/theme.rs:113` | **Confirmed.** Pre-existing, same HLD line. Its own packet. |

## How it was attacked

All of the below ran against `3c0e9976` with a temporary test module appended to
`render::row_plan::tests` and `render::plan_cache::tests`, plus a `#[cfg(test)]`
`PlanCache::classes(r)` accessor. The module was removed again before the gate; the
assertions that matter are quoted inline below so they can be re-created.

Frames came from the **real VT engine** (`FrameBuilder::…build_with_fixture()` then
`fixture.feed(bytes)`), not from hand-set wrap flags, so the wrap points, the wide cells and
the spacers are the engine's.

### (a) Wide (CJK) cells and multi-byte tokens at the boundary — **scatter is correct,
scanner is not**

Two tests, both green on the fixed tree:

```rust
// the same text at 20 columns (wraps over 5 rows) and at 200 (one row):
// every *character* must get the same class. Columns cannot be compared across
// widths, because a wide char that will not fit the last column is moved whole.
let line = "echo \"日本語 error テスト mixed\" /etc/hosts https://x.test/ok";
assert_eq!(trimmed(char_classes(&fx, &fed(8, 20, line.as_bytes()))),
           trimmed(char_classes(&fx, &fed(1, 200, line.as_bytes()))));

// every WIDE_CHAR column and its spacer carry the same class, on every row
for (col, cell) in row.cells().enumerate() {
    if cell.flags.contains(CellFlags::WIDE_CHAR) {
        assert_eq!(classes[r][col], classes[r][col + 1]);
    }
}
```

The `(row, col)` scatter in `scan_logical_line` (`row_plan.rs:533-549`) stays aligned: it
carries a per-char source row from `FrameRow::append_text_into`, writes through
`out.get_mut(col)` so an out-of-range column is skipped rather than panicking, and paints the
spacer column from the same class. **A wrap can never split a multi-cell char** — the engine
moves a wide char that does not fit onto the next row and leaves the trailing column a
spacer — so the case the brief asks about does not arise in a frame the engine produced.

What *is* broken lives one crate over, and is exposed by exactly this input:

```rust
let mut out = Vec::new();
SemanticOverlay::new(ShellProfile::Unix, true).scan_into("日本語 error here", 0, &mut out);
// got: [0,0,0,0,0,0,0,0,0,0,4,4,4,4]  ← Class::Error on "here"
// want:[0,0,0,0,4,4,4,4,4,0,0,0,0,0]  ← Class::Error on "error"
```

See finding **F3**.

### (b) A wrap run whose head is above the viewport — **no panic, and stable**

`mark_scan_runs` closes runs upward, so display row 0 is the only row a rescan can enter
mid-run; `class_rows_into` (`row_plan.rs:495-503`) then starts a fresh logical line there.
That is the documented `US-0092` viewport limit and it behaves.

```rust
// 5x24, a wrapped prompt + command in the scrollback, scrolled one row at a
// time back and forward, comparing the cache against a full rescan each step
for back in 1..=4 { fixture.scroll_back(1); …;
    assert_classes_match_a_full_rescan(&h, &h.cache, &frame, &format!("scrolled back {back}")); }
for fwd in 1..=4 { fixture.scroll_forward(1); …; }
```

Green: 8 scroll steps, every row byte-identical to a from-scratch scan. No panic. The
colours of a run whose head is off-screen do change as it scrolls — that is the limit the
packet records, not a defect.

### (c) A run longer than the viewport — **no panic, cache exact, scope is the viewport**

```rust
let frame = fed(5, 20, format!("user@host:~$ echo \"{}\" end", "x".repeat(400)).as_bytes());
// 5 rows, every row 20 classes, no panic
```

and at the cache level, a 10x20 grid holding one 400-char line, one keystroke appended:

```
FrameStats { rows_total: 10, rows_candidate: 2, rows_planned: 2, url_rows_scanned: 10, … }
```

The classes still match a full rescan. The *scanned* scope is all ten rows, because the wrap
run is all ten rows. See finding **F5**.

### (d) Reflow — **correct; no old-width buffer survives**

A width change makes `restyled` true (`plan_cache.rs:134-135` compares `GridSize`), which
drops every `RowKey`, so every row is dirty, so every row is in `self.scan`, and
`class_rows_into` does `out.clear(); out.resize(frame.row(r).len(), …)` before writing. The
old `class_prev` shape therefore cannot be read at the new width. Exercised against the real
engine's reflow:

```rust
for cols in [20u16, 30, 18, 60] {
    fixture.terminal().resize(Size { rows: 6, cols }, ResizePolicy::default());
    …; assert_classes_match_a_full_rescan(&h, &h.cache, &frame, &format!("{cols} cols"));
}
```

Green at every width, on a line whose wrap points move each time.

### (e) The class-delta replan — **it really is the run, when the run is short**

The shipped test asserts `url_rows_scanned <= 2` on a **3-row** grid, which is nearly
vacuous. Re-run on a 12-row grid whose wrapped line is rows 4-5:

```rust
rewrite_row(&mut frame, &mut fixture, 4, "echo  aaa");
let stats = h.update(cx, &frame, key);
assert_eq!(stats.url_rows_scanned, 2);   // exactly the run, not the 12-row viewport
assert_eq!(stats.rows_planned, 2);
```

Green. The claim holds. Its wording does not — see **F5**.

### (f) Performance (§10) — **no quadratic path; worst case measured**

There is no bench harness, as the packet says. Reading the code first: `class_rows_into`
walks `range` once, splits it into runs, and calls `scan_logical_line` **once per run**;
`scan_logical_line` appends each row of the run once and calls `scan_line_into` once. The
caller (`plan_cache.rs:201-226`) calls `class_rows_into` once per contiguous `self.scan`
block. There is **no per-row rejoin of the run** anywhere — the shape the brief asks about
does not exist.

A timing probe on the worst case the brief names (a minified-JSON-style single logical line
filling a 40x200 viewport = 8 000 chars):

| Work | Chars scanned | Debug build, 20-run mean |
| --- | --- | --- |
| The whole viewport as one logical line | 8 000 | **4.14 ms** |
| One row alone (`39..40`, the run's tail) | 200 | **0.10 ms** |

Read that as: **per changed frame, the cost is the whole wrap run, and per keystroke on a
viewport-long line the whole run is the changed run**, so ≈8 000 chars and ≈4 ms in a debug
build. On `main` the same keystroke classified only the dirty rows — ≈400 chars. That is a
~20x rise for this (rare) shape, and it is the price of correctness, not a bug.

Two things make it worse than the table suggests and are worth recording:

- `oneterm-highlight` is **not** in `[profile.fast-dev.package]` (`Cargo.toml:207-219`), so
  the scanner runs at `opt-level = 0` in both `dev` and `fast-dev` — the 4.14 ms number is
  close to what a `fast-dev` build actually pays. `release` optimizes it.
- §10's cost table still budgets "one `aho_corasick` find_iter over **≤200 chars**" *per
  line*. After this change a line is up to viewport-width × rows. See **F9**.

For ordinary content — a wrapped prompt is 2-4 rows — the packet's argument is right and the
call count genuinely falls (one `scan_line_into` per run instead of one per row).

### (g) `RowRoles` and `prompt_line_bg` — **both confirmed dead, both pre-existing**

```
$ rtk proxy grep -rn "row_roles" crates/
crates/terminal-view/src/highlight/overlay.rs:18,34,74,75        (field, default, read, read)
$ rtk proxy grep -rn "prompt_line_bg" crates/
crates/highlight/src/theme.rs:54,61,113   crates/terminal-view/src/highlight/bridge.rs:53,131
```

No setter for `row_roles` exists, so `scan_into`'s `role_at(first_row)` branch is unreachable
and every line falls through to the prompt regex. `prompt_line_bg` is parsed from the theme
and read only by an "is anything set" predicate; nothing paints it. Both are **pre-existing
gaps, correctly scoped out**, and both were already recorded at
`high-level-design.md:363` before this packet. They should be filed as their own packets, as
the Handoff says. Note the consequence for this fix: because the OSC 133 path is inert, the
*entire* correctness of a wrapped prompt rests on the regexes in `profile.rs` — which is
where F1 and F2 live.

### (h) The PowerShell and Unix profiles — **the Unix profile hides the bug, PowerShell has one**

Running the unit tests' logic through the other profiles is where the two prompt findings
came from (F1, F2). It also showed that the shipped prompt test is insensitive — F4.

### Mutations

| Mutation | Result |
| --- | --- |
| `class_rows_into`: `let end = start;` (scan per visual row again) | `wrapped_line_classifies_like_the_same_text_unwrapped`, `a_string_that_straddles_a_wrap_is_one_run`, `a_hard_newline_is_not_joined`, `editing_one_row_reclassifies_the_whole_logical_line`, `class_delta_replans_the_continuation_row` **FAIL**. `a_prompt_that_wraps_keeps_its_sign_and_command` **passes** → F4. |
| `plan_cache.rs`: delta on the mask only (drop `\|\| class_cur != class_prev`) | `class_delta_replans_the_continuation_row` **FAILS**, as claimed. |
| `plan_cache.rs::shift`: stop rotating `class_prev` with its rows | **The whole shipped suite stays green** → F7. |

Both files were restored with `git checkout --` before the gate.

## Findings, ranked

### F1 (major) — a PowerShell prompt is never a prompt, so Acceptance item 1 is false for it

`crates/highlight/src/profile.rs:103-105`

```rust
/// PowerShell prompt: `PS C:\path>` or `>>`.
static PROMPT_PWSH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:PS[^\s>]*>[ ]?)|(?:^>+[ ]?)").expect("PowerShell prompt regex is valid")
});
```

`[^\s>]*` forbids whitespace, and a real PowerShell prompt is `PS` **space** `C:\path>`. The
doc comment on line 102 names the very shape the pattern cannot match. The universal
fallback (`crates/highlight/src/scanner/prompt.rs:22-28`) needs a drive letter at column 0 or
a `$`/`#`/`%` sign, so it does not rescue it. `crates/terminal-view/src/terminal_view/render.rs:616`
maps `ShellKind::PowerShell | ShellKind::Pwsh` to this profile, so this is what every
PowerShell tab gets.

Measured on the fixed tree, `ShellProfile::PowerShell`, the prompt wrapped over four rows of
a 20-column grid:

```
line  = r#"PS C:\Users\verification\AppData\Local\Temp\oneterm\ws> echo "a b" -Force"#
flat[line.find('>')] == 15 (Class::Operator)   want 1 (Class::PromptSign)
```

There is no test for `PROMPT_PWSH` anywhere in `crates/highlight`.

**Impact.** Acceptance item 1 of the packet is ticked and reads "A `cmd.exe` **or
PowerShell** prompt whose cwd pushes it past the row width is highlighted exactly like a
short prompt". For PowerShell it is neither highlighted like a short prompt nor like a
prompt: short or wrapped, it is scanned as output. The packet's own Gaps section says "no
PowerShell tab was captured" — that gap is load-bearing, not cosmetic.

The *visible* damage on a real PowerShell tab is smaller than the classification error,
and the GUI frames say why: `BUG-0071-verify-after-05-powershell-prompt.png` shows the cwd
correctly blue on all three rows (the output-mode path probe finds it once the logical line
is joined — that is this fix working), and in `-after-06-…` the command and its quoted string
are coloured by **PSReadLine's own SGR**, which §9's merge policy deliberately keeps. What is
missing is the prompt sign: `>` does not get `PromptSign` the way it does on the cmd tab, and
nothing after it is ever `Command`/`Option` from OneTerm. On a shell without its own input
colouring — or for OSC 133 work later — the error is total.

**Action.** File a BUG against `crates/highlight`: `PROMPT_PWSH` should be something like
`^(?:PS ?[^>]*>[ ]?)|(?:^>+[ ]?)`, with tests. Qualify Acceptance item 1 to cmd.exe and the
Unix profiles until then.

### F2 (major) — a Windows cwd with a space is not a cmd prompt either

`crates/highlight/src/profile.rs:98-100`, same `[^\s>]*`. `C:\Users\John Doe\Documents\…>` —
the default shape of a Windows profile directory when the account name has a space — does not
match `PROMPT_CMD`, and `UNIVERSAL_PROMPT` repeats the identical first branch, so it fails
too. Verified:

```rust
line = r#"C:\Users\John Doe\Documents\oneterm workspace\deep>echo hi"#
flat[line.find('>')] == 15 (Class::Operator)   want 1 (Class::PromptSign)
```

**Impact.** The owner's report is literally "the cwd is too long and wraps". A long cwd very
often *is* `C:\Users\<First Last>\…`. For those users `BUG-0071` changes nothing: the line
still falls into output mode, and it will still look "intermittent" as the wrap point moves,
because output-mode matchers are position-sensitive in a way prompt mode is not. This is not
a defect *in* the fix, but it means the fix does not close the report for a large slice of
the population, and nothing in the packet says so.

**Action.** Same BUG as F1. The cmd branch should tolerate spaces before the final `>`
(e.g. anchor on the last `>` rather than forbidding whitespace), with tests.

### F3 (major) — `byte_to_char_map` maps char index to char index

`crates/highlight/src/scanner/mod.rs:100-109`

```rust
fn byte_to_char_map(s: &str) -> Vec<usize> {
    let mut map: Vec<usize> = Vec::with_capacity(s.len() + 1);
    let mut char_index = 0;
    for _ in s.char_indices() {   // ← once per CHAR, not once per BYTE
        map.push(char_index);
        char_index += 1;
    }
    map.push(char_index);
    map
}
```

The loop runs once per char, so `map == [0, 1, …, n]` and `LineText::char_index(byte)`
returns `byte` (clamped to `n`). It is only correct while the line is pure ASCII. Every
byte-matched class — the aho-corasick keyword automaton and the structural regexes in
`scanner/structural.rs` — is therefore written at `byte_offset` instead of `char_index`, i.e.
shifted right by the number of extra UTF-8 bytes before it, then clamped at the end of the
line. Reproduced above: `"日本語 error here"` paints `here` as `Error`.

**Impact.** Pre-existing — the file is untouched by this diff. But it is squarely inside what
this packet audited: the audit table clears `crates/highlight/src/scanner/**` with "Line-
oriented by construction … No change needed", and §13 **Q4** — the open question about CJK
alignment — concludes the char↔column mapping is "the single source of truth". The mapping in
the *view* is correct; the char indices the *scanner* emits are not, so Q4's conclusion does
not hold end to end. And `BUG-0071` enlarges the blast radius: before, one non-ASCII char
corrupted the classes of one visual row; now it corrupts the whole wrap run.

**Action.** File a BUG: push `char_index` once per **byte** of each char
(`for (b, c) in s.char_indices() { for _ in 0..c.len_utf8() { map.push(idx) } … }`), with a
CJK test. Add a wrapped-CJK case to this packet's regression set.

### F4 (medium) — the prompt test the packet names does not detect the defect

`crates/terminal-view/src/render/row_plan.rs:1272-1291`
(`a_prompt_that_wraps_keeps_its_sign_and_command`) is the only test for the headline symptom,
and it passes with `scan_logical_line` mutated back to per-row scanning. Its fixture is
`ShellProfile::Unix` (`Fixture::new` hardcodes it, `row_plan.rs:662`) and its text is
`user@host:/srv/customer/acme/backend/services/gateway$ echo hello` at 20 columns, so the
tail row is `es/gateway$ echo hel` — which the Unix pattern
`[^\s]*[@:~/\]][^\s]*[\$#%](?: |$)` matches **on its own**. The row is recognised as a prompt
with or without the fix.

The root cause the packet documents is specifically about `PROMPT_CMD` failing on the first
*and* the last visual row. **No test in the repository constructs a `ShellProfile::Cmd` or
`::PowerShell` overlay.** The reported case is unguarded: a future refactor can put the
per-row scan back and only the string/newline tests will notice.

**Action.** Add a cmd-profile wrapped-prompt case (mine is in this trace, §(h)/F1-F2) to
`row_plan.rs`'s `BUG-0071` block.

### F5 (medium) — "never the viewport" is not the guarantee

`crates/terminal-view/src/render/plan_cache.rs:172-173` and the packet's Performance section
both say the rescan is "never the whole viewport". The guarantee the code gives is *the
dirty rows closed under wrap runs*; when one logical line is longer than the viewport that
closure **is** the viewport. Measured: 10x20 grid, one 400-char line, one keystroke →
`url_rows_scanned: 10` of `rows_total: 10`.

Acceptance item 7 ("the rows a frame classifies stay the dirty rows closed under their wrap
runs, never the viewport") is self-contradictory for this shape. The bound is correct and
tight; only the prose is wrong.

**Action.** Reword §10 / the packet to "bounded by the wrap run, which for a line longer than
the viewport is the viewport", and carry the 8 000-char / ~4 ms worst case from §(f).

### F6 (medium) — "a quoted string … stays `String`" is not what a command line does

`crates/highlight/src/scanner/command.rs:62-70`: in command mode "arguments stay Default" —
no `String`, no `Path`, no `Number`. So on a **prompt** line (which a wrapped prompt now
correctly is), a quoted argument and a path argument get no class at all.

This is visible in the packet's own after-frame,
`evidence/BUG-0071-after-02-wrapped-command.png`, and reproduced independently in
`evidence/BUG-0071-verify-before-02-wrapped-command.png` vs `-verify-after-02-…`: on `main`
`"error log upload"` is **cyan** (`String`) and `C:\Temp\report\error-2026.log` is
blue/red/white; after the fix both are **plain white**. The packet phrases that as "the torn
path is whole" — true, but it is whole because it is now uniformly *unclassified*.

The behaviour is design-conformant (§4.1) and the packet's real invariant — wrapped text
classifies exactly as the same text unwrapped — holds. But Acceptance item 2 is ticked with
the words "a quoted string opened on row 1 stays `String` on row 2", and that only happens
on **output** lines; the shipped test that proves it
(`a_string_that_straddles_a_wrap_is_one_run`) uses an output line, not a command line. A
reader of the packet will expect coloured strings after a wrapped prompt and will not get
them.

**Action.** Qualify Acceptance item 2 ("on an output line") and note in §9/§4.1 that command
arguments are deliberately unclassified, so the fix *reduces* colour on a wrapped command
line relative to `main`. If that is not wanted, it is a separate packet.

### F7 (minor) — the `class_prev` rotation added by the fix is untested

`crates/terminal-view/src/render/plan_cache.rs:336-342`. Disabling the rotation leaves the
entire shipped suite green; only a cache-vs-full-rescan scroll walk catches it. A regression
here would ship a stale-colour-after-scroll bug silently.

**Action.** Add the scroll walk from §(b) (it is the class analogue of the existing
`url_v2_scrolling_the_viewport_keeps_the_masks_exact`), together with a
`#[cfg(test)] PlanCache::classes(r)` accessor.

### F8 (minor) — citation drift

The packet's Root cause cites `crates/terminal-view/src/render/row_plan.rs:457` for the
`overlay.scan_into(&scratch.line_text, row.index(), …)` call. On `main`, 457 is
`row.text_into(`; the `scan_into` call is 463. `profile.rs:99`, `overlay.rs:74` and
`prompt.rs:34` are all exact.

### F9 (minor) — §10's cost table was not reconciled with the change

`docs/terminal-semantic-highlighting.md:388-392` still budgets the keyword and regex passes
"over ≤200 chars" *per line*, and §10's prose "Only visible viewport lines are lexed
(≤~50/frame)". §8 and Q5 were amended for the logical-line unit; the §10 table was not, and
it is the table Acceptance item 7 points at.

### F10 (trivial) — class scans are counted as URL scans

`FrameStats::url_rows_scanned` / `url_scans` (`plan_cache.rs:198, 210`) now also cover the
class pass. The `terminal-diagnostics` overlay therefore reports semantic scanning under a
URL label. Harmless, but the next person reading the counter will be misled.

### Noted from reading, not measured

A `LEADING_WIDE_CHAR_SPACER` — the blank the engine leaves in the last column when a wide
char will not fit — is a spacer, so `FrameRow::append_text_into` skips it and no class is
ever written to that column. Its class stays `Default`. For a class that only sets a
foreground this is invisible — the cell is blank. For one that carries a decoration or a
background (§9: `Error`/`Warn`/`Url` are underlined) it would leave a one-cell hole at the
wrap boundary. I did not construct a frame that shows it, so this is a code-reading note,
not a measured defect; it is worth a look when §8 item 6 (`prompt_line_bg`) is implemented,
because that is when a per-class background starts painting.

## GUI walk

Own `fast-dev` build, own pid, own window handle; the app was closed with `WM_CLOSE` posted
to that handle and its `Process` object, never by image name. `USERPROFILE` pointed at a long
scratch path so the `cmd.exe` prompt wraps, reproducing the owner's case. **Both runs used the
same home path**, so the text is byte-identical and only the binary differs: "before" is the
same build with the four source files reverted to `main` (`git apply -R`) and relinked,
"after" is `3c0e9976` unmodified. Window 1000x640, then 760x640 for the narrowed frame.
Frames were taken with `PrintWindow(PW_RENDERFULLCONTENT)` rather than `CopyFromScreen`,
because the desktop was locked and a screen grab returned the lock screen.

| Frame | Before (`main`) | After (`3c0e9976`) |
| --- | --- | --- |
| `-before-01-wrapped-prompt.png` / `-after-01-…` | the third row of the cwd, `home-walk-verification-long-profile`, is **white with orange `-` separators** — the tail of the same path, scanned as output | the whole three-row path is **blue**, `>` is the prompt sign |
| `-before-02-wrapped-command.png` / `-after-02-…` | `curl` white, `"error log upload"` **cyan** (`String`, from output mode), `C:\Temp\report\` blue + `error` red + `-2026.lo` white | `curl` **pink** (`Command`), `--retry` orange (`Option`), the quoted string **white**, the path **uniformly white** — the fix is working *and* **F6** is visible |
| `-before-03-wrapped-output.png` / `-after-03-…` | same line after more typing | same |
| `-before-04-narrowed.png` / `-after-04-…` | at 760 px the *second* row of the path is now the white/orange one — a different row than at 1000 px. This is the "intermittent" the owner reported | all three rows blue at both widths; nothing changes but the wrap point |
| `-after-05-powershell-prompt.png`, `-after-06-powershell-command.png` | — | the PowerShell tab. The path is blue across all three rows (the output-mode path probe now sees the whole logical line), but `>` is **not** the prompt-sign colour it is on the cmd tab: **F1** in the product |

The `gecho` artifact in frames 03/04 is the posted-`WM_CHAR` driver racing the shell's
line editor — the same pitfall the original packet recorded. Both runs were driven by the
identical script, so the pair is still comparable.

## Commands

```
git reset --hard 3c0e9976
$env:CARGO_BUILD_JOBS=6
cargo test -p oneterm-highlight -p oneterm-terminal-view
#   oneterm-highlight:     71 passed; 0 failed
#   oneterm-terminal-view: 358 passed; 0 failed; 3 ignored
pwsh scripts/ci-local.ps1
#   26 steps, exit 0, final line: "ci-local: all checks passed."
cargo build -p oneterm-app --profile fast-dev                # for the GUI walk
```

Temporary, reverted before the gate: a `#[cfg(test)] PlanCache::classes(r)` accessor and
12 added tests (`v_*`) in `render::row_plan::tests` and `render::plan_cache::tests`, plus the
three mutations listed above.

## Gaps in this verification

- **No measured `release` timing.** The §(f) numbers are a debug build; `oneterm-highlight`
  is `opt-level = 0` in `dev` and `fast-dev` alike, so they are representative of a `fast-dev`
  run but pessimistic for `release`.
- **Unix (`cfg(unix)`) behaviour is unverifiable on this host**, as always for this repo.
- **Scrollback beyond the viewport** is out of the contract and was not attacked beyond the
  eight scroll steps in §(b).
- The GUI walk is driven by posted `WM_CHAR` messages, so keystrokes can race the shell the
  same way the original packet's walk did.
