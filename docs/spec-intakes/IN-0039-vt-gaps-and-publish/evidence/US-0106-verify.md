# Independent verification: US-0106 (DECRQCRA, DECRQSS, XTGETTCAP, esctest harness)

Packet: [`../US-0106-conformance-queries.md`](../US-0106-conformance-queries.md)
Owning design: [`../low-level-design/conformance-queries.md`](../low-level-design/conformance-queries.md)
Intake: [`../IN-0039.md`](../IN-0039.md) -- risk lane **high_risk** (screen read-back)

Branch under test: `feat/vt-conformance-queries` @ `f0ae8c46`, base `main` @ `c5ddad59`.
Verified in a separate worktree; nothing was committed to the implementer's branch.

> **All twelve findings were closed by the implementer on 2026-09-16**, after this branch was
> rebased onto `main` @ `4437b98e` (`BUG-0059`). The report below is preserved **as written**: it
> describes the tree at `f0ae8c46`, and the two findings that changed engine behaviour (5, the
> `DECOM` clamp; 6, the combining-scalar sum) are the reason the numbers in it moved. See
> "Verification notes closed" in the packet for the disposition of each. This verification's 33
> tests were adopted as `crates/vt/tests/verify_us0106.rs`; the two that recorded findings 5 and 6
> as *characterisations* now assert xterm's answer and keep the old number as a regression guard.

## Verdict

**PASS-WITH-NOTES.**

Everything the packet claims about the *engine* holds under adversarial, independently
written tests: 33 new integration tests, written against the public API and against
xterm's own `xtermCheckRect` source rather than against the implementer's expectations,
all pass. The checksum digits match hand-computed sums. Both `XTGETTCAP` ceilings, the
8 KiB payload ceiling, the closed-by-default gate, the scrollback exclusion, the
`BUG-0058` routing property and `DA1`/`DA2`/`DECRQM` are all as described. The
`#[non_exhaustive]` impossibility is reproduced as a compiler error, so the design
correction is evidence-based rather than asserted.

The notes are eleven findings. None invalidates a shipped reply; the two that matter
most are a defect in the **Unix half of the esctest bridge** (which no host here can
compile, so it is a code-reading finding) and a set of **documentation statements that
the code contradicts**. The packet's own unmet criterion -- esctest has never run --
stands, and this verification did not and could not retire it.

## What was verified, and how

New test file (kept in the verifier's worktree, **not committed**):

- `crates/vt/tests/verify_us0106.rs` -- 33 tests, public API only.

Reproduction of the `#[non_exhaustive]` finding (scratchpad, outside the repository):

- `<scratchpad>/ne/` -- a two-crate workspace holding a **copy** of `crates/vt` with
  `#[non_exhaustive]` added to `Config`, plus a consumer crate using
  `Config { scrollback_limit: 100, ..Config::default() }`.

External sources consulted (fetched, quoted where load-bearing):

- xterm `screen.c` -> `xtermCheckRect`, `xtermParseRect`, `validRect`
  (`ThomasDickey/xterm-snapshots`), and `ptyx.h` -> the `CSBITS` enum.
- `esctest2` `esctest/escutil.py` -> the `xterm_checksum < 279` negation inverse and the
  `expected 0 / actual 32` blank fudge; `esctest/esc.py` -> `empty()` returning `' '`
  only at `xterm_checksum >= 334`.
- `esctest2` README -> `--expected-terminal={iTerm2,xterm}` and the
  `--max-vt-level=4` invocation.

`invisible-island.net/xterm/ctlseqs` itself was unreachable from this host (the proxy
refused the connection). The XFree86 mirror of `ctlseqs` confirmed the `DECRQSS`
setting list (`" q`, `" p`, `r`, `m`) and the `DCS 1 $ r` / `DCS 0 $ r` and
`DCS 1 + r` reply shapes; the `DECRQCRA` wording could not be read there, so xterm's
source was used instead, which is the stronger reference anyway.

## Checksum worked examples

xterm at `checksumExtension: 7` is `csPOSITIVE | csATTRIBS | csNOTRIM`, i.e. in
`xtermCheckRect` the `!(mode & csATTRIBS)` attribute block is skipped, `total` (not
`trimmed`) is returned, and `total` is **not** negated. Each drawn cell contributes
`charSeen[col]`; an undrawn cell contributes `' '` because `csNOTRIM` is set.

All of the following are asserted in `verify_us0106.rs` and all pass.

| # | Grid and content | Request | Hand sum | Reply |
| --- | --- | --- | --- | --- |
| F1 | 4x8; row 1 `AB`, row 2 `CD` | `CSI 1;0;1;1;2;3 * y` | `0x41+0x42+0x20+0x43+0x44+0x20` = 330 | `DCS 1 ! ~ 014A ST` |
| F2 | 4x8; one `A` | `CSI * y` **and** `CSI 0;0;0;0;0;0 * y` | `0x41 + 31*0x20` = 1057 | `DCS 0 ! ~ 0421 ST` |
| F3 | 4x8; one `A` | `CSI 9;<page>;1;1;4;8 * y` for page 0,1,2,5,65535 | 1057, unchanged by page | `DCS 9 ! ~ 0421 ST` for every page |
| F4 | 4x8 filled with `U+4E16` | `CSI 1;0;1;1;4;8 * y` | `16*0x4E16 + 16*0x20` = 320 352; `& 0xffff` = 58 208 | `DCS 1 ! ~ E360 ST` |
| F5 | 4x8; one `U+4E16` at 1,1 | cols 1-1 / 2-2 / 1-2 | `0x4E16` / `0x20` / `0x4E36` | `4E16` / `0020` / `4E36` |
| F8 | 4x8; eight rows of `Q` pushed through | `CSI 1;0;1;1;65535;65535 * y` | `3*8*0x51 + 8*0x20` = 1272 | `DCS 1 ! ~ 04F8 ST`, identical to the exact `1;1;4;8` request |
| F12 | 4x8 | `Pb < Pt`, `Pr < Pl`, both | 0 | `DCS 1 ! ~ 0000 ST` |
| F13 | 4x8 | rectangle wholly past the grid | 0 | `DCS 1 ! ~ 0000 ST` |

The 16-bit mask (F4) is the one worth stating plainly: `320352 - 4*65536 = 58208 =
0xE360`, so the engine masks rather than saturating or widening.

## Findings

Severity: **High** blocks acceptance on its own; **Medium** should be fixed before
merge; **Low** is a correctness-of-the-record issue; **Info** is recorded context.

### 1. (Medium) `vt-esctest` loses esctest's exit status on the end-of-file path

`crates/tools/src/bin/vt-esctest.rs:161-163` breaks out of the event loop as soon as
`drain` returns `false`:

```rust
if event.readable && !drain(&mut pty, &mut term, &mut batch, &mut buffer) {
    break 'outer;
}
```

`drain` returns `false` on `Ok(0)` **and on any `Err` other than `WouldBlock` /
`Interrupted`** (`:197`, `:201`). On Linux a pty master returns `EIO` once the last
slave descriptor closes, which is exactly what happens when `esctest` exits. That path
leaves `status == None`, so `:176-179` prints

```text
vt-esctest: the harness did not exit within 900s
```

and exits **2** -- for a harness that in fact finished normally. Whether the run
reports esctest's own status or this false message depends on which of the two
level-triggered descriptors `events.iter()` yields first in the final wake-up, so it is
a race, not a certainty. The packet and the module header both promise "exit with
`esctest`'s own status".

The harness's *output* is not lost (the bytes are fed inside `drain` before the failing
read) and `esctest.log` is written by esctest itself, so the artifact survives; and the
CI job is `continue-on-error`, so nothing breaks. What is lost is the pass/fail signal
the packet says the bridge carries.

Suggested fix, one line in spirit: on `drain` returning `false`, poll
`pty.next_child_event()` (briefly, or for the remainder of a short grace period) before
falling through to the timeout branch.

**Not compiled or executed.** `rustup target list --installed` on this host offers only
`x86_64-pc-windows-msvc` and the file is `#[cfg(unix)]`, so this is a reading of the
source against `crates/vt/src/pty/unix.rs` (`set_nonblocking` at `:177`, the
`PTY_CHILD_EVENT_TOKEN` registration at `:232-249`), not a reproduction.

### 2. (Medium) The bridge overrides `XTGETTCAP`'s `TN` away from the `TERM` it sets

`crates/tools/src/bin/vt-esctest.rs:84-88` states the intent:

> `esctest` keys some expectations off `TERM`, and the engine answers `XTGETTCAP`'s
> `TN` with the same name.

and sets `TERM=xterm-256color`. Thirty lines later, `:120` sets
`product_name: Some("oneterm-vt-esctest(1.0.0)".into())`, and `query::capability`
(`crates/vt/src/terminal/query.rs:246-263`) makes `product_name`'s name half **override**
`TN`. So the terminal tells the harness `TERM=xterm-256color` through the environment
and `TN=oneterm-vt-esctest` through `XTGETTCAP`. The comment is false as written, and
any esctest assertion that compares the two will fail for a reason that has nothing to
do with the engine.

Verified empirically, on the engine side, by `xtgettcap_reports_the_product_name` (F29):
with `product_name = "OneTerm(1.2.3)"`, `DCS + q 544e ST` answers
`DCS 1 + r 544e=4F6E655465726D ST` ("OneTerm"), not `xterm-256color`.

Either drop `product_name` from the bridge or fix the comment; the first is safer for a
conformance run.

### 3. (Medium) `State::dcs_payload`'s doc comment states the ceiling the packet withdrew

`crates/vt/src/terminal/mod.rs:174-177`:

> `clear()`ed rather than dropped, so a program polling `XTGETTCAP` in a loop allocates
> once. **Bounded by the parser's existing `DCS_MAX_BYTES`, which aborts through
> `dcs_unhook` and needs no second ceiling here.**

That is the design the packet explicitly withdrew. The field is in fact bounded by
`query::QUERY_MAX_BYTES` = 8 KiB (`crates/vt/src/terminal/query.rs:31`, enforced at
`crates/vt/src/terminal/dispatch.rs:1778` and `:1804-1807`). The low-level design
carries the correction, guide chapter 11 carries the correct number, and the one comment
sitting on the field itself carries the old claim. A reader of the struct learns the
wrong ceiling and the wrong counter (the 8 KiB path increments
`unhandled_sequences`, not `aborted_dcs`).

### 4. (Medium) Guide chapter 10, "Hostile input, ceilings and counters", was not updated

`crates/vt/docs/guide/10-limits.md:13-29` is the chapter whose whole subject is the
engine's ceilings. This packet added three -- the 8 KiB query payload, 16 names per
`XTGETTCAP` request, 128 bytes per name -- and none of them is in the table. Chapter 11
describes them in prose, which is the wrong chapter for a reader looking up a limit.

Worse, `10-limits.md:99-100` now reads ambiguously:

> `aborted_dcs` -- `DCS` sequences abandoned by `CAN` or `SUB`, or for running past the
> payload ceiling.

There are now two payload ceilings with two different counters: past `DCS_MAX_BYTES`
the sequence is aborted and `aborted_dcs` rises; past `QUERY_MAX_BYTES` nothing is
aborted, `unhandled_sequences` rises and `aborted_dcs` does not. Confirmed by
`decrqss_over_long_request_answers_nothing` (F19): a 9 KiB `DCS $ q` payload gives
`unhandled_sequences == 1` and `aborted_dcs == 0`.

Chapter 10 is not in the packet's "Documentation Action" table at all, so this is a
missed doc rather than a stale one.

### 5. (Medium) Under `DECOM` the rectangle is not clamped to the scrolling region

`crates/vt/src/terminal/dispatch.rs:776-792` applies the origin-mode offset to `top` and
`bottom` and then clamps both to the **screen**:

```rust
let offset = if self.origin() { self.region().top } else { 0 };
...
let top = top.saturating_sub(1).saturating_add(offset).min(rows);
let bottom = bottom.saturating_add(offset).min(rows);
```

xterm clamps to the **region**. `xtermParseRect` defaults `top`/`bottom` to
`minRectRow(screen)` / `maxRectRow(screen)`, which under origin mode are the scrolling
margins, and `limitedParseRow` clamps an explicit row into the same span; `validRect`
then rejects anything still outside it.

Measured by `characterise_decrqcra_origin_mode_is_not_clamped_to_the_region` (F11), on a
4x8 grid holding `A`,`B`,`C`,`D` on rows 1-4 with region rows 2-3 and `DECOM` set:

| Request | xterm | this engine |
| --- | --- | --- |
| `CSI 1;0;1;1;1;1 * y` (region row 1) | `0042` (`B`) | `0042` -- agrees |
| `CSI 7;0 * y` (whole page, defaults) | rows 2-3: `0x42+0x43+14*0x20` = **0245** | rows 2-4: `0x42+0x43+0x44+21*0x20` = **0369** |
| `CSI 7;0;1;1;99;99 * y` | rows 2-3: **0245** | rows 2-4: **0369** |

The offset half of `DECOM` is right (which is the Contour bug the design cites); the
clamp half is missing. It does not widen the read beyond the visible screen -- the gate
and the scrollback exclusion still hold -- but it is a conformance deviation that an
`esctest` origin-mode group can be expected to find, and it is not named anywhere in the
design, the guide, or the packet's gaps.

### 6. (Medium) "The variant xterm reaches with `checksumExtension: 7`" is not exactly what is implemented

At extension 7, `csBYTE` is **clear**, so `xtermCheckRect` runs:

```c
if_OPT_WIDE_CHARS(screen, {
    if (!(mode & csBYTE)) {
        size_t off;
        for_each_combData(off, ld) {
            total += (int) ld->combData[off][col];
        }
    }
});
```

xterm adds every combining scalar into the total. This engine adds only the cluster's
**first** scalar (`dispatch.rs:805-811`, `CellContent::Grapheme(id) => ... .first()`).
Measured by `characterise_decrqcra_drops_combining_marks` (F6): with `? 2027` set and
`e` + `U+0301` written, xterm at extension 7 would answer `0x65 + 0x301 = 0366`; the
engine answers `0065`.

That is a defensible choice -- it is the choice that keeps `escutil.py`'s one-cell
`ord(char)` comparison working -- but three documents identify the variant as
"`checksumExtension: 7`" with no exception named:

- `crates/vt/docs/guide/11-conformance.md`, "The checksum variant, pinned";
- `crates/vt/CHANGELOG.md`, the `DECRQCRA` bullet;
- `low-level-design/conformance-queries.md`, "The checksum, and the quirk".

The guide sentence that does the work -- "the positive sum of each cell's first Unicode
scalar value" -- is accurate; the label above it is not. One clause ("except that a
grapheme cluster contributes only its first scalar, where xterm at extension 7 adds
every combining mark") would make the three agree with the code.

A second, smaller mismatch with the same label: xterm's `validRect` **rejects** a
rectangle that falls outside the page and returns 0; this engine clamps it to the page
(F13 vs `clamps_a_rectangle_from_outside_the_grid`). Harmless, and arguably friendlier,
but again unnamed.

### 7. (Low) The CI job's comment claims a commit pin the job does not have

`.github/workflows/ci.yml:359`:

> Pinned to a commit so a rewritten upstream test cannot silently change what this
> engine is scored against.

`.github/workflows/ci.yml:367`:

```yaml
ESCTEST_REF: master
```

`master` is a branch. The packet's Gaps section discloses this honestly and asks the
first CI run to replace it with the resolved sha, which is the right plan; the finding
is that the file itself asserts something false, and the file is what a reader of CI
sees. A one-word comment change ("pinned by `ESCTEST_REF`; **replace `master` with the
sha of the first green run**") would close it without waiting for that run.

Everything else about the job checks out: `continue-on-error: true` is present
(`:340`); no aggregate job `needs:` it, so it cannot gate anything (the workflow has no
`needs:` edges at all among the top-level jobs); `actions/upload-artifact` runs under
`if: always()`; the `esctest2/esctest/esctest.py` path matches the upstream layout; and
`--expected-terminal xterm --xterm-checksum 334 --max-vt-level 4` is the invocation the
design pins, with `334` confirmed load-bearing by `esc.py`'s `empty()` and `279` by
`escutil.py`'s negation inverse.

### 8. (Low) An undeclared deviation: dropped `XTGETTCAP` names are counted once, not per name

`low-level-design/conformance-queries.md:258` says:

> **Every dropped name and every over-long request increments
> `FeedStats::unhandled_sequences`**

The implementation counts **once per request** regardless of how many names were dropped
(`dispatch.rs:883-921`, the count at `:918-920`, and the doc comment at `:880-882` says so explicitly: "counted
-- **once**, not once per name, so a hostile 4 000-name request cannot drive the counter
by 4 000"). Measured by `xtgettcap_name_ceiling_drops_and_counts_once` (F27): 17 names
gives `unhandled_sequences == 1`; 4 000 names gives `1`.

The implementation's behaviour is the better one. The finding is that the packet records
two deviations from its design and this is a third, unrecorded, in a document the packet
did correct in two other places.

### 9. (Low) A dead branch in `color_parameters` contradicts its own comment

`crates/vt/src/terminal/query.rs:120-124` says a named underline colour "cannot be
reported and is dropped rather than mis-reported". `color_parameters` at `base == 58`
does not drop it: `Color::Named` falls into the shared arm at `query.rs:142-150` and produces
`58 + index` (so `Named(Black)` would emit a bare `58`, and `Named(Red)` a `59`, which
is *reset underline colour*).

The branch is unreachable today -- `SGR 58` is parsed only through `parse_sgr_color`
(`dispatch.rs:1156-1167`) and `colon_color`, both of which yield only `Color::Palette`
or `Color::Rgb` -- so nothing is mis-reported in practice. It is a trap for the next
person who makes `underline_color` settable another way. Either make the arm return
`None` for `base == 58` or drop the comment's claim.

### 10. (Low) The budget table's numbers drifted from the branch

The packet's budget block lists `crates/vt/src/terminal/dcs_routing_tests.rs | 20 +-`;
`git diff --stat c5ddad59..f0ae8c46` reports `21`. The full diff is 18 files,
2 106 insertions, 96 deletions (the packet's table covers the 14 non-intake files). The
overrun is disclosed either way; the table was evidently taken one commit early.

### 11. (Info) A bare `ESC` finishes an in-flight query, and the answer is given

`a_bare_escape_finishes_an_in_flight_query` (F33): feeding `DCS + q 436f` and then
`ESC c` (`RIS`) answers `DCS 1 + r 436f=323536 ST` before resetting -- the parser unhooks
the DCS on the `ESC`, so `dcs_unhook(aborted: false)` runs and the partial payload is
answered as if terminated. `decrqss_survives_a_split_string_terminator` (F20) shows the
same thing across feeds: the reply is emitted on the `ESC`, and the following `\` is
consumed as the `ST` final rather than printed.

This is consistent with how the engine finishes a partial Sixel and with the DEC parser
state machine, and no stray byte reaches the screen. It is recorded because it means
`Handler::clear_dcs_query` at `RIS` and `DECSTR` (`dispatch.rs:647-650`) is unreachable
in practice -- which is what its own comment claims, now demonstrated rather than argued.

### 12. (Info) The esctest criterion is unmet, and this verification did not change that

The packet marks it `NOT MET` and says it blocks acceptance. Confirmed: no matrix exists,
the branch is unpushed, the harness is `#[cfg(unix)]` and only `x86_64-pc-windows-msvc`
is installed here, so the Unix half of `vt-esctest.rs` has still been read and never
compiled. Finding 1 above is a direct consequence of that gap: a defect in that half is
exactly what nothing here can catch.

## Claim-by-claim result

| Claim | Result | Evidence |
| --- | --- | --- |
| `DECRQCRA` answers `DCS Pid ! ~ XXXX ST` only with the gate open, else counted | **holds** | F9 (no reply, `unhandled_sequences` 1 then 2), F1-F8 with the gate open |
| Gate default `false`, construction-time only, no runtime setter | **holds** | F10; `Terminal::config` returns `&Config`; the snapshot has `method config` and no `config_mut`; `grep allow_screen_readback crates/` finds it set only in `vt-esctest.rs:119` |
| Checksum = xterm `checksumExtension 7` | **holds with one exception** | F1-F5, F8, F12, F13 match hand sums; F6 shows combining marks dropped where xterm adds them -- finding 6 |
| Blank cells count as `U+0020`, no attributes, no negation, masked to 16 bits | **holds** | F2, F4 (mask), the implementer's `decrqcra_ignores_the_video_attributes` |
| Defaults when parameters are omitted or 0 = whole page; `Pid` echoed; `Pp` ignored | **holds** | F2, F3 |
| Out-of-range clamped, reversed rectangle empty, no panic | **holds** | F12, F13 (differs from xterm's *reject*, finding 6) |
| Alternate screen is what is read | **holds** | F7 |
| The scrollback cannot be reached | **holds** | F8 |
| `DECRQSS` answers `m`, `r`, `SP q`, `" q`, `" p`; everything else `DCS 0 $ r ST` | **holds** | F15-F18 |
| The SGR answer is re-feedable and reproduces the style | **holds** | F14 over nine styles including `1;38;5;196;48;2;10;20;30;4:3;58;5;99` and `58;2;255;0;255`; `Style` compared for equality, not strings |
| `DECSCUSR` reports the shape, not the visibility | **holds** | F16, shapes 1-6, each re-checked after `DECTCEM` off |
| `DECSCL` agrees with `DA1` | **holds** | F17 (`62;1` vs `\x1b[?62;4;22c`) |
| 9 KiB query payload answers nothing and is counted | **holds** | F19 (`unhandled_sequences` 1, `aborted_dcs` 0, next request still answered) |
| A split `ST` still answers exactly once | **holds** | F20 (answer lands on the `ESC`; the `\` prints nothing) |
| `CAN` aborts without answering | **holds** | F21 (`aborted_dcs` 1) |
| `XTGETTCAP` per-name replies, hex in / hex out, values as documented | **holds** | F22 (TN, Co, colors, RGB, Ms, Se, Ss, Su) and the unknowns (kbs, bce, u8, XT) |
| Multiple names in one request, one reply each, in order | **holds** | F25 |
| Odd-length / non-hex answered unknown; non-hex not echoed | **holds** | F26, including a payload carrying a real `ESC \` |
| Ceilings: 16 names, 128 bytes per name, dropped not truncated, counted | **holds** | F27, F28 (counted **once**, see finding 8) |
| Name lookup is case-sensitive | **holds** | F24 (`tn`, `co`, `rgb`, `su`, `ms`, `SS`, `SE`, `COLORS` all unknown) |
| `product_name` overrides `TN`, name half only | **holds** | F29 (and see finding 2) |
| `DCS q` still Sixel; the query families never open the decoder; other DCS still counted | **holds** | F30 (a Sixel still produces a placement; both query forms produce none; DECUDK / `DCS $ r` / `DCS + p` / `DCS ! |` still counted) |
| `DA1`, `DA2`, `DECRQM` unchanged | **holds** | F31 |
| Two queries in one feed answer in order | **holds** | F32 |
| `#[non_exhaustive]` on `Config` is impossible (E0639) | **holds, reproduced** | see below |
| CI job is report-only | **holds** | `continue-on-error: true`; no job `needs:` it |
| `ESCTEST_REF` should be a sha | **holds** | finding 7 |
| esctest never ran; corpus unchanged; budget overrun disclosed | **holds** | finding 12; `corpus_check` green; finding 10 for the stat drift |

### The `#[non_exhaustive]` reproduction

A copy of `crates/vt` with `#[non_exhaustive]` added to `Config`, plus an external
consumer crate:

```rust
use oneterm_vt::Config;
pub fn build() -> Config {
    Config { scrollback_limit: 100, ..Config::default() }
}
```

```text
error[E0639]: cannot create non-exhaustive struct using struct expression
 --> consumer\src\lib.rs:5:5
  |
5 | /     Config {
6 | |         scrollback_limit: 100,
7 | |         ..Config::default()
8 | |     }
  | |_____^
```

So the withdrawal is correct and the corrected text in
`low-level-design/api-surface.md` ("Rust forbids a struct expression for a
non-exhaustive struct outside the defining crate entirely, functional-update syntax
included") is accurate. Guide chapter 12's new paragraph says the same thing in the
right place and keeps the count at eight. The correction's closing note -- that
`BUG-0059` and `US-0105` must re-check the *structs* among their planned marks -- is
the right consequence to have drawn.

## Records

- **Packet ticks.** The two unmet criteria are marked unmet and both say why: the `esctest`
  run (`[ ] ... NOT MET -- the run has not happened`) and the budget. The guide-chapter-11
  criterion is marked `[~]` with the reason (rewritten from the code, not from a run). That
  is an honest set of ticks; no criterion is claimed that this verification could not
  confirm.
- **Guide chapter 11** was read in full against the code. Its statements match, with the one
  exception in finding 6 (the `checksumExtension: 7` label) and the missing note about
  clamping vs xterm's rejection. Its three new doctests compile and run
  (`cargo test -p oneterm-vt --doc`, 38 pass).
- **Guide chapter 10** is the gap in finding 4: not updated, and not in the packet's
  documentation table.
- **Guide chapter 12 and the CHANGELOG** both amend clause 6 identically (`DA3`, `DECRQCRA`,
  `DECRQSS`, `XTGETTCAP` added) and both explain why `Config` is not marked. They agree with
  each other and with the corrected `api-surface.md`.
- **The reconciliation grep** the packet ran was re-run here and is still clean: no file
  under `crates/vt/` describes any of the three as unimplemented, unanswered or uncountable.
- **The harness snippet** was checked against the real database, read-only, through a copy
  (`harness.db` was not modified). `intake` row `id = 44` exists with `document_number = 39`
  and the IN-0039 summary, so `intake_id=44` is right. Every column the snippet names
  exists on `story`; `risk_lane='high_risk'`, `status='implemented'`, the four proof flags
  as `0`/`1` and `last_verified_result='pass'` all satisfy the table's `CHECK` constraints;
  `created_at` is `NOT NULL DEFAULT (datetime('now'))`, so omitting it is fine. No `US-0106`
  row exists yet, so the `INSERT` will not collide. The proof flags (`unit 1`,
  `integration 1`, `e2e 0`, `platform 0`) match the packet's `HARNESS:PROOF` block and match
  what was observed here.

## Commands run

```text
git reset --hard feat/vt-conformance-queries            # f0ae8c46, merge-base c5ddad59
$env:CARGO_BUILD_JOBS = 3

cargo test -p oneterm-vt --test verify_us0106           # 33 passed
cargo test -p oneterm-vt                                # lib 528, +33 verifier
cargo test -p oneterm-vt --no-default-features          # lib 504, +33 verifier
cargo test -p oneterm-vt --all-features                 # lib 533, +33 verifier
cargo test -p oneterm-vt --features vt-paranoid         # lib 528, +33 verifier
cargo test -p oneterm-tools                             # pass
cargo fmt --all -- --check                              # clean (after formatting the new test)
pwsh scripts/ci-local.ps1 -Full                         # see below
```

`ci-local -Full` covers `cargo test --workspace` (and therefore `corpus_check`, the
46-recording replay), `vt-public-api.py --check --no-doc` and `--diff-platforms`,
`check-english.py`, `check-doc-paths.py`, `verify-dependency-graph.py`, the rustdoc
self-containment grep, `cargo package -p oneterm-vt --list` and `cargo deny`.

**Result: `ci-local: all checks passed.` (exit 0)** -- all 25 steps, run once, in order,
against the final tree **including** this verification's new test file, so `cargo fmt
--all -- --check`, both `clippy` passes, `cargo test --workspace` and `check-english.py`
all saw it. That closes the packet's own honesty note about its `ci-local` run having had
`fmt` and the first `clippy` execute against a pre-fixup tree: this run had no fixups.

Individually confirmed from that log: `cargo test --workspace` green (including
`corpus_check`, the 46-recording replay); `cargo test -p oneterm-vt --features
vt-paranoid` green; `--no-default-features` and `--all-features` builds, examples and
tests green; `cargo run -p oneterm-vt --example headless` green;
`RUSTDOCFLAGS=-D warnings cargo doc --all-features` green; `vt-public-api.py --check
--no-doc` reports no drift and `--diff-platforms` passes; both rustdoc self-containment
greps pass over `crates/vt/src` and `crates/vt/docs/guide`; `cargo package -p oneterm-vt
--list` satisfies the dependency-graph policy; `check-doc-paths.py`, `check-english.py`,
`completion-catalog.py validate`, `third-party-notices.py --check` and `cargo deny check
licenses bans advisories` all pass.

## What could not be verified

- **`esctest` itself.** Linux-only harness, Windows host, unpushed branch. No outside
  conformance number exists, and none was produced here.
- **The Unix half of `crates/tools/src/bin/vt-esctest.rs`.** Never compiled: only
  `x86_64-pc-windows-msvc` is installed and no cross-check target is available. Finding
  1 is a source reading, not a reproduction, and there may be more where it came from.
- **The CI job as executed.** The YAML was read and its shape checked (report-only,
  artifact upload, path to `esctest.py`, the pinned invocation); it has never run.
- **`invisible-island.net/xterm/ctlseqs`.** The proxy refused the connection, so
  `DECRQCRA`'s prose specification was taken from xterm's source rather than from
  `ctlseqs`. `DECRQSS` and `XTGETTCAP` were confirmed against the XFree86 `ctlseqs`
  mirror.
- **xterm's own answer for a double-width spacer column.** `HIDDEN_CHAR` is not defined
  in `ptyx.h` and the rest of the xterm tree was not fetched, so F5 pins *this engine's*
  numbers (glyph `0x4E16`, spacer `0x20`) without a confirmed xterm comparison.
- **Whether `esctest` compares `TERM` against `XTGETTCAP`'s `TN`.** Finding 2 is a
  contradiction inside the bridge's own source; whether it costs a test group is
  unknown until the job runs.
