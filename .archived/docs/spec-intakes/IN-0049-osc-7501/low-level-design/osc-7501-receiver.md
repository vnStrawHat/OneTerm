# Low-Level Design: OSC 7501 receiver

Intake: IN-0049
HLD: [`../high-level-design.md`](../high-level-design.md)
Topic: parser, record table, router wiring, summary event (`oneterm-terminal`)
Date: 2026-10-15

## Concern

Turn forwarded `OSC 7501` sequences into one summary state per session, by the rules of the
Program Status Protocol rev 0.3, with the probe answered in byte order and every malformed or
oversized report dropped whole.

## Design

### Route

`adapter_config` (`crates/terminal/src/handle.rs`): `route(7501, OscRoute::Forward)` and
`large(7501, true)`. The spec allows 4096-byte sequences; the inline bound is 2 KiB.

### Module `crates/terminal/src/program_status.rs` (+ `program_status_tests.rs`)

Pure functions and one table; no gpui, no I/O.

**Parse** `parse(params: &[&[u8]]) -> Option<Osc7501>` where `params[0]` is the number:

1. Exactly one parameter after the number (`;` is not in the value charset). Otherwise drop.
2. Body `?` is `Osc7501::Probe`.
3. Body longer than `MAX_BODY` (4096 minus the 7-byte `ESC ] 7501 ;` and a 2-byte `ST`) is
   dropped.
4. Split the body on `:`. Each pair splits at its first `=`. A pair whose key is not
   `[a-z]{1,16}` or whose value has a byte outside `[A-Za-z0-9_.,+/=-]` is skipped. A
   repeated key: last wins. Unknown keys are ignored.
5. `state` is required and must be `idle|working|done|blocked|error|clear`; otherwise drop.
6. `id`, when present: `/`-separated segments of `[A-Za-z0-9_.+-]{1,32}`, at most 8 segments
   and 128 bytes. A malformed id drops the report (no fallback to the root).
7. `title` (<= 256 encoded, <= 192 decoded) and `msg` (<= 2732 / 2048): standard base64,
   padding optional, then UTF-8. Bad base64, bad UTF-8, an over-long value, or any C0, DEL or
   C1 character drops the report. The decoded text is then thrown away (HLD: validated, never
   shown).
8. `kind`, `progress`, `app` are syntax-checked by step 4 and otherwise ignored: the icon does
   not use them.

Result: `Osc7501::Report { id: Option<String>, state: ReportState }`.

**Record table** `RecordTable`:

- `BTreeMap<String, (ProgramState, u64)>`, key `""` for the root record, value the state and a
  monotonic update tick. Cap `MAX_RECORDS = 64` (the spec's minimum); inserting a new id at
  the cap evicts the smallest tick.
- `apply(report)`: a non-`clear` state replaces the record. `clear` with an id removes that id
  and every key starting with `id + "/"`; a bare `clear` removes everything.
- `prompt_started()`: removes every `working`, `blocked` and `idle` record.
- `summary() -> Option<ProgramState>`: the highest of `Blocked > Error > Working > Done >
  Idle`; `None` when empty.

### Router (`backend/osc_router.rs`)

In the `VtEvent::Osc` arm, for `code == 7501`:

- `truncated` -> drop (a cut report is corrupt, not short), debug log.
- `Probe` -> `self.reply(b"\x1b]7501;?" + terminator)`, in the same pass as every other
  reply, so it precedes a DA1 sent after it.
- `Report` -> `state.lock().program_status.apply(report)`.

In the `ShellMark::PromptStart` arm: `program_status.prompt_started()`.

`drain` ends with: compute `summary()`; if it differs from `last_program_status` (both in the
locked state), store it and push `SessionEvent::ProgramStatus(summary)`. At most one event per
batch, whatever the report rate.

### Types (`oneterm-terminal`, re-exported)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProgramState { Idle, Done, Working, Error, Blocked } // declaration order = priority

pub enum SessionEvent {
    // ...
    /// The summary of this terminal's OSC 7501 records changed; `None` = no record.
    ProgramStatus(Option<ProgramState>),
}
```

## Interfaces

- `pub(crate) fn parse(params: &[&[u8]]) -> Option<Osc7501>`
- `pub(crate) struct RecordTable` with `apply`, `prompt_started`, `summary`
- `pub enum ProgramState`, `SessionEvent::ProgramStatus(Option<ProgramState>)`
- Wire reply: `ESC ] 7501 ; ? ST` with the query's terminator; nothing else is ever sent.

## Edge Cases and Failure Modes

- [ ] `state=working` with a second `;` parameter: dropped.
- [ ] Unknown key, repeated key, malformed pair: ignored / last wins / skipped.
- [ ] Missing or unknown `state`: dropped. Malformed `id` (empty segment, 9 levels, 33-byte
  segment, 129 bytes): dropped.
- [ ] `msg` with bad base64, a control character after decoding, or over 2048 bytes: dropped.
- [ ] 65th distinct id evicts the least recently updated record.
- [ ] `clear` with `id=a` removes `a` and `a/b`, keeps `ab`; bare `clear` empties the table.
- [ ] OSC 133 `A` keeps `done` / `error`, drops the rest.
- [ ] A sequence over 2 KiB and under 4096 bytes is not truncated (route is `large`).
- [ ] 1000 reports in one batch yield at most one `ProgramStatus` event.
- [ ] Probe followed by `CSI c` in one batch: the 7501 reply is written before the DA1 reply.

## Verification

- [ ] `program_status_tests.rs`: a table test per row above for parser and record table.
- [ ] `backend_tests.rs`: probe ordering against DA1; one event per batch; prompt start;
  4096-byte report through `adapter_config`.
- [ ] `handle.rs` route test: 7501 is `Forward` and `large`.
- [ ] E2E: Claude Code >= 2.1.295 in a local ConPTY tab (settles the intake's Q8: whether
  conhost lets OneTerm's probe answer reach the program before its own DA1) and over SSH.
