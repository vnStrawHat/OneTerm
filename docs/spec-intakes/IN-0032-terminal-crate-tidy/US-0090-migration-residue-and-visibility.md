# Work: The migration leaves no residue, and the engine publishes what it is used for

ID: US-0090
Intake: IN-0032
Created: 2026-09-14

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

- Change type: maintenance
- Risk lane: normal
- Spec Intake, when required: `IN-0032`

## Outcome

Nothing in `crates/terminal` or `crates/vt` survives only because a comment asked an
already-shipped packet to delete it, and neither crate publishes an item no other crate names.

Specifically: the `Engine` newtype and its empty `exit()` are gone; the three methods that are
byte-identical copies of their neighbours are gone with their call sites rewritten; `Demand` lives
in `crates/terminal` next to the policy that uses it, so `crates/vt` holds no atomic and
`crates/vt/src/lib.rs:4` is true as written; every `pub` item the audit proved has no external
consumer is `pub(crate)` or deleted; every comment and doc line citing a deleted file is
re-pointed or removed; and `python scripts/check-doc-paths.py` would have caught the rot it was
blind to.

**No behaviour changes.** Every deletion is either a zero-call-site item or a call-site rewrite to
a byte-identical twin; every visibility change is a narrowing that the compiler proves is safe.

## Scope

### In scope

- [ ] **Dead code in `crates/terminal/src/handle.rs`** (audit §2.1 `E1`–`E5`, §3 items 3 and 4):
  - `Engine::exit()` (`:49-57`, 9 lines) and the `Engine` newtype it is the only inherent method
    of (`:34-72`, 39 lines), plus its re-export at `crates/terminal/src/lib.rs:44`. The lock
    becomes `FairMutex<Terminal>` directly.
  - `lock_unfair` (`:154-156`), `try_lock_unfair` (`:159-161`) and one of the
    `take_render_demand` (`:131-133`) / `render_demand_raised` (`:136-138`) pair, with the
    explanatory comment at `:145-152` that exists only to apologise for them.
  - The four call sites: `crates/local-shell/src/event_loop.rs:436, :438, :523` and
    `crates/ssh/src/task_tests.rs:152`.
- [ ] **Move `Demand`** (audit §1.5, §2.1 `E7`, §3 item 11): `crates/vt/src/render/demand.rs`
  (62 lines) to `crates/terminal/src/handle.rs`, with its `crates/vt/src/render/mod.rs:18` and
  `crates/vt/src/lib.rs:7, :45` exports. Its only in-crate user is
  `crates/vt/src/render/render_tests.rs:787`, which moves or is rewritten with it.
- [ ] **Visibility pass on `oneterm-vt`** (audit §5): the 22 dead names in the
  `crates/vt/src/lib.rs:34-56` re-export block; `pub mod` → `pub(crate) mod` for the eight root
  modules no other crate reaches by path (`cell`, `event`, `graphics`, `reflow`, `render`,
  `selection`, `terminal`, `width`), which first requires rewriting the four redundant deep
  imports at `crates/terminal/src/content.rs:22`, `model.rs:14`, `model.rs:15` and
  `mouse_encode.rs:8` to use the names already at the `vt` root; and `pub` → `pub(crate)` on the
  zero-external-user items concentrated in `grid/screen.rs` (56), `grid/terminal_grid.rs` (15),
  `intern.rs` (14), `terminal/mode.rs` (12), `grid/row.rs` (13), `grid/anchor.rs` (5, whole
  module) and `render/sync.rs` (5, whole module).
- [ ] **Visibility pass on `oneterm-terminal`** (audit §5): `pub mod` → `pub(crate) mod` for
  `color_classification`, `key_encode`, `osc_color` (`lib.rs:14, :18, :24`); `pub` → `pub(crate)`
  for `paste`'s five items (the module is already `pub(crate)`, so they are decorative) and
  `osc.rs`'s six free items; `pub(crate)` for the 13 `SharedSessionState` methods with no external
  caller (`backend/state.rs:103-227`); and dropping `DefaultColors`, `EventQueueDiagnostics` and
  `TerminalLogError` from the `pub use` block.
- [ ] **Stale prose `R1`–`R7`** (audit §2.2): the dangling `` [`super::legacy_resize`] `` links at
  `crates/terminal/src/model_tests.rs:4, :89`; the `vendor/alacritty_terminal/...` citation at
  `crates/terminal/src/backend/osc_router.rs:400`; the deleted-`sixel_tests.rs` reference at
  `crates/vt/src/graphics/sixel.rs:9`; the "`Terminal` does not exist yet" comment at
  `crates/vt/src/render/state.rs:86-89`; the `vte` / "alacritty fork" paragraph at
  `crates/terminal/src/osc.rs:3-6`; and the dead-tooling note at
  `crates/vt/tests/parser_limits.rs:4-7`.
- [ ] **Doc rot `D1`–`D7`** (audit §2.3): `docs/terminal-backend.md:862-863`;
  `docs/architecture.md:11-29` (the `oneterm-terminal` label, and the missing `oneterm-vt`,
  `oneterm-tools`, `oneterm-theme`, `oneterm-highlight` and `oneterm-actions` rows);
  `docs/agents/structure.md:75, :183-189` and the `docs/refactor/` reference near `:196`.
- [ ] **Widen `scripts/check-doc-paths.py`**: add `docs/terminal-backend.md` to `DOCUMENTS` and
  drop the back-tick requirement from `PATH_PATTERN`, so a path inside an ASCII tree diagram is
  checked too. This is what would have caught `D1` and `D6` on the commit that created them.
- [ ] **Settle `terminal-diagnostics`** — see Decisions.

### Out of scope

- [ ] `model::ResizePolicy` and its two `From` impls (audit `E6`, §3 item 2). It is dead, but both
  backends are forced to name it by `impl_pty_terminal_session!`'s signature. It dies with the
  macro in `US-0091` and must not be touched here.
- [ ] `impl_pty_terminal_session!` itself, and anything in `crates/local-shell` or `crates/ssh`
  beyond the four call-site rewrites named above — `US-0091`.
- [ ] The two per-frame viewport scans — `US-0092`.
- [ ] The corpus move and the byte budget — `US-0093`.
- [ ] Any crate merge, split, creation or removal. The owner rejected Option C; see the
  High-Level Design.
- [ ] `B2` (`TerminalContent`'s 11 forwarding accessors, audit §3 item 7) and the `frame.rs`
  engine-vocabulary mirror (item 6). The audit recommends against both.
- [ ] The `Vec<&[u8]>` per claimed OSC event and the `set_title` / `store_clipboard` copies (audit
  §3 items 12 and 13). Real, tiny, and not this outcome.
- [ ] Renaming anything for taste. The one name that changes is the surviving member of the
  `take_render_demand` / `render_demand_raised` pair, and only because one of the two must go.
- [ ] The Apache-2.0 attribution headers in `crates/pty/src/windows*.rs` and `NOTICE`, the
  behaviour-provenance citations in `crates/vt` (`reflow/mod.rs:7`, `selection/expand.rs:5`,
  `grid/row.rs:121`, …), every `LEGACY_AGENT_OSC` name, and `display_offset`. The audit names all
  of these as legitimate and explicitly not to be touched.

## Acceptance

- [ ] **Zero users proved before deletion.** For each of `Engine`, `Engine::exit`, `lock_unfair`,
  `try_lock_unfair` and `render_demand_raised` (or `take_render_demand`, whichever is dropped),
  the packet records the grep output from the branch point showing the call sites are exactly the
  ones listed in Scope and no others. The audit's counts are from `782bb77` and must be re-proved
  on the branch, not inherited.
- [ ] **`crates/vt` holds no atomic.** `grep -rn "Atomic\|atomic::" crates/vt/src` returns nothing
  outside test files, and `crates/vt/src/lib.rs:4`'s "holds no lock and no interior mutability"
  needs no qualifying sentence.
- [ ] **Line delta.** Net **at least −80 production lines** across `crates/terminal/src` and
  `crates/vt/src` (the audit's estimate for Option A's code portion is −90, excluding the
  ~46 doc lines and the visibility pass). The packet records the measured figure per crate; if it
  comes in under −80, the packet explains what the audit over-counted rather than padding the
  deletion.
- [ ] **Public surface shrinks measurably.** Baseline on the branch point, by the reproducible
  proxy count below, is **`vt` 535** and **`terminal` 301**:

  ```bash
  for c in vt terminal; do
    grep -rn '^[[:space:]]*pub \(fn\|struct\|enum\|const\|static\|mod\|trait\|type\|use\)' \
      crates/$c/src --include='*.rs' \
      | grep -v '_tests\.rs\|_props\.rs\|_bench\.rs\|test_support\.rs' | wc -l
  done
  ```

  After this packet: `vt` **≤ 400** and `terminal` **≤ 270**. (This proxy counts only items
  declared at the start of a line, so it differs from the audit's 507 / 325, which counts inherent
  `impl` methods too; the audit's own target is about 320 and about 250 on its measure. Use the
  proxy for the pass/fail gate because it is one command anyone can re-run, and quote the audit's
  measure alongside it.)
- [ ] **No test is lost.** Record `cargo test -p oneterm-vt`, `-p oneterm-terminal`,
  `-p oneterm-local-shell`, `-p oneterm-ssh` and `-p oneterm-terminal-view` counts at the branch
  point, and the same counts after. Each must be **greater than or equal to** its baseline. A test
  that moves with `Demand` is still counted; a test that is deleted must be named and justified.
- [ ] **`cargo doc -p oneterm-vt --no-deps` builds with no broken intra-doc link**, which is what
  proves `R1`/`R2`'s dangling `` [`super::legacy_resize`] `` links are actually gone rather than
  merely edited.
- [ ] **`python scripts/check-doc-paths.py` passes with the widened `DOCUMENTS` and regex**, and
  the packet records the new "checked N paths in M documents" line against the current
  "120 current paths in 10 documents". The number must go **up**; if it does not, the widening did
  not take effect.
- [ ] **Behaviour unchanged.** No test assertion is edited to accommodate this packet. If one has
  to be, that is a behaviour change and this packet is wrong.
- [ ] **`pwsh scripts/ci-local.ps1` exits 0**, with its test totals recorded.

## Documentation

### Owning Docs Reviewed

- `docs/agents/crate-dependency-rules.md` — R7 ("the engines and the transport are gpui-free";
  `pty` and `vt` depend on no OneTerm crate). Moving `Demand` out of `crates/vt` does not change
  the edge set, but it does make R7's premise about `vt` stronger: the crate then holds no atomic
  at all. The rules table itself needs no edit.
- `docs/agents/structure.md` — the per-crate responsibility table and the directory tree. Carries
  `D5` (the `vt/` subtree lists 4 files against 54 across 9 subdirectories), `D6`
  (`docs/refactor/ui-crate-restructure.md`, a directory that does not exist) and `D7`
  (`url.rs / url_policy.rs`, where only `url_policy.rs` exists). **Must change.**
- `docs/architecture.md` — carries `D3` (labels `oneterm-terminal` "Terminal engine"; it is the
  adapter) and `D4` (the crate map has no `oneterm-vt` row at all, plus four other missing rows).
  **Must change.**
- `docs/terminal-backend.md` — §"File layout (current)" carries `D1` (lists `engine_shim.rs`,
  deleted at `US-0085`) and `D2` (describes `content.rs` as holding "the legacy shape", which
  `crates/terminal/src/content.rs:12` contradicts). Also the owning description of
  `TerminalHandle`, which this packet edits. **Must change.**
- `IN-0029/low-level-design/events-and-api.md` — the engine's intended outward API. Read to
  confirm the visibility pass narrows toward what this document specifies rather than away from
  it; any item the LLD names as public stays public even if the audit found no current consumer,
  and the packet records each such exception.
- `IN-0029/low-level-design/damage-and-render-state.md` — owns `Demand`'s contract (who raises it,
  who reads it, what a raised demand means). The type moves crate; the contract does not. Check
  whether the LLD states the crate it lives in; if it does, that sentence must change.
- `crates/vt/src/lib.rs:1-10` — the crate doc claiming no lock and no interior mutability. This is
  the claim the `Demand` move makes true.
- `crates/terminal/src/handle.rs:112-115, :127-130, :145-152` — the three comments that document
  the lock policy, the non-consuming render demand, and the fairness apology. Two of the three
  describe code this packet deletes.

### Documentation Action

Update required:

- `docs/terminal-backend.md` — `D1` and `D2` in the file-layout section; and the `TerminalHandle`
  description, which must stop naming `Engine` and must name the surviving demand accessor. Also
  the first doc this packet's own widening of `check-doc-paths.py` brings under the checker, so it
  must be path-clean afterwards.
- `docs/architecture.md` — `D3` and `D4`: relabel `oneterm-terminal`, and add the missing
  `oneterm-vt`, `oneterm-tools`, `oneterm-theme`, `oneterm-highlight` and `oneterm-actions` rows.
  The file calls itself the architecture source of truth and currently omits the first-party
  engine.
- `docs/agents/structure.md` — `D5`, `D6`, `D7`.
- `IN-0029/low-level-design/damage-and-render-state.md` — only if it names `Demand`'s crate.
- `crates/vt/src/lib.rs` and `crates/terminal/src/handle.rs` doc comments — they must describe what
  is left, not what was deleted.

Reason: this is a maintenance packet whose entire subject is documentation and visibility drifting
away from the code, so "no contract change" cannot be the answer for the docs that carry the drift.
No **behavioural** contract changes: `TerminalSession`, `SessionFactory`, `SftpBackend`, the panel
names and every boundary in `docs/PROJECT.md` § Important Boundaries keep their shape, and
`docs/PROJECT.md` itself needs no edit.

### Reconciliation

Before completion, list the docs changed, and for each of `docs/PROJECT.md`,
`docs/agents/crate-dependency-rules.md`, `docs/agents/persistence.md` and
`IN-0029/low-level-design/events-and-api.md` either the change or the explicit no-change reason.

## Context

The two comments that define this packet's mandate, quoted from the code:

```text
crates/terminal/src/handle.rs:49  // ponytail: a no-op that exists only so crates/local-shell's
                                  // read loop — US-0083's — is not edited by this packet.
                                  // Upgrade path: delete the call site and this type with it.
crates/terminal/src/handle.rs:36  "It is US-0083's to delete, and this type goes with it."
crates/vt/src/render/demand.rs:20 "This is the adapter's primitive, not the engine's: it is
                                  reachable from no engine type, holds the crate's only atomic,
                                  and lives here so the contract and its test have one home
                                  until US-0081 moves the pump loop over."
```

`US-0081` shipped at `f9af66c`; `US-0083` and `US-0084` shipped in the `IN-0029` merge at
`782bb77`. Every one of these comments is a shipped promise.

`handle.rs:147` on the two "unfair" methods: *"`parking_lot`'s fairness lives in `unlock`, so
there is no unfair acquire to call and both of these are the plain ones."* The bodies are
byte-identical, so the rewrite is textual and the compiler proves it.

**On the demand pair, prefer keeping `render_demand_raised` and deleting `take_render_demand`.**
The audit (§3 item 4) proposes the opposite, cutting `render_demand_raised` to
`take_render_demand` because the latter has the non-test call sites. But it also records, one
paragraph later, that `take_render_demand` *"takes nothing — it is a non-consuming `is_raised()`,
deliberately so since the `US-0082` rework"*, and that the name mis-states the contract to every
reader of `event_loop.rs:477` and `task.rs:87`. Keeping the accurately named twin costs the same
diff (three call sites rewritten instead of one) and removes the naming residue at the same time.
Record whichever way it lands and why.

`crates/vt/src/lib.rs:34-56`'s 22 dead re-export names, for the grep: `StrSpan`, `ByteSpan`,
`ParamSpans`, `FeedStats`, `MAX_DIMENSION`, `Placement`, `RowRef`, `TerminalGrid`,
`GRAPHEME_MAX_LEN`, `StyleId`, `GraphemeId`, `ResizeOutcome`, `Watermark`, `EngineView`,
`SyncState`, `SEMANTIC_ESCAPE_CHARS`, `Invalidation`, `ColorOverrides`, `ThemeColors`,
`ModeState`, `CursorStyle`, `cluster_width`.

Two of these need care rather than a blanket deletion: **`RowRef`** is the type whose
`is_allocated()` / `occ()` hints `US-0092` uses from `crates/terminal/src/content.rs`, and
**`Placement`** is named by the graphics path. Re-prove each with its own grep at the moment of
deletion; if `US-0092` lands first, `RowRef` will have gained an external user and must stay.

Three `vt` root modules must stay `pub` because another crate reaches them by path:
`intern` (`crates/terminal/src/content.rs:21` needs `intern::Hyperlink`, which has no root
re-export), `grid` (`crates/terminal/src/handle.rs:24` needs
`grid::{DEFAULT_SCROLLBACK, SCROLLBACK_MAX}`, neither re-exported) and `parser`
(`crates/terminal/src/logging.rs:10` drives its own parser and needs
`parser::{Dispatch, OscParams, Params, Parser, StringTerm}`, and `parser` has no root re-exports
at all). The first two could instead be closed by adding root re-exports for three names; that is
a smaller public surface but a bigger diff, so take it only if it falls out naturally.

`crates/tools` is why several items read as externally used: it drives `Screen` directly from
`corpus_replay.rs:29`. Do not narrow anything `crates/tools` names — `cargo clippy --workspace
--all-targets` compiles it and will say so immediately either way.

## Plan

- [ ] Record the branch-point baselines first: the five `cargo test -p …` counts, the two public
  surface proxy counts, and `check-doc-paths.py`'s current output line.
- [ ] Run and record the zero-user greps for all five deletion targets.
- [ ] Delete `Engine`, `exit()`, and the duplicate methods; rewrite the four call sites. Compile.
- [ ] Move `Demand` to `crates/terminal/src/handle.rs` with its test; delete its `vt` exports.
  Compile, and confirm `grep -rn "Atomic" crates/vt/src` is clean outside tests.
- [ ] Visibility pass on `crates/vt`: the 22 dead re-exports first (each re-proved by grep), then
  the four redundant deep imports in `crates/terminal`, then the eight root modules, then the
  per-file `pub(crate)` concentrations. Compile after each step — `-D warnings` across all targets
  is the proof, and taking it in steps keeps a failure attributable.
- [ ] Visibility pass on `crates/terminal`, same shape.
- [ ] Stale prose `R1`–`R7`, then doc rot `D1`–`D7`.
- [ ] Widen `scripts/check-doc-paths.py`, run it, and fix whatever new rot it now reports —
  including in `docs/terminal-backend.md`, which it has never checked.
- [ ] Settle `terminal-diagnostics` per Decisions; record the branch taken and its reason.
- [ ] Re-measure everything in Acceptance; run `pwsh scripts/ci-local.ps1`.

## Decisions

- **`terminal-diagnostics`: CI-gate it or delete it.** Open in `IN-0032` § Open Decisions with the
  full evidence. **Default: keep the feature and add one CI step** —
  `cargo clippy --workspace --all-targets --features oneterm-app/terminal-diagnostics -- -D warnings`
  in `.github/workflows/ci.yml` and both `scripts/ci-local.sh` and `scripts/ci-local.ps1`. One
  line in three files buys compile coverage for about 200 lines of incident diagnostics, against a
  deletion that is unrecoverable at the moment those lines are wanted. Taking the delete branch
  instead is acceptable if the implementer finds the guarded code has itself rotted — but that
  finding must be recorded with the compiler errors that show it. Either way this needs **no**
  `DEC` record: it is a CI policy choice inside one packet, not a rationale future work must
  inherit.
- No other decision here. The seam choices, the rejected Option C, and the `Demand` ownership
  argument are all in the intake's High-Level Design.

## Verification Plan

Focused proof:

- `cargo test -p oneterm-vt`, `-p oneterm-terminal`, `-p oneterm-local-shell`, `-p oneterm-ssh`,
  `-p oneterm-terminal-view`, each against its recorded baseline count.
- `cargo test -p oneterm-vt --features vt-paranoid` — the whole-history integrity walk. The
  `Demand` move touches `crates/vt/src/render/`, so this is not optional.
- `cargo doc -p oneterm-vt --no-deps` and `cargo doc -p oneterm-terminal --no-deps`, clean: the
  only way to prove the dangling intra-doc links are gone and that narrowing visibility did not
  break a doc link into a now-private item.

Regression:

- `cargo clippy --workspace --all-targets -- -D warnings`. This is the load-bearing check for the
  whole visibility pass: every over-narrowing surfaces here as a hard error, and it compiles
  `crates/tools` too.
- `cargo test --workspace`.
- `python scripts/check-doc-paths.py` (widened), `python scripts/check-english.py`,
  `python scripts/verify-dependency-graph.py`.

Platform:

- `pwsh scripts/ci-local.ps1`, exit 0, totals recorded.

No E2E. This packet deletes unreachable code, moves one type between crates, and narrows
visibility; there is nothing a GUI walk could observe that `-D warnings` and the workspace tests
do not already prove. If any step of it turns out to need a GUI check, that is a signal the change
was not behaviour-neutral and the packet should stop.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record: the branch point and commit; the five zero-user greps; the per-crate
line delta; the before/after public-surface proxy counts and the audit-measure estimate; the five
test counts before and after; `check-doc-paths.py`'s before and after output lines; the
`terminal-diagnostics` branch taken; and `ci-local.ps1`'s totals. Record any item the audit listed
as dead that turned out to have a user, with the grep that found it.

## Handoff

`US-0091` starts from this packet's head: it rewrites the same `TerminalSession` surface and
deletes `model::ResizePolicy`, which this packet deliberately leaves alone. `US-0092` is
independent and may run in parallel — but if it lands first, `RowRef` gains an external user and
must not be dropped from `crates/vt/src/lib.rs`'s re-exports.
