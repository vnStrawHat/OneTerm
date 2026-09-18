# US-0128 — independent verification

Date: 2026-09-18
Verifier: a second agent, no part in the implementation.
Under test: `1e1dbf0a` (branch `feat/sftp-upload-confirm-overwrite`, 3 commits on
`main` @ `b64b70bc`), checked out in an isolated worktree as `verify/US-0128`.
Host: Windows 11, `fast-dev` profile, loopback `sftp-dev-server` on `127.0.0.1:2288`.

## Verdict

**PASS with findings.** Every claim the packet makes about the *implemented*
behaviour holds: one shared `confirm_replace`, one funnel (`do_upload_paths`) that
every GUI upload entry point reaches, a batch that asks once, a `Skip` that still
uploads the rest, a fail-open on an unlistable directory, and a download direction
that behaves exactly as before. The data-loss path the packet set out to close is
closed for the ordinary case.

It is **not** closed on a case-insensitive remote filesystem, and two statements in
the packet are wrong about their own code (the dismiss path, and the duplicate-name
gap). None of these is a regression against `main` — before this packet *no* upload
asked at all — so the verdict is PASS, with F1 recommended as a follow-up `BUG`
against `IN-0042` rather than acceptance rework.

## Claims, checked

| # | Claim | Verdict | Evidence |
|---|---|---|---|
| C1 | Shared `confirm_replace(...)` in `transfer.rs`, used by both directions; danger `Replace`, outline keep button | **Holds** | `crates/sftp-ui/src/transfer.rs:51` defines it; upload calls it at `:344`, download at `:672`. `.danger()` on the replace button (`:84-88`), `.outline()` on the keep button (`:77`). |
| C2 | `Rc` + `Cell` once-guard, *including dismiss* | **Partly** — guard exists, dismiss never reaches it | `transfer.rs:62-67`. See **F4**: Escape dispatches `Cancel` → `on_cancel(\|_,_,_\| true)` (`transfer.rs:93-96`) → `reference/gpui-kit/crates/base/src/dialog.rs:510-523` closes the dialog *without* calling `answer`. Backdrop dismissal is disabled by design (`reference/gpui-kit/crates/component/src/dialog/alert_dialog.rs:218-219`). Behaviour is the safe one (nothing is transferred); the doc comment at `transfer.rs:60-61` is wrong. |
| C3 | Every GUI upload entry funnels through `do_upload_paths` | **Holds** | Enumerated below. `sftp.upload(...)` is called from exactly two places in `crates/sftp-ui`: `transfer.rs:398` (inside the gated `upload_batch`) and `edit.rs:744` (`upload_edit_now`, out of scope by the packet). |
| C4 | `do_upload_paths` lists the remote cwd **once** and gates the batch | **Holds** | `transfer.rs:320` one `read_dir`; pinned by `a_batch_with_several_collisions_asks_once` asserting `read_dir_requests() == vec![cwd]` (`transfer.rs:1252`, inside the test at `:1224`). |
| C5 | Pure `colliding_names` / `keep_after_answer` / `collision_prompt` with 4 unit tests + 4 panel tests | **Holds** (`transfer.rs:113`, `:123`, `:143`; tests `:1091`–`:1274`) | `cargo test -p oneterm-sftp-ui` → **67 passed**. But see **F5**: no test drives an *answer*. |
| C6 | One collision → `Replace`/`Cancel`; several → `Replace all`/`Skip`; `Skip` reads `Cancel` when nothing would remain | **Holds** | `collision_prompt` `transfer.rs:143-180`; pinned by `the_prompt_names_the_files_and_labels_the_answers`. Confirmed in the walk (frames 02 and 08). |
| C7 | An unlistable cwd fails OPEN, logged at `warn` | **Holds** | `transfer.rs:322-330`; pinned by `an_unlistable_remote_directory_does_not_block_the_upload`. See **F6** on whether that is the right side of the trade. |
| C8 | Directories are confirmed as one entry, meaning merge-overwrite | **Holds** | `sftp_upload_dir` (`crates/ssh/src/sftp_task/transfer/upload.rs:202-328`) creates each remote directory, tolerates one that already exists, and overwrites only the files it carries. Walk frame 11 shows `onlyremote.txt` surviving a `Replace`d folder upload. Wording caveat in **F7**. |
| C9 | The editor save path (`edit.rs::upload_edit_now`) is excluded and has its own mtime-conflict dialog | **Holds** | `edit.rs:685-718` (`warn_conflict_then_upload`). The "always upload this file" checkbox (`edit.rs:577`, the checkbox at `:600`, set at `:613`) skips only the *"was saved, upload it?"* prompt — it still routes through `begin_conflict_check_and_upload` → `stat` → conflict dialog, so it is **not** a bypass of the foreign-change guard. |
| C10 | The download direction still behaves exactly as before | **Holds** | Same title, same labels, same `danger`/`outline`, and `move \|replace, ..\| if replace { start(...) }` (`transfer.rs:675-680`) is behaviourally identical to the old "only the Replace button calls `start`". Only the button element id changed (`"cancel"` → `"keep"`). Walk frame 13. |

### C3 — the enumeration

Every call in `crates/sftp-ui` that ends in a remote *write*:

| Call site | Reaches | Gated? |
|---|---|---|
| `local_pane.rs:400`, `:407`, `:971`, `:1048` — Local pane Upload buttons and menu items | `upload_selected` (`local_pane.rs:751`) → `do_upload_paths` (`local_pane.rs:764`) | yes |
| `local_pane.rs:705` — Local pane row double-click | same | yes |
| `render.rs:89`/`:90` — `SftpUploadFiles` / `SftpUploadFolder` actions | `do_upload` (`transfer.rs:417`) → `do_upload_paths` (`transfer.rs:476`) | yes |
| `table_delegate_menu.rs:166`/`:167` — remote context menu Upload Files / Upload Folder | same | yes |
| `render.rs:437-440` — a Local pane row dropped on the remote list | `do_upload_paths` (`render.rs:439`) | yes |
| `render.rs:441-455` — external paths dropped on the remote list | `do_upload_paths` (`render.rs:449`) | yes |
| `edit.rs:744` — `upload_edit_now` | `sftp.upload` directly | **no** — out of scope, own mtime dialog (C9) |
| `actions.rs:173` `rename`, `:258` `remove`, `:335` `mkdir` | not content writes; each has its own dialog | n/a |

There is no paste path, no second drop target (`can_drop`/`on_drop` appear only at
`render.rs:436` — remote list, upload — and `local_pane.rs:1112` — local list,
download), and no retry in the transfer queue (`retry` matches nothing in
`crates/sftp-ui/src`). The remote drop target uploads into the *cwd*, never into a
hovered directory row, so there is no "drop onto a folder" variant to gate.

## Findings, ranked

### F1 — MAJOR: the collision check is case-sensitive; a case-insensitive server still overwrites silently

`colliding_names` compares byte-exactly (`transfer.rs:117`, `existing.contains(name)`).
A Windows or default-macOS SFTP server resolves `case.txt` onto an existing
`Case.txt`. The check finds no collision, no dialog opens, and the server truncates
the other file. This is the exact data-loss path §4.5 of the before/after report
named, still open on those hosts.

Reproduced in the walk (frames 06/07): the remote directory held `Case.txt`
(20 B, `"REMOTE-CASE-ORIGINAL"`); uploading local `case.txt` (90 B) produced **no
dialog**, the transfer started immediately, and the directory afterwards held one
90 B `case.txt` carrying the local content. The 20 B original is gone. (The staged
`.part` file is renamed onto the *uploaded* name, so on this host the entry is also
renamed; on a case-preserving server the name would survive and only the content
would be destroyed. Either way the user was never asked.)

The behaviour is also completely unpinned: replacing the comparison with
`e.eq_ignore_ascii_case(name)` leaves **all 67 tests passing** (mutation M2 below).
Whichever way the project decides, a test should say so.

Note a real design tension: the *client* cannot know the server's case-folding, so
the honest fix is to match case-insensitively (ask more often — the cost is one
extra dialog on a case-sensitive server when `Keep.txt` and `keep.txt` genuinely
coexist, which the batch wording already handles) or to compare both ways and
word the question as "a file differing only in case already exists". Recommend a
follow-up `BUG` under `IN-0042`.

### F2 — MEDIUM: the listing is a snapshot; a file that appears between the check and the write is overwritten silently (TOCTOU)

`read_dir` runs at `transfer.rs:320`; the `put` runs after the human answers, so
the window is not milliseconds but however long the dialog is on screen — and on
the no-collision path the batch starts immediately but still races anything created
in the meantime. `finalize_remote_file` (`crates/ssh/src/sftp_task/transfer/staging.rs:55-100`)
renames the staged `.part` over whatever is there, so the late arrival is lost. It
does refuse to replace a remote *directory* or *symlink*, which bounds the damage.

For a single-user GUI this is an acceptable trade, but it is not stated anywhere.
The packet's Gaps list should say the guard is advisory, not atomic. (Closing it
would need `SSH_FXF_EXCL` on the staged-file finalize, which the backend does not
expose — out of scope for this packet.)

### F3 — MEDIUM: duplicate names in one batch are counted and listed twice, and the packet's Gap says the opposite

`colliding_names` maps one entry per *path* and never de-duplicates
(`transfer.rs:113-118`). A batch of `a\x.txt` and `b\x.txt` against a remote
`x.txt` produces `colliding = ["x.txt", "x.txt"]`, so the dialog reads

> `2 files already exist in the remote folder: "x.txt", "x.txt". Replace them?`

The packet's last Gap states these "are counted as one collision entry in the
dialog text" — the code does the opposite. `keep_after_answer` still behaves
correctly (both paths are dropped on Skip), so this is wording plus a wrong gap,
not a data-loss bug.

Reproduced in the walk (frame 14) with `home\keep.txt` + `home\dup\keep.txt` +
`home\fresh.txt`, all three colliding: the dialog read `3 files already exist in
the remote folder: "keep.txt", "keep.txt", "fresh.txt". Replace them?` — one
name listed twice and a count of 3 where two distinct names collide. The
`Skip`/`Cancel` label rule (`colliding.len() < batch_len`, `transfer.rs:147`) is
*not* affected: the count can never exceed the batch length, so it still reads
`Cancel` exactly when nothing would be left to upload.

Related and not mentioned anywhere: two batch entries with the same name that
collide with *nothing* remotely upload one after the other, the second silently
overwriting the first, with no prompt. Only reachable by dropping files from two
different local folders.

### F4 — MEDIUM (documentation): the once-guard does not cover dismiss, and the comment says it does

`transfer.rs:60-61`:

> `// The dialog builder is a Fn and both buttons plus the dismiss path can`
> `// reach answer; the flag keeps the decision to the first one that does.`

There is no dismiss handler wired to `answer`. Escape dispatches the `Cancel`
action, which runs `on_cancel(|_, _, _| true)` (`transfer.rs:93-96`), closes the
dialog and drops the `Rc` — `answer` is never called. Clicking outside cannot
dismiss at all (`AlertDialog` sets `close_on_backdrop_press(false)`,
`reference/gpui-kit/crates/base/src/alert_dialog.rs:194`). `Enter` dispatches
`Confirm` → `on_ok(|_, _, _| false)` → the dialog stays open and nothing is
transferred, which is the right default for a destructive question.

So the outcome is safe on every dismiss route (verified in the walk, frame 03: the
remote file is byte-identical after Escape), but the guard is defending against a
path that does not exist. Once the dialog is closed by either button, a second
click cannot land either. The comment should be corrected; the `Cell` can stay as
cheap insurance.

### F5 — MEDIUM (coverage): no automated test answers the dialog

The four panel tests assert only whether a dialog appears and that no *transfer*
was requested (`transfer.rs:1172`, `:1200`, `:1224`, `:1258`). None clicks
`Replace`, `Skip` or `Cancel`. That leaves two Acceptance rows —
"Replace starts the upload" and "Skip still uploads the files that collide with
nothing" — and the packet's own Plan bullet ("multi collision → one dialog, Skip
uploads only the non-colliding file") resting entirely on the manual walk.

Mutation testing bears this out: breaking `keep_after_answer` so that Skip keeps
the whole batch is caught by exactly one test (the pure one), and by no panel test.

### F6 — LOW/MEDIUM: fail-open on an unlistable cwd is silent

`transfer.rs:322-330` logs at `warn` and uploads. Measured against
`docs/agents/error-policy.md`, this fits the "optional telemetry/UI refresh" row
(`warn`, operation named, continue) rather than the "user input or action" row
(stop and notify) — and the packet's justification is sound: failing closed would
make it impossible to upload into a directory the user may write but not read, which
the server itself would accept. It is also not a regression: before this packet the
guard did not exist at all.

But it is the one case where the user has no way to see the directory contents
themselves, and they are told nothing. The honest middle — the one the brief
proposes — is to still ask: *"The remote folder could not be listed, so existing
files cannot be checked. Upload anyway?"*. That keeps the write-only directory
working and never silently overwrites. Recommend it as a follow-up, not a blocker;
the packet does record the current behaviour as a Gap.

### F7 — LOW: a folder collision is announced as a "File", and "Replace" overstates what happens

`collision_prompt` has one wording for everything (`transfer.rs:151-157`), so
uploading a folder onto an existing remote folder reads **Replace Remote File** /
`"adir" already exists in the remote folder. Replace it?`. What actually happens
(C8, walk frame 11) is a merge: colliding files inside are overwritten with no
further prompt, non-colliding remote files survive. "Replace" suggests the folder
is wiped first. The packet records the semantics correctly in its Gaps, but the
dialog does not say them.

### F8 — NIT: "requests nothing from the backend until it is answered" is not literally true

The first Acceptance row says the dialog "requests **nothing** from the backend
until it is answered". One `read_dir` is issued before the dialog opens
(`transfer.rs:320`); the test that backs the row asserts only that no *transfer*
was requested. The intent (no **write** before the answer) holds. Also worth
noting: every upload now costs one extra round trip even when nothing collides.

## Recommended follow-ups

| Finding | Suggested record |
|---|---|
| **F1** case-insensitive collision | a new `BUG` under `IN-0042` — the data-loss path is still open on Windows/macOS servers, and no test pins either behaviour |
| **F3** duplicate names counted twice | fold into the same `BUG`, or a one-line `dedup` plus a corrected Gap line in `US-0128` |
| **F4** wrong comment about the dismiss path | a comment fix in `transfer.rs:60-61`; no behaviour change |
| **F5** no test answers the dialog | add one panel test that drives `Replace` and one that drives `Skip` |
| **F6** silent fail-open | a question ("could not check — upload anyway?") instead of silence; ask the owner first, it is a UX call |
| **F7** folder announced as a "File" | wording only; `§4.16` of `docs/sftp-browser-design.md` would gain the folder sentence |

None of them blocks acceptance of `US-0128`: the packet delivers the outcome it
states, and every one of these is either a narrower case it never claimed, or text.

## Commands

| Command | Result |
|---|---|
| `git reset` to `1e1dbf0a` in an isolated worktree, branch `verify/US-0128` | `main` was left untouched (the feature branch is checked out in another worktree, so a new branch was created at the same commit) |
| `cargo test -p oneterm-sftp-ui` | `test result: ok. 67 passed; 0 failed` — matches the packet |
| Mutation **M1** — `keep_after_answer` returns `local_paths` unconditionally (Skip keeps everything) | **caught**: `the_answer_decides_what_is_left_of_the_batch` FAILED at `transfer.rs:1119`; `66 passed; 1 failed`. Restored. |
| Mutation **M2** — `colliding_names` compares with `eq_ignore_ascii_case` | **NOT caught**: `67 passed; 0 failed`. The case behaviour is unpinned in both directions (see F1). Restored. |
| `pwsh scripts/ci-local.ps1` (after deleting `target/fast-dev`, `CARGO_BUILD_JOBS=4`) | `ci-local: all checks passed.` — every step green, including `check-english`, `check-doc-paths` (202 paths, 11 documents) and `third-party-notices --check` |

## GUI walk

Loopback `sftp-dev-server` on `127.0.0.1:2288`, own port, own scratch `$HOME`,
own pid. Fixture:

| Side | Files |
|---|---|
| remote | `keep.txt` 3 B, `other.txt` 8 B, `readme.md` 14 B, `Case.txt` 20 B, `adir/` (`inner.txt` 1 B, `onlyremote.txt` 7 B) |
| local | `keep.txt` 330 B, `other.txt` 360 B, `fresh.txt` 120 B, `case.txt` 90 B, `adir/` (`inner.txt` 24 B, `onlylocal.txt` 10 B) |

| Frame | What it shows |
|---|---|
| `US-0128-verify-01-before.png` | Connected, dual pane, the starting remote listing (`adir`, `Case.txt` 20 B, `keep.txt` 3 B, `other.txt` 8 B, `readme.md` 14 B), all `10:15`. |
| `US-0128-verify-02-single-collision.png` | Local `keep.txt` selected, **Upload**: **Replace Remote File** — `"keep.txt" already exists in the remote folder. Replace it?` — outline **Cancel**, danger **Replace**. No transfer row. |
| `US-0128-verify-03-escape-dismiss.png` | **Escape** on that dialog: it closes, no queue row appears, and on the server's disk `keep.txt` is still `3 B / 10:15:48`. Dismiss is safe (**F4**). |
| `US-0128-verify-04-cancel.png` | The same upload answered with **Cancel**: server disk unchanged (`keep.txt 3 10:15:48`). |
| `US-0128-verify-05-replace.png` | Answered with **Replace**: `keep.txt` is `330 B / 10:23:33` on the server, one `↑ keep.txt … Done` row. |
| `US-0128-verify-06-case-no-dialog.png` | **F1 live.** Local `case.txt` (90 B) uploaded while the remote directory held `Case.txt` (20 B, `"REMOTE-CASE-ORIGINAL"`): **no dialog at all**, the transfer starts immediately. |
| `US-0128-verify-07-case-overwrote.png` | The remote listing after it: `Case.txt` is gone, one 90 B `case.txt` in its place whose content is the local upload. The 20 B original was destroyed without a question. |
| `US-0128-verify-08-batch-replace-all-skip.png` | `Upload Files` with `keep.txt`, `other.txt`, `fresh.txt`: **one** question — **Replace Remote Files**, `2 files already exist in the remote folder: "keep.txt", "other.txt". Replace them?` — outline **Skip**, danger **Replace all**. Queue still shows only the two earlier transfers. |
| `US-0128-verify-09-skip-only-new.png` | **Skip**: only `fresh.txt` went up (`120 B / 10:27:11`). `other.txt` untouched at `8 B / 10:15:48`; the skipped files never entered the queue (3 rows total). |
| `US-0128-verify-10-folder-collision.png` | Local folder `adir` uploaded onto the existing remote `adir`: **Replace Remote File** / `"adir" already exists in the remote folder. Replace it?` — a folder announced as a "File" (**F7**). |
| `US-0128-verify-11-folder-merged.png` | **Replace** on that folder: `inner.txt` overwritten `1 B → 24 B`, `onlylocal.txt` added (10 B), and **`onlyremote.txt` survives at 7 B / 10:15:48**. Merge-overwrite, exactly as C8 claims, with no per-file prompt inside. |
| `US-0128-verify-12-drag-drop.png` | **The drag-and-drop path is gated.** A Local pane row (`other.txt`) dragged onto the remote list opens the same **Replace Remote File** dialog; **Cancel** left the server's `other.txt` at `8 B / 10:15:48`. |
| `US-0128-verify-13-download-same-dialog.png` | Regression: downloading remote `keep.txt` onto the existing local copy still asks — **Replace Local File**, outline **Cancel**, danger **Replace**. Same shape, both directions. |
| `US-0128-verify-14-duplicate-names.png` | **F3 live.** A batch of `home\keep.txt`, `home\dup\keep.txt`, `home\fresh.txt`: `3 files already exist in the remote folder: "keep.txt", "keep.txt", "fresh.txt". Replace them?` — the same name listed and counted twice, and the neutral button reads **Cancel** although only two distinct names collide. |

Server disk after the walk:

```text
adir\inner.txt       24  10:27:49   (was 1 B — overwritten by the confirmed folder upload)
adir\onlylocal.txt   10  10:27:49   (added)
adir\onlyremote.txt   7  10:15:48   (survived)
case.txt             90  10:23:48   (F1: was Case.txt, 20 B, no question asked)
fresh.txt           120  10:27:11   (Skip still uploaded the non-colliding file)
keep.txt            330  10:23:33   (confirmed Replace)
other.txt             8  10:15:48   (Cancel / Skip / cancelled drop — never written)
readme.md            14  10:15:48   (never in a batch)
```

Driver: posted `WM_*` messages plus `PrintWindow`, own pid only, own scratch `$HOME`,
own port 2288. `target/fast-dev` was deleted after the walk.

## Gaps in this verification

- **Drag & drop of *external* paths (from Explorer) was not driven.** That is an OLE
  drop, which posted messages cannot synthesize. The *internal* Local-row drop was
  driven and is gated (frame 12); the external drop reaches the identical
  `do_upload_paths` call one line away (`render.rs:449` vs `:439`), so it is covered
  by inspection, not by a frame.
- **Windows only.** No Linux/macOS run — same gap the packet records. The guard has
  no `cfg`, but **F1** is precisely a platform-dependent behaviour, so a run against
  a case-sensitive server would show the *other* half of it.
- `harness.db` was not touched; the packet's status blocks were left as the author
  set them. The findings above are recommendations for follow-up packets, not edits
  to `US-0128`.
- The worktree could not check out `feat/sftp-upload-confirm-overwrite` (it is
  checked out elsewhere), so the review ran on `verify/US-0128`, created at the same
  commit `1e1dbf0a`. `main` was not moved.
