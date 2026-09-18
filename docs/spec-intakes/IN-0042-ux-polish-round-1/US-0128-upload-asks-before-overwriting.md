# Work: Upload asks before overwriting an existing remote file

ID: US-0128
Intake: IN-0042
Created: 2026-09-18

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change
- Risk lane: normal
- Risks: this is the **data-loss path** `evidence/before-after-report.md` §4.5 named
  ("A live data-loss path left open") — an upload silently truncates a remote file
  the user never agreed to lose. The change adds a guard rather than removing one,
  but it sits directly on that path, so the failure mode to watch is a guard that
  is *skipped* (a new upload entry point that bypasses `do_upload_paths`) or one
  that lets the transfer start before the answer arrives.
- Spec Intake, when required: `IN-0042` (UX polish round 1)

## Outcome

An upload never replaces a file that is already in the remote directory without the
user saying so. The confirmation is the **same decision** the download direction
already makes before it replaces a local file — one helper, one dialog style
(danger-styled Replace, neutral keep-what-is-there button, the file named in the
description) — extended to the other direction. Nothing is transferred until the
question is answered, and when several files of one batch collide the user answers
once for the whole batch with Replace all / Skip.

## Scope

- [ ] In scope:
  - `crates/sftp-ui/src/transfer.rs` — the shared `confirm_replace` dialog helper
    (extracted from `download_entry_to_local`, used by both directions), the
    collision decision (`colliding_names` / `keep_after_answer`), and
    `do_upload_paths` asking before it starts a batch.
  - Every GUI upload path, which all funnel through `do_upload_paths`: the Local
    pane's **Upload** button and its menu/double-click (`local_pane.rs`), the
    remote pane's Upload Files / Upload Folder action and context menu
    (`render.rs`, `table_delegate_menu.rs`, `do_upload`), and drag & drop of
    external paths or of a Local pane row onto the remote list (`render.rs`).
  - `docs/sftp-browser-design.md` §4.6 / §4.15 and the before/after report §4.5.
- [ ] Out of scope:
  - **The editor's upload-on-save path** (`crates/sftp-ui/src/edit.rs`,
    `upload_edit_now`). It writes the user's own edits back to the very file they
    opened, and it already refuses to clobber a *foreign* change: `warn_conflict_then_upload`
    compares the remote mtime against the baseline taken at open and asks before
    overwriting when it moved. Asking "replace?" on every save of a file the user
    is editing would be noise, not a guard. Its dialog keeps its own wording
    because the question it asks ("the file changed under you") is a different
    question. Recorded as a gap below.
  - Per-file answers inside one batch (the batch answers once), a "skip all but
    this one" choice, and rename-on-collision.
  - The backend contract: `SftpBackend` is unchanged; the check uses the existing
    `read_dir`.

## Acceptance

- [x] Uploading a file whose name already exists in the remote directory opens a
      confirmation dialog naming the file, and requests **nothing** from the
      backend until it is answered.
- [x] Cancel/Skip leaves the colliding file untouched on the server — no transfer
      request, no queue item for it.
- [x] Replace starts the upload, exactly as before the packet.
- [x] A batch in which several files collide asks **once**, offering **Replace all**
      and **Skip**; Skip still uploads the files of the batch that collide with
      nothing.
- [x] A batch in which nothing collides is unchanged: no dialog, upload starts
      immediately.
- [x] The dialog is the same shape as the download direction's: danger-styled
      Replace, neutral (outline) keep button, the name(s) in the description —
      because both directions now render it from one helper.
- [x] A remote directory that cannot be listed does not block the upload; it only
      means the collision cannot be known (logged at `warn`).

## Documentation

### Owning Docs Reviewed

- `docs/sftp-browser-design.md` §4.6 (File operations — UI flow: the Upload and
  Download rows), §4.7 (Transfer queue), §4.15 (Dual-pane mode — states that
  `download_to` confirms "before an existing file is replaced" and that transfers
  between panes reuse `do_upload_paths`) — the owning contract for both directions.
- `docs/spec-intakes/IN-0042-ux-polish-round-1/US-0124-sftp-table-fits-and-dual-pane-has-transfer-controls.md`
  — its Gaps entry is the source of this packet: "uploading onto an existing remote
  file still overwrites without asking".
- `docs/spec-intakes/IN-0042-ux-polish-round-1/evidence/before-after-report.md` §4.5
  — "A live data-loss path left open".
- `docs/spec-intakes/IN-0025-sftp-dual-pane/` — the LLD that introduced the
  download-side confirmation this packet generalizes.
- `docs/agents/error-policy.md` — user-action failures stop the operation and
  notify; a best-effort step logs with its operation name and continues.

### Documentation Action

- Update required:
  - `docs/sftp-browser-design.md` §4.6 (Upload row) and §4.15 (the sentence on
    transfers between panes) must say that **both** directions confirm before an
    existing target is replaced, and that the confirmation is one shared helper.
  - `.../evidence/before-after-report.md` §4.5 gets the "Closed 2026-09-18 by
    `US-0128`" treatment §4.3 already uses.
  - `.../IN-0042.md` gains the `US-0128` row in Candidate Work Packets.

Reason: the design doc currently documents the guard on the download side only,
which is exactly the asymmetry the report called out; after this packet the
contract is symmetric and the doc must say so.

### Reconciliation

Docs changed:

- `docs/sftp-browser-design.md` — §4.6 Upload and Download rows, the §4.15
  dual-pane paragraph, and a new §4.16 that owns the shared confirmation.
- `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md` — `US-0128` added to
  Candidate Work Packets.
- `docs/spec-intakes/IN-0042-ux-polish-round-1/evidence/before-after-report.md` —
  §4.5 marked closed.
- `docs/spec-intakes/IN-0042-ux-polish-round-1/US-0124-...md` — its Gaps entry
  points at this packet.

## Context

`crates/sftp-ui/src/transfer.rs` already held the whole answer on the download
side: `download_entry_to_local` probes the target with `PathBuf::exists()` on the
background executor, and when it exists it builds a `start` closure and opens
`window.open_alert_dialog(...).confirm()` with a custom `DialogFooter` — an
outline **Cancel** that only closes, and a `.danger()` **Replace** that closes and
then runs `start`. Nothing is transferred until Replace is clicked.

The upload side had no such step: `do_upload_paths` went straight from the batch to
`sftp.upload(...)` per file. Every GUI upload path funnels through it, so one guard
there covers the button, the menu, the actions and both drop targets — the root
cause, not four symptom patches.

The remote-side "does it exist?" probe is one `read_dir` of the remote cwd rather
than one `stat` per file: a single round trip, the same call the panel already uses
to list that directory, and it answers for a whole batch at once.

## Plan

- [x] Extract the download dialog into `confirm_replace(...)` in `transfer.rs`,
      parameterized by title, description and the two button labels, with a
      single `answer(bool, &mut Window, &mut App)` callback that fires once.
- [x] Add the two pure decision functions and their unit tests.
- [x] Give `do_upload_paths` a `&mut Window`, list the remote cwd, and gate the
      batch on the answer; update the four call sites.
- [x] Point `download_entry_to_local` at the shared helper.
- [x] Panel tests for: no collision → no dialog; one collision → dialog, nothing
      requested; Cancel → nothing requested; Replace → upload requested; multi
      collision → one dialog, Skip uploads only the non-colliding file.
- [x] Reconcile the docs and capture the GUI frames.

## Decisions

No new decision record. The behavior is the existing IN-0025 confirmation contract
applied to the other direction; the shape of the dialog is already settled there.

## Verification Plan

- `cargo test -p oneterm-sftp-ui` — the decision unit tests plus the panel tests
  above.
- `pwsh scripts/ci-local.ps1` — the full gate.
- A GUI walk against `oneterm-tools sftp-dev-server` on loopback, captured as
  frames: the dialog on an existing file, the remote listing unchanged after
  Cancel (same mtime/size), the overwrite after Replace, and the Replace all /
  Skip dialog for a multi-file collision.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

`cargo test -p oneterm-sftp-ui` — 67 passed, 0 failed (8 of them new: four on the
decision itself, four on the panel wiring). `pwsh scripts/ci-local.ps1` —
`ci-local: all checks passed.` The GUI walk against a loopback `sftp-dev-server`
is written up with its seven frames in `evidence/US-0128-verification.md`: the
dialog on an existing remote file, the remote listing unchanged after Cancel
(same 3 B / `09:38`), the overwrite after Replace (384 B / `09:48`), the batch's
single Replace all / Skip question, Skip uploading only the file that collided
with nothing, and the download direction still showing the same dialog.

Gaps:

- **The editor's save path is deliberately untouched** (see Scope). It guards the
  foreign-change case with its own mtime conflict dialog and does not use
  `confirm_replace`, so the "one dialog style" rule holds for the two transfer
  directions but not for that third, differently-worded question.
- **An unlistable remote directory fails open.** If `read_dir` of the remote cwd
  errors, the upload proceeds without a prompt (logged at `warn`), exactly as it
  behaved before this packet. Failing closed would make an upload impossible into a
  write-only directory, which the server itself would accept.
- **Directories are confirmed like files.** A folder upload onto an existing remote
  name asks the same question, and Replace means "merge, overwriting what
  collides inside" rather than "delete then write" — the same meaning the download
  direction's confirmation has. No per-file prompting inside a recursive upload.
- **Windows-only run.** The frames come from a Windows host against a loopback
  dev server; no Linux/macOS pass (Platform proof unchecked).
- Two names that are equal after the batch is picked (the same file name from two
  different local folders, only reachable by drag & drop) are counted as one
  collision entry in the dialog text.

## Handoff

None — the packet is complete in one session.
