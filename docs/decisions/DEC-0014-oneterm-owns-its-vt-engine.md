# DEC-0014 OneTerm owns its VT engine

Date: 2026-09-12

## Status

accepted

## Context

OneTerm's terminal engine is a vendored fork pair: `vendor/alacritty_terminal`
(`zed-industries/alacritty` @ `fcf32fe`, Apache-2.0) and `vendor/vte` (0.15.0, Apache-2.0 OR
MIT), consumed through the `[patch]` block at `Cargo.toml:240-244` and proven pristine-plus-
patches by `bash vendor/refresh.sh --check` in CI (`.github/workflows/ci.yml:65-68`). The
OneTerm delta is **823 patch lines across five patches**, of which the Sixel patch alone is
577 (`research/prior-art.md` § 1.1). Every OneTerm-specific terminal capability shipped so
far has cost a patch in that tree:

- OSC 7 / 9 / 9;4 / 9;7 / 133 need `vte/0001` (`Handler::report_osc`), because upstream `vte`
  dispatches thirteen OSC numbers and drops the rest (`research/engine-semantics.md` § 1.8).
- Sixel needs `vte/0002` (DCS hook/put/unhook) **and** `alacritty_terminal/0003` (the whole
  `term/graphics.rs` module, `Cell::graphic`, `Term::take_graphics`, the DA1 answer)
  (`research/api-surface.md` § 5.2, § 5.4).
- The gutter's clear epoch needs `alacritty_terminal/0002` (`Event::ClearScreen`).

The list of capabilities that would each cost another patch is long and already scoped:
Kitty graphics, kitty keyboard, mode 2027 grapheme clustering, mode 2048 in-band resize,
mode 2031 colour-scheme notification, XTVERSION, DECSLRM, blink, overline, SGR-pixel mouse
(`research/prior-art.md` § 6.3). Upstream `alacritty_terminal` is in maintenance mode and has
rejected graphics by policy — the Sixel PR has been open since 2021 — and the `zed-industries`
fork we pin is five Unix-only commits (`research/prior-art.md` § 0.4).

Three facts bound the decision:

1. **Throughput is not the argument.** The current engine measures 126-253 MB/s full parse +
   grid against a real ConPTY producer rate of ~1.2 MiB/s (a `cmd.exe` loop) to ~30 MiB/s
   (DOOM-fire class) on the owner's machine, and the per-frame viewport snapshot costs 29.9 us,
   0.18 % of a 60 Hz budget (`research/perf-baseline.md` § 4). Where headroom is thin the lever
   is the grid-mutation side, which costs 3-8x the parser.
2. **The blast radius is already surveyed and small.** 193 references in 40 files, concentrated
   in `crates/terminal` (79); the UI sees the engine only through `TerminalContent`
   (`research/api-surface.md` § 2). `crates/terminal-view/src/render/frame.rs:3-7` states that
   it is the single file under `render/` naming an engine type.
3. **Three defects ship today**, all reachable from any SSH session: `vte`'s `osc_raw` is an
   unbounded `Vec<u8>` under `std`, `CellExtra.zerowidth` is an unbounded `Vec<char>` per cell
   in our pinned revision, and `Row::new(0)` writes through a dangling pointer in release
   builds (`research/prior-art.md` § 0.5, § 10 risk 6).

## Decision

**OneTerm writes and owns its VT engine as Apache-2.0 workspace crates.** No vendored fork, no
patch series, no FFI. The engine is split in two:

- `crates/vt` (`oneterm-vt`) — parser, dispatch, cell, grid, scrollback, reflow, damage,
  render state, events, graphics. A leaf crate: no OneTerm dependency, no gpui.
- `crates/pty` (`oneterm-pty`) — pseudo-console transport only, extracted **before** the engine
  work so it survives independently. `crates/ssh` needs no PTY; `crates/tools` needs PTY without
  a grid. On Windows it keeps today's resolution order — the bundled `conpty.dll` +
  `OpenConsole.exe` pair first, `kernel32`'s `CreatePseudoConsole` as the fallback — because the
  inbox `conhost.exe` on Windows 11 24H2 swallows Sixel DCS payloads, so IN-0028's graphics
  depend on the bundled host (`DEC-0013`, owned and being rewritten by IN-0030). `openpty` on
  Unix.

**OneTerm owns the parser too.** The state machine is a module of `oneterm-vt`
(`parser/`), not a dependency and not a separate crate. `vte` is retained only as a
differential-test oracle (a dev-dependency on the unmodified crates.io release).

**The API is OneTerm's, not alacritty's.** Call sites may change anywhere in the workspace if
the result is better. Absolute row ids replace negative `Line` indices, events are batch-
returned from `feed()` instead of firing under a lock, colours use typed keys instead of the
magic indices 256/257/258, and the ConPTY resize policy becomes an engine-native `ResizePolicy`
(`DEC-0015` records the two contracts future work inherits).

**Upstream alacritty stays the parity reference.** Its 45 Apache-2.0 ref-test recordings are
vendored (recordings only, ~956 KB) and replayed as the migration gate, so "we lost behaviour
nobody can name" is caught by real tmux / vim / zsh captures rather than by review.

**The baseline stays runnable until the parity gate is green.** The engine lands behind the
existing `TerminalSession` / `TerminalContent` seam; `vendor/`, `vendor/refresh.sh`, its CI job
and the `THIRD-PARTY-NOTICES.md` § 2 rows are deleted in the last packet, not the first.

## Alternatives

- [x] Selected: write and own the engine, parser included, as workspace crates.
- [ ] **Keep the vendored alacritty fork.** Rejected: every capability on the roadmap is another
  patch on a tree that `refresh.sh --check` must keep rebasable, against an upstream that is in
  maintenance mode and has rejected graphics by policy. The 823 patch lines are the cost we are
  buying out, and they grow with each feature. It also keeps the three unbounded-buffer defects
  above until upstream releases and we rebase.
- [ ] **Adopt `rio-vt` 0.5.26.** Rejected on two grounds. Licence: it declares MIT while
  `crosswords/`, `grid/` and `ansi/` are visibly derived from Apache-2.0 `alacritty_terminal` and
  its own file header says so; we enforce licences mechanically (`deny.toml`) and generate
  `THIRD-PARTY-NOTICES.md`, so an unresolved relicensing chain would ship in every release
  (`research/prior-art.md` § 8.2, § 10 risk 4). Structure: it is a fork we would immediately
  patch again for OSC 9;7 and for anything else OneTerm-specific, which is the cost this
  decision exists to remove. It also adds `simdutf` (a C++ FFI crate, new toolchain requirement
  on Windows CI) and `tracing`, which is on the do-not-re-add list
  (`docs/agents/dependencies.md` § 3). It remains excellent design prior art: 8-byte packed
  cell, interned styles and extras with GC, `total_lines_scrolled` stable row space,
  `ReflowRemap`, and a resize that measures ~45x faster than alacritty's.
- [ ] **Adopt `libghostty-vt` 0.2.1.** Rejected: it is the best engine in the survey and the
  worst fit for this product. Zig 0.16 enters the Windows build (or we vendor a prebuilt
  `.lib` with a drift job, a second build system); there is no tagged release and the header
  says the ABI "is definitely going to change"; all handle types are `!Send + !Sync`, which
  invalidates the `Arc<Mutex<Terminal>>` ownership model and every test helper built on it;
  it has **no Sixel**, so adopting it regresses IN-0028; and internals cannot be patched, so
  OSC 9;7 would have to be upstreamed into Zig or pre-filtered out of the byte stream — the
  opposite of the reason for this intake (`research/prior-art.md` § 3.2, § 3.4, § 10 risk 7).
  Revisit only if Ghostty tags a stable C ABI **and** lands Sixel. Its page list, interned
  `RefCountedSet` with an explicit overflow ladder, row dirty bit inside the row header, and
  two-phase `beginUpdate` / `endUpdate` render state are all borrowed as design.
- [ ] **Keep `vte` as the parser layer and own only the grid.** Rejected for three reasons.
  (a) `vte`'s OSC buffer is either unbounded (`std`) or capped at 1 KiB (`no_std`), and neither
  is what we want: OSC 52 payloads legitimately exceed 1 KiB while an unterminated `ESC ]` from
  any SSH session grows memory without bound. (b) `ansi.rs` — `vte`'s semantic layer — is where
  every OneTerm patch has lived (`report_osc`, DCS forwarding); owning the grid but not the
  dispatch layer would leave the patch series exactly where it is. (c) The dispatch layer needs
  structural colon-subparameter information (`38:2:<cs>:R:G:B` versus `38;2;R;G;B`) and a
  batched print-run entry point, neither of which `vte` 0.15 exposes. The state machine itself
  is the least valuable part to write and is kept as a replaceable module so the choice can be
  revisited without touching dispatch.
- [ ] **`wezterm-term` / `termwiz`.** Not a dependency we can take: unpublished on crates.io, a
  monorepo git pin with a `NOASSERTION` top-level licence. Its `SequenceNo` damage model and
  dual-form line storage are adopted as design.

## Consequences

- [ ] Benefit to confirm: `vendor/`, `vendor/patches/` (823 lines), `vendor/refresh.sh`, its CI
  job and the two `THIRD-PARTY-NOTICES.md` § 2 rows disappear; a new VT capability becomes a
  change in a first-party crate with normal review instead of a patch rebase.
- [ ] Benefit to confirm: the three unbounded-buffer defects are designed out (bounded OSC with
  truncation, capped grapheme length with a GC, no unsafe row allocation).
- [ ] Benefit to confirm: `crates/terminal/src/model.rs:481-541` (the `KeepViewportTop`
  correction and its scratch-grid probe) and `crates/terminal/src/backend/line_accounting.rs`
  (49 lines reconstructing an absolute line counter) are deleted, not ported.
- [ ] Tradeoff: roughly 12-13k lines of grid and parse code must be written and must clear
  today's behaviour bar before it can beat it. Mitigated by the ref-test parity gate, the
  differential old-versus-new runner during migration, property-tested reflow, and a phase
  plan whose every exit criterion is a green test rather than a judgement.
- [ ] Tradeoff: reflow is the highest-risk component in the rewrite (alacritty has six open
  reflow bugs, xterm.js ships a "known to fail for an unknown reason" guard, and OneTerm has a
  second contract to satisfy in conhost's quirks). Mitigated by porting `avt`'s Apache-2.0
  reflow iterator with attribution, designing the API around a tracking-point slice and an
  old-to-new remap from day one, and proptest coverage.
- [ ] Tradeoff: `parking_lot`, `unicode-width`, `unicode-segmentation`, `smallvec`, `memchr`
  and `proptest` become direct workspace declarations. All six are already in `Cargo.lock`
  transitively, so the dependency graph does not grow; `docs/agents/dependencies.md` § 3 must
  record them.
- [ ] Follow-up: OSC 9;7 collides with ConEmu's "run some process with arguments" sub-code
  (`research/prior-art.md` § 6.4). That is a defect in `docs/osc-agent-status.md`, not in the
  engine, and needs its own packet. The engine's OSC registration table makes either resolution
  a one-line registration change.
- [ ] Follow-up: while the vendored fork is still the shipping engine, the unbounded `osc_raw`
  remains reachable from any SSH session. Either cap it with one more vendor patch or accept the
  exposure for the migration window — an owner decision recorded in `IN-0029.md`.
