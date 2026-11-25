# High-Level Design: Terminal crate tidy

Intake: IN-0032
Lane: normal
Date: 2026-09-14

## Idea

Keep every crate boundary exactly where the `IN-0029` migration put it, and delete the things that
migration left behind.

The audit settled the boundary question first, and the answer was "the boundaries are right". They
were not fork-era leftovers: they were re-derived during the rewrite, and they are the reason a
fifteen-packet migration could be verified one packet at a time. What is left over is small,
specific, and mostly self-documented — six pieces of code carry comments asking to be deleted by a
packet that has already shipped, 187 of `oneterm-vt`'s 507 public items have no consumer outside
the crate, seven doc paths point at deleted files, and two per-frame loops in the render path
still cost viewport area rather than change.

So this intake is four small, independent deletions and one scoping fix, with the crate graph
frozen. Nothing here changes what OneTerm draws or how it behaves; the only observable difference
should be that a frame in which one row changed stops doing the work of a frame in which all
45 rows changed.

## Diagram

The graph does not move. What moves is which side of an existing boundary two small things sit on,
and how much work two existing loops do.

```text
                     BEFORE                                        AFTER

 crates/vt  (L0 leaf, no OneTerm dep)              crates/vt  (L0 leaf, no OneTerm dep)
   parser · cell · grid · reflow                     parser · cell · grid · reflow
   selection · render · graphics                     selection · render · graphics
   11 pub mod, 507 pub items                         3 pub mod, ~320 pub items
   render/demand.rs  Arc<AtomicUsize> ◄── the only    (no atomic, no interior mutability —
                        atomic in the crate           lib.rs:4 is now true)         US-0090
   tests/corpus/  3.2 MB, read by nobody here
        │  read via  ../vt/tests/corpus                       US-0093
        ▼
 crates/tools                                      crates/tools
   corpus.rs, vt-bench, vt-corpus                     corpus/  3.2 MB  ◄── resolved from
                                                      CARGO_MANIFEST_DIR, no sibling hack

 crates/terminal  (L0 adapter, gpui-free)          crates/terminal  (L0 adapter, gpui-free)
   handle.rs  Engine newtype + exit()                handle.rs  FairMutex<Terminal>
              lock / lock_unfair       (identical)              lock / try_lock
              try_lock / try_lock_unfair                        render_demand_raised
              take_render_demand / render_demand_raised         Demand  ◄── moved here  US-0090
   model.rs   ResizePolicy + 2 From impls
   session.rs impl_pty_terminal_session!  286 lines  session.rs  struct PtySession   US-0091
                 │ expands into two other crates                    (one concrete type)
                 ▼                                              (model::ResizePolicy deleted)
   backend/  pump · osc_router · state · event_sink   backend/  … + byte_budget.rs   US-0093
                                                                     ▲        ▲
 crates/local-shell        crates/ssh                           ─────┘        └─────
   session_terminal.rs       session_terminal.rs         local-shell           ssh
     macro call (42)           macro call (55)             owns a PtySession, impls
   event_loop.rs  byte budget  transport.rs  byte budget    capabilities() only
        (35 duplicated lines between them)

 crates/terminal-view  (L3 feature, gpui)          crates/terminal-view
   render/plan_cache.rs                              render/plan_cache.rs
     any row dirty ──▶ url_masks_into(whole            any row dirty ──▶ rescan the dirty rows
     viewport) then compare the whole mask             plus their wrap-connected neighbours
                                                                                    US-0092
 crates/terminal/src/content.rs                    crates/terminal/src/content.rs
   last_content_row: every cell of every row          last_content_row: skip a row whose
   × 3 interner lookups, every frame                  RowRef says !is_allocated() || occ()==0
```

### Why each crate stays — the boundary table

Each row is the one fact that makes merging that crate cost more than it saves.

| Crate | Production lines | Why it stays separate | If it were merged |
| --- | ---: | --- | --- |
| `vt` | 11 100 | Leaf with **no OneTerm dependency at all** — stricter than R7 requires. No gpui, no I/O, no lock. 11 283 lines of tests run with no UI and no PTY; `cargo test -p oneterm-vt` is the fastest loop in the repo. | The engine's test loop would drag in whatever it was merged with. |
| `terminal` | 5 702 | The gpui-free half of the terminal feature, not "an adapter": it owns the lock (`TerminalHandle`), the event-delivery policy (`backend/`, 1 024 lines both protocols share), `TerminalSession`, and about 2 600 lines of gpui-free view support that must live below `gpui`. R7's engine-swap seam is here. | There would be no seam left to swap an engine behind. |
| `pty` | 1 853 | Leaf, **not Windows-only** (`unix.rs` is 526 lines), and it has **two** consumers: `local-shell` and `crates/tools/src/bin/pty-throughput.rs`, which exists precisely to measure the PTY without a grid. `DEC-0014` extracted it deliberately so it survives independently. | The throughput probe would depend on the whole terminal stack, or ConPTY would be duplicated — and `tools` may only reach L0 leaves. |
| `local-shell` | 1 003 | R3/R8: no UI crate may reach it. Holds the only read loop that keeps the engine lock across reads, which is why `MAX_LOCKED_READ` exists here and nowhere else. 58 % of the crate is its own tests. | `polling` and `windows-sys` would enter L0. |
| `ssh` | 4 557 | R3/R8, and it hides a whole tokio runtime behind a trait. | See the Option C table below — this is the expensive one. |
| `terminal-view` | 13 491 | The R7 boundary seen from the gpui side. | The workspace's largest crate would grow further. |
| `agent-ui` | 1 200 | Its entire public surface is `init(cx)` plus `AgentListView`; the model it renders already lives in `crates/state`. A separate parallel compile unit at L3, and R12 keeps one feature's panel registration out of another's `init()`. | Two dock-panel registrations in one `init()` — exactly what R12 separates. LOC saved: **0**. |
| `tools` | 3 812 | Outside L0–L4; nothing depends on it. Carries a crate-scoped `GPL-3.0-only` term for `doom-fire.rs` with a matching `deny.toml` exception. | Nothing to merge it into; it is the one crate that legitimately reaches sideways. |
| `core` | 1 983 | R6: pure domain, no gpui, no `oneterm-pty`, no `oneterm-vt`. | R6 would stop being checkable. |

### Option C, and why it is rejected

The owner has already decided against it. Recorded here so the next reader does not re-open it.

| Proposed merge | Rule it breaks | The measurement |
| --- | --- | --- |
| `local-shell` + `ssh` → `terminal` | **R3 violated outright.** "No UI→backend edge" becomes unverifiable: `cargo tree -i oneterm-ssh -e normal`, which R3 names as its verification, would return the whole workspace instead of only `oneterm-app`. | `crates/terminal` is depended on by **seven** crates — `terminal-view`, `sftp-ui`, `session-ui`, `state`, `agent-ui`, `workspace`, `app`. `crates/ssh` pulls `russh`, `russh-sftp`, `tokio`, `tokio-util`. Merging puts an async SSH client on the compile path of the settings panel, and touching the SSH transport rebuilds the whole UI. |
| | **R7 violated.** `terminal` must stay gpui-free *and* engine-swappable; it would then own two protocol stacks. | The shared plumbing is **already** in `crates/terminal/src/backend/` and both backends already use it. Duplicated code actually recovered by the merge: **about 35 lines** — the byte budget, which `US-0093` collects without moving a crate. |
| | **R8 deleted.** "Backends implement traits only" would describe nothing, because there would be no backend crates. | The two read loops are structurally different, not duplicated: the local one holds the engine guard across reads under `MAX_LOCKED_READ`; the SSH one *cannot* hold a guard across reads at all. |
| `pty` → `local-shell` | The Layers rule: `tools` may only reach down to L0 leaves, and `pty-throughput.rs` declares `oneterm-pty`. | 1 853 production + 906 test lines moved. Duplicated code removed: **0**. |
| `agent-ui` → `terminal-view` | **R12** blurred: one `init()` would register two features' dock panels. | 1 200 lines added to the largest crate, one parallel compile unit lost, `Cargo.toml` saved: one. |

Option C's whole LOC delta is about −100 manifest and re-export lines. **No production code is
removed** — the code merely changes file. Against that: four L0/L3 crates stop compiling in
parallel, and the layering rules that a fifteen-packet migration just re-proved become false.

## UI Wireframe

`N/A — no UI surface.`

No packet in this intake changes a screen, a control, a colour, a glyph or a layout. `US-0092`
changes how much work is done to produce a frame; the frame it produces must be identical, and
that identity is `US-0092`'s primary acceptance criterion.

## Data Flow

Nothing about the runtime data flow changes. The four packets each move one seam:

1. **`US-0090` — residue and visibility.** `TerminalHandle` stops wrapping `Terminal` in an
   `Engine` newtype whose only inherent method was an empty `exit()`, and the three methods that
   are byte-identical copies of their neighbours are removed with their four call sites rewritten.
   `Demand` moves from `crates/vt/src/render/demand.rs` to `crates/terminal/src/handle.rs`, next
   to the policy that already uses it: the view raises it, the pump reads it, and neither crosses
   the engine boundary any more. `pub` becomes `pub(crate)` wherever the audit proved no other
   crate names the item — eight of `vt`'s eleven root modules, 56 methods in `screen.rs`, the 22
   dead names in `vt`'s root re-export block, and `terminal`'s `color_classification`,
   `key_encode`, `osc_color`, `paste` and `osc` module surfaces.
2. **`US-0091` — one concrete session.** `impl_pty_terminal_session!` expands about 45 forwarding
   methods into `local-shell` and `ssh` today. Its own doc (`session.rs:446-453`) already states
   the design that replaces it: *"the local shell and SSH sessions differ only in their
   `TerminalCapabilities`, their `SessionKind`, their `ResizePolicy` and how the channel is torn
   down."* So one `PtySession` struct in `crates/terminal` holds `SharedTerminal`, `SharedState`,
   the transport, `SessionKind`, an `oneterm_vt::ResizePolicy`, a close hook and the `marked_text`
   / `event_rx` cells; each backend owns one and implements `capabilities()` only. The four
   composed traits (`TerminalRender` / `TerminalInput` / `TerminalIme` / `TerminalLifecycle`)
   **stay** — the macro is the problem, not the partition, and `test_support.rs`'s
   `FakeTerminalSession` implements them separately. `model::ResizePolicy` then has no namer left
   and goes with it.
3. **`US-0092` — change-scoped frames.** Two loops learn what the rest of the render path already
   knows. The URL mask cannot simply scan dirty rows, because a changed row can start or end a URL
   that continues into an untouched one — but `PlanCache` already tracks `self.wraps[r]` per row
   for exactly this reason, so the correct scope is *the dirty rows plus their wrap-connected
   neighbours*, not the viewport. `last_content_row` gains a skip on the engine's existing
   over-approximating hint: `RowHeader.occ` is documented as *"no column at or above `occ` has been
   touched since the last reset"* — false positives allowed, never false negatives, which is
   precisely what a skip needs.
4. **`US-0093` — one home each.** The parity corpus moves to the crate that reads it, so
   `corpus_root()` resolves from its own `CARGO_MANIFEST_DIR` instead of reaching into a sibling
   crate's `tests/` directory. The byte-budget reservation idiom becomes one `ByteBudget` in
   `crates/terminal/src/backend/`, which both backends already depend on; the two 4 MiB constants
   stay where they are, because their values are each backend's policy.

## Old problems, new state

| Old problem | Evidence it is a problem | New state |
| --- | --- | --- |
| Code alive only because a shipped packet forgot to delete it | Six items carry their own "delete me with `US-0083`/`US-0084`/`US-0081`" comments; those packets shipped | Deleted or moved; the comments go with them (`US-0090`, `US-0091`) |
| Three methods that are byte-identical copies of their neighbours | `lock_unfair` ≡ `lock`, `try_lock_unfair` ≡ `try_lock`, `render_demand_raised` ≡ `take_render_demand`; the comment at `handle.rs:147` admits the first two | One method each, and the surviving name states the contract: a non-consuming `render_demand_raised`, not a `take_` that takes nothing (`US-0090`) |
| `crates/vt/src/lib.rs:4` claims no lock and no interior mutability, and `:7` contradicts it | `render/demand.rs` is the crate's only atomic, and its own doc says it is the adapter's | True as written; R7 gets stronger (`US-0090`) |
| 187 of 507 `vt` public items and 74 of 325 `terminal` items have no external consumer | Audit §5, counting a comment mention as a use, so a lower bound | About 320 and about 250; `cargo doc` for the engine describes an API a reader can hold in their head (`US-0090`) |
| Seven comments cite deleted files; seven doc paths point at nothing | `vendor/`, `sixel_tests.rs`, `legacy_resize`, `docs/refactor/`, a `vt` subtree listing 4 of 54 files | Re-pointed or deleted, and `check-doc-paths.py` widened so the next one fails CI on the commit that creates it (`US-0090`) |
| A 286-line macro expands a 45-method surface into two other crates | `migration.md` records it blocking `US-0083`'s and `US-0084`'s resize-policy change, and forcing a dead enum to stay alive in both backends | One concrete type in one crate; `cargo test -p oneterm-terminal` stops recompiling a macro that expands elsewhere (`US-0091`) |
| A frame in which one row changed does the work of a frame in which every row changed | `plan_cache.rs:143-152` and `content.rs:56-67`, both O(viewport) per frame; `DEC-0015` removed this shape one layer down | Both scoped to change, proved by a counted-work test that fails against today's code (`US-0092`) |
| 3.2 MB of test data in a crate that never reads it, reached by a sibling-path hack | `grep -rn corpus crates/vt/src crates/vt/tests/*.rs` returns only prose; `crates/tools/src/corpus.rs:88-92` does `.parent().join("vt/tests/corpus")` | Data and reader in one crate, path resolved from its own manifest dir (`US-0093`) |
| The only genuine cross-backend duplication, about 35 lines, copied twice | `event_loop.rs:120-140` against `transport.rs:121-134` — the same `fetch_update(checked_add().filter(<= BUDGET))` | One `ByteBudget` in `backend/`, each backend keeping its own constant (`US-0093`) |
| A feature guarding about 200 lines that CI never compiles | `grep -rn terminal-diagnostics .github/ scripts/` → 0 hits | Either CI-gated or gone; the choice and its reason recorded in `US-0090` |

## Detail Design

- [x] Detail design: not needed
- Reason: normal lane, and every packet's design already exists as prose in the audit with file
  and line references. `US-0090` and `US-0093` are deletions and moves with zero call sites or a
  byte-identical twin; `US-0091`'s target shape is specified by the macro's own doc comment and is
  recorded in this document's Data Flow; `US-0092`'s two fixes are a scope narrowing each, using
  primitives the engine already exposes and documents. The one genuinely uncertain item —
  whether `US-0091` is a net line win or a wash (the audit estimates anywhere from −80 to −200) —
  is a sizing question, not a design question, and the packet records the measured number rather
  than predicting it. If `US-0091` turns out to need ownership moved out of the backend structs in
  a way this document does not describe, that packet adds its own file under `low-level-design/`
  rather than expanding this one.
