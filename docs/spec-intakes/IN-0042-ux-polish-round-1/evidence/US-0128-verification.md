# US-0128 — verification

Date: 2026-09-18
Branch: `feat/sftp-upload-confirm-overwrite`
Host: Windows 11, `fast-dev` profile, loopback `sftp-dev-server` on `127.0.0.1:2277`

## Commands

| Command | Result |
| --- | --- |
| `cargo test -p oneterm-sftp-ui` | `test result: ok. 67 passed; 0 failed` |
| `cargo clippy -p oneterm-sftp-ui --all-targets -- -D warnings` | clean |
| `pwsh scripts/ci-local.ps1` | see the last line recorded below |

### The decision, unit-tested

Four tests cover the decision itself, independent of any dialog:

| Test | What it pins |
| --- | --- |
| `collisions_are_the_batch_names_the_remote_directory_already_holds` | which uploads collide, in batch order, and that an empty directory or an empty batch collides with nothing |
| `a_nameless_path_collides_under_its_fallback_name` | a path with no file name is matched under the same `"uploaded"` name the upload would use |
| `the_answer_decides_what_is_left_of_the_batch` | Replace keeps the whole batch; Skip drops exactly the colliding files and keeps the rest in order; an all-colliding batch uploads nothing when skipped |
| `the_prompt_names_the_files_and_labels_the_answers` | the wording and the four label combinations (`Replace`/`Cancel`, `Replace`/`Skip`, `Replace all`/`Cancel`, and the five-name cap with "and N more") |

Four panel tests cover the wiring:

| Test | What it pins |
| --- | --- |
| `upload_without_a_collision_asks_nothing` | no dialog, the transfer is requested straight away |
| `upload_onto_an_existing_remote_file_asks_first` | the dialog opens and the backend is asked for **nothing** — no transfer request, no queue item |
| `a_batch_with_several_collisions_asks_once` | one listing and one question for the batch, not one per file |
| `an_unlistable_remote_directory_does_not_block_the_upload` | a failed `read_dir` leaves the upload running as before |

## GUI walk

Fixture — remote root served by `sftp-dev-server`, local folder handed to the app as its home:

| Side | Files |
| --- | --- |
| remote | `keep.txt` 3 B, `other.txt` 9 B, `readme.md` 16 B — all `09:38` |
| local | `fresh.txt` 110 B, `keep.txt` 384 B, `other.txt` 440 B |

| Frame | What it shows |
| --- | --- |
| `US-0128-01-before-both-panes.png` | The starting state: both panes, the three remote files at 3 B / 9 B / 16 B, `09:38`. |
| `US-0128-02-upload-asks-before-overwriting.png` | Local `keep.txt` selected, **Upload** clicked: **Replace Remote File** — `"keep.txt" already exists in the remote folder. Replace it?` — neutral **Cancel**, danger **Replace**. Nothing has been transferred. |
| `US-0128-03-cancel-transferred-nothing.png` | **Cancel**, then the remote list re-read from the server: `keep.txt` is still 3 B at `09:38`. The transfer queue never appeared. On disk: `keep.txt 3 9/18/2026 9:38:57 AM`. |
| `US-0128-04-replace-overwrote.png` | The same upload answered with **Replace**: remote `keep.txt` is 384 B at `09:48`, queue row `↑ keep.txt … Done`. On disk: `keep.txt 384 9/18/2026 9:48:21 AM`. |
| `US-0128-05-batch-offers-replace-all-or-skip.png` | `Upload Files` with `keep.txt`, `other.txt`, `fresh.txt` in one batch: one question — **Replace Remote Files**, `2 files already exist in the remote folder: "keep.txt", "other.txt". Replace them?` — neutral **Skip**, danger **Replace all**. |
| `US-0128-06-skip-uploaded-only-the-new-file.png` | **Skip**: only `fresh.txt` went up (110 B at `09:50`); `other.txt` is untouched at 9 B `09:38` and `keep.txt` still carries the 09:48 replace. The queue holds two rows for the walk's two accepted transfers — the skipped files never entered it. |
| `US-0128-07-download-direction-same-dialog.png` | Regression: downloading remote `keep.txt` onto the local copy still asks, in the same shape — **Replace Local File**, neutral **Cancel**, danger **Replace**. Both directions render it from `confirm_replace`. |

Remote directory after the walk (on the server's disk):

```text
fresh.txt    110 9/18/2026 9:50:33 AM
keep.txt     384 9/18/2026 9:48:21 AM
other.txt      9 9/18/2026 9:38:57 AM
readme.md     16 9/18/2026 9:38:57 AM
```

## Gaps

- The batch frame is driven through the OS file picker, which is the only upload
  path that can hand several files to one batch; drag & drop of several external
  files takes the same `do_upload_paths` call and is not separately framed.
- Windows only. The guard is platform-independent (no `cfg`), but no Linux/macOS
  run was made.
- `Skip` on a batch where **every** file collides reads `Cancel`, because nothing
  would be uploaded either way; that label variant is covered by the unit test,
  not by a frame.
