# US-0101 independent verification -- render becomes snapshot

Verifier: independent agent worktree `.claude/worktrees/agent-aadc14f995d7b85fc`
Branch under test: `refactor/vt-snapshot` @ `70401eb3` (base `main` @ `98a72148`)
Date: 2026-09-15

## Verdict: PASS-WITH-NOTES

It is only a rename. The new surface is exactly the one `low-level-design/api-surface.md`
specifies. Four record/doc notes, none of them a code defect.

## Findings

1. **PASS -- it is only a rename.** Own script (see Scripts) applies the LLD name map to each
   changed file's `main` text and compares token multisets with the branch text.
   36 of 36 changed `.rs` files: 33 identical, 1 punctuation-only
   (`crates/vt/tests/engine_without_pty.rs`, one rustfmt trailing comma), 2 residues that are prose
   only -- `crates/terminal-view/src/render/element_tests.rs:219,465` (two comment lines) and
   `crates/vt/src/snapshot/mod.rs:1` ("render-state hand-off" -> "snapshot hand-off"). No
   behaviour-bearing token change anywhere.
2. **PASS -- the name table is exactly the LLD's.** `public-api.windows.txt` and
   `public-api.unix.txt` are each an exact permutation of `main`'s under the map (818 / 822 lines,
   no item added or removed); no `Render*` or `render_update` survives in either.
   `--diff-platforms` still reports exactly six `oneterm_vt::pty` lines.
   `Render[A-Z]` over `crates/vt crates/terminal crates/tools`: 0 lines of Rust.
   `crates/terminal-view/src/render/` keeps its own `RenderState` / `RenderInputs` (11 sites).
   `render_demand_raised` untouched. vt-bench tier 3 is still named `render`
   (`crates/tools/src/bench.rs:10`). `oneterm_terminal::ContentCell` exists, `SnapshotCell` in
   `crates/terminal` is only the engine re-export.
3. **PASS -- external embedder.** An out-of-workspace crate with
   `oneterm-vt = { path = <worktree>/crates/vt }` naming `SnapshotState`, `SnapshotUpdate` and
   `Terminal::snapshot_update` compiles; the same crate naming `RenderState` / `RenderUpdate` fails
   with `E0432 ... no RenderState in the root`. No deprecated alias, as `api-surface.md` line 64
   requires ("No deprecation shims anywhere"). Probe deleted afterwards.
4. **MEDIUM -- no harness record.** `US-0101` carries no `story` insert snippet, and
   `harness.db` has no `US-0101` row, while every sibling in this intake (`US-0097`..`US-0100`,
   `BUG-0058`) carries one. Nothing to schema-check; `intake_id=43` is correct for `IN-0038`.
   File: `docs/spec-intakes/IN-0038-embeddable-vt-core/US-0101-render-becomes-snapshot.md` (absent).
5. **LOW -- two current LLDs of this intake still name the old surface.**
   `low-level-design/osc-extension.md:91` names `RenderState` as a live type;
   `low-level-design/packaging.md:206` still writes `render_update()` and `:211` says the rename
   "becomes `snapshot_update` at `US-0101`, and the example changes with it" in the future tense
   although `examples/headless.rs` has already changed. Both are current-mechanics docs, so the
   packet's own rule ("changed because they describe current engine code") reaches them. Every
   other remaining `Render*` hit under `docs/` is a historical packet, evidence or research file,
   or `terminal-view`'s own types -- the "deliberately not changed" list is defensible.
6. **LOW -- one evidence count is wrong.** The packet says the literal `Render[A-Z]` grep "returns
   15" (7 CHANGELOG + 8 recording lines). Over `crates/vt crates/terminal crates/tools` it is 18:
   7 in `crates/vt/CHANGELOG.md` and 11 in the two frozen alacritty recordings (5 + 6). The
   substantive claim -- 0 lines of Rust -- holds.
   File: `US-0101-render-becomes-snapshot.md:192-196`.
7. **LOW, pre-existing -- the ticked doc command does not pass as written.** The acceptance box
   says "`cargo doc -p oneterm-vt --no-deps` warning-free"; that literal command fails under
   `RUSTDOCFLAGS=-D warnings` on `unresolved link to SearchPattern::Regex`
   (`crates/vt/src/search/mod.rs:49`, behind the off-by-default `regex` feature). Identical on
   `main` @ `98a72148`, so it is `US-0100` residue and not a `US-0101` regression; the packet's
   Evidence line and `ci-local` both use `--all-features`, which is clean. Not blocking here.
8. **LOW, cosmetic -- the renamed method kept a `render` parameter.**
   `crates/vt/src/terminal/mod.rs:284`: `pub fn snapshot_update(&mut self, render: &mut
   SnapshotState, now: Instant)`. Parameter names appear in rustdoc signatures, so the public
   surface still says "render" once. Also `crates/vt/src/graphics/graphics_tests.rs:139` has a
   private test helper `fn render()`.

Ticks checked against reality: no acceptance box is over-ticked apart from finding 7. The `+-5`
net-delta box is correctly left unticked with the +6 reason recorded, and `E2E proof` is correctly
unticked. `git diff --shortstat 98a72148 HEAD -- '*.rs'` reproduces 36 files, 324+/318-.
The CHANGELOG `Changed` entry names all seven types, the method, the unchanged types and the
absence of a shim.

## Commands

    git reset --hard refactor/vt-snapshot                 # 70401eb3
    python <scratch>/rename_proof.py ; python <scratch>/rename_proof2.py
    python <scratch>/api_proof.py
    python scripts/vt-public-api.py --check | --diff-platforms      # both pass, 6 pty lines
    cargo test -p oneterm-vt                               # 479 + 84 pass, 0 fail
    cargo test -p oneterm-vt --no-default-features          # pass
    cargo test -p oneterm-vt --features regex               # pass
    cargo test -p oneterm-vt --features vt-paranoid         # pass
    cargo test -p oneterm-terminal ; -p oneterm-terminal-view   # pass
    cargo run -p oneterm-tools --bin vt-corpus -- check     # 45 recordings, 45 passed, 0 failed
    RUSTDOCFLAGS=-D warnings cargo doc -p oneterm-vt --no-deps --all-features   # clean
    python scripts/check-english.py ; scripts/check-doc-paths.py   # pass, pass
    pwsh scripts/ci-local.ps1 -Full       # 23 steps, "ci-local: all checks passed."

## Not verified

- **E2E.** No Windows launch; the owner runs their session inside OneTerm. Same gap the packet
  declares.
- **Unix.** Only Windows was exercised; `public-api.unix.txt` was checked by the permutation proof
  and `--diff-platforms`, not by a Unix build.
- Whether the `SearchPattern::Regex` doc link (finding 7) is tracked anywhere as `US-0100` debt.
