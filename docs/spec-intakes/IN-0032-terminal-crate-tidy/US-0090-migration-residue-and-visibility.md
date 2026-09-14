# Work: The migration leaves no residue, and the engine publishes what it is used for

ID: US-0090
Intake: IN-0032
Created: 2026-09-14

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
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

- [x] **Dead code in `crates/terminal/src/handle.rs`** (audit §2.1 `E1`–`E5`, §3 items 3 and 4):
  - `Engine::exit()` (`:49-57`, 9 lines) and the `Engine` newtype it is the only inherent method
    of (`:34-72`, 39 lines), plus its re-export at `crates/terminal/src/lib.rs:44`. The lock
    becomes `FairMutex<Terminal>` directly.
  - `lock_unfair` (`:154-156`), `try_lock_unfair` (`:159-161`) and one of the
    `take_render_demand` (`:131-133`) / `render_demand_raised` (`:136-138`) pair, with the
    explanatory comment at `:145-152` that exists only to apologise for them.
  - The four call sites: `crates/local-shell/src/event_loop.rs:436, :438, :523` and
    `crates/ssh/src/task_tests.rs:152`.
- [x] **Move `Demand`** (audit §1.5, §2.1 `E7`, §3 item 11): `crates/vt/src/render/demand.rs`
  (62 lines) to `crates/terminal/src/handle.rs`, with its `crates/vt/src/render/mod.rs:18` and
  `crates/vt/src/lib.rs:7, :45` exports. Its only in-crate user is
  `crates/vt/src/render/render_tests.rs:787`, which moves or is rewritten with it.
- [x] **Visibility pass on `oneterm-vt`** (audit §5): the 22 dead names in the
  `crates/vt/src/lib.rs:34-56` re-export block; `pub mod` → `pub(crate) mod` for the eight root
  modules no other crate reaches by path (`cell`, `event`, `graphics`, `reflow`, `render`,
  `selection`, `terminal`, `width`), which first requires rewriting the four redundant deep
  imports at `crates/terminal/src/content.rs:22`, `model.rs:14`, `model.rs:15` and
  `mouse_encode.rs:8` to use the names already at the `vt` root; and `pub` → `pub(crate)` on the
  zero-external-user items concentrated in `grid/screen.rs` (56), `grid/terminal_grid.rs` (15),
  `intern.rs` (14), `terminal/mode.rs` (12), `grid/row.rs` (13), `grid/anchor.rs` (5, whole
  module) and `render/sync.rs` (5, whole module).
- [x] **Visibility pass on `oneterm-terminal`** (audit §5): `pub mod` → `pub(crate) mod` for
  `color_classification`, `key_encode`, `osc_color` (`lib.rs:14, :18, :24`); `pub` → `pub(crate)`
  for `paste`'s five items (the module is already `pub(crate)`, so they are decorative) and
  `osc.rs`'s six free items; `pub(crate)` for the 13 `SharedSessionState` methods with no external
  caller (`backend/state.rs:103-227`); and dropping `DefaultColors`, `EventQueueDiagnostics` and
  `TerminalLogError` from the `pub use` block.
- [x] **Stale prose `R1`–`R7`** (audit §2.2): the dangling `` [`super::legacy_resize`] `` links at
  `crates/terminal/src/model_tests.rs:4, :89`; the `vendor/alacritty_terminal/...` citation at
  `crates/terminal/src/backend/osc_router.rs:400`; the deleted-`sixel_tests.rs` reference at
  `crates/vt/src/graphics/sixel.rs:9`; the "`Terminal` does not exist yet" comment at
  `crates/vt/src/render/state.rs:86-89`; the `vte` / "alacritty fork" paragraph at
  `crates/terminal/src/osc.rs:3-6`; and the dead-tooling note at
  `crates/vt/tests/parser_limits.rs:4-7`.
- [x] **Doc rot `D1`–`D7`** (audit §2.3): `docs/terminal-backend.md:862-863`;
  `docs/architecture.md:11-29` (the `oneterm-terminal` label, and the missing `oneterm-vt`,
  `oneterm-tools`, `oneterm-theme`, `oneterm-highlight` and `oneterm-actions` rows);
  `docs/agents/structure.md:75, :183-189` and the `docs/refactor/` reference near `:196`.
- [x] **Widen `scripts/check-doc-paths.py`**: add `docs/terminal-backend.md` to `DOCUMENTS` and
  drop the back-tick requirement from `PATH_PATTERN`, so a path inside an ASCII tree diagram is
  checked too. This is what would have caught `D1` and `D6` on the commit that created them.
- [x] **Settle `terminal-diagnostics`** — see Decisions.

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

- [x] **Zero users proved before deletion.** For each of `Engine`, `Engine::exit`, `lock_unfair`,
  `try_lock_unfair` and `render_demand_raised` (or `take_render_demand`, whichever is dropped),
  the packet records the grep output from the branch point showing the call sites are exactly the
  ones listed in Scope and no others. The audit's counts are from `782bb77` and must be re-proved
  on the branch, not inherited.
- [x] **`crates/vt` holds no atomic.** `grep -rn "Atomic\|atomic::" crates/vt/src` returns nothing
  outside test files, and `crates/vt/src/lib.rs:4`'s "holds no lock and no interior mutability"
  needs no qualifying sentence.
- [ ] **NOT MET, explained below (Evidence, "Line delta").** Net **at least −80 production lines**
  across `crates/terminal/src` and
  `crates/vt/src` (the audit's estimate for Option A's code portion is −90, excluding the
  ~46 doc lines and the visibility pass). The packet records the measured figure per crate; if it
  comes in under −80, the packet explains what the audit over-counted rather than padding the
  deletion.
- [x] **Public surface shrinks measurably.** Baseline on the branch point, by the reproducible
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
- [x] **No test is lost.** Record `cargo test -p oneterm-vt`, `-p oneterm-terminal`,
  `-p oneterm-local-shell`, `-p oneterm-ssh` and `-p oneterm-terminal-view` counts at the branch
  point, and the same counts after. Each must be **greater than or equal to** its baseline. A test
  that moves with `Demand` is still counted; a test that is deleted must be named and justified.
- [x] **`cargo doc -p oneterm-vt --no-deps` builds with no broken intra-doc link**, which is what
  proves `R1`/`R2`'s dangling `` [`super::legacy_resize`] `` links are actually gone rather than
  merely edited.
- [x] **`python scripts/check-doc-paths.py` passes with the widened `DOCUMENTS` and regex**, and
  the packet records the new "checked N paths in M documents" line against the current
  "120 current paths in 10 documents". The number must go **up**; if it does not, the widening did
  not take effect.
- [x] **Behaviour unchanged.** No test assertion is edited to accommodate this packet. If one has
  to be, that is a behaviour change and this packet is wrong.
- [x] **`pwsh scripts/ci-local.ps1` exits 0**, with its test totals recorded.

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
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

No E2E, by the Verification Plan above: this packet deletes unreachable code, moves one type
between crates and narrows visibility, and `-D warnings` across all targets plus the workspace
tests are the exhaustive proof.

## Evidence and Gaps

**Branch point.** `4e83f31` ("docs(terminal): IN-0032 terminal crate tidy — intake, design and four
packets, before any code"). Worktree branch `worktree-agent-a85e44f034eb1c863`, hard-reset onto
`main` before any edit.

### The five zero-user greps, re-proved on the branch

| Target | Command | Result |
| --- | --- | --- |
| `Engine::exit` | `grep -rn '\.exit()\|Engine::exit' crates/ --include='*.rs'` | **no output** — zero callers, including the definition's own doc, which no longer says `.exit()` |
| `Engine` (the newtype) | `grep -rnw 'Engine' crates/ --include='*.rs' \| grep -v 'EngineView\|engine_'` | 79 hits, none outside `crates/terminal/src/handle.rs` + its re-export at `lib.rs:44`. Every other hit is a different `Engine`: `oneterm_completion::Engine` (8), `base64::Engine` (3), `crates/vt/src/render/render_tests.rs`'s test harness struct (66), one comment in `backend/osc_router.rs:39` |
| `lock_unfair` | `grep -rn 'lock_unfair' crates/ --include='*.rs'` | 8 hits: the two definitions, `local-shell/src/event_loop.rs:436, :438, :523`, and three in `handle.rs`'s own tests (`:309, :335, :337`). **Exactly the Scope list.** |
| `try_lock_unfair` | same command | included above — `event_loop.rs:436` plus `handle.rs:335, :337` |
| `render_demand_raised` | `grep -rn 'render_demand_raised' crates/ docs/` | one non-test caller: **none**; callers: `ssh/src/task_tests.rs:152` and four in `handle.rs`'s tests. `take_render_demand` had `event_loop.rs:477` and `ssh/src/task.rs:87` |

Discrepancy against the audit: **none for the five targets.** The audit's `E3` says `lock_unfair`'s
remaining call sites are `event_loop.rs:438, :523`; that is right, and `:436` is `try_lock_unfair`
(`E4`), as the audit also says.

**Which twin was kept.** `render_demand_raised`, as the packet prefers and against the audit's §3
item 4. `take_render_demand` takes nothing — it has been a non-consuming `is_raised()` since the
`US-0082` rework — so the surviving name now states the contract. Cost: three call sites rewritten
(`event_loop.rs:477`, `ssh/src/task.rs:87`, `ssh/src/task_tests.rs:152` needed none) instead of
one, plus the module docs at `event_loop.rs:11` and `task.rs:26` and
`docs/terminal-backend.md` §5.1, §5.2, §6.

### Items the audit listed as dead that turned out to have a user

| Item | The grep that found the user | Disposition |
| --- | --- | --- |
| `DefaultColors` (audit §5, "drop from the `pub use` block") | `grep -rnw DefaultColors crates/terminal/` → `crates/terminal/src/session.rs:520`, inside `impl_pty_terminal_session!` as `$crate::DefaultColors::new(...)`. The macro expands into `crates/local-shell` and `crates/ssh`, so the root re-export is load-bearing; a plain identifier grep cannot see a `$crate::` path | **Kept.** It dies with the macro in `US-0091` |
| `EventQueueDiagnostics` (same) | It is the return type of the public `SessionEventSink::diagnostics()`; dropping the re-export leaves a public method returning an unnameable type | **Kept**, re-exported from `backend/mod.rs` as before |
| `FeedStats` (one of the 22 dead `vt` root re-exports) | It is what the public `Terminal::feed` returns | **Kept** at the `vt` root |
| `cluster_width` (same list) | No user, by design — `crates/vt/src/width.rs:12` states it is "the answer mode 2027 will need … implemented and tested now so that landing the mode is a print-path change" | **Kept** published, with a comment at the re-export saying why |
| `RowRef` (same list) | No user today; `US-0092` runs in parallel and reads `RowRef::is_allocated()` / `occ()` from `crates/terminal/src/content.rs` | **Kept** published, and both methods held at `pub` through the narrowing pass |
| `paste`'s five items (audit: "decorative `pub`") | `grep -n '^\s*pub' crates/terminal/src/paste.rs` → all five are **already** `pub(crate)`; the audit's line numbers predate that | **No change needed** |
| `SshCommandDiagnostics` / `SshTransport::diagnostics` | `grep -rn 'SshCommandDiagnostics\|\.diagnostics()' crates/ssh/ crates/app/ crates/terminal-view/` → **no output**. Under `--features terminal-diagnostics` they are dead code and fail the new CI step | Narrowed from `#[cfg(any(test, feature = "terminal-diagnostics"))]` to `#[cfg(test)]`, which is who actually calls them. See "terminal-diagnostics" below |

The other 20 names in the `vt` root block were re-proved dead with
`for n in …; do grep -rnw "$n" crates/ --include='*.rs' | grep -v '^crates/vt/' | wc -l; done`:
all returned `0` except `ColorOverrides` (19) and `CursorStyle` (3), both of which resolve to
**different types** — `oneterm_settings::ColorOverrides` and `gpui::CursorStyle` — so all 22 were
genuinely dead as `oneterm-vt` exports.

### `crates/vt` holds no atomic

```text
$ grep -rn 'Atomic\|atomic::' crates/vt/src
(no output)
```

`crates/vt/src/lib.rs`'s crate doc now reads "There is no atomic here at all" instead of the
paragraph that contradicted line 4, and needs no qualifying sentence.

### Public surface, by the packet's reproducible proxy

| Crate | Branch point | After | Target | Audit measure (inherent `impl` methods included) |
| --- | ---: | ---: | ---: | --- |
| `oneterm-vt` | **535** | **392** | ≤ 400 | audit's own target ≈ 320 from 507; this pass narrowed 143 line-start items, so the audit-measure estimate is ≈ **360** |
| `oneterm-terminal` | **301** | **269** | ≤ 270 | audit's target ≈ 250 from 325; ≈ **290** on that measure |

Both gates met. The `vt` figure lands above the audit's ≈ 320 for one structural reason recorded
here rather than pursued: closing `grid` and `intern` as `pub(crate) mod` (the audit's "or
re-export three names and close them") makes every `pub` item inside them dead-code-eligible, and
about fifteen of them — `Anchors::{kind, live_in, live}`, `Row::{heap_bytes, bytes}`,
`RowMut::{id, reset}`, `Cursor::erase`, `TabStops::heap_bytes`, `CursorOrigin`,
`TerminalGrid::{primary, alt, wrapline, heap_bytes}`, `GraphemeArena::{needs_sweep, sweep,
truncated, entries, chars}`, `GraphemeRemap` — are used **only by that crate's own tests**. Closing
those two modules therefore forces either deleting engine machinery the tests exercise or
`#[cfg(test)]`-gating it, neither of which is this packet's mandate. `grid`, `intern` and `parser`
stay `pub mod`, exactly as the packet's Context says they must.

Method, recorded so the next pass can repeat it: a temporary `#![warn(unreachable_pub)]` on each
crate root named every item no longer reachable, then `cargo clippy --workspace --all-targets`
named every item that must stay `pub` (E0364/E0365 at the re-export, E0624 at the use site,
`private_interfaces` in a public signature, `dead_code` where the narrowing made an item
unreachable). Nothing was narrowed by judgement; every `pub(crate)` here is one the compiler
proved safe across `crates/tools`, every test target and every bench.

### Line delta — the one acceptance clause not met, and why

```text
$ git diff --numstat main..HEAD -- crates/vt/src crates/terminal/src   # excluding *_tests.rs etc.
+397 / -401     net -4
```

Adjusted for the one `#[cfg(test)]` block that the filename filter cannot see — the 75-line
`Demand` contract test ported into `crates/terminal/src/handle.rs`'s inline `mod tests`
(`handle.rs:288-362`) — the production figure is **+322 / −401, net −79**, one line short of the
−80 gate.

What the audit over-counted, rather than padding the deletion:

- **`Demand` is counted twice.** The audit books its 62 lines as a deletion (§2.1 `E7`). The
  acceptance sums **both** crates, and the type *moves*: `crates/vt/src/render/demand.rs` is −62
  and `crates/terminal/src/handle.rs` is +48 for the same type, so the pair nets −14, not −62.
- **`Engine`'s 39 lines come back as `Demand`'s 48.** The audit's −48 for `Engine` + `exit()` is
  real, but the file that loses them is the file that gains the moved type, so `handle.rs`'s
  production half is roughly flat.
- **The visibility pass is line-neutral by construction** — `pub ` becomes `pub(crate) ` in place —
  and the audit says so ("~260 `pub` → `pub(crate)`", listed separately from the −90).
- **Six prose rewrites are net-positive lines.** `R1`–`R7` replace a stale sentence with an
  accurate one; `R6` (the `vte` paragraph) and `R7` grew by a line each because the accurate
  history needs one more clause than the wrong one did.

Per crate, production only, after the same `#[cfg(test)]` adjustment: `crates/vt/src`
**+195 / −264 (net −69)**, `crates/terminal/src` **+127 / −137 (net −10)**. The split is the move
made visible: `vt` carries `demand.rs`'s whole −62, and `terminal` pays +48 of it back.

Whole diff, all files including docs: `56 files changed, 551 insertions(+), 558 deletions(-)`.

### Test counts

| Crate | Branch point | After |
| --- | ---: | ---: |
| `oneterm-vt` | 365 passed / 2 ignored (lib) + 0/1 + 6/0 (integration) | **364** / 2 + 0/1 + 6/0 |
| `oneterm-terminal` | 270 / 0 | **271** / 0 |
| `oneterm-local-shell` | 33 / 2 | **33** / 2 |
| `oneterm-ssh` | 67 / 0 | **67** / 0 |
| `oneterm-terminal-view` | 288 / 3 | **288** / 3 |

One test moved and none was deleted: `vt`'s
`render::tests::pump_yields_to_the_render_demand_within_a_bounded_number_of_chunks` is now
`terminal::handle::tests::a_pump_yields_to_the_demand_within_a_bounded_number_of_chunks`. It kept
its subject — the `Demand` contract against an **unfair** `std::sync::Mutex`, where the only reason
the renderer gets in within a bounded number of chunks is that the pump asks the flag and parks —
and every assertion (`waited < 2 s`, `chunks_waited <= 8`, `!demand.is_raised()`). Its payload
stopped being a `Terminal`, because the subject is the flag and `crates/terminal` has no access to
`crates/vt`'s render-test harness; the one dropped assertion, `state.rows().len() == 24`, was about
that harness rather than about `Demand`.

No test assertion was edited to accommodate this packet.

### `cargo doc`

`RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps` and `-p oneterm-terminal --no-deps`
both build clean. `R1`/`R2`'s dangling `` [`super::legacy_resize`] `` links are gone with the
sentences that carried them. Fifteen further links had to change: narrowing an item makes a
`[`Link`]` from a still-public doc a `private_intra_doc_links` error, so those became plain code
spans (`Screen::{set_template, blank_row, blank_slot}`, `GRAPHEME_MAX_LEN` ×2, `Modes`,
`crate::render::SyncState`, `Demand::release` ×2, `ctrl_bytes`, `MAX_TITLE_BYTES`,
`MAX_NOTIFICATION_BYTES`, `MAX_CLIPBOARD_BYTES`, `MAX_CWD_BYTES`, `crate::search`). Two
pre-existing rustdoc errors were fixed with them: a redundant explicit link target at
`grid/mod.rs:10` and a bare URL at `terminal/src/osc.rs:13`.

### `check-doc-paths.py`

```text
before:  Doc path check passed for 120 current paths in 10 documents.
after:   Doc path check passed for 188 current paths in 11 documents.
```

The widening added `docs/terminal-backend.md` to `DOCUMENTS` and dropped the back-tick requirement
from `PATH_PATTERN`, plus two guards the bare pattern needs: a look-behind so
`reference/gpui-kit/crates/component/src/` (the deliberately unchecked upstream clone) is not
matched from the middle, and a `CITATION` trim so `…rs:120-160` and `…rs::test_name` resolve to the
file. It immediately found three live paths: `crates/terminal/src/terminal.rs` and
`crates/terminal_view/src/{terminal_element,terminal_view}.rs` at `docs/terminal-backend.md:19-21`
— **Zed's** paths, cited as a reference under a rev lock, in this repository's path shape. They are
now spelled `zed/crates/…` and labelled as Zed's tree.

**Recorded blind spot.** The widening catches `D6` (`docs/refactor/ui-crate-restructure.md`, a bare
path) but **not** `D1` (`engine_shim.rs`), because a tree diagram lists its leaves as bare file
names under a directory line; reconstructing paths from box-drawing indentation costs more than it
catches. That limit is written into the script's module doc.

### `terminal-diagnostics`: the CI-gate branch, taken

Default branch taken — **keep the feature and add one CI step** — in three files plus `AGENTS.md`'s
quality-gate list: `.github/workflows/ci.yml` (both jobs), `scripts/ci-local.sh` and
`scripts/ci-local.ps1` now run
`cargo clippy --workspace --all-targets --features oneterm-app/terminal-diagnostics -- -D warnings`.

The step earns itself on the first run. It failed, exactly as the audit predicted rot would:

```text
error: struct `SshCommandDiagnostics` is never constructed
  --> crates\ssh\src\transport.rs:47:19
error: method `diagnostics` is never used
  --> crates\ssh\src\transport.rs:96:19
```

`crates/local-shell`'s 18 bare `#[cfg(feature = …)]` sites — the ones no CI step has ever compiled
— are clean. `crates/ssh`'s two are not: they are gated `#[cfg(any(test, feature = …))]`, and
nothing outside the tests reads them, so under the feature alone they are dead. The fix is the
honest description of who calls them, `#[cfg(test)]`, with a comment saying it is one line to widen
again when a diagnostics build grows somewhere to show them. The counters themselves
(`CommandCounters`, `record_failure`) keep the `any(test, feature)` gate, because they are written
under it.

### `pwsh scripts/ci-local.ps1`

Exit **0**. Totals across every step:

| | sections | passed | failed | ignored |
| --- | ---: | ---: | ---: | ---: |
| baseline (`main`) | 60 | 1935 | 0 | 14 |
| after | **60** | **1934** | **0** | **14** |

The −1 is fully explained and is not a lost test: `cargo test -p oneterm-vt --features vt-paranoid`
runs the `vt` suite a second time, so the one test that moved from `vt` to `terminal` is counted
twice at the baseline and once now. Within `cargo test --workspace` the total is unchanged (`vt`
364 = 365 − 1, `terminal` 271 = 270 + 1).

### `vt-bench`, tiers 1–5 (`--mib 2`, median of 3, geometry 160x45)

Recorded, never gated; run to show no engine-side regression. Tier 1 parser throughput
1301 MiB/s `plain_ascii`, 1594 `long_lines`, 464 `heavy_sgr`; tier 2 parse+grid 71.4 / 75.0 /
221.8 MiB/s; tier 3 per-frame 4.3–4.7 us `plain_ascii`, 16.4–19.9 us `tui_redraw`, 20.7–21.9 us
`scroll_region`; tier 4 resize 8 us at 0 rows, 2.5 ms at 10 000, 28.8 ms at 100 000; tier 5 live
heap 1364 bytes/row. Two consecutive runs differ by up to 20 % on the frame tiers, which is the
bench's own noise — nothing in this packet changes generated code, only which module path a symbol
is reachable from.

### `harness.db` rows (not applied — the database is gitignored and mirrored in the main checkout)

```python
import sqlite3, datetime
now = datetime.datetime.now(datetime.timezone.utc).isoformat()
db = sqlite3.connect("harness.db")
db.execute(
    "UPDATE stories SET status = ?, updated_at = ? WHERE id = ?",
    ("implemented", now, "US-0090"),
)
db.execute(
    """UPDATE stories
          SET proof_unit = 1, proof_integration = 1, proof_e2e = 0,
              proof_platform = 1, proof_verified = 1,
              evidence_summary = ?
        WHERE id = ?""",
    (
        "ci-local.ps1 exit 0: 60 sections / 1934 passed / 0 failed / 14 ignored "
        "(baseline 1935; the -1 is the vt test that moved to oneterm-terminal, "
        "double-counted at baseline by the vt-paranoid step). "
        "pub-item proxy: vt 535 -> 392 (<=400), terminal 301 -> 269 (<=270). "
        "crates/vt holds no atomic. check-doc-paths 120/10 -> 188/11. "
        "cargo doc clean for both crates under -D warnings. "
        "terminal-diagnostics: kept + CI-gated; the new step caught two dead items "
        "in crates/ssh/src/transport.rs. "
        "Line delta -79 production lines, short of the -80 gate; see the packet.",
        "US-0090",
    ),
)
db.commit()
```

### Gaps

1. **The line-delta gate is missed by one line** (−79 against −80), with the arithmetic above. No
   deletion was padded to close it.
2. **`oneterm-vt`'s surface is 392, not the audit's ≈ 320 ambition.** The structural reason —
   closing `grid` and `intern` turns about fifteen test-only engine methods into dead code — is
   recorded above. A future packet that wants the smaller number has to decide whether those
   methods are kept as `#[cfg(test)]` helpers or deleted; that is a question about the engine's
   test harness, not about visibility.
3. **No E2E, by design** (Verification Plan). Nothing here is observable from a GUI.
4. **`vt-bench` is a single machine, single run pair.** It is recorded, never gated, and it is
   evidence of "nothing exploded", not a measurement.
5. **`harness.db` was not written** — it is gitignored and this work ran in a worktree; the rows
   are the snippet above, for the main checkout to apply after the merge.

## Reconciliation

Docs changed by this packet:

| Doc | Change |
| --- | --- |
| `docs/terminal-backend.md` | `D1` (the `engine_shim.rs` tree entry, deleted at `US-0085`) and `D2` (`content.rs` "+ the legacy shape"); the `TerminalHandle` description in §5.1 — it stops naming `Engine`, names `Demand` as the adapter's own type in `handle.rs`, and names `render_demand_raised()` with the correct "the asking takes nothing away" contract; the `lock_unfair` / `try_lock_unfair` sentence deleted; §5.2's and §6's `take_render_demand()` citations; and the three Zed reference paths at `:19-21` relabelled `zed/crates/…`, which is what the widened checker demanded |
| `docs/architecture.md` | `D3` — `oneterm-terminal` relabelled "Terminal adapter" and described as the lock plus the shared pump; `D4` — added the missing `oneterm-vt` row (as "Terminal engine"), plus `oneterm-actions`, `oneterm-theme`, `oneterm-highlight` and `oneterm-tools` |
| `docs/agents/structure.md` | `D5` — the `vt/` subtree now lists all nine subdirectories with what each owns, instead of 4 files out of 54; `D6` — the `docs/refactor/ui-crate-restructure.md` line (a directory that does not exist) replaced with `docs/architecture.md`; `D7` — `url.rs / url_policy.rs` corrected to `url_policy.rs` |
| `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md` | It **did** name `Demand`'s crate — a whole "Placement: the engine crate, as shipped, and it is a stated exception" block. Rewritten: the exception is gone, the primitive is the adapter's, and the HLD sentence it excepted is now true. Also the `take_render_demand()` bullet and the "clears by asking" sentence, both of which described the pre-`US-0082` one-shot flag |
| `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md` | Added an "As shipped (`US-0090`)" note under the Interfaces block. The block is a design-time sketch that reality diverged from in three ways (it names `anchor`, `damage`, `strip` and `testing` modules that never existed, publishes twenty names that have no consumer, and says `parser` is `pub(crate)` when the session logger needs it public). The note records the divergence and points at `crates/vt/src/lib.rs` as the authority, and names `RowRef` and `cluster_width` as the two deliberate no-consumer exports |
| `AGENTS.md` | §4's quality-gate command list gains the `terminal-diagnostics` clippy step, so the list still matches what `ci-local` runs |
| `scripts/check-doc-paths.py` | Module doc rewritten for the widened pattern, including the recorded blind spot |
| `crates/vt/src/lib.rs`, `crates/terminal/src/handle.rs` | Crate and module docs describe what is left: no atomic in the engine, `Demand` and the lock in one file, and why only `grid`, `intern` and `parser` stay reachable by path |

Explicit no-change reasons for the four docs the packet names:

- **`docs/PROJECT.md`** — no change. Nothing in § Important Boundaries moves: `TerminalSession`,
  `SessionFactory`, `SftpBackend`, `AppServices`, `WorkspaceCommands` and every registered dock
  panel name keep their shape, and no user-visible behaviour, persisted format or setting changes.
  The Rust surfaces that shrank are workspace-internal.
- **`docs/agents/crate-dependency-rules.md`** — no change. The edge set is identical: moving
  `Demand` out of `crates/vt` removes a type, not a dependency, and `crates/vt` still depends on no
  OneTerm crate. R7's premise gets *stronger* (the engine now holds no atomic at all), which the
  rule's own wording already covers, so the table needs no edit. `python
  scripts/verify-dependency-graph.py` passes.
- **`docs/agents/persistence.md`** — no change. No persisted schema, no storage mechanic and no
  migration is touched; this packet writes nothing to disk.
- **`IN-0029/low-level-design/events-and-api.md`** — **changed**, see the table above. Read first to
  check the visibility pass narrowed *toward* it; it does for the module partition (the LLD wants
  everything but a named list `pub(crate)`), and away from it for `parser`, which the LLD wants
  private and which `crates/terminal/src/logging.rs` needs. Every item the LLD names as public and
  that still exists stays public except the twenty with no consumer, each recorded above.

## Handoff

`US-0091` starts from this packet's head: it rewrites the same `TerminalSession` surface and
deletes `model::ResizePolicy`, which this packet deliberately leaves alone. `US-0092` is
independent and may run in parallel — but if it lands first, `RowRef` gains an external user and
must not be dropped from `crates/vt/src/lib.rs`'s re-exports.
