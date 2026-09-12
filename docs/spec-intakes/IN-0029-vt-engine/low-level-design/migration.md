# Low-Level Design: Migration

Intake: IN-0029
HLD: ../high-level-design.md
Topic: migration
Date: 2026-09-12

> One concern per file. How the new engine replaces the vendored fork without the product
> stopping working in between.

## Concern

The seam the swap happens behind, the packet-by-packet order, the compatibility shim that makes
each slice independently verifiable, the exact list of code that is deleted, and the documentation
and notices that must be reconciled.

The governing constraint: **the baseline stays runnable throughout.** `main` builds and runs the
vendored engine until the shim packet flips it, and both engines coexist from `US-0072` to
`US-0087` so the differential runner always has something to compare against.

## Design

### The seam

`TerminalSession` (`crates/terminal/src/session.rs:399-...`, four composed traits) and
`TerminalContent` (`crates/terminal/src/content.rs`) are the boundary, and they keep their names.
Everything above them — `crates/terminal-view`, `crates/sftp-ui`, `crates/state`,
`crates/workspace`, `crates/app` — is unaware of which engine is underneath.

Above the seam, four files in `crates/terminal-view` import `alacritty_terminal` (R-26):
`src/render/frame.rs`, `src/input/mouse.rs`, `src/input/mouse_tests.rs` and
`src/theme/palette.rs`. The narrower claim in `frame.rs:3-7` — "the only file under `render/`" —
is the true one, and the migration list below covers all four.

### Packet order

Two packets start immediately and in parallel, because neither depends on the engine design:

- **`US-0071` — `oneterm-pty` extraction.** The PTY and the engine ship in the same crate today,
  so the transport would disappear with the fork if it were not extracted first. About 1 660 lines
  with no dependency on the grid. Details: [`pty.md`](pty.md).
- **`US-0072` — benchmark and parity harness.** Records the baseline and **blesses both
  expectation files with the old engine** before any replacement code exists. Details:
  [`testing-and-bench.md`](testing-and-bench.md).

Then the engine, bottom-up:

```
US-0073 parser ───────┐
US-0074 cell/style ───┴─▶ US-0075 grid + anchors ──┬─▶ US-0076 dispatch  (PARITY GATE GREEN)
                                                    ├─▶ US-0077 reflow ──▶ US-0078 selection
                                                    └─▶ US-0079 damage/render/events
                                         US-0080 graphics (needs 0075 + 0076)
```

Then the migration proper, one verifiable slice at a time (R-42):

```
US-0081 engine behind the seam (shim)      workspace green, zero behaviour diff
   ├─▶ US-0082 crates/terminal goes native
   ├─▶ US-0083 crates/local-shell goes native
   ├─▶ US-0084 crates/ssh goes native
   └─▶ US-0085 crates/terminal-view goes native
US-0086 deferred deviations + extension-point hardening   (after the gate and the swap)
US-0087 decommission the fork
```

### `US-0081` — the shim, and why it is a real slice

The earlier plan had one packet rewrite `crates/terminal`, `crates/ssh`, `crates/local-shell` and
`crates/terminal-view`, delete eleven call-site constructs, rewrite about 45 tests and 662 lines of
test support, and be accepted by three GUI walks. `docs/HARNESS.md` requires one coherent,
independently verifiable outcome per packet, and "rollback is `git revert` of one commit" was an
admission that the packet was the whole migration.

The split is made real by a named shim:

```rust
// crates/terminal/src/engine_shim.rs  — exists only from US-0081 to US-0085
/// Produces today's TerminalContent from the new engine's RenderState.
pub(crate) struct LegacySnapshot {
    state: RenderState,
    row_of: Vec<RowId>,      // viewport index -> RowId, so the seam can translate both ways
}
impl LegacySnapshot {
    pub fn refill(&mut self, term: &mut Terminal, out: &mut TerminalContent, now: Instant);
    pub fn display_row(&self, id: RowId) -> Option<usize>;   // RowId -> display row
    pub fn row_id(&self, display_row: usize) -> Option<RowId>;
}
```

`LegacySnapshot::refill` calls `render_update`, then fills the existing `TerminalContent` fields —
`cells: Vec<IndexedCell>`, `cursor`, `selection`, `mode`, `display_offset`, `damage`, `graphics` —
so `crates/terminal-view` compiles and behaves **unchanged**. That is the shim's whole job, and it
is what makes `US-0081`'s acceptance provable: *workspace green, differential runner zero
divergence, three GUI walks reproduced, no consumer changed*.

The two-way translation (`display_row` / `row_id`) is the seam adapter the earlier draft mentioned
in prose without designing (R-45). It is `RowId` to viewport index and back, computed once per
snapshot from the render state, and it is what lets `crates/completion`, the gutter, search and
the agent panel keep speaking display rows until their own packet moves them. It is deleted with
the last of them at `US-0085`.

**What each slice needs:**

**Disjoint scopes (N-04).** `US-0081` must touch the two backends, because each constructs the
`Arc<FairMutex<Term>>` and nothing compiles until they hold `oneterm_vt::Terminal`. What it may
touch there is bounded to the construction lines, and everything else is explicitly the later
packet's:

| Slice | May touch | Must not touch |
| --- | --- | --- |
| `US-0081` shim | all of `crates/terminal`; in `crates/local-shell` and `crates/ssh` **only** the type of the shared terminal, its construction call and the `Cargo.toml` line that adds `oneterm-vt` | their read loops, their transports, their resize paths, their tests |
| `US-0082` `crates/terminal` native | `model.rs`, `session.rs`, `content.rs`, `search.rs`, `url*.rs`, `palette.rs`, `osc_color.rs`, `logging.rs`, `test_support.rs`, `backend/` | any other crate |
| `US-0083` `local-shell` native | the read loop drains the `EventBatch` instead of the deferred sink; `ResizePolicy::KeepViewportTop` is selected through the engine API rather than through `TerminalModel`; the `oneterm-pty` tokens replace the last local constants; the `alacritty_terminal` manifest line is deleted | `crates/terminal` |
| `US-0084` `ssh` native | the same three for the tokio task, plus `ResizePolicy::BottomAnchor` selection; the `alacritty_terminal` manifest line is deleted | `crates/terminal` |
| `US-0085` `terminal-view` native | `render/frame.rs`, `plan_cache`, `input/mouse.rs`, `mouse_tests.rs`, `theme/palette.rs`; deletes `engine_shim.rs` | the backends |

None of the five can run behind the old engine, which is exactly why `US-0081` exists: it is the
one flip, it changes no behaviour, and every later slice is a refactor with the differential
runner still available.

### What the view still needs from the engine, and who owns it

Taken from the `US-0079` verification, which inventoried `crates/terminal/src/content.rs` and
`crates/terminal-view/src/render/frame.rs` against the shipped `RenderState`. **`RenderState`
alone cannot drive `frame.rs` yet**; every gap has an owner and none of them is the adapter's to
invent.

| Gap | View site | Owner |
| --- | --- | --- |
| **Cursor shape** — Block / Beam / Underline / HollowBlock / Hidden | `render/frame.rs:421-440`, `render/cursor.rs:54-55` | **`US-0076`** — `CursorStyle` is part of the dispatch packet (`DECSCUSR`) |
| **Selection range** — `start`, `end`, `is_block` | `render/frame.rs:451-463`, `:547-560`, `render/overlay.rs:32-67` | **`US-0078`** — [`selection.md`](selection.md) |
| **Per-cell graphic offset** — `RenderCell.graphic` carries the id only; the painter needs the `(col, row)` offset *inside the image's cell grid*, plus `width` / `height` / `rgba` and the virtual cell | `render/frame.rs:264-271`, `:313-317`, `render/element.rs:332-375` | **`US-0080`** — the `Placement` table must be able to reproduce that offset; see [`graphics.md`](graphics.md) § "Ownership" |
| **Absolute output-line count** for the gutter | `terminal_view/gutter_timestamps.rs:66`, `:84` | **`US-0076`** — `Terminal::lines_produced()` |
| **`size()` on the render state** — the view reads `GridSize { rows, cols }` | `render/frame.rs:577-582`, `render/element.rs:108-118`, `render/overlay.rs:36-44` | **`US-0079`** — a public accessor over the field it already stores |
| **Dim colour rule** — see below | `render/row_plan.rs:197-199` | **`US-0079`** — the `Palette` must be able to express it |
| **`last_content_line` / the clear epoch** | `terminal_view/gutter_timestamps.rs:91` | **`US-0081`** — adapter-side, read off the grid, not the render state |

**The dim rule is OneTerm's, not the generic one.** `Palette` must reproduce
`crates/terminal/src/palette.rs:128-137`: a dim colour is a **50 % mix with the background**, not a
fixed fraction toward black. The view then applies its own `fg.a *= 0.7`
(`render/row_plan.rs:197-199`) on top, and that stays where it is. A `Palette` whose `named()`
hard-codes a dim derivation with no override hook cannot express this, so the type takes the dim
colours from the adapter like every other themed colour.

### Per-crate swap detail

**`US-0082` — `crates/terminal`.** `model.rs`, `session.rs`, `content.rs`, `search.rs`, `url.rs`,
`url_policy.rs`, `palette.rs`, `color_classification.rs`, `osc_color.rs`, `mouse_encode.rs`,
`logging.rs`, `test_support.rs` and the whole `backend/` module move onto the native API:
`RowId`, `render_update`, the batch drain, `ColorKey`, `ModeSnapshot`.

**Search, URL detection and the `GridText` snapshot (R-19).** `crates/terminal/src/search.rs:70-148`
copies every cell of history plus viewport under the lock, once, so the search itself runs
unlocked; `url.rs` and `url_policy.rs` read the same snapshot. The new API offers
`row_text(RowId, &mut String)` per row, which would mean looping under the lock. **`GridText`
stays**, rebuilt adapter-side: one lock, `for id in term.row_range() { term.row_text(id, &mut s) }`
into the existing owned structure, then unlocked matching exactly as today. It is the same
O(rows x cols) lock-held copy the design argues against elsewhere, and it is kept deliberately
because search is user-initiated and rare, unlike a per-frame snapshot. A later packet may make it
incremental off the row sequence numbers; that is not this intake.

**`US-0085` — `crates/terminal-view`.** `render/frame.rs` consumes `RenderRow` and `RenderCell`;
`plan_cache` keys on `(RowId, SeqNo)`; `theme/palette.rs` and `input/mouse.rs` move to the engine's
`Rgb` and `SelectionKind`; `mouse_tests.rs` follows.

### Deletion list

Deleted outright (code), each in the packet named:

| What | Where | Packet |
| --- | --- | --- |
| `resize_keeping_viewport_top` | `crates/terminal/src/model.rs:481-527` | `US-0082` |
| `conhost_cursor_row` (the scratch-grid probe) | `crates/terminal/src/model.rs:528-541` | `US-0082` |
| `LineAccounting` — the whole file | `crates/terminal/src/backend/line_accounting.rs:1-49` | `US-0082` |
| The deferred/reliable tier of `SessionEventSink` and `flush_reliable[_blocking]` | `crates/terminal/src/backend/event_sink.rs`, driven from `pump.rs:163-178` | `US-0082` |
| `TermDamageInfo`'s display-line conversion | `crates/terminal/src/content.rs:66-106` | `US-0082` |
| The per-frame cell clone loop in `refill` | `crates/terminal/src/content.rs:173-222` | `US-0082` |
| The 256/257/258 colour index constants | `crates/terminal/src/osc_color.rs:23-27` | `US-0082` |
| `NamedColor` discriminant arithmetic | `crates/terminal/src/palette.rs:136`, `crates/terminal-view/src/render/frame.rs:118`, `:121` | `US-0082`, `US-0085` |
| The second `vte::Parser` in session logging | `crates/terminal/src/logging.rs:8`, `:57-83` | `US-0082` |
| The two display-offset fallbacks | `crates/terminal-view/src/render/frame.rs:514-532` | `US-0085` |
| `engine_shim.rs` (`LegacySnapshot`) | `crates/terminal/src/engine_shim.rs` | `US-0085` |
| The locally redeclared `PTY_CHILD_EVENT_TOKEN` and the two cfg'd child-pid helpers | `crates/local-shell/src/event_loop.rs:58-64`, `:168-179` | `US-0071` |
| `vt-diff` and every `--engine old` path | `crates/tools` | `US-0087` |

Deleted outright (the fork and its scaffolding), all at `US-0087`:

| What | Where |
| --- | --- |
| The vendored trees | `vendor/vte/`, `vendor/alacritty_terminal/` |
| The patch series (823 lines, five patches) | `vendor/patches/` |
| The refresh tool and its readme | `vendor/refresh.sh`, `vendor/README.md` |
| The CI job proving the trees are pristine plus patches | `.github/workflows/ci.yml:65-68` |
| The `vendor/**` path triggers | `.github/workflows/ci.yml:23`, `:41` |
| The workspace exclusion of the vendored trees | `Cargo.toml:24-26` |
| The `alacritty_terminal` git dependency and its comment block | `Cargo.toml:68-76` |
| The profile overrides for the fork | `Cargo.toml:172`, `Cargo.toml:200` |
| The `[patch]` block | `Cargo.toml:234-244` |
| `alacritty_terminal.workspace = true` in five manifests | `crates/terminal/Cargo.toml:19`, `crates/local-shell/Cargo.toml:20`, `crates/ssh/Cargo.toml:20`, `crates/terminal-view/Cargo.toml:32`, `crates/tools/Cargo.toml:35` |
| The two fork rows in the notices header | `scripts/third-party-notices.py:85-86` |
| The `--full` step running `vendor/refresh.sh --check` | `scripts/ci-local.sh`, `scripts/ci-local.ps1` |
| The `vte` dev-dependency and the differential oracle | `crates/vt/Cargo.toml`, `crates/vt/tests/differential.rs` |

Rewritten, not deleted: `crates/terminal/src/test_support.rs` (662 lines) and
`crates/terminal-view/src/render/frame.rs`'s `FrameBuilder` (`frame.rs:586-757`).

### Tests that change

| Suite | File:lines | Disposition |
| --- | --- | --- |
| Resize / ConPTY policy, 10 tests | `crates/terminal/src/model.rs:616-910` | **Kept running against the OLD engine until `US-0082`** (R-44), while the engine-side equivalents are built in `US-0077`. Both suites must be green at `US-0082`; only then is the old one deleted. Those ten tests are the only written form of the `KeepViewportTop` contract, so translating them in the same packet that implements the new coordinate model would remove the independent check |
| Sixel, 10 tests | `crates/terminal/src/sixel_tests.rs:46-271` | **Move** into `oneterm-vt` as `graphics::tests::*` at `US-0080`; the old file stays until `US-0082` |
| Snapshot / damage, 5 tests | `crates/terminal/src/content.rs:269-324` | **Rewrite** at `US-0082`: "damage is Full until the first reset" becomes "the first `render_update` is `Full`"; "Partial with only the cursor row" becomes "`Partial` with no changed rows when only the cursor moved" |
| Search, 11 tests | `crates/terminal/src/search.rs:226-344` | **Keep** at `US-0082`, with `Line`/`display_offset` assertions replaced by `RowId`; the `display_row(display_offset)` conversion test is deleted with the conversion |
| Router / pump, about 25 tests | `crates/terminal/src/backend/backend_tests.rs:98-640` | **Keep** the event-mapping and ordering coverage at `US-0082`; **delete** the deferred-flush and CORR-01 deadlock cases, unreachable once events are values. Each deletion named in the packet with that reason |
| Frame conversions, 5 tests | `crates/terminal-view/src/render/frame.rs:764-890` | **Rewrite** at `US-0085` |
| Mouse tests | `crates/terminal-view/src/input/mouse_tests.rs:218`, `:242` | **Keep** at `US-0085`, on `ModeSnapshot` and `SelectionKind` |
| Loopback PTY loop | `crates/local-shell/src/event_loop_tests.rs:196-...` | **Move** to `oneterm-pty` at `US-0071`, unchanged in substance |

Every rewritten or deleted test names, in its packet, what it used to pin and what pins it now.

### Documentation reconciliation (R-46)

Each doc edit lands in the packet that changes the code it describes, because
`scripts/verify-dependency-graph.py` carries a manifest allow-list that must be edited in the same
commit as a new crate or CI fails — which made the earlier "all docs at the end" schedule
impossible anyway.

| Doc | Change | Packet |
| --- | --- | --- |
| `docs/agents/structure.md` §1, §3 | add `crates/pty` | `US-0071` |
| `scripts/verify-dependency-graph.py` allow-list | add `oneterm-pty` | `US-0071` |
| `docs/agents/dependencies.md` §3 | `polling`, `windows-sys`, `libc` under the new crate | `US-0071` |
| `.github/workflows/ci.yml` | the bench job | `US-0072` |
| `docs/agents/dependencies.md` §3 | `proptest`, the `vte` dev-oracle, the `cargo-fuzz` nightly exception | `US-0072` |
| `docs/agents/structure.md` §1, §3 + the allow-list | add `crates/vt` | `US-0073` |
| `docs/agents/crate-dependency-rules.md` R6/R7/R8 | stop naming `alacritty_terminal`; name the two new crates | `US-0073` |
| `docs/agents/dependencies.md` §3 | `memchr`, `unicode-width`, `unicode-segmentation`, `smallvec`, `bitflags`, `rustc-hash`, each pinned to its 2.x / current line (R-23, R-48) | `US-0073`, `US-0074` |
| `docs/osc-sequences-checklist.md` | real coverage, and the three stale statements fixed | `US-0076` |
| `docs/terminal-backend.md` §5 | `feed`-and-drain, watermark damage, the demand signal | `US-0081` |
| `docs/terminal-backend.md` §5.3 | the native `ResizePolicy` | `US-0082` |
| `docs/spec-intakes/IN-0018-.../high-level-design.md` | the frame pipeline consumes a render state | `US-0085` |
| `docs/spec-intakes/IN-0028-.../{high-level-design,low-level-design/vendor-graphics}.md` | superseded in place, pointing at [`graphics.md`](graphics.md) | `US-0080` |
| `docs/architecture.md` | the two new crates | `US-0071`, `US-0073` |
| `docs/terminal-backend.md` §4, `docs/agents/dependencies.md` §1, `docs/PROJECT.md`, `README.md`, `vendor/README.md`, `THIRD-PARTY-NOTICES.md`, `NOTICE` | **removal** rows only | `US-0087` |

`python scripts/check-doc-paths.py` covers `docs/architecture.md`, `docs/agents/*.md`,
`docs/README.md`, `README.md` and `AGENTS.md`, so every `vendor/...` path in those five must be
gone before `US-0087` can pass CI.

### Notices and licensing

| Change | Detail | Packet |
| --- | --- | --- |
| Added | the vendored ref-test recordings: alacritty, Apache-2.0, revision recorded, "test data, unmodified" | `US-0072` |
| Added | the ported `avt` reflow iterator: Apache-2.0, with the § 4(b) "state changes" notice in the source header, `NOTICE` and the notices file | `US-0077` |
| Removed | the two `THIRD-PARTY-NOTICES.md` § 2 rows and the "pristine upstream plus the listed patch set" prose in `scripts/third-party-notices.py:85-86`, and the matching claim in `NOTICE` | `US-0087` |
| Unchanged | `THIRD-PARTY-NOTICES.md` § 1 — the bundled `conpty.dll` and `x64/OpenConsole.exe` rows with their SHA-256 hashes. Only § 2 is removed; the console-host bundle is owned by IN-0030 and is not touched by this intake | — |
| Unchanged | `deny.toml`'s allow-list: every new direct dependency is already permitted | — |
| Verified by | `python scripts/third-party-notices.py --check` in CI | `US-0087` |

Design ideas carry no obligation. kitty (GPL-3.0) and Warp (AGPL-3.0) were read for design only;
no line is transcribed, and commit messages keep that separation explicit.

## Interfaces

`LegacySnapshot`, above, is the only new interface this file owns. It is `pub(crate)` in
`crates/terminal` and lives for four packets.

## Edge Cases and Failure Modes

- [ ] **A packet lands half-swapped** — each compiles and tests green on its own; `US-0081` is the
  single flip and changes no behaviour.
- [ ] **The differential runner finds a divergence late** — it runs from `US-0072`, so it is
  available from the first engine packet.
- [ ] **The parity gate goes red after the flip** — the fork is still vendored until `US-0087`, so
  the comparison is available and `git revert` restores a working build.
- [ ] **A GUI walk regresses** — it is `US-0081`'s acceptance evidence; a failure reopens that
  packet rather than opening a new BUG (`docs/HARNESS.md` routing table).
- [ ] **`vendor/refresh.sh --check` failing mid-migration** because someone edits the vendored tree
  to work around a difference — forbidden. The fork is frozen from `US-0073`; a difference is fixed
  in the new engine or recorded as a deviation.
- [ ] **The unbounded `osc_raw` in the still-shipping fork** stays exploitable from any SSH session
  until `US-0081`. Pre-existing, and an explicit owner decision in `IN-0029.md`.
- [ ] **`crates/completion`, the gutter and the agent panel still speak display rows** after
  `US-0081` — `LegacySnapshot`'s two-way translation covers them until `US-0082` and `US-0085`.

## Verification

- [ ] `US-0071`: `grep -rn "alacritty_terminal::tty" crates/` empty; `cargo test -p oneterm-pty`
  green; a local shell opens, resizes and exits on Windows.
- [ ] `US-0081`: `cargo test --workspace` green with the new engine behind the shim; `vt-diff`
  reports no divergence over the 45 recordings and a captured session; the three GUI walks **plus
  IN-0028's Sixel walk** reproduced with fresh screenshots (N-02); no file outside
  `crates/terminal` changed except the bounded backend lines listed in the scope table above, and
  no backend test changed (N-04).
- [ ] `US-0082`: `grep -rn "resize_keeping_viewport_top\|conhost_cursor_row\|LineAccounting" crates/`
  empty; the old `model.rs` resize suite and the new `reflow::tests` suite both green in the same
  commit, then the old one deleted (R-44).
- [ ] `US-0085`: `engine_shim.rs` deleted; `plan_cache` keyed on `(RowId, SeqNo)`.
- [ ] `US-0087`: `test -d vendor` fails; `grep -rn "alacritty_terminal\|vendor/" Cargo.toml .github/workflows/ci.yml scripts/`
  empty; `python scripts/third-party-notices.py --check`, `python scripts/check-doc-paths.py` and
  `pwsh scripts/ci-local.ps1` all green.
