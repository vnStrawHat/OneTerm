# 2. Tier 1 — Eliminate Per-Cell `shape_line`

> **STATUS: ✅ IMPLEMENTED** — `layout/row.rs` now flushes the text batch at a
> block cell and emits no space-only run. Measured: `shapes`/`runs` 5389 → ~1 per
> frame. Results in [`06-results-and-ceiling.md`](06-results-and-ceiling.md).
>
> The single highest-leverage optimization: stop emitting a separate,
> invisible space-only text run for every block cell. This cuts `shape_line`
> from ~5389/frame to near zero on the DOOM-fire workload.

---

## 2.1. Root cause

In `layout/row.rs`, when a cell is a box-drawing / block glyph, it is pushed into the
separate `box_draws` list **and** a space is appended into the current text batch:

```rust
if is_box_drawing(cell.c)
    && (is_rounded_corner(cell.c) || !box_drawing_rects(cell.c, 16, 16).is_empty())
{
    box_draws.push(BoxDrawCell { point: lp, color: style.color, c: cell.c });
    let mut sp = style;
    sp.len = ' '.len_utf8();
    if let Some(b) = current_batch.as_mut() {
        if b.start.column + b.cell_count as i32 == lp.column && b.can_append(&sp) {
            b.append_char(' ');
        } else {
            let old = current_batch.take().unwrap();
            runs.push(old);
            current_batch = Some(BatchedTextRun::new(lp, ' ', sp));
        }
    } else {
        current_batch = Some(BatchedTextRun::new(lp, ' ', sp));
    }
    continue;
}
```

`can_append` compares `style.color` (among other fields). In a fire gradient the
foreground color differs between adjacent cells, so `can_append` returns `false` for
almost every cell → each space becomes **its own run** → one `shape_line` per cell in
`prepaint.rs`:

```rust
for run in &row.runs {
    let shaped = window.text_system().shape_line(
        SharedString::from(run.text.clone()),
        font_size,
        std::slice::from_ref(&run.style),
        Some(cell_width),
    );
    row.shaped_lines.push(Some(shaped));
    shape_line_count += 1;
}
```

These runs contain only a space. They render **nothing visible**: the cell background is
drawn by the `LayoutRect` list, and the glyph shape is drawn by the box-draw primitive
list. We are paying ~5389 shape calls + ~5389 `String` clones per frame to shape
invisible whitespace.

---

## 2.2. Why the space filler exists (and why it is unnecessary here)

The space was added to preserve **intra-run glyph advance** when real text is
interleaved with block glyphs on the same line: a `ShapedLine` lays glyphs out relative
to the run's start, so skipping a column would shift subsequent glyphs in that run left
by one cell.

However, each run is painted at an **absolute** column origin in `paint.rs`:

```rust
let x = cell_x(run.start.column);
run.paint(shaped, x, y, cw, lh, window, cx);
```

Because every run carries its own `start.column` and is positioned absolutely, a block
glyph can simply **terminate the current run**; the next real-text segment starts a new
run at its own column. No space filler is needed for correctness — it is only a way to
keep more characters inside a single run.

For DOOM-fire (no interleaved text) this means: terminate the batch at each block cell
and never start a space-only run ⇒ **pure-block lines produce zero runs ⇒ zero
`shape_line`**.

---

## 2.3. Proposed change

In the block branch of `layout/row.rs`, flush the current batch (if any) and do **not**
create a space run:

```rust
if is_box_drawing(cell.c) && has_box_geometry(cell.c) {
    box_draws.push(BoxDrawCell { point: lp, color: style.color, c: cell.c });
    // Flush the active text batch so following real text starts a fresh run
    // at its own absolute column. Do NOT emit a space-only run: it would force
    // a shape_line per block cell (invisible whitespace) — the dominant cost
    // on full-screen block workloads (DOOM-fire).
    if let Some(old) = current_batch.take() {
        runs.push(old);
    }
    continue;
}
```

(`has_box_geometry` is the allocation-free probe introduced in
[`03-quads-and-allocations.md`](03-quads-and-allocations.md); until then, keep the
existing `!box_drawing_rects(cell.c, 16, 16).is_empty()` guard.)

### Correctness notes

- **Positioning is preserved** because runs are absolutely positioned by
  `run.start.column`. Splitting a run at a block glyph produces two runs whose start
  columns are correct.
- **Mixed text + block lines** (e.g. a TUI border with a label) will produce slightly
  more runs than before (one extra split per block run boundary), but each is still a
  real, visible run. The extra runs are bounded by the number of block glyphs on the
  line, not by the number of cells.
- **Wide chars / zero-width chars** are unaffected: they are handled before the block
  branch and continue to append to the batch as today.

---

## 2.4. Expected impact

| Metric | Before | After |
|---|---|---|
| `shapes` per frame (DOOM-fire) | ~5389 | ~0 |
| `runs` per frame | ~5389 | ~0 |
| `String` clones per frame | ~5389 | ~0 |

Text shaping is the most expensive per-cell operation in the pipeline, so removing it
for block cells is expected to be the largest single frame-time reduction. On the debug
build it also removes ~5389 unoptimized `String` allocations per frame (see
[`05-debug-vs-release.md`](05-debug-vs-release.md)).

---

## 2.5. Regression checks

After implementing, verify visually that lines mixing text and box-drawing still align:

```bash
# Border with a centered label — text + box-drawing on the same row.
printf '\xe2\x94\x8c\xe2\x94\x80 Title \xe2\x94\x80\xe2\x94\x90\n'
printf '\xe2\x94\x82 body  \xe2\x94\x82\n'
printf '\xe2\x94\x94\xe2\x94\x80\xe2\x94\x80\xe2\x94\x80\xe2\x94\x80\xe2\x94\x80\xe2\x94\x98\n'
```

The label text must stay in its original columns (no left shift), and the frame must
still close flush. Then re-run DOOM-fire and confirm `shapes`/`runs` collapse to ~0.
