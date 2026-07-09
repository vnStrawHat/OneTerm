# 4. Tier 3 — Notify Cadence & Channel Backpressure

> **STATUS: ✅ 1 ms timer REMOVED; backpressure NOT needed.** The fixed 1 ms delay
> in the `Output` handler is gone. Channel backpressure turned out to be unnecessary:
> the pump `try_send`s into a 4096-slot channel and never blocks, and the render is
> **not** the fps limiter (the pump is — see
> [`06-results-and-ceiling.md`](06-results-and-ceiling.md) §6.3).
>
> The `Output` event handler adds a fixed 1 ms delay per batch, and the
> `SessionEvent` channel has no backpressure. Both matter more once the
> per-frame cost is reduced by Tier 1/2.

---

## 4.1. Fixed 1 ms delay per `Output` batch

In `view/mod.rs`, the subscribe task handles `SessionEvent::Output` like this:

```rust
SessionEvent::Output => {
    let s = session_for_spawn.clone();
    Self::drain_coalesced_events(&rx, &this, cx);
    cx.background_executor()
        .timer(Duration::from_millis(1))   // ← fixed 1 ms sleep every batch
        .await;
    Self::drain_coalesced_events(&rx, &this, cx);
    let _ = this.update(cx, |view, cx| {
        cx.notify();
        s.read(cx).scroll_to_bottom();
        let info = s.read(cx).terminal_info();
        view.update_line_times(&info);
        view.refresh_search(cx);
    });
}
```

The minimum frame period is therefore `1 ms + render time`. At a 6.25 ms budget
(160 fps) the 1 ms is ~16% of the frame. Once Tier 1/2 shrink render time, this fixed
cost dominates a larger fraction.

The delay's intent is to **coalesce** bursts of output into a single render. But
`drain_coalesced_events` already drains the channel via `try_recv()`, so the coalescing
happens without the sleep — the timer mostly adds latency.

### Options (in order of preference)

1. **Remove the timer entirely** and rely on `drain_coalesced_events` + GPUI's own
   frame scheduling to coalesce. Simplest; re-measure fps.
2. **Coalesce on the frame boundary** instead of a wall-clock sleep: request a redraw
   and let GPUI merge multiple `notify()`s into one paint per vsync/frame, rather than
   pacing with `timer(1ms)`.
3. If some throttle is still desired to avoid re-rendering faster than the display can
   present, gate on `window.request_animation_frame`-style scheduling rather than a
   fixed sleep.

> Note the per-`Output` work also calls `scroll_to_bottom`, `terminal_info`,
> `update_line_times`, and `refresh_search`. `refresh_search` in particular can be
> non-trivial; consider skipping it when there is no active search query.

---

## 4.2. No backpressure on the event channel

`session.subscribe()` returns an `async-channel` receiver with no drop-oldest policy. A
continuous producer (DOOM-fire) enqueues `Output` events faster than a slow consumer can
drain them. With a fast consumer this is fine (coalescing keeps it bounded per frame),
but with a slow consumer (debug build) the backlog grows and the consumer never catches
up — see [`05-debug-vs-release.md`](05-debug-vs-release.md).

### Options

- **Coalesce at the producer.** Since consecutive `Output` events are equivalent (each
  just means "snapshot changed"), the producer could avoid enqueuing another `Output`
  while one is already pending (e.g. an `AtomicBool` "dirty" flag reset when the
  consumer drains). This makes the queue depth for `Output` at most 1.
- **Bounded channel with drop-oldest for `Output`.** Keep control events (Title,
  Clipboard, Bell, Progress) reliable, but allow `Output` to collapse. A single "dirty"
  signal is sufficient for rendering.

Either approach guarantees the render loop cannot be driven faster than it can consume,
which directly prevents the runaway backlog described in file 5.

---

## 4.3. Expected impact

| Change | Effect |
|---|---|
| Remove/replace the 1 ms timer | Removes a fixed ~1 ms/frame; larger relative gain after Tier 1/2. |
| `Output` coalescing / backpressure | Bounds the render backlog; prevents the debug hang from compounding; smooths frame pacing. |
| Skip `refresh_search` with no query | Removes avoidable per-frame work. |
