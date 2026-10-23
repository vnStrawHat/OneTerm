# US-0205 verify — advisor code review

Date: 2026-10-23
Reviewer: advisor (read-only: no cargo, no GUI, no test run; the shared `target/` was busy)
Range: `git diff 2dc3e1f0 bd11fa32` on branch `worktree-agent-a3006be724356a35a`
Owner rulings applied: IN-0017 home, pure bump, Windows-only walk, keep "0.7.3 kit" history wording, no upstream request about the zoom row.

## Verdict: PASS with defects

The manifest, lock, notices and the three tag citations are correct and complete; no OneTerm source
changes and none are needed. The defects are records only (the packet inherits a wrong claim from the
research note) plus one stale version citation outside the packet's listed scope.

## What was checked and how

| Attack item | Method | Result |
|---|---|---|
| (a) lock diff | `git diff` of `Cargo.lock`; sha256 of the four `.crate` files in `.cargo-home/registry/cache` against the lock checksums (python, no cargo) | Exactly 8 lines moved (version + checksum of `gpui-base`, `gpui-component`, `gpui-component-macros`, `gpui-kit-assets`); all four checksums match the downloaded crates; the dependency lists of those four entries are byte-identical between 0.7.3 and 0.8.0 |
| (a) feature sets | `diff` of `crates/{base,component,assets}/Cargo.toml` between the `v0.7.3` checkout (main) and the `v0.8.0` clone (worktree); the registry manifests `gpui-base-0.7.3/Cargo.toml:89-93` and `gpui-component-0.7.3/Cargo.toml:308-312` | No `default` feature in either tag; `gpui-fast` is `optional = true` in **both** 0.7.3 and 0.8.0 (base, component, assets). The only manifest change is the new optional `syntect` feature. OneTerm never had `gpui-fast` on, so no fast path is lost |
| (b) docs | `python scripts/check-english.py`, `python scripts/check-doc-paths.py`; grep for `0.7` in the cited docs and `crates/theme/src/theme.rs` | Both checks pass. `dependencies.md` intro, table, section 2 snippet, section 5 tag; `structure.md:230`; `terminal-backend.md:1257` all say 0.8 / `v0.8.0`. `gui-layout.md:40` rewording is a generic reference, not the Q4 history wording (`:59`, `:73` kept). The `theme.rs:629` pin comment is history (the test passed under 0.8.0). `.claude/agents/coder.md:8` still says "gpui-component 0.7" (defect 2) |
| (c) notices | `scripts/third-party-notices.py:181-197` (`reachable_from` prunes dev edges only); `THIRD-PARTY-NOTICES.md:964` count | Count unchanged at 866; only the four version cells moved. `gpui-fast` / `gpui-fast-macros` were listed at 0.7.3 too (optional-off then as well), so the over-report is pre-existing, not introduced here. Apache-2.0, already allow-listed: no licence-compliance problem. Policy: `dependencies.md:89` calls the file "the default graph"; that holds for workspace-member features but `cargo metadata`'s resolve keeps optional deps of registry packages, which the script cannot see (`dep_kinds` carries `kind` and `target` only). Not a defect of this packet; see note 4 |
| (d) `PopupMenu::rebuild` | `diff` of `component/src/menu/popup_menu.rs` and `component/src/dock/tab_panel.rs` between tags | `rebuild` (`popup_menu.rs:755-766` at v0.8.0) is unchanged; the only menu change is that item click handlers now receive the real `ClickEvent` instead of a default one. `tab_panel.rs` still appends the zoom row unconditionally; its diff is `DockToggleEvent` wiring and tests. The `tab_title.rs:983-1010` strip keeps its semantics |
| (e) release notes | `reference/gpui-kit/release-notes.md` at `v0.8.0` (worktree clone), full breaking-changes section plus the component list | See note 3: the kit `Tree` (Session panel) changed in two ways the research did not name |

## Defects

1. **Low — packet records carry an unmeetable acceptance line.** `US-0205-bump-gpui-kit-0-8-0.md`
   Scope ("`gpui-fast` and `gpui-fast-macros` leave the lock") and Acceptance ("`gpui-fast` and
   `gpui-fast-macros` leave it", left unticked) restate research section 1.7 / 2.1 / 5, which said
   `gpui-fast` was mandatory at 0.7.3. It was optional at both tags (registry manifests cited above);
   the release note's "optional again" is relative to the kit's `main` between tags, not to 0.7.3.
   The coder's evidence paragraph already says this correctly. Fix shape: reword both lines to "only
   the four kit packages move; `gpui-fast` / `gpui-fast-macros` stay as optional, inactive entries,
   as at 0.7.3" and tick them; tick the `cargo tree -i` line too (the evidence records the run).
   The research note is corrected by this file (section "Research corrections").
2. **Low — stale version citation.** `.claude/agents/coder.md:8` says "gpui-component 0.7 at
   `reference/gpui-kit`". Fix shape: "0.8". Coordinator-owned file; one word.

## Notes (not defects)

3. **Session tree behaviour the research under-weighted.** `gpui_base::tree` at v0.8.0 moves entry
   select / toggle from mouse-down to click (`base/src/tree.rs:471-475`, PR 3567: "a press that is
   dragged away leaves the entry alone"; content click handlers run first and see the previous
   selection) and `gpui_component::tree` wraps the list in `ScrollBounce` (`component/src/tree.rs:125-129`),
   so the wheel over the Session tree is now owned by the wrapper (bounce itself stays off on
   Windows). OneTerm's rows (`crates/session-ui/src/tree_render.rs:208-230`, `:289-326`) keep their
   own mouse-down handlers for highlight and right / middle selection, and the double-click path
   reads the store by id, not the selection, so no code change is needed. Add to the Windows walk:
   wheel-scroll a long Session tree; left-click a group toggles it; click a session then Enter /
   Delete act on that session; press-and-drag-away leaves selection and expansion alone.
4. **Notices over-report** (coder's weakest point 1): acceptable, pre-existing, and outside a pure
   bump. If the owner wants the file to match the linked graph, that is its own packet (filter the
   `cargo metadata` walk against `cargo tree -e normal,build --prefix none`), not this one.
5. **Nested-context-menu rule** (coder's weakest point 3): it is a change to the kit's
   `TestWindowExt` test harness (`right_click` opens only the innermost menu); OneTerm implements no
   `TestWindowExt`, so the strip-menu tests are not exposed and no further proof is owed.
6. **Textarea wheel capture** (PR 3484): a wheel over a multi-line text input that can scroll no longer
   reaches the page around it. Worth one row in the walk on a dialog that has a textarea (startup
   command, `IN-0051`).
7. No pinned test added: no code defect was found, and the review ran no cargo.

## Research corrections

`research-gpui-kit-0-8-0.md` section 1.7, 2.1 (`gpui-fast` row), 2.2 #2 and section 5 (notices row):
`gpui-fast` 0.1.10 is an optional, inactive dependency at **both** 0.7.3 and 0.8.0; it does not leave
`Cargo.lock` or `THIRD-PARTY-NOTICES.md`, and the notices regeneration changes only the four kit
version cells. Everything else in the note stands.

## Still to check by hand (owner / ui-verifier, Windows)

- The packet's walk list, plus notes 3 and 6 above.
- Refresh the shared `reference/gpui-kit` to `v0.8.0` after merge (the worktree clone is gitignored).
