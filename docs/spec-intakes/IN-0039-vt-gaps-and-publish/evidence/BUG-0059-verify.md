# Independent verification: BUG-0059, unnameable public return types and the gate

Verifier: independent agent, worktree `.claude/worktrees/agent-aeaaec053c225af68`, detached at
`6d8a442c` (branch `fix/vt-nameable-types`, base `main` `c5ddad59`). Not committed, not pushed.
Diffstat against `main`: 16 files, +309 / -40 -- matches the packet.

## Verdict

**PASS WITH NOTES.** Every load-bearing claim reproduces: the four re-exports work from a real
external crate, the gate fails on `main` and passes on the branch, and all three ways of breaking
the gate (a new private type in a public signature, a ledger entry removed while still firing, a
ledger entry that does not fire) make it fail. The notes are record accuracy and one missing
hand-off artefact; none of them is a defect in the shipped code.

## Findings

| # | Severity | Finding |
| --- | --- | --- |
| F1 | medium | **No harness story row and no snippet for one.** `harness.db` (repo root, read via a copy) has **zero** `story` rows with `intake_id = 44`, and `BUG-0059-unnameable-public-types.md` has no `## Harness Row` section. The comparable IN-0038 packet has one (`docs/spec-intakes/IN-0038-embeddable-vt-core/US-0104-pty-in-core.md:683`) and its row **was** applied (`story` rows for `US-0097..US-0104`, `intake_id` 43). `docs/HARNESS.md:35` makes `harness.db` authoritative for status and proof state, so the packet's ticked Status/Proof blocks currently have no counterpart the owner can apply. Expected row: `id='BUG-0059'`, `intake_id=44`, `status='implemented'`, `unit_proof=1`, `integration_proof=1`, `e2e_proof=0`, `platform_proof=1`, `last_verified_result='pass'`. Note the real schema is `story(id, title, created_at, risk_lane, contract_doc, packet_doc, status, unit_proof, integration_proof, e2e_proof, platform_proof, evidence, verify_command, last_verified_at, last_verified_result, notes, intake_id)` -- the US-0104 snippet's column names (`key`, `kind`, `risk`, `proof_unit`, ...) do **not** match it and must not be copied. |
| F2 | medium | **The follow-up is not named anywhere a tracker can see it.** `BUG-0059-unnameable-public-types.md:286` says only "**A follow-up packet under `IN-0039` should empty it.**" There is no packet id, no backlog row, and `IN-0039.md` is unchanged: its packet table at line 203 still lists only the original four packets and the pre-overrun budget. The `KNOWN_UNNAMEABLE` ledger's shrink-only promise therefore has no owner. |
| F3 | medium | **A factual overstatement in the Evidence table.** `BUG-0059-unnameable-public-types.md:275` says of `StrSpan`/`ByteSpan`/`ParamSpans` "so those three methods cannot be called from outside at all". They can. Verified by compiling an external crate against the branch: `for ev in batch.iter() { if let VtEvent::Title(span) = ev { batch.str(*span) } }` compiles -- the span binds by inference and `EventBatch::str`/`bytes`/`params` accept it. What is impossible is *writing the type down* (storing a span in a struct field, returning one from a helper), which is the same, narrower defect the four types had. Worth correcting so the follow-up is not mis-prioritised. |
| F4 | medium | **The worst of the seven is `ModeState`, and that deserves priority in the follow-up.** `Mode::inert_state(self) -> Option<ModeState>` is public (`crates/vt/src/terminal/mode.rs:225`), and `ModeState` (`crates/vt/src/terminal/mode.rs:282`) is an **enum** with five variants. An external crate cannot name any variant, so the return value cannot be matched, compared or converted -- only `{:?}`-printed. Verified by compiling `format!("{:?}", mode.inert_state())` (works) beside a commented `matches!(.., Some(oneterm_vt::ModeState::Set))` (no path exists). This is a usability defect of the same class the packet just fixed, not a cosmetic one. |
| F5 | low | **Budget arithmetic is off by 5.** `BUG-0059-unnameable-public-types.md:248` states `crates/vt` is "+62 / -13". `git diff --numstat main...HEAD` gives, excluding the two generated surface files, **+57 / -13** (CHANGELOG +13, guide +7/-4, `src` +37/-9). The table row "CHANGELOG and guide chapter 12 | +24 / -4" should read +20 / -4. `scripts/vt-public-api.py` **+98 / -0** is exact. The overrun itself is disclosed, which is what the acceptance criterion asked for. |
| F6 | low | **"the crate has ten unnameable types, not four"** (`BUG-0059-unnameable-public-types.md:270`) is eleven: the four fixed plus the seven ledgered. Running the gate with `KNOWN_UNNAMEABLE` emptied prints 10 *findings* over 7 distinct types; on `main` it printed 5 findings over 4 distinct types. |
| F7 | low | **A gate blind spot not in its three stated limits.** `DEFINITION` (`scripts/vt-public-api.py:100`) matches only `pub` and `pub(crate)` definitions, so a `pub(super)` or `pub(in path)` type never enters the `defined` map and can never be reported. Such types exist in the crate (`crates/vt/src/parser/state.rs:15`, `crates/vt/src/pty/windows/child.rs:100`). Also `NAMED` requires three or more characters, so a one- or two-character type name would be skipped -- none exists today. One line in the docstring closes both. |
| F8 | low | **`Placement` cannot be constructed by an embedder at all.** It is `#[non_exhaustive]` (`crates/vt/src/graphics/mod.rs:85`) and derives no `Default`, so an external crate has no struct literal, no functional update (E0639) and no constructor. `ResizeOutcome` is also `#[non_exhaustive]` but derives `Default` and has `pub` fields, so default-then-assign works (verified to compile); `CursorStyle` is unmarked and takes a struct literal (verified); `SyncState` has `pub fn new`. The LLD's reason for `Placement` ("returned, never built outside the crate") is defensible, but an embedder writing its own painter test cannot synthesise one. Deriving `Default` on `Placement` would cost one line and remove the lockout. |
| F9 | low | **The stale-ledger message is imprecise.** `check_nameable` prints "`... are nameable now`" for any ledger name that stopped firing -- including the case where the method carrying it was simply deleted -- and uses a plural verb for one name ("TitleState are nameable now"). Cosmetic. |
| F10 | low | **`--check` is order-dependent on the preceding doc build.** Run after only `cargo doc -p oneterm-vt --no-deps`, `--check` fails with `- variant Regex` (the `regex` feature is not default). It passes after `cargo doc --all-features`, which is the last doc step in both CI entry points, so CI is correct -- but a human running the gate by hand will hit this. Pre-existing, not introduced here. |

## What was verified, and how

All commands run from the verifier worktree with `CARGO_BUILD_JOBS=3`.

**The gate fails on `main`.** `git checkout --detach 04246a79` (the gate commit; its parent is
`main` `c5ddad59`, so the crate tree is `main`'s), `RUSTDOCFLAGS=-D warnings cargo doc -p oneterm-vt
--no-deps --all-features`, then `python scripts/vt-public-api.py --check-nameable --no-doc` ->
**exit 1**, five findings naming `CursorStyle` (twice, on `Config` and `Terminal`), `Placement`,
`ResizeOutcome` and `SyncState`, each with its private module. Byte-identical to the output quoted
in the packet.

**And passes on the branch.** Same doc build at `6d8a442c` -> exit 0,
`every type in a public signature is nameable, but the 7 in KNOWN_UNNAMEABLE`.

**Break test A -- a new private type in a public signature.** Added
`pub fn probe_title(&self) -> &crate::terminal::TitleState` to `Terminal`, rebuilt docs, ran the
gate -> **exit 1**, `oneterm_vt::Terminal: 'TitleState' is not nameable (defined in
'terminal::mode')`. Reverted.

**Break test B -- a ledger name that still fires.** Removed `"StrSpan"` from `KNOWN_UNNAMEABLE` ->
**exit 1**, two findings (`oneterm_vt::VtEvent`, `oneterm_vt::EventBatch`). Reverted.

**Break test C -- a ledger name that does not fire (shrink-only).** Added `"TitleState"` to
`KNOWN_UNNAMEABLE` -> **exit 1**, `KNOWN_UNNAMEABLE is stale: TitleState are nameable now`.
Reverted.

**External probe (scratchpad, path dependency, deleted after the run).** A `nameprobe` crate
outside the workspace stored all four types in struct fields, matched `CursorStyle::shape` across
every `CursorShape` variant, returned a `ResizeOutcome` from its own function, and called
`resize` / `cursor_style` / `sync` / `placements`. `cargo check` clean against the branch. With
`crates/vt/src/{lib.rs,snapshot/mod.rs,terminal/mod.rs}` reverted to `main`:
`error[E0432]: unresolved imports 'oneterm_vt::CursorStyle', 'oneterm_vt::Placement',
'oneterm_vt::ResizeOutcome', 'oneterm_vt::SyncState'` -- the same four names, exactly as claimed.
The same crate carried the span-usability and construction probes behind F3, F4 and F8.

**Gates.** `python scripts/vt-public-api.py --check --no-doc` exit 0 (after the `--all-features`
doc build; see F10), `--check-nameable --no-doc` exit 0, `--diff-platforms` exit 0 with
`the delta is 6 lines, all inside 'oneterm_vt::pty'`. Both surface files gain exactly the four
items (+18 / -0 each), nothing removed or renamed. `RUSTDOCFLAGS=-D warnings cargo doc -p
oneterm-vt --no-deps` and the same `--all-features` both exit 0 with `missing_docs` on.

**`pwsh scripts/ci-local.ps1`** (23 steps, no `-Full`): **`ci-local: all checks passed.`** That
covers `cargo fmt --check`, both `clippy --workspace --all-targets -D warnings` runs,
`cargo test --workspace`, `cargo test -p oneterm-vt` under `vt-paranoid`, `regex` and
`--no-default-features`, both `-D warnings` doc builds, all three `vt-public-api.py` modes, and
`verify-dependency-graph` / `check-doc-paths` / `check-english` (885 files) /
`completion-catalog` / `third-party-notices`. Two states ci-local does **not** cover were run
separately and pass: `cargo test -p oneterm-vt --all-features` (exit 0) and
`cargo test -p oneterm-vt --doc` -- **36 passed**, matching the packet's number and including the
crate-root nameability doctest. `cargo deny`
was **not** run: `-Full` is the only path to it and it cannot reach the RustSec advisory database
through this machine's proxy. That is reproducible on `main` and unrelated to this branch, so it
is stated rather than treated as a failure.

**Records.** Status block ticks only `Planned` + `Implemented` -- correct, nothing over-ticked.
Proof block unit 1 / integration 1 / e2e 0 / platform 1 / verify 1 is honest (nothing user-visible
changed). Guide chapter 12's "Ten public types are marked / Seven enums / Three structs" matches
`grep -c non_exhaustive` over the crate exactly (7 enums: `KeySpec`, `NamedKey`, `OscRoute`,
`Progress`, `SearchPattern`, `ShellMark`, `VtEvent`; 3 structs: `SearchOptions`, `ResizeOutcome`,
`Placement`). The CHANGELOG entry is under `[Unreleased] / ### Added`, names all four types, states
they were previously returned but unnameable, and names the gate. `crates/vt/README.md` correctly
unchanged.

## What could not be verified

- **Unix.** Everything here ran on Windows. `public-api.unix.txt` was mirrored by hand and its
  first real check is the Linux `vt-package` CI job. The packet says so.
- **`.github/workflows/ci.yml`.** The new `--check-nameable` line was read, not executed; no CI run
  exists for this branch.
- **`cargo deny`.** Network-blocked, as above.
- **The harness database.** Read from a copy only. Nothing was written to it, as instructed.
