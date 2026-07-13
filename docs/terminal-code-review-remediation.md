# Terminal Core and UI Review: Risks and Remediation Plan

**Review date:** 2026-07-13  
**Scope:** `crates/core/src/terminal/` (11 files) and `crates/ui/src/views/terminal/` (62 files). Local and SSH implementations were inspected only where needed to verify the `TerminalSession` contract and lifecycle behavior.  
**Priority order:** Security, Performance, Readability, Maintainability, Simplicity, Reusability.

## 1. Executive summary

The terminal implementation has several good foundations: protocol code is mostly framework-independent, render snapshots are owned so the terminal lock is not held during paint, render damage is distinguished from query reads, OSC 52 clipboard reads default to disabled, and the renderer already has row-level caching and instrumentation.

However, the current design is not safe to extend without first addressing four classes of risk:

1. **Terminal-controlled data crosses trust boundaries without one policy layer.** SSH input is logged with content, OSC 8 targets are opened without scheme validation, bracketed paste can be terminated by pasted data, OSC strings are not capped/sanitized at the application boundary, and remote OSC 52 writes always replace the local clipboard.
2. **Closing a terminal tab does not reliably close its sessions or tasks.** The panel has no removal hook, while detached tasks can continue forever and the event task strongly retains the session. This can leave PTYs/SSH channels and periodic tasks alive after the UI is gone.
3. **The hot path repeatedly performs work proportional to the viewport or scrollback.** Mouse hover and mode queries clone the entire visible grid; active search scans all scrollback synchronously; timestamps are cloned wholesale and maintained with front-draining vectors; URL masks scan every row every frame; contrast correction can run dozens of color conversions per cell.
4. **The row cache is not keyed by all inputs that affect layout.** Theme, font, semantic settings, and dynamic palette changes can leave stale cached colors or shaped text, while selection changes unnecessarily invalidate every row.

These are not isolated micro-optimizations. They come from an overly broad session interface, query APIs that return render-sized data, lifecycle ownership split across panels/views/tasks, and renderer state passed through many independent `Rc<RefCell<_>>` channels.

## 2. Severity and target order

- **P0 — immediate:** security exposure, secret leakage, resource/session leaks, input loss, or visible cache corruption.
- **P1 — next:** UI-thread stalls, protocol correctness, high recurring allocation/CPU cost, crash-only error handling, or architecture that blocks safe fixes.
- **P2 — cleanup:** readability, stale/dead APIs, avoidable duplication, and lower-frequency correctness risks.

Within the same severity, the table follows the requested priority order.

| ID | Severity | Area | Finding | Primary dimensions |
|---|---|---|---|---|
| SEC-01 | P0 | SSH output bridge | User keystrokes and pasted secrets are logged as text | Security |
| SEC-02 | P0 | URL opening | OSC 8 can invoke arbitrary OS URL schemes | Security |
| SEC-03 | P0 | Paste | Pasted text can terminate bracketed-paste mode early | Security |
| SEC-04 | P1 | OSC/clipboard/title | No central size, sanitation, notification-rate, or clipboard-write policy | Security, Maintainability |
| LIFE-01 | P0 | Panel/view/tasks | Removing a tab does not close all sessions; detached tasks outlive views | Performance, Maintainability |
| PERF-01 | P0 | Session queries | Lightweight queries clone the full viewport grid | Performance, Simplicity |
| PERF-02 | P0 | Renderer cache | Cache misses required invalidation inputs and over-invalidates on selection | Performance, Correctness |
| PERF-03 | P1 | Search | Full scrollback search runs synchronously under the terminal lock | Performance |
| PERF-04 | P1 | Timestamps | Full timestamp vector clones and front drains run on the render/output path | Performance |
| PERF-05 | P1 | URL/semantic/contrast | Repeated full-row scans, quadratic lookup, and iterative per-cell color work | Performance |
| PERF-06 | P1 | Frame orchestration | Multiple terminal locks and unchanged theme/default work occur per frame | Performance, Simplicity |
| PERF-07 | P2 | Paint helpers | Gutter text and rounded/rare primitives are recomputed every paint | Performance |
| IO-01 | P0 | Input/events | Void writes plus bounded `try_send` silently drop input/control events | Security, Reliability |
| INPUT-01 | P1 | Mouse | Legacy X10/X11 bytes are encoded incorrectly; modifiers are discarded | Correctness |
| INPUT-02 | P1 | Keyboard | Unknown named keys become text; documented keys/modifiers are unsupported | Correctness, Readability |
| INPUT-03 | P1 | Coordinates/IME | Padding/grid bounds are missing from hit-testing; IME cursor metrics are never wired | Correctness, Maintainability |
| UX-01 | P1 | Output/clear behavior | Output forces scroll-to-bottom and “Clear” runs a shell-specific command | Correctness, Simplicity |
| RENDER-01 | P1 | Primitive glyphs | Unsupported powerline glyphs become blocks and shade glyphs vanish at high DPI | Correctness, Simplicity |
| SEARCH-01 | P2 | Search model | Wide, combining, and wrapped text do not map to highlights correctly | Correctness |
| ARCH-01 | P1 | Core interface | `TerminalSession` mixes render, input, search, IME, lifecycle, SFTP, and state | Maintainability, Reusability |
| ARCH-02 | P1 | Crate boundaries | UI constructs `LocalSession` directly and crashes on spawn failure | Maintainability, Reusability |
| ARCH-03 | P1 | Duplication | Local/SSH adapters and core/UI URL detectors duplicate behavior and drift | Maintainability, Reusability |
| ARCH-04 | P2 | Renderer API | Large argument lists and multiple mutable side channels obscure invariants | Readability, Simplicity |
| ARCH-05 | P2 | Split tree | Temporary `None` root and ignored mutation outcomes make invariants fragile | Maintainability, Simplicity |
| CLEAN-01 | P2 | Dead/stale code | Unused APIs, ignored parameters, stale comments, and duplicate settings UI remain | Readability, Simplicity |

## 3. Detailed findings and required changes

### SEC-01 — Never log terminal input content (P0)

**Evidence**

- `crates/ssh/src/session_terminal.rs:136-142` logs every write, including a lossy text rendering, at trace level.
- `crates/ssh/src/listener.rs:59-68` logs the same bytes again at debug level.

**Risk**

Commands, pasted tokens, passwords entered into prompts, private keys, and OSC 52 replies can be written to application logs. Debug logging is commonly enabled while diagnosing SSH problems, precisely when users are likely to share logs.

**Action**

1. Remove payload formatting from both locations.
2. If diagnostics are necessary, log only byte count, command kind, queue depth, and a generated correlation ID at trace level.
3. Add a repository lint/review rule: terminal input, clipboard content, host passwords, and file contents must never implement ad-hoc `Debug` logging.
4. Add a test logger that sends a sentinel secret and asserts that no captured log record contains it.

### SEC-02 — Put all external target opening behind one URL policy (P0)

**Evidence**

- `crates/ui/src/views/terminal/handlers/mouse.rs:34-44` passes the detected target directly to `cx.open_url`.
- `crates/ui/src/views/terminal/url/detect.rs:36-64` accepts an OSC 8 URI without parsing or validating its scheme.
- Plain-text detection and core detection disagree: UI recognizes only `http`, `https`, `ftp`, and `www`, while `crates/core/src/terminal/url.rs:38-45` uses `linkify`; the core helpers have no production callers.

**Risk**

Remote output controls OSC 8. A custom URI scheme may launch a registered local application or invoke scheme-specific behavior. The displayed link text can differ from the OSC 8 target, so visual inspection is not sufficient.

**Action**

Create a single core `ExternalTargetPolicy` and use it for hover, click, context menu, and any future “open link” action:

- Parse with a real URL parser.
- Allow `https` and `http` by default; make other schemes explicit settings.
- Reject control characters, invalid encodings, credentials in authority unless confirmed, and oversized targets.
- For OSC 8 or display-text/target mismatches, show the normalized destination in a confirmation UI.
- Treat `file`, `ssh`, application-specific, and unknown schemes as denied by default.
- Support the platform shortcut (`Cmd` on macOS) rather than hard-coding Control.

Acceptance tests must include malicious custom schemes, mixed-case schemes, Unicode host names, credentials, display/target mismatch, an oversized URI, and a valid wrapped HTTPS URL.

### SEC-03 — Make bracketed paste marker-safe (P0)

**Evidence**

`crates/core/src/terminal/session.rs:265-282` places unmodified text between `ESC [ 200 ~` and `ESC [ 201 ~`. If the text itself contains `ESC [ 201 ~`, the receiving application exits paste mode and interprets the remainder as keystrokes/commands.

**Action**

1. Move paste encoding into a pure, tested function such as `encode_paste(text, mode, policy) -> Vec<u8>`.
2. In bracketed mode, neutralize every embedded end marker using the behavior adopted by the target terminal compatibility policy; do not rely on newline confirmation alone.
3. Optionally warn before multiline or control-character paste, especially into an active shell prompt.
4. Set a configurable maximum paste size and stream larger pastes without one `format!` allocation.

Tests must cover embedded complete and split terminators, NUL/control characters, multiline text, Unicode, empty text, and large input.

### SEC-04 — Add one bounded OSC trust policy (P1)

**Evidence**

- `crates/core/src/terminal/osc.rs:69-135` accepts and allocates OSC 7/9/133 payloads without application-level limits; `decode_osc52` and `encode_osc52` at `149-160` also expose unlimited allocation helpers.
- Local and SSH listeners store and forward terminal-controlled title, clipboard, cwd, and notification strings (`crates/local/src/listener.rs:113-121,129-177`; `crates/ssh/src/listener.rs:106-140,150-165`).
- `crates/ui/src/views/terminal/view/mod.rs:171-175,305-309` accumulates notifications in a `Vec`, and `render/mod.rs:31-39` creates every queued notification during render.
- OSC 52 reads have a good default-off gate (`view/mod.rs:256-277`), but OSC 52 writes always replace the system clipboard (`view/mod.rs:129-137,289-297`).
- `crates/ui/src/views/terminal/panel/mod.rs:53-76,267-276` uses the terminal title without length, control-character, or bidirectional-control sanitation.
- `parse_cwd_url` (`crates/core/src/terminal/osc.rs:137-147`) is string slicing, not URL parsing; it does not percent-decode and discards host semantics.

**Risk**

A local or remote program can flood notifications, overwrite the clipboard, retain large terminal-controlled strings, create deceptive tab titles, or produce incorrect cwd state. Depending only on upstream parser limits makes the application boundary implicit and fragile.

**Action**

Introduce `TerminalSecurityPolicy` with explicit defaults and enforce it before data reaches persistent state or GPUI:

- Maximum bytes for title, URI, cwd, notification, clipboard set/read reply, and color-query batches.
- Token-bucket notification rate limit and a bounded queue with coalescing.
- Separate default-off policies for remote clipboard read and remote clipboard write; consider session-scoped prompts.
- Title normalization: remove C0/C1 controls and bidi overrides, cap grapheme count, preserve an audit-safe original only if needed.
- Standards-based OSC 7 URL parsing and percent-decoding, with local/remote host semantics documented.
- Bounded color-query queue instead of `Arc<Mutex<Vec<_>>>` with unrestricted `push` (`osc_color.rs:34-50`).

Do not silently truncate security-sensitive targets and then open the truncated value; reject them.

### LIFE-01 — Give sessions and tasks deterministic ownership (P0)

**Evidence**

- `LocalTerminalView::new` detaches the event loop and cursor loop (`crates/ui/src/views/terminal/view/mod.rs:124-214`). The event task captures a strong session entity; the cursor loop never exits even after `WeakEntity::update` fails.
- `TerminalPanel` implements `Panel` but has no `on_removed` cleanup (`panel/mod.rs:254-390`). Tab close buttons call `TabPanel::remove_panel` directly (`313-359`).
- Explicit multi-space close calls `session.close()` only for the returned leaf (`panel/ops.rs:42-65`). `SpaceTree::close` returns no removed view when the last leaf is closed (`space/ops.rs:43-49`).
- The active SFTP/cwd handles are published into global `AppState` (`panel/ops.rs:170-189`) but are not cleared when the panel is removed.

**Risk**

A closed tab can leave a PTY or SSH channel alive. The detached event task retains the session, and every removed view leaves a cursor timer that wakes every 500 ms forever. Global state can retain SSH/SFTP resources after the UI disappears.

**Action**

1. Add a single idempotent `TerminalPanel::shutdown` used by `Panel::on_removed`, last-space close, window shutdown, and error paths.
2. Traverse all terminal leaves, call `close()` exactly once, cancel per-view tasks, clear subscriptions, and clear global active-session handles if they refer to the removed panel.
3. Store GPUI `Task<()>` handles or a cancellation token in `LocalTerminalView`; do not detach infinite loops. Break immediately when the weak entity cannot be upgraded.
4. Make cursor blinking event-driven: run only while focused, visible, alive, and configured to blink.
5. Make the event receiver/session ownership acyclic. Closing the view must close/drop the receiver even if the backend misbehaves.
6. Add fake-session GPUI tests for close button, middle-click, close action, split close, drag source removal, and window shutdown. Assert one close call, no post-removal wakeups, and released `Arc`/entity weak references.

### PERF-01 — Replace render-sized query snapshots with compact state (P0)

**Evidence**

- `TerminalContent::from_query` clones every displayed cell (`crates/core/src/terminal/content.rs:193-237`).
- The trait default can even consume render damage (`session.rs:135-143`), contradicting the method’s stated contract for implementors that forget to override it.
- Full query snapshots are used for a mode bit (`session.rs:253-270`; `handlers/keyboard.rs:235-239`), cursor bounds (`crates/local/src/session_terminal.rs:316-337`; SSH equivalent at `333-354`), URL hover on every pointer move (`handlers/url.rs:27-37`), URL click (`handlers/mouse.rs:34-42`), and Shift+PageUp viewport size (`handlers/keyboard.rs:99-103`).
- `terminal_info()` itself scans every viewport cell to find the last content line (`crates/core/src/terminal/content.rs:36-54`; local backend `session_terminal.rs:65-78`) and is called from both view render and element prepaint.

**Action**

Replace `snapshot_query()` with purpose-sized APIs:

```text
TerminalQueryState { mode, cursor_point, cursor_shape, display_offset,
                     rows, cols, total_lines, alive, color_generation }
RenderFrame { cells, selection, damage, query_state, dynamic_colors }
```

- Produce `RenderFrame` once per prepaint after resize, under one terminal lock.
- Expose a row/logical-line query for link hit-testing rather than the viewport.
- Cache `last_content_line` as output is parsed or compute it only when the gutter is enabled and output changed.
- Remove the dangerous `snapshot_query` default; require a correct implementation if a temporary compatibility method remains.

Acceptance criterion: idle key handling, cursor positioning, scrolling shortcuts, and mouse hover perform zero full-grid clones; one rendered frame performs one content snapshot.

### PERF-02 — Make row-cache keys complete and remove selection relayout (P0)

**Evidence**

- `RowLayoutCache` stores only grid size, display offset, and selection (`layout/types.rs:119-146`).
- Global invalidation checks only those fields (`layout/cache.rs:39-47`). Theme, font/features, font size, semantic profile/mode/styles, dynamic palette, and rendering-policy changes are absent even though `layout_row` bakes colors and font styles into cached artifacts.
- Selection changes force all rows dirty (`cache.rs:42-47,71-88`), yet `selection_set` is ignored by `layout_row` (`layout/row.rs:22-32`) and selection is painted as separate rectangles.
- `build_selection_set` expands every selected visible cell into a `HashSet` (`layout/selection.rs:79-92`) despite having no consumer.

**Risk**

Settings and OSC color changes can show stale colors or stale shaped fonts until unrelated terminal damage occurs. Selection drag unnecessarily rebuilds and reshapes the viewport.

**Action**

1. Add a `RenderStyleKey`/generation containing font identity/features, font size/cell metrics, full effective palette/theme generation, semantic mode/profile/style generation, and primitive policy.
2. Separate geometry/style/shaping invalidation where useful, but prefer a correct single generation before adding complexity.
3. Remove `selection_set`, its parameter chain, and selection from row-layout invalidation; keep selection rectangles in `LayoutState`.
4. Add cache tests with `Partial([])` damage: theme change updates color, font change reshapes, dynamic OSC palette change updates color, semantic toggle updates style, and selection-only change performs no row layout or shape calls.

### PERF-03 — Debounce, cancel, and move search off the UI/terminal lock path (P1)

**Evidence**

- Every input change calls `run_search` synchronously (`search.rs:65-79,95-113`).
- Every output batch refreshes active search synchronously (`view/mod.rs:157-163`).
- Both backends hold the terminal lock while scanning the full history (`crates/local/src/session_terminal.rs:292-296`; SSH `309-313`).
- The core algorithm compares the query at every column (`crates/core/src/terminal/search.rs:62-169`).
- Every frame linearly scans all matches to extract visible ones (`ui/search.rs:189-220`).

**Action**

- Debounce query changes (for example, 100–200 ms) and cancel superseded work.
- Search an immutable text/index snapshot on a background executor; never hold the live terminal lock for the full scan.
- Refresh incrementally for appended lines, or at a capped cadence during continuous output.
- Store matches in line order and binary-search the visible range.
- Preserve the active match by stable content/absolute-line identity rather than vector index.
- Add a 10,000-line benchmark with continuous output and rapid query edits; verify bounded UI latency and cancellation.

### PERF-04 — Replace timestamp cloning and front-draining with a bounded ring (P1)

**Evidence**

- Every render clones all timestamp strings before constructing `TerminalElement` (`render/mod.rs:159-184`), which stores an owned `Vec<String>` (`element/mod.rs:63-67`).
- At full scrollback, `line_times.drain(0..drop)` shifts the remaining vector (`view/mod.rs:408-414`).
- Timestamp maintenance runs on output and render even when the gutter is disabled (`view/mod.rs:157-163`; `render/mod.rs:114-116`).
- Gutter entries are shaped again in every paint (`element/paint.rs:190-242`).

**Action**

Use a bounded ring keyed by absolute line number (`VecDeque`, fixed ring, or sparse run structure), store compact timestamps, format only visible rows, and pass a borrowed/shared visible slice to the element. Skip all timestamp work when the gutter is disabled. Cache shaped gutter lines until their text/font/style changes.

Acceptance criterion: after the ring reaches scrollback capacity, appending one line is amortized O(1), and an idle frame does not clone scrollback-sized data.

### PERF-05 — Make URL, semantic, and contrast work proportional to dirty visible data (P1)

**Evidence**

- `url_masks_wrapped` allocates/scans masks for every line on every cache update (`layout/cache.rs:94-97`; `url/mask.rs:105-287`), even when only one row is dirty.
- URL hover then independently clones a snapshot and runs a second detector; core has a third implementation.
- Semantic flattening performs a linear `find` through the row for every character (`layout/cache.rs:186-200`), making wide-character classification O(columns²).
- `ensure_minimum_contrast` can perform up to 40 iterations in two directions (`theme/contrast.rs:27-70`) and is called per ordinary cell during row layout (`cell/color.rs:34-64`).

**Action**

- Consolidate URL detection into one logical-line scanner returning normalized targets and column spans. Cache results per line hash and invalidate only changed/wrap-adjacent rows.
- Carry the wide flag in the existing character-to-column map; remove repeated `find` calls.
- Memoize resolved `(foreground, background, minimum-contrast, policy)` combinations. Precompute ANSI/theme combinations and use bounded binary search or direct color-space math for uncommon true combinations.
- Add allocation/CPU benchmarks for plain logs, URL-heavy logs, semantic highlighting, ANSI color matrices, and wide Unicode text.

### PERF-06 — Build and read frame state once (P1)

**Evidence**

`LocalTerminalView::render` builds/clones settings, font features, theme, and overlays and performs several session reads (`render/mod.rs:41-150`). Prepaint then measures, calls `terminal_info`, resizes, and snapshots again (`element/prepaint.rs:53-167`). Default colors are written to backend state every frame (`render/mod.rs:120-134`) even when unchanged.

**Action**

- Move immutable/effective render configuration into a cached `TerminalRenderConfig` updated by settings/theme observers.
- Push default colors only when the effective theme generation changes.
- After layout measurement and conditional resize, acquire one `RenderFrame` and derive scrollbar, cursor, gutter, dynamic colors, and row cache updates from it.
- Persist `SemanticOverlay` instead of recreating it each render; this is also required before row-role support can work.
- Change periodic frame statistics from unconditional info logs (`element/paint.rs:164-185`) to trace or an opt-in diagnostics feature.

### PERF-07 — Cache expensive primitive geometry (P2)

Rounded corners run 4×4 supersampling and allocate geometry per cell per paint (`box_drawing/rounded.rs:61-100`; `element/paint.rs:124-140`). Rare non-block primitives also create a transient vector despite an “into” API (`box_drawing/drawing.rs:21-45`). Cache geometry by `(glyph, device cell width, device line height)` and append directly into reusable buffers.

### IO-01 — Make writes/control events reliable and observable (P0)

**Evidence**

- `TerminalSession::write`, resize, and close return no result (`core/session.rs:173-188,232-240`).
- SSH sends writes, resize, and close into a bounded 64-item channel with `try_send`; failures only log (`crates/ssh/src/listener.rs:59-82`; channel creation in `crates/ssh/src/session.rs:106-107`).
- Event forwarding uses bounded channels and drops any event when full, although the comment only justifies dropping coalescible output (`crates/local/src/listener.rs:91-96`; SSH `85-89`). Exit, close, clipboard, title, and security-relevant requests can therefore be lost.

**Action**

- Return a typed `Result` or enqueue receipt from input, resize, and close operations.
- Use an ordered writer task with explicit backpressure. Never silently drop user input. Coalesce resize; prioritize close; define a maximum paste size/chunk policy.
- Separate event classes: coalesced/watch state for Output/title/progress, reliable channel for lifecycle and permission requests, and bounded/rate-limited notifications.
- Surface terminal transport failure once in the UI and mark the session closed.
- Add saturation tests proving ordered input delivery, latest-resize delivery, reliable close/exit, and bounded memory.

### INPUT-01 — Correct mouse protocol bytes and preserve modifiers (P1)

**Evidence**

- Legacy encoding adds 32 to the button but not to coordinates, then creates a Rust `String` from byte values (`core/mouse_encode.rs:67-83`). Existing tests codify `0x01` for row/column zero (`186-208`), but classic X10/X11 requires encoded coordinate bytes offset by 32.
- UI/backend signatures discard event modifiers; both backends always pass `MouseModifiers::default()` (for example, local `session_terminal.rs:158-249`).

**Action**

Return raw `Vec<u8>`, encode classic bytes exactly, cap at that protocol’s representable coordinate range, and retain SGR’s larger numeric range. Thread Shift/Alt/Ctrl through the session mouse API. Add conformance vectors for press/release/motion/wheel, modifiers, boundaries, and coordinates above ASCII 127. The row-zero/column-zero classic expectation should be `0x21`, not `0x01`.

### INPUT-02 — Use an explicit keyboard mapping table (P1)

**Evidence**

- `NamedKey` lacks function keys (`core/key_encode.rs:18-35`), while `send_keystroke` documentation promises `F1` (`session.rs:253-256,332-379`).
- Alt modifiers are ignored for arrows/Home/End/Page keys in several encoder branches (`key_encode.rs:100-136`).
- Unknown GPUI named keys fall back to `keystroke.key` and are sent as character text (`ui/view/key.rs:14-40`), so a key name such as `f1` can become literal input.
- UI shortcuts hard-code Control rather than platform modifiers and copy/paste branches return without stopping propagation (`handlers/keyboard.rs:34-97,168-199`).

**Action**

Create a table-driven mapping with explicit `Unsupported`; never convert a named key label to text unless `key_char` is present. Add F1–F24 as required, complete modifier forms under a documented xterm/CSI-u compatibility policy, handle platform shortcuts, and stop propagation for every consumed shortcut. Test mapping separately from encoding.

### INPUT-03 — Make the UI own pixel/grid/IME geometry (P1)

**Evidence**

- `GridMetrics` stores terminal bounds, cell size, and combined gutter/left padding but not grid origin, padding top, or rows/columns (`layout/types.rs:5-13`; populated at `element/prepaint.rs:233-249`).
- `pixel_to_grid` subtracts the left gutter but not top padding and does not reject right/bottom padding (`view/grid.rs:9-23`).
- IME bounds add element origin to backend cursor coordinates without gutter/padding (`ime.rs:84-100`).
- Backend cursor bounds depend on `set_cell_size`, but production code never calls either backend’s method; only tests do. The fields start at zero, so cursor bounds normally return `None` (`crates/local/src/session.rs:127-135`; `session_terminal.rs:316-323`).
- Marked/preedit text is stored but never painted, and `accepts_text_input` always returns true (`ime.rs:35-80,112-114`).

**Action**

Store exact `grid_bounds`, rows, and columns in one UI-owned metrics snapshot. Hit-testing should return `None` outside that rectangle and preserve fractional cell side for selection. Derive the IME cursor rectangle from the compact cursor query plus UI metrics; remove pixel metrics and `CursorBounds` from backend sessions. Paint marked text at the cursor, position by terminal cell width rather than UTF-16 count, and disable input explicitly in unsupported alt-screen states. Add padding, gutter, high-DPI, wide-character, and composition tests.

### UX-01 — Separate terminal-view actions from shell commands (P1)

**Evidence**

- Every Output event calls `scroll_to_bottom` before checking whether the user was reading history (`view/mod.rs:139-163`).
- The core contract describes `clear` as clearing screen and scrollback (`core/session.rs:203-211`), but both backends send the literal shell command `clear\r` (local `session_terminal.rs:286-290`; SSH `303-307`). The default Windows `cmd.exe` shell uses `cls`, and aliases/functions can change command behavior.

**Action**

Track an explicit follow-output state: remain at bottom only if the user was already following output, and resume follow on End, explicit scroll-to-bottom, or a dedicated affordance. Preserve history position while new output arrives. Accumulate fractional trackpad deltas instead of rounding every small event to a line.

Define separate, accurately named actions for emulator `clear_scrollback`/`clear_selection` and shell input such as Ctrl+L. A UI clear action should mutate terminal model state through a backend-neutral method and must not insert an unexpected command into shell history. Add Windows cmd, PowerShell, POSIX shell, alt-screen, and scrolled-history tests.

### RENDER-01 — Prefer correct font fallback over knowingly wrong primitives (P1)

**Evidence**

- Unimplemented powerline characters are painted as full blocks (`box_drawing/powerline.rs:63-65`), so the font fallback is never used.
- Shade geometry returns empty when device-cell area exceeds 1024 (`box_drawing/shade.rs:3-6`), but the layout probe uses a fixed 16×16 geometry and classifies the character as primitive (`layout/row.rs:121-163`), making it disappear at high DPI/large font sizes.
- Cursor block/hollow rendering is a single filled quad and does not preserve the glyph under the cursor (`element/paint.rs:245-287`).

**Action**

Only claim primitive support for glyphs with correct geometry at the actual metrics. Unsupported powerline/shade cases must fall back to font rendering. Cache supported geometry and add high-DPI golden/invariant tests. Define cursor text inversion and hollow-block behavior explicitly.

### SEARCH-01 — Define search in grapheme-to-grid coordinates (P2)

The core search deliberately inserts NUL for wide spacers and does not cross rows (`core/search.rs:62-115`). A wide glyph match ends at `start + 1`, so its highlight covers one of two cells; combining characters stored as zero-width extras are not searchable; a logical line wrapped by the terminal cannot match across display rows. Build logical-line text with a byte/grapheme-to-grid span map, reuse it for search and URL detection, and test wide, combining, emoji/ZWJ, wrapped, and reflowed content.

### ARCH-01 — Split the session contract by responsibility (P1)

`TerminalSession` spans rendering, querying, writes, scrolling, mouse, clipboard, search, IME, lifecycle, shell integration, network stats, SFTP, and cwd (`core/session.rs:120-329`). Defaults can silently hide missing capabilities, and UI-only concerns such as pixel cursor bounds have leaked into core.

Refactor toward a small aggregate of explicit capabilities:

- `TerminalRenderer`: render frame and compact query state.
- `TerminalInput`: ordered input/paste/mouse/resize with typed errors.
- `TerminalLifecycle`: events, alive state, close/cancellation.
- Optional `SearchProvider`, `SftpProvider`, and `CwdSource` capability objects.

Avoid no-op defaults for correctness-critical behavior. Capability absence should be represented in types, not an empty vector or silent no-op.

### ARCH-02 — Restore the crate boundary and recover from spawn errors (P1)

`crates/ui/src/views/terminal/panel/mod.rs:27-29,153-168` imports `oneterm_local`, constructs a local backend, and calls `expect`. This violates the project rule that UI must not import local/SSH protocol crates and turns a missing shell/PTY error into an application crash.

Move session creation to the app composition layer through an injected `SessionFactory` returning `Result<Box<dyn TerminalSession>>`. Render a recoverable error/empty space with retry and diagnostics. The UI should know only the core capability interfaces.

### ARCH-03 — Remove duplicated backend and URL logic (P1)

Local and SSH `TerminalSession` implementations duplicate most render, selection, scroll, search, IME, and mouse code (`crates/local/src/session_terminal.rs` and `crates/ssh/src/session_terminal.rs`). The duplication has already drifted in comments and helper placement. Core and UI also have incompatible URL detectors.

Extract a shared terminal-model adapter around `Term<EventListener>` for pure grid operations. Keep only transport-specific write/resize/close behavior in local/SSH. Consolidate logical-line extraction, URL spans, and search span mapping in core. Do this after the compact frame/query contract is defined so the shared layer does not preserve the current oversized API.

### ARCH-04 — Bundle renderer inputs and mutable cache state (P2)

`TerminalElement::new` has 24 parameters (`element/mod.rs:81-108`), and prepaint/paint helpers suppress `too_many_arguments` (`element/prepaint.rs:25-52`; `element/paint.rs:19-35`). Several parameters are explicitly ignored. View state is bridged through independent `Rc<RefCell<_>>` values for metrics, row cache, gutter, and grid size.

Introduce cohesive values such as `TerminalRenderConfig`, `TerminalInteractionState`, `TerminalRenderCache`, and `TerminalFrame`. Keep one clearly owned cache bridge if the custom-element lifecycle requires interior mutability. Remove ignored parameters rather than renaming them with underscores.

### ARCH-05 — Make split-tree mutations total and transactional (P2)

`SpaceTree` temporarily removes its root into `Option` and relies on `expect` to restore it (`space/mod.rs:68-120`). A panic during recursive transformation leaves the tree invalid. `fill_empty` and `split` do not return whether they succeeded (`space/ops.rs:15-79`), so drag/drop cannot roll back if the target changed.

Use mutation APIs that preserve a valid root throughout, or a private guard that restores on unwind. Return typed outcomes for split/fill/take/close and make drag moves transactional. Add pure tree tests for missing IDs, last leaf, nested collapse, rollback, active-leaf preservation, and ID overflow handling.

### CLEAN-01 — Delete slop instead of documenting around it (P2)

After the preceding changes:

- Remove core URL helpers if replaced, or make them the sole implementation.
- Remove unused `decode_osc52` if decoding remains entirely upstream.
- Remove `selection_set`, ignored hover/control layout parameters, unused paint session/counter parameters, and the no-op `let _ = self` in `render/theme_apply.rs:31-39`.
- Either integrate `gpui_component::Scrollbar` with `TerminalScrollHandle` or remove the unused trait implementation; the current custom scrollbar duplicates handle math and stores a “drag start” that is not used as a grab offset.
- Retire or migrate the shell-only `TerminalSettingsPanel`; its comments say six presets while returning seven, and it duplicates the main settings surface (`settings_panel.rs:1-5,39-61`).
- Rename `LocalTerminalView` to `TerminalView`; it renders SSH too.
- Replace numeric `NamedColor` discriminant matching (`core/palette.rs:106-123`) with explicit variants or an upstream conversion API.
- Split `box_drawing/drawing.rs` (502 lines) by glyph family and replace self-referential geometry tests with protocol/visual invariants.
- Update comments that claim behavior not implemented, including F1 parsing, grow-only timestamp timing, semantic row roles, and “allocation-free” cold paths.

## 4. Target architecture

The desired frame and ownership flow is:

1. The **app layer** creates a backend through `SessionFactory` and gives the UI core capability objects.
2. `TerminalPanel` exclusively owns terminal views and has one idempotent shutdown path.
3. `TerminalView` owns cancellable event/blink/search tasks. No infinite task is detached.
4. Settings/theme observers build an immutable, generation-keyed `TerminalRenderConfig`.
5. Prepaint measures bounds, sends a coalesced resize if needed, then requests exactly one `RenderFrame`.
6. The renderer updates a cache keyed by content damage plus render-config generation. Selection/cursor overlays do not invalidate text shaping.
7. Logical-line extraction is shared by URL detection and search and carries exact grid spans.
8. All terminal-controlled side effects pass through `TerminalSecurityPolicy` before touching the OS or persistent UI state.
9. Input is ordered and fallible; lifecycle/control events are reliable; only explicitly coalescible telemetry may be dropped.

## 5. Implementation sequence

### Phase 0 — Lock down behavior with tests and counters

- Add fake session/transport types and GPUI lifecycle tests.
- Add security vectors for logs, paste, URL policy, OSC caps, title sanitation, and clipboard permissions.
- Preserve current render counters, but expose them only in test/diagnostic mode.
- Record baseline CPU, allocations, lock duration, row-layout count, shape count, and queue saturation behavior.

### Phase 1 — Security and lifecycle hotfixes

1. Remove SSH payload logs.
2. Add URL allowlist/confirmation and bracketed-paste sanitization.
3. Add bounded OSC/title/notification/clipboard policy.
4. Implement panel/view shutdown and task cancellation.
5. Make write/close and lifecycle event delivery reliable.
6. Replace local spawn `expect` with a recoverable factory error.

Do not wait for the renderer refactor before shipping this phase.

### Phase 2 — Compact core contracts and correct input

- Introduce `RenderFrame` and `TerminalQueryState`; remove full-grid query call sites and the unsafe default.
- Split input/lifecycle/capability interfaces and add typed errors.
- Fix raw mouse bytes/modifiers, keyboard mapping, and platform shortcuts.
- Move cursor/IME geometry to UI metrics.

### Phase 3 — Correct cache and remove scrollback-sized UI work

- Add full render-config generation invalidation.
- Remove selection relayout and `selection_set`.
- Replace timestamps with a visible, bounded ring view.
- Persist render config/semantic overlay and push default colors on generation changes only.

### Phase 4 — Move expensive analysis off the frame path

- Consolidate and cache logical-line/URL analysis.
- Make semantic flattening linear.
- Cache contrast results and primitive geometry.
- Debounce/cancel/background search and incrementally update on output.
- Cache gutter shaping.

### Phase 5 — Simplify and deduplicate

- Extract the shared local/SSH terminal-model adapter.
- Bundle renderer arguments/cache state.
- Make split-tree operations transactional and tested.
- Remove obsolete APIs, panels, comments, suppressions, and duplicate detectors.

## 6. Verification matrix and completion criteria

### Security

- Logs contain no input, paste, clipboard, password, or private content under trace/debug.
- Embedded bracketed-paste end markers cannot escape paste mode.
- Only policy-approved schemes open without confirmation; OSC 8 display/target mismatch is visible.
- Terminal-controlled strings and queues have explicit byte/item/rate limits.
- Clipboard read and write permissions are separate and default-safe for SSH.

### Lifecycle and reliability

- Every panel removal path closes each contained session exactly once.
- No timer, search, or event task wakes after its view is removed.
- Global active SFTP/cwd state cannot retain a removed session.
- Saturated transport queues do not reorder/drop user input, close, or exit events.
- Spawn and transport errors are visible and recoverable, not panics.

### Correctness

- Mouse encoder passes classic and SGR conformance vectors.
- Unsupported named keys send nothing; supported function/modifier keys match the selected protocol.
- Hit-testing and IME positioning pass nonzero padding/gutter/high-DPI tests.
- Theme/font/semantic/dynamic-color changes update an undamaged cached row immediately.
- URL/search highlights map correctly across wide, combining, and wrapped text.
- Unsupported primitive glyphs render through the font instead of as blocks or blanks.

### Performance

- An idle terminal performs no full-grid query clones and no unchanged row relayout.
- One frame takes one render snapshot after at most one coalesced resize.
- Selection drag does not reshape terminal rows.
- URL analysis runs only for changed logical lines; hover reads cached spans/current line only.
- Appending after full scrollback is amortized O(1) for timestamps.
- Search is cancellable and does not hold the live terminal lock while scanning history.
- Hidden/non-blinking terminals have no cursor timer wakeups.

### Quality gate

For each implementation phase, run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace
```

The remediation is complete only when the behavioral tests and performance assertions above are automated; passing formatting and compilation alone is insufficient.
