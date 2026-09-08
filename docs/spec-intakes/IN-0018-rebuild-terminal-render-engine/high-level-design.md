# High-Level Design: Rebuild terminal-view from scratch on GPUI

Intake: IN-0018
Lane: normal
Date: 2026-09-08

## Idea

Replace the internals of `crates/terminal-view` (`oneterm-terminal-view`) with a render engine
designed from GPUI 0.3.3's own element model and first principles, while keeping the crate's
public API, dependency edge set, and every user-visible behavior inventoried for the old crate.
The terminal **engine** (`crates/terminal`, alacritty grid) is consumed unchanged through
`TerminalSession`; the view owns rendering, input, and per-terminal UI state.

Three ideas drive the design:

1. **One snapshot, per-row plans, one paint layer.** Each frame the element calls
   `snapshot_into` once, decides which display rows changed (damage + row hash), rebuilds only
   those rows' `RowPlan`s (background spans, shaped text runs, shape quads, decorations), and
   paints every plan inside a single `paint_layer` so the whole grid is one bounds-tree insert
   and a handful of draw calls. An idle terminal shapes nothing and plans nothing.
2. **Shapes are geometry, not glyphs.** Box drawing, blocks, shades, braille, and powerline
   code points are built in a center-origin cell space from `Stroke`/`Rect` primitives, mirrored
   by reflection, snapped symmetrically to device pixels, and painted as quads (paths only for
   arcs and diagonals). They join seamlessly across cells in any font.
3. **Alacritty stops at `render/frame.rs`.** That file is the only module that names an
   alacritty type; everything above it works with view-owned `Cell`, `CellFlags`, `Color`,
   `CursorShape`, `Selection`, and `Damage`.

## Diagram

```text
crates/terminal (engine, unchanged)             crates/terminal-view (this intake)
┌──────────────────────────────┐   snapshot_into   ┌────────────────────────────────────────────┐
│ Entity<Box<dyn TerminalSession>>│ ───────────────▶ │ render/frame.rs   Frame (view-owned cells)  │
│  · grid + damage             │   query_state    │      │ damage ∪ cursor row ∪ scroll rotation │
│  · SessionEvent channel      │ ◀─── write/mouse │ render/plan_cache.rs  hash-verify → rebuild │
└──────────────────────────────┘                  │      │ RowPlan  (row_plan.rs + shapes.rs +   │
        ▲                                         │      │           glyphs.rs + theme/ + url/)   │
        │ events pump (foreground task)           │ render/element.rs  prepaint: geometry,      │
        │                                         │      overlays, cursor, hitbox, IME install   │
┌───────┴──────────────────────┐ Render           │      paint: layer 1 grid, layer 2 cursor     │
│ terminal_view/view.rs        │ ────────────────▶│ input/  keys · mouse · menu · edit          │
│  TerminalView (Entity)       │ builds element   │      ↑ GridGeometry (shared hit-test)        │
│  search · scrollbar · gutter │ + bars/overlays  └────────────────────────────────────────────┘
│  completion · ime · agent    │
└───────┬──────────────────────┘
        │ Entity<TerminalView>
┌───────┴──────────────────────┐   ┌───────────────────┐   ┌──────────────────────────────────┐
│ space/  SpaceTree (splits)   │◀──│ panel/ TerminalPanel│──▶│ status.rs · agent.rs · security.rs│
└──────────────────────────────┘   └───────────────────┘   └──────────────────────────────────┘
```

## UI Wireframe

No intended user-visible change except the shape fixes listed under Deviations. The terminal
tab keeps its layout:

```text
+------------------------------------------------------------------+
| [● title]  [+ ▾]                                    (tab strip)  |
+------------------------------------------------------------------+
| [progress bar (OSC 9;4), top edge, only while active]            |
| [ SSH closed banner (Alert::warning), only after remote close ]  |
| [search: [query........] [Aa] [W] 3/12 [<] [>] [x]] (Ctrl+F)     |
|                                                             [🔔]  |
| [HH:MM:SS]  12 │ $ cargo build                                  ▒|
| [HH:MM:SS]  13 │ ┌──────┐ ▀▄▀▄ ░▒▓ ⣿⣿  E0B0                      ▒|
| [--:--:--]     │ █ cursor                                        ▒|
|    gutter      │  grid (one TerminalElement)          scrollbar ▲|
|                │  ┌ completion overlay ─────────┐                 |
|                │  │ > git status      history   │                 |
|                │  │   git stash       command   │                 |
|                │  └─────────────────────────────┘                 |
+------------------------------------------------------------------+
```

Split Spaces, placeholders, context menus, and the rename dialog are unchanged from the
inventory (`docs/terminal-split/` is historical after this intake; the parity checklist below is
the contract).

## Data Flow

1. `TerminalPanel::open(PanelSpec)` spawns or wraps a `Box<dyn TerminalSession>`, wraps it in an
   `Entity`, and creates `TerminalView::new(session, TerminalDeps, window, cx)`.
2. `TerminalView::new` takes the session's event receiver once and runs the **events pump**
   (`cx.spawn`): consecutive `Output` events are drained with `try_recv`, reliable events are
   handled in order (`handle_event`), then `cx.notify()`.
3. `TerminalView::render` reads settings/theme, refreshes `RenderInputs` (font, metrics inputs,
   theme, cursor config, focus, blink, gutter stamps, search highlights) into the shared
   `Rc<RefCell<RenderState>>`, and builds the wrapper div (focus tracking, key/mouse listeners,
   context menu) around one `TerminalElement`, plus the bars, badges, banner, and overlays.
4. `TerminalElement::request_layout` asks for a full-size Taffy node with no children.
5. `TerminalElement::prepaint(bounds)`: compute `GridGeometry`; if the grid size changed call
   `session.resize`; `frame.snapshot(session)` (the frame's only `snapshot_into`); `plan_cache
   .update(...)` rebuilds changed rows; compute selection/search rects and the cursor paint;
   insert the hitbox; return `PrepaintState`.
6. `TerminalElement::paint`: `paint_layer(bounds)` → background quads, shape quads, search and
   selection quads, paths, decorations, glyphs, gutter labels; second `paint_layer` for the cursor;
   set the mouse cursor style; install the IME handler when the view is focused.
7. Input: the wrapper div's listeners map pixel → grid through `GridGeometry`, classify keys
   (`input/keys.rs`) and mouse events (`input/mouse.rs`), and call `TerminalSession` methods; the
   backend decides mouse-mode encoding versus selection. Every input path that writes bytes first
   `scroll_to_bottom()`s.
8. Session events: `Title` → `TerminalViewEvent::TitleChanged` (panel re-reads the live title);
   `Clipboard*`, `Notification`, `Progress`, `AgentStatus`, `Exited`, `Closed`, `Bell` handled as
   inventoried (§2.20); `Cwd` and `ShellIntegration` stay unhandled.

## Detail Design

- [x] Detail design: added (optional)
- Reason: the geometry, cache, and input contracts are exact enough that implementers should
  not redesign them per packet.

| File | Concern |
| --- | --- |
| `low-level-design/shapes.md` | Cell space, `Stroke`/`Rect`, thickness rules, per-family construction tables, symmetric snapping, API, tests |
| `low-level-design/render-pipeline.md` | Frame model types, `RowPlan`, plan cache algorithm, glyph cache, element phases, layers, cursor, decorations, overlays, gutter, diagnostics, wide/zero-width, allocation plan, `FrameStats` |
| `low-level-design/input.md` | Key classification table, IME rules, mouse state machine, wheel math, paste path, scroll chords, hit-testing contract |

## Module Map

`~` sizes are line estimates including unit tests in `*_tests.rs` siblings. "retained" modules
keep their current files and tests; they are adapted, not rewritten (they are domain logic with
no renderer structure, and their tests are the acceptance spec).

| Path | Responsibility | Packet | ~ |
| --- | --- | --- | --- |
| `src/lib.rs` | module declarations, the 7 public items, `init` | 0050 | 50 |
| `src/render/mod.rs` | declarations only | 0046 | 20 |
| `src/render/shapes.rs` + `shapes_tests.rs` | `shape_quads`, `shape_paths`, `Stroke`/`DeviceRect`, mirror/rotate, symmetric snap | 0046 | 650 + 450 |
| `src/render/frame.rs` | `Frame`, `FrameRow`, `Cell`, `Color`, `CellFlags`, `CursorShape`, `Selection`, `Damage`; the only alacritty-typed file | 0047 | 320 |
| `src/render/metrics.rs` | `CellMetrics` (device-snapped cell), `GridGeometry` (origin, padding, gutter, rows/cols, hit-test), `grid_size_for` | 0047 | 220 |
| `src/render/glyphs.rs` | `GlyphCache`: run text → `ShapedLine` via `shape_line_by_hash`/`force_width`, generation eviction | 0047 | 160 |
| `src/render/row_plan.rs` | `RowPlan` + `build_row_plan` (bg spans, text runs, shape quads coalesced, paths, decorations, class merge, contrast) | 0047 | 480 |
| `src/render/plan_cache.rs` | `PlanCache`: candidates (damage ∪ cursor row ∪ mask delta), scroll rotation, hash verify, style-key invalidation | 0047 | 260 |
| `src/render/state.rs` | `RenderState` (frame, plans, glyphs, geometry, inputs, overlays, scratch, stats) shared by view/element/input | 0047 | 150 |
| `src/render/element.rs` + `element_tests.rs` | `TerminalElement` (`Element` impl), `PrepaintState`, paint order, IME install hook | 0047 | 380 + 260 |
| `src/render/cursor.rs` | cursor shape/color resolution, blink gating, hollow vs filled, glyph re-paint | 0047 | 160 |
| `src/render/overlay.rs` | selection rects, search rects, URL mask → `Class::Url` merge into the class buffer | 0047 | 220 |
| `src/render/diagnostics.rs` | `FrameStats`, `LatencySamples`, 5 s throttled `log::debug!` (cfg-gated) | 0047 | 130 |
| `src/input/mod.rs` | declarations | 0048 | 10 |
| `src/input/keys.rs` | `KeyAction`, `classify_key`, `map_key`, completion interception order | 0048 | 380 |
| `src/input/mouse.rs` | `MouseState` machine, wheel math, selection type, URL ctrl-click, copy-on-select, scrollbar drag | 0048 | 380 |
| `src/input/menu.rs` | context-menu builder (16 items, submenus) | 0048 | 260 |
| `src/input/edit.rs` | copy / paste / select-all / clear + paste-error toast | 0048 | 90 |
| `src/terminal_view/mod.rs` | declarations, `pub(crate) use` of `TerminalView`, `TerminalViewEvent`, `TerminalDeps`, `SplitContext` re-export | 0049 | 30 |
| `src/terminal_view/view.rs` | `TerminalView` struct, `new`, events pump with coalescing, blink task, focus, `shutdown`, OSC-to-UI | 0049 | 480 |
| `src/terminal_view/render.rs` | `Render`/`Focusable`: inputs refresh, wrapper div + listeners, element, bars/badges/banner/progress/overlays | 0049 | 320 |
| `src/terminal_view/ime.rs` | `EntityInputHandler` | 0049 | 130 |
| `src/terminal_view/search.rs` | `SearchState`, debounce, navigation, bar | 0049 | 360 |
| `src/terminal_view/scrollbar.rs` | `ScrollbarState`, geometry, drag, auto-hide fade, element | 0049 | 260 |
| `src/terminal_view/gutter_timestamps.rs` | grow-only stamps, epoch reset, formatting | 0049 | 210 |
| `src/terminal_view/completion.rs` | completion glue: prompt strip, recompute, accept, history capture, overlay anchoring | 0049 | 360 |
| `src/terminal_view/agent_status.rs` | `push_agent_status`, lifecycle marks, nav registration | 0049 | 130 |
| `src/theme/` (retained) | `TerminalTheme` build, palette, overrides, dynamic colors, contrast (warts 9, 10 fixed), `Color` → `Hsla` table | 0047/0049 | existing |
| `src/highlight/` (retained) | semantic overlay bridge (`scan_line_into`) | 0047 | existing |
| `src/url/` (retained) | detect, mask, hover — inputs adapted to `Frame`/`FrameRow` | 0047/0049 | existing |
| `src/completion/` (retained) | controller + overlay | 0049 | existing |
| `src/space/` (rewritten in place) | `SpaceId`, `SplitContext`, `SpaceTree`, nodes, ops, placeholder, render, drag | 0050 | ~900 |
| `src/panel/` (rewritten in place) | `PanelSpec`, `TerminalPanel`, ops, actions, title | 0050 | ~1500 |
| `src/status.rs`, `src/agent.rs`, `src/security.rs` (retained) | bridges into `oneterm_state` | 0050 | existing |

Deleted by US-0049: `src/view/`, `src/element/`, `src/layout/`, `src/box_drawing/`,
`src/handlers/`. Deleted by US-0050: nothing else remains; `#[allow(dead_code)]` on the new
roots is removed there. Stable module paths across the swap: `crate::panel::TerminalPanel`,
`crate::space::{SpaceId, SplitContext}`, `crate::terminal_view::{TerminalView, TerminalDeps,
TerminalViewEvent}`.

## Frame Pipeline

Once per font/size/scale change (in `TerminalView::render` when `RenderInputs` differ):

- `font_id = text_system.resolve_font(font)`, `cell_width = snap_device(ch_advance)` (fallback
  `advance('m')`, then 8 px), `line_height = snap_device(max(font_size * factor, ascent +
  descent))`, `baseline = text_system.baseline_offset(font_id, font_size, line_height)`.
  `snap_device` rounds to a whole device pixel and returns both `Pixels` and the integer
  device size; this collapses glyph atlas subpixel variants to one per glyph.
- `TerminalTheme` is rebuilt only when `(gpui theme, color overrides, dynamic colors)` differ;
  its `Hsla` table (`[Hsla; 259]`: 256 indexed + fg/bg/cursor) resolves `Color` in O(1).
  `session.set_default_colors` is pushed only when the resolved palette changed.

Per frame:

| Phase | Work | GPUI calls |
| --- | --- | --- |
| `request_layout` | none | `window.request_layout(Style{size: full}, [], cx)` |
| `prepaint` | `GridGeometry` from bounds; resize session if `(rows, cols)` changed; `frame.snapshot`; `plan_cache.update`; overlays; cursor; gutter labels for visible rows | `insert_hitbox(bounds, Normal)` |
| `paint` | layer 1: bg quads → shape quads → search quads → selection quads → paths → underlines/strikes → glyphs (mono/subpixel via `paint_glyph`, emoji via `paint_emoji`) → gutter glyphs; layer 2: cursor quads and cursor glyph | `paint_layer`, `paint_quad`, `paint_path`, `paint_underline`, `paint_strikethrough`, `paint_glyph`, `paint_emoji`, `set_cursor_style`, `handle_input` |

Quads inside one layer keep insertion order, so backgrounds, then shapes, then translucent
search/selection produce the intended stacking; the kind order (Quad → Path → Underline →
Sprite) guarantees glyphs over quads regardless of call order. The cursor needs its own layer
because a block cursor must cover glyphs.

All quad edges are computed as `origin + px(device_x / scale)` from whole-device-pixel values so
`snap_bounds` reproduces identical edges for abutting cells (no seams, no double coverage).

## Cross-frame State and Caches

`RenderState` (one per `TerminalView`, `Rc<RefCell<_>>`, cloned into the element and read by
input handlers):

| Field | Lifetime / reuse |
| --- | --- |
| `frame: Frame` | wraps the reused `TerminalContent`; `snapshot_into` reuses its `cells` and damage buffers |
| `plans: PlanCache` | one `RowPlan` per display row; vectors cleared, not reallocated, on rebuild; rotated on scroll |
| `glyphs: GlyphCache` | `HashMap<RunKey, (ShapedLine, generation)>`, cap 4096; entries unused for 2 generations are evicted when the cap is hit |
| `geometry: Option<GridGeometry>` | written in prepaint, read by input handlers (hit-test contract) |
| `inputs: RenderInputs` | written by `TerminalView::render` before the element is built |
| `overlays` | `selection: Vec<DeviceRect>`, `search: Vec<SearchRect>`, `url_mask: Vec<Vec<bool>>` (prev/cur double buffer), reused |
| `scratch` | run text `String`, class `Vec<u8>`, line text `String`, `char_cols: Vec<u16>`, rect scratch, `SmallVec` of open rects |
| `stats: FrameStats`, `latency: LatencySamples` | cfg(any(test, feature = "terminal-diagnostics")) |

The view keeps (outside `RenderState`): `SearchState`, `ScrollbarState`, `GutterTimestamps`,
`CompletionState`, `UrlHover`, `SemanticOverlay`, notification queue, progress, bell, focus,
tasks, `last_pushed_palette`, `cached_font`.

## Invalidation Rules

| Change | Effect on plans |
| --- | --- |
| `Damage::Full` | every row is a candidate; each candidate is re-hashed; only rows whose hash differs from the stored hash are rebuilt |
| `Damage::Rows(lines)` | listed rows are candidates (hash-verified) |
| cursor row | always a candidate (catches undamaged echo) |
| `display_offset` delta `d`, `abs(d) < rows`, grid unchanged | plans and hashes are rotated (`rotate_right(d)` when `d > 0`, i.e. scrolling into history); the `d` scrolled-in rows are candidates; the rest are hash-verified only if damage says so |
| `abs(d) >= rows`, grid size change, `StyleKey` change (font family/size/weight/features, palette hash, min contrast, semantic enabled, shell profile, show_gutter) | all rows rebuilt (hash check skipped) |
| URL mask row changed vs previous frame | that row is a candidate (fixes wrapped-URL continuation rows) |
| selection, hover, search matches, cursor blink, focus, scrollbar | never touch plans (painted as overlays / cursor layer) |
| `GlyphCache` | key includes font family/size/weight/style bits, so a style change naturally misses; stale entries age out |
| gutter | labels are shaped through `GlyphCache` keyed by label text, so unchanged rows hit the cache |

`FrameStats` on an idle frame must read `rows_planned == 0`, `shape_calls == 0`,
`quads > 0`.

## Threading/Async

Everything in this crate runs on the GPUI foreground thread. Background work lives in
`crates/terminal` (PTY/SSH pumps). Tasks held by `TerminalView` (dropping cancels them):

- **events pump**: `cx.spawn` loop over `take_events()`; `Output` coalescing via `try_recv`;
  `handle_event` per reliable event; stops on `Closed`/channel end.
- **blink**: `cx.spawn` loop, `background_executor().timer(500 ms)`, flips `blink_visible` and
  notifies only while `alive && !ssh_closed && focused && cursor_blink == On`.
- **search debounce**: one `Task` replaced on each input change (150 ms).
- **scrollbar fade**: `on_next_frame` re-notify while `opacity.is_some()`.
- `snapshot_into` briefly locks the engine's `FairMutex` inside prepaint; nothing in the view
  holds it across a GPUI call.

## Public and pub(crate) Interfaces

Public (unchanged): `init`, `PanelSpec`, `TerminalPanel`, `status_metrics`, `agent_focuser`,
`terminal_security_policy`, `find_in_active_terminal`, `new_terminal_with_shell_cmd`, cargo
feature `terminal-diagnostics`. Dependencies: unchanged `Cargo.toml` edge set.

`TerminalView` surface used by `panel/` and `space/` (US-0049 keeps the old field names so the
old panel/space compile against it with a type rename only):

```rust
pub(crate) struct TerminalDeps { settings, agent_registry: Option<_>, completion_history: Option<_> }
impl TerminalDeps { pub(crate) fn from_globals(cx: &App) -> Self }

pub(crate) enum TerminalViewEvent { TitleChanged }
impl EventEmitter<TerminalViewEvent> for TerminalView {}
impl Focusable for TerminalView {}          // returns `focus`

pub(crate) struct TerminalView {
    pub(crate) session: Entity<Box<dyn TerminalSession>>,
    pub(crate) duplicate_config: Option<SessionDuplicateConfig>,
    pub(crate) split_ctx: Option<crate::space::SplitContext>,
    pub(crate) focus: FocusHandle,
    ..private
}
impl TerminalView {
    pub(crate) fn new(session, deps: TerminalDeps, window, cx: &mut Context<Self>) -> Self;
    pub(crate) fn shutdown(&mut self, cx: &mut Context<Self>);            // idempotent
    pub(crate) fn toggle_search(&mut self, window, cx: &mut Context<Self>);
    pub(crate) fn handle_event(&mut self, ev: SessionEvent, cx: &mut Context<Self>); // tests
    #[cfg(any(test, feature = "terminal-diagnostics"))]
    pub(crate) fn render_diagnostics(&self) -> FrameStats;
}
```

`space::SplitContext { panel: WeakEntity<TerminalPanel>, space_id: SpaceId }` and
`space::SpaceId` keep their paths through the US-0050 rewrite.

Element constructor input (built by `terminal_view/render.rs` each frame):

```rust
pub(crate) struct TerminalElementSpec {
    pub id: ElementId,
    pub session: Entity<Box<dyn TerminalSession>>,
    pub state: Rc<RefCell<RenderState>>,          // inputs already refreshed by the view
    pub ime: Option<Box<dyn FnOnce(Bounds<Pixels>, &mut Window, &mut App)>>, // installs handle_input
}
```

Everything else the element needs (theme, font, metrics inputs, cursor config, focus, blink,
gutter stamps, search highlights, semantic overlay, shell profile) travels in
`RenderState.inputs` so the element is testable with `FakeTerminalSession` and no view.

## Deviations from Old Behavior

| # | Old | New | Reason |
| --- | --- | --- | --- |
| 1 | 12 of 16 powerline code points painted as solid blocks (wart 1) | all 16 have real geometry (triangles/chevrons/half-discs/quadrant triangles, filled and outlined) | inventory wart; owner allowed |
| 2 | shades emit nothing above 1024 device px² (wart 2) | pattern pitch scales with the cell; bounded quad count at any size | inventory wart |
| 3 | `╒/╘` and `╓/╙` identical (wart 3) | up/down variants are mirror_y of each other | inventory wart |
| 4 | SGR hidden ignored (wart 5) | `CellFlags::HIDDEN` cells emit background but no text run | inventory wart |
| 5 | contrast luminance exponent 2 (wart 9) | WCAG exponent 2.4 | inventory wart |
| 6 | settings `min_contrast` 0.0 silently disables enforcement (wart 10) | `min_contrast <= 0.0` keeps the theme default 4.5; `0.0 < v <= 1.0` disables; `> 1.0` is the threshold | inventory wart |
| 7 | rounded corners via 4×4 supersampled alpha rects | GPUI stroked arc paths | paths are the sanctioned primitive for arcs; fewer primitives |
| 8 | scroll with `Damage::Full` rebuilt every row | rotation + hash verification rebuilds only changed rows | performance; observable output identical |
| 9 | light stroke thickness `round(cw/6)` etc. | thickness table in `shapes.md`, parity-matched per axis | symmetric snapping (DEC-0007 item 4) |
| 10 | URL continuation rows only replanned when themselves damaged | mask delta marks them dirty | correctness of always-on URL underline across wraps |

Kept as-is on purpose (out of scope, from inventory §6): no middle-click paste (6), IME on the
primary screen only (8), `strip_prompt` heuristic (7), `class_style.bg`/`prompt_line_bg` unused
(11), no layout persistence (15), no pane swap / focus traversal (16).

## Parity Checklist

One line per inventory item (`feature-inventory.md` §2 numbering), grouped by owning packet.
US-0050 ticks every box during sign-off.

### US-0046 shapes (§2.3 items 37–42)

- [ ] 37 lines/corners/tees/crosses/dashes/doubles U+2500–257F as quads, centered strokes, seamless across cells; diagonals as paths (were font fallback)
- [ ] 38 rounded corners U+256D–2570 anti-aliased (paths)
- [ ] 39 block elements U+2580–259F as rects; identical full-width-band runs coalesce into one quad
- [ ] 40 powerline U+E0B0–E0BF (deviation 1: all 16 real)
- [ ] 41 shades U+2591–2593 (deviation 2: scaled pattern)
- [ ] 42 U+25AC single half-height centered rect
- [ ] braille U+2800–28FF as dot quads (new coverage required by DEC-0007)

### US-0047 render core (§2.3 items 1–36, §2.15, §2.22)

- [ ] 1 bg rect for non-default bg or inverse, merged with adjacent same-color rect
- [ ] 2 fg: inverse swap → semantic class fg when ANSI default or `override_ansi` → contrast unless exact/decorative
- [ ] 3 bold from flag, OR'd with class bold
- [ ] 4 italic from flag, OR'd with class italic
- [ ] 5 dim = fg alpha × 0.7
- [ ] 6 inverse swaps fg/bg and forces a bg rect
- [ ] 7 hidden: deviation 4 (implemented)
- [ ] 8 single underline for any underline flag or hyperlink, 1 px
- [ ] 9 undercurl = wavy underline
- [ ] 10 strikethrough 1 px
- [ ] 11 class underline only when no ANSI underline
- [ ] 12 wide chars: spacer has no run, glyph spans two columns
- [ ] 13 zero-width chars join the base run; overflow slot after them has no run but keeps bg
- [ ] 14 cursor shapes Block/Beam(20 % width, min 1 px)/Underline(15 % height, min 2 px)/HollowBlock/Hidden
- [ ] 15 cursor color = override or palette cursor
- [ ] 16 config shape override except when snapshot shape is Hidden
- [ ] 17 unfocused always paints cursor; focused honors blink
- [ ] 18 hollow when unfocused or HollowBlock, 1-device-px outline, Block only; Beam/Underline always filled
- [ ] 19 filled block cursor re-paints the covered glyph in bg color
- [ ] 20 selection quads above bg/search, below text
- [ ] 21 search highlight active vs match colors, painted after bg before selection
- [ ] 22 URL underline via `Class::Url` through the class decoration path
- [ ] 23 semantic overlay baked into cell style, no separate layer
- [ ] 24 gutter `[HH:MM:SS] N` two colors, shaped labels cached
- [ ] 25 gutter fallbacks `[--:--:--]`, newest/oldest reuse
- [ ] 26 scrollbar not in the element (US-0049)
- [ ] 27 diagnostics = log line only, every ≥ 5 s under the feature
- [ ] 28 bell badge lives in the view (US-0049)
- [ ] 29 per-row plan cache with reusable dirty bitset
- [ ] 30 global invalidation on grid size, non-scroll display change, style key; Full damage handled (deviation 8)
- [ ] 31 scroll-only rotation; delta ≥ rows dirties all
- [ ] 32 partial damage marks listed rows
- [ ] 33 cursor row hash fallback (FNV-1a over ch/fg/bg/flags/zerowidth/hyperlink)
- [ ] 34 URL mask recomputed only when any row is dirty
- [ ] 35 selection/hover never invalidate rows
- [ ] 36 selection mapping: block = same column span per line; linear = single / first-to-EOL / full middle / SOL-to-end
- [ ] §2.15 1–5, 7–9 semantic bridge: synchronous per dirty row, style-key rescans, char→column flatten with wide propagation, URL mask authoritative, defaults from embedded JSON (malformed → inactive)
- [ ] §2.22 resize: `grid_size_for` (gutter, padding, device snapping, ≥ 1×1); `session.resize` only when `(rows, cols)` changed; called from prepaint
- [ ] `view/tests::phase0_renderer_baseline_counts_dirty_and_idle_frames` equivalent passes on the new element

### US-0048 input (§2.4–2.8)

- [ ] 2.4.1 Ctrl/Cmd+F toggles search
- [ ] 2.4.2 Enter swallowed while the search input is focused
- [ ] 2.4.3 Ctrl+Shift+Space forces completion
- [ ] 2.4.4–6 platform +`-` / `=`/`+` / `0` zoom out/in/reset
- [ ] 2.4.7–10 Shift+PageUp/PageDown/Home/End scroll screen/top/bottom
- [ ] 2.4.11–12 platform+Shift+Up/Down scroll one line
- [ ] 2.4.13 Ctrl+Shift+C (Cmd+C on macOS) copy
- [ ] 2.4.14–15 Ctrl+Shift+V / Cmd+V / Shift+Insert paste
- [ ] 2.4.16 plain printable char on the primary screen is a no-op (IME path delivers it)
- [ ] 2.4.17 Windows AltGr chord with `key_char` is a no-op
- [ ] 2.4.18 Ctrl+C → `send_ctrl_c` + snap to bottom, regardless of selection
- [ ] 2.4.19 other mapped keys → `encode_key(spec, mods, app_cursor)` → write; snap to bottom; clear bell
- [ ] 2.4.20 unmapped → no-op, no stop_propagation
- [ ] 2.4.21 completion interception order (Down/Ctrl+N, Up/Ctrl+P only when selected; Esc; Enter accept-if-selected else forward + history capture; Tab select-then-accept when `accept_tab`)
- [ ] key handoff: `map_key` named keys / `key_char` / `"space"` → NUL for Ctrl+Space; `Interrupt` and completion accept bypass `encode_key`
- [ ] 2.5.1–4 click count → Simple/Semantic/Lines; Alt → Block
- [ ] 2.5.5 drag → `mouse_drag` every move
- [ ] 2.5.6 mouse-mode decided by the backend; view always forwards
- [ ] 2.5.7 wheel: delta ÷ line height × multiplier, `abs ≥ 0.001`
- [ ] 2.5.9 no middle-click paste (Middle forwards to the session)
- [ ] 2.5.10 right click: context menu only when `show_context_menu`, else forwarded
- [ ] 2.5.11 Ctrl/Cmd+Left on a URL opens it; else normal mouse-down
- [ ] 2.5.12 copy-on-select on left-up when the setting is on
- [ ] 2.5.13 scrollbar thumb drag checked before selection; ends on any button release
- [ ] 2.5.14 URL hover on every move, pointer cursor independent of Ctrl
- [ ] 2.5.15 every down/up/drag marks the scrollbar scrolled and notifies
- [ ] 2.6 context menu: 16 steps incl. Duplicate submenu, Log submenu, Close Space guard
- [ ] 2.7.2 follow-output only on Send/Interrupt/paste/IME commit, not on output
- [ ] 2.7.5–6 scroll actions notify; alt-screen no-op is backend-side
- [ ] 2.8.1–7 copy (silent no-op), paste (scroll_to_bottom then `paste`, warning toast on error), select all, clear; reachable from keys, menu, panel actions

### US-0049 terminal view (§2.9–2.17, 2.19–2.21)

- [ ] 2.9 detection: OSC 8 first, then `https:// http:// ftp:// www.` wrap-aware, trailing punctuation strip, `www.` → `https://`
- [ ] 2.9 masking three-pass (mark, wrap-extend, strip)
- [ ] 2.9.1–7 always-on highlight; cell-granular re-detect; modifiers-changed re-detect; pointer on hover; open needs Ctrl/Cmd+Left; policy Allow/Confirm(dialog)/Deny(log); leaving clears hover
- [ ] 2.10.1–7 IME: disabled on alt screen; marked text lives in the backend; commit = scroll_to_bottom + `commit_text` + clear bell; preedit set/clear; candidate bounds from cursor cell + `GridGeometry`; `text_for_range`/`character_index_for_point` → None
- [ ] 2.11.1–7 scrollbar geometry per frame, no thumb when not scrollable, thumb ≥ 24 px inverted mapping, optimistic drag, 2 s opaque + quartic fade to 3 s, 8 px thumb in 12 px track, gray 0.8 × opacity
- [ ] 2.12.1–7 gutter stamps only on Output, grow-only, epoch/absolute reset, content-only, pop-front, lazy format, `show_gutter` gate
- [ ] 2.13.1–12 search: toggle, refocus-if-open, clear on close, 150 ms debounce, Enter/Shift+Enter wrap, case/whole-word only, `n/total`, backend search, run vs refresh-if-dirty, viewport-clamped highlights, click/Esc propagation rules
- [ ] 2.14.1–14 completion: auto trigger with prompt strip, forced trigger, gating, sole-exact suppression, navigation, Tab, accept bytes, Escape, auto dismiss, sources, history capture with redaction, anchoring, theme-driven overlay
- [ ] 2.15.3, 6 style-key rescan on enable/profile; `class_style.bg` unused
- [ ] 2.16 theme table (fg/bg/cursor from theme, ANSI_16, selection by lightness, gutter 50 %, search yellows, class styles); override pass 1 (deviation 6), pass 2 dynamic colors; palette resolution; contrast (deviation 5)
- [ ] 2.17 settings observed by the panel; view re-reads per render; font rebuilt only on family/weight/features change; palette push skip-checked
- [ ] 2.19.1–6 SSH close sets `ssh_closed` + toast once; read-only banner; Unix profile and Bash family for SSH
- [ ] 2.20.1–11 OSC-to-UI table incl. unhandled Cwd/ShellIntegration, bounded notification queue, progress bar colors, agent status grouping/nav, Exited vs Closed vs shutdown lifecycle, Output coalescing keeps a trailing Exited
- [ ] 2.21 bell badge (`has_bell && bell_enabled`), cleared on key send / IME commit
- [ ] old `view/*` per-file tests carried over (search 7, scrollbar 7, gutter 7, local_view 3, render 4, completion glue 10, grid 4, key 6, view/tests 3)

### US-0050 panel, spaces, swap (§1, §2.1, §2.2, §2.18, §4)

- [ ] §1 public API: 7 items + `init` + feature; consumers `app/init.rs`, `session-ui/common.rs` compile unchanged
- [ ] §1.6 ten `on_action` handlers on `TerminalPanel`
- [ ] §1.7 no `dump` override; restore = fresh default shell
- [ ] 2.1.1–2 open/from_spec single leaf; spawn failure → empty tree + warn
- [ ] 2.1.3–4 close with siblings removes the panel; last tab resets in place (title "Terminal", override cleared, subs rebuilt, focus)
- [ ] 2.1.5 no confirm dialog
- [ ] 2.1.6–7 close Space; `CloseSpace` guarded by `leaf_count > 1`
- [ ] 2.1.8–9 middle-click title and × close the tab
- [ ] 2.1.10–12 duplicate to new tab / existing Space / split; failures never leak a session
- [ ] 2.1.13–17 rename dialog rules; override wins; live title resolution; path basename trimming; subscription-driven repaint
- [ ] 2.1.18 placeholder numbering, `new_terminal_here`
- [ ] 2.1.19–21 recording dot, active bar, draggable title
- [ ] 2.1.22–24 drop no-ops (occupied, self) and move semantics incl. empty source panel removal
- [ ] 2.1.25–29 zoom both, "+" dropdown makes tabs, focus proxy, republish rules
- [ ] 2.2.1–18 tree model, split/close/collapse, resizable delegation, no swap/traversal, fill/take, set_active guard, stable numbering, whole-pane drop, no drop-split, placeholder content and menu, single-leaf fast path, bordered leaf rendering, no persistence
- [ ] §2.18 status metrics, agent focuser, security policy bridges unchanged
- [ ] §4 tests carried: panel 11, space 15, ops duplicate 2, title tests, completion 15, url 13, theme 7, highlight 7
- [ ] performance sign-off (below) recorded in the packet evidence

## Performance Budget and How It Is Measured

| Requirement | Budget | Measurement |
| --- | --- | --- |
| Idle frame | 0 shape calls, 0 row plans, 0 URL scans, paint only | `element_tests::idle_frame_plans_nothing_but_paints` asserts `FrameStats { rows_planned: 0, shape_calls: 0, url_scans: 0, quads > 0 }` |
| Dirty frame | plans ≤ damaged rows, one `snapshot_into` | `dirty_frame_plans_rows_and_shapes` asserts `snapshot_calls == 1`, `rows_planned <= rows_candidate` |
| DOOM-fire full-screen (`crates/tools` workload, 80×40 blocks) | one snapshot, plans only changed rows, block runs coalesced (quads per row ≈ color runs, not cells), one grid layer | manual run with `--features terminal-diagnostics`; the 5 s log line reports `rows_planned`, `quads`, `layers`, p95/p99 prepaint+paint µs; compared against the old crate's log on the same machine and recorded in US-0050 evidence |
| Steady-state heap allocations | 0 per frame in view code | `element_tests::idle_frame_allocates_nothing` uses a counting global allocator (test-only) around two idle frames |
| Shaped lines | cached across frames | `glyph_cache_hits_across_rows`: second frame `shape_calls == 0`, `glyph_hits > 0` |
| Cell width | whole device pixels | `metrics_snap_cell_to_device_pixels` at scale 1.0, 1.25, 1.5, 2.0 |
| Shape geometry | ≤ 24 quads per shade cell, ≤ 8 per braille cell, ≤ 6 per box cell | `shapes_tests` upper-bound assertions |

## Risks

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Parity-matched symmetric snapping makes 1 px strokes 2 px on even device cell sizes, and `─`/`│` may differ by 1 px when width and height parity differ | box drawing looks heavier at 1× with even cell widths | documented in `shapes.md`; thickness nominal is `round(W/8)` so ≥ 16 px cells are unaffected; revisit as a decision if the owner objects after the sign-off screenshots |
| `force_width` shaping treats a wide glyph as one base and misplaces following glyphs | CJK misalignment | text runs break at wide chars (own run, no force width) |
| `shape_line_by_hash` collision | wrong glyphs for a run | 64-bit FNV over bytes + font key + byte length; accepted by GPUI's own contract |
| alacritty reports `Damage::Full` on every scroll | old crate replanned all rows | hash-verify makes rebuild proportional to real change; hashing 80×40 cells is µs-scale |
| GPUI phase assertions (`insert_hitbox` prepaint-only, `handle_input` paint-only) | debug panics | element tests run under `gpui::test` in debug |
| Path tessellation per frame for rounded corners / powerline / diagonals | CPU cost on prompt rows | paths are built only for those code points; a prompt row has a handful; if profiling shows cost, cache built `Path<Pixels>` per (glyph, cell size, origin) |
| Old `panel/`/`space/` compiling against the new view during US-0049 | one packet with both old and new code | `TerminalView` keeps the four field names and three method names the old modules use |
| Manual UAT is Windows-only | Linux/macOS regressions unobserved | unchanged project limitation (`docs/PROJECT.md`) |
