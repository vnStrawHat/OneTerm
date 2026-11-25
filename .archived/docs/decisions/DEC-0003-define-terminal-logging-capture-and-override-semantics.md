# DEC-0003 Define terminal logging capture and override semantics

Date: 2026-08-19

## Status

accepted

## Context

Terminal transport reads can split output arbitrarily, terminal streams contain ANSI/OSC/control payloads, and a saved SSH-session boolean would either make the global SSH setting ineffective or be unable to force logging off. Overwrite behavior at a filename collision also determines whether existing user data is lost.

## Decision

Future terminal logging work must:

- use a terminal parser to retain only printable output and commit one timestamped record per LF- or CR-terminated message;
- use a saved SSH tri-state (`inherit`, `on`, `off`), with `on` and `off` overriding the global SSH setting;
- interpret Overwrite as truncating a colliding file once when Start opens it, then continuing to write all records to that open file.

## Alternatives

- [x] Selected approach described above.
- [ ] Raw read chunks: rejected because transport boundaries are arbitrary and expose escape/control bytes.
- [ ] Per-character records: rejected because output is unreadable and excessively large.
- [ ] SSH boolean: rejected because it cannot both inherit and force either outcome.
- [ ] Unique suffix under Overwrite: rejected because it contradicts the selected write mode.

## Consequences

- [ ] Benefit to confirm: local and SSH logs have stable, readable semantics independent of transport chunking.
- [ ] Tradeoff to address: carriage-return progress output creates multiple chronological records rather than reconstructing an in-place screen.
- [ ] Tradeoff to address: Overwrite can destroy an older same-name file; timestamps make collision uncommon, and this is explicit user-selected behavior.
