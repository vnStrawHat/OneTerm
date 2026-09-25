# BUG-0079 adversarial verification

- Subject: `1f1914a4` `fix(vt): key implicit OSC 8 links by URI so repainting one is
  idempotent` and `d993d89d` `test(corpus): declare correction C16 for the implicit-link
  identity change`, on top of `main` @`e8b16a74`.
- Packet: [`../BUG-0079-implicit-osc8-links-reinterned-on-every-repaint.md`](../BUG-0079-implicit-osc8-links-reinterned-on-every-repaint.md)
- LLD: [`../low-level-design/osc8-interning.md`](../low-level-design/osc8-interning.md)
- Date: 2026-09-25. Host: Windows 11, MSVC, `CARGO_BUILD_JOBS=2`, worktree-local target dir.
- "Before" figures were built from `e8b16a74`'s `crates/vt/src` checked out over the fix, then
  restored. Every probe below was a throwaway (a counting-allocator example, extra test
  modules) and was removed before the gate ran.

## Verdict: PASS

Option A removes the growth at its source. 70,000 repaints of one implicit link hold 1
hyperlink entry and 2 extras entries (before: 65,535 and 65,535, 20.56 MB). The link, a later
explicit link and a later Sixel image all keep their cells. Sharing one id across occurrences
of the same implicit URI is allowed by the OSC 8 specification. The user-visible difference
from alacritty is limited to two same-URI runs that touch on one row, which is exactly
iTerm2's documented heuristic. `RIS` emptying the extras table leaves no dangling id. The
negative control fails with `65535`. `1f1914a4` alone failed the parity gate (F4).
`d993d89d` declares that one difference as `C16`, and the full gate is green at `d993d89d`.
The findings are Low or Info, apart from one Medium residual gap in the same class (F7),
which is pre-existing and out of this packet's scope.

## Findings

### F1 (Info): spec conformance, one id per implicit URI is allowed

The specification (Egmont Koblinger, "Hyperlinks (a.k.a. HTML-like anchors) in terminal
emulators") fixes identity only for explicit ids: "Character cells that have the same target
URI and the same nonempty `id` are always underlined together on mouseover." For links
without an id it leaves grouping to the terminal:

> For hyperlink cells that do not have an `id` (or have an empty `id`, these two are
> interchangeable), the terminal emulator does some heuristics in figuring out which cells
> belong together. Here VTE and iTerm2 differ, but from a practical point of view, this
> difference should not matter. (VTE automatically assigns a new unique `id` whenever it
> encounters an OSC 8 with a URI but without `id`. [...] iTerm2 looks at the onscreen
> contents and connects those cells that are next to each other, lack the `id`, but point to
> the same URI.)

The before behaviour was VTE's and alacritty's: one id per occurrence. With the fix, the
engine hands out one id per URI. OneTerm's only consumer that groups by id is
`terminal-view`'s hover and Ctrl+click (`crates/terminal-view/src/url/detect.rs`, which walks
a contiguous same-id run within one row). Together they reproduce iTerm2's heuristic exactly:
cells that are next to each other, lack the id, and point to the same URI. The specification
names that heuristic as acceptable.

The user-visible difference, concretely:

- `see https://a.example and https://a.example`, where each URL is its own
  `OSC 8 ;; https://a.example` run. There is **no difference**. The unlinked ` and ` cells
  break the run, so hovering either link underlines only that link, before and after.
- The same two links on different rows, as in the corpus `hyperlinks` recording. There is
  **no difference**, because hover never crosses rows.
- `ESC]8;;U ESC\ foo ESC]8;; ESC\ ESC]8;;U ESC\ bar ESC]8;; ESC\` prints `foobar` with two
  touching runs to one URI. **Before**, hovering `foo` underlined `foo`. **After**, it
  underlines `foobar`, and Ctrl+click's SEC-03 label is `foobar`. Both open the same URI.
  VTE would split the two runs and iTerm2 would merge them.

An embedder that groups by `HyperlinkId` across the whole screen, rather than by adjacency,
would now underline every on-screen occurrence of one implicit URI. The rustdoc on
`HyperlinkTable::intern` and the CHANGELOG say so ("send `id=` to keep them apart"), and
`Hyperlink::implicit` lets such an embedder apply adjacency itself. Declaring the corpus
difference (F4) is therefore right: this is a permitted behaviour, not a regression.

Per-occurrence alternatives were considered and are not needed. One is "reuse only when the
previous open of that URI is still the most recent implicit link". The other is "reuse the id
the cells already carry". Either would keep VTE grouping, but the first still leaks when a
frame alternates two links, and the second needs a grid read on every open. The specification
does not ask for either.

### F2 (Info): the explicit `id=1` aliasing is fixed, and nothing promises the old behaviour

Before, `index` held implicit links under the key `("<counter>", uri)`, so
`OSC 8 ; id=1 ; X` after the first implicit link to `X` resolved to that implicit entry. The
two maps are now disjoint (`intern.rs` `implicit` and `index`). A search of `crates/` and
`docs/` for per-occurrence wording turns up only historical records:
`US-0074-cell-and-style.md:303`, `US-0074-verify.md:210`, and the IN-0029 ladder text at
`dispatch-and-modes.md:312`, which the new "Amended by" paragraph follows. The
`corpus_replay.rs:187` comment is still accurate: implicit ids are renumbered by first
appearance. No test asserts the aliasing. `intern_tests.rs` and `terminal_tests.rs` now
assert the opposite.

### F3 (Low, pre-existing): an empty `id=` is not treated as "no id"

The specification says an empty id and no id "are interchangeable". The dispatcher
(`dispatch.rs:1916`) passes `Some("")` for `OSC 8 ; id= ; U`, so the link is interned as the
explicit pair `("", U)`. The throwaway test `v_params_and_empty_id` gives these ids:
`;U` → 1 (implicit) and `id=;U` → 2 (explicit, id `""`). The two do not share an entry, and two
touching runs of them would not merge on hover. This is bounded, because `("", U)` also
deduplicates per URI, so it leaks nothing. It predates the fix. Suggested fix, one line in
the dispatcher: `.filter(|id| !id.is_empty())`.

### F4 (Info): parameters are handled safely, and C16 is the designed mechanism

- Parameters: `id=x:foo=bar;U` and `foo=bar:id=x;U` resolve to the same entry, and
  `foo=bar;U` and `;U` resolve to the same implicit entry (`v_params_and_empty_id`). Unknown
  keys are never stored, and never part of either key. The first `id=` wins. This follows the
  specification ("Currently only the `id` key is defined").
- The corpus: at `1f1914a4`, `cargo test -p oneterm-tools --test corpus_check` **failed**
  (`hyperlinks: grid: row 29 col 0..2 hyperlink: expected "#1~https://example.com", got
  "#0~..."`), while `main`'s engine passed the same test. The recording prints two separate
  implicit links to one URI on rows 27 and 29, so the only difference is the renumbered id.
  The hover grouping cannot differ, because the links are on different rows.
- `d993d89d` declares that difference as `C16`. The expected-diffs mechanism is the one the
  IN-0029 LLD prescribes ("every behaviour correction lands in its natural packet with a
  declared expected difference"). It is the `DiffWindow` parser that validates the deviation
  id against `KNOWN_DEVIATIONS` and the field against `CELL_FIELDS`, and reports stale
  windows. `C16` is its **first use**: C1 to C15 were all measured free
  ("45 of 45 green with no `expected-diffs` file", `US-0076`), so there is no earlier
  `expected-diffs.json` to compare with. The window is exact (row 29, cols `0..3`, field
  `hyperlink`), and the stale check fails the gate if the difference ever disappears.
- Low, classification: IN-0029 defines `C*` as "a defect in the engine being replaced, fixed
  rather than reproduced" and `D*`/`G*` as deliberate deviations. Alacritty's per-occurrence
  id is not a defect by the specification, so `D16` would describe it more accurately. The
  `C16` row also sits between C12 and C11 in a table otherwise ordered from C15 down, and the
  sentence "45 of 45 green with no `expected-diffs` file" now needs an "(until `C16`)" note.
  None of this changes the gate.

### F5 (Info): correctness, verified by throwaway tests (all pass at `1f1914a4`; `d993d89d` changes no `crates/vt` file)

| Property | Test (throwaway) | Result |
| --- | --- | --- |
| Same URI with explicit ids `a`, `b`, and `a` again | `v_explicit_ids_same_uri_distinct_and_implicit_vs_explicit` | `a` ≠ `b`; the second `a` = the first `a` |
| Implicit then explicit, and explicit `id=1` then implicit, same URI | same | distinct both ways |
| Implicit link reopened after `RIS` | `v_ris_then_reopen_fresh` | table 1 entry, extras 2, resolves to its URI |
| History rows keep their links after 5,000 new URIs | `v_history_rows_resolve_after_many_new_links` | oldest row still resolves `http://first` and explicit `http://ex` |
| Cap with 70,000 distinct implicit URIs | `v_cap_holds_for_distinct_uris`, `verifier_implicit_map_bounded` | `entries` = `implicit` = 65,535, `index` = 0, `clear()` empties `implicit`; an already interned URI still resolves once the table is full |

- No O(n) work per open: one `FxHashMap::get(&str)` (`Box<str>: Borrow<str>`, no
  allocation) for implicit links. The explicit lookup still builds two `Box<str>` keys per open.
  That predates the fix, and the allocation is transient.
- Memory of the new map, worst case per terminal: 65,535 entries in a hashbrown table of
  131,072 buckets × (24 B slot + 1 control byte) ≈ **3.3 MB**, plus one copy of each URI. An
  OSC 8 URI is at most `OSC_INLINE` = 2,048 bytes, so a hostile stream can make that up to
  ≈ 134 MB. The same URI is also held in `entries`. This is **not a regression**: before,
  `index` held a `(id, uri)` copy of every implicit link in 40 B slots (≈ 5.2 MB of buckets
  plus the same URI copy). Per implicit link, the new map is smaller.

### F6 (Info): `RIS` emptying the extras table leaves no dangling id

This was traced in `reset_state` (`dispatch.rs:595`). `grid.reset()` resets both screens
(`Screen::reset`). The reset runs `clear_history`, and `Cursor::at` gives
`template: Cell::EMPTY` and `erase: Cell::EMPTY`. It also sets `saved_cursor = cursor` and
resets every row. The erase cell never carries extras (`set_template` builds it from the
background alone). The alt screen and `alt_active` are reset, and graphics `pending` and the
parser are cleared. Placements hold `GraphicId`s, not `ExtrasId`s, and die through the release
sweep because their rows were blanked. Consumers resolve extras under the lock
(`terminal/src/model.rs:425`, `content.rs:45`, `snapshot/row.rs:146`) and keep no `ExtrasId`
across frames. The test `v_ris_with_open_pen_saved_cursor_alt_and_history` covered a link in
history, a link pen saved by `DECSC`, an explicit link on the alt screen, and a pen left open.
After `RIS`, extras has 1 entry, history is 0 rows, every cell on both screens is
`ExtrasId::NONE`, and `DECRC` restores no link pen.

### F7 (Medium, residual, pre-existing, out of scope): every Sixel image still takes a permanent extras entry

`place` interns `Extras { graphic: Some(id) }` with a fresh `GraphicId` for every image, and
nothing but `RIS` frees it. Evicting or releasing a placement does not free it. A program that
re-sends a preview image on every redraw, such as a file manager with Sixel previews, repeats
BUG-0079 with images. After 65,534 images the extras table is full, and from then on new links
**and** new images lose their cells until `RIS`. The throwaway probe `v_sixel_repaint_fills_extras`
confirmed this (see Measurements). The packet records it as "Sixel still takes one extras entry per image". It
should be a follow-up BUG with its own fix. Candidates are to free an image's entry when its
placement is released (the release sweep already proves that no cell names it), or to reuse
the entry for an identical re-sent image.

### F8 (Low): the full-table warnings

- The new placement warning is once per terminal (`GraphicsState::unstamped_warned`). It is
  set on the first unstamped image and not reset by `RIS`, so it is never per frame. The
  generic `"extras table is full ..."` warning from `InternTable::intern` still fires once as
  well. The LLD says the placement warns "instead of relying on the generic extras warning".
  Both fire, so "in addition to" is the accurate wording.
- `InternTable::clear` keeps `warned`, so a table that fills again after `RIS` logs nothing.
  That is documented ("session telemetry") and acceptable.

### F9 (Low): records

- The packet's "Evidence and Gaps" still reads "Only the measurement that confirms the bug
  [...] No code change yet". The Handoff still says "whoever takes the `oneterm-vt` fix
  writes the LLD note first". Neither was updated for the fix, and the before/after table is
  missing. The table is below. The packet should link this file and list the gaps: F3, F7,
  hostile explicit ids and distinct URIs (the table is still fillable by 65,535 distinct URIs
  or explicit ids, with the same drop-the-attribute ladder), and no live re-measure.
- The CHANGELOG "Fixed" entries are accurate. "About half an hour at 30 frames a second" is
  36 minutes, and "about 21 MB" is 20.56 MB measured here and 21.0 MB in phase 4. They cite
  nothing repo-only. The `crates/vt` diff has no `US-`/`BUG-`/`IN-`/`DEC-` citation and no bare
  `crates/` or `docs/` path. `vt-public-api.py --check` is unchanged (gate).
- `dispatch.rs:984` (`set_hyperlink` rustdoc) still says "a stream of un-`id=`-ed OSC 8 links
  is attacker-reachable". That remains true for distinct URIs, so there is nothing to change.

## Negative control

In `HyperlinkTable::intern`, `None => self.implicit.get(uri)` was replaced by a constant
`None`, which restores the old per-occurrence path. `cargo test -p oneterm-vt --lib` failed 3
tests:

- `repainting_an_implicit_link_does_not_grow_the_tables`: `left: 65535, right: 1` (`terminal_tests.rs:1740`);
- `osc_8_sets_and_clears_the_hyperlink`: `left: ExtrasId(1), right: ExtrasId(2)`;
- `hyperlink_ids_are_per_terminal_not_global`: `left: Some(HyperlinkId(1)), right: Some(HyperlinkId(0))`.

The source was restored from `HEAD`.

## Measurements

A headless, release-profile throwaway example with a counting global allocator (live bytes =
alloc − dealloc ± realloc delta). One terminal at 158 x 40 is fed `\n\n\n`, and then the
`tui-mimic.py --link-repaint` frame: `CSI 3 A`, then three lines, the middle one holding
`OSC 8 ;; https://docs.anthropic.com/en/docs/claude-code ST docs OSC 8 ;; ST`. The baseline
was taken after the first `\n\n\n`.

| Repaints | Before: hyperlink entries | Before: extras entries | Before: live MB | After: hyperlink entries | After: extras entries | After: live MB |
| --- | --- | --- | --- | --- | --- | --- |
| 5,000 | 5,000 | 5,001 | 1.63 | 1 | 2 | 0.01 |
| 9,000 | 9,000 | 9,001 | 3.16 | 1 | 2 | 0.01 |
| 30,000 | 30,000 | 30,001 | 9.99 | 1 | 2 | 0.01 |
| 65,534 | 65,534 | 65,535 | 20.56 | 1 | 2 | 0.01 |
| 65,535 | 65,535 | 65,535 | 20.56 | 1 | 2 | 0.01 |
| 70,000 | 65,535 | 65,535 | 20.56 | 1 | 2 | 0.01 |

After the 70,000 repaints:

| Check | Before | After |
| --- | --- | --- |
| Linked cells on the repainted row | 0 | 4 |
| An explicit `id=x` link printed next | 0 cells | 2 cells |
| A Sixel image placed next | 0 cells | 1 cell |

This reproduces the implementer's 20.56 MB → 0.00 MB. The 0.01 MB here is the first link's
own entries and map allocations, because this baseline was taken before the first link.

F7 probe at `d993d89d` (`v_sixel_repaint_fills_extras`, one 1x6 Sixel re-sent at `CSI H` in a loop): the extras table is full after **65,534 images** (65,535 entries), and an explicit `id=z` link printed afterwards gets **no** link on its cells.

**Not run: the live 5-minute two-tab `measure.ps1 -AllocLog` run.** It needs the phase-4
probe allocator in `crates/app/src/oom.rs`, which was never committed and would have to be
rebuilt. It also needs two thin-LTO release builds of the app (before and after) at
`CARGO_BUILD_JOBS=2` on a machine shared with another agent. The headless figure covers the
mechanism the live run measured (four small allocations per repaint in phase 4 § 5.2). A live
confirmation remains a gap.

## Gate

`pwsh scripts/ci-local.ps1` at `d993d89d` (`CARGO_BUILD_JOBS=2`, `target/release` deleted
first), final line:

```
ci-local: all checks passed.
```

Also run separately at `1f1914a4`: `cargo test -p oneterm-vt --lib` (566 passed, 2 ignored), and
`cargo test -p oneterm-tools --test corpus_check` (**failed**, F4, fixed by `d993d89d`).
`vt-paranoid` and `--no-default-features` are gate steps.

## Gaps

- F7: a Sixel image re-sent on every redraw still fills the extras table (follow-up BUG).
- F3: an empty `id=` is not normalized to "no id".
- Hostile streams can still fill the hyperlink table with distinct URIs or distinct explicit
  ids. The ladder drops further links, and at worst the table holds ≈ 3.3 MB of buckets plus
  two copies of up to 2 KB per URI.
- No live 5-minute re-measure on the app. No run of the real `claude` CLI.
- Hover merges touching same-URI runs on one row (F1). This is allowed by the specification,
  and it differs from alacritty and VTE.
