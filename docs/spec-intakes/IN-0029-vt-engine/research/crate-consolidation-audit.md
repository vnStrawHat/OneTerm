# Crate consolidation audit — terminal-related crates after IN-0029

Status: research note. **Read-only audit, no code changed.** No owning work packet; this exists
so the owner can choose an option, after which each chosen item needs its own packet.

Scope: `crates/{vt, terminal, terminal-view, ssh, local-shell, pty, tools, agent-ui}` plus
`crates/core` where terminal types live, on `main` @ `782bb77` (the IN-0029 / IN-0031 merge),
2026-09-14.

Method: read the sources and manifests; `grep`/`wc` for counts; no build, no GUI, no benchmark
run. Every performance number quoted is from an existing recorded measurement in
`evidence/` or `research/perf-baseline.md`, cited inline.

---

## (a) Executive summary

### The short answer

**The crates should not be merged.** The boundaries the owner is asking about were not left
over from the fork era — they were re-derived during IN-0029 and they are the reason the
migration could be done in fifteen independently verifiable packets. Merging any of the pairs
named in the brief makes the build slower, the tests heavier, or the layering rules false, and
none of them removes a meaningful amount of duplicated code, because the duplication was
already removed: `crates/terminal/src/backend/` **is** the shared plumbing, and both backends
already use it.

What *is* left over is small, specific, and self-documented. Six pieces of code carry comments
saying "delete me with the packet that moves my call site", and that packet has shipped. Three
hundred public items in `oneterm-vt` have no consumer outside the crate. Seven doc paths point
at files that no longer exist. And two per-frame loops in the render path still scale with
viewport area rather than with change — which is precisely the shape `DEC-0015` was written to
eliminate everywhere else.

So: **take Option A, plus B1 and B4 from Option B. Skip Option C entirely.**

### The sizes, corrected

The brief's line counts include test code. Split out, the picture changes:

| Crate | total src | test (`*_tests.rs`, `*_props.rs`, `*_bench.rs`, `test_support.rs`) | **production** |
|---|---:|---:|---:|
| `vt` | 22 383 | 11 283 | **11 100** |
| `terminal-view` | 21 513 | 8 022 | **13 491** |
| `terminal` | 11 084 | 5 382 | **5 702** |
| `ssh` | 7 364 | 2 807 | **4 557** |
| `tools` | 4 127 | 315 | **3 812** |
| `core` | 2 925 | 942 | **1 983** |
| `pty` | 2 759 | 906 | **1 853** |
| `local-shell` | 2 398 | 1 395 | **1 003** |
| `agent-ui` | 1 292 | 92 | **1 200** |

"11k lines for an adapter" is really **5 702**, of which 1 024 is the backend pump layer shared
by both protocols and roughly 2 600 is gpui-free view support (key/mouse encoding, paste,
search, URL policy, palette, security policy) that has to live *somewhere* below `gpui`.
`local-shell` is **1 003** production lines — 58 % of that crate is its own test suite.
`vt` is a 1:1 code-to-test ratio, which is the right ratio for a VT engine.

### Options

| | Option A — minimal tidy | Option B — targeted merges | Option C — full consolidation |
|---|---|---|---|
| **What** | Delete the six self-documented residues; move `Demand` out of `vt`; tighten `pub`; fix the doc rot; decide `terminal-diagnostics` | A, plus: replace `impl_pty_terminal_session!` with one concrete type (B1); move the parity corpus data to its only reader (B3); fix the two per-frame viewport scans (B4); share the byte-budget (B5) | Fold `pty` + `local-shell` + `ssh` into `terminal`; fold `agent-ui` into `terminal-view` |
| **LOC delta** | **−90 code**, ~260 `pub` → `pub(crate)`, ~46 doc lines. Optional −200 more if `terminal-diagnostics` goes | **−280 code** total (A's −90, plus −150 from B1, −43 from the dead `ResizePolicy` enum, −35 from B5, +~45 for B4) | −~100 manifest/re-export lines. **No production code is removed** — the code merely changes file |
| **Build / test cycle** | Unchanged | Slightly better: `cargo test -p oneterm-terminal` stops recompiling a macro that expands into two other crates; `crates/vt` sheds 3.2 MB of test data it never reads | **Worse, materially.** `russh` + `tokio` + `polling` + `windows-sys` enter L0. Every crate that depends on `oneterm-terminal` today — `terminal-view`, `sftp-ui`, `session-ui`, `state`, `agent-ui`, `workspace`, `app`: **7 crates** — transitively grows an async-SSH stack. Touching the SSH transport rebuilds the whole UI. The four L0/L3 crates stop compiling in parallel |
| **R1–R12** | None touched. **R7 gets stronger**: `oneterm-vt` loses its only atomic (`Demand`), making its "holds no lock and no interior mutability" claim (`crates/vt/src/lib.rs:4`) true instead of immediately contradicted at `:7` | None violated. B1 stays inside `crates/terminal`; B3 touches no crate graph; B5 adds a type to `crates/terminal/src/backend/` which both backends already depend on | **R3 violated outright** ("no UI→backend edge" becomes unverifiable — `cargo tree -i oneterm-ssh` would list every UI crate). **R7 violated** (`terminal` must stay gpui-free *and* engine-swappable; it would now own two protocol stacks). **R8 deleted** (there would be no backend crates). **R2** loses its cleanest example. Also: `agent-ui`→`terminal-view` costs a parallel-compile unit and blurs **R12** |
| **Risk** | Very low. Every deletion has zero call sites or a byte-identical twin, proven by grep | B1 medium (rewrites both backends' whole `TerminalSession` surface — but `backend_tests.rs` is 1 280 lines and both backends have their own suites). B3/B5 very low. B4 low-medium (touches the render path; `element_tests.rs` + `plan_cache` tests cover it) | High, and unrecoverable: the layering rules are the product's main structural invariant and they were just re-proved by a fifteen-packet migration |
| **Effort** | 1–2 packets | 4–5 packets on top of A | 6+ packets, and a `DEC` reversing `DEC-0014`'s layering premise |

### Recommendation

**Option A now, as one packet.** It is pure deletion of things whose own comments ask to be
deleted, plus a visibility pass. Nothing in it can regress behaviour.

**Then B1 and B4, one packet each.** B1 because `impl_pty_terminal_session!` is the single
piece of coupling the migration kept tripping over — `low-level-design/migration.md` records it
blocking `US-0083`'s and `US-0084`'s resize-policy change and forcing the fork's manifest line
to survive in both backend crates long after no backend source named it. B4 because the two
remaining per-frame full-viewport scans are the last places the render path still costs
viewport area instead of change.

**B3 and B5 are cheap and optional.** B2 (collapsing `TerminalContent`'s forwarders) and the
`frame.rs` vocabulary mirror are listed in the deletion table but I do **not** recommend them:
the churn exceeds the benefit and the mirror still does one real job (flattening dim/bright
into a theme lookup key).

**Do not do Option C.** The single sentence that settles it: `crates/terminal` is depended on
by seven crates including every UI crate, and `crates/ssh` pulls `russh` + `tokio`; merging
them puts an async SSH client on the compile path of the settings panel.

---

## (b) Detailed findings

## 1. Boundaries — does each crate still earn its keep?

| Crate | Owns | Why separate, today | Verdict |
|---|---|---|---|
| `vt` | Parser, cell/intern, grid+scrollback, reflow, selection, damage/render state, graphics | Leaf: no OneTerm dep, no gpui, no I/O, no lock. 11 283 lines of tests run with no UI and no PTY. `cargo test -p oneterm-vt` is the fastest loop in the repo | **Keep.** The cleanest boundary in the workspace |
| `terminal` | The lock (`TerminalHandle`), event-delivery policy (`backend/`), `TerminalSession`, and the gpui-free view support | R7: gpui-free so it is unit-testable and so a future engine swap has a seam. Also the only place `ssh` and `local-shell` can share plumbing without depending on each other | **Keep.** Not "an adapter" — it is the gpui-free half of the terminal feature |
| `pty` | ConPTY (Windows) / `openpty` (Unix), the evented traits, the ring | Leaf, **and it is not Windows-only** — `crates/pty/src/unix.rs` is 526 lines. Two consumers: `local-shell` and `crates/tools/src/bin/pty-throughput.rs`. Platform gating is contained here instead of spreading through a backend | **Keep** — see 1.1 |
| `local-shell` | 1 003 production lines: the poll loop, the transport, the spawn path | R3/R8: a UI crate must never reach it. Holds the only loop that keeps the engine lock across reads, which is why `MAX_LOCKED_READ` exists here and nowhere else | **Keep** |
| `ssh` | russh client, SFTP, tunnels, jump hosts, agent auth | R3/R8, and it hides a whole tokio runtime | **Keep** |
| `terminal-view` | The gpui terminal feature | R7 boundary from the other side | **Keep** |
| `agent-ui` | One dock panel over a registry that lives in `state` | R12 self-registration; a separate parallel compile unit | **Keep** — see 1.4 |
| `tools` | Five dev binaries + the VT bench/parity library | Outside L0–L4; never a dependency of the app | **Keep, with one file move** — see 1.5 |
| `core` | Pure domain | R6 | **Keep** |

### 1.1 `pty` into `local-shell`?

**No.** Three reasons, in order of weight:

1. `crates/tools/src/bin/pty-throughput.rs` is the second consumer, and it exists precisely to
   measure the PTY *without* a grid. `crates/tools/Cargo.toml:51` declares `oneterm-pty`. Folding
   `pty` into `local-shell` would make the throughput probe depend on the whole terminal stack,
   or duplicate the ConPTY code — and `local-shell` cannot be a `tools` dependency without
   breaking the "tools may only reach down to L0 leaves" rule (`crate-dependency-rules.md`
   Layers §).
2. `pty` is a leaf with **no OneTerm dependency at all**, which is stricter than R7 requires.
   `cargo test -p oneterm-pty` (906 test lines, including `loopback_tests.rs`) runs with nothing
   else compiled. Merging puts those tests behind `oneterm-core` + `oneterm-terminal` + `oneterm-vt`.
3. `DEC-0014` extracted it deliberately, *before* the engine work, "so it survives independently"
   — and `US-0071` moved `event_loop_tests.rs`'s loopback PTY loop into it for exactly this reason
   (`migration.md`, "Tests that change").

LOC moved if done anyway: 1 853 production + 906 test. Duplicated code removed: **0**.

### 1.2 `local-shell` + `ssh` into `terminal` as backend modules?

**No, and this is the one the brief most expects a yes on.** The measurement:

The shared plumbing is *already* in `crates/terminal/src/backend/` — 1 024 production lines:
`pump.rs` (250), `osc_router.rs` (416), `state.rs` (230), `event_sink.rs` (102), `transport.rs`
(21), `mod.rs` (35). Both backends consume it. What is left in each backend is genuinely
protocol-specific:

| Concern | `local-shell` | `ssh` | Duplicated? |
|---|---|---|---|
| Read loop | `event_loop.rs:399-545` — a `polling::Poller` loop that holds the engine guard across reads and caps each read at `MAX_LOCKED_READ` | `task.rs:55-138` — a `tokio::select!` that locks per chunk | **No.** Structurally different; the SSH one *cannot* hold a guard across reads |
| Pump usage | `pump.advance` + manual guard management + `finish_batch_blocking` | `pump.process_chunk` + `finish_batch().await` | Shared already |
| Transport | `transport.rs` (74 lines) — `ShellMsg` over an `mpsc::SyncSender` + a `Poller::notify` | `transport.rs` (222 lines) — `Cmd` over an `async_channel` + a closing flag + resize coalescing | Partly — see B5 |
| Byte budget | `event_loop.rs:120-140` — `fetch_update(checked_add.filter(<= LOCAL_COMMAND_BYTE_BUDGET))` | `transport.rs:121-129` — the same idiom against `SSH_COMMAND_BYTE_BUDGET` | **Yes, ~35 lines.** The one real duplicate |
| `TerminalSession` impl | `session_terminal.rs` (42) | `session_terminal.rs` (55) | Both are one macro call + `capabilities()` — see B1 |

**Total duplicated code between the two backends: about 35 lines** (the byte budget), plus the
macro-call shape. That is the entire prize, and B5 collects it without moving a crate.

The cost of merging is not a judgement call, it is a dependency fact. `crates/ssh/Cargo.toml`
pulls `russh`, `russh-sftp`, `tokio`, `tokio-util`; `crates/local-shell` pulls `polling` and
(via `pty`) `windows-sys` / `libc`. `crates/terminal` is depended on by `terminal-view`,
`sftp-ui`, `session-ui`, `state`, `agent-ui`, `workspace` and `app`. Merging hands all seven an
async SSH stack, makes `cargo tree -i oneterm-ssh -e normal` (R3's stated verification) return
the whole workspace, and makes `R8` ("backends implement traits only") describe nothing.

### 1.3 `agent-ui` into `terminal-view`?

**No.** `agent-ui` is 1 200 production lines in three files whose entire public surface is
`init(cx)` + `AgentListView` (`crates/agent-ui/src/lib.rs:21-32`). The model it renders already
lives in `crates/state` (`agent_registry.rs`, `agent_model.rs`), so there is no shared code to
recover. Merging would: add 1 200 lines to the workspace's largest crate (13 491 production),
remove a parallel compile unit at L3, and put two dock-panel registrations in one `init()`,
which is exactly what R12 separates. LOC saved: **0** (one `Cargo.toml`).

### 1.4 `tools` split?

Optional, low value, one real argument. `crates/tools/Cargo.toml:11` declares
`license = "Apache-2.0 AND GPL-3.0-only"` because `src/bin/doom-fire.rs` (480 lines) is a
GPL-3.0 port, and `deny.toml` carries a crate-scoped exception for it. That GPL term is attached
to the same crate as the VT parity gate (`tests/corpus_check.rs`) that runs in
`cargo test --workspace`. Nothing ships, so nothing is actually at risk — but if the owner wants
clean licence metadata, splitting `doom-fire` into its own crate is the one-file change that
buys it. I would not spend a packet on it.

### 1.5 Boundaries that are *wrong* today

Two, both small:

- **`crates/vt/tests/corpus/` (3.2 MB, 45 + 1 recordings) is read by nothing in `crates/vt`.**
  Verified: `grep -rn corpus crates/vt/src crates/vt/tests/*.rs` returns only prose. Its only
  reader is `crates/tools/src/corpus.rs:88-92`, which reaches it as
  `CARGO_MANIFEST_DIR/../vt/tests/corpus` — an undeclared filesystem-sibling assumption between
  two crates. **B3**: move the directory (and its `NOTICE`) to `crates/tools/corpus/` and delete
  the `.parent().join("vt/...")` hack.
- **`Demand` lives in `crates/vt`.** `crates/vt/src/render/demand.rs` is an `Arc<AtomicUsize>`
  wrapper, ~40 lines of code. Its own module doc, `:20-22`, says: *"This is the **adapter's**
  primitive, not the engine's: it is reachable from no engine type, holds the crate's only
  atomic, and lives here so the contract and its test have one home **until `US-0081` moves the
  pump loop over**."* `US-0081` shipped at `f9af66c`. Inside `crates/vt` it is used only by
  `render_tests.rs:787`. Moving it to `crates/terminal/src/handle.rs` — where the policy that
  uses it already is — makes `lib.rs:4`'s "holds no lock and no interior mutability" true.

---

## 2. Migration residue

Everything the shim era created as *code* is gone: `engine_shim`, `LegacySnapshot`,
`line_accounting`/`LineAccounting`, `total_lines_scrolled`, `Line(i32)`, the `vte` dev-oracle,
the `vendor/` tree, the `[patch]` block — **zero identifier hits across `crates/`, `Cargo.toml`,
`Cargo.lock`, `deny.toml`, `.github/workflows/`, `scripts/`**. There are **zero**
`#[allow(dead_code)]`, **zero** `#[allow(unused)]` and **zero** `#[deprecated]` in the whole
workspace; the eight `#[allow(...)]` that exist are clippy-only (`too_many_arguments` ×5,
`cast_lossless`, `permissions_set_readonly_false`). That is an unusually clean decommission and
it deserves saying.

What remains is **dead wrappers with live exports** and **stale prose**.

### 2.1 Dead code with zero call sites

| # | Item | Location | Lines | Evidence it is unused |
|---|---|---|---|---|
| E1 | `Engine::exit()` | `crates/terminal/src/handle.rs:49-57` | 9 | `grep -rn "\.exit()" crates/` → the only hits are `pty/src/loopback_tests.rs:249` (a test *name*) and this function's own doc. **Zero callers.** Its own comment: `// ponytail: a no-op that exists only so crates/local-shell's read loop — US-0083's — is not edited by this packet. Upgrade path: delete the call site and this type with it.` |
| E2 | `struct Engine` (the newtype) | `crates/terminal/src/handle.rs:34-72` | 39 | Pure `Deref`/`DerefMut` to `Terminal`. `E1` was its only inherent method. Exported at `crates/terminal/src/lib.rs:44`; no crate outside `terminal` names it (`grep -rn '\bEngine\b'` outside `handle.rs` hits only `oneterm-completion::Engine`, `base64::Engine`, and `vt`'s test helper). Doc at `:36-39`: *"It is `US-0083`'s to delete, and this type goes with it."* Delete → `FairMutex<Terminal>` directly |
| E3 | `TerminalHandle::lock_unfair` | `crates/terminal/src/handle.rs:154-156` | 3 | Body is `self.engine.lock()` — **byte-identical to `lock()` at `:91-93`**. The comment at `:147` admits it: *"`parking_lot`'s fairness lives in `unlock`, so there is no unfair acquire to call and both of these are the plain ones."* Remaining call sites: `local-shell/src/event_loop.rs:438, :523` only — `ssh/src/task.rs` already stopped using it |
| E4 | `TerminalHandle::try_lock_unfair` | `crates/terminal/src/handle.rs:159-161` | 3 | **Byte-identical to `try_lock()` at `:96-98`.** One call site: `event_loop.rs:436` |
| E5 | `TerminalHandle::render_demand_raised` | `crates/terminal/src/handle.rs:136-138` | 3 | **Byte-identical to `take_render_demand()` at `:131-133`.** Only non-test caller: none. Only caller at all: `ssh/src/task_tests.rs:152` |
| E6 | `model::ResizePolicy` + its two `From` impls | `crates/terminal/src/model.rs:28-70` | 43 | Its own doc (`:32-36`): *"this enum survives only because the backends name `ResizePolicy::Default` through `impl_pty_terminal_session!`, and `US-0083`/`US-0084` replace that one token each … after which it is deleted."* Both backends still name it (`local-shell/src/session_terminal.rs:21,23`, `ssh/src/session_terminal.rs:24`) **because the macro's signature forces them to** — `migration.md` records this as `US-0085`'s unfinished follow-up. Dies with **B1** |
| E7 | `Demand` in the wrong crate | `crates/vt/src/render/demand.rs` | 62 | See §1.5 |

`take_render_demand` (`handle.rs:131`) is worth one line of its own: the verb is residue. It
*takes* nothing — it is a non-consuming `is_raised()`, deliberately so since the `US-0082`
rework. The doc explains it (`:127-130`) but the name still mis-states the contract to every
reader of `event_loop.rs:477` and `task.rs:87`.

### 2.2 Stale prose (comments citing deleted files)

| # | Location | What it cites | Status |
|---|---|---|---|
| R1/R2 | `crates/terminal/src/model_tests.rs:4`, `:89` | intra-doc link ``[`super::legacy_resize`]`` | The item does not exist anywhere. Dangling |
| R3 | `crates/terminal/src/backend/osc_router.rs:400` | `vendor/alacritty_terminal/src/term/mod.rs:1692-1705` | No `vendor/` directory exists |
| R4 | `crates/vt/src/graphics/sixel.rs:9` | "the behaviour `crates/terminal/src/sixel_tests.rs` pins" | That file was deleted at `US-0082` |
| R5 | `crates/vt/src/render/state.rs:86-89` | "the split exists because `Terminal` does not exist yet" | `Terminal` exists (`crates/vt/src/terminal/mod.rs`) |
| R6 | `crates/terminal/src/osc.rs:3-6` | "These are the OSCs **vte** does not dispatch … the OneTerm **alacritty fork** routes them through `Handler::report_osc`" | Describes a dependency that is gone; this is the only `vte::` mention left in `crates/` |
| R7 | `crates/vt/tests/parser_limits.rs:4-7` | "a wide `vte`-backed differential next to it" | Accurate history, dead tooling |

Legitimate and **not** to be touched: the Apache-2.0 attribution headers in `crates/pty/src/windows*.rs`
and `NOTICE`; the behaviour-provenance citations in `crates/vt` (`reflow/mod.rs:7`,
`selection/expand.rs:5`, `grid/row.rs:121`, …) — those are the spec; the frozen corpus; every
`LEGACY_AGENT_OSC` name (that is the live OSC 9;7 alias, unrelated to the rewrite); and
`display_offset`, which is a current public field on `TerminalInfo`, not a fork coordinate.

### 2.3 Doc rot

`python scripts/check-doc-paths.py` passes ("120 current paths in 10 documents") but is blind
here twice: its `DOCUMENTS` list excludes `docs/terminal-backend.md` entirely, and its regex
requires back-ticked `` `crates/…` `` tokens, so ASCII tree diagrams are invisible to it.

| # | Location | Problem |
|---|---|---|
| D1 | `docs/terminal-backend.md:863` | Tree entry `engine_shim.rs # the compatibility conversion (deleted at US-0085)` — a deleted file listed inside a section headed "File layout (**current**)" |
| D2 | `docs/terminal-backend.md:862` | `content.rs # TerminalContent: the RenderState it owns **+ the legacy shape**` — contradicted by `crates/terminal/src/content.rs:12` |
| D3 | `docs/architecture.md:12` | Labels `oneterm-terminal` "Terminal engine". It is the adapter |
| D4 | `docs/architecture.md:11-29` | The crate map has **no `oneterm-vt` row** — the first-party engine is absent from the file that calls itself the architecture source of truth. `oneterm-tools`, `oneterm-theme`, `oneterm-highlight`, `oneterm-actions` are missing too |
| D5 | `docs/agents/structure.md:183-189` | The `vt/` subtree lists 4 files; `crates/vt/src` has 54 across 9 subdirectories. Frozen at the start of the rewrite |
| D6 | `docs/agents/structure.md:~196` | `docs/refactor/ui-crate-restructure.md` — `docs/refactor/` does not exist |
| D7 | `docs/agents/structure.md:75` | Lists `url.rs / url_policy.rs`; only `url_policy.rs` exists |

Cheap follow-up worth more than the seven fixes: add `docs/terminal-backend.md` to
`check-doc-paths.py`'s `DOCUMENTS` and drop the back-tick requirement from its regex. That
script would then have caught D1 and D6 on the commit that created them.

### 2.4 Feature flags

No dead features. All three are enabled by someone:

| Feature | Declared | Enabled by | Verdict |
|---|---|---|---|
| `vt-paranoid` | `crates/vt/Cargo.toml:21` | `.github/workflows/ci.yml:129`, `:162`; `scripts/ci-local.{sh,ps1}`; read at `grid/screen.rs:1751`, `render/render_bench.rs:131` | Healthy |
| `test-support` | `crates/terminal/Cargo.toml:12` | dev-deps of `local-shell`, `ssh`, `state`, `terminal-view` | Healthy |
| `terminal-diagnostics` | `app:14`, `local-shell:12`, `ssh:12`, `terminal-view:14` | **Nothing automated.** `grep -rn terminal-diagnostics .github/ scripts/` → 0 hits, and no CI step passes `--all-features` | **Flag it** |

`terminal-diagnostics` guards ~24 `cfg` sites, of which **18 are bare `#[cfg(feature = …)]`
threaded through `crates/local-shell/src/event_loop.rs`'s 638-line read loop** (`:68, :71, :302-312,
:320, :329, :403, :405, :443, :486, :506, :526, :536, :556`), plus
`crates/terminal-view/src/render/diagnostics.rs:90-140` and three call sites. None of it is ever
type-checked or clippy'd by CI. (`crates/ssh/src/transport.rs`'s nine sites are
`#[cfg(any(test, feature = …))]`, so `cargo test` does compile those.)

Two honest choices: add one CI step
(`cargo clippy --workspace --all-targets --features oneterm-app/terminal-diagnostics -- -D warnings`),
or delete the feature and its ~200 lines. Leaving it as-is means ~200 lines rot silently until
someone needs them during an incident, which is the worst moment to discover they no longer
compile.

---

## 3. Duplication and over-engineering — ranked deletions

Ranked by lines removable × confidence. "Net" accounts for replacement code.

| # | What | Where | Lines | Cut it to | Risk |
|---:|---|---|---:|---|---|
| 1 | **`impl_pty_terminal_session!`** — a 286-line macro generating ~45 forwarding methods, expanded into two other crates, plus the two `session_terminal.rs` that invoke it | `crates/terminal/src/session.rs:443-728`; `local-shell/src/session_terminal.rs` (42); `ssh/src/session_terminal.rs` (55) | 383 | One concrete `PtySession` struct in `crates/terminal` holding `SharedTerminal`, `SharedState`, the transport, `SessionKind`, `oneterm_vt::ResizePolicy`, a close hook and the `marked_text`/`event_rx` cells. Each backend owns one and implements only `capabilities()`. The macro's own doc (`session.rs:446-453`) already specifies exactly this: *"the local shell and SSH sessions differ only in their `TerminalCapabilities`, their `SessionKind`, their `ResizePolicy` and how the channel is torn down"* | **Medium**. Covered by `backend_tests.rs` (1 280), `local-shell` (1 395 test lines), `ssh` (2 807) |
| 2 | **`model::ResizePolicy` + 2 `From` impls** | `crates/terminal/src/model.rs:28-70` | 43 | Delete; backends pass `oneterm_vt::ResizePolicy` directly, which `TerminalModel::new` has accepted since `US-0082`. **Blocked only by #1** | Low, after #1 |
| 3 | **`Engine` newtype + `exit()`** | `crates/terminal/src/handle.rs:34-72`, export at `lib.rs:44` | 40 | `FairMutex<Terminal>` | **None** — zero callers |
| 4 | **`lock_unfair` / `try_lock_unfair` / `render_demand_raised`** | `crates/terminal/src/handle.rs:131-161` + comments `:145-152` | 28 | `lock` / `try_lock` / `take_render_demand`; rewrite 4 call sites (`event_loop.rs:436,438,523`, `task_tests.rs:152`) | **None** — byte-identical bodies |
| 5 | **`terminal-diagnostics`, if not CI-gated** | `event_loop.rs` ×18, `render/diagnostics.rs:90-140`, `render/state.rs:165,191`, `render/element.rs:210`, 4 manifests | ~200 | Delete, or add one CI step. See §2.4 | Low either way |
| 6 | **`frame.rs`'s engine-vocabulary mirror** — `Color` ↔ `EngineColor` (two 25-arm matches + `ANSI_NAMED`/`DIM_NAMED` tables), `CellFlags` ↔ `Attrs` + `CellWidth` | `crates/terminal-view/src/render/frame.rs:78-290` | 213 | Use `oneterm_vt`'s own types; keep only the dim/bright flattening the theme lookup needs. Justified when the engine was a third-party fork (HLD idea 3); now it translates one OneTerm type into another | **Medium-high** churn: `row_plan.rs`, `plan_cache.rs`, `shapes.rs`, `cursor.rs`, `theme/palette.rs` all speak `Color`/`CellFlags`. **Not recommended** |
| 7 | **`TerminalContent`'s 11 forwarding accessors** | `crates/terminal/src/content.rs:199-293` | 95 | The view holds `RenderState` plus the three fields `TerminalContent` really owns (`cursor_shape`, `total_lines`, `graphics`). Would make `TerminalContent` a 40-line struct | Medium. **Optional** |
| 8 | **Duplicated byte-budget reservation** | `local-shell/src/event_loop.rs:120-140` vs `ssh/src/transport.rs:121-134` — the same `fetch_update(checked_add().filter(<= BUDGET))` + release idiom against two identically-valued 4 MiB constants | 35 | One `ByteBudget(AtomicUsize)` in `crates/terminal/src/backend/`, which both backends already depend on. **This is the only genuine cross-backend duplication left** | Low |
| 9 | **Corpus data in the wrong crate** | `crates/vt/tests/corpus/` (3.2 MB) + the sibling-path hack at `crates/tools/src/corpus.rs:88-92` | 5 + 3.2 MB | Move to `crates/tools/corpus/`, resolve from `CARGO_MANIFEST_DIR` directly | Very low |
| 10 | **`vt`'s dead `pub use` names** | `crates/vt/src/lib.rs:34-56` | 22 names | See §5 | None |
| 11 | **`Demand` in `crates/vt`** | `crates/vt/src/render/demand.rs` + `render/mod.rs:18` + `lib.rs:7,45` | 62 (moved) | Move to `crates/terminal/src/handle.rs` | Very low |
| 12 | **`Vec<&[u8]>` allocated per claimed OSC event** | `crates/terminal/src/backend/osc_router.rs:194` | 1 | The `EventBatch` design (`events-and-api.md`) exists specifically to stop allocating per-OSC — *"the fork allocates one `Vec` per OSC parameter … purely so the event can cross a channel"* — and the adapter re-introduces one heap allocation per claimed OSC to `collect()` the spans. `parse_osc`/`note_agent_osc`/`is_agent_osc` all take `&[&[u8]]`; a `[&[u8]; MAX_OSC_PARAMS]` on the stack, or passing the iterator, removes it | Low. Cosmetic on OSC 133 rates; ironic rather than costly |
| 13 | **`set_title` / `store_clipboard` extra copies** | `osc_router.rs:281-287`, `:289-302` | 4 | `set_title` clones the sanitized `String` to cache it and then `unwrap_or_default()`s the original; `store_clipboard` makes three owned copies of the payload (`to_owned` → `to_string` → `clone`). Neither is hot | Low |
| 14 | **Stale prose R1–R7** | §2.2 | 16 | Delete or re-point | None |
| 15 | **Doc rot D1–D7** | §2.3 | ~46 | Fix, and widen `check-doc-paths.py` | None |

### Things that look like over-engineering and are not — do not cut these

- **`SessionFactory`** (`crates/terminal/src/factory.rs:37-58`) — an interface with exactly one
  implementation (`AppSessionFactory`). It is the dependency inversion that makes R3 possible:
  without it every UI crate would name `oneterm-ssh`. Keep.
- **`PtyTransport`** (`backend/transport.rs`, 21 lines) — two implementations, and it is what
  lets `OscRouter`/`TerminalPump` be shared. Keep.
- **The four composed session traits** (`TerminalRender` / `TerminalInput` / `TerminalIme` /
  `TerminalLifecycle`) — they partition a 45-method surface and `test_support.rs`'s
  `FakeTerminalSession` implements them separately. Keep the split; it is item #1's *macro* that
  is the problem, not the traits.
- **`vt`'s module split** — 11 100 production lines across 9 subsystems, no harness code in
  `src/` (the `pub mod testing` and `pub mod strip` the LLD specified were never built, which is
  the right outcome). The only test-support type inside `src/` is
  `render/render_tests.rs`'s `pub(crate) struct Engine`, used by `render_bench.rs`. Clean.

---

## 4. Hot path

Read: `TerminalPump::advance` + `finish_batch*` (`backend/pump.rs`), `TerminalHandle`
(`handle.rs`), the local read loop (`local-shell/src/event_loop.rs:399-545`), the SSH loop
(`ssh/src/task.rs:67-90`), `TerminalContent::refill` (`content.rs:182-191`), `PlanCache::update`
(`terminal-view/src/render/plan_cache.rs:96-184`), `build_row_plan`, and the PTY ring.

**The big things are already right, and measured.** For the record, so nobody re-optimises them:

- No double copy of output bytes on either path. Local: `pty.reader().read(&mut buf[..])` then
  `pump.advance(engine, &buf[..unprocessed])` by reference. SSH: `data.as_ref()` straight into
  `process_chunk`. The 1 MiB `READ_BUFFER_SIZE` is allocated once (`event_loop.rs:270`).
- Steady-state per-frame allocation is zero: `TerminalContent::refill` reuses its `RenderState`,
  `PlanCache` clears-and-refills every vector, `EventBatch`'s arena grows to a high-water mark.
- The yield rule works and the numbers are recorded (`evidence/US-0083-verify.md:199-202`,
  `:344-345`): capping each read at `MAX_LOCKED_READ = 64 KiB` moved worst-case frame wait from
  **22.6 ms to 2.25 ms** while *raising* throughput **41.4 → 54.2 MiB/s**; with the yield
  disabled the worst frame wait is **805 ms**. `migration.md` records the same property as
  1 batch/0.9 ms against 1 016 batches/850 ms.
- Engine throughput after the rewrite (`evidence/US-0087-verify.md:144-148`): parser
  418–1 460 MiB/s, parse+grid **41.4–201.9 MiB/s**, render tier **1.2–22.6 µs/frame** at
  7 200 cells. Against a real ConPTY producer ceiling of ~1.2 MiB/s (`cmd.exe`) to ~30 MiB/s
  (DOOM-fire class), that is two orders of magnitude of headroom.

**Two real remaining costs, both in the view, both O(viewport) per frame:**

### H1 — the URL mask rescans the whole viewport on every changed frame

`crates/terminal-view/src/render/plan_cache.rs:144-152`:

```rust
let any_dirty = self.dirty.iter().any(|&d| d);
if any_dirty {
    url_masks_into(frame, &mut self.mask_cur, &mut self.wraps);   // full viewport
    stats.url_scans += 1;
    self.mask_prev.resize_with(rows, Vec::new);
    for (r, d) in self.dirty.iter_mut().enumerate() {
        if self.mask_cur[r] != self.mask_prev[r] { *d = true; }   // full viewport again
    }
}
```

`url_masks_into` (`crates/terminal-view/src/url/mask.rs:17-…`) walks **every column of every
row** and, at each unmarked column, tries every entry of `PREFIXES`. So a frame in which one
row changed does `rows × cols × |PREFIXES|` character comparisons, then a second
`rows × cols` byte comparison for the mask delta. At 45×160 that is ~7 200 cells scanned per
output frame regardless of change.

This is exactly the shape `DEC-0015` removed from the render copy — *"it scales with viewport
area rather than with change"* — surviving one layer up. The plan cache became incremental at
`US-0085`; the URL mask did not follow.

The fix is not "scan only dirty rows": a changed row can start or end a URL that continues into
an untouched row, which is why the pass is whole-viewport today (the comment at `:139-140` says
so). But the code **already tracks `self.wraps[r]` per row for precisely this reason**, so the
correct scope is *the dirty rows plus their wrap-connected neighbours*, not the viewport. The
counter to verify against exists: `stats.url_scans`, and `FrameStats` is already plumbed to the
diagnostics overlay.

Allocation is fine — `mask_cur`/`mask_prev` are `clear()`+`resize()`d, and `shift()` rotates
them with the scroll — so this is pure CPU, not heap.

### H2 — `last_content_row` is an unbounded viewport scan, once per frame, under the plain lock

`crates/terminal/src/content.rs:56-67` walks rows bottom-up until it finds a non-blank one, and
`is_blank_cell` (`:35-48`) does **three interner lookups per cell**: `resolve_style(style_id)`,
`text_char(&graphemes)`, `resolve_extras(extras_id)`.

Call chain, once per rendered frame: `terminal-view/src/terminal_view/render.rs:219`
(`session.read(cx).terminal_info()`, and again at `:236` on a scrollbar-drag frame) →
`session.rs:525-528` → `model.rs:167-176` → `crate::last_content_row(&term)`.

Two problems:

1. **The worst case is the common idle case.** A fresh shell has a prompt on row 0 and blank
   rows below it, so the loop scans the entire viewport before returning — 45 × 160 cells × 3
   hash lookups, every frame. Under sustained output the cursor sits near the bottom and it
   returns on the first row, which is why this has never been noticed.
2. **It runs under `TerminalHandle::lock()`, not `lock_for_render()`** (`model.rs:168`), so it
   does not raise the render demand and can queue behind a pump burst. `handle.rs:112-115`
   documents that choice on the explicit premise that `query_state` and `terminal_info` "are
   O(1) under the lock". `query_state` is; `terminal_info` is not, because of this scan.

The fix is three lines and the engine already has the primitive. `RowRef` exposes
`is_allocated()` (`crates/vt/src/grid/row.rs:307`) and `occ()` (`:292`), and `RowHeader.occ` is
documented as *"the reference's over-approximating hint: no column at or above `occ` has been
touched since the last reset"* (`row.rs:61-63`) — false positives allowed, never false
negatives, which is exactly what a skip needs:

```rust
let row = screen.row(top + u64::from(index));
if !row.is_allocated() || row.occ() == 0 { continue; }   // ← the whole fix
```

That turns the idle case from 7 200 cells × 3 lookups into 45 header loads. It also restores the
`handle.rs:112-115` premise, after which routing `terminal_info` through `lock_for_render` is no
longer needed.

### H3 — events materialised then dropped: real, and smaller than it looks

The brief asks whether `RowsScrolled` / `RowsTrimmed` / `GraphicReleased` still cost anything
now that `crates/terminal/src/backend/osc_router.rs:239-243` drops all three.

Measured by reading, not by running: each costs **one push into a `Vec<VtEvent>` that is reused
across batches, plus one match arm**. No allocation in the steady state (`EventBatch`'s vector
grows to a high-water mark). Frequency:

- `RowsScrolled` — `crates/vt/src/grid/screen.rs:818, :855, :892`. **Not emitted for the common
  case**: a whole-screen scroll reports `scrolled = None` (`screen.rs:808-822`, and the comment
  at `:807-810` explains why). Only *bounded-region* scrolls emit it — tmux, vim, htop.
- `RowsTrimmed` — emitted once per line once scrollback is full, so under a `cat bigfile` flood
  it is one push per output line.
- `GraphicReleased` — `graphics/placement.rs:172`, only when an image is evicted.

At ~10 000 lines/s that is ~10 000 pushes of a 24-byte enum into a warm `Vec` per second. It is
not zero and it is not worth a packet. **Do not optimise this**; if it is ever measured to
matter, the right shape is a claim bitmap on `Config` (the same mechanism `OscClaims` already
uses), not special-casing in the router.

### H4 — smaller notes, none actionable on their own

- `TerminalPump::pending` is a `Mutex<Vec<SessionEvent>>` (`pump.rs:69`) locked once per
  `advance`. Uncontended, ~20 ns. The comment explains it exists because the backends hold the
  pump by `&` at the lifecycle call sites. Fine; **B1** would let it become a plain field.
- `router.logging().process(bytes)` runs on every chunk before `feed` (`pump.rs:112`). It takes
  a mutex and early-returns when logging is off (`logging.rs:213-217`). **When logging is on it
  is a genuine second full parse of every byte** through a second `oneterm_vt::parser::Parser`
  — the feature is opt-in and the cost is documented, so this is a note, not a finding.
- `pump.advance` recomputes `history_len() + rows` and `lines_produced()` per chunk
  (`pump.rs:115-117`). O(1). Fine.
- No `Arc` is cloned per row anywhere in the render path. The per-frame `Arc` clones are
  `element.rs:372` (one per *graphic placement*) and `element.rs:70` (the element id). Both
  bounded and tiny.

---

## 5. Public API surface

Method: every `pub` item outside `#[cfg(test)]` blocks and `*_tests.rs` / `*_props.rs` /
`*_bench.rs` files, cross-referenced against every token appearing in any other crate. The
matching is deliberately generous (a hit in a *comment* counts as "used"), so **the dead lists
below are a lower bound**.

| Crate | `pub` items | zero external users | % |
|---|---:|---:|---:|
| `oneterm-vt` | 507 | **187** | 36.9 % |
| `oneterm-terminal` | 325 | **74** | 22.8 % |

`lib.rs` re-exports specifically: `vt` exports 74 names, **22 have no external user**;
`terminal` exports 100 names, **11 have none**.

For context on how small the real surface is: `crates/terminal` names **56 distinct**
`oneterm_vt` symbols and `crates/tools` names **25**. The engine's entire outward world is about
**66 distinct names out of 507 public items**.

### `oneterm-vt` — where the leverage is

All eleven root modules are `pub mod` (`crates/vt/src/lib.rs:19-32`). Only three need to be:

| Module | Why it must stay `pub` | Verdict |
|---|---|---|
| `intern` | `crates/terminal/src/content.rs:21` needs `intern::Hyperlink`, which has no root re-export | Keep `pub`, or re-export `Hyperlink` and close it |
| `grid` | `crates/terminal/src/handle.rs:24` needs `grid::{DEFAULT_SCROLLBACK, SCROLLBACK_MAX}`, neither re-exported | Same |
| `parser` | `crates/terminal/src/logging.rs:10` needs `parser::{Dispatch, OscParams, Params, Parser, StringTerm}` — the session logger drives its own parser. `parser` has zero root re-exports | Keep `pub` |
| `cell`, `event`, `graphics`, `reflow`, `render`, `selection`, `terminal`, `width` | Reached only as redundant deep paths (`content.rs:22`, `model.rs:14`, `model.rs:15`, `mouse_encode.rs:8` all import names that are **already at the vt root**) | **`pub(crate) mod`** after rewriting those four imports |

Concentrations of dead `pub`:

| File | Zero-user `pub` items | Note |
|---|---:|---|
| `crates/vt/src/grid/screen.rs` | **56** | 45 of them methods on `Screen`. `Screen` is named outside only by `crates/tools/src/corpus_replay.rs:29`. Making the methods `pub(crate)` cuts **11 % of vt's entire public surface in one file** |
| `crates/vt/src/grid/terminal_grid.rs` | 15 | `TerminalGrid` is re-exported from `lib.rs:39` and **nobody imports it** |
| `crates/vt/src/intern.rs` | 14 | incl. `StyleId`, `GraphemeId`, `GRAPHEME_MAX_LEN` — all re-exported, all unused |
| `crates/vt/src/terminal/mode.rs` | 12 | incl. `ModeState`, `CursorStyle` — re-exported, unused |
| `crates/vt/src/grid/row.rs` | 13 | incl. `RowRef` (re-exported, unused), `RowMut`, `RowFlags`, `RowHeader` |
| `crates/vt/src/grid/anchor.rs` | 5 | **whole module has no external consumer** |
| `crates/vt/src/render/sync.rs` | 5 | **whole module has no external consumer**; `SyncState` is re-exported and unused |

The 22 dead names in `crates/vt/src/lib.rs`'s `pub use` block are a pure deletion needing no
visibility surgery: `StrSpan`, `ByteSpan`, `ParamSpans`, `FeedStats`, `MAX_DIMENSION`,
`Placement`, `RowRef`, `TerminalGrid`, `GRAPHEME_MAX_LEN`, `StyleId`, `GraphemeId`,
`ResizeOutcome`, `Watermark`, `EngineView`, `SyncState`, `SEMANTIC_ESCAPE_CHARS`,
`Invalidation`, `ColorOverrides`, `ThemeColors`, `ModeState`, `CursorStyle`, `cluster_width`.

Caveat the owner should weigh: several `<-external` marks exist **only** because
`crates/tools` names the type. `tools` is a diagnostics crate that ships nothing. If the corpus
replayer were given a narrower entry point (it drives `Screen` directly today), the `grid`,
`cell`, `intern` and `terminal` deep surfaces would collapse much further.

### `oneterm-terminal` — what to close

| Item | Location | Action |
|---|---|---|
| `pub mod color_classification`, `pub mod key_encode`, `pub mod osc_color` | `lib.rs:14, 18, 24` | `pub(crate) mod` — nothing outside names the module path, only the re-exported items |
| `paste`'s 5 `pub` items | `paste.rs:14, 18, 36, 45, 64` | The module is **already** `pub(crate) mod` (`lib.rs:26`), so these are decorative. `pub(crate)` |
| `osc.rs`'s 6 free items (`parse_osc`, `parse_cwd_url`, `decode_osc52`, `agent_support_reply`, `Osc133Kind`, `OscPayload`) | `osc.rs:25, 56, 82, 214, 232, 284` | `lib.rs` deliberately does not re-export them and nothing reaches `oneterm_terminal::osc::`. `pub(crate)` |
| 13 `SharedSessionState` methods with no external caller | `backend/state.rs:103-227` | `pub(crate)` |
| `DefaultColors`, `EventQueueDiagnostics`, `TerminalLogError` | re-exported, unused | Drop from the `pub use` block |
| `Engine` | `lib.rs:44` | Delete with item #3 of §3 |

Net effect if all of §5 is applied: `oneterm-vt`'s public surface drops from 507 to roughly
**320**, `oneterm-terminal`'s from 325 to roughly **250** — and, more usefully, `cargo doc` for
the engine starts describing an API a reader can hold in their head.

---

## (d) What I could not determine

1. **The actual cost of H1 and H2.** No benchmark measures the view. `crates/tools`'s five
   tiers stop at `render_update`; there is no criterion bench anywhere in the workspace
   (`grep -rn criterion crates/` → nothing), and `FrameStats`/`stats.url_scans` are counters,
   not timers. My claims are complexity arguments from reading, not measurements. Both fixes
   are small enough that "measure after" is cheaper than "measure first", but neither should be
   sold to the owner as a known win.
2. **Whether B1 (replacing the macro) is a net LOC win or a wash.** I estimate the concrete
   `PtySession` at ~230 lines against 383 deleted, but that depends on how much of
   `marked_text` / `event_rx` / `owner_join` ownership can move out of the backend structs. It
   could land anywhere between −80 and −200.
3. **Real compile-time numbers.** I did not run `cargo build --timings`, so the build-cycle
   claims in the Option table are structural (dependency-graph reasoning), not stopwatch
   figures. The direction is certain; the magnitude is not.
4. **Whether the `oneterm/` corpus directory (1 recording, Sixel) has a second consumer.** It is
   read only by `crates/tools`, like `alacritty-ref/`, so B3 covers it — but `US-0072`'s
   blessing rules (`R-58`, "nothing blesses") may make the owner prefer it stay adjacent to the
   engine for provenance reasons. That is a judgement call, not a fact I can settle.
5. **Whether `crates/ssh`'s read loop has an H1/H2 equivalent under a real host.** `IN-0029.md`
   §"What remains" records that step 8 of the acceptance walk (SSH) was never run: neither
   configured host could open a shell. Everything I assert about the SSH path is from the code
   and from the loopback fixture, not from a real session.
6. **How much of `crates/terminal-view` (13 491 production lines) is itself consolidatable.**
   The brief scoped it as a consumer, and I read only its render path. `shapes.rs` (1 341) and
   `row_plan.rs` (1 038) were not audited for over-engineering.
