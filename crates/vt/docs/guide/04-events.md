# 4. Every event, and what you owe it

One `feed` fills one [`EventBatch`] with [`VtEvent`] values in byte order. An
event is never a callback: nothing of yours runs inside the engine, so you may
hold your lock across `feed` and drain afterwards.

An event does not own its payload. `Title`, `Cwd`, `Osc` and the rest carry
spans into the batch's single arena, resolved with `batch.str`, `batch.bytes`
and `batch.params`. The borrow checker therefore refuses to let an event outlive
its batch, which is the point: the next `feed` clears that arena, and a payload
that survived would silently read the next chunk's bytes. `VtEvent` is
deliberately not `Clone` for the same reason. Copy out what you need to keep.

The enum is `#[non_exhaustive]`, so a `match` on it needs a wildcard arm and a
new variant is a patch release rather than a break.

## The full set

| Variant | Fires when | What you must do |
| --- | --- | --- |
| [`Repaint`] | at most once per batch, last, when the chunk dispatched anything | schedule a frame; it is a hint, not damage |
| [`Title`] | `OSC 0` or `OSC 2` with text | set your window title, after sanitising it yourself |
| [`TitleReset`] | `OSC 0` / `OSC 2` with an empty string | go back to your default title |
| [`Bell`] | a `BEL` byte outside a string sequence | ring, flash, mark the tab, or ignore |
| [`ClipboardStore`] | `OSC 52` store, already base64-decoded and UTF-8 checked | decide whether a program may write that selection, then write it |
| [`ClipboardLoad`] | `OSC 52` with a `?` payload | the engine has no clipboard: format and write the reply yourself, or refuse |
| [`Reply`] | the terminal owes the program bytes: `DA1`, `DA2`, `DSR`, `DECRQM`, `XTVERSION`, `DECRPSS` | write the bytes to the process input verbatim, in arrival order |
| [`ColorQuery`] | `OSC 4` / `10` / `11` / `12` with a `?` value | you own the palette, so you format the answer |
| [`ScreenCleared`] | `ED 2`, `ED 3` or `RIS`; never `ED 0` or `ED 1` | drop caches keyed to screen content, such as a rendered overlay |
| [`Osc`] | an OSC number routed `Forward` or `BuiltinAndForward` | your own protocol; chapter 5 |
| [`RowsScrolled`] | content moved between row ids | shift a cache keyed by row id instead of rebuilding it |
| [`RowsTrimmed`] | history was trimmed | rows older than `oldest` no longer exist; drop anything keyed to them |
| [`GraphicReleased`] | the last cell referencing an image is gone | drop the texture you uploaded for that id |
| [`Cwd`] | `OSC 7`, the shell reporting its directory | treat it as untrusted data; chapter 5 |
| [`IconName`] | `OSC 1` | set an icon name, or ignore it |
| [`Notification`] | `OSC 9` | your policy decides whether to show it, how big it may be and how often |
| [`Pointer`] | `OSC 22`, a pointer shape by name, verbatim | map the name to a cursor of yours, or ignore it |
| [`Progress`] | `OSC 9 ; 4`, ConEmu taskbar progress | drive a taskbar or progress indicator |
| [`ShellMark`] | `OSC 133`, a shell-integration boundary | track prompts, command blocks and exit codes |
| [`CursorStyleChanged`] | `OSC 50`, or any `CSI SP q` that changes the shape | read the new shape from `Terminal::cursor_style` |

## Answering a query

Four of those are questions. Three of them the engine cannot answer, because the
answer is something only you hold.

**`Reply` is not a question**, it is the answer to one. The engine has already
composed the bytes; write them to the process input exactly as given and in the
order they arrive. That includes the `XTVERSION` and `DA2` answers, which name
your product rather than this engine when you set `Config::product_name`:

```rust
use std::time::Instant;
use oneterm_vt::{Config, EventBatch, Size, Terminal, VtEvent};

let mut term = Terminal::new(
    Size { rows: 24, cols: 80 },
    Config {
        product_name: Some("MyTerm(1.4.0)".into()),
        ..Config::default()
    },
);
let mut batch = EventBatch::new();

// XTVERSION: `CSI > 0 q`.
term.feed(b"\x1b[>0q", &mut batch, Instant::now());
let reply = batch
    .events()
    .iter()
    .find_map(|event| match event {
        VtEvent::Reply(span) => Some(batch.bytes(*span)),
        _ => None,
    })
    .expect("XTVERSION is answered");
// DCS > | MyTerm(1.4.0) ST -- write it to the child's stdin, unchanged.
assert_eq!(reply, b"\x1bP>|MyTerm(1.4.0)\x1b\\");
```

Programs such as tmux and vim key capability detection off that string, so set
the name to your product. It is sanitised once when the terminal is built --
every C0, `DEL` and C1 byte dropped so it cannot end the reply's own string
early, then cut to 64 bytes -- and `Terminal::config` reports the sanitised
value, not what you passed.

**`ColorQuery` is a question the engine cannot answer.** It holds only the
override layer a program wrote with `OSC 4 / 10 / 11 / 12`; the theme underneath
is yours. `ColorKey::query_prefix` gives the number the answer has to echo back,
and the event carries the terminator the question used, because the answer must
end the way the question did:

```rust
use std::time::Instant;
use oneterm_vt::{ColorKey, Config, EventBatch, Rgb, Size, StringTerm, Terminal, VtEvent};

fn answer(key: ColorKey, terminator: StringTerm, color: Rgb) -> Vec<u8> {
    let end = match terminator {
        StringTerm::Bel => "\x07",
        StringTerm::St => "\x1b\\",
    };
    // xterm's form: each channel doubled to 16 bits.
    format!(
        "\x1b]{};rgb:{:02x}{:02x}/{:02x}{:02x}/{:02x}{:02x}{end}",
        key.query_prefix(),
        color.r, color.r, color.g, color.g, color.b, color.b
    )
    .into_bytes()
}

let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
let mut batch = EventBatch::new();
// "What is the default background?", terminated by BEL.
term.feed(b"\x1b]11;?\x07", &mut batch, Instant::now());

for event in batch.iter() {
    if let VtEvent::ColorQuery { key, terminator } = event {
        // Your palette answers; `Terminal::color` is only the override layer.
        let color = term.color(*key).unwrap_or(Rgb { r: 0x1e, g: 0x1e, b: 0x1e });
        assert_eq!(answer(*key, *terminator, color), b"\x1b]11;rgb:1e1e/1e1e/1e1e\x07");
    }
}
```

**`ClipboardLoad` is a question about a clipboard the engine does not have.**
Answer it, or do not -- refusing is a legitimate policy, and the usual one for a
remote session. The reply, if you send one, is `OSC 52 ; <selection> ; <base64>`
terminated the way the question was. `ClipboardStore` is the write direction and
arrives already base64-decoded and validated as UTF-8; a payload that was
neither is dropped and counted in `FeedStats::malformed_sequences` instead of
reaching you.

## Events about row identity

A `RowId` names the same content for as long as that content is live, which is
what makes it safe to key a cache on. Two events maintain that promise:
[`RowsScrolled`] says content moved by a delta within a range, so a cache can
shift instead of rebuilding; [`RowsTrimmed`] says everything older than `oldest`
is gone. Both also reach a renderer indirectly through `SnapshotUpdate`, so a
consumer that only draws does not have to handle them. A consumer that keeps its
own index -- a search overlay, a link map, a block-level command history -- does.

[`GraphicReleased`] is the same idea for images: the last cell referencing that
id is gone, so the texture you uploaded can be dropped. Chapter 8 is the whole
image lifecycle.

## What the engine will never do

It will not open a URL, write a clipboard, raise a notification, set a title,
resolve a path, touch the filesystem, or check a host name. Every one of those
arrives as an event and stops there. That is not an omission: a terminal's
policy belongs to the program that owns the window, and a remote shell that can
reach any of them through an escape sequence is the bug this design refuses to
have.

[`EventBatch`]: crate::EventBatch
[`VtEvent`]: crate::VtEvent
[`Repaint`]: crate::VtEvent::Repaint
[`Title`]: crate::VtEvent::Title
[`TitleReset`]: crate::VtEvent::TitleReset
[`Bell`]: crate::VtEvent::Bell
[`ClipboardStore`]: crate::VtEvent::ClipboardStore
[`ClipboardLoad`]: crate::VtEvent::ClipboardLoad
[`Reply`]: crate::VtEvent::Reply
[`ColorQuery`]: crate::VtEvent::ColorQuery
[`ScreenCleared`]: crate::VtEvent::ScreenCleared
[`Osc`]: crate::VtEvent::Osc
[`RowsScrolled`]: crate::VtEvent::RowsScrolled
[`RowsTrimmed`]: crate::VtEvent::RowsTrimmed
[`GraphicReleased`]: crate::VtEvent::GraphicReleased
[`Cwd`]: crate::VtEvent::Cwd
[`IconName`]: crate::VtEvent::IconName
[`Notification`]: crate::VtEvent::Notification
[`Pointer`]: crate::VtEvent::Pointer
[`Progress`]: crate::VtEvent::Progress
[`ShellMark`]: crate::VtEvent::ShellMark
[`CursorStyleChanged`]: crate::VtEvent::CursorStyleChanged
